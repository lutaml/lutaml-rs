//! Regression for choice/recursive emitters combined with initializer and serialization hooks.
#![cfg(feature = "xml")]
mod support;
use serde_json::json;

#[test]
fn choices_hooks_initializers_namespaces_and_cdata_share_a_generated_consumer() {
    let attr = |name: &str| {
        json!({"name":name,"type":{"kind":"builtin","name":"string"},
        "required":false,"collection":false,"default":{"present":false},"initialize_empty":false})
    };
    let bundle = json!({"lml_bundle":1,"roots":["MergeRoot@default"],"value_types":{},
        "capabilities":["json","model.choices.v1","xml"],"models":{
        "MergeRoot@default":{"lml":2,"model":"MergeRoot","register":"default",
            "custom_initialize":true,"attributes":[attr("title"),attr("body")],
            "choices":{"version":1,"validation":"explicit","groups":[
                {"min":1,"max":1,"format":null,"members":[{"attribute":"title"},{"attribute":"body"}]}]},
            "mappings":{
                "xml":{"root":{"element":"root"},"namespace":{"mode":"uri","uri":"urn:merge"},
                    "rules":[
                        {"kind":"element","name":["title"],"to":"title","namespace":{"mode":"unqualified"},
                            "custom":{"from":"read_title","to":"write_title"}},
                        {"kind":"element","name":["body"],"to":"body","namespace":{"mode":"inherit"},"cdata":true}]},
                "json":{"rules":[
                    {"key":["title"],"to":"title","custom":{"from":"read_title_json","to":"write_title_json"}},
                    {"key":["body"],"to":"body"}]}}}}});
    let run = support::run_consumer(
        "merge-interactions-consumer",
        &serde_json::to_vec(&bundle).unwrap(),
        &[("checks.rs", CONSUMER)],
        "#[cfg(test)] mod checks;\n",
        "\"xml\"",
        &Default::default(),
    );
    assert!(run.stdout.contains("3 passed"), "{}", run.stdout);
}

const CONSUMER: &str = r##"
use super::{contracts::ContractResult,models::*,xml_codecs::*,json_codecs::*};
struct Hooks;
impl MergeRootXmlHooks<usize> for Hooks {
    fn read_title(&self,m:&mut MergeRoot,v:&super::xml_rt::moxml_binding::XmlElement<'_>,s:&mut usize)->ContractResult<()> {
        m.title=Presence::Value(v.text()); *s+=1; Ok(())
    }
    fn write_title(&self,m:&MergeRoot,p:&mut super::xml_rt::moxml_binding::XmlParent,s:&mut usize)->ContractResult<()> {
        if let Presence::Value(t)=&m.title { p.add_element(super::xml_rt::moxml_binding::moxml::document::ElementSpec::new(super::xml_rt::moxml_binding::moxml::document::QName::local("title")).text(t)); } *s+=1; Ok(())
    }
}
impl MergeRootHooks<usize> for Hooks {
    fn read_title_json(&self,m:&mut MergeRoot,v:&serde_json::Value,s:&mut usize)->ContractResult<()> {
        m.title=Presence::Value(v.as_str().unwrap().into()); *s+=1; Ok(())
    }
    fn write_title_json(&self,m:&MergeRoot,d:&mut serde_json::Map<String,serde_json::Value>,s:&mut usize)->ContractResult<()> {
        if let Presence::Value(t)=&m.title { d.insert("title".into(),t.clone().into()); } *s+=1; Ok(())
    }
}
#[test]
fn xml_hooks_and_cdata_keep_deferred_choice_validation() {
    let mut calls=0usize;
    // The unqualified root is accepted as the model namespace; the hook runs before shape validation.
    let m=read_MergeRoot_xml_with(b"<root><title>T</title><body><![CDATA[B]]></body></root>",&Hooks,&mut calls).unwrap();
    assert_eq!(calls,1); assert_eq!(m.title,Presence::Value("T".into()));
    assert_eq!(m.body,Presence::Value("B".into())); assert!(m.validate().is_err());
    let out=write_MergeRoot_xml_with(&m,&Hooks,&mut calls).unwrap();
    assert_eq!(calls,2); assert!(out.contains("<![CDATA[B]]>"),"{out}");
}
#[test]
fn json_hooks_fill_choices_before_shape_validation() {
    let mut calls=0usize;
    let m=read_MergeRoot_json_with(r#"{"title":"T","body":"B"}"#,&Hooks,&mut calls).unwrap();
    assert_eq!(calls,1); assert!(m.validate().is_err());
    let wire=write_MergeRoot_json_with(&m,&Hooks,&mut calls).unwrap();
    assert_eq!(calls,2); assert_eq!(serde_json::from_str::<serde_json::Value>(&wire).unwrap(),serde_json::json!({"title":"T","body":"B"}));
}
#[test]
fn initializer_stub_and_explicit_initializer_keep_full_choice_validation() {
    assert_eq!(MergeRoot::construct(()).unwrap_err().reason,"custom_initialize");
    let m=MergeRoot::construct_with((),|_|{let mut m=MergeRoot::default();m.title=Presence::Value("T".into());Ok(m)}).unwrap();
    assert!(m.validate().is_ok());
    assert!(MergeRoot::construct_with((),|_|Ok(MergeRoot::default())).is_err());
}
"##;
