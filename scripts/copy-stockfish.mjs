import { copyFile, mkdir, stat } from "node:fs/promises";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";

const root = join(dirname(fileURLToPath(import.meta.url)), "..");
const source = join(root, "node_modules", "stockfish", "bin");
const target = join(root, "static", "stockfish");
const files = ["stockfish-18-lite-single.js", "stockfish-18-lite-single.wasm"];

await mkdir(target, { recursive: true });
for (const file of files) {
  const sourcePath = join(source, file);
  const info = await stat(sourcePath);
  if (!info.isFile() || info.size === 0) throw new Error(`Invalid Stockfish asset: ${file}`);
  await copyFile(sourcePath, join(target, file));
}

await copyFile(join(root, "node_modules", "stockfish", "Copying.txt"), join(target, "COPYING"));
