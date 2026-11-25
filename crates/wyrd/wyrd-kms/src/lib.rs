//! `WyrdKms`: the one encryption owner for recoverable stored secrets.
//!
//! Every recoverable secret Wyrd stores — auth-provider and workload-issuer
//! client secrets, the platform OIDC client secret, Gateway managed provider
//! keys, Operator connection secrets, and any future owner's — is sealed and
//! opened here. Server composition builds one [`WyrdKms`] from [`KmsConfig`]
//! and hands it to each resource owner, which keeps its own authorization,
//! persistence, and domain lifecycle and persists the opaque envelope bytes
//! this facade returns.
//!
//! Sealing generates a fresh OS-random 32-byte data-encryption key (DEK),
//! encrypts the value with AES-256-GCM under a fresh nonce, and asks the
//! configured backend to wrap the DEK under the deployment key-encryption key
//! (KEK). File and explicit memory backends wrap locally; GCP KMS, AWS KMS,
//! and Vault Transit wrap remotely and keep the KEK. Both layers authenticate
//! the canonical [`SecretContext`], so an envelope moved to another tenant,
//! owner, purpose, authority, or version refuses to open. The envelope names
//! a logical wrapping-key reference that resolves only through the configured
//! active and retained keys; nothing in stored ciphertext can select a
//! different endpoint or key resource, and no refusal falls back to another
//! key.

#![deny(missing_docs)]

mod aws;
mod config;
mod envelope;
mod gcp;
mod local;
mod remote;
#[cfg(test)]
mod remote_tests;
mod transit;

use std::fmt;

use wyrd_crypt::{EncryptedPayload, SecretBytes, SecretKey};
use wyrd_spec::DataTenantId;
use zeroize::Zeroizing;

pub use config::{KmsBackendKind, KmsConfig};

use crate::aws::AwsKms;
use crate::envelope::Envelope;
use crate::gcp::GcpKms;
use crate::local::LocalKeys;
use crate::transit::TransitKms;

/// Domain tag of the payload layer's associated data.
const PAYLOAD_DOMAIN: &str = "wyrd.kms.payload.v1";

/// Domain tag of the DEK-wrapping layer's authenticated context.
const WRAP_DOMAIN: &str = "wyrd.kms.dek.v1";

/// Whose secret this is: one tenant, or the deployment platform itself.
///
/// Platform secrets use this distinct scope, never an invented tenant.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SecretScope {
    /// A secret owned by one tenant.
    Tenant(DataTenantId),
    /// A deployment-level secret no tenant owns.
    Platform,
}

/// What a stored secret is for; each resource owner has its own purpose.
///
/// A future secret owner adds a variant here instead of a new cipher path.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SecretPurpose {
    /// A tenant human-login connection's OIDC client secret.
    AuthHumanConnection,
    /// A tenant workload trusted issuer's client secret.
    AuthTrustedIssuer,
    /// The platform OIDC connection's client secret.
    PlatformOidc,
    /// A Gateway managed provider credential.
    GatewayProviderCredential,
    /// An Operator connection secret.
    OperatorConnection,
    /// A shared social sign-in provider's OAuth client secret.
    SocialProvider,
    /// The application-mail SMTP relay password.
    ApplicationMail,
    /// The signup challenge (Turnstile) secret.
    SignupChallenge,
    /// The platform derivation key that signup cookies, email proofs and
    /// keyed pseudonyms derive their per-use keys from.
    PlatformDerivationKey,
}

impl SecretPurpose {
    /// The stable label authenticated in the canonical context.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::AuthHumanConnection => "auth_human_connection",
            Self::AuthTrustedIssuer => "auth_trusted_issuer",
            Self::PlatformOidc => "platform_oidc",
            Self::GatewayProviderCredential => "gateway_provider_credential",
            Self::OperatorConnection => "operator_connection",
            Self::SocialProvider => "social_provider",
            Self::ApplicationMail => "application_mail",
            Self::SignupChallenge => "signup_challenge",
            Self::PlatformDerivationKey => "platform_derivation_key",
        }
    }
}

/// The trusted identity of one stored secret value.
///
/// Callers build it from verified authority and trusted row metadata, never
/// from the envelope. Sealing and opening must present the same context; any
/// difference refuses.
#[derive(Debug, Clone, Copy)]
pub struct SecretContext<'a> {
    /// Owning tenant or the platform.
    pub scope: SecretScope,
    /// What the secret is for.
    pub purpose: SecretPurpose,
    /// Stable identity of the owning resource, such as its id.
    pub owner: &'a str,
    /// Provider or credential authority bound to the secret, such as the
    /// provider name and issuer or client metadata, in a fixed order.
    pub authority: &'a [&'a str],
    /// Secret version, where the owner versions its secret.
    pub version: Option<i64>,
}

impl SecretContext<'_> {
    /// The unambiguous canonical encoding of this context under `domain`.
    fn canonical(&self, domain: &str) -> Vec<u8> {
        let (scope, tenant) = match self.scope {
            SecretScope::Tenant(tenant) => ("tenant", tenant.to_string()),
            SecretScope::Platform => ("platform", String::new()),
        };
        let authority_len = self.authority.len().to_string();
        let version = self
            .version
            .map_or_else(|| "-".to_owned(), |v| v.to_string());
        let mut parts: Vec<&str> = vec![
            scope,
            &tenant,
            self.purpose.as_str(),
            self.owner,
            &authority_len,
        ];
        parts.extend_from_slice(self.authority);
        parts.push(&version);
        wyrd_crypt::canonical_aad(domain, &parts)
    }
}

/// Why a remote wrapping provider produced no usable answer.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProviderFailure {
    /// The provider could not be reached or answered unusably.
    Transport,
    /// The call did not finish within its bound.
    Timeout,
    /// The provider refused the caller's identity or permissions.
    Refused,
    /// The provider's access credential could not be obtained.
    Credential,
}

impl ProviderFailure {
    /// The stable snake-case label logs and errors carry.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Transport => "transport",
            Self::Timeout => "timeout",
            Self::Refused => "refused",
            Self::Credential => "credential",
        }
    }
}

/// Why a stored secret could not be sealed or opened, or the facade not built.
///
/// No variant carries key bytes, plaintext, endpoints, paths, tokens, key
/// references, or provider text, so every error is safe to log and map onto a
/// resource owner's public error.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum KmsError {
    /// The stored bytes are not a known envelope format or algorithm.
    #[error("the stored secret is not a recognized envelope")]
    UnknownFormat,
    /// The envelope names a wrapping key this deployment does not allow.
    #[error("the stored secret names a wrapping key this deployment does not allow")]
    KeyNotAllowed,
    /// The envelope did not authenticate under the expected context: a
    /// substituted owner, tampered ciphertext, or wrong key.
    #[error("the stored secret did not authenticate under its expected context")]
    Authentication,
    /// The unwrapped data key is not 32 bytes.
    #[error("the unwrapped data key has the wrong length")]
    InvalidDek,
    /// Local encryption failed.
    #[error("stored secret encryption failed")]
    Encrypt,
    /// The wrapping backend could not serve the call.
    #[error("the {backend} wrapping backend is unavailable: {}", failure.as_str())]
    Unavailable {
        /// Configured backend.
        backend: KmsBackendKind,
        /// Stable failure class.
        failure: ProviderFailure,
    },
    /// The configuration is missing, malformed, or incoherent.
    #[error("kms configuration: {setting} {reason}")]
    Config {
        /// Canonical name of the offending setting.
        setting: &'static str,
        /// Short static reason naming no value.
        reason: &'static str,
    },
}

/// The configured wrapping backend.
enum Backend {
    /// File or explicit memory keys held in process memory.
    Local(LocalKeys),
    /// GCP Cloud KMS.
    Gcp(GcpKms),
    /// AWS KMS.
    Aws(AwsKms),
    /// Vault Transit.
    Transit(TransitKms),
}

impl Backend {
    /// Wrap `dek` under the configured key `key_ref`, authenticating `aad`.
    ///
    /// Remote calls are boxed so a caller's future carries a pointer, not
    /// every provider's HTTP state; the local arm allocates nothing.
    ///
    /// # Errors
    /// Returns [`KmsError`] when wrapping fails.
    async fn wrap(&self, key_ref: &str, dek: &SecretKey, aad: &[u8]) -> Result<Vec<u8>, KmsError> {
        match self {
            Self::Local(keys) => keys.wrap(key_ref, dek, aad),
            Self::Gcp(kms) => Box::pin(kms.wrap(key_ref, dek.expose(), aad)).await,
            Self::Aws(kms) => Box::pin(kms.wrap(key_ref, dek.expose(), aad)).await,
            Self::Transit(kms) => Box::pin(kms.wrap(key_ref, dek.expose(), aad)).await,
        }
    }

    /// Unwrap the opaque `wrapped` DEK under the configured key `key_ref`.
    ///
    /// Remote calls are boxed as in [`Self::wrap`].
    ///
    /// # Errors
    /// Returns [`KmsError`] when unwrapping fails or does not authenticate.
    async fn unwrap(
        &self,
        key_ref: &str,
        wrapped: &[u8],
        aad: &[u8],
    ) -> Result<SecretBytes, KmsError> {
        match self {
            Self::Local(keys) => keys.unwrap(key_ref, wrapped, aad).map(SecretBytes::from),
            Self::Gcp(kms) => Box::pin(kms.unwrap(key_ref, wrapped, aad)).await,
            Self::Aws(kms) => Box::pin(kms.unwrap(key_ref, wrapped, aad)).await,
            Self::Transit(kms) => Box::pin(kms.unwrap(key_ref, wrapped, aad)).await,
        }
    }
}

/// The one stored-secret encryption owner a server process builds.
///
/// Holds the active wrapping-key reference new envelopes use, the retained
/// references still allowed to open old envelopes, and one reusable backend
/// client. Cloning is not offered; share it behind an `Arc`.
pub struct WyrdKms {
    /// Configured backend kind.
    kind: KmsBackendKind,
    /// Reference new envelopes are wrapped under.
    active: String,
    /// References still allowed to open existing envelopes.
    retained: Vec<String>,
    /// Wrap/unwrap implementation.
    backend: Backend,
}

impl fmt::Debug for WyrdKms {
    /// Prints the backend kind and retained-key count only.
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("WyrdKms")
            .field("backend", &self.kind)
            .field("retained_keys", &self.retained.len())
            .finish_non_exhaustive()
    }
}

impl WyrdKms {
    /// Build the facade over a validated `config`.
    ///
    /// `production` refuses the development memory backend and plaintext
    /// endpoints. File keys are read once here; remote clients are built once
    /// and reused for every call.
    ///
    /// # Errors
    /// Returns [`KmsError::Config`] for a missing, malformed, or incoherent
    /// setting or an unreadable key file. Nothing falls back to another
    /// backend or a generated key.
    pub fn new(config: &KmsConfig, production: bool) -> Result<Self, KmsError> {
        let settings = config.resolve(production)?;
        let backend = match settings.kind {
            KmsBackendKind::Memory => Backend::Local(LocalKeys::new(
                [(settings.active.clone(), SecretKey::generate())].into(),
            )),
            KmsBackendKind::File => Backend::Local(LocalKeys::read(
                settings.key_dir.as_deref().ok_or(KmsError::Config {
                    setting: "key_dir",
                    reason: "is required for the file backend",
                })?,
                std::iter::once(&settings.active).chain(&settings.retained),
            )?),
            KmsBackendKind::GcpKms => Backend::Gcp(GcpKms::new(settings.endpoint.as_ref())?),
            KmsBackendKind::AwsKms => {
                let (_, region) = config::aws_region(&settings.active).ok_or(KmsError::Config {
                    setting: "active_key",
                    reason: "is not a valid key reference for the backend",
                })?;
                Backend::Aws(AwsKms::new(region, settings.endpoint.as_ref())?)
            }
            KmsBackendKind::VaultTransit => {
                let (Some(address), Some(vault)) = (settings.endpoint.clone(), settings.vault)
                else {
                    return Err(KmsError::Config {
                        setting: "endpoint",
                        reason: "is required for the vault_transit backend",
                    });
                };
                Backend::Transit(TransitKms::new(address, vault)?)
            }
        };
        Ok(Self {
            kind: settings.kind,
            active: settings.active,
            retained: settings.retained,
            backend,
        })
    }

    /// A facade over an already-built `backend`, for adapter tests.
    #[cfg(test)]
    fn with_backend(
        kind: KmsBackendKind,
        active: &str,
        retained: &[&str],
        backend: Backend,
    ) -> Self {
        Self {
            kind,
            active: active.to_owned(),
            retained: retained.iter().map(|key| (*key).to_owned()).collect(),
            backend,
        }
    }

    /// Seal `plaintext` as the secret `context` identifies under a fresh DEK
    /// wrapped by the active key, returning the common envelope to persist.
    ///
    /// # Errors
    /// Returns [`KmsError::Encrypt`] or the backend's [`KmsError`]; no
    /// envelope is returned unless both layers succeeded.
    pub async fn seal(
        &self,
        context: &SecretContext<'_>,
        plaintext: &[u8],
    ) -> Result<Vec<u8>, KmsError> {
        let dek = SecretKey::generate();
        let payload = wyrd_crypt::encrypt_aad(&dek, plaintext, &context.canonical(PAYLOAD_DOMAIN))
            .map_err(|_| KmsError::Encrypt)?;
        let wrapped_dek = self
            .backend
            .wrap(&self.active, &dek, &context.canonical(WRAP_DOMAIN))
            .await?;
        Envelope {
            key_ref: &self.active,
            wrapped_dek: &wrapped_dek,
            nonce: payload.nonce,
            ciphertext: &payload.ciphertext,
        }
        .encode()
    }

    /// Open `envelope` as the secret `context` identifies.
    ///
    /// # Errors
    /// Returns [`KmsError::UnknownFormat`], [`KmsError::KeyNotAllowed`]
    /// before any backend call, [`KmsError::Authentication`],
    /// [`KmsError::InvalidDek`], or the backend's [`KmsError`].
    pub async fn open(
        &self,
        context: &SecretContext<'_>,
        envelope: &[u8],
    ) -> Result<Zeroizing<Vec<u8>>, KmsError> {
        let envelope = Envelope::decode(envelope)?;
        let dek = self.unwrap_dek(context, &envelope).await?;
        wyrd_crypt::decrypt_aad(
            &dek,
            &EncryptedPayload {
                nonce: envelope.nonce,
                ciphertext: envelope.ciphertext.to_vec(),
            },
            &context.canonical(PAYLOAD_DOMAIN),
        )
        .map(Zeroizing::new)
        .map_err(|_| KmsError::Authentication)
    }

    /// Renew encryption: open `envelope` and reseal the same value under a
    /// fresh DEK and nonce wrapped by the active key.
    ///
    /// Use this when a DEK may be exposed; the returned envelope shares no
    /// key material with the old one.
    ///
    /// # Errors
    /// Returns any [`WyrdKms::open`] or [`WyrdKms::seal`] error; no envelope
    /// is returned unless both succeeded.
    pub async fn renew(
        &self,
        context: &SecretContext<'_>,
        envelope: &[u8],
    ) -> Result<Vec<u8>, KmsError> {
        let plaintext = self.open(context, envelope).await?;
        self.seal(context, &plaintext).await
    }

    /// Rewrap: move `envelope`'s DEK to the active key, keeping the DEK,
    /// payload nonce, and ciphertext unchanged. The value is never decrypted.
    ///
    /// # Errors
    /// Returns [`KmsError::UnknownFormat`], [`KmsError::KeyNotAllowed`],
    /// [`KmsError::InvalidDek`], or the backend's [`KmsError`]; no envelope
    /// is returned unless unwrap and wrap both succeeded.
    pub async fn rewrap(
        &self,
        context: &SecretContext<'_>,
        envelope: &[u8],
    ) -> Result<Vec<u8>, KmsError> {
        let old = Envelope::decode(envelope)?;
        let dek = self.unwrap_dek(context, &old).await?;
        let wrapped_dek = self
            .backend
            .wrap(&self.active, &dek, &context.canonical(WRAP_DOMAIN))
            .await?;
        Envelope {
            key_ref: &self.active,
            wrapped_dek: &wrapped_dek,
            ..old
        }
        .encode()
    }

    /// The configured backend kind.
    #[must_use]
    pub const fn backend(&self) -> KmsBackendKind {
        self.kind
    }

    /// Whether `envelope` already names the active wrapping key.
    ///
    /// # Errors
    /// Returns [`KmsError::UnknownFormat`] for bytes that are not an envelope.
    pub fn is_current(&self, envelope: &[u8]) -> Result<bool, KmsError> {
        Ok(Envelope::decode(envelope)?.key_ref == self.active)
    }

    /// Unwrap `envelope`'s DEK after checking its reference is configured.
    ///
    /// # Errors
    /// Returns [`KmsError::KeyNotAllowed`] before any backend call,
    /// [`KmsError::InvalidDek`] for a DEK that is not 32 bytes, or the
    /// backend's [`KmsError`].
    async fn unwrap_dek(
        &self,
        context: &SecretContext<'_>,
        envelope: &Envelope<'_>,
    ) -> Result<SecretKey, KmsError> {
        if !self.allows(envelope.key_ref) {
            return Err(KmsError::KeyNotAllowed);
        }
        let bytes = self
            .backend
            .unwrap(
                envelope.key_ref,
                envelope.wrapped_dek,
                &context.canonical(WRAP_DOMAIN),
            )
            .await?;
        let dek: [u8; 32] = bytes[..].try_into().map_err(|_| KmsError::InvalidDek)?;
        Ok(SecretKey::from_bytes(dek))
    }

    /// Whether `key_ref` is the active or a retained reference.
    fn allows(&self, key_ref: &str) -> bool {
        self.active == key_ref || self.retained.iter().any(|held| held == key_ref)
    }
}

#[cfg(test)]
mod tests {
    //! Facade behavior over the local backends.

    use super::*;

    /// A tenant Operator-connection context.
    fn context(tenant: DataTenantId) -> SecretContext<'static> {
        SecretContext {
            scope: SecretScope::Tenant(tenant),
            purpose: SecretPurpose::OperatorConnection,
            owner: "conn-1",
            authority: &["slack", "ops"],
            version: Some(1),
        }
    }

    /// An explicit memory facade whose active key is `dev`.
    fn memory() -> WyrdKms {
        WyrdKms::new(
            &KmsConfig {
                backend: Some(KmsBackendKind::Memory),
                active_key: Some("dev".to_owned()),
                ..KmsConfig::default()
            },
            false,
        )
        .expect("memory facade builds")
    }

    /// Write an owner-only base64 key file `name` holding `byte` repeated.
    fn key_file(dir: &std::path::Path, name: &str, byte: u8) {
        use base64::Engine as _;
        let path = dir.join(name);
        std::fs::write(
            &path,
            base64::engine::general_purpose::STANDARD.encode([byte; 32]),
        )
        .expect("key file writes");
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt as _;
            std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o600))
                .expect("owner-only");
        }
    }

    /// A file facade over `dir` with `active` and `retained` references.
    fn file(dir: &std::path::Path, active: &str, retained: &[&str]) -> Result<WyrdKms, KmsError> {
        WyrdKms::new(
            &KmsConfig {
                backend: Some(KmsBackendKind::File),
                active_key: Some(active.to_owned()),
                retained_keys: retained.iter().map(|key| (*key).to_owned()).collect(),
                key_dir: Some(dir.to_path_buf()),
                ..KmsConfig::default()
            },
            false,
        )
    }

    /// Two seals of the same bytes under the same context open to the value
    /// and are independent: different DEKs, payload nonces, and ciphertexts,
    /// each in the common envelope under the active key.
    #[tokio::test]
    async fn seal_uses_a_fresh_dek_and_nonce_per_value_in_the_common_envelope() {
        let kms = memory();
        let ctx = context(DataTenantId::new_v7());
        let first = kms.seal(&ctx, b"xoxb-token").await.expect("seals");
        let second = kms.seal(&ctx, b"xoxb-token").await.expect("seals");
        for sealed in [&first, &second] {
            assert_eq!(
                kms.open(&ctx, sealed).await.expect("opens").as_slice(),
                b"xoxb-token"
            );
        }
        let (a, b) = (
            Envelope::decode(&first).expect("common envelope"),
            Envelope::decode(&second).expect("common envelope"),
        );
        assert_eq!((a.key_ref, b.key_ref), ("dev", "dev"));
        assert_ne!(a.nonce, b.nonce, "fresh payload nonce");
        assert_ne!(a.ciphertext, b.ciphertext);
        let aad = ctx.canonical(WRAP_DOMAIN);
        let dek_a = kms
            .backend
            .unwrap("dev", a.wrapped_dek, &aad)
            .await
            .expect("dek");
        let dek_b = kms
            .backend
            .unwrap("dev", b.wrapped_dek, &aad)
            .await
            .expect("dek");
        assert_eq!(dek_a.len(), 32);
        assert_ne!(dek_a[..], dek_b[..], "independent DEKs");
        assert!(!first.windows(10).any(|w| w == b"xoxb-token"));
    }

    /// File keys survive a restart: a second facade over the same key files
    /// opens what the first sealed.
    #[tokio::test]
    async fn file_keys_open_envelopes_after_restart() {
        let dir = tempfile::tempdir().expect("tempdir");
        key_file(dir.path(), "k1", 7);
        let ctx = context(DataTenantId::new_v7());
        let sealed = file(dir.path(), "k1", &[])
            .expect("file facade builds")
            .seal(&ctx, b"client-secret")
            .await
            .expect("seals");
        let restarted = file(dir.path(), "k1", &[]).expect("file facade rebuilds");
        assert_eq!(
            restarted
                .open(&ctx, &sealed)
                .await
                .expect("opens")
                .as_slice(),
            b"client-secret"
        );
    }

    /// Opening under any substituted scope, purpose, owner, authority, or
    /// version, or after tampering with the payload or wrapped DEK, refuses
    /// as an authentication failure.
    #[tokio::test]
    async fn substituted_context_or_tampered_envelope_refuses() {
        let kms = memory();
        let tenant = DataTenantId::new_v7();
        let ctx = context(tenant);
        let sealed = kms.seal(&ctx, b"xoxb-token").await.expect("seals");
        let substituted = [
            SecretContext {
                scope: SecretScope::Tenant(DataTenantId::new_v7()),
                ..ctx
            },
            SecretContext {
                scope: SecretScope::Platform,
                ..ctx
            },
            SecretContext {
                purpose: SecretPurpose::GatewayProviderCredential,
                ..ctx
            },
            SecretContext {
                owner: "conn-2",
                ..ctx
            },
            SecretContext {
                authority: &["pagerduty", "ops"],
                ..ctx
            },
            SecretContext {
                authority: &["slack", "ops", "1"],
                version: None,
                ..ctx
            },
            SecretContext {
                version: Some(2),
                ..ctx
            },
            SecretContext {
                version: None,
                ..ctx
            },
        ];
        for other in &substituted {
            assert_eq!(
                kms.open(other, &sealed).await.map(|_| ()),
                Err(KmsError::Authentication),
                "{other:?}"
            );
        }
        let envelope = Envelope::decode(&sealed).expect("decodes");
        let wrapped_at = sealed.len() - envelope.ciphertext.len() - 12 - envelope.wrapped_dek.len();
        for at in [sealed.len() - 1, wrapped_at] {
            let mut tampered = sealed.clone();
            tampered[at] ^= 1;
            assert_eq!(
                kms.open(&ctx, &tampered).await.map(|_| ()),
                Err(KmsError::Authentication),
                "byte {at}"
            );
        }
    }

    /// An envelope naming a key the deployment does not configure, an
    /// unknown magic, format, or algorithm, a truncated envelope, and an
    /// unwrapped DEK of the wrong length all refuse without trying another
    /// key.
    #[tokio::test]
    async fn disallowed_key_unknown_format_and_bad_dek_refuse() {
        let ctx = context(DataTenantId::new_v7());
        let sealed = memory().seal(&ctx, b"xoxb-token").await.expect("seals");
        assert_eq!(
            memory().open(&ctx, &sealed).await.map(|_| ()),
            Err(KmsError::Authentication),
            "another process's memory key does not open it"
        );
        let other = WyrdKms::new(
            &KmsConfig {
                backend: Some(KmsBackendKind::Memory),
                active_key: Some("other".to_owned()),
                ..KmsConfig::default()
            },
            false,
        )
        .expect("builds");
        assert_eq!(
            other.open(&ctx, &sealed).await.map(|_| ()),
            Err(KmsError::KeyNotAllowed)
        );
        for at in [0, 4, 5] {
            let mut unknown = sealed.clone();
            unknown[at] ^= 0x40;
            assert_eq!(
                other.open(&ctx, &unknown).await.map(|_| ()),
                Err(KmsError::UnknownFormat),
                "byte {at}"
            );
        }
        assert_eq!(
            other
                .open(&ctx, &sealed[..sealed.len() - 17])
                .await
                .map(|_| ()),
            Err(KmsError::UnknownFormat)
        );

        let dir = tempfile::tempdir().expect("tempdir");
        key_file(dir.path(), "k1", 7);
        let kms = file(dir.path(), "k1", &[]).expect("builds");
        let short = wyrd_crypt::encrypt_aad(
            &SecretKey::from_bytes([7; 32]),
            &[0; 16],
            &ctx.canonical(WRAP_DOMAIN),
        )
        .expect("wraps");
        let mut wrapped = short.nonce.to_vec();
        wrapped.extend_from_slice(&short.ciphertext);
        let bad = Envelope {
            key_ref: "k1",
            wrapped_dek: &wrapped,
            nonce: [0; 12],
            ciphertext: &[0; 16],
        }
        .encode()
        .expect("encodes");
        assert_eq!(
            kms.open(&ctx, &bad).await.map(|_| ()),
            Err(KmsError::InvalidDek)
        );
    }

    /// The file constructor refuses a missing, loose, non-base64, or
    /// wrong-length key file without generating a substitute, and a key
    /// file present in the directory but not configured opens nothing.
    #[tokio::test]
    async fn file_constructor_refuses_bad_keys_and_holds_only_configured_ones() {
        use base64::Engine as _;
        let dir = tempfile::tempdir().expect("tempdir");
        let refusal = |result: Result<WyrdKms, KmsError>| match result {
            Err(KmsError::Config { setting, .. }) => setting,
            other => panic!("expected a configuration refusal, got {other:?}"),
        };
        assert_eq!(refusal(file(dir.path(), "k1", &[])), "key_dir");
        assert_eq!(
            std::fs::read_dir(dir.path()).expect("lists").count(),
            0,
            "no fallback key is generated"
        );
        key_file(dir.path(), "k1", 1);
        for (name, contents) in [
            (
                "short",
                base64::engine::general_purpose::STANDARD.encode([1; 16]),
            ),
            ("text", "not base64!".to_owned()),
        ] {
            std::fs::write(dir.path().join(name), contents).expect("writes");
            #[cfg(unix)]
            {
                use std::os::unix::fs::PermissionsExt as _;
                std::fs::set_permissions(
                    dir.path().join(name),
                    std::fs::Permissions::from_mode(0o600),
                )
                .expect("owner-only");
            }
            assert_eq!(
                refusal(file(dir.path(), "k1", &[name])),
                "key_dir",
                "{name}"
            );
        }
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt as _;
            key_file(dir.path(), "loose", 2);
            std::fs::set_permissions(
                dir.path().join("loose"),
                std::fs::Permissions::from_mode(0o644),
            )
            .expect("loosens");
            assert_eq!(refusal(file(dir.path(), "loose", &[])), "key_dir");
        }

        key_file(dir.path(), "k2", 2);
        let ctx = context(DataTenantId::new_v7());
        let old = file(dir.path(), "k1", &[])
            .expect("builds")
            .seal(&ctx, b"client-secret")
            .await
            .expect("seals");
        let rotated = file(dir.path(), "k2", &[]).expect("builds");
        assert_eq!(
            rotated.open(&ctx, &old).await.map(|_| ()),
            Err(KmsError::KeyNotAllowed),
            "an unconfigured key file grants nothing"
        );
        let retaining = file(dir.path(), "k2", &["k1"]).expect("builds");
        assert_eq!(
            retaining
                .open(&ctx, &old)
                .await
                .expect("retained opens")
                .as_slice(),
            b"client-secret"
        );
    }

    /// Renewal reseals the same value under a fresh DEK and nonce and the
    /// active key; rewrap keeps the DEK, nonce, and ciphertext and only moves
    /// the wrapping to the active key, so the result opens without the old
    /// key retained.
    #[tokio::test]
    async fn renew_uses_a_fresh_dek_and_rewrap_keeps_it_under_the_active_key() {
        let dir = tempfile::tempdir().expect("tempdir");
        key_file(dir.path(), "k1", 1);
        key_file(dir.path(), "k2", 2);
        let ctx = context(DataTenantId::new_v7());
        let old = file(dir.path(), "k1", &[])
            .expect("builds")
            .seal(&ctx, b"client-secret")
            .await
            .expect("seals");
        let rotating = file(dir.path(), "k2", &["k1"]).expect("builds");
        let retired = file(dir.path(), "k2", &[]).expect("builds");
        assert_eq!(rotating.is_current(&old), Ok(false));
        assert_eq!(
            rotating.is_current(b"garbage"),
            Err(KmsError::UnknownFormat)
        );
        let aad = ctx.canonical(WRAP_DOMAIN);
        let before = Envelope::decode(&old).expect("decodes");
        let old_dek = rotating
            .backend
            .unwrap("k1", before.wrapped_dek, &aad)
            .await
            .expect("dek");

        let renewed = rotating.renew(&ctx, &old).await.expect("renews");
        let after = Envelope::decode(&renewed).expect("decodes");
        assert_eq!(after.key_ref, "k2");
        assert_ne!(after.nonce, before.nonce);
        assert_ne!(
            rotating
                .backend
                .unwrap("k2", after.wrapped_dek, &aad)
                .await
                .expect("dek")[..],
            old_dek[..],
            "renewal uses a fresh DEK"
        );
        assert_eq!(
            retired
                .open(&ctx, &renewed)
                .await
                .expect("opens")
                .as_slice(),
            b"client-secret"
        );

        let rewrapped = rotating.rewrap(&ctx, &old).await.expect("rewraps");
        let moved = Envelope::decode(&rewrapped).expect("decodes");
        assert_eq!(moved.key_ref, "k2");
        assert_eq!(
            (moved.nonce, moved.ciphertext),
            (before.nonce, before.ciphertext)
        );
        assert_eq!(
            rotating
                .backend
                .unwrap("k2", moved.wrapped_dek, &aad)
                .await
                .expect("dek")[..],
            old_dek[..],
            "rewrap keeps the DEK"
        );
        assert_eq!(rotating.is_current(&rewrapped), Ok(true));
        assert_eq!(
            retired
                .open(&ctx, &rewrapped)
                .await
                .expect("opens")
                .as_slice(),
            b"client-secret"
        );
        assert_eq!(
            retired.open(&ctx, &old).await.map(|_| ()),
            Err(KmsError::KeyNotAllowed)
        );
        assert_eq!(
            rotating
                .rewrap(&context(DataTenantId::new_v7()), &old)
                .await,
            Err(KmsError::Authentication),
            "rewrap still authenticates the owner"
        );
    }
}
