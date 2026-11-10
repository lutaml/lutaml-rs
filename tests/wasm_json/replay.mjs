import { loadArtifact, digest } from '../../src/wasm/loader.mjs';
const canonical = value => JSON.stringify(value, (_, v) => v && typeof v === 'object' && !Array.isArray(v) ? Object.fromEntries(Object.entries(v).sort(([a], [b]) => a.localeCompare(b))) : v);
const assert = (condition, detail) => { if (!condition) throw Error(detail); };
async function rejects(fn, reason, id) {
  try { await fn(); } catch (error) { assert(error.reason === reason, `${id}: expected ${reason}, got ${error.reason}: ${error.message}`); return; }
  throw Error(`${id}: expected ${reason}`);
}
export async function replay(options) {
  const { wasm, manifest, corpus } = await loadArtifact(options);
  assert(corpus.corpus === 1 && corpus.generation === 'wasm-json-v1' && corpus.cases.length === 9, 'missing expected corpus');
  const ids = new Set();
  for (const test of corpus.cases) {
    assert(test.model_id === 'Invoice@default' && test.lml_checksum === manifest.lml_checksum && test.format === 'json' && test.comparison === 'data', `${test.id}: case identity mismatch`);
    assert(!ids.has(test.id), `duplicate case ${test.id}`); ids.add(test.id);
    if (test.expected_error) await rejects(() => wasm.round_trip_json(test.input), test.expected_error, test.id);
    else assert(canonical(JSON.parse(wasm.round_trip_json(test.input))) === canonical(test.expected_data), `${test.id}: JSON output differs`);
  }
  await rejects(() => wasm.initialize('wrong', manifest.lml_checksum), 'unsupported_version', 'init-version');
  await rejects(() => wasm.round_trip_json('{"total":"1"}'), 'capability_missing', 'init-gate');
  await rejects(() => wasm.initialize(manifest.generator_version, 'sha256:wrong'), 'checksum_mismatch', 'init-checksum');
  wasm.initialize(manifest.generator_version, manifest.lml_checksum);
  assert(JSON.parse(wasm.round_trip_json('{"total":"1.00"}')).total === '1.00', 'reinitialization');
  let imports = 0;
  const noImport = async () => { imports++; throw Error('unverified code imported'); };
  await rejects(() => loadArtifact({ ...options, expectedDigest: 'sha256:wrong', importModule: noImport }), 'checksum_mismatch', 'manifest-pin');
  for (const path of ['model_wasm_bg.wasm', 'fixture.json', 'models.rs', 'cases.json', 'model_wasm.js']) {
    await rejects(() => loadArtifact({ ...options, importModule: noImport, read: async name => name === path ? new Uint8Array([0]) : options.read(name) }), 'checksum_mismatch', `byte-pin-${path}`);
  }
  for (const [key, value, reason] of [['generator_version', 'wrong', 'unsupported_version'], ['manifest', 2, 'unsupported_version'], ['capabilities', [], 'capability_missing'], ['corpora', [], 'checksum_mismatch']]) {
    const bytes = new TextEncoder().encode(JSON.stringify({ ...manifest, [key]: value }));
    const expectedDigest = await digest(bytes);
    await rejects(() => loadArtifact({ ...options, manifestBytes: bytes, expectedDigest, importModule: noImport }), reason, `metadata-${key}`);
  }
  assert(imports === 0, 'verification must precede every module import');
  return { ids: [...ids], executed: ids.size, boundary_checks: 14 };
}
