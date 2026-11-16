//! Deterministic artifacts with caller-pinned manifest identity.
use crate::{codecs::RustJsonEmitter, contracts::*, generator::RustEmitter, lml::Snapshot};
use serde::Deserialize;
use sha2::{Digest, Sha256};
use std::{
    collections::BTreeSet,
    error::Error,
    fs, io,
    path::{Component, Path, PathBuf},
};

pub type ArtifactResult<T> = Result<T, Box<dyn Error>>;
pub const MANIFEST_FILE: &str = "manifest.json";
pub const GENERATOR_VERSION: &str = concat!("lutaml-rs/", env!("CARGO_PKG_VERSION"));

pub fn digest(bytes: &[u8]) -> String {
    format!("sha256:{:x}", Sha256::digest(bytes))
}
fn invalid(detail: &str) -> Box<dyn Error> {
    io::Error::new(io::ErrorKind::InvalidData, detail).into()
}

#[derive(Deserialize)]
struct CorpusIndex {
    corpus: u32,
    generation: String,
    cases: Vec<CorpusCase>,
}
#[derive(Deserialize)]
struct CorpusCase {
    id: String,
    lml_checksum: String,
}

/// Generated files and their independently hashable canonical manifest.
pub struct Artifacts {
    pub files: SourceMap,
    pub manifest: Manifest,
}
impl Artifacts {
    pub fn generate(snapshot: &Snapshot) -> ArtifactResult<Self> {
        Self::generate_with(snapshot, &TypeBindings::new())
    }
    pub fn generate_with(snapshot: &Snapshot, bindings: &TypeBindings) -> ArtifactResult<Self> {
        let bundle = snapshot.bundle()?;
        let mut files = RustEmitter.generate_models(&bundle, bindings)?;
        for (path, source) in RustEmitter
            .generate_parg_bindings(&bundle, &[])?
            .into_iter()
            .chain(RustJsonEmitter.generate_json_codecs(&bundle, bindings)?)
        {
            if files.insert(path, source).is_some() {
                return Err(invalid("duplicate generated path"));
            }
        }
        #[allow(unused_mut)]
        let mut module = String::from("pub use lutaml_rs::{contracts, values};\npub mod models;\npub mod parg_bindings;\npub mod json_codecs;\n");
        // Source XML declarations alone do not imply an XML implementation in the
        // generated consumer: the codecs are emitted only where this build has them.
        #[allow(unused_mut)]
        let mut capabilities: Vec<String> = vec!["json".into()];
        #[cfg(feature = "xml")]
        if bundle.models.values().any(|m| m.mappings.xml.is_some()) {
            use crate::contracts::XmlCodecEmitter;
            files.extend(crate::xml::RustXmlEmitter.generate_xml_codecs(&bundle, bindings)?);
            module.push_str("pub use lutaml_rs::xml as xml_rt;\npub mod xml_codecs;\n");
            capabilities.push("xml".into());
        }
        files.insert("mod.rs".into(), module);
        let manifest = Manifest {
            manifest: 1,
            lml_checksum: snapshot.checksum().into(),
            schema_version: 2,
            generator_version: format!(
                "{GENERATOR_VERSION}/{}",
                digest(
                    &[
                        include_bytes!("../generator/mod.rs").as_slice(),
                        include_bytes!("../generator/runtime.rs.txt").as_slice(),
                        include_bytes!("../codecs/mod.rs").as_slice(),
                        include_bytes!("../codecs/runtime.rs.txt").as_slice(),
                        #[cfg(feature = "xml")]
                        include_bytes!("../xml/emitter.rs").as_slice(),
                        #[cfg(feature = "xml")]
                        include_bytes!("../xml/reader_emitter.rs").as_slice(),
                        #[cfg(feature = "xml")]
                        include_bytes!("../xml/writer_emitter.rs").as_slice(),
                        #[cfg(feature = "xml")]
                        include_bytes!("../xml/projection.rs").as_slice(),
                    ]
                    .concat()
                )
            ),
            target: Target::Rust,
            capabilities,
            corpora: vec![],
            files: files
                .iter()
                .map(|(path, source)| FilePin {
                    path: path.clone(),
                    sha256: digest(source.as_bytes()),
                })
                .collect(),
            dependencies: vec![DependencyPin {
                name: "lutaml-rs".into(),
                revision: env!("CARGO_PKG_VERSION").into(),
                sha256: digest(
                    &[
                        include_bytes!("../../Cargo.lock").as_slice(),
                        include_bytes!("../contracts.rs").as_slice(),
                        include_bytes!("../values/mod.rs").as_slice(),
                    ]
                    .concat(),
                ),
            }],
        };
        Ok(Self { files, manifest })
    }
    /// Pin an index's actual bytes and source identity, without claiming replay
    /// or coverage of any indexed case. The digest must come from the caller.
    pub fn with_corpus(mut self, bytes: &[u8], expected_digest: &str) -> ArtifactResult<Self> {
        let actual_digest = digest(bytes);
        if actual_digest != expected_digest {
            return Err(invalid("corpus index checksum mismatch"));
        }
        let index: CorpusIndex = serde_json::from_slice(bytes)?;
        if index.corpus != 1 {
            return Err(invalid("unsupported corpus index version"));
        }
        if index.generation.trim().is_empty() || index.cases.is_empty() {
            return Err(invalid("corpus generation and cases must be nonempty"));
        }
        let mut ids = BTreeSet::new();
        for case in index.cases {
            if case.id.trim().is_empty() || !ids.insert(case.id) {
                return Err(invalid("corpus case IDs must be nonempty and unique"));
            }
            if case.lml_checksum != self.manifest.lml_checksum {
                return Err(invalid("corpus case source checksum mismatch"));
            }
        }
        self.manifest.corpora = vec![CorpusPin {
            generation: index.generation,
            sha256: actual_digest,
        }];
        Ok(self)
    }
    pub fn manifest_bytes(&self) -> ArtifactResult<Vec<u8>> {
        canonical_manifest(&self.manifest)
    }
    pub fn manifest_digest(&self) -> ArtifactResult<String> {
        Ok(digest(&self.manifest_bytes()?))
    }
    pub fn write(&self, root: &Path) -> ArtifactResult<()> {
        fs::create_dir_all(root)?;
        for (name, source) in &self.files {
            fs::write(safe_path(root, name)?, source)?;
        }
        fs::write(safe_path(root, MANIFEST_FILE)?, self.manifest_bytes()?)?;
        Ok(())
    }
    /// Verify caller identity, all pins, safe paths, and actual bytes. A supplied
    /// manifest cannot expand the set of files authorized by this generation.
    pub fn verify(&self, root: &Path, expected_digest: &str) -> ArtifactResult<()> {
        let bytes = fs::read(safe_path(root, MANIFEST_FILE)?)?;
        let supplied: Manifest = serde_json::from_slice(&bytes)?;
        if bytes != canonical_manifest(&supplied)?
            || digest(&bytes) != expected_digest
            || self.manifest_digest()? != expected_digest
        {
            return Err(invalid("manifest checksum or pinned generation mismatch"));
        }
        validate_paths(&supplied.files)?;
        if supplied != self.manifest {
            return Err(invalid("manifest pins differ from expected generation"));
        }
        for pin in &supplied.files {
            if digest(&fs::read(safe_path(root, &pin.path)?)?) != pin.sha256 {
                return Err(invalid("artifact checksum mismatch"));
            }
        }
        let mut pinned: BTreeSet<&str> = supplied.files.iter().map(|p| p.path.as_str()).collect();
        pinned.insert(MANIFEST_FILE);
        let mut found = Vec::new();
        list_files(root, root, &mut found)?;
        if let Some(extra) = found.iter().find(|f| !pinned.contains(f.as_str())) {
            return Err(invalid(&format!(
                "unpinned file in artifact directory: {extra}"
            )));
        }
        Ok(())
    }
}
/// Every entry under `root` as a `/`-joined relative path; symlinks are listed,
/// never followed, so they surface as unpinned files.
fn list_files(root: &Path, dir: &Path, out: &mut Vec<String>) -> ArtifactResult<()> {
    for entry in fs::read_dir(dir)? {
        let entry = entry?;
        let path = entry.path();
        if entry.file_type()?.is_dir() {
            list_files(root, &path, out)?;
        } else {
            let rel = path.strip_prefix(root)?;
            out.push(
                rel.components()
                    .map(|c| c.as_os_str().to_string_lossy())
                    .collect::<Vec<_>>()
                    .join("/"),
            );
        }
    }
    Ok(())
}
fn canonical_manifest(manifest: &Manifest) -> ArtifactResult<Vec<u8>> {
    // serde_json's default Map sorts keys, including nested manifest records.
    Ok(serde_json::to_vec(&serde_json::to_value(manifest)?)?)
}
fn validate_paths(pins: &[FilePin]) -> ArtifactResult<()> {
    let mut names = BTreeSet::new();
    for pin in pins {
        relative(&pin.path)?;
        if !names.insert(&pin.path) {
            return Err(invalid("duplicate artifact path"));
        }
    }
    Ok(())
}
fn relative(name: &str) -> ArtifactResult<()> {
    if name.is_empty()
        || name.contains('\\')
        || name.contains(':')
        || !Path::new(name)
            .components()
            .all(|c| matches!(c, Component::Normal(_)))
    {
        return Err(invalid("artifact path must be relative without traversal"));
    }
    Ok(())
}
fn safe_path(root: &Path, name: &str) -> ArtifactResult<PathBuf> {
    relative(name)?;
    // Reject symlinks at every existing component, including the artifact root.
    let mut path = root.to_path_buf();
    for part in std::iter::once(None).chain(Path::new(name).components().map(Some)) {
        if let Some(part) = part {
            path.push(part.as_os_str());
        }
        match fs::symlink_metadata(&path) {
            Ok(meta) if meta.file_type().is_symlink() => {
                return Err(invalid("artifact symlink is forbidden"))
            }
            Ok(_) => {}
            Err(e) if e.kind() == io::ErrorKind::NotFound => {}
            Err(e) => return Err(e.into()),
        }
    }
    Ok(path)
}
