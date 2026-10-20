// Lädt yt-dlp.exe aus den offiziellen GitHub-Releases und prüft die SHA-256-Prüfsumme.
// Wird von Tauri als „externalBin“ neben die App gelegt.
import { createHash } from "node:crypto";
import { existsSync, mkdirSync, writeFileSync } from "node:fs";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";

const root = join(dirname(fileURLToPath(import.meta.url)), "..");
const target = join(root, "src-tauri", "binaries", "yt-dlp-x86_64-pc-windows-msvc.exe");
const base = "https://github.com/yt-dlp/yt-dlp/releases/latest/download";

if (existsSync(target) && !process.argv.includes("--force")) {
  console.log("yt-dlp ist bereits vorhanden.");
  process.exit(0);
}

async function download(url) {
  const res = await fetch(url);
  if (!res.ok) throw new Error(`${url}: HTTP ${res.status}`);
  return Buffer.from(await res.arrayBuffer());
}

const [exe, sums] = await Promise.all([download(`${base}/yt-dlp.exe`), download(`${base}/SHA2-256SUMS`)]);
const expected = sums
  .toString()
  .split("\n")
  .find((line) => line.trim().endsWith(" yt-dlp.exe"))
  ?.split(/\s+/)[0];
const actual = createHash("sha256").update(exe).digest("hex");

if (!expected || expected !== actual) {
  throw new Error(`Prüfsumme stimmt nicht (erwartet ${expected}, erhalten ${actual})`);
}

mkdirSync(dirname(target), { recursive: true });
writeFileSync(target, exe);
console.log(`yt-dlp gespeichert (${(exe.length / 1024 / 1024).toFixed(1)} MB, Prüfsumme OK).`);
