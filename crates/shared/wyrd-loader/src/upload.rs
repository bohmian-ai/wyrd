//! One file or directory published as an Artifact Card, with optional
//! extraction outputs prepared beside its originals before registration.

use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};

use base64::Engine as _;
use sha2::{Digest, Sha256};
use wyrd_spec::MediaFormat;
use wyrd_spec::card::artifact::ArtifactSpec;
use wyrd_spec::card::common::NonSecretValue;
use wyrd_spec::envelope::{CardKind, Metadata};
use wyrd_spec::error::WyrdError;
use wyrd_spec::registry::{ArtifactManifestEntry, CardSubmission, RelativeArtifactPath};

use crate::manifest::{NONREGULAR, collect_files, manifest_of, nonregular};
use crate::{DeclarationInput, Diagnostic, EXTRACT_VERSION, ExtractLimits, Extraction, LoadError};

/// A [`WyrdError::Validation`] of the upload `source` under `rule`, whose
/// `details` add `context`'s fields to the rule and field.
fn invalid(rule: &str, message: String, context: serde_json::Value) -> WyrdError {
    let mut details = serde_json::json!({ "rule": rule, "field": "source" });
    if let (Some(details), serde_json::Value::Object(context)) = (details.as_object_mut(), context)
    {
        details.extend(context);
    }
    WyrdError::Validation {
        message: format!("source: {message}"),
        details,
    }
}

/// The directory extraction outputs are written under, inside the Artifact.
const EXTRACTED: &str = "extracted";

/// A prepared Artifact upload: one Artifact submission whose manifest holds
/// exactly the supplied files, plus any extraction outputs, ready for the
/// ordinary declaration lifecycle.
///
/// A file keeps its basename; a directory keeps every regular file at its
/// relative path, envelope-like names included. With extraction, each
/// supported source `P` gains `extracted/P.json`, written to a private
/// temporary directory that lives as long as this value, so drop it only
/// after the declaration has transferred the outputs. The source tree is
/// never written.
#[derive(Debug)]
pub struct ArtifactUpload {
    /// The declaration input: the one submission and its local sources.
    pub input: DeclarationInput,
    /// Each extracted `(source, output)` Artifact path pair, in source order.
    pub extracted: Vec<(String, String)>,
    /// Source paths no extractor supports, published unextracted.
    pub skipped: Vec<String>,
    /// Holds the prepared outputs until the upload is dropped.
    scratch: Option<tempfile::TempDir>,
}

impl ArtifactUpload {
    /// Prepare `source` as Artifact `metadata` with `spec`, extracting
    /// supported documents first when `extract` is set.
    ///
    /// Extraction reads each supported source once, refuses it when its
    /// bytes no longer match the manifest digest taken moments before, and
    /// records `spec.metadata.extraction` as `{version, files: [{source,
    /// output}], skipped}`. The declaration lifecycle re-hashes every
    /// source before submission, so a later change fails the upload too.
    ///
    /// # Errors
    /// Returns [`WyrdError::Validation`] for a blank `artifact_kind`, with
    /// `extract` a caller-supplied `metadata.extraction` (rule
    /// `extraction_report`), and, with `field` `source`, a source that is
    /// not a regular file or a directory of regular files
    /// (`artifact_source_regular`), an empty directory
    /// (`artifact_source_empty`), a supported document the extractor refuses
    /// (`extraction_refused`), or an output path that collides with a source
    /// (`extraction_output_collision`); `details` name the offending path
    /// and what was expected. Returns [`WyrdError::LoaderInvalidEnvelope`]
    /// carrying an IO diagnostic for an unreadable file, and one for a file
    /// that changed while being prepared.
    pub fn prepare(
        source: &Path,
        metadata: Metadata,
        mut spec: ArtifactSpec,
        extract: bool,
    ) -> Result<Self, WyrdError> {
        spec.validate()?;
        if extract && spec.metadata.contains_key("extraction") {
            return Err(WyrdError::Validation {
                message: "spec.metadata.extraction: is written by extraction; remove it or upload without extract".to_owned(),
                details: serde_json::json!({ "rule": "extraction_report", "path": "spec.metadata.extraction" }),
            });
        }
        let refused = |message: &str| {
            WyrdError::from(LoadError::single(Diagnostic::invalid_envelope(
                source.to_path_buf(),
                message.to_owned(),
            )))
        };
        let kind = fs::symlink_metadata(source)
            .map_err(|error| LoadError::single(Diagnostic::io(source.to_path_buf(), &error)))?;
        let (root, files) = if kind.is_file() {
            (
                source.parent().unwrap_or_else(|| Path::new("")),
                vec![source.to_path_buf()],
            )
        } else if kind.is_dir() {
            let mut files = Vec::new();
            collect_files(source, &mut files).map_err(|error| {
                match error.diagnostics.as_slice() {
                    [entry]
                        if entry
                            .context
                            .as_ref()
                            .is_some_and(|context| context["rule"] == NONREGULAR) =>
                    {
                        nonregular(&entry.path)
                    }
                    _ => WyrdError::from(error),
                }
            })?;
            (source, files)
        } else {
            return Err(nonregular(source));
        };
        if files.is_empty() {
            return Err(invalid(
                "artifact_source_empty",
                format!("{} holds no regular files to upload", source.display()),
                serde_json::json!({
                    "source": source.display().to_string(),
                    "expected": "at least one regular file",
                }),
            ));
        }
        let mut manifest = manifest_of(root, files)?;
        let mut upload = Self {
            input: DeclarationInput {
                submissions: Vec::new(),
                artifact_sources: BTreeMap::new(),
            },
            extracted: Vec::new(),
            skipped: Vec::new(),
            scratch: None,
        };
        if extract {
            let scratch = upload
                .scratch
                .insert(tempfile::tempdir().map_err(|error| {
                    LoadError::single(Diagnostic::io(std::env::temp_dir(), &error))
                })?)
                .path()
                .to_path_buf();
            let originals: Vec<String> = manifest
                .entries
                .iter()
                .map(|entry| entry.relative_path.as_str().to_owned())
                .collect();
            for original in manifest.entries.clone() {
                let path = original.relative_path.as_str().to_owned();
                let local = manifest.sources[&original.relative_path].clone();
                let format = path
                    .rsplit_once('.')
                    .and_then(|(_, extension)| extension.parse::<MediaFormat>().ok())
                    .filter(|format| format.is_extractable());
                let Some(format) = format else {
                    upload.skipped.push(path);
                    continue;
                };
                let output = format!("{EXTRACTED}/{path}.json");
                let collides = |other: &String| {
                    *other == output
                        || other.starts_with(&format!("{output}/"))
                        || output.starts_with(&format!("{other}/"))
                };
                if let Some(other) = originals.iter().find(|other| collides(other)) {
                    return Err(invalid(
                        "extraction_output_collision",
                        format!(
                            "the extraction output {output} collides with the uploaded {other}"
                        ),
                        serde_json::json!({
                            "source": path,
                            "output": output,
                            "collides_with": other,
                            "expected": format!("no uploaded file at or around {output}"),
                        }),
                    ));
                }
                let bytes = fs::read(&local)
                    .map_err(|error| LoadError::single(Diagnostic::io(local.clone(), &error)))?;
                if Some(digest(&bytes)) != original.sha256 {
                    return Err(refused(&format!(
                        "{path} changed while it was being prepared"
                    )));
                }
                let extraction = Extraction::new(&ExtractLimits::default())
                    .run(&bytes, format)
                    .map_err(|error| {
                        invalid(
                            "extraction_refused",
                            format!("{path} cannot be extracted: {error}"),
                            serde_json::json!({
                                "source": path,
                                "format": format,
                                "expected": format!(
                                    "a parseable {format} document within the extraction limits"
                                ),
                                "reason": error.0,
                            }),
                        )
                    })?;
                let json = serde_json::to_vec_pretty(&extraction)
                    .map_err(|error| refused(&format!("{path} extraction: {error}")))?;
                let written = scratch.join(&output);
                write(&written, &json)?;
                let relative = RelativeArtifactPath::new(&output)
                    .map_err(|error| refused(&format!("invalid output path {output}: {error}")))?;
                manifest.entries.push(ArtifactManifestEntry {
                    relative_path: relative.clone(),
                    sha256: Some(digest(&json)),
                    size_bytes: Some(json.len() as u64),
                    content_type: None,
                });
                manifest.sources.insert(relative, written);
                upload.extracted.push((path, output));
            }
            manifest.entries.sort_by(|left, right| {
                left.relative_path
                    .as_str()
                    .cmp(right.relative_path.as_str())
            });
            spec.metadata
                .insert("extraction".to_owned(), upload.report());
        }
        let spec = serde_json::to_value(&spec)
            .map_err(|error| refused(&format!("the Artifact spec does not serialize: {error}")))?;
        upload.input.submissions.push(CardSubmission {
            api_version: wyrd_spec::api_version::ApiVersion::v1(),
            kind: CardKind::Artifact,
            metadata,
            spec,
            artifacts: manifest.entries,
        });
        upload.input.artifact_sources = manifest.sources;
        Ok(upload)
    }

    /// The `metadata.extraction` report of this upload.
    fn report(&self) -> NonSecretValue {
        let text = |value: &str| NonSecretValue::Str(value.to_owned());
        NonSecretValue::Object(BTreeMap::from([
            ("version".to_owned(), text(EXTRACT_VERSION)),
            (
                "files".to_owned(),
                NonSecretValue::List(
                    self.extracted
                        .iter()
                        .map(|(source, output)| {
                            NonSecretValue::Object(BTreeMap::from([
                                ("source".to_owned(), text(source)),
                                ("output".to_owned(), text(output)),
                            ]))
                        })
                        .collect(),
                ),
            ),
            (
                "skipped".to_owned(),
                NonSecretValue::List(self.skipped.iter().map(|path| text(path)).collect()),
            ),
        ]))
    }
}

/// The base64 SHA-256 of `bytes`, the manifest's digest form.
fn digest(bytes: &[u8]) -> String {
    base64::engine::general_purpose::STANDARD.encode(Sha256::digest(bytes))
}

/// Write `bytes` to new file `path`, creating its directories.
///
/// # Errors
/// Returns an IO diagnostic when a directory or the file cannot be written.
fn write(path: &PathBuf, bytes: &[u8]) -> Result<(), LoadError> {
    let failed = |error: std::io::Error| LoadError::single(Diagnostic::io(path.clone(), &error));
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(failed)?;
    }
    fs::write(path, bytes).map_err(failed)
}

#[cfg(test)]
mod tests;
