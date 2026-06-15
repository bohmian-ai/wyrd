//! Tenant-scoped object path validation.

use crate::error::StorageError;
use std::sync::LazyLock;
use url::Url;
use wyrd_spec::DataTenantId;

/// Full tenant path regex.
static FULL_PATH_RE: LazyLock<regex::Regex> = LazyLock::new(|| {
    regex::Regex::new(concat!(
        r"^(?P<tenant>[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12})",
        r"/cards/(?P<card>[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12})",
        r"/(?P<rel>[A-Za-z0-9._\-]+(?:/[A-Za-z0-9._\-]+)*)$",
    ))
    .unwrap_or_else(|error| panic!("invariant: tenant path regex compiles: {error}"))
});

/// Tenant path validation errors.
#[derive(Debug, thiserror::Error)]
pub enum TenantPathError {
    /// Path was empty.
    #[error("path is empty")]
    Empty,
    /// Path contained an empty, current-dir, or parent-dir segment.
    #[error("path contains forbidden segment: {0}")]
    ForbiddenSegment(String),
    /// Path did not match the required shape.
    #[error("path does not match required shape: {0}")]
    ShapeMismatch(String),
    /// Tenant prefix did not match the verified caller tenant.
    #[error("tenant prefix {prefix} does not match caller tenant {caller}")]
    TenantMismatch {
        /// Tenant encoded in the path.
        prefix: DataTenantId,
        /// Verified caller tenant.
        caller: DataTenantId,
    },
}

/// Validated tenant-scoped path.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ValidatedPath {
    /// Full tenant-scoped object path.
    pub full: String,
    /// Data tenant id prefix.
    pub data_tenant_id: DataTenantId,
    /// Card uid segment.
    pub card_uid: String,
    /// Relative object path.
    pub relative_path: String,
}

/// Parse a tenant prefix segment.
///
/// # Errors
/// Returns [`TenantPathError::ShapeMismatch`] when the segment is not a `UUIDv7`
/// tenant id.
pub fn parse_prefix(first_segment: &str) -> Result<DataTenantId, TenantPathError> {
    first_segment
        .parse::<DataTenantId>()
        .map_err(|_| TenantPathError::ShapeMismatch(first_segment.to_owned()))
}

/// Return the post-bucket path portion of a substrate URI.
///
/// For `s3://`, `gs://`, and `file://`, the URL authority is the bucket or
/// host and the path is already post-bucket. For `az://`, the first path
/// segment is the container, so it is stripped before tenant-prefix checks.
///
/// # Errors
/// Returns [`StorageError::InvalidUri`] for unsupported schemes or malformed
/// Azure URIs that do not contain a tenant segment after the container.
pub fn strip_bucket<'a>(scheme: &str, uri: &'a Url) -> Result<&'a str, StorageError> {
    let path = uri.path().trim_start_matches('/');
    match scheme {
        "s3" | "gs" | "file" => Ok(path),
        "az" => {
            let (_container, rest) = path.split_once('/').ok_or_else(|| {
                StorageError::InvalidUri(format!(
                    "az:// uri missing tenant segment after container: {uri}"
                ))
            })?;
            if rest.is_empty() {
                return Err(StorageError::InvalidUri(format!(
                    "az:// uri missing tenant segment after container: {uri}"
                )));
            }
            Ok(rest)
        }
        other => Err(StorageError::InvalidUri(format!(
            "unsupported scheme: {other}"
        ))),
    }
}

/// Build a full tenant-scoped storage path.
#[must_use]
pub fn build(data_tenant_id: DataTenantId, card_uid: &str, relative_path: &str) -> String {
    format!("{data_tenant_id}/cards/{card_uid}/{relative_path}")
}

/// Validate a full tenant-scoped object path.
///
/// `caller_data_tenant_id` must come from a verified identity claim.
///
/// # Errors
/// Returns a [`TenantPathError`] when the path is malformed or belongs to a
/// different tenant.
///
pub fn validate(
    path: &str,
    caller_data_tenant_id: DataTenantId,
) -> Result<ValidatedPath, TenantPathError> {
    if path.is_empty() {
        return Err(TenantPathError::Empty);
    }
    if path.contains('\\') {
        return Err(TenantPathError::ShapeMismatch(path.to_owned()));
    }
    for segment in path.split('/') {
        if matches!(segment, "" | "." | "..") {
            return Err(TenantPathError::ForbiddenSegment(segment.to_owned()));
        }
    }
    let captures = FULL_PATH_RE
        .captures(path)
        .ok_or_else(|| TenantPathError::ShapeMismatch(path.to_owned()))?;
    let Some(tenant_match) = captures.name("tenant") else {
        return Err(TenantPathError::ShapeMismatch(path.to_owned()));
    };
    let tenant = tenant_match
        .as_str()
        .parse::<DataTenantId>()
        .map_err(|_| TenantPathError::ShapeMismatch(path.to_owned()))?;
    if tenant != caller_data_tenant_id {
        return Err(TenantPathError::TenantMismatch {
            prefix: tenant,
            caller: caller_data_tenant_id,
        });
    }
    let Some(card_match) = captures.name("card") else {
        return Err(TenantPathError::ShapeMismatch(path.to_owned()));
    };
    let Some(relative_path_match) = captures.name("rel") else {
        return Err(TenantPathError::ShapeMismatch(path.to_owned()));
    };

    Ok(ValidatedPath {
        full: path.to_owned(),
        data_tenant_id: tenant,
        card_uid: card_match.as_str().to_owned(),
        relative_path: relative_path_match.as_str().to_owned(),
    })
}
