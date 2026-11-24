//! The one reader for environment and mounted-file secret references.
//!
//! Every Wyrd process that resolves a [`SecretRef`] itself — gateway
//! credential bindings, the server's signing key, and `[kms]` key files and
//! tokens — reads it here, and Google Cloud Storage and Cloud KMS resolve
//! Wyrd's inline Google service-account variables here,
//! so all share one rule: a file is checked
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
pub fn read_secret_file(path: &Path) -> Result<String, &'static str> {
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

/// Google service-account JSON from Wyrd's inline credential variables.
///
/// `GOOGLE_ACCOUNT_JSON_BASE64`, when readable, is decoded and is the only
/// input considered: a payload that is not base64 of UTF-8 yields `None`.
/// Otherwise `GOOGLE_APPLICATION_CREDENTIALS_JSON` is returned as is. `None`
/// leaves the provider's own chain (a credentials file or ambient identity)
/// in charge. Google Cloud Storage and Cloud KMS both select their identity
/// here, once, when their client is built.
#[must_use]
pub fn google_service_account_json() -> Option<String> {
    use base64::Engine as _;
    if let Ok(encoded) = std::env::var("GOOGLE_ACCOUNT_JSON_BASE64") {
        return base64::engine::general_purpose::STANDARD
            .decode(encoded)
            .ok()
            .and_then(|json| String::from_utf8(json).ok());
    }
    std::env::var("GOOGLE_APPLICATION_CREDENTIALS_JSON").ok()
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

#[cfg(test)]
mod tests {
    use base64::Engine as _;
    use base64::engine::general_purpose::STANDARD;

    use super::google_service_account_json;

    /// Resolve with exactly these two inline variables.
    fn resolve(encoded: Option<&str>, raw: Option<&str>) -> Option<String> {
        temp_env::with_vars(
            [
                ("GOOGLE_ACCOUNT_JSON_BASE64", encoded),
                ("GOOGLE_APPLICATION_CREDENTIALS_JSON", raw),
            ],
            google_service_account_json,
        )
    }

    /// The base64 variable is decoded, the raw variable is verbatim, the
    /// base64 one wins when both are set, and neither leaves the provider
    /// chain in charge.
    #[test]
    fn google_service_account_json_prefers_the_decoded_base64_variable() {
        let encoded_json = r#"{"client_email":"encoded@wyrd.test"}"#;
        let raw_json = r#"{"client_email":"raw@wyrd.test"}"#;
        let encoded = STANDARD.encode(encoded_json);

        assert_eq!(resolve(Some(&encoded), None).as_deref(), Some(encoded_json));
        assert_eq!(resolve(None, Some(raw_json)).as_deref(), Some(raw_json));
        assert_eq!(
            resolve(Some(&encoded), Some(raw_json)).as_deref(),
            Some(encoded_json)
        );
        assert_eq!(resolve(None, None), None);
    }

    /// A set base64 variable that is not base64, or decodes to non-UTF-8,
    /// resolves nothing and does not fall back to the raw variable.
    #[test]
    fn google_service_account_json_does_not_fall_back_past_a_malformed_base64_variable() {
        let raw_json = r#"{"client_email":"raw@wyrd.test"}"#;
        let not_utf8 = STANDARD.encode([0xff, 0xfe]);

        assert_eq!(resolve(Some("not base64!"), Some(raw_json)), None);
        assert_eq!(resolve(Some(&not_utf8), Some(raw_json)), None);
    }
}
