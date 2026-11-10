// Wavely Native auf GitHub veröffentlichen.
//
//   node scripts/build-native.mjs
//   node scripts/release-native.mjs
//
// Lädt den signierten Installer hoch und committet updates/latest-native.json auf diesen Branch –
// die native App sucht dort nach Updates (native/src/updater.rs). Außerdem bekommt der Test-Kanal
// der alten Wavely (updates/latest-test.json auf main) diese Version: Admins der alten App
// aktualisieren so auf Wavely Native, der Installer ersetzt dabei die alte App.
//
// Version mit „-native“ (z. B. 1.1.0-native.1): Testversion als Pre-Release, normale Nutzer merken nichts.
// Version ohne Zusatz (z. B. 1.0.0): volles Release, wird „latest“ und bekommt eine latest.json –
// damit aktualisieren alle Nutzer der alten Wavely auf Wavely Native.
//
// --replace: Installer einer schon veröffentlichten Version austauschen (gleiche Versionsnummer)
// Das GitHub-Token kommt aus der Git-Anmeldung (Git Credential Manager), es wird nirgends gespeichert.
import { execFileSync } from "node:child_process";
import { readFileSync, existsSync, writeFileSync } from "node:fs";
import { basename, join } from "node:path";

const REPO = "ZyroMysty/Wavely";
const root = join(import.meta.dirname, "..");
const version = readFileSync(join(root, "native", "Cargo.toml"), "utf8").match(/^version\s*=\s*"([^"]+)"/m)[1];
const tag = `v${version}`;
const isTest = version.includes("-");
if (isTest && !version.includes("-native")) throw new Error(`„${version}“: Testversionen heißen z. B. 1.1.0-native.1.`);

const changelog = readFileSync(join(root, "CHANGELOG.md"), "utf8").replace(/\r/g, "");
const section = changelog.split(/^## /m).find((part) => part.startsWith(`${version} `) || part.startsWith(`${version}\n`));
const notes = section?.split("\n").slice(1).join("\n").trim();
if (!notes) throw new Error(`In CHANGELOG.md fehlt der Abschnitt „## ${version}“.`);

const installer = join(root, "native", "dist", `Wavely-Native_${version}_x64-setup.exe`);
const manifestFile = join(root, "updates", "latest-native.json");
for (const file of [installer, `${installer}.sig`, manifestFile]) {
  if (!existsSync(file)) throw new Error(`${file} fehlt – erst „node scripts/build-native.mjs“ ausführen.`);
}

// Nur veröffentlichen, was auch auf GitHub liegt
const git = (...args) => execFileSync("git", args, { cwd: root, encoding: "utf8" }).trim();
const dirty = git("status", "--porcelain").split("\n").filter((l) => l && !l.endsWith("updates/latest-native.json"));
if (dirty.length) throw new Error(`Es gibt nicht committete Änderungen:\n${dirty.join("\n")}`);
execFileSync("git", ["push", "origin", "HEAD"], { cwd: root, stdio: "inherit" });
const head = git("rev-parse", "HEAD");

const credentials = execFileSync("git", ["credential", "fill"], { input: "protocol=https\nhost=github.com\n\n", encoding: "utf8" });
const token = credentials.match(/^password=(.*)$/m)?.[1];
if (!token) throw new Error("Keine GitHub-Anmeldung gefunden.");

async function github(path, options = {}) {
  const response = await fetch(path.startsWith("http") ? path : `https://api.github.com/repos/${REPO}${path}`, {
    ...options,
    headers: {
      Authorization: `Bearer ${token}`,
      Accept: "application/vnd.github+json",
      "X-GitHub-Api-Version": "2022-11-28",
      ...options.headers,
    },
  });
  if (!response.ok) throw new Error(`GitHub ${response.status}: ${await response.text()}`);
  return response.status === 204 ? null : response.json();
}

const replace = process.argv.includes("--replace");
const existing = await fetch(`https://api.github.com/repos/${REPO}/releases/tags/${tag}`, { headers: { Authorization: `Bearer ${token}` } });
if (existing.ok && !replace) throw new Error(`Release ${tag} gibt es schon – Version in native/Cargo.toml erhöhen (oder --replace).`);

const body = isTest
  ? `Native Testversion ohne Browser-Technik. Admins der bisherigen Wavely bekommen sie als Test-Update; sie ersetzt dabei die alte App (Einstellungen, Anmeldungen und Playlists werden übernommen).\n\n${notes}`
  : `Wavely Native: komplett neu gebaut, ohne Browser-Technik. Die bisherige Wavely aktualisiert sich automatisch darauf und wird dabei ersetzt (Einstellungen, Anmeldungen und Playlists werden übernommen).\n\n${notes}`;
const name = basename(installer);
let release;
if (existing.ok) {
  release = await existing.json();
  console.log(`Tausche den Installer von ${tag} aus…`);
  await github(`/releases/${release.id}`, { method: "PATCH", body: JSON.stringify({ body }) });
  for (const asset of release.assets.filter((a) => a.name === name)) {
    await github(`/releases/assets/${asset.id}`, { method: "DELETE" });
  }
} else {
  console.log(`Erstelle ${isTest ? "Testversion" : "Release"} ${tag}…`);
  release = await github("/releases", {
    method: "POST",
    body: JSON.stringify({
      tag_name: tag,
      target_commitish: head,
      name: isTest ? `Wavely Native ${version} (Testversion)` : `Wavely Native ${version}`,
      body,
      prerelease: isTest,
      make_latest: isTest ? "false" : "true",
    }),
  });
}

const url = release.upload_url.replace(/\{.*\}$/, `?name=${encodeURIComponent(name)}`);
await github(url, { method: "POST", body: readFileSync(installer), headers: { "Content-Type": "application/octet-stream" } });
console.log(`  hochgeladen: ${name}`);

// Update-Datei für die native App (mit den Release Notes)
const manifest = JSON.parse(readFileSync(manifestFile, "utf8"));
manifest.notes = notes;
manifest.url = `https://github.com/${REPO}/releases/download/${tag}/${name}`;
writeFileSync(manifestFile, JSON.stringify(manifest, null, 2) + "\n");
execFileSync("git", ["add", "updates/latest-native.json"], { cwd: root });
if (git("status", "--porcelain", "updates/latest-native.json")) {
  execFileSync("git", ["commit", "-q", "-m", isTest ? `Native Testversion ${version}` : `Wavely Native ${version}`], { cwd: root });
  execFileSync("git", ["push", "-q", "origin", "HEAD"], { cwd: root, stdio: "inherit" });
}
console.log("  Update-Kanal aktualisiert: updates/latest-native.json");

// Test-Kanal der alten Wavely (Tauri-Format) direkt auf main ändern, ohne main auszuchecken
const latestTest = {
  version,
  notes,
  pub_date: new Date().toISOString(),
  platforms: { "windows-x86_64": { signature: manifest.signature, url: manifest.url } },
};
const testPath = "/contents/updates/latest-test.json";
const current = await github(`${testPath}?ref=main`);
await github(testPath, {
  method: "PUT",
  body: JSON.stringify({
    message: `Test-Kanal: Wavely Native ${version} für Admins`,
    content: Buffer.from(JSON.stringify(latestTest, null, 2) + "\n").toString("base64"),
    sha: current.sha,
    branch: "main",
  }),
});
console.log("  Test-Kanal der alten Wavely aktualisiert: updates/latest-test.json (main)");

// Volles Release: latest.json am Release – die alte Wavely sucht unter releases/latest/download/latest.json
if (!isTest) {
  const jsonUrl = release.upload_url.replace(/\{.*\}$/, "?name=latest.json");
  for (const asset of (release.assets ?? []).filter((a) => a.name === "latest.json")) {
    await github(`/releases/assets/${asset.id}`, { method: "DELETE" });
  }
  await github(jsonUrl, { method: "POST", body: JSON.stringify(latestTest, null, 2), headers: { "Content-Type": "application/json" } });
  console.log("  latest.json hochgeladen: alle Nutzer der alten Wavely bekommen das Update");
}
console.log(`Fertig: ${release.html_url}`);
