// Shared by Node and the browser: replay every corpus case through the wasm codecs,
// compare to the Ruby corpus and, byte for byte, to the native Rust results.
import { loadArtifact as load, digest, XML_PROFILE } from '../../src/wasm/loader.mjs';
const loadArtifact = options => load({ profile: XML_PROFILE, ...options });
import { dump, withoutRubyMarker } from './dump.mjs';
import { xmlTree } from './xmltree.mjs';
const canonical = value => JSON.stringify(value, (_, v) => v && typeof v === 'object' && !Array.isArray(v) ? Object.fromEntries(Object.entries(v).sort(([a], [b]) => a < b ? -1 : a > b ? 1 : 0)) : v);
const assert = (condition, detail) => { if (!condition) throw Error(detail); };
const same = (a, b, detail) => assert(canonical(a) === canonical(b), `${detail}\n  got      ${canonical(a)}\n  expected ${canonical(b)}`);
// wasm throws the serialized ContractError; native prints it as {err}.
const attempt = fn => { try { return { ok: fn() }; } catch (error) { return { err: error }; } };
const nullable = e => Object.fromEntries(['reason', 'model_id', 'attribute', 'rule', 'format', 'location', 'detail'].map(k => [k, e?.[k] ?? null]));
async function rejects(fn, reason, id) {
  try { await fn(); } catch (error) { assert(error.reason === reason, `${id}: expected ${reason}, got ${error.reason}: ${error.message}`); return; }
  throw Error(`${id}: expected ${reason}`);
}
export async function replay(options, native) {
  const { wasm, manifest, corpus, bundles } = await loadArtifact(options);
  assert(corpus.corpus === 1 && corpus.generation === 'wasm-xml-v1' && corpus.cases.length === 34, 'missing expected corpus');
  assert(native.cases.length === corpus.cases.length, 'native results cover a different case count');
  const checksums = JSON.parse(wasm.set_checksums());
  const ids = new Set();
  let reads = 0, writes = 0, errors = 0, trees = 0;
  for (const test of corpus.cases) {
    assert(!ids.has(test.id), `duplicate case ${test.id}`); ids.add(test.id);
    assert(test.format === 'xml' && checksums[test.set], `${test.id}: case identity mismatch`);
    const ref = native.cases.find(c => c.id === test.id);
    assert(ref, `${test.id}: no native result`);
    const read = attempt(() => wasm.read_xml(test.set, test.model_id, test.input));
    reads++;
    if (test.expected_error) {
      assert(read.err, `${test.id}: expected error ${test.expected_error}, got ${read.ok}`);
      assert(read.err.reason === test.expected_error && read.err.format === 'xml' && typeof read.err.model_id === 'string', `${test.id}: error ${JSON.stringify(read.err)}`);
      if (test.expected_attribute) assert(read.err.attribute === test.expected_attribute && read.err.rule === test.expected_attribute, `${test.id}: attribute ${JSON.stringify(read.err)}`);
      same(nullable(read.err), nullable(ref.read.err), `${test.id}: wasm error differs from native`);
      errors++;
      continue;
    }
    assert(read.ok !== undefined, `${test.id}: read failed ${JSON.stringify(read.err)}`);
    assert(read.ok === ref.read.ok, `${test.id}: wasm read bytes differ from native`);
    const model = JSON.parse(read.ok);
    same(dump(bundles[test.set], test.model_id, model), withoutRubyMarker(test.expected_data), `${test.id}: data differs from Ruby corpus`);
    const write = attempt(() => wasm.write_xml(test.set, test.model_id, read.ok));
    writes++;
    assert(write.ok !== undefined, `${test.id}: write failed ${JSON.stringify(write.err)}`);
    assert(write.ok === ref.write.ok, `${test.id}: wasm write bytes differ from native`);
    if (test.comparison === 'xml_tree') {
      same(xmlTree(write.ok), test.expected_tree, `${test.id}: written tree differs from Ruby tree`);
      same(xmlTree(test.expected_output), test.expected_tree, `${test.id}: corpus bytes and tree disagree`);
      assert(write.ok === test.rust_output, `${test.id}: written bytes differ from the pinned Rust bytes: ${write.ok}`);
      trees++;
    }
    // The written document reads back to the same model through wasm.
    const again = attempt(() => wasm.read_xml(test.set, test.model_id, write.ok));
    assert(again.ok !== undefined, `${test.id}: written XML unreadable ${JSON.stringify(again.err)}`);
  }
  assert(errors >= 6 && trees === 2, `unexpected case mix: ${errors} errors, ${trees} tree comparisons`);
  // Unavailable capabilities, uninitialized and mis-pinned sessions fail loudly.
  const call = (set, model) => () => wasm.read_xml(set, model, '<x/>');
  await rejects(call('oracle', 'XmlOracle::Missing@default'), 'capability_missing', 'unknown-model');
  await rejects(call('nope', 'XmlOracle::Person@default'), 'capability_missing', 'unknown-set');
  await rejects(() => wasm.write_xml('oracle', 'XmlOracle::Plain@default', '{"v":1}'), 'invalid_document', 'bad-model-json');
  await rejects(() => wasm.initialize('wrong', manifest.lml_checksum), 'unsupported_version', 'init-version');
  await rejects(call('oracle', 'XmlOracle::Plain@default'), 'capability_missing', 'init-gate');
  await rejects(() => wasm.initialize(manifest.generator_version, 'sha256:wrong'), 'checksum_mismatch', 'init-checksum');
  wasm.initialize(manifest.generator_version, manifest.lml_checksum);
  assert(JSON.parse(wasm.read_xml('oracle', 'XmlOracle::Plain@default', '<plain><v>1</v></plain>')).v === '1', 'reinitialization');
  // Corrupt or mis-pinned artifacts never reach an import.
  let imports = 0;
  const noImport = async () => { imports++; throw Error('unverified code imported'); };
  await rejects(() => loadArtifact({ ...options, expectedDigest: 'sha256:wrong', importModule: noImport }), 'checksum_mismatch', 'manifest-pin');
  const byteControls = manifest.files.map(f => f.path);
  for (const path of byteControls) {
    await rejects(() => loadArtifact({ ...options, importModule: noImport, read: async name => name === path ? new Uint8Array([0]) : options.read(name) }), 'checksum_mismatch', `byte-pin-${path}`);
  }
  await rejects(() => loadArtifact({ ...options, importModule: noImport, read: async name => { if (name === 'mml_wasm_bg.wasm') throw Error('gone'); return options.read(name); } }), 'checksum_mismatch', 'missing-wasm');
  const metadata = [['generator_version', 'wrong', 'unsupported_version'], ['manifest', 2, 'unsupported_version'], ['capabilities', [], 'capability_missing'], ['capabilities', ['json'], 'capability_missing'], ['corpora', [], 'checksum_mismatch'], ['dependencies', [], 'unsupported_version']];
  for (const [key, value, reason] of metadata) {
    const bytes = new TextEncoder().encode(JSON.stringify({ ...manifest, [key]: value }));
    const expectedDigest = await digest(bytes);
    await rejects(() => loadArtifact({ ...options, manifestBytes: bytes, expectedDigest, importModule: noImport }), reason, `metadata-${key}`);
  }
  assert(imports === 0, 'verification must precede every module import');
  // A wasm that is correctly pinned but not a valid module fails at instantiation, loudly.
  const garbage = new Uint8Array([0, 97, 115, 109, 1, 0, 0, 0, 255]);
  const pinned = { ...manifest, files: manifest.files.map(f => f.path === 'mml_wasm_bg.wasm' ? { ...f, sha256: null } : f) };
  pinned.files.find(f => f.path === 'mml_wasm_bg.wasm').sha256 = await digest(garbage);
  const pinnedBytes = new TextEncoder().encode(JSON.stringify(pinned));
  let instantiated = true;
  const pinnedDigest = await digest(pinnedBytes);
  try { await loadArtifact({ ...options, manifestBytes: pinnedBytes, expectedDigest: pinnedDigest,
    // A distinct source forces a fresh glue module; wasm-bindgen's init is a no-op once initialized.
    importModule: source => options.importModule(`${source}\n// fresh instance ${Math.random()}`), read: async name => name === 'mml_wasm_bg.wasm' ? garbage : options.read(name) }); }
  catch (error) { instantiated = false; assert(/wasm|WebAssembly|module|compile/i.test(String(error?.message ?? error)), `invalid wasm failed without a wasm error: ${error}`); }
  assert(!instantiated, 'a pinned but invalid wasm module loaded');
  return { ids: [...ids], executed: ids.size, reads, writes, errors, tree_comparisons: trees, boundary_checks: 8 + byteControls.length + metadata.length + 1 };
}
