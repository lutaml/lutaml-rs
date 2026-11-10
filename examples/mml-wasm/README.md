# Generated mml/oracle XML in wasm

This crate compiles three LML bundles in `build.rs` with the actual 09a/09c emitters
(`RustEmitter`, `RustXmlEmitter`) over moxml-rs: the 32-case Ruby oracle bundle
(`fixtures/oracle.json`, copied from `tests/fixtures/xml_oracle.json`) and the small
mml v3/v4 mi-mixed graphs (3 models each, mixed ordered content). No generated model is
mirrored by hand. The large `math-nested` graphs are not replayed here.

Needs Rust >= 1.87 (the pinned moxml-rs revision), the `wasm32-unknown-unknown` target,
wasm-bindgen CLI exactly 0.2.100, Node 22+ and, for the browser run, Chrome. Hold the
host-wide `fcntl.flock` on `/private/tmp/lutaml-1033-memory.lock` for each cargo job.

```sh
cargo build --locked --manifest-path examples/mml-wasm/Cargo.toml --target wasm32-unknown-unknown --release
node examples/mml-wasm/package.mjs      # wasm-bindgen + manifest, corpus and pins in target/package
node tests/wasm_xml/run.mjs             # native replay, then Node, then headless Chrome
```

`run.mjs` replays all 34 case IDs (the 32 oracle IDs in `xml_oracle.json` order, then
`v3-mi-mixed`, `v4-mi-mixed`) natively through the same generated codecs
(`cargo run --features native --bin native_replay`) and through wasm, and requires:
reads equal the Ruby data (after the neutral dump; native divergences from Ruby are the
`OVERRIDES` table, mirrored from `tests/xml_runtime.rs`), wasm read and write outputs
equal native byte for byte, and the mml written XML resolves to the Ruby `tree`. The Rust
writer emits prefixed (`mml:`) bytes where Ruby emits a default namespace, so mml uses the
contract's `xml_tree` comparison and pins the Rust bytes separately. Corrupt or mis-pinned
artifacts, a pinned-but-invalid wasm module, an uninitialized session and unknown
models/sets fail with named errors. `CHROME_BIN` selects Chrome; without Chrome the browser
step prints `NOT RUN` unless `REQUIRE_BROWSER` is set (CI sets it).
