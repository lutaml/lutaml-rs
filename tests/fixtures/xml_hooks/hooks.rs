//! HANDWRITTEN XML hooks for the shared tiny fixture (SymbolLike content hook, Mixed element hooks incl. a
//! hook-only rule). Not compiler output; nothing is evaluated from the document.
use super::contracts::ContractResult;
use super::models::*;
use super::xml_codecs::*;
use super::xml_rt::moxml_binding::moxml::document::{ElementSpec, QName};
use super::xml_rt::moxml_binding::{XmlElement, XmlParent};

const NS: &str = "urn:lml:xmlhooks";

/// Application state for values that live only in hooks (the hook-only `extra` element).
#[derive(Default, Debug)]
pub struct State {
    pub extra: Option<String>,
    pub calls: Vec<&'static str>,
}

pub struct Real;
impl LmlXmlHookFixtures_MixedXmlHooks<State> for Real {
    // A declared member filled by a hook BEFORE validation.
    fn title_from_xml(&self, model: &mut LmlXmlHookFixtures_Mixed, value: &XmlElement<'_>, state: &mut State) -> ContractResult<()> {
        state.calls.push("title_from_xml");
        model.title = Presence::Value(value.text());
        Ok(())
    }
    fn title_to_xml(&self, model: &LmlXmlHookFixtures_Mixed, parent: &mut XmlParent, state: &mut State) -> ContractResult<()> {
        state.calls.push("title_to_xml");
        if let Presence::Value(title) = &model.title {
            // `unqualified` in the mapping: no namespace.
            parent.add_element(ElementSpec::new(QName::local("title")).text(title));
        }
        Ok(())
    }
    fn extra_from_xml(&self, _model: &mut LmlXmlHookFixtures_Mixed, value: &XmlElement<'_>, state: &mut State) -> ContractResult<()> {
        state.calls.push("extra_from_xml");
        state.extra = Some(value.text());
        Ok(())
    }
    fn extra_to_xml(&self, _model: &LmlXmlHookFixtures_Mixed, parent: &mut XmlParent, state: &mut State) -> ContractResult<()> {
        state.calls.push("extra_to_xml");
        if let Some(extra) = &state.extra {
            parent.add_element(ElementSpec::new(QName::ns(Some("h"), "extra", NS)).text(extra));
        }
        Ok(())
    }
}
impl LmlXmlHookFixtures_SymbolLikeXmlHooks<State> for Real {
    // Source parity: an explicit read no-op.
    fn content_from_xml(&self, _model: &mut LmlXmlHookFixtures_SymbolLike, _value: &str, state: &mut State) -> ContractResult<()> {
        state.calls.push("content_from_xml");
        Ok(())
    }
    // Raw markup, never escaped.
    fn content_to_xml(&self, model: &LmlXmlHookFixtures_SymbolLike, parent: &mut XmlParent, state: &mut State) -> ContractResult<()> {
        state.calls.push("content_to_xml");
        if let Presence::Value(markup) = &model.content {
            parent.add_xml_fragment(markup)?;
        }
        Ok(())
    }
}
