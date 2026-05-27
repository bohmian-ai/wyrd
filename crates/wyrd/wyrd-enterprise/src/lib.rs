//! Enterprise-tier license verification scaffold.

#![deny(missing_docs)]

use chrono::Utc;
use serde::{Deserialize, Serialize};
use wyrd_auth_verify::AuthError;

const LICENSE_PUBLIC_KEY_PEM: &[u8] = include_bytes!("../keys/license-pub.pem");

/// Known enterprise license feature flags.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum LicenseFeature {
    /// Governance token issuance and validation.
    Governance,
    /// Future feature flags deserialize here without breaking existing licenses.
    #[serde(other)]
    Unknown,
}

/// Verified enterprise license claims.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct License {
    /// Licensed organization.
    pub org: String,
    /// Expiry as Unix seconds.
    pub exp: usize,
    /// Enabled enterprise feature flags.
    #[serde(default)]
    pub features: Vec<LicenseFeature>,
}

/// Resolved enterprise license state.
pub enum LicenseState {
    /// License has a valid signature and is not expired.
    Active(License),
    /// License has a valid signature but is past its expiry timestamp.
    Expired(License),
}

/// Enterprise license resolution errors.
#[derive(Debug, thiserror::Error)]
pub enum LicenseError {
    /// License token, key, or signature validation failed.
    #[error("license verification failed")]
    Invalid(#[source] AuthError),
    /// No enterprise license is configured.
    #[error("no license configured")]
    Missing,
}

/// Verify a license token against the embedded Ed25519 public key.
///
/// Returns [`LicenseState::Active`] when the signature is valid and the token
/// is not expired. Returns [`LicenseState::Expired`] when the signature is
/// valid but the token is past its expiry. Returns an error when the embedded
/// public key cannot be parsed or the token fails EdDSA verification.
///
/// # Errors
/// Returns [`LicenseError::Invalid`] when signature verification fails.
pub fn verify_license(token: &str) -> Result<LicenseState, LicenseError> {
    verify_license_with_key(token, LICENSE_PUBLIC_KEY_PEM)
}

/// Like [`verify_license`] but accepts a caller-supplied public key PEM.
///
/// Used in tests to verify tokens signed with a test key without needing the
/// production private key.
///
/// # Errors
/// Returns [`LicenseError::Invalid`] when signature verification fails.
pub fn verify_license_with_key(token: &str, pem: &[u8]) -> Result<LicenseState, LicenseError> {
    use jsonwebtoken::{Algorithm, Validation};
    use wyrd_auth_verify::{public_key_from_pem, verify_eddsa_with};

    let key = public_key_from_pem(pem).map_err(LicenseError::Invalid)?;
    let mut validation = Validation::new(Algorithm::EdDSA);
    validation.validate_aud = false;
    validation.validate_exp = false;

    let license =
        verify_eddsa_with::<License>(token, &key, validation).map_err(LicenseError::Invalid)?;

    let now = Utc::now().timestamp() as usize;
    if license.exp < now {
        Ok(LicenseState::Expired(license))
    } else {
        Ok(LicenseState::Active(license))
    }
}

/// Load and verify the enterprise license from `WYRD_LICENSE`.
///
/// # Errors
/// Returns [`LicenseError::Missing`] when `WYRD_LICENSE` is unset, or
/// [`LicenseError::Invalid`] when the configured token is invalid.
pub fn license_from_env() -> Result<LicenseState, LicenseError> {
    let token = std::env::var("WYRD_LICENSE").map_err(|_| LicenseError::Missing)?;
    verify_license(&token)
}

/// Enterprise startup hook.
///
/// Logs license state and always continues with community behavior on missing,
/// invalid, or expired licenses. Does not log organization names.
pub fn on_server_start() {
    match license_from_env() {
        Ok(LicenseState::Active(_)) => {
            tracing::info!("enterprise license active");
        }
        Ok(LicenseState::Expired(_)) => {
            tracing::warn!("enterprise license expired; running community features");
        }
        Err(LicenseError::Missing) => {
            tracing::info!("no license configured; running community features");
        }
        Err(error) => {
            tracing::warn!(error = %error, "license invalid; running community features");
        }
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Mutex;

    use chrono::Utc;
    use jsonwebtoken::{Algorithm, EncodingKey, Header, encode};

    use super::{
        License, LicenseError, LicenseFeature, LicenseState, license_from_env,
        verify_license_with_key,
    };

    static ENV_LOCK: Mutex<()> = Mutex::new(());

    const PRIVATE_KEY_PEM: &[u8] = b"-----BEGIN PRIVATE KEY-----\nMC4CAQAwBQYDK2VwBCIEID78cHNjuFihX8aWPytQRoR2iUKHVXgdh92bcTcjQTYV\n-----END PRIVATE KEY-----\n";
    const PUBLIC_KEY_PEM: &[u8] = b"-----BEGIN PUBLIC KEY-----\nMCowBQYDK2VwAyEAWhCX9H41EwSjJJI1E6X3z5fTKyCZ3v2DsJluJ+DZ8Vw=\n-----END PUBLIC KEY-----\n";

    #[test]
    fn valid_license_verifies() {
        let license = license_with_exp(now() + 3_600);
        let token = sign_license(&license);
        let state = verify_with(&token, PUBLIC_KEY_PEM).expect("license verifies");

        assert!(matches!(state, LicenseState::Active(ref l) if l.org == license.org));
    }

    #[test]
    fn expired_license_returns_expired_state() {
        let license = license_with_exp(now() - 3_600);
        let token = sign_license(&license);
        let state =
            verify_with(&token, PUBLIC_KEY_PEM).expect("expired license verifies signature");

        assert!(matches!(state, LicenseState::Expired(_)));
    }

    #[test]
    fn tampered_license_rejected() {
        let license = license_with_exp(now() + 3_600);
        let mut token = sign_license(&license);
        token.push('x');

        assert!(matches!(
            verify_with(&token, PUBLIC_KEY_PEM),
            Err(LicenseError::Invalid(_))
        ));
    }

    #[test]
    fn license_from_env_missing_when_unset() {
        let _guard = ENV_LOCK.lock().expect("env lock is not poisoned");
        let previous = std::env::var_os("WYRD_LICENSE");
        // SAFETY: ENV_LOCK serializes all environment mutations in this test binary.
        unsafe {
            std::env::remove_var("WYRD_LICENSE");
        }

        let result = license_from_env();

        match previous {
            Some(value) => {
                // SAFETY: ENV_LOCK serializes all environment mutations in this test binary.
                unsafe {
                    std::env::set_var("WYRD_LICENSE", value);
                }
            }
            None => {
                // SAFETY: ENV_LOCK serializes all environment mutations in this test binary.
                unsafe {
                    std::env::remove_var("WYRD_LICENSE");
                }
            }
        }

        assert!(matches!(result, Err(LicenseError::Missing)));
    }

    #[test]
    fn license_feature_governance_deserializes() {
        let json = r#""governance""#;
        let feature: LicenseFeature = serde_json::from_str(json).expect("deserializes");
        assert_eq!(feature, LicenseFeature::Governance);
    }

    #[test]
    fn license_feature_unknown_deserializes() {
        let json = r#""future_feature_xyz""#;
        let feature: LicenseFeature = serde_json::from_str(json).expect("deserializes unknown");
        assert_eq!(feature, LicenseFeature::Unknown);
    }

    #[test]
    fn license_with_features_roundtrips() {
        let license = License {
            org: "acme".to_owned(),
            exp: now() + 3_600,
            features: vec![LicenseFeature::Governance],
        };
        let json = serde_json::to_string(&license).expect("serializes");
        let parsed: License = serde_json::from_str(&json).expect("deserializes");
        assert_eq!(parsed.features, license.features);
    }

    #[test]
    fn license_from_env_success_with_test_key() {
        let _guard = ENV_LOCK.lock().expect("env lock is not poisoned");
        let previous = std::env::var_os("WYRD_LICENSE");
        let license = license_with_exp(now() + 3_600);
        let token = sign_license(&license);

        // SAFETY: ENV_LOCK serializes all environment mutations in this test binary.
        unsafe {
            std::env::set_var("WYRD_LICENSE", &token);
        }

        let result = verify_license_with_key(&token, PUBLIC_KEY_PEM);

        match previous {
            Some(value) => {
                // SAFETY: ENV_LOCK serializes all environment mutations in this test binary.
                unsafe {
                    std::env::set_var("WYRD_LICENSE", value);
                }
            }
            None => {
                // SAFETY: ENV_LOCK serializes all environment mutations in this test binary.
                unsafe {
                    std::env::remove_var("WYRD_LICENSE");
                }
            }
        }

        let state = result.expect("valid test-key license verifies");
        assert!(matches!(state, LicenseState::Active(_)));
    }

    fn verify_with(token: &str, pem: &[u8]) -> Result<LicenseState, LicenseError> {
        verify_license_with_key(token, pem)
    }

    fn license_with_exp(exp: usize) -> License {
        License {
            org: "acme".to_owned(),
            exp,
            features: vec![LicenseFeature::Governance],
        }
    }

    fn sign_license(license: &License) -> String {
        let key = EncodingKey::from_ed_pem(PRIVATE_KEY_PEM).expect("test private key parses");
        encode(&Header::new(Algorithm::EdDSA), license, &key).expect("test license signs")
    }

    fn now() -> usize {
        Utc::now().timestamp() as usize
    }
}
