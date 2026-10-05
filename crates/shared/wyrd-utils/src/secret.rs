//! The one reader for environment and mounted-file secret references.
//!
//! Every Wyrd process that resolves a [`SecretRef`] itself — gateway
//! credential bindings, the server's tenant wrapping keys, and local Workflow
//! gateway bindings — reads it here, so all share one rule: a file is checked
//! through metadata of the already-open handle, so no path swap can slip
//! between check and read, and must be a regular file that, on Unix, has no
//! group or other permission bits, holding at most [`MAX_SECRET_FILE_BYTES`].
//! Reads are synchronous; async callers run them on the blocking pool.

use std::fs::{File, Metadata};
use std::io::Read as _;
#[cfg(unix)]
use std::os::unix::fs::PermissionsExt as _;
use std::path::Path;

use secrecy::SecretString;
use wyrd_spec::security::SecretRef;

/// Largest mounted secret file read, in bytes; longer files fail closed.
pub const MAX_SECRET_FILE_BYTES: u64 = 64 * 1024;

/// Read an environment-variable or mounted-file secret reference.
///
/// # Errors
///
/// Returns a short static reason — an unset variable, an unopenable,
/// non-regular, permissive, or oversized file, or a reference kind other than
/// `env` or `file` —
/// naming no variable, path, or content, so a caller can render it verbatim
/// in a redacted error.
pub fn read_secret_ref(secret: &SecretRef) -> Result<SecretString, &'static str> {
    match secret {
        SecretRef::Env { name } => std::env::var(name)
            .map(SecretString::from)
            .map_err(|_| "names an unset environment variable"),
        SecretRef::File { path } => read_secret_file(Path::new(path)).map(SecretString::from),
        _ => Err("must be an env or file secret reference"),
    }
}

/// Read a mounted secret file under the owner-only, bounded rule.
///
/// # Errors
///
/// Returns a short static reason — unopenable, non-regular or permissive, or
/// oversized — naming no path and no content.
fn read_secret_file(path: &Path) -> Result<String, &'static str> {
    let file = File::open(path).map_err(|_| "names an unreadable file")?;
    let metadata = file.metadata().map_err(|_| "names an unreadable file")?;
    if !restrictive(&metadata) {
        return Err("names a file that is not a regular owner-only file");
    }
    let mut value = String::new();
    let read = file
        .take(MAX_SECRET_FILE_BYTES + 1)
        .read_to_string(&mut value)
        .map_err(|_| "names an unreadable file")?;
    if read as u64 > MAX_SECRET_FILE_BYTES {
        return Err("names a file larger than one secret");
    }
    Ok(value)
}

/// Whether an opened secret file is regular and, on Unix, readable by its
/// owner only: the six low group and other mode bits are all clear.
#[cfg(unix)]
fn restrictive(metadata: &Metadata) -> bool {
    metadata.is_file() && metadata.permissions().mode().trailing_zeros() >= 6
}

/// Whether an opened secret file is regular; access restriction is a
/// deployment requirement on platforms without Unix mode bits.
#[cfg(not(unix))]
fn restrictive(metadata: &Metadata) -> bool {
    metadata.is_file()
}
