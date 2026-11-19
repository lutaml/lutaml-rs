use lutaml_rs::{
    contracts::{InputFormat, TypeBindings, XmlCodecEmitter},
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
    let bindings = TypeBindings::new();
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
