// Lightweight negative controls run before the generated-code dependency lands.
import { test } from 'node:test';
import assert from 'node:assert/strict';
import { digest, loadArtifact } from '../../src/wasm/loader.mjs';
const ZERO = 'sha256:' + '0'.repeat(64);
test('rejects a manifest digest before parsing or importing', async () => {
  let imported = false;
  await assert.rejects(loadArtifact({ manifestBytes: new TextEncoder().encode('{}'), expectedDigest: 'bad', read: async () => { throw Error('unexpected read'); }, importModule: () => { imported = true; } }), { reason: 'checksum_mismatch' });
  assert.equal(imported, false);
});
test('rejects unknown versions with authenticated manifest bytes', async () => {
  const bytes = new TextEncoder().encode('{"manifest":2}');
  await assert.rejects(loadArtifact({ manifestBytes: bytes, expectedDigest: await digest(bytes) }), { reason: 'unsupported_version' });
});
test('authenticated malformed shapes always return named errors', async () => {
  const base = { manifest: 1, schema_version: 2, generator_version: 'lml-rust-wasm-v1', target: 'wasm', capabilities: ['json'], dependencies: [{ name: 'wasm-bindgen', revision: '0.2.100', sha256: 'sha256:' + '0'.repeat(64) }], files: [], corpora: [] };
  for (const [manifest, reason] of [[null, 'invalid_document'], [{ ...base, files: [null] }, 'invalid_document'], [{ ...base, files: [{ path: '../escape', sha256: 'sha256:' + '0'.repeat(64) }] }, 'invalid_document'], [{ ...base, dependencies: {} }, 'unsupported_version'], [{ ...base, dependencies: [null] }, 'unsupported_version']]) {
    const bytes = new TextEncoder().encode(JSON.stringify(manifest));
    await assert.rejects(loadArtifact({ wasmBindgenSha256: ZERO, manifestBytes: bytes, expectedDigest: await digest(bytes), read: () => { throw Error('must reject before file access'); } }), { reason });
  }
});
test('a wasm-bindgen digest that is well-formed but not the consumer pin is refused', async () => {
  const dependencies = [{ name: 'wasm-bindgen', revision: '0.2.100', sha256: ZERO }];
  const manifest = { manifest: 1, schema_version: 2, generator_version: 'lml-rust-wasm-v1', target: 'wasm', capabilities: ['json'], dependencies, files: [], corpora: [] };
  const bytes = new TextEncoder().encode(JSON.stringify(manifest));
  const read = () => { throw Error('must reject before file access'); };
  for (const wasmBindgenSha256 of ['sha256:' + '1'.repeat(64), undefined])
    await assert.rejects(loadArtifact({ wasmBindgenSha256, manifestBytes: bytes, expectedDigest: await digest(bytes), read }), { reason: 'unsupported_version' });
  await assert.rejects(loadArtifact({ wasmBindgenSha256: ZERO, manifestBytes: bytes, expectedDigest: await digest(bytes), read }), { reason: 'invalid_document' });
});
