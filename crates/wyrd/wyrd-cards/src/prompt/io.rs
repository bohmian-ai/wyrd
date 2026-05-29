//! Filesystem IO helpers for local `PromptCard` materialization.

use std::path::Path;

use wyrd_spec::card::prompt::validate::PromptError;
use wyrd_spec::card::prompt::{
    CardLoadFormat, PromptSpec, parse_card_bytes, parse_spec_bytes, serialize_card,
};
use wyrd_spec::envelope::Card;
use wyrd_spec::error::WyrdError;

/// Read a full Prompt Card envelope from a local JSON or YAML file.
///
/// # Errors
/// Returns a Wyrd prompt loader error when the extension is unsupported, the
/// file cannot be read, or the bytes do not decode as a Prompt Card envelope.
pub fn read_card_file(path: &Path) -> Result<Card, WyrdError> {
    let format = format_from_path(path)?;
    let bytes = std::fs::read(path).map_err(|error| loader_io(path, &error))?;
    parse_card_bytes(format, &bytes)
}

/// Write a full Prompt Card envelope to a local JSON or YAML file.
///
/// Parent directories are created when needed. The file format is selected from
/// the target extension.
///
/// # Errors
/// Returns a Wyrd prompt loader error when the extension is unsupported,
/// serialization fails, or the file cannot be written.
pub fn write_card_file(card: &Card, path: &Path) -> Result<(), WyrdError> {
    let format = format_from_path(path)?;
    let bytes = serialize_card(format, card)?;
    if let Some(parent) = path
        .parent()
        .filter(|parent| !parent.as_os_str().is_empty())
    {
        std::fs::create_dir_all(parent).map_err(|error| loader_io(parent, &error))?;
    }
    std::fs::write(path, bytes).map_err(|error| loader_io(path, &error))
}

/// Read a bare `PromptSpec` body from a local JSON or YAML file.
///
/// # Errors
/// Returns a Wyrd prompt loader error when the extension is unsupported, the
/// file cannot be read, or the bytes do not decode as a `PromptSpec`.
pub fn read_spec_file(path: &Path) -> Result<PromptSpec, WyrdError> {
    let format = format_from_path(path)?;
    let bytes = std::fs::read(path).map_err(|error| loader_io(path, &error))?;
    parse_spec_bytes(format, &bytes)
}

fn format_from_path(path: &Path) -> Result<CardLoadFormat, WyrdError> {
    CardLoadFormat::from_extension(path.extension().and_then(std::ffi::OsStr::to_str))
}

fn loader_io(path: &Path, error: &std::io::Error) -> WyrdError {
    PromptError::LoaderIo {
        path: path.display().to_string(),
        message: error.to_string(),
    }
    .into()
}
