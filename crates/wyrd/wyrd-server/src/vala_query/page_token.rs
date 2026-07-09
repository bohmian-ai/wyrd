//! Signed opaque page tokens for ValaQueryService pagination (M-09).
//!
//! A token is a base64url-encoded JSON body plus a dot-separated HMAC-SHA256
//! signature (also base64url). The body pins the query so page N+1 reads
//! the same result window:
//!
//! - `tenant_id` + `auth_hash` — not replayable under a different tenant or caller
//! - `route` + `query_hash` — not reusable across methods or filter sets
//! - `last_seen_key` — keyset continuation (no OFFSET)
//! - `snapshot_id` — Iceberg snapshot pinned on page 1
//! - `issued_at` + `ttl_secs` — TTL-bounded to snapshot lifetime

use base64::Engine;
use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use wyrd_spec::vala::BifrostError;

/// Token lifetime; must be ≤ the table's minimum snapshot-retention horizon.
pub const DEFAULT_PAGE_TOKEN_TTL_SECS: u32 = 300;

/// Decoded page token body.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PageTokenBody {
    pub tenant_id: String,
    pub auth_hash: String,
    pub route: String,
    pub query_hash: u64,
    pub last_seen_key: String,
    pub snapshot_id: Option<i64>,
    pub issued_at: i64,
    pub ttl_secs: u32,
}

impl PageTokenBody {
    pub fn new(
        tenant_id: String,
        auth_hash: String,
        route: &str,
        query_hash: u64,
        last_seen_key: String,
        snapshot_id: Option<i64>,
        ttl_secs: u32,
    ) -> Self {
        Self {
            tenant_id,
            auth_hash,
            route: route.to_owned(),
            query_hash,
            last_seen_key,
            snapshot_id,
            issued_at: chrono::Utc::now().timestamp(),
            ttl_secs,
        }
    }

    pub fn is_expired(&self) -> bool {
        let now = chrono::Utc::now().timestamp();
        now > self.issued_at + i64::from(self.ttl_secs)
    }
}

/// HMAC-SHA256 per RFC 2104 using the `sha2` crate (no external hmac dep needed).
fn hmac_sha256(key: &[u8], msg: &[u8]) -> [u8; 32] {
    const BLOCK_SIZE: usize = 64;
    let mut k = [0u8; BLOCK_SIZE];
    if key.len() > BLOCK_SIZE {
        let h = Sha256::digest(key);
        k[..32].copy_from_slice(&h);
    } else {
        k[..key.len()].copy_from_slice(key);
    }
    let mut ipad = [0x36u8; BLOCK_SIZE];
    let mut opad = [0x5cu8; BLOCK_SIZE];
    for i in 0..BLOCK_SIZE {
        ipad[i] ^= k[i];
        opad[i] ^= k[i];
    }
    let inner = Sha256::new()
        .chain_update(ipad)
        .chain_update(msg)
        .finalize();
    Sha256::new()
        .chain_update(opad)
        .chain_update(inner)
        .finalize()
        .into()
}

/// Encode a `PageTokenBody` into a signed opaque token string.
pub fn encode(body: &PageTokenBody, key: &[u8]) -> Result<String, BifrostError> {
    let json = serde_json::to_string(body).map_err(|e| BifrostError::Internal {
        detail: format!("page token encode failed: {e}"),
    })?;
    let body_b64 = URL_SAFE_NO_PAD.encode(json.as_bytes());
    let sig = hmac_sha256(key, body_b64.as_bytes());
    let sig_b64 = URL_SAFE_NO_PAD.encode(sig);
    Ok(format!("{body_b64}.{sig_b64}"))
}

/// Decode and verify a page token, returning the body on success.
///
/// Returns `BifrostError::PageTokenInvalid` on signature or decode failure.
/// Returns `BifrostError::PageSnapshotExpired` when the token is live-TTL but
/// the snapshot is confirmed absent (caller's responsibility to check snapshot).
pub fn decode(token: &str, key: &[u8]) -> Result<PageTokenBody, BifrostError> {
    let (body_b64, sig_b64) = token
        .split_once('.')
        .ok_or(BifrostError::PageTokenInvalid)?;

    let expected_sig = hmac_sha256(key, body_b64.as_bytes());
    let received_sig = URL_SAFE_NO_PAD
        .decode(sig_b64.as_bytes())
        .map_err(|_| BifrostError::PageTokenInvalid)?;

    // Constant-time comparison.
    let ok = expected_sig.len() == received_sig.len()
        && expected_sig
            .iter()
            .zip(received_sig.iter())
            .fold(0u8, |acc, (a, b)| acc | (a ^ b))
            == 0;
    if !ok {
        return Err(BifrostError::PageTokenInvalid);
    }

    let json_bytes = URL_SAFE_NO_PAD
        .decode(body_b64.as_bytes())
        .map_err(|_| BifrostError::PageTokenInvalid)?;
    let body: PageTokenBody =
        serde_json::from_slice(&json_bytes).map_err(|_| BifrostError::PageTokenInvalid)?;

    if body.is_expired() {
        return Err(BifrostError::PageTokenInvalid);
    }

    Ok(body)
}

/// Verify a token AND assert tenant / auth / route / query-shape binding.
///
/// A cross-tenant, cross-method, or mutated-filter token is `PageTokenInvalid`.
pub fn verify(
    token: &str,
    key: &[u8],
    tenant_id: &str,
    auth_hash: &str,
    route: &str,
    query_hash: u64,
) -> Result<PageTokenBody, BifrostError> {
    let body = decode(token, key)?;
    if body.tenant_id != tenant_id
        || body.auth_hash != auth_hash
        || body.route != route
        || body.query_hash != query_hash
    {
        return Err(BifrostError::PageTokenInvalid);
    }
    Ok(body)
}

/// Compute a stable hash of the caller's effective permissions for binding.
pub fn permissions_hash(permissions: &wyrd_runtime::PermissionSet) -> String {
    let mut perms: Vec<String> = permissions.iter().map(|p| p.to_string()).collect();
    perms.sort_unstable();
    let combined = perms.join(",");
    let hash = Sha256::digest(combined.as_bytes());
    URL_SAFE_NO_PAD.encode(hash)
}

/// Compute a stable hash of typed query parameters for shape binding.
pub fn query_hash(parts: &[&str]) -> u64 {
    let mut hash: u64 = 14695981039346656037;
    for part in parts {
        for byte in part.as_bytes() {
            hash ^= u64::from(*byte);
            hash = hash.wrapping_mul(1099511628211);
        }
        hash ^= b'|' as u64;
        hash = hash.wrapping_mul(1099511628211);
    }
    hash
}

#[cfg(test)]
mod tests {
    use super::*;

    const KEY: &[u8] = b"test-page-token-key-32-bytes-pad!!";

    #[test]
    fn round_trip_encodes_and_decodes() {
        let body = PageTokenBody::new(
            "tenant-1".to_owned(),
            "authhash".to_owned(),
            "get_trace",
            42,
            "2026-07-01T00:00:00Z:span-abc".to_owned(),
            Some(12345),
            DEFAULT_PAGE_TOKEN_TTL_SECS,
        );
        let token = encode(&body, KEY).expect("encode succeeds");
        let decoded = decode(&token, KEY).expect("decode succeeds");
        assert_eq!(decoded.tenant_id, "tenant-1");
        assert_eq!(decoded.route, "get_trace");
        assert_eq!(decoded.query_hash, 42);
    }

    #[test]
    fn tampered_signature_is_invalid() {
        let body = PageTokenBody::new(
            "t".to_owned(),
            "h".to_owned(),
            "r",
            0,
            "k".to_owned(),
            None,
            DEFAULT_PAGE_TOKEN_TTL_SECS,
        );
        let token = encode(&body, KEY).expect("encode");
        let tampered = format!("{token}x");
        assert!(matches!(
            decode(&tampered, KEY),
            Err(BifrostError::PageTokenInvalid)
        ));
    }

    #[test]
    fn wrong_key_is_invalid() {
        let body = PageTokenBody::new(
            "t".to_owned(),
            "h".to_owned(),
            "r",
            0,
            "k".to_owned(),
            None,
            DEFAULT_PAGE_TOKEN_TTL_SECS,
        );
        let token = encode(&body, KEY).expect("encode");
        assert!(matches!(
            decode(&token, b"wrong-key"),
            Err(BifrostError::PageTokenInvalid)
        ));
    }

    #[test]
    fn cross_tenant_verify_is_invalid() {
        let body = PageTokenBody::new(
            "tenant-a".to_owned(),
            "h".to_owned(),
            "route",
            1,
            "k".to_owned(),
            None,
            DEFAULT_PAGE_TOKEN_TTL_SECS,
        );
        let token = encode(&body, KEY).expect("encode");
        let err =
            verify(&token, KEY, "tenant-b", "h", "route", 1).expect_err("cross-tenant is rejected");
        assert!(matches!(err, BifrostError::PageTokenInvalid));
    }

    #[test]
    fn cross_route_verify_is_invalid() {
        let body = PageTokenBody::new(
            "t".to_owned(),
            "h".to_owned(),
            "route-a",
            1,
            "k".to_owned(),
            None,
            DEFAULT_PAGE_TOKEN_TTL_SECS,
        );
        let token = encode(&body, KEY).expect("encode");
        let err = verify(&token, KEY, "t", "h", "route-b", 1).expect_err("cross-route is rejected");
        assert!(matches!(err, BifrostError::PageTokenInvalid));
    }

    #[test]
    fn mutated_query_hash_is_invalid() {
        let body = PageTokenBody::new(
            "t".to_owned(),
            "h".to_owned(),
            "route",
            1,
            "k".to_owned(),
            None,
            DEFAULT_PAGE_TOKEN_TTL_SECS,
        );
        let token = encode(&body, KEY).expect("encode");
        let err =
            verify(&token, KEY, "t", "h", "route", 99).expect_err("mutated query hash is rejected");
        assert!(matches!(err, BifrostError::PageTokenInvalid));
    }

    #[test]
    fn query_hash_deterministic() {
        let h1 = query_hash(&["service=checkout", "status=ERROR"]);
        let h2 = query_hash(&["service=checkout", "status=ERROR"]);
        assert_eq!(h1, h2);
        let h3 = query_hash(&["service=other", "status=ERROR"]);
        assert_ne!(h1, h3);
    }
}
