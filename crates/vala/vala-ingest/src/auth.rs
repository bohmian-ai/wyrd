//! gRPC ingest authentication.
//!
//! The interceptor binds to the **same** [`TokenVerifier`] the HTTP
//! `AuthenticatedPrincipal` uses: it reads the bearer from the
//! `x-wyrd-access-token` metadata, derives the expected tenant from the
//! unverified token, and verifies the signature against that tenant through the
//! single verifier seam. The resulting [`AuthContext`] carries the resolved
//! `Principal` (with its `card_scope`) plus the request correlator; scope
//! enforcement is the service's job, not the interceptor's.
//!
//! `wyrd-request-id`: the interceptor reads the inbound correlator or mints a
//! UUIDv7 when absent, so `olap_commits` attribution and the C5 audit event
//! carry the same id the HTTP routes use.

use std::sync::Arc;

use secrecy::{ExposeSecret, SecretString};
use wyrd_auth_verify::{
    PermissionResolver, TokenVerifier, WYRD_ACCESS_TOKEN_HEADER,
    tenant_from_unverified_access_token,
};
use wyrd_runtime::Principal;
use wyrd_spec::ids::DataTenantId;
use wyrd_spec::request_id::RequestId;
use wyrd_tonic::tonic::metadata::MetadataMap;

use crate::error::IngestError;

/// gRPC metadata key for the Wyrd request correlator (the gRPC spelling of the
/// HTTP `Wyrd-Request-Id` header). Independent of `traceparent`.
pub const WYRD_REQUEST_ID_METADATA: &str = "wyrd-request-id";

/// Resolved identity for one ingest stream, produced by [`authenticate`] and
/// handed to the service via request extensions.
#[derive(Clone, Debug)]
pub struct AuthContext {
    /// The verified principal (carries `card_scope`, C0b).
    pub principal: Principal,
    /// The tenant resolved from the token (never from the wire).
    pub tenant: DataTenantId,
    /// The request correlator, read from metadata or freshly minted.
    pub request_id: RequestId,
}

/// Read the bearer token from the `x-wyrd-access-token` metadata, stripping an
/// optional `Bearer ` prefix.
///
/// # Errors
/// Returns [`IngestError::Unauthenticated`] when the header is absent or not
/// valid ASCII.
pub fn extract_bearer(metadata: &MetadataMap) -> Result<SecretString, IngestError> {
    let raw = metadata
        .get(WYRD_ACCESS_TOKEN_HEADER.as_str())
        .ok_or_else(|| IngestError::Unauthenticated("missing x-wyrd-access-token".to_owned()))?;
    let value = raw
        .to_str()
        .map_err(|_| IngestError::Unauthenticated("x-wyrd-access-token is not ASCII".to_owned()))?;
    let token = value.strip_prefix("Bearer ").unwrap_or(value);
    Ok(SecretString::from(token.to_owned()))
}

/// Read `wyrd-request-id` from the inbound metadata, or mint a UUIDv7 when it is
/// absent or malformed.
#[must_use]
pub fn read_or_mint_request_id(metadata: &MetadataMap) -> RequestId {
    metadata
        .get(WYRD_REQUEST_ID_METADATA)
        .and_then(|value| value.to_str().ok())
        .and_then(|value| RequestId::parse(value).ok())
        .unwrap_or_else(|| {
            RequestId::parse(&uuid::Uuid::now_v7().to_string())
                .expect("UUIDv7 is a valid request id")
        })
}

/// Verify the inbound stream's bearer through `verifier` and build the
/// [`AuthContext`].
///
/// # Errors
/// Returns [`IngestError::Unauthenticated`] when the bearer is missing, does not
/// name a tenant, or fails signature/tenant verification.
pub async fn authenticate<R: PermissionResolver + 'static>(
    verifier: &TokenVerifier<R>,
    metadata: &MetadataMap,
) -> Result<AuthContext, IngestError> {
    let token = extract_bearer(metadata)?;
    let expected_tenant = tenant_from_unverified_access_token(token.expose_secret())
        .map_err(|_| IngestError::Unauthenticated("token does not name a tenant".to_owned()))?;
    let verified = verifier
        .verify(&token, &expected_tenant)
        .await
        .map_err(|error| IngestError::Unauthenticated(error.to_string()))?;
    let request_id = read_or_mint_request_id(metadata);
    Ok(AuthContext {
        principal: verified.principal.clone(),
        tenant: expected_tenant,
        request_id,
    })
}

/// Interceptor holder generic over the concrete resolver-backed verifier.
///
/// S3.C2 injects the concrete `SqlPermissionResolver`-backed verifier at mount
/// and wires [`authenticate`] into request-extension population; this type keeps
/// the verifier seam in one place so ingest can never grow a second auth path.
#[derive(Clone)]
pub struct IngestAuthInterceptor<R: PermissionResolver + 'static> {
    verifier: Arc<TokenVerifier<R>>,
}

impl<R: PermissionResolver + 'static> IngestAuthInterceptor<R> {
    /// Verify `metadata`'s bearer and produce the [`AuthContext`].
    ///
    /// # Errors
    /// Propagates [`authenticate`]'s failure.
    pub async fn authenticate(&self, metadata: &MetadataMap) -> Result<AuthContext, IngestError> {
        authenticate(&self.verifier, metadata).await
    }
}

/// Construct an [`IngestAuthInterceptor`] bound to `verifier`.
pub fn ingest_auth_interceptor<R: PermissionResolver + 'static>(
    verifier: Arc<TokenVerifier<R>>,
) -> IngestAuthInterceptor<R> {
    IngestAuthInterceptor { verifier }
}

#[cfg(test)]
mod tests {
    use super::{WYRD_REQUEST_ID_METADATA, extract_bearer, read_or_mint_request_id};
    use wyrd_tonic::tonic::metadata::MetadataMap;

    #[test]
    fn extract_bearer_strips_prefix() {
        let mut md = MetadataMap::new();
        md.insert("x-wyrd-access-token", "Bearer abc.def.ghi".parse().unwrap());
        let token = extract_bearer(&md).expect("bearer present");
        assert_eq!(secrecy::ExposeSecret::expose_secret(&token), "abc.def.ghi");
    }

    #[test]
    fn extract_bearer_rejects_absent_header() {
        let md = MetadataMap::new();
        assert!(extract_bearer(&md).is_err());
    }

    #[test]
    fn request_id_is_minted_when_absent() {
        let md = MetadataMap::new();
        let id = read_or_mint_request_id(&md);
        // Minted value is a valid UUIDv7 request id.
        assert!(wyrd_spec::request_id::RequestId::parse(id.as_str()).is_ok());
    }

    #[test]
    fn request_id_is_preserved_when_supplied() {
        let supplied = uuid::Uuid::now_v7().to_string();
        let mut md = MetadataMap::new();
        md.insert(WYRD_REQUEST_ID_METADATA, supplied.parse().unwrap());
        let id = read_or_mint_request_id(&md);
        assert_eq!(id.as_str(), supplied);
    }

    #[test]
    fn malformed_request_id_is_replaced_with_minted() {
        let mut md = MetadataMap::new();
        md.insert(WYRD_REQUEST_ID_METADATA, "not-a-uuid".parse().unwrap());
        let id = read_or_mint_request_id(&md);
        assert_ne!(id.as_str(), "not-a-uuid");
        assert!(wyrd_spec::request_id::RequestId::parse(id.as_str()).is_ok());
    }
}
