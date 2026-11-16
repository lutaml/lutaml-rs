// The consumer-owned wasm-bindgen pin: the digest of the installed 0.2.100 CLI, taken
// independently of the packager and handed to the loader (and the browser page).
import { readFile } from 'node:fs/promises';
import { spawnSync } from 'node:child_process';
import { digest } from '../../src/wasm/loader.mjs';
export async function bindgenDigest() {
  const cli = process.env.WASM_BINDGEN ?? 'wasm-bindgen';
  const path = cli.includes('/') ? cli : spawnSync('which', [cli], { encoding: 'utf8' }).stdout.trim();
  return digest(await readFile(path));
}
