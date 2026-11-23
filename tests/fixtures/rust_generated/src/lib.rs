pub mod models;
pub mod parg_bindings;
#[cfg(test)]
mod tests {
    use super::models::*;
    #[test]
    fn absence_null_false_and_explicit_defaults() {
        let mut node: Node = serde_json::from_str(r#"{"title":"root","tags":["a"]}"#).unwrap();
        assert!(matches!(node.enabled, Presence::Absent));
        assert_eq!(serde_json::to_value(&node).unwrap().get("enabled"), None);
        node.apply_defaults().unwrap();
        assert_eq!(serde_json::to_value(&node).unwrap()["enabled"], false);
        let mut null: Node = serde_json::from_str(r#"{"title":"root","enabled":null}"#).unwrap();
        null.apply_defaults().unwrap();
        assert!(matches!(null.enabled, Presence::Null));
        assert_eq!(
            serde_json::to_value(null).unwrap()["enabled"],
            serde_json::Value::Null
        );
    }
    #[test]
    fn cardinality_required_and_recursive_validation() {
        assert!(Node::default().validate().is_err());
        let valid: Node =
            serde_json::from_str(r#"{"title":"root","tags":["one"],"child":{"title":"child"}}"#)
                .unwrap();
        valid.validate().unwrap();
        for wire in [
            r#"{"title":"root","tags":[]}"#,
            r#"{"title":"root","tags":["a","b","c"]}"#,
            r#"{"title":"root","child":{}}"#,
        ] {
            let node: Node = serde_json::from_str(wire).unwrap();
            assert!(node.validate().is_err());
        }
    }
    #[test]
    fn discriminators_and_typed_enums() {
        let node: Node = serde_json::from_str(
            r#"{"title":"root","enabled":true,"item":{"kind":"leaf","text":"ok"}}"#,
        )
        .unwrap();
        node.validate().unwrap();
        assert_eq!(serde_json::to_value(node).unwrap()["item"]["kind"], "leaf");
        assert!(serde_json::from_str::<Node>(r#"{"item":{"kind":"unknown"}}"#).is_err());
        assert!(serde_json::from_str::<Node>(r#"{"enabled":"false"}"#).is_err());
    }
    #[test]
    fn exact_values_are_strings_preserving_lexical_digits() {
        let decimal: ExactDecimal =
            serde_json::from_str("\"123456789012345678901234567890.00100\"").unwrap();
        assert_eq!(decimal.as_str(), "123456789012345678901234567890.00100");
        assert_eq!(
            serde_json::to_string(&decimal).unwrap(),
            "\"123456789012345678901234567890.00100\""
        );
        assert!(serde_json::from_str::<ExactDecimal>("0.1").is_err());
        assert!(ExactDecimal::parse("1e4").is_err());
        assert!(ExactInteger::parse("123456789012345678901234567890").is_ok());
        assert!(ExactInteger::parse("1.0").is_err());
        for ok in ["5.", ".5", "+1", "-.5", "+5.", "0.5"] {
            assert!(ExactDecimal::parse(ok).is_ok(), "{ok}");
        }
        for bad in [".", "+", "-", "", "1e4", "1.2.3", "++1", "+-1", " 1"] {
            assert!(ExactDecimal::parse(bad).is_err(), "{bad:?}");
        }
        for ok in ["+1", "-1", "007"] {
            assert!(ExactInteger::parse(ok).is_ok(), "{ok}");
        }
        for bad in ["", "+", "-", "5.", ".5", "1e2", "+-1"] {
            assert!(ExactInteger::parse(bad).is_err(), "{bad:?}");
        }
    }
    #[test]
    fn parg_capture_constructor_has_static_result_type() {
        let leaf: Leaf = super::parg_bindings::bind_leaf("captured text".into()).unwrap();
        assert_eq!(leaf.text, Presence::Value("captured text".into()));
    }

    #[test]
    fn ordered_mixed_content_round_trips_and_checks_references() {
        let wire = r#"{"title":"root","tags":["first","second"],"__lml_content":[{"kind":"text","literal":"before"},{"kind":"element","attribute":"tags","index":1},{"kind":"cdata","literal":"between"},{"kind":"element","attribute":"tags","index":0},{"kind":"text","attribute":"title","index":0}]}"#;
        let node: Node = serde_json::from_str(wire).unwrap();
        node.validate().unwrap();
        assert_eq!(
            serde_json::to_value(&node).unwrap(),
            serde_json::from_str::<serde_json::Value>(wire).unwrap()
        );
        let base = serde_json::to_value(node).unwrap();
        for tokens in [
            serde_json::json!([{"kind":"element","attribute":"tags","index":2}]),
            serde_json::json!([{"kind":"element","attribute":"missing","index":0}]),
            serde_json::json!([{"kind":"element","attribute":"tags","index":0},{"kind":"element","attribute":"tags","index":0}]),
        ] {
            let mut invalid = base.clone();
            invalid["__lml_content"] = tokens;
            let node: Node = serde_json::from_value(invalid).unwrap();
            assert_eq!(
                node.validate().unwrap_err().reason,
                "invalid_ordered_content"
            );
        }
    }

    #[test]
    fn public_content_token_reference_supports_external_traversal() {
        let token = ContentToken::Element {
            attribute: "tags".into(),
            index: 1,
        };
        assert_eq!(token.reference(), Some(("tags", 1)));
        let literal = ContentToken::Text {
            source: TextSource::Literal {
                literal: "text".into(),
            },
        };
        assert_eq!(literal.reference(), None);
    }

    #[test]
    fn content_tokens_reject_contradictory_and_unknown_fields() {
        for token in [
            r#"{"kind":"element","attribute":"tags","index":0,"literal":"x"}"#,
            r#"{"kind":"text","literal":"x","attribute":"title","index":0}"#,
            r#"{"kind":"text","literal":"x","bogus":1}"#,
            r#"{"kind":"nope","literal":"x"}"#,
        ] {
            assert!(serde_json::from_str::<ContentToken>(token).is_err(), "{token}");
        }
        assert!(serde_json::from_str::<ContentToken>(r#"{"kind":"text","literal":"x"}"#).is_ok());
    }
    #[test]
    fn discriminator_tag_is_hoisted_onto_the_case_and_written_once() {
        let node: Node = serde_json::from_str(r#"{"item":{"kind":"leaf","text":"ok"}}"#).unwrap();
        match &node.item {
            Presence::Value(Node_item_Union::Case0(leaf)) => {
                assert_eq!(leaf.kind, Presence::Value("leaf".into()))
            }
            other => panic!("unexpected {other:?}"),
        }
        let out = serde_json::to_string(&node).unwrap();
        assert_eq!(out.matches("\"kind\"").count(), 1, "{out}");
        assert_eq!(
            serde_json::from_str::<serde_json::Value>(&out).unwrap(),
            serde_json::from_str::<serde_json::Value>(r#"{"item":{"kind":"leaf","text":"ok"}}"#).unwrap()
        );
    }
    #[test]
    fn custom_initialize_separates_data_reconstruction_from_application_construction() {
        // Declared-data path (serde): unchanged and data-only.
        let data: Money = serde_json::from_str(r#"{"currency":"USD"}"#).unwrap();
        data.validate().unwrap();
        assert!(Money::CUSTOM_INITIALIZE);
        // Application construction with no handwritten implementation: the named stub failure.
        let error = Money::construct(5u32).unwrap_err();
        assert_eq!(error.reason, "custom_initialize");
        assert_eq!(error.model_id, "Money@default");
        assert_eq!(
            Money::initialize_stub(()).unwrap_err().reason,
            "custom_initialize"
        );
        // An explicit handwritten initializer succeeds.
        let built = Money::construct_with(250u32, |cents| {
            let mut money = Money::default();
            money.currency = Presence::Value(format!("{cents}c"));
            Ok(money)
        })
        .unwrap();
        assert_eq!(built.currency, Presence::Value("250c".to_string()));
        // Its result still passes validation.
        let invalid = Money::construct_with((), |_| Ok(Money::default())).unwrap_err();
        assert_eq!(invalid.reason, "invalid_document");
        assert_eq!(invalid.attribute, "currency");
    }
    #[test]
    fn apply_defaults_reaches_present_children_only() {
        let mut node: Node = serde_json::from_str(r#"{"title":"root","child":{"title":"child"}}"#).unwrap();
        node.apply_defaults().unwrap();
        let Presence::Value(child) = &node.child else { panic!("child") };
        assert!(!child.enabled.is_absent());
        assert!(child.__lml_applied.contains("enabled"));
        let mut bare: Node = serde_json::from_str(r#"{"title":"root"}"#).unwrap();
        bare.apply_defaults().unwrap();
        assert!(bare.child.is_absent());
        assert!(serde_json::from_str::<Node>(r#"{"title":"r","__lml_applied":["enabled"]}"#).is_err());
    }
}
