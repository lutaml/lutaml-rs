//! HANDWRITTEN counterpart of the one XML hook in the real Unit graph, `Units::Symbol`'s content rule
//! `{to: content_to_xml, from: content_from_xml}` (unitsml-ruby ee09d6d7, lib/unitsml/model/units/symbol.rb).
//! Source behavior: the read callback is an explicit no-op; the write callback inserts the raw fragment.
use super::contracts::ContractResult;
use super::models::*;
use super::xml_codecs::*;
use super::xml_rt::moxml_binding::XmlParent;

pub struct UnitHooks;
impl Unitsml_Model_Units_SymbolXmlHooks<()> for UnitHooks {
    // `def content_from_xml(**); end`
    fn content_from_xml(&self, _model: &mut Unitsml_Model_Units_Symbol, _value: &str, _state: &mut ()) -> ContractResult<()> {
        Ok(())
    }
    // `doc.add_xml_fragment(parent, model.content)`; a nil content inserts nothing.
    fn content_to_xml(&self, model: &Unitsml_Model_Units_Symbol, parent: &mut XmlParent, _state: &mut ()) -> ContractResult<()> {
        if let Presence::Value(markup) = &model.content {
            parent.add_xml_fragment(markup)?;
        }
        Ok(())
    }
}
