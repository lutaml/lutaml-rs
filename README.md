# lutaml-rs
## JSON profile

This crate consumes the `portable-exact-json-v1` profile of the LutaML
conformance contract: Integer, Decimal, Rational and Float values travel as
JSON strings in their plain lexical form (`"0.15"`, never `"0.15e1"`). Ruby's
default `to_json` emits the `ruby-native-v1` profile instead (numeric JSON,
and `BigDecimal#to_json` exponent spellings such as `"0.15e1"`). That output is
refused, not coerced; the decode error names both profiles. Export with the
portable profile on the Ruby side. See `docs/_references/lml-conformance.adoc`
in lutaml/lutaml-model.


## Explicit model choices

The optional v2 `choices` declaration uses version 1, validation `explicit`,
and ordered groups (`min`, `max`, `format`, `members`). A null maximum is
unbounded. Members name one declared `attribute` or carry one nested `choice`.
Bundles must advertise `model.choices.v1`. Generated models expose explicit
validation; constructors and codecs defer group checks. Existing field/type
validation remains independent. Nested alternatives, false values, finite
collection batches, optional empty collections and XML render policies follow
the Ruby source algorithm. Unsupported metadata is refused.

The source-produced fixture records each construction, JSON read/write and
XML read/write phase separately. Passing this fixture establishes those
cases, not full OMML or arbitrary custom-value-type compatibility.
