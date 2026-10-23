// Installer bauen. Liegt der Update-Schlüssel unter ~/.tauri/wavely.key, wird der Installer für
// automatische Updates signiert; ohne Schlüssel (z. B. auf fremden Rechnern) entsteht ein normaler Installer.
import { spawnSync } from "node:child_process";
import { existsSync, readFileSync } from "node:fs";
import { homedir } from "node:os";
import { join } from "node:path";

const env = { ...process.env };
const args = ["tauri", "build", ...process.argv.slice(2)];
const keyFile = join(homedir(), ".tauri", "wavely.key");

if (env.TAURI_SIGNING_PRIVATE_KEY) {
  // schon gesetzt
} else if (existsSync(keyFile)) {
  env.TAURI_SIGNING_PRIVATE_KEY = keyFile;
  env.TAURI_SIGNING_PRIVATE_KEY_PASSWORD ??= "";
} else {
  console.warn(`Kein Update-Schlüssel unter ${keyFile} – baue ohne Update-Signatur.`);
  args.push("--config", JSON.stringify({ bundle: { createUpdaterArtifacts: false } }));
}

// Last.fm-Zugang (nicht im Repository): ~/.tauri/lastfm.json mit { "apiKey": "…", "secret": "…" }
const lastfmFile = join(homedir(), ".tauri", "lastfm.json");
if (existsSync(lastfmFile)) {
  const { apiKey, secret } = JSON.parse(readFileSync(lastfmFile, "utf8"));
  env.LASTFM_API_KEY ??= apiKey;
  env.LASTFM_API_SECRET ??= secret;
} else {
  console.warn(`Keine Last.fm-Zugangsdaten unter ${lastfmFile} – Last.fm ist in diesem Build ausgeblendet.`);
}

const result = spawnSync("npx", args, { stdio: "inherit", env, shell: true });
process.exit(result.status ?? 1);
