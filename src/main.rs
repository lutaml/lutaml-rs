use lutaml_rs::{
    artifacts::Artifacts,
    contracts::{TypeBinding, TypeBindings},
    lml, InputFormat,
};
use std::{error::Error, path::PathBuf};
const HELP: &str = "lutaml-rs <generate|check> --input BUNDLE --output DIR [--format json|yaml] [--manifest-sha256 sha256:HEX] [--bindings FILE] [--corpus-index FILE --corpus-sha256 sha256:HEX]\n--bindings supplies trusted value-type bindings: {\"types\":{\"<ValueTypeId>\":{\"rust_type\":\"...\",\"codec\":null}}}.\ncheck requires a caller-pinned --manifest-sha256. generate prints this digest.\nCorpus options must be paired and pin index identity, without claiming replay.\nGenerated mod.rs uses lutaml-model runtime plus pinned serde and serde_json dependencies.";
fn load_bindings(path: &str) -> Result<TypeBindings, Box<dyn Error>> {
    let wire: serde_json::Value = serde_json::from_slice(&std::fs::read(path)?)?;
    let types = wire
        .get("types")
        .and_then(|t| t.as_object())
        .ok_or("bindings file must be {\"types\":{...}}")?;
    let mut bindings = TypeBindings::new();
    for (id, entry) in types {
        let rust_type = entry
            .get("rust_type")
            .and_then(|t| t.as_str())
            .filter(|t| {
                !t.trim().is_empty()
                    && t.chars()
                        .all(|c| c.is_ascii_alphanumeric() || "_:<>, ".contains(c))
            })
            .ok_or_else(|| format!("binding {id}: rust_type must be a non-empty Rust type path"))?;
        let codec = match entry.get("codec") {
            None | Some(serde_json::Value::Null) => None,
            Some(serde_json::Value::String(c)) => Some(c.clone()),
            Some(_) => return Err(format!("binding {id}: codec must be a string or null").into()),
        };
        bindings.insert(
            id.clone(),
            TypeBinding {
                rust_type: rust_type.into(),
                codec,
            },
        );
    }
    Ok(bindings)
}
fn run() -> Result<(), Box<dyn Error>> {
    let mut args = std::env::args().skip(1);
    let Some(command) = args.next() else {
        return Err(HELP.into());
    };
    if command == "--help" || command == "-h" {
        println!("{HELP}");
        return Ok(());
    }
    if command != "generate" && command != "check" {
        return Err("expected generate or check".into());
    }
    let (mut input, mut output, mut format, mut expected, mut corpus_index, mut corpus_pin) =
        (None, None, None, None, None, None);
    let mut bindings_path = None;
    while let Some(arg) = args.next() {
        let slot = match arg.as_str() {
            "--input" => &mut input,
            "--output" => &mut output,
            "--format" => &mut format,
            "--manifest-sha256" => &mut expected,
            "--bindings" => &mut bindings_path,
            "--corpus-index" => &mut corpus_index,
            "--corpus-sha256" => &mut corpus_pin,
            _ => return Err(format!("unknown option: {arg}").into()),
        };
        if slot.is_some() {
            return Err(format!("duplicate option: {arg}").into());
        }
        *slot = Some(args.next().ok_or("option requires a value")?);
    }
    let corpus = match (corpus_index, corpus_pin) {
        (Some(index), Some(pin)) => Some((std::fs::read(index)?, pin)),
        (None, None) => None,
        _ => return Err("--corpus-index and --corpus-sha256 must be supplied together".into()),
    };
    let input = PathBuf::from(input.ok_or("--input is required")?);
    let output = PathBuf::from(output.ok_or("--output is required")?);
    let format = match format
        .as_deref()
        .or_else(|| input.extension().and_then(|s| s.to_str()))
    {
        Some("json") => InputFormat::Json,
        Some("yaml" | "yml") => InputFormat::Yaml,
        _ => return Err("input format must be json or yaml".into()),
    };
    let snapshot = lml::load(&std::fs::read(input)?, format)?;
    let bindings = match bindings_path {
        Some(path) => load_bindings(&path)?,
        None => TypeBindings::new(),
    };
    let artifacts = Artifacts::generate_with(&snapshot, &bindings)?;
    let artifacts = if let Some((bytes, pin)) = corpus {
        artifacts.with_corpus(&bytes, &pin)?
    } else {
        artifacts
    };
    if command == "check" {
        artifacts.verify(
            &output,
            &expected.ok_or("check requires --manifest-sha256")?,
        )?;
    } else {
        if let Some(expected) = expected {
            if artifacts.manifest_digest()? != expected {
                return Err("expected manifest checksum mismatch".into());
            }
        }
        artifacts.write(&output)?;
    }
    println!("{}", artifacts.manifest_digest()?);
    Ok(())
}
fn main() -> std::process::ExitCode {
    match run() {
        Ok(()) => std::process::ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("{error}");
            std::process::ExitCode::FAILURE
        }
    }
}
