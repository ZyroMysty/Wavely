// Gebaute Version als GitHub-Release veröffentlichen, damit installierte Wavely-Apps sie als Update finden.
//
//   npm run build
//   npm run release
//
// Die Release Notes kommen aus dem Abschnitt „## <version>“ in CHANGELOG.md.
// Lädt den signierten Installer und latest.json (Version, Notizen, Signatur, Download-Adresse) hoch.
// Das GitHub-Token kommt aus der Git-Anmeldung (Git Credential Manager), es wird nirgends gespeichert.
//
// Testversion (nur für Admins):  Version wie „0.15.0-test.1“, dann  npm run release -- --test
// → GitHub-Pre-Release; statt latest.json wird updates/latest-test.json im Repository aktualisiert.
// Normale Nutzer bekommen davon nichts mit.
import { execFileSync } from "node:child_process";
import { readFileSync, existsSync, writeFileSync, mkdirSync } from "node:fs";
import { basename, join } from "node:path";

const REPO = "ZyroMysty/Wavely";
const root = join(import.meta.dirname, "..");
const { version } = JSON.parse(readFileSync(join(root, "src-tauri", "tauri.conf.json"), "utf8"));
const tag = `v${version}`;
const isTest = process.argv.includes("--test");
if (isTest && !version.includes("-")) throw new Error(`Testversionen brauchen eine Nummer wie „${version}-test.1“.`);
if (!isTest && version.includes("-")) throw new Error(`„${version}“ ist eine Testversion – mit „npm run release -- --test“ veröffentlichen.`);

// Abschnitt „## 1.2.3 – Datum“ bis zur nächsten Überschrift
const changelog = readFileSync(join(root, "CHANGELOG.md"), "utf8").replace(/\r/g, "");
const section = changelog.split(/^## /m).find((part) => part.startsWith(`${version} `) || part.startsWith(`${version}\n`));
const notes = section?.split("\n").slice(1).join("\n").trim();
if (!notes) throw new Error(`In CHANGELOG.md fehlt der Abschnitt „## ${version}“ mit den Release Notes.`);

const installer = join(root, "src-tauri", "target", "release", "bundle", "nsis", `Wavely_${version}_x64-setup.exe`);
const signatureFile = `${installer}.sig`;
for (const file of [installer, signatureFile]) {
  if (!existsSync(file)) throw new Error(`${file} fehlt – erst „npm run build“ ausführen (mit Update-Schlüssel).`);
}

// Nur veröffentlichen, was auch auf GitHub liegt
const head = execFileSync("git", ["rev-parse", "HEAD"], { cwd: root, encoding: "utf8" }).trim();
const dirty = execFileSync("git", ["status", "--porcelain"], { cwd: root, encoding: "utf8" }).trim();
if (dirty) throw new Error("Es gibt nicht committete Änderungen – erst committen und pushen.");
execFileSync("git", ["push", "origin", "HEAD"], { cwd: root, stdio: "inherit" });

const credentials = execFileSync("git", ["credential", "fill"], {
  input: "protocol=https\nhost=github.com\n\n",
  encoding: "utf8",
});
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

const existing = await fetch(`https://api.github.com/repos/${REPO}/releases/tags/${tag}`, {
  headers: { Authorization: `Bearer ${token}` },
});
if (existing.ok) throw new Error(`Release ${tag} gibt es schon – Version in tauri.conf.json erhöhen.`);

const installerName = basename(installer);
const latest = {
  version,
  notes,
  pub_date: new Date().toISOString(),
  platforms: {
    "windows-x86_64": {
      signature: readFileSync(signatureFile, "utf8").trim(),
      url: `https://github.com/${REPO}/releases/download/${tag}/${installerName}`,
    },
  },
};

console.log(`Erstelle ${isTest ? "Testversion" : "Release"} ${tag}…`);
const release = await github("/releases", {
  method: "POST",
  body: JSON.stringify({
    tag_name: tag,
    target_commitish: head,
    name: isTest ? `Wavely ${version} (Testversion)` : `Wavely ${version}`,
    body: isTest ? `Testversion – nur für Admins gedacht.\n\n${notes}` : notes,
    prerelease: isTest,
  }),
});

async function upload(name, data, type) {
  const url = release.upload_url.replace(/\{.*\}$/, `?name=${encodeURIComponent(name)}`);
  await github(url, { method: "POST", body: data, headers: { "Content-Type": type } });
  console.log(`  hochgeladen: ${name}`);
}

await upload(installerName, readFileSync(installer), "application/octet-stream");
if (isTest) {
  // Test-Kanal: Datei im Repository, die nur Admin-Apps abfragen (siehe src-tauri/src/channels.rs)
  mkdirSync(join(root, "updates"), { recursive: true });
  writeFileSync(join(root, "updates", "latest-test.json"), JSON.stringify(latest, null, 2) + "\n");
  execFileSync("git", ["add", "updates/latest-test.json"], { cwd: root });
  execFileSync("git", ["commit", "-q", "-m", `Testversion ${version} für Admins`], { cwd: root });
  execFileSync("git", ["push", "-q", "origin", "HEAD"], { cwd: root, stdio: "inherit" });
  console.log("  Test-Kanal aktualisiert: updates/latest-test.json");
} else {
  await upload("latest.json", JSON.stringify(latest, null, 2), "application/json");
}
console.log(`Fertig: ${release.html_url}`);
