// Actual Node wasm execution followed by the same corpus in headless Chrome.
import { readFile, mkdtemp, rm, realpath } from 'node:fs/promises';
import { createServer } from 'node:http';
import { fileURLToPath } from 'node:url';
import { tmpdir } from 'node:os';
import { join, resolve, extname } from 'node:path';
import { replay } from './replay.mjs';
import { bindgenDigest } from './bindgen.mjs';
import { runBrowser } from './browser.mjs';
const repo = resolve(fileURLToPath(new URL('../..', import.meta.url)));
const packaged = `${repo}/examples/model-wasm/target/package`;
const artifactRoot = await realpath(packaged);
const read = async path => {
  const file = await realpath(resolve(artifactRoot, path));
  if (!file.startsWith(`${artifactRoot}/`)) throw Error('artifact symlink escapes root');
  return readFile(file);
};
const wasmBindgenSha256 = await bindgenDigest();
const options = { wasmBindgenSha256, manifestBytes: await read('manifest.json'), expectedDigest: (await read('manifest.sha256')).toString(), read,
  importModule: source => import(`data:text/javascript;base64,${Buffer.from(source).toString('base64')}`) };
console.log('Node PASS', JSON.stringify(await replay(options)));
const server = createServer(async (request, response) => {
  try {
    const path = resolve(repo, '.' + decodeURIComponent(new URL(request.url, 'http://localhost').pathname));
    if (!path.startsWith(`${repo}/`)) throw Error('outside root');
    const mime = { '.mjs': 'text/javascript', '.js': 'text/javascript', '.wasm': 'application/wasm', '.html': 'text/html' };
    const file = await realpath(path);
    if (!file.startsWith(`${repo}/`)) throw Error('symlink outside root');
    const bytes = await readFile(file);
    response.writeHead(200, { 'Content-Type': mime[extname(path)] ?? 'application/octet-stream' }); response.end(bytes);
  } catch { response.writeHead(404); response.end(); }
});
await new Promise(resolve => server.listen(0, '127.0.0.1', resolve));
const chrome = process.env.CHROME_BIN ?? '/Applications/Google Chrome.app/Contents/MacOS/Google Chrome';
const profile = await mkdtemp(join(tmpdir(), 'lml-wasm-chrome-'));
try {
  const result = await runBrowser(chrome, profile, `http://127.0.0.1:${server.address().port}/tests/wasm_json/smoke.html?bindgen=${encodeURIComponent(wasmBindgenSha256)}`);
  console.log('Browser PASS', result);
} finally { server.close(); await rm(profile, { recursive: true, force: true }); }
