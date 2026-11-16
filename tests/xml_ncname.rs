#[path = "../src/xml/contract.rs"]
#[allow(dead_code)]
mod contract;
#[path = "../src/contracts.rs"]
#[allow(dead_code)]
mod contracts;
#[path = "../src/xml/projection.rs"]
#[allow(dead_code)]
mod projection;

use contracts::Bundle;
use projection::compile_projection;
use serde_json::{json, Value};

const ID: &str = "Example::Node@default";
fn fixture() -> Value {
    json!({"lml_bundle":1,"roots":[ID],"models":{ID:{
        "lml":2,"model":"Example::Node","register":"default",
        "attributes":[
            {"name":"id","type":{"kind":"builtin","name":"string"},"required":true,"collection":false,"default":{"present":false},"initialize_empty":false},
            {"name":"children","type":{"kind":"model","id":ID},"required":false,"collection":{"min":0,"max":null},"default":{"present":false},"initialize_empty":true},
            {"name":"text","type":{"kind":"builtin","name":"string"},"required":false,"collection":false,"default":{"present":true,"value":""},"initialize_empty":false}
        ],
        "mappings":{"xml":{"root":{"element":"node"},"namespace":{"mode":"uri","uri":"urn:node","prefix":"n"},
            "rules":[
                {"kind":"attribute","name":["id","identifier"],"to":"id","namespace":{"mode":"unqualified"}},
                {"kind":"element","name":["child","legacy"],"to":"children","namespace":{"mode":"inherit"},"render":{"nil":"as_nil","empty":"blank","default":false},"discriminator":{"attribute":"kind","cases":{"node":ID}}},
                {"kind":"content","to":"text","namespace":{"mode":"inherit"},"cdata":true}
            ],
            "sequence":{"kind":"sequence","members":["children",{"kind":"choice","members":["children"],"min":0,"max":1}],"min":0,"max":null},
            "choice":{"kind":"choice","members":["children"],"min":0,"max":1}
        }},"ordered_content":{"member":"__lml_content","version":1}
    }},"value_types":{},"capabilities":["xml"]})
}
fn bundle(value: Value) -> Bundle {
    serde_json::from_value(value).unwrap()
}
fn accepts(name: &str) -> bool {
    let mut v = fixture();
    v["models"][ID]["mappings"]["xml"]["root"]["element"] = json!(name);
    compile_projection(&bundle(v), ID).is_ok()
}

#[test]
fn ncname_follows_xml_1_0_fifth_edition_ranges() {
    for ok in [
        "a",
        "A9",
        "_x",
        "a-b.c",
        "a\u{301}",
        "a\u{200c}",
        "a\u{B7}",
        "\u{C0}x",
        "\u{10000}",
    ] {
        assert!(accepts(ok), "{ok:?} should be accepted");
    }
    for bad in [
        "",
        "1a",
        "-a",
        ".a",
        "a:b",
        "a b",
        "a\u{b2}",
        "\u{aa}",
        "\u{301}a",
        "\u{B7}a",
        "\u{D7}",
        "a\u{2000}",
    ] {
        assert!(!accepts(bad), "{bad:?} should be refused");
    }
}

fn rule_name_accepted(kind_index: usize, name: &str) -> bool {
    let mut v = fixture();
    v["models"][ID]["mappings"]["xml"]["rules"][kind_index]["name"] = json!([name]);
    compile_projection(&bundle(v), ID).is_ok()
}

#[test]
fn attributes_may_use_the_predeclared_xml_prefix() {
    // rule 0 is an attribute, rule 1 an element
    assert!(rule_name_accepted(0, "xml:lang"));
    assert!(rule_name_accepted(0, "xml:space"));
    assert!(!rule_name_accepted(0, "xml:"));
    assert!(!rule_name_accepted(0, "xmlx:lang"));
    assert!(!rule_name_accepted(0, "a:b"));
    assert!(!rule_name_accepted(1, "xml:lang"));
}
