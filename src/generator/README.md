# Static Rust emitter

`RustEmitter` implements the shared `ModelEmitter` and `PargBindingEmitter`
contracts. Wire it with `pub mod generator` when the root integration card lands.
Generated `models.rs` is standalone and needs serde (derive) and serde_json.
Applications own behavior outside generated files.

Every field uses `Presence<T>`; serde omits absent fields, preserves null, and
retains typed false/empty values. `Default` constructs absent fields.
`apply_defaults()` explicitly initializes only absent fields, and `validate()`
checks required members, inclusive collection bounds, nested models and ordered
content references. These steps are explicit so deserialization does not erase
absence. References and discriminator payloads are boxed to size recursive types.
A `hash` attribute is an order-preserving `NeutralHash`: any JSON value that every
language holds exactly (null, bool, string, array, object, and a number only if a
double holds it: an integer-spelled number within +-(2^53-1) and not -0, otherwise
a spelling equal to ECMAScript `Number::toString`), nested to at most 200 levels,
with no duplicate keys. The XML `any_attribute` target is the same type: it reads as
string values and writes scalars as text, refusing null and nested values. The JSON
codec needs serde_json `preserve_order` (lutaml-rs enables it) to keep key order.
Integer/decimal/rational boundaries use validated lexical string newtypes without
floating-point conversion. A rational's wire boundary is a terminating decimal;
exact arithmetic/rendering is the separate values module's responsibility.

Named value types require caller-trusted `TypeBindings`; their serde implementation
must enforce the declared base/facets. Builtins needing lexical codecs, custom
collection classes and nonliteral structured defaults currently refuse with named
contract errors. No unchecked JSON field substitutes for an unsupported type.

`generate_parg_bindings` lowers declared grammar-rule/capture names into typed
`bind_<rule>` constructors in `parg_bindings.rs`, a sibling of `models.rs`.
Capture arguments follow the declaration's sorted capture keys; emitted comments
retain their stable names. Constructors apply defaults and validate the result.
Parsing, grammar execution and application methods remain external. Unknown class
IDs, members, duplicate rules/members and captures needing undeclared bindings
refuse during generation.

`model_names(&Bundle)` exposes the exact stable model-ID/type-name map and rejects
name collisions. `member_identifier(&str)` exposes field-name normalization;
callers must reject duplicate normalized members within each model. Other
emitters should reuse these helpers instead of copying normalization rules.
Generated `ContentToken::reference()` exposes a member/index pair for reference
tokens and `None` for literal text/CDATA, allowing external serializers to walk
the declared order without interpreting schemas.
