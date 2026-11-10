// Documented native divergences from the Ruby oracle: the same table as OVERRIDES
// in tests/xml_runtime.rs (Ruby is lenient where the generated Rust codecs are strict).
const NULLS = { id: null, lang: null, name: null, nick: null, tags: null, addr: null, age: null, ni: null, nb: null, ee: null };
export const OVERRIDES = {
  'person-prefixed-attribute-lenient': NULLS,
  'person-wrong-root': { error: 'invalid_document' },
  'person-wrong-namespace': { error: 'invalid_document' },
  'person-bad-integer': { error: 'invalid_document', attribute: 'age' },
  'kennel-poly-unknown-kind': { error: 'unknown_discriminator' },
};

// Error reasons of the Ruby-error cases that have no override, measured on the native codecs.
export const ERROR_REASONS = {
  'person-malformed': 'invalid_document',
  'para-comment-refused': 'invalid_ordered_content',
};
// Bytes the generated Rust writer emits for the mml mi-mixed cases (prefixed form). The Ruby
// bytes use a default namespace; both resolve to the same tree, which is the contracted comparison.
export const RUST_MI_BYTES = '<mml:mi xmlns:mml="http://www.w3.org/1998/Math/MathML" mathvariant="bold">x<mml:malignmark/>y<mml:mglyph src="g.png" alt="g"/>z</mml:mi>';
