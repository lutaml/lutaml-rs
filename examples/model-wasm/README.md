# Generated model JSON in wasm

This separate crate compiles `fixture.json` with the actual 08c model and 08e JSON
emitters in `build.rs`. It needs merged 08e; a prepared wrapper is not runtime
acceptance. No generated model definition is mirrored by hand. Models, codecs,
source fixture, corpus, wasm and generated glue are pinned by artifact hashes.
Keep `manifest.sha256` in trusted consumer storage separate from downloaded
artifacts. The producer must not be the consumer's trust source.

Hold the host-wide Python `fcntl.flock` on `/private/tmp/lutaml-1033-memory.lock`
for each cargo/install/wasm job. With that lease and the Rust wasm target,
wasm-bindgen CLI exactly 0.2.100, Node 22+ and Chrome installed, run:

```sh
cargo test --locked --manifest-path examples/model-wasm/Cargo.toml
cargo build --locked --release --manifest-path examples/model-wasm/Cargo.toml --target wasm32-unknown-unknown
node examples/model-wasm/package.mjs
node tests/wasm_json/run.mjs
```

`WASM_BINDGEN` and `CHROME_BIN` can specify executable paths. Package output stays
in this example's ignored `target/package`. The runner executes the same nine
local fixture cases in actual Node and browser wasm, plus initialization and
artifact corruption controls. These case IDs are wrapper acceptance fixtures;
core Ruby corpus equivalence belongs to 08e and is not inferred from this smoke.
Exact integer/decimal values are JSON strings, including lexical trailing zeros.
No JavaScript number conversion, dynamic schema evaluation, XML stack or native
Document interface is involved.
