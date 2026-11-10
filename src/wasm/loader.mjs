// Verify every byte before instantiating wasm or importing generated glue.
export class ArtifactError extends Error {
  constructor(reason, location, detail) {
    super(detail); this.name = 'ArtifactError'; this.reason = reason; this.location = location;
  }
}
const fail = (reason, location, detail) => { throw new ArtifactError(reason, location, detail); };
export async function digest(bytes) {
  const hash = new Uint8Array(await crypto.subtle.digest('SHA-256', bytes));
  return `sha256:${Array.from(hash, b => b.toString(16).padStart(2, '0')).join('')}`;
}
// One loader, one profile per generated artifact family (JSON or XML codecs).
export const JSON_PROFILE = {
  generatorVersion: 'lml-rust-wasm-v1', capability: 'json', capabilityLabel: 'JSON', corpusGeneration: 'wasm-json-v1', glue: 'model_wasm',
  required: ['model_wasm.js', 'model_wasm_bg.wasm', 'fixture.json', 'models.rs', 'json_codecs.rs', 'cases.json'],
  extras: () => ({}),
};
export const XML_PROFILE = {
  generatorVersion: 'lml-rust-wasm-xml-v1', capability: 'xml', capabilityLabel: 'XML', corpusGeneration: 'wasm-xml-v1', glue: 'mml_wasm',
  required: ['mml_wasm.js', 'mml_wasm_bg.wasm', 'cases.json', 'oracle.json', 'mml-v3-mi-mixed.json', 'mml-v4-mi-mixed.json', 'mml-oracle.json',
    'oracle_models.rs', 'oracle_xml_codecs.rs', 'mml_v3_models.rs', 'mml_v3_xml_codecs.rs', 'mml_v4_models.rs', 'mml_v4_xml_codecs.rs'],
  extras: text => ({ bundles: { oracle: JSON.parse(text('oracle.json')), mml_v3: JSON.parse(text('mml-v3-mi-mixed.json')), mml_v4: JSON.parse(text('mml-v4-mi-mixed.json')) } }),
};
// `wasmBindgenSha256` is the consumer-owned digest of the wasm-bindgen 0.2.100 CLI that
// may have produced the glue (the binary differs per platform, so the loader cannot
// carry one value). It is compared exactly; the manifest only says what it claims.
export async function loadArtifact({ manifestBytes, expectedDigest, read, importModule, wasmBindgenSha256, profile = JSON_PROFILE }) {
  if (await digest(manifestBytes) !== expectedDigest)
    fail('checksum_mismatch', '/manifest', 'manifest digest mismatch');
  let manifest;
  try { manifest = JSON.parse(new TextDecoder('utf-8', { fatal: true }).decode(manifestBytes)); }
  catch { fail('invalid_document', '/manifest', 'invalid manifest JSON'); }
  if (!manifest || typeof manifest !== 'object' || Array.isArray(manifest))
    fail('invalid_document', '/manifest', 'manifest object required');
  if (manifest.manifest !== 1 || manifest.schema_version !== 2 || manifest.generator_version !== profile.generatorVersion || manifest.target !== 'wasm')
    fail('unsupported_version', '/manifest', 'unsupported wasm artifact version');
  if (!Array.isArray(manifest.capabilities) || !manifest.capabilities.includes(profile.capability))
    fail('capability_missing', '/capabilities', `${profile.capabilityLabel} codec required`);
  if (!Array.isArray(manifest.dependencies) || !manifest.dependencies.some(d => d && d.name === 'wasm-bindgen' && d.revision === '0.2.100' && /^sha256:[a-f0-9]{64}$/.test(d.sha256) && d.sha256 === wasmBindgenSha256))
    fail('unsupported_version', '/dependencies', 'wasm-bindgen 0.2.100 with the consumer-pinned digest required');
  if (!Array.isArray(manifest.files) || !Array.isArray(manifest.corpora))
    fail('invalid_document', '/files', 'files and corpus pins required');
  const files = new Map();
  for (const entry of manifest.files) {
    if (!entry || typeof entry !== 'object' || !/^sha256:[a-f0-9]{64}$/.test(entry.sha256))
      fail('invalid_document', '/files', 'file digest required');
    const { path, sha256 } = entry;
    if (typeof path !== 'string' || !path || /[\\:%?#]/.test(path) || path.startsWith('/') || path.split('/').some(p => !p || p === '.' || p === '..') || files.has(path))
      fail('invalid_document', '/files', 'unsafe or duplicate artifact path');
    let bytes;
    try { bytes = await read(path); } catch { fail('checksum_mismatch', `/files/${path}`, 'missing artifact'); }
    if (await digest(bytes) !== sha256) fail('checksum_mismatch', `/files/${path}`, 'artifact digest mismatch');
    files.set(path, bytes);
  }
  for (const path of profile.required)
    if (!files.has(path)) fail('invalid_document', '/files', `required artifact ${path} missing`);
  if (!manifest.corpora.some(c => c && c.generation === profile.corpusGeneration && c.sha256 === manifest.files.find(f => f.path === 'cases.json').sha256))
    fail('checksum_mismatch', '/corpora', 'corpus pin mismatch');
  const text = path => new TextDecoder('utf-8', { fatal: true }).decode(files.get(path));
  const wasm = await importModule(text(`${profile.glue}.js`));
  await wasm.default({ module_or_path: files.get(`${profile.glue}_bg.wasm`) });
  wasm.initialize(manifest.generator_version, manifest.lml_checksum);
  return { wasm, manifest, corpus: JSON.parse(text('cases.json')), ...profile.extras(text) };
}
