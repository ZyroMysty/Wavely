// Baut die native Wavely-Testversion: Release-Build, Installer (NSIS), Signatur und Update-Datei.
// Aufruf: node scripts/build-native.mjs
// Ergebnis in native/dist/: Wavely-Native_<Version>_x64-setup.exe (+ .sig) und updates/latest-native.json

import { execFileSync } from "node:child_process";
import { copyFileSync, existsSync, mkdirSync, readFileSync, writeFileSync } from "node:fs";
import { homedir } from "node:os";
import { join } from "node:path";

const root = join(import.meta.dirname, "..");
const native = join(root, "native");
const version = readFileSync(join(native, "Cargo.toml"), "utf8").match(/^version = "([^"]+)"/m)[1];
const dist = join(native, "dist");
const stage = join(dist, "stage");
mkdirSync(stage, { recursive: true });

console.log(`Baue Wavely Native ${version} …`);
execFileSync("cargo", ["build", "--release"], { cwd: native, stdio: "inherit" });
copyFileSync(join(native, "target", "release", "wavely-native.exe"), join(stage, "wavely-native.exe"));

// yt-dlp mitliefern (wie die normale Wavely)
const ytdlp = [join(root, "src-tauri", "binaries", "yt-dlp-x86_64-pc-windows-msvc.exe"), join(process.env.APPDATA, "com.wavely.app", "bin", "yt-dlp.exe")].find(existsSync);
if (!ytdlp) throw new Error("yt-dlp.exe nicht gefunden (npm run setup:ytdlp)");
copyFileSync(ytdlp, join(stage, "yt-dlp.exe"));

const setup = join(dist, `Wavely-Native_${version}_x64-setup.exe`);
const makensis = join(process.env.LOCALAPPDATA, "tauri", "NSIS", "makensis.exe");
execFileSync(makensis, [`/DVERSION=${version}`, `/DSOURCE=${stage}`, `/DOUTFILE=${setup}`, join(native, "installer", "wavely-native.nsi")], {
  stdio: "inherit",
});

// Signieren mit dem Wavely-Schlüssel (gleich wie bei der normalen Wavely, ohne Passwort)
const key = join(homedir(), ".tauri", "wavely.key");
// Tauri-CLI aus diesem Ordner oder der normalen Wavely daneben
const tauri = [join(root, "node_modules", ".bin", "tauri.cmd"), join(root, "..", "wavely", "node_modules", ".bin", "tauri.cmd")].find(existsSync);
if (!tauri) throw new Error("Tauri-CLI nicht gefunden (npm install)");
execFileSync(tauri, ["signer", "sign", `"${setup}"`], {
  stdio: "inherit",
  shell: true,
  env: { ...process.env, TAURI_SIGNING_PRIVATE_KEY: readFileSync(key, "utf8").trim(), TAURI_SIGNING_PRIVATE_KEY_PASSWORD: "" },
});
const signature = readFileSync(`${setup}.sig`, "utf8").trim();

const notes = process.argv[2] ?? "Native Testversion";
const manifest = {
  version,
  notes,
  url: `https://github.com/ZyroMysty/Wavely/releases/download/v${version}/Wavely-Native_${version}_x64-setup.exe`,
  signature,
};
mkdirSync(join(root, "updates"), { recursive: true });
writeFileSync(join(root, "updates", "latest-native.json"), JSON.stringify(manifest, null, 2) + "\n");
console.log(`Fertig: ${setup}`);
