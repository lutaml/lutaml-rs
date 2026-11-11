use lutaml_rs::{
    contracts::{InputFormat, TypeBinding, TypeBindings, XmlCodecEmitter},
    generator::RustEmitter,
    lml,
    xml::RustXmlEmitter,
};
use std::{error::Error, path::PathBuf};

fn main() -> Result<(), Box<dyn Error>> {
    let args: Vec<_> = std::env::args_os().skip(1).collect();
    let [input, output] = args.as_slice() else {
        return Err("usage: mml-xml-codegen BUNDLE OUTPUT".into());
    };
    let snapshot = lml::load(&std::fs::read(input)?, InputFormat::Json)?;
    let bundle = snapshot.bundle()?;
    let mut bindings = TypeBindings::new();
    bindings.insert("Omml::Models::STCoordinate@omml".into(), TypeBinding { rust_type: "crate::bindings::coordinate::st_coordinate::STCoordinate".into(), codec: Some("xml:crate::bindings::coordinate::st_coordinate".into()) });
    bindings.insert("Omml::Models::STPositiveCoordinate@omml".into(), TypeBinding { rust_type: "crate::bindings::coordinate::st_positive_coordinate::STPositiveCoordinate".into(), codec: Some("xml:crate::bindings::coordinate::st_positive_coordinate".into()) });
    bindings.insert("Omml::Models::STInteger2@omml".into(), TypeBinding { rust_type: "crate::bindings::integer::st_integer2::STInteger2".into(), codec: Some("xml:crate::bindings::integer::st_integer2".into()) });
    bindings.insert("Omml::Models::STInteger255@omml".into(), TypeBinding { rust_type: "crate::bindings::integer::st_integer255::STInteger255".into(), codec: Some("xml:crate::bindings::integer::st_integer255".into()) });
    bindings.insert("Omml::Models::STSpacingRule@omml".into(), TypeBinding { rust_type: "crate::bindings::integer::st_spacing_rule::STSpacingRule".into(), codec: Some("xml:crate::bindings::integer::st_spacing_rule".into()) });
    bindings.insert("Omml::Models::STTextScale@omml".into(), TypeBinding { rust_type: "crate::bindings::integer::st_text_scale::STTextScale".into(), codec: Some("xml:crate::bindings::integer::st_text_scale".into()) });
    bindings.insert("Omml::Models::UnsignedInt@omml".into(), TypeBinding { rust_type: "crate::bindings::integer::unsigned_int::UnsignedInt".into(), codec: Some("xml:crate::bindings::integer::unsigned_int".into()) });
    bindings.insert("Omml::Models::UnsignedLong@omml".into(), TypeBinding { rust_type: "crate::bindings::integer::unsigned_long::UnsignedLong".into(), codec: Some("xml:crate::bindings::integer::unsigned_long".into()) });
    bindings.insert("Omml::Models::Token@omml".into(), TypeBinding { rust_type: "crate::bindings::token::Token".into(), codec: Some("xml:crate::bindings::token_codec".into()) });
    for name in ["HexBinary", "STLang"] {
        bindings.insert(format!("Omml::Models::{name}@omml"), TypeBinding {
            rust_type: "String".into(), codec: None,
        });
    }
    bindings.insert("builtin:date_time".into(), TypeBinding {
        rust_type: "crate::bindings::date_time::DateTimeValue".into(),
        codec: Some("xml:crate::bindings::date_time".into()),
    });
    let files = RustEmitter.generate_xml_models(&bundle, &bindings)?;
    let codecs = RustXmlEmitter.generate_xml_codecs(&bundle, &bindings)?;
    let output = PathBuf::from(output);
    std::fs::create_dir_all(&output)?;
    for (name, source) in files.into_iter().chain(codecs) {
        std::fs::write(output.join(name), source)?;
    }
    println!("{}", snapshot.checksum());
    Ok(())
}
