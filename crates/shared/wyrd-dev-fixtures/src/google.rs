//! Google service-account keys for credential-selection tests.
//!
//! Each key is a fresh RSA key generated in-process, so no private key is
//! committed. `object_store` signs a self-signed JWT from such a key whose
//! `iss` is the account's `client_email`, so a local fixture can tell which
//! identity a client selected without any token exchange.

use aws_lc_rs::encoding::AsDer as _;
use aws_lc_rs::rsa::{KeyPair, KeySize};
use base64::Engine as _;
use base64::engine::general_purpose::{STANDARD, URL_SAFE_NO_PAD};

/// Service-account JSON for `client_email` over a freshly generated key.
///
/// # Panics
/// Panics when the crypto backend cannot generate or encode an RSA key, a
/// test-environment invariant.
#[must_use]
pub fn service_account_json(client_email: &str) -> String {
    let key = KeyPair::generate(KeySize::Rsa2048).expect("RSA key generation");
    let der = key.as_der().expect("PKCS#8 encoding");
    let body = STANDARD.encode(der.as_ref());
    let mut pem = String::from("-----BEGIN PRIVATE KEY-----\n");
    for line in body.as_bytes().chunks(64) {
        pem.push_str(std::str::from_utf8(line).expect("base64 is ASCII"));
        pem.push('\n');
    }
    pem.push_str("-----END PRIVATE KEY-----\n");
    serde_json::json!({
        "type": "service_account",
        "project_id": "wyrd-test",
        "private_key_id": "wyrd-test",
        "private_key": pem,
        "client_email": client_email,
        "client_id": "0",
        "token_uri": "https://oauth2.googleapis.com/token",
    })
    .to_string()
}

/// The `iss` claim of the bearer JWT in an `Authorization` header value.
///
/// Returns `None` when the value is not a bearer JWT with an `iss` claim.
#[must_use]
pub fn bearer_issuer(authorization: &str) -> Option<String> {
    let claims = authorization.strip_prefix("Bearer ")?.split('.').nth(1)?;
    let claims: serde_json::Value =
        serde_json::from_slice(&URL_SAFE_NO_PAD.decode(claims).ok()?).ok()?;
    claims["iss"].as_str().map(str::to_owned)
}
