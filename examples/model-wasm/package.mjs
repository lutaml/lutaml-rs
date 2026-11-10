// Run after the locked wasm build. Output is ignored build data in target/.
import { readFile, writeFile, mkdir, readdir, copyFile } from 'node:fs/promises';
import { spawnSync } from 'node:child_process';
import { fileURLToPath } from 'node:url';
import { digest } from '../../src/wasm/loader.mjs';
const root = fileURLToPath(new URL('.', import.meta.url));
const target = `${root}target/wasm32-unknown-unknown/release`;
const out = `${root}target/package`;
const cli = process.env.WASM_BINDGEN ?? 'wasm-bindgen';
const cliPath = cli.includes('/') ? cli : spawnSync('which', [cli], { encoding: 'utf8' }).stdout.trim();
const cliDigest = await digest(await readFile(cliPath));
const version = spawnSync(cli, ['--version'], { encoding: 'utf8' });
if (version.status !== 0 || version.stdout.trim() !== 'wasm-bindgen 0.2.100') throw Error('wasm-bindgen 0.2.100 required');
await mkdir(out, { recursive: true });
const result = spawnSync(cli, [`${target}/model_wasm.wasm`, '--target', 'web', '--out-dir', out, '--out-name', 'model_wasm'], { stdio: 'inherit' });
if (result.status !== 0) throw Error('wasm-bindgen packaging failed');
const candidates = [];
for (const dir of await readdir(`${target}/build`)) {
  try {
    const path = `${target}/build/${dir}/out`;
    const identity = JSON.parse(await readFile(`${path}/identity.json`, 'utf8'));
    candidates.push({ path, identity });
  } catch { /* build directories for dependencies have no identity */ }
}
if (candidates.length !== 1) throw Error('expected exactly one generated fixture build; clean stale example build outputs');
const { path, identity } = candidates[0];
for (const file of ['models.rs', 'json_codecs.rs']) await copyFile(`${path}/${file}`, `${out}/${file}`);
await copyFile(`${root}fixture.json`, `${out}/fixture.json`);
const sourceCorpus = JSON.parse(await readFile(`${root}../../tests/wasm_json/cases.json`, 'utf8'));
const revision = spawnSync('git', ['rev-parse', 'HEAD'], { cwd: root, encoding: 'utf8' });
if (revision.status !== 0) throw Error('source revision unavailable');
sourceCorpus.source_revision = revision.stdout.trim();
sourceCorpus.cases = sourceCorpus.cases.map(test => ({ ...test, model_id: 'Invoice@default', lml_checksum: identity.lml_checksum, format: 'json', comparison: 'data', expected_data: test.expected, expected_output: test.expected && JSON.stringify(test.expected) }));
await writeFile(`${out}/cases.json`, JSON.stringify(sourceCorpus));
const files = [];
for (const path of ['cases.json', 'fixture.json', 'json_codecs.rs', 'model_wasm.js', 'model_wasm_bg.wasm', 'models.rs'])
  files.push({ path, sha256: await digest(await readFile(`${out}/${path}`)) });
const manifest = { manifest: 1, schema_version: 2, generator_version: 'lml-rust-wasm-v1', target: 'wasm',
  lml_checksum: identity.lml_checksum, capabilities: ['json'], corpora: [{ generation: 'wasm-json-v1', sha256: files.find(f => f.path === 'cases.json').sha256 }],
  dependencies: [{ name: 'wasm-bindgen', revision: '0.2.100', sha256: cliDigest }], files };
const bytes = new TextEncoder().encode(JSON.stringify(manifest, (_, value) => value && typeof value === 'object' && !Array.isArray(value) ? Object.fromEntries(Object.entries(value).sort(([a], [b]) => a < b ? -1 : a > b ? 1 : 0)) : value));
await writeFile(`${out}/manifest.json`, bytes);
// Consumer-owned pin, separate from the manifest it authenticates.
await writeFile(`${out}/manifest.sha256`, await digest(bytes));
console.log(`Packaged ${out}`);
