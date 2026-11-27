# Rust artifact CLI

```
lutaml-model generate --input bundle.json --output generated
lutaml-model check --input bundle.json --output generated --manifest-sha256 sha256:EXPECTED
```

Generation prints the canonical manifest digest. Store it in trusted consumer
configuration; checking requires that pin rather than trusting a digest packaged
beside the files. JSON and YAML inputs are selected by extension, or explicitly
with `--format json|yaml`. Check recomputes the expected generation from the
source snapshot and verifies all manifest pins and file bytes. The manifest must
have its generated canonical encoding. Traversal and artifact symlinks reject.

Include `generated/mod.rs` as a Rust module. The consumer needs `lutaml-model`
with `default-features = false, features = ["runtime"]`, serde `=1.0.219` with
`derive`, and serde_json `=1.0.140` with `arbitrary_precision`. Models, JSON codecs,
and PARG module declarations are generated together; empty PARG bindings contain
no application grammar. Custom bindings remain explicit emitter APIs.

Manifest identity uses the original loaded LML wire checksum. Generator identity
pins emitter source/runtime templates; the runtime dependency pin hashes this
crate's Cargo.lock, shared contracts and exact-value implementation, in that order.
Manifest capabilities describe generated JSON codecs; XML source declarations do not advertise an XML runtime.
Add `--corpus-index index.json --corpus-sha256 sha256:EXPECTED` to generation
and checking to pin an optional corpus index. Both options are required together;
the SHA-256 covers the actual index bytes, including whitespace. Version 1,
nonempty generation/cases, unique nonempty case IDs, and every case's original
LML source checksum are verified before any artifact writes. The manifest records
only generation and index digest. This proves index identity and source binding;
it does not claim execution or coverage of indexed cases. Checking a corpus-pinned
artifact requires the same explicit index and pin. Without these options no corpus
is implicitly claimed and the corpus list is empty. Generated
files and dependencies are sorted. The manifest digest is external and includes
all canonical manifest fields. Artifact directories must not be modified
concurrently during generation or verification.

Host loading, generation, codecs and artifact APIs require the `generator`
feature. Shared values/contracts remain available to runtime, XML and wasm
consumers; XML/Wasm implementations use the existing explicit plugin traits and
feature slots and are integrated separately.
