// Native Rust replay of the packaged corpus, then the same corpus through the wasm
// codecs in Node and (when CHROME_BIN or a local Chrome exists) headless Chrome.
import { readFile, writeFile, mkdtemp, rm, realpath, access } from 'node:fs/promises';
import { createServer } from 'node:http';
import { spawnSync } from 'node:child_process';
import { fileURLToPath } from 'node:url';
import { tmpdir } from 'node:os';
import { join, resolve, extname } from 'node:path';
import { replay } from './replay.mjs';
import { bindgenDigest } from '../wasm_json/bindgen.mjs';
import { runBrowser } from '../wasm_json/browser.mjs';
const repo = resolve(fileURLToPath(new URL('../..', import.meta.url)));
const example = `${repo}/examples/mml-wasm`;
const packaged = `${example}/target/package`;
const artifactRoot = await realpath(packaged);
const read = async path => {
  const file = await realpath(resolve(artifactRoot, path));
  if (!file.startsWith(`${artifactRoot}/`)) throw Error('artifact symlink escapes root');
  return readFile(file);
};
// Shared Ruby case IDs: the native xml_runtime oracle, in the same order and count.
const oracle = JSON.parse(await readFile(`${repo}/tests/fixtures/xml_oracle.json`, 'utf8'));
const corpus = JSON.parse(await read('cases.json'));
const expectedIds = [...oracle.cases.map(c => c.id), 'v3-mi-mixed', 'v4-mi-mixed'];
if (JSON.stringify(corpus.cases.map(c => c.id)) !== JSON.stringify(expectedIds)) throw Error('packaged case IDs differ from the Ruby/native oracle IDs');

function nativeRun(ops) {
  const result = spawnSync(process.env.CARGO ?? 'cargo', ['run', '--quiet', '--locked', '--manifest-path', `${example}/Cargo.toml`, '--features', 'native', '--bin', 'native_replay'],
    { input: JSON.stringify(ops), encoding: 'utf8', maxBuffer: 1 << 28 });
  if (result.status !== 0) throw Error(`native replay failed (${result.status}): ${result.stderr}`);
  return JSON.parse(result.stdout).map(r => r.err ? { err: r.err } : { ok: r.ok });
}
const reads = nativeRun(corpus.cases.map(c => ({ op: 'read', set: c.set, model: c.model_id, input: c.input })));
const writable = corpus.cases.map((c, i) => ({ c, i })).filter(({ i }) => reads[i].ok !== undefined);
const written = nativeRun(writable.map(({ c, i }) => ({ op: 'write', set: c.set, model: c.model_id, input: reads[i].ok })));
const native = { cases: corpus.cases.map((c, i) => ({ id: c.id, read: reads[i], write: written[writable.findIndex(w => w.i === i)] })) };
await writeFile(`${example}/target/native-results.json`, JSON.stringify(native));
console.log(`Native PASS ${native.cases.length} cases, ${reads.filter(r => r.err).length} errors, ${written.length} writes`);

const wasmBindgenSha256 = await bindgenDigest();
const options = { wasmBindgenSha256, manifestBytes: await read('manifest.json'), expectedDigest: (await read('manifest.sha256')).toString(), read,
  importModule: source => import(`data:text/javascript;base64,${Buffer.from(source).toString('base64')}`) };
console.log('Node PASS', JSON.stringify(await replay(options, native)));

const chrome = process.env.CHROME_BIN ?? '/Applications/Google Chrome.app/Contents/MacOS/Google Chrome';
const haveChrome = await access(chrome).then(() => true, () => false);
if (!haveChrome) {
  if (process.env.REQUIRE_BROWSER) throw Error(`browser required but not found: ${chrome}`);
  console.log(`Browser NOT RUN: no Chrome at ${chrome}`);
} else {
  const server = createServer(async (request, response) => {
    try {
      const path = resolve(repo, '.' + decodeURIComponent(new URL(request.url, 'http://localhost').pathname));
      if (!path.startsWith(`${repo}/`)) throw Error('outside root');
      const mime = { '.mjs': 'text/javascript', '.js': 'text/javascript', '.wasm': 'application/wasm', '.html': 'text/html', '.json': 'application/json' };
      const file = await realpath(path);
      if (!file.startsWith(`${repo}/`)) throw Error('symlink outside root');
      response.writeHead(200, { 'Content-Type': mime[extname(path)] ?? 'application/octet-stream' }); response.end(await readFile(file));
    } catch { response.writeHead(404); response.end(); }
  });
  await new Promise(resolve => server.listen(0, '127.0.0.1', resolve));
  const profile = await mkdtemp(join(tmpdir(), 'lml-wasm-xml-chrome-'));
  try {
    const result = await runBrowser(chrome, profile, `http://127.0.0.1:${server.address().port}/tests/wasm_xml/smoke.html?bindgen=${encodeURIComponent(wasmBindgenSha256)}`);
    console.log('Browser PASS', result);
  } finally { server.close(); await rm(profile, { recursive: true, force: true }); }
}
