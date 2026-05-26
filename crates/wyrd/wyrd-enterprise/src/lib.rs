//! Enterprise-tier license verification scaffold.

#![deny(missing_docs)]

use chrono::Utc;
use serde::{Deserialize, Serialize};

const LICENSE_PUBLIC_KEY_PEM: &[u8] = include_bytes!("../keys/license-pub.pem");

/// Verified enterprise license claims.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct License {
    /// Licensed organization.
    pub org: String,
    /// Expiry as Unix seconds.
    pub exp: usize,
    /// Enabled enterprise feature flags.
    #[serde(default)]
    pub features: Vec<String>,
}

/// Enterprise license resolution errors.
#[derive(Debug, thiserror::Error)]
pub enum LicenseError {
    /// License token, key, or signature validation failed.
    #[error("license verification failed: {0}")]
    Invalid(String),
    /// No enterprise license is configured.
    #[error("no license configured")]
    Missing,
}

/// Verify a license token against the embedded Ed25519 public key.
///
/// Expiration is intentionally not rejected here. The startup hook verifies
/// the signature first, then downgrades expired licenses to community mode.
///
/// # Errors
/// Returns an error when the embedded public key cannot be parsed or the token
/// fails EdDSA verification.
pub fn verify_license(token: &str) -> Result<License, LicenseError> {
    use jsonwebtoken::{Algorithm, Validation};
    use wyrd_auth_verify::{public_key_from_pem, verify_eddsa_with};

    let key = public_key_from_pem(LICENSE_PUBLIC_KEY_PEM)
        .map_err(|source| LicenseError::Invalid(source.to_string()))?;
    let mut validation = Validation::new(Algorithm::EdDSA);
    validation.validate_aud = false;
    validation.validate_exp = false;

    verify_eddsa_with::<License>(token, &key, validation)
        .map_err(|source| LicenseError::Invalid(source.to_string()))
}

/// Load and verify the enterprise license from `WYRD_LICENSE`.
///
/// # Errors
/// Returns [`LicenseError::Missing`] when `WYRD_LICENSE` is unset, or
/// [`LicenseError::Invalid`] when the configured token is invalid.
pub fn license_from_env() -> Result<License, LicenseError> {
    let token = std::env::var("WYRD_LICENSE").map_err(|_| LicenseError::Missing)?;
    verify_license(&token)
}

/// Enterprise startup hook.
///
/// This hook is intentionally inert in Stage 0. It logs license state and
/// always downgrades to community behavior on missing, invalid, or expired
/// licenses.
pub fn on_server_start() {
    match license_from_env() {
        Ok(license) => {
            let now = Utc::now().timestamp() as usize;
            if license.exp < now {
                eprintln!("wyrd-enterprise: license expired; running community features");
            } else {
                eprintln!("wyrd-enterprise: licensed org={}", license.org);
            }
        }
        Err(LicenseError::Missing) => {
            eprintln!("wyrd-enterprise: no license; running community features");
        }
        Err(error) => {
            eprintln!("wyrd-enterprise: {error}; running community features");
        }
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Mutex;

    use chrono::Utc;
    use jsonwebtoken::{Algorithm, EncodingKey, Header, Validation, encode};

    use super::{License, LicenseError, license_from_env};

    static ENV_LOCK: Mutex<()> = Mutex::new(());

    const PRIVATE_KEY_PEM: &[u8] = b"-----BEGIN PRIVATE KEY-----\nMC4CAQAwBQYDK2VwBCIEID78cHNjuFihX8aWPytQRoR2iUKHVXgdh92bcTcjQTYV\n-----END PRIVATE KEY-----\n";
    const PUBLIC_KEY_PEM: &[u8] = b"-----BEGIN PUBLIC KEY-----\nMCowBQYDK2VwAyEAWhCX9H41EwSjJJI1E6X3z5fTKyCZ3v2DsJluJ+DZ8Vw=\n-----END PUBLIC KEY-----\n";

    #[test]
    fn valid_license_verifies() {
        let license = license_with_exp(now() + 3_600);
        let token = sign_license(&license);
        let verified = verify_with(&token, PUBLIC_KEY_PEM).expect("license verifies");

        assert_eq!(verified.org, license.org);
    }

    #[test]
    fn expired_license_still_verifies_signature() {
        let license = license_with_exp(now() - 3_600);
        let token = sign_license(&license);
        let verified = verify_with(&token, PUBLIC_KEY_PEM).expect("expired license verifies");

        assert!(verified.exp < now());
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
        // Environment mutation is process-global; the test holds ENV_LOCK so
        // this crate's tests do not race with each other while isolating it.
        unsafe {
            std::env::remove_var("WYRD_LICENSE");
        }

        let result = license_from_env();

        match previous {
            Some(value) => unsafe {
                std::env::set_var("WYRD_LICENSE", value);
            },
            None => unsafe {
                std::env::remove_var("WYRD_LICENSE");
            },
        }

        assert!(matches!(result, Err(LicenseError::Missing)));
    }

    fn verify_with(token: &str, pem: &[u8]) -> Result<License, LicenseError> {
        let key = wyrd_auth_verify::public_key_from_pem(pem)
            .map_err(|source| LicenseError::Invalid(source.to_string()))?;
        let mut validation = Validation::new(Algorithm::EdDSA);
        validation.validate_aud = false;
        validation.validate_exp = false;

        wyrd_auth_verify::verify_eddsa_with::<License>(token, &key, validation)
            .map_err(|source| LicenseError::Invalid(source.to_string()))
    }

    fn license_with_exp(exp: usize) -> License {
        License {
            org: "acme".to_owned(),
            exp,
            features: vec!["governance".to_owned()],
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
