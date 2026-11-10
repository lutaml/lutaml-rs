// Run after the locked wasm build. Output is ignored build data in target/.
import { readFile, writeFile, mkdir, readdir, copyFile } from 'node:fs/promises';
import { spawnSync } from 'node:child_process';
import { fileURLToPath } from 'node:url';
import { digest } from '../../src/wasm/loader.mjs';
import { OVERRIDES, ERROR_REASONS, RUST_MI_BYTES } from '../../tests/wasm_xml/overrides.mjs';
const root = fileURLToPath(new URL('.', import.meta.url));
const profile = process.env.WASM_PROFILE ?? 'release';
const cargoTarget = process.env.CARGO_TARGET_DIR ?? `${root}target`;
const target = `${cargoTarget}/wasm32-unknown-unknown/${profile}`;
const out = `${root}target/package`;
const cli = process.env.WASM_BINDGEN ?? 'wasm-bindgen';
const cliPath = cli.includes('/') ? cli : spawnSync('which', [cli], { encoding: 'utf8' }).stdout.trim();
const cliDigest = await digest(await readFile(cliPath));
const version = spawnSync(cli, ['--version'], { encoding: 'utf8' });
if (version.status !== 0 || version.stdout.trim() !== 'wasm-bindgen 0.2.100') throw Error('wasm-bindgen 0.2.100 required');
await mkdir(out, { recursive: true });
const result = spawnSync(cli, [`${target}/mml_wasm.wasm`, '--target', 'web', '--out-dir', out, '--out-name', 'mml_wasm'], { stdio: 'inherit' });
if (result.status !== 0) throw Error('wasm-bindgen packaging failed');
const candidates = [];
for (const dir of await readdir(`${target}/build`)) {
  try {
    const path = `${target}/build/${dir}/out`;
    candidates.push({ path, identity: JSON.parse(await readFile(`${path}/identity.json`, 'utf8')) });
  } catch { /* build directories for dependencies have no identity */ }
}
if (candidates.length !== 1) throw Error('expected exactly one generated fixture build; clean stale example build outputs');
const { path, identity } = candidates[0];
const SETS = ['oracle', 'mml_v3', 'mml_v4'];
const generated = SETS.flatMap(set => [`${set}_models.rs`, `${set}_xml_codecs.rs`]);
for (const file of generated) await copyFile(`${path}/${file}`, `${out}/${file}`);
const fixtures = ['oracle.json', 'mml-v3-mi-mixed.json', 'mml-v4-mi-mixed.json', 'mml-oracle.json'];
for (const file of fixtures) await copyFile(`${root}fixtures/${file}`, `${out}/${file}`);
// The shared Ruby corpus: the 32-case oracle also replayed by native xml_runtime
// and the mml mi-mixed cases, each carrying the Ruby bytes.
const oracle = JSON.parse(await readFile(`${root}../../tests/fixtures/xml_oracle.json`, 'utf8'));
const mml = JSON.parse(await readFile(`${root}fixtures/mml-oracle.json`, 'utf8'));
const cases = [];
for (const c of oracle.cases) {
  const o = OVERRIDES[c.id];
  const error = c.expected_error ?? o?.error;
  if (error && !(o?.error ?? ERROR_REASONS[c.id])) throw Error(`no pinned error reason for ${c.id}`);
  cases.push({ id: c.id, set: 'oracle', model_id: c.model_id, format: 'xml', comparison: 'data', input: c.input,
    ...(error ? { expected_error: o?.error ?? ERROR_REASONS[c.id], ruby_error: c.expected_error, ...(o?.attribute && { expected_attribute: o.attribute }) }
      : { expected_data: o ?? c.expected_data }) });
}
for (const c of mml.cases) {
  const set = c.register;
  cases.push({ id: c.id, set, model_id: c.model_id, format: 'xml', comparison: 'xml_tree', input: c.input,
    expected_data: c.expected.data, expected_output: c.expected.output, expected_tree: c.expected.tree, rust_output: RUST_MI_BYTES });
}
const revision = spawnSync('git', ['rev-parse', 'HEAD'], { cwd: root, encoding: 'utf8' });
if (revision.status !== 0) throw Error('source revision unavailable');
const corpus = { corpus: 1, generation: 'wasm-xml-v1', source_revision: revision.stdout.trim(), cases };
await writeFile(`${out}/cases.json`, JSON.stringify(corpus));
const files = [];
for (const file of ['cases.json', ...fixtures, ...generated, 'mml_wasm.js', 'mml_wasm_bg.wasm'])
  files.push({ path: file, sha256: await digest(await readFile(`${out}/${file}`)) });
files.sort((a, b) => a.path < b.path ? -1 : a.path > b.path ? 1 : 0);
const manifest = { manifest: 1, schema_version: 2, generator_version: 'lml-rust-wasm-xml-v1', target: 'wasm',
  lml_checksum: identity.lml_checksum, capabilities: ['xml'], corpora: [{ generation: 'wasm-xml-v1', sha256: files.find(f => f.path === 'cases.json').sha256 }],
  dependencies: [{ name: 'wasm-bindgen', revision: '0.2.100', sha256: cliDigest }], files };
const bytes = new TextEncoder().encode(JSON.stringify(manifest, (_, value) => value && typeof value === 'object' && !Array.isArray(value) ? Object.fromEntries(Object.entries(value).sort(([a], [b]) => a < b ? -1 : a > b ? 1 : 0)) : value));
await writeFile(`${out}/manifest.json`, bytes);
// Consumer-owned pin, separate from the manifest it authenticates.
await writeFile(`${out}/manifest.sha256`, await digest(bytes));
console.log(`Packaged ${out} (${cases.length} cases)`);
