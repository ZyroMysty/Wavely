// Installer bauen. Liegt der Update-Schlüssel unter ~/.tauri/wavely.key, wird der Installer für
// automatische Updates signiert; ohne Schlüssel (z. B. auf fremden Rechnern) entsteht ein normaler Installer.
import { spawnSync } from "node:child_process";
import { existsSync } from "node:fs";
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

const result = spawnSync("npx", args, { stdio: "inherit", env, shell: true });
process.exit(result.status ?? 1);
