//! Filesystem loader and dumper for prompt builder values.

use std::path::Path;

use wyrd_spec::{CardLoadFormat, PromptSpec, parse_spec_bytes, serialize_spec_bytes};

use crate::error::{PromptBuilderError, PromptBuilderResult};
use crate::prompt::Prompt;

/// Load a bare `PromptSpec` file and return the prompt builder wrapper.
pub fn load_prompt(path: impl AsRef<Path>) -> PromptBuilderResult<Prompt> {
    let path = path.as_ref();
    let bytes = std::fs::read(path).map_err(|error| loader_io(path, &error))?;
    let format = format_from_path(path)?;
    let spec = parse_spec_bytes(format, &bytes)?;
    Ok(Prompt::from_native(spec.prompt))
}

/// Dump a prompt builder as a bare `PromptSpec` file.
pub fn dump_prompt(prompt: &Prompt, path: impl AsRef<Path>) -> PromptBuilderResult<()> {
    let path = path.as_ref();
    let format = format_from_path(path)?;
    let spec = PromptSpec::new(prompt.native().clone())?;
    let bytes = serialize_spec_bytes(format, &spec)?;
    std::fs::write(path, bytes).map_err(|error| loader_io(path, &error))
}

fn format_from_path(path: &Path) -> PromptBuilderResult<CardLoadFormat> {
    CardLoadFormat::from_extension(path.extension().and_then(std::ffi::OsStr::to_str))
        .map_err(Into::into)
}

fn loader_io(path: &Path, error: &std::io::Error) -> PromptBuilderError {
    PromptBuilderError::Io {
        path: path_string(path),
        message: error.to_string(),
    }
}

fn path_string(path: &Path) -> String {
    path.to_string_lossy().into_owned()
}
