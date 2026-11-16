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
