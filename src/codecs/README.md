# Model-specific serde JSON codecs

`RustJsonEmitter` implements `JsonCodecEmitter` and emits `json_codecs.rs` with
`read_<RustModelName>_json` and `write_<RustModelName>_json` functions. The consumer
module root exposes sibling `models` (08c), `contracts` (08a) and `values` (08d)
modules. A downstream crate can re-export contracts/values from lutaml-model once
08f wires those public modules. Generated consumers need serde (derive), pinned
serde_json with arbitrary_precision, and the exact-value dependencies. A bundle with a
`hash` attribute also needs serde_json `preserve_order` (lutaml-rs enables it; the
codecs refuse otherwise): its values are neutral JSON read with exact number spellings,
while a number anywhere else still refuses as a non-portable exact boundary.

Readers lower declared aliases and nested model/discriminator mappings into the
static generated structs, preserve absent/null/value, and run model validation.
They do not automatically apply defaults, which would erase absence. Writers
apply declared nil/empty/default render policies and use the first declared alias.
Missing render policies resolve to Ruby's false/omit policy for all three options.
Exact values remain strings; merged 08d helpers validate integer/decimal lexical
boundaries without floating conversion. Model fields remain typed; JSON values
are only temporary input/output buffers. Schema declarations are compiled out.

Canonical rule-level root/child descriptors bind their own `to` collection.
Legacy mapping-level descriptors bind a single unambiguous model collection. `key` names its object key, `value` names the whole object value, and
string/array paths lower nested values. A key-only descriptor preserves the child
model's remaining JSON mappings; additional explicit paths replace them. Ambiguous
collection targets, missing keys, overlapping paths, custom codec bindings without
a callable signature, and keyed discriminator collections reject explicitly.
Root keyed descriptors with ordered-content metadata reject until an envelope is
declared. Ordinary ordered-content token lists round-trip with field mappings.

The focused tests compile and run an actual generated consumer in target/. The
outer Cargo invocation must hold the host memory lock; the child build uses one
worker and shares that lease.

The basic Ruby oracle is pinned from core card 04a revision
`22b74ccc9a22d6f19de629c1d327ec8479128efb` in `oracle/`, with manifest and file
SHA-256 verification. The Rust target explicitly selects `portable-exact-json-v1`:
expected/executed IDs are both `[core-json-portable-exact]`, counts 1/1. Native
Ruby's four declared cases remain accounted under their separate profile; they
are not claimed as Rust passes. Missing profiles/cases, unsupported target
profiles and corrupted bytes fail. Schema identity comes from the immutable 08b
Snapshot, never a hash of its normalized detached Bundle.

This replay covers basic portable JSON only. It makes no XML claim and freezes
no ordered/mixed/polymorphic/keyed oracle expectations affected by 02d. Those
behaviors currently have synthetic generated-consumer tests. The 02d format/rule
extensions are retained by the shared typed contracts and 08b validation. Target
operations without an implementation (unknown policy strings, rule-only
discriminators, polymorphic maps) refuse explicitly instead of dropping metadata.

Model names come directly from `generator::model_names`; normalization is shared
with actual structs, enums and PARG constructors.
