//! Per-attempt provider credential resolution.
//!
//! The server admits a call before the engine asks this resolver for plaintext,
//! so a credential is never touched for a denied, over-limit, or over-budget
//! call. Plaintext lives only in a [`ProviderSecret`] that the engine drops
//! when the attempt finishes.

use std::collections::BTreeMap;
use std::fs::{File, Metadata};
use std::io::Read as _;
#[cfg(unix)]
use std::os::unix::fs::PermissionsExt;
use std::path::Path;
use std::sync::Arc;

use secrecy::{ExposeSecret, SecretString};
use tokio::io::AsyncReadExt;
use wyrd_spec::DataTenantId;
use wyrd_spec::gateway::{ProviderCredentialState, ProviderDeployment};
use wyrd_spec::ids::SecretBackendName;
use wyrd_spec::security::SecretRef;

use crate::managed::{ManagedSecretBinding, ManagedSecretKeys};
use crate::snapshot::{GatewayCredentialSource, GatewayTenantSnapshot};
use crate::vault::VaultBackend;

/// Largest mounted secret file read, in bytes; longer files fail closed.
const MAX_SECRET_FILE_BYTES: u64 = 64 * 1024;

/// Redacted provider credential plaintext held for one upstream attempt.
#[derive(Debug)]
pub struct ProviderSecret(SecretString);

impl ProviderSecret {
    /// Wraps resolved plaintext.
    #[must_use]
    pub fn new(value: SecretString) -> Self {
        Self(value)
    }

    /// Borrows the plaintext for building the upstream authentication header.
    #[must_use]
    pub fn expose(&self) -> &str {
        self.0.expose_secret()
    }
}

/// Stable reason a deployment's credential could not be resolved.
///
/// Every variant fails the attempt closed before dispatch and never carries
/// a credential value, binding target, or backend reference.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum CredentialError {
    /// The deployment names a credential absent from the admitted snapshot,
    /// or Vault has no live string at the referenced key.
    #[error("provider credential is missing")]
    Missing,
    /// The credential was revoked.
    #[error("provider credential is revoked")]
    Revoked,
    /// The credential belongs to another provider than the deployment.
    #[error("provider credential does not match the deployment provider")]
    ProviderMismatch,
    /// The binding, backend, or assignment is not configured for this
    /// tenant, provider, and endpoint, or Vault refused the token.
    #[error("provider credential source is not configured")]
    Unconfigured,
    /// The source exists but produced no usable value.
    #[error("provider credential source is unavailable")]
    Unavailable,
}

/// Resolves deployment credentials from operator bindings, Vault backends,
/// and the tenant keyrings protecting managed secrets.
#[derive(Debug, Clone, Default)]
pub struct CredentialResolver {
    /// Vault KV v2 backends keyed by operator-declared identity.
    backends: BTreeMap<SecretBackendName, VaultBackend>,
    /// Configured tenant keyrings, shared with gateway administration so one
    /// capability owns key selection and envelope protection.
    keys: Arc<ManagedSecretKeys>,
}

impl CredentialResolver {
    /// Builds a resolver over the configured Vault backends and tenant
    /// keyrings.
    #[must_use]
    pub fn new(
        backends: BTreeMap<SecretBackendName, VaultBackend>,
        keys: Arc<ManagedSecretKeys>,
    ) -> Self {
        Self { backends, keys }
    }

    /// Resolves the plaintext `deployment` authenticates with for `tenant`,
    /// or `None` when it uses no upstream authentication.
    ///
    /// The credential must exist in the admitted `snapshot`, be active, and
    /// belong to the deployment's provider. An operator-owned source must
    /// additionally carry an assignment permitting `tenant`, that provider,
    /// and — for `OpenAiCompatible` — the deployment's exact endpoint host.
    /// Environment bindings read the operator's environment variable or
    /// restrictive mounted file; Vault sources read the latest version afresh;
    /// a managed secret is opened locally from `tenant`'s own keyring, with no
    /// Postgres round trip beyond the admission snapshot. Surrounding
    /// whitespace is trimmed and an empty value fails closed.
    ///
    /// # Errors
    ///
    /// Returns the [`CredentialError`] naming why no usable value exists.
    pub async fn resolve(
        &self,
        tenant: DataTenantId,
        snapshot: &GatewayTenantSnapshot,
        deployment: &ProviderDeployment,
    ) -> Result<Option<ProviderSecret>, CredentialError> {
        let Some(name) = deployment.auth.credential() else {
            return Ok(None);
        };
        let credential = snapshot.credential(name).ok_or(CredentialError::Missing)?;
        if credential.state == ProviderCredentialState::Revoked {
            return Err(CredentialError::Revoked);
        }
        if credential.provider != deployment.model.provider {
            return Err(CredentialError::ProviderMismatch);
        }
        // A managed secret carries no operator binding: the submitting
        // tenant's own keyring is its authority, and the sealed payload binds
        // the identity an operator assignment would otherwise cover.
        let managed = matches!(
            credential.source,
            GatewayCredentialSource::ManagedSecret { .. }
        );
        let assigned = managed
            || credential.assignment.as_ref().is_some_and(|assignment| {
                assignment.permits(tenant, &credential.provider, Some(&deployment.adapter))
            });
        if !assigned {
            return Err(CredentialError::Unconfigured);
        }
        let raw = match &credential.source {
            GatewayCredentialSource::Environment { secret, .. } => {
                read_binding(secret.as_ref().ok_or(CredentialError::Unconfigured)?).await?
            }
            GatewayCredentialSource::ExternalSecret { backend, reference } => {
                self.backends
                    .get(backend)
                    .ok_or(CredentialError::Unconfigured)?
                    .fetch(reference)
                    .await?
            }
            GatewayCredentialSource::ManagedSecret { envelope } => self.keys.open(
                ManagedSecretBinding {
                    tenant,
                    name: &credential.name,
                    provider: &credential.provider,
                },
                envelope,
            )?,
        };
        let trimmed = raw.expose_secret().trim();
        if trimmed.is_empty() {
            return Err(CredentialError::Unavailable);
        }
        Ok(Some(ProviderSecret::new(SecretString::from(trimmed))))
    }
}

/// Reads an operator environment or mounted-file secret reference.
///
/// A file is validated through metadata of the already-open handle, so no
/// path swap can slip between check and read: on Unix it must be a regular
/// file with no group or other permission bits. Other platforms check only
/// that it is a regular file and rely on the deployment restricting access.
///
/// # Errors
///
/// Returns `Unavailable` for an unset variable, an unreadable, non-regular,
/// permissive, or oversized file, non-UTF-8 content, or a reference kind
/// bindings do not support.
pub(crate) async fn read_binding(secret: &SecretRef) -> Result<SecretString, CredentialError> {
    if let SecretRef::Env { name } = secret {
        return std::env::var(name)
            .map(SecretString::from)
            .map_err(|_| CredentialError::Unavailable);
    }
    let SecretRef::File { path } = secret else {
        return Err(CredentialError::Unavailable);
    };
    let file = tokio::fs::File::open(path)
        .await
        .map_err(|_| CredentialError::Unavailable)?;
    let metadata = file
        .metadata()
        .await
        .map_err(|_| CredentialError::Unavailable)?;
    if !restrictive(&metadata) {
        return Err(CredentialError::Unavailable);
    }
    let mut value = String::new();
    let read = file
        .take(MAX_SECRET_FILE_BYTES + 1)
        .read_to_string(&mut value)
        .await
        .map_err(|_| CredentialError::Unavailable)?;
    if read as u64 > MAX_SECRET_FILE_BYTES {
        return Err(CredentialError::Unavailable);
    }
    Ok(SecretString::from(value))
}

/// Reads a mounted secret file synchronously under the same rules as
/// [`read_binding`].
///
/// The boot path that loads tenant wrapping keys has no runtime to await on,
/// but it holds the same authority a request-path binding does, so it shares
/// this module's [`restrictive`] rule and byte cap rather than its own. As
/// there, the check runs against metadata of the already-open handle, so no
/// path swap can slip between check and read.
///
/// # Errors
///
/// Returns a short static reason — unopenable, non-regular or permissive, or
/// oversized — naming no path and no content, so a caller can render it
/// verbatim in a redacted configuration error.
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
    use std::io::Write as _;

    use serde_json::{Value, json};
    use url::Url;
    use wiremock::matchers::method;
    use wiremock::{Mock, MockServer, ResponseTemplate};
    use wyrd_spec::gateway::{
        ExternalSecretReference, GatewayCapturePolicy, GatewayFallbackPolicy,
        GatewayGovernancePolicy,
    };
    use wyrd_spec::ids::{CredentialBindingName, ProviderCredentialName, ProviderId};

    use super::*;
    use crate::snapshot::{CredentialAssignment, GatewayCredentialSnapshot};

    /// Tenant every fixture assignment belongs to.
    const TENANT: DataTenantId = DataTenantId::SYSTEM_OWNER;

    /// Builds a bearer deployment of `provider/model` with `adapter`,
    /// referencing credential `primary`.
    fn deployment(provider: &str, adapter: &Value) -> ProviderDeployment {
        serde_json::from_value(json!({
            "name": "primary",
            "model": {"provider": provider, "model": "m"},
            "adapter": adapter,
            "auth": {"bearer": {"credential": "primary"}},
            "capabilities": ["chat_completions"],
            "routing_weight": 1,
        }))
        .expect("deployment decodes")
    }

    /// Assignment of `tenant` and `provider` with an optional `host`.
    fn assignment(
        tenant: DataTenantId,
        provider: &str,
        host: Option<&str>,
    ) -> CredentialAssignment {
        CredentialAssignment {
            tenant,
            provider: ProviderId::new(provider).expect("provider"),
            host: host.map(str::to_owned),
        }
    }

    /// Builds a snapshot holding one `primary` credential.
    fn snapshot(
        provider: &str,
        state: ProviderCredentialState,
        source: GatewayCredentialSource,
        assignment: Option<CredentialAssignment>,
    ) -> GatewayTenantSnapshot {
        GatewayTenantSnapshot {
            deployments: Vec::new(),
            credentials: vec![GatewayCredentialSnapshot {
                name: ProviderCredentialName::new("primary").expect("name"),
                provider: ProviderId::new(provider).expect("provider"),
                state,
                source,
                assignment,
            }],
            fallback: GatewayFallbackPolicy::default(),
            governance: GatewayGovernancePolicy::default(),
            capture: serde_json::from_value::<GatewayCapturePolicy>(
                json!({"mode": "disabled", "payload_fields": [], "version": 1}),
            )
            .expect("capture decodes"),
        }
    }

    /// Environment source over `secret`.
    fn environment(secret: Option<SecretRef>) -> GatewayCredentialSource {
        GatewayCredentialSource::Environment {
            binding: CredentialBindingName::new("openai-env").expect("binding"),
            secret,
        }
    }

    /// Vault source over `backend`.
    fn external(backend: &str) -> GatewayCredentialSource {
        GatewayCredentialSource::ExternalSecret {
            backend: SecretBackendName::new(backend).expect("backend"),
            reference: ExternalSecretReference::new("tenants/a/openai#key").expect("reference"),
        }
    }

    /// File reference to `file`.
    fn file_ref(file: &tempfile::NamedTempFile) -> SecretRef {
        SecretRef::File {
            path: file.path().to_string_lossy().into_owned(),
        }
    }

    /// Resolver whose `(name, address)` Vault backends read the token in
    /// `token` from mount `kv`.
    fn resolver(backends: &[(&str, &str)], token: &tempfile::NamedTempFile) -> CredentialResolver {
        CredentialResolver::new(
            backends
                .iter()
                .map(|(name, address)| {
                    let backend = VaultBackend::new(
                        Url::parse(address).expect("address"),
                        "kv".to_owned(),
                        file_ref(token),
                        None,
                        None,
                    )
                    .expect("backend builds");
                    (SecretBackendName::new(*name).expect("backend"), backend)
                })
                .collect(),
            Arc::default(),
        )
    }

    /// Restrictive file holding `value`.
    fn secret_file(value: &str) -> tempfile::NamedTempFile {
        let mut file = tempfile::NamedTempFile::new().expect("secret file");
        writeln!(file, "{value}").expect("secret written");
        file
    }

    /// Resolves `snap` for `target` as [`TENANT`], exposing the plaintext.
    async fn resolve(
        resolver: &CredentialResolver,
        snap: GatewayTenantSnapshot,
        target: &ProviderDeployment,
    ) -> Result<Option<String>, CredentialError> {
        resolver
            .resolve(TENANT, &snap, target)
            .await
            .map(|secret| secret.map(|value| value.expose().to_owned()))
    }

    /// Environment, mounted-file, and Vault sources resolve trimmed plaintext
    /// only within their tenant/provider/host assignment; an unassigned,
    /// cross-tenant, wrong-provider, wrong-host, or unconfigured source is
    /// unconfigured.
    #[tokio::test]
    async fn credential_sources_resolve_within_assignments() {
        let vault = MockServer::start().await;
        Mock::given(method("GET"))
            .respond_with(
                ResponseTemplate::new(200)
                    .set_body_json(json!({"data": {"data": {"key": "sk-vault\n"}}})),
            )
            .mount(&vault)
            .await;
        let token = secret_file("hvs.test");
        let resolver = resolver(&[("vault", &vault.uri())], &token);
        let file = secret_file("sk-file");
        let openai = deployment("openai", &json!("openai"));
        let active = ProviderCredentialState::Active;
        let owned = || Some(assignment(TENANT, "openai", None));

        let env_file = || environment(Some(file_ref(&file)));
        assert_eq!(
            resolve(
                &resolver,
                snapshot("openai", active, env_file(), owned()),
                &openai
            )
            .await,
            Ok(Some("sk-file".to_owned()))
        );
        assert_eq!(
            resolve(
                &resolver,
                snapshot("openai", active, external("vault"), owned()),
                &openai
            )
            .await,
            Ok(Some("sk-vault".to_owned()))
        );

        let other_tenant = DataTenantId::new_v7();
        let compatible = |host: &str| {
            deployment(
                "deepseek",
                &json!({"openai_compatible": {"base_url": format!("https://{host}/v1")}}),
            )
        };
        let compat_assignment =
            || Some(assignment(TENANT, "deepseek", Some("API.deepseek.example")));
        assert_eq!(
            resolve(
                &resolver,
                snapshot("deepseek", active, env_file(), compat_assignment()),
                &compatible("api.deepseek.example")
            )
            .await,
            Ok(Some("sk-file".to_owned())),
            "exact assigned host receives the credential"
        );
        for (snap, target) in [
            (snapshot("openai", active, env_file(), None), openai.clone()),
            (
                snapshot(
                    "openai",
                    active,
                    env_file(),
                    Some(assignment(other_tenant, "openai", None)),
                ),
                openai.clone(),
            ),
            (
                snapshot(
                    "openai",
                    active,
                    env_file(),
                    Some(assignment(TENANT, "anthropic", None)),
                ),
                openai.clone(),
            ),
            (
                snapshot("deepseek", active, env_file(), compat_assignment()),
                compatible("evil.example"),
            ),
            (
                snapshot(
                    "deepseek",
                    active,
                    env_file(),
                    Some(assignment(TENANT, "deepseek", None)),
                ),
                compatible("api.deepseek.example"),
            ),
            (
                snapshot("openai", active, environment(None), owned()),
                openai.clone(),
            ),
            (
                snapshot("openai", active, external("absent"), owned()),
                openai.clone(),
            ),
        ] {
            assert_eq!(
                resolve(&resolver, snap, &target).await,
                Err(CredentialError::Unconfigured)
            );
        }
    }

    /// Unreadable, permissive, non-regular, and unreachable sources are
    /// unavailable; revoked, mismatched, and missing credentials fail closed;
    /// and a resolved secret's debug output is redacted.
    #[tokio::test]
    async fn credential_sources_fail_closed() {
        let token = secret_file("hvs.test");
        let resolver = resolver(&[("down", "http://127.0.0.1:9")], &token);
        let file = secret_file("sk-file");
        let openai = deployment("openai", &json!("openai"));
        let active = ProviderCredentialState::Active;
        let owned = || Some(assignment(TENANT, "openai", None));
        let env_file = || environment(Some(file_ref(&file)));
        let unset = SecretRef::Env {
            name: "WYRD_GATEWAY_CREDENTIAL_TEST_UNSET_VARIABLE".to_owned(),
        };
        let directory = tempfile::tempdir().expect("directory");
        let permissive = tempfile::NamedTempFile::new().expect("permissive file");
        write!(permissive.as_file(), "sk-leaky").expect("secret written");
        #[cfg(unix)]
        std::fs::set_permissions(permissive.path(), std::fs::Permissions::from_mode(0o644))
            .expect("mode set");
        for source in [
            environment(Some(unset)),
            environment(Some(file_ref(&permissive))),
            environment(Some(SecretRef::File {
                path: directory.path().to_string_lossy().into_owned(),
            })),
            external("down"),
        ] {
            assert_eq!(
                resolve(
                    &resolver,
                    snapshot("openai", active, source, owned()),
                    &openai
                )
                .await,
                Err(CredentialError::Unavailable)
            );
        }
        assert_eq!(
            resolve(
                &resolver,
                snapshot(
                    "openai",
                    ProviderCredentialState::Revoked,
                    env_file(),
                    owned()
                ),
                &openai
            )
            .await,
            Err(CredentialError::Revoked)
        );
        assert_eq!(
            resolve(
                &resolver,
                snapshot("anthropic", active, env_file(), owned()),
                &openai
            )
            .await,
            Err(CredentialError::ProviderMismatch)
        );
        let mut missing = snapshot("openai", active, env_file(), owned());
        missing.credentials.clear();
        assert_eq!(
            resolve(&resolver, missing, &openai).await,
            Err(CredentialError::Missing)
        );
        assert!(
            !format!("{:?}", ProviderSecret::new(SecretString::from("sk-live")))
                .contains("sk-live"),
            "debug output is redacted"
        );
    }
}
