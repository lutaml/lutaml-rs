// Compile-time PARG capture constructors; parsing and application behavior are external.
use super::models::*;
// capture "body" -> member "text"
pub fn bind_leaf(capture_0: String)->Result<Leaf,ValidationError>{ let mut value=Leaf{ text: Presence::Value(capture_0), ..Default::default() }; value.apply_defaults()?; value.validate()?; Ok(value) }
