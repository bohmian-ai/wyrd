//! Deterministic Ed25519 keys for in-process Wyrd tests.

use std::sync::OnceLock;

use ed25519_dalek::pkcs8::spki::der::pem::LineEnding;
use ed25519_dalek::pkcs8::{EncodePrivateKey, EncodePublicKey};
use ed25519_dalek::{SigningKey, VerifyingKey};
use secrecy::SecretString;

static KEYPAIR: OnceLock<(SigningKey, VerifyingKey)> = OnceLock::new();
static PRIVATE_KEY_PEM: OnceLock<String> = OnceLock::new();
static PUBLIC_KEY_PEM: OnceLock<String> = OnceLock::new();

/// Return the deterministic process-local keypair.
#[must_use]
pub fn keypair() -> &'static (SigningKey, VerifyingKey) {
    KEYPAIR.get_or_init(|| {
        let signing = SigningKey::from_bytes(&[0x42; 32]);
        let verifying = signing.verifying_key();
        (signing, verifying)
    })
}

/// Return the deterministic signing key.
#[must_use]
pub fn signing_key() -> &'static SigningKey {
    &keypair().0
}

/// Return the deterministic verifying key.
#[must_use]
pub fn verifying_key() -> &'static VerifyingKey {
    &keypair().1
}

pub(crate) fn private_key_pem() -> SecretString {
    SecretString::from(
        PRIVATE_KEY_PEM
            .get_or_init(|| {
                signing_key()
                    .to_pkcs8_pem(LineEnding::LF)
                    .expect("deterministic Ed25519 signing key encodes as PKCS#8 PEM")
                    .to_string()
            })
            .clone(),
    )
}

pub(crate) fn public_key_pem() -> &'static str {
    PUBLIC_KEY_PEM.get_or_init(|| {
        verifying_key()
            .to_public_key_pem(LineEnding::LF)
            .expect("deterministic Ed25519 verifying key encodes as public-key PEM")
    })
}

#[cfg(test)]
mod tests {
    use secrecy::ExposeSecret;

    use super::{keypair, private_key_pem, public_key_pem};

    #[test]
    fn keypair_is_deterministic() {
        let first = keypair();
        let second = keypair();

        assert_eq!(first.0.to_bytes(), second.0.to_bytes());
        assert_eq!(first.1.to_bytes(), second.1.to_bytes());
    }

    #[test]
    fn keypair_exports_pem() {
        assert!(private_key_pem().expose_secret().contains("PRIVATE KEY"));
        assert!(public_key_pem().contains("PUBLIC KEY"));
    }
}
