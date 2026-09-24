//! Key-encryption keys (KEKs) for Operator connection credentials.
//!
//! Wyrd stores every Slack, PagerDuty, and HTTP credential in Postgres as
//! envelope ciphertext. [`OperatorKeys`] owns the only other half: reading the
//! 32-byte KEK that wraps each credential's data key from the configured
//! source (environment, owner-only file, or HashiCorp Vault KV v2), sealing
//! and opening secrets under the canonical context, and rewrapping rows onto
//! the active key version. Keys are read at use and never cached across
//! operations, so a rotated file or Vault secret takes effect on the next read.
//!
//! Failures leave this owner only as a [`KeyError`] carrying the source kind,
//! key version, and a stable [`KeyFailure`] class. No environment name, file
//! path, Vault address or selector, or raw provider/I/O text is kept, so no
//! error, response, or log line can disclose the deployment's secret layout.

use std::collections::HashMap;
use std::collections::hash_map::Entry;
#[cfg(unix)]
use std::os::unix::fs::PermissionsExt as _;
use std::path::{Path, PathBuf};
use std::time::Duration;

use base64::Engine as _;
use reqwest::Client;
use secrecy::ExposeSecret as _;
use serde_json::Value;
use sqlx::Error as SqlxError;
use tokio::time::Instant;
use wyrd_crypt::{EncryptedPayload, Envelope, SecretKey};
use wyrd_spec::DataTenantId;
use wyrd_spec::error::WyrdError;
use wyrd_spec::ids::{ConnectionName, OperatorConnectionId};
use wyrd_spec::operator_connection::{OperatorConnectionView, OperatorProvider};
use wyrd_sql::queries::operator_connections::{
    SealedSecret, StoredConnection, referenced_key_versions, rewrap_connection,
    stale_key_connections,
};
use wyrd_sql::queries::platform::tenants::list_active_tenant_ids;
use wyrd_sql::{OperatorPool, TenantConn, WyrdPostgres};
use zeroize::Zeroizing;

use crate::config::{BifrostTarget, OperatorKeySource, OperatorKeysConfig};

/// Deadline of one Vault key read.
const VAULT_TIMEOUT: Duration = Duration::from_secs(10);

/// Rows one tenant rewrap transaction moves at most.
const REWRAP_BATCH: i64 = 100;

/// The constant public detail of every key failure; the cause stays internal.
const UNAVAILABLE_DETAIL: &str =
    "the Operator key provider could not supply a usable key; retry once it is restored";

/// Stable, selector-free class of a key failure.
///
/// This is the only failure detail that leaves [`OperatorKeys`]: it names what
/// went wrong, never where the key lives or what the provider answered.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum KeyFailure {
    /// The source needs settings or a client that is absent.
    NotConfigured,
    /// The source holds no value for this tenant and version.
    Missing,
    /// A key or token file is readable beyond its owner.
    LoosePermissions,
    /// A key or token file exists but could not be read.
    Unreadable,
    /// The provider could not be reached or did not answer in time.
    Transport,
    /// The provider answered with a non-success status.
    Refused,
    /// The value is not base64, or the provider response has no `key` string.
    Malformed,
    /// The decoded key is not 32 bytes.
    WrongLength,
    /// Envelope sealing failed.
    Sealing,
    /// Postgres failed while reading or rewrapping rows.
    Database,
    /// A per-tenant or per-pass rewrap budget elapsed.
    TimedOut,
}

impl KeyFailure {
    /// The stable snake-case label logs and errors carry.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::NotConfigured => "not_configured",
            Self::Missing => "missing",
            Self::LoosePermissions => "loose_permissions",
            Self::Unreadable => "unreadable",
            Self::Transport => "transport",
            Self::Refused => "refused",
            Self::Malformed => "malformed",
            Self::WrongLength => "wrong_length",
            Self::Sealing => "sealing",
            Self::Database => "database",
            Self::TimedOut => "timed_out",
        }
    }
}

/// Why a key could not be produced or used. Never carries key bytes or a
/// secret selector.
#[derive(Debug, thiserror::Error)]
pub enum KeyError {
    /// The configured source has no usable key for this tenant and version.
    #[error("operator key version {version} from the {kind:?} source is unavailable: {}", failure.as_str())]
    Unavailable {
        /// Key version requested.
        version: i32,
        /// Configured source kind.
        kind: OperatorKeySource,
        /// Stable failure class.
        failure: KeyFailure,
    },
    /// The key opened nothing: wrong key, tampered row, or moved context.
    #[error("operator credential ciphertext did not authenticate under key version {version}")]
    Authentication {
        /// Key version used.
        version: i32,
    },
    /// The Vault HTTP client could not be built with its timeout and
    /// no-redirect policy, so key reads would run unbounded or follow
    /// redirects; construction refuses instead.
    #[error("the Operator key provider HTTP client could not be built")]
    Client,
    /// The configured active version exceeds the persisted `i32` key-version
    /// range, so no stored row could name it; construction refuses instead of
    /// narrowing it onto a different key.
    #[error("the Operator key active version exceeds the persisted key-version range")]
    VersionOutOfRange,
}

impl KeyError {
    /// Log this failure with bounded, selector-free fields only.
    fn log(&self, message: &'static str) {
        match self {
            Self::Unavailable {
                version,
                kind,
                failure,
            } => tracing::warn!(
                key_source = ?kind,
                key_version = version,
                failure = failure.as_str(),
                "{message}"
            ),
            Self::Authentication { version } => tracing::warn!(
                key_version = version,
                failure = "authentication",
                "{message}"
            ),
            Self::Client => tracing::warn!(failure = "client", "{message}"),
            Self::VersionOutOfRange => {
                tracing::warn!(failure = "version_out_of_range", "{message}");
            }
        }
    }
}

impl From<KeyError> for WyrdError {
    /// Map every key failure to the stable retryable unavailable code with a
    /// constant detail, logging only bounded fields.
    fn from(error: KeyError) -> Self {
        error.log("operator connection key unavailable");
        Self::OperatorKeyUnavailable {
            message: UNAVAILABLE_DETAIL.to_owned(),
            details: serde_json::json!({}),
        }
    }
}

/// Identity of one connection secret version: the canonical authenticated
/// context `(data_tenant_id, connection_id, provider, name, secret_version)`.
#[derive(Debug, Clone, Copy)]
pub struct SecretIdentity<'a> {
    /// Owning tenant.
    pub tenant: DataTenantId,
    /// Connection identity.
    pub connection_id: OperatorConnectionId,
    /// Connection provider.
    pub provider: OperatorProvider,
    /// Connection name.
    pub name: &'a ConnectionName,
    /// Secret version.
    pub secret_version: i32,
}

impl<'a> SecretIdentity<'a> {
    /// The identity of `view`'s secret version `secret_version`.
    #[must_use]
    pub fn of(tenant: DataTenantId, view: &'a OperatorConnectionView, secret_version: i32) -> Self {
        Self {
            tenant,
            connection_id: view.connection_id,
            provider: view.config.provider(),
            name: &view.name,
            secret_version,
        }
    }

    /// The canonical context strings `wyrd-crypt` authenticates.
    fn context(&self) -> [String; 5] {
        [
            self.tenant.to_string(),
            self.connection_id.to_string(),
            <&str>::from(self.provider).to_owned(),
            self.name.as_str().to_owned(),
            self.secret_version.to_string(),
        ]
    }
}

/// Borrow a context array as the `&[&str]` `wyrd-crypt` authenticates.
fn parts(context: &[String; 5]) -> [&str; 5] {
    context.each_ref().map(String::as_str)
}

/// Owner of Operator KEK reads, sealing, opening, and rewrap.
///
/// The derived `Debug` stays selector-free: the configuration prints through
/// its own redacting `Debug`, and the Vault client holds no default headers
/// because the token is attached per request.
#[derive(Debug)]
pub struct OperatorKeys {
    /// Configured source and active version.
    config: OperatorKeysConfig,
    /// The validated active version, exactly as configured.
    ///
    /// Invariant: [`OperatorKeys::new`] refuses any version above
    /// `i32::MAX`, so the persisted `i32` key version names the same external
    /// key the operator configured.
    active_version: i32,
    /// Bounded, non-redirecting HTTP client for Vault reads; built only for
    /// the Vault source.
    http: Option<Client>,
}

impl Default for OperatorKeys {
    /// The environment-sourced owner at key version 1; builds no client.
    fn default() -> Self {
        Self {
            config: OperatorKeysConfig::default(),
            active_version: 1,
            http: None,
        }
    }
}

impl OperatorKeys {
    /// Build the owner a server `role` uses.
    ///
    /// Only API-bearing roles serve Operator connections, so only they build
    /// the configured owner; a dedicated Forge worker keeps the unused default
    /// and never reads Operator key settings it does not own or validate.
    ///
    /// # Errors
    /// Returns any [`OperatorKeys::new`] error for an API-bearing role.
    pub(crate) fn for_role(
        role: BifrostTarget,
        config: &OperatorKeysConfig,
    ) -> Result<Self, KeyError> {
        if role.serves_api() {
            Self::new(config.clone())
        } else {
            Ok(Self::default())
        }
    }

    /// Build the owner over `config`.
    ///
    /// The Vault source gets a client with the read deadline and no redirect
    /// policy after installing Wyrd's Rustls provider, like every other
    /// server HTTP client; other sources build none.
    ///
    /// # Errors
    /// Returns [`KeyError::Client`] when the TLS provider cannot be installed
    /// or the client cannot be built, so a Vault read never runs without its
    /// timeout and redirect policy, and [`KeyError::VersionOutOfRange`] when
    /// `config.active_version` exceeds `i32::MAX` because the caller skipped
    /// [`OperatorKeysConfig`] validation.
    pub fn new(config: OperatorKeysConfig) -> Result<Self, KeyError> {
        let active_version =
            i32::try_from(config.active_version.get()).map_err(|_| KeyError::VersionOutOfRange)?;
        let http = match config.source {
            OperatorKeySource::Vault => {
                wyrd_tls::install_crypto_provider().map_err(|_| KeyError::Client)?;
                Some(
                    Client::builder()
                        .timeout(VAULT_TIMEOUT)
                        .redirect(reqwest::redirect::Policy::none())
                        .build()
                        .map_err(|_| KeyError::Client)?,
                )
            }
            OperatorKeySource::Env | OperatorKeySource::File => None,
        };
        Ok(Self {
            config,
            active_version,
            http,
        })
    }

    /// Version new and rotated secrets are wrapped under.
    #[must_use]
    pub const fn active_version(&self) -> i32 {
        self.active_version
    }

    /// A selector-free failure of `version` in this source.
    #[must_use]
    pub const fn unavailable(&self, version: i32, failure: KeyFailure) -> KeyError {
        KeyError::Unavailable {
            version,
            kind: self.config.source,
            failure,
        }
    }

    /// Read the KEK for `tenant` at `version` from the configured source.
    ///
    /// Env and file sources hold one deployment key per version; the tenant
    /// is still bound by the authenticated context. Vault holds one key per
    /// tenant and version. File reads run on Tokio's blocking pool, so a
    /// stalled mount never blocks an executor thread.
    ///
    /// # Errors
    /// Returns [`KeyError::Unavailable`] when the source is missing,
    /// unreadable, readable beyond its owner, unreachable, refusing, or holds
    /// a value that is not a base64 32-byte key.
    pub async fn key(&self, tenant: DataTenantId, version: i32) -> Result<SecretKey, KeyError> {
        let encoded = match self.config.source {
            OperatorKeySource::Env => Zeroizing::new(
                std::env::var(format!("WYRD_OPERATOR_KEK_V{version}"))
                    .map_err(|_| self.unavailable(version, KeyFailure::Missing))?,
            ),
            OperatorKeySource::File => {
                let dir = self.config.dir.as_deref().unwrap_or(Path::new("."));
                read_owner_only(dir.join(format!("v{version}")))
                    .await
                    .map_err(|failure| self.unavailable(version, failure))?
            }
            OperatorKeySource::Vault => self
                .vault_key(tenant, version)
                .await
                .map_err(|failure| self.unavailable(version, failure))?,
        };
        decode_key(&encoded).map_err(|failure| self.unavailable(version, failure))
    }

    /// Read `<mount>/data/<prefix>/<tenant>/<version>` field `key` from Vault.
    ///
    /// The token file passes the same owner-only check as key files before
    /// any request is sent.
    ///
    /// # Errors
    /// Returns the [`KeyFailure`] for a missing Vault section or token, an
    /// unreadable or loose token file, a transport failure, a 404 (missing)
    /// or other non-success status, or a response without a string `key`.
    async fn vault_key(
        &self,
        tenant: DataTenantId,
        version: i32,
    ) -> Result<Zeroizing<String>, KeyFailure> {
        let (Some(vault), Some(http)) = (&self.config.vault, &self.http) else {
            return Err(KeyFailure::NotConfigured);
        };
        let token = match (&vault.token_file, &vault.token) {
            (Some(path), _) => {
                let token = read_owner_only(path.clone()).await?;
                Zeroizing::new(token.trim().to_owned())
            }
            (None, Some(token)) => Zeroizing::new(token.expose_secret().to_owned()),
            (None, None) => return Err(KeyFailure::NotConfigured),
        };
        let url = format!(
            "{}/v1/{}/data/{}/{tenant}/{version}",
            vault.addr.trim_end_matches('/'),
            vault.mount.trim_matches('/'),
            vault.prefix.trim_matches('/')
        );
        let response = http
            .get(url)
            .header("X-Vault-Token", token.as_str())
            .send()
            .await
            .map_err(|_| KeyFailure::Transport)?;
        let status = response.status();
        if status == reqwest::StatusCode::NOT_FOUND {
            return Err(KeyFailure::Missing);
        }
        if !status.is_success() {
            return Err(KeyFailure::Refused);
        }
        let body: Value = response.json().await.map_err(|_| KeyFailure::Malformed)?;
        let key = body
            .pointer("/data/data/key")
            .and_then(Value::as_str)
            .ok_or(KeyFailure::Malformed)?;
        Ok(Zeroizing::new(key.to_owned()))
    }

    /// Prove the active key of every active provisioned tenant is readable
    /// and decodes to 32 bytes.
    ///
    /// Multi-tenant production boot calls this before the server can become
    /// ready, reusing [`key`](Self::key) and the platform tenant directory;
    /// the first failure refuses startup. Keys are read, checked, and
    /// dropped, never cached.
    ///
    /// # Errors
    /// Returns [`KeyFailure::Database`] when the directory read fails, and
    /// the first tenant's [`KeyError`] when its active key is unavailable,
    /// missing, malformed, or of the wrong length.
    pub async fn verify_active(&self, directory: &OperatorPool) -> Result<usize, KeyError> {
        let active = self.active_version();
        let tenants = list_active_tenant_ids(directory)
            .await
            .map_err(|_| self.unavailable(active, KeyFailure::Database))?;
        for tenant in &tenants {
            self.key(*tenant, active).await?;
        }
        Ok(tenants.len())
    }

    /// Seal `plaintext` as the secret version `identity` names under the
    /// active key version.
    ///
    /// # Errors
    /// Returns [`WyrdError::OperatorKeyUnavailable`] with a constant detail
    /// when the active key cannot be read or sealing fails.
    pub async fn seal(
        &self,
        identity: SecretIdentity<'_>,
        plaintext: &[u8],
    ) -> Result<SealedSecret, WyrdError> {
        let key_version = self.active_version();
        let kek = self.key(identity.tenant, key_version).await?;
        let context = identity.context();
        let envelope = wyrd_crypt::seal(&kek, plaintext, &parts(&context))
            .map_err(|_| WyrdError::from(self.unavailable(key_version, KeyFailure::Sealing)))?;
        Ok(SealedSecret {
            ciphertext: envelope.secret.ciphertext,
            nonce: envelope.secret.nonce,
            wrapped_dek: envelope.wrapped_dek.ciphertext,
            dek_nonce: envelope.wrapped_dek.nonce,
            key_version,
        })
    }

    /// Open the current secret of `stored` for one delivery attempt.
    ///
    /// # Errors
    /// Returns [`KeyError::Unavailable`] when its key version cannot be read
    /// and [`KeyError::Authentication`] when the row does not authenticate.
    pub async fn open(
        &self,
        tenant: DataTenantId,
        stored: &StoredConnection,
    ) -> Result<Zeroizing<Vec<u8>>, KeyError> {
        let version = stored.sealed.key_version;
        let kek = self.key(tenant, version).await?;
        let context = SecretIdentity::of(tenant, &stored.view, stored.secret_version).context();
        wyrd_crypt::open(&kek, &envelope(&stored.sealed), &parts(&context))
            .map_err(|_| KeyError::Authentication { version })
    }

    /// Rewrap up to [`REWRAP_BATCH`] of the tenant's rows still on an older
    /// key version onto the active one, without decrypting any credential.
    ///
    /// Rows are locked `SKIP LOCKED`, so concurrent replicas split the work,
    /// and each write is fenced on the secret version it read, so a racing
    /// rotation wins. Each distinct old version is read once per call and
    /// dropped when it returns, so a batch of rows on one old version costs
    /// one provider read. Nothing commits here; the caller owns `conn`.
    ///
    /// # Errors
    /// Returns [`KeyError`] when a key cannot be read or a row does not
    /// authenticate, and [`KeyFailure::Database`] when a read or write fails.
    pub async fn rewrap_tenant(
        &self,
        conn: &mut TenantConn<'_>,
        tenant: DataTenantId,
    ) -> Result<usize, KeyError> {
        let active = self.active_version();
        let database = |_: SqlxError| self.unavailable(active, KeyFailure::Database);
        let stale = stale_key_connections(conn, active, REWRAP_BATCH)
            .await
            .map_err(database)?;
        if stale.is_empty() {
            return Ok(0);
        }
        let new_kek = self.key(tenant, active).await?;
        let mut old_keks: HashMap<i32, SecretKey> = HashMap::new();
        let mut moved = 0;
        for row in &stale {
            let old_version = row.sealed.key_version;
            let old_kek = match old_keks.entry(old_version) {
                Entry::Occupied(cached) => cached.into_mut(),
                Entry::Vacant(slot) => slot.insert(self.key(tenant, old_version).await?),
            };
            let context = SecretIdentity::of(tenant, &row.view, row.secret_version).context();
            let wrapped = wyrd_crypt::rewrap(
                old_kek,
                &new_kek,
                &envelope(&row.sealed).wrapped_dek,
                &parts(&context),
            )
            .map_err(|_| KeyError::Authentication {
                version: old_version,
            })?;
            if rewrap_connection(
                conn,
                row.view.connection_id,
                row.secret_version,
                &wrapped.ciphertext,
                &wrapped.nonce,
                active,
            )
            .await
            .map_err(database)?
            {
                moved += 1;
            }
        }
        Ok(moved)
    }

    /// One bounded rotation pass over every tenant with rows on an older key
    /// version: one tenant transaction and at most [`REWRAP_BATCH`] rows each.
    ///
    /// The whole pass, including the cross-tenant discovery read, runs under
    /// `pass_budget`, and each tenant under `tenant_budget` capped by what
    /// remains; an overrun drops the in-flight future, which cancels a stalled
    /// discovery before any row is touched or rolls back the tenant
    /// transaction. A tenant whose keys are unavailable or whose budget
    /// elapsed is logged with selector-free fields and skipped so it cannot
    /// stall the others; the next pass retries it. Dropping the returned
    /// future (on shutdown) likewise rolls back the tenant in progress.
    /// Returns the rows moved.
    ///
    /// # Errors
    /// Returns the database error when the cross-tenant discovery read fails.
    pub async fn rewrap_pass(
        &self,
        postgres: &WyrdPostgres,
        operator: &OperatorPool,
        pass_budget: Duration,
        tenant_budget: Duration,
    ) -> Result<usize, SqlxError> {
        let deadline = Instant::now() + pass_budget;
        let active = self.active_version();
        let Ok(discovered) =
            tokio::time::timeout_at(deadline, referenced_key_versions(operator)).await
        else {
            pass_elapsed();
            return Ok(0);
        };
        let mut tenants: Vec<DataTenantId> = discovered?
            .into_iter()
            .filter(|(_, version)| *version != active)
            .map(|(tenant, _)| tenant)
            .collect();
        tenants.dedup();
        let mut moved = 0;
        for tenant in tenants {
            let remaining = deadline.saturating_duration_since(Instant::now());
            if remaining.is_zero() {
                pass_elapsed();
                break;
            }
            let rewrapped = tokio::time::timeout(tenant_budget.min(remaining), async {
                let database = |_| self.unavailable(active, KeyFailure::Database);
                let mut conn = postgres.tenant_conn(tenant).await.map_err(database)?;
                let count = self.rewrap_tenant(&mut conn, tenant).await?;
                conn.commit().await.map_err(database)?;
                Ok::<_, KeyError>(count)
            })
            .await
            .unwrap_or_else(|_| Err(self.unavailable(active, KeyFailure::TimedOut)));
            match rewrapped {
                Ok(count) => moved += count,
                Err(error) => tracing::info_span!("operator_key_rewrap", %tenant)
                    .in_scope(|| error.log("operator key rewrap skipped a tenant this pass")),
            }
        }
        Ok(moved)
    }
}

/// Log, with the selector-free timeout class only, that a rewrap pass ran out
/// of budget and left the remaining work to the next pass.
fn pass_elapsed() {
    tracing::warn!(
        failure = KeyFailure::TimedOut.as_str(),
        "operator key rewrap pass budget elapsed; the next pass continues"
    );
}

/// Rebuild the `wyrd-crypt` envelope of a stored sealed secret.
fn envelope(sealed: &SealedSecret) -> Envelope {
    Envelope {
        secret: EncryptedPayload {
            nonce: sealed.nonce,
            ciphertext: sealed.ciphertext.clone(),
        },
        wrapped_dek: EncryptedPayload {
            nonce: sealed.dek_nonce,
            ciphertext: sealed.wrapped_dek.clone(),
        },
    }
}

/// Read a key or token file that only its owner may read, on Tokio's
/// blocking pool so a stalled mount never occupies an executor thread.
///
/// # Errors
/// Returns [`KeyFailure::Missing`] when the file does not exist,
/// [`KeyFailure::LoosePermissions`] when group or others may read it, and
/// [`KeyFailure::Unreadable`] for any other read failure.
async fn read_owner_only(path: PathBuf) -> Result<Zeroizing<String>, KeyFailure> {
    tokio::task::spawn_blocking(move || read_owner_only_blocking(&path))
        .await
        .map_err(|_| KeyFailure::Unreadable)?
}

/// Blocking body of [`read_owner_only`]: check the mode, then read.
///
/// # Errors
/// As [`read_owner_only`].
fn read_owner_only_blocking(path: &Path) -> Result<Zeroizing<String>, KeyFailure> {
    let io = |error: std::io::Error| match error.kind() {
        std::io::ErrorKind::NotFound => KeyFailure::Missing,
        _ => KeyFailure::Unreadable,
    };
    #[cfg(unix)]
    if std::fs::metadata(path).map_err(io)?.permissions().mode() & 0o077 != 0 {
        return Err(KeyFailure::LoosePermissions);
    }
    std::fs::read_to_string(path)
        .map(Zeroizing::new)
        .map_err(io)
}

/// Decode a base64 32-byte key, trimming surrounding whitespace, with the
/// decoded bytes zeroized on drop.
///
/// Operator KEK reads and the server's auth sealing key share this decoder.
///
/// # Errors
/// Returns [`KeyFailure::Malformed`] for a value that is not base64 and
/// [`KeyFailure::WrongLength`] for one that is not 32 bytes.
pub(crate) fn decode_key(encoded: &str) -> Result<SecretKey, KeyFailure> {
    let bytes = Zeroizing::new(
        base64::engine::general_purpose::STANDARD
            .decode(encoded.trim())
            .map_err(|_| KeyFailure::Malformed)?,
    );
    let key: [u8; 32] = bytes
        .as_slice()
        .try_into()
        .map_err(|_| KeyFailure::WrongLength)?;
    Ok(SecretKey::from_bytes(key))
}

#[cfg(test)]
mod tests {
    //! Key decoding, file permissions, selector-free failures, Vault reads,
    //! nonblocking file reads, sealing, and context binding.

    use std::io::{self, Write};
    use std::num::NonZeroU32;
    use std::sync::{Arc, Mutex};

    use chrono::Utc;
    use secrecy::SecretString;
    use tracing_subscriber::fmt::MakeWriter;
    use uuid::Uuid;
    use wiremock::matchers::{header, method, path};
    use wiremock::{Mock, MockServer, ResponseTemplate};
    use wyrd_spec::operator_connection::{OperatorConnectionConfig, OperatorConnectionStatus};

    use super::*;
    use crate::config::VaultKeysConfig;

    /// A redacted PagerDuty view fixture.
    fn view() -> OperatorConnectionView {
        OperatorConnectionView {
            connection_id: OperatorConnectionId::new_v7(),
            name: ConnectionName::new("pagerduty").expect("valid name"),
            config: OperatorConnectionConfig::PagerDuty {},
            status: OperatorConnectionStatus::Active,
            created_at: Utc::now(),
            updated_at: Utc::now(),
        }
    }

    /// A file-sourced owner over `dir` at `active`.
    fn file_keys(dir: &Path, active: u32) -> OperatorKeys {
        OperatorKeys::new(OperatorKeysConfig {
            source: OperatorKeySource::File,
            active_version: NonZeroU32::new(active).expect("positive"),
            dir: Some(dir.to_path_buf()),
            vault: None,
        })
        .expect("file keys build")
    }

    /// A Vault-sourced owner at `addr` whose token is read from `token_file`.
    fn vault_keys(addr: &str, token_file: &Path) -> OperatorKeys {
        OperatorKeys::new(OperatorKeysConfig {
            source: OperatorKeySource::Vault,
            vault: Some(VaultKeysConfig {
                addr: addr.to_owned(),
                mount: "secret".to_owned(),
                prefix: "wyrd/operator-keys".to_owned(),
                token_file: Some(token_file.to_path_buf()),
                token: None,
            }),
            ..OperatorKeysConfig::default()
        })
        .expect("vault keys build")
    }

    /// Write `contents` to `path` with `mode`.
    fn write_mode(path: &Path, contents: &str, mode: u32) {
        std::fs::write(path, contents).expect("file writes");
        std::fs::set_permissions(path, std::fs::Permissions::from_mode(mode)).expect("chmod");
    }

    /// Write key `byte` as version `version`, owner-only.
    fn write_key(dir: &Path, version: u32, byte: u8) {
        write_mode(
            &dir.join(format!("v{version}")),
            &base64::engine::general_purpose::STANDARD.encode([byte; 32]),
            0o600,
        );
    }

    /// The failure class of a key read.
    fn failure_of(result: Result<SecretKey, KeyError>) -> KeyFailure {
        match result {
            Err(KeyError::Unavailable { failure, .. }) => failure,
            Err(other) => panic!("unexpected key error {other}"),
            Ok(_) => panic!("the key read unexpectedly succeeded"),
        }
    }

    /// In-memory log sink for one thread's subscriber.
    #[derive(Clone, Default)]
    struct Captured(Arc<Mutex<Vec<u8>>>);

    impl Captured {
        /// Everything logged so far.
        ///
        /// # Panics
        /// Panics when the sink lock is poisoned.
        fn text(&self) -> String {
            String::from_utf8_lossy(&self.0.lock().expect("log sink")).into_owned()
        }
    }

    impl Write for Captured {
        /// Append `buf` to the sink.
        fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
            self.0.lock().expect("log sink").extend_from_slice(buf);
            Ok(buf.len())
        }

        /// Nothing is buffered.
        fn flush(&mut self) -> io::Result<()> {
            Ok(())
        }
    }

    impl<'a> MakeWriter<'a> for Captured {
        type Writer = Self;

        /// A handle onto the shared sink.
        fn make_writer(&'a self) -> Self::Writer {
            self.clone()
        }
    }

    /// Only a base64 32-byte key decodes.
    #[test]
    fn decode_key_requires_32_base64_bytes() {
        let engine = base64::engine::general_purpose::STANDARD;
        assert!(decode_key(&format!("{}\n", engine.encode([1_u8; 32]))).is_ok());
        assert!(matches!(
            decode_key(&engine.encode([1_u8; 31])),
            Err(KeyFailure::WrongLength)
        ));
        assert!(matches!(
            decode_key("not base64!"),
            Err(KeyFailure::Malformed)
        ));
    }

    /// A sealed secret opens under the same tenant and version, fails for
    /// another tenant, a group-readable key file is refused, a missing key
    /// keeps the stable code, and rewrap moves a secret to a new key version
    /// without changing what it opens to.
    ///
    /// # Panics
    /// Panics when any key or context binding breaks.
    #[tokio::test]
    async fn file_keys_seal_open_and_bind_context() {
        let dir = tempfile::tempdir().expect("tempdir");
        let keys = file_keys(dir.path(), 1);
        let tenant = DataTenantId::new(Uuid::now_v7()).expect("tenant");
        let view = view();

        let missing = keys
            .seal(SecretIdentity::of(tenant, &view, 1), b"secret")
            .await
            .unwrap_err();
        assert_eq!(missing.code(), "WYRD_OPERATOR_503_KEY_UNAVAILABLE");

        write_key(dir.path(), 1, 7);
        let sealed = keys
            .seal(SecretIdentity::of(tenant, &view, 1), b"secret")
            .await
            .expect("seals");
        assert_eq!(sealed.key_version, 1);
        let stored = StoredConnection {
            view: view.clone(),
            sealed,
            secret_version: 1,
        };
        assert_eq!(
            keys.open(tenant, &stored).await.expect("opens").as_slice(),
            b"secret"
        );
        let other = DataTenantId::new(Uuid::now_v7()).expect("tenant");
        assert!(matches!(
            keys.open(other, &stored).await,
            Err(KeyError::Authentication { version: 1 })
        ));
        let moved = StoredConnection {
            secret_version: 2,
            ..stored.clone()
        };
        assert!(
            keys.open(tenant, &moved).await.is_err(),
            "secret version is authenticated"
        );

        std::fs::set_permissions(
            dir.path().join("v1"),
            std::fs::Permissions::from_mode(0o640),
        )
        .expect("chmod");
        assert_eq!(
            failure_of(keys.key(tenant, 1).await),
            KeyFailure::LoosePermissions
        );
        std::fs::set_permissions(
            dir.path().join("v1"),
            std::fs::Permissions::from_mode(0o600),
        )
        .expect("chmod");

        write_key(dir.path(), 2, 9);
        let rotated = file_keys(dir.path(), 2);
        let context = SecretIdentity::of(tenant, &view, 1).context();
        let wrapped = wyrd_crypt::rewrap(
            &rotated.key(tenant, 1).await.expect("old key"),
            &rotated.key(tenant, 2).await.expect("new key"),
            &envelope(&stored.sealed).wrapped_dek,
            &parts(&context),
        )
        .expect("rewraps");
        let mut sealed = stored.sealed.clone();
        sealed.wrapped_dek = wrapped.ciphertext;
        sealed.dek_nonce = wrapped.nonce;
        sealed.key_version = 2;
        let rewrapped = StoredConnection { sealed, ..stored };
        assert_eq!(
            rotated
                .open(tenant, &rewrapped)
                .await
                .expect("opens")
                .as_slice(),
            b"secret"
        );
    }

    /// Env, file, and Vault failures behind sentinel selectors surface only
    /// the stable code and constant detail, the complete public problem names
    /// no generic key-location template, and the captured logs carry no
    /// selector, token, or provider text.
    ///
    /// # Panics
    /// Panics when a sentinel reaches the public error or the logs.
    #[tokio::test]
    async fn key_failures_disclose_no_selector() {
        let captured = Captured::default();
        let _logs = tracing::subscriber::set_default(
            tracing_subscriber::fmt()
                .with_writer(captured.clone())
                .with_ansi(false)
                .finish(),
        );
        let root = tempfile::tempdir().expect("tempdir");
        let sentinel_dir = root.path().join("sentinel-kek-dir");
        std::fs::create_dir(&sentinel_dir).expect("dir");
        let vault = MockServer::start().await;
        Mock::given(method("GET"))
            .respond_with(ResponseTemplate::new(403).set_body_string("sentinel-provider-text"))
            .mount(&vault)
            .await;
        let token = root.path().join("sentinel-token-file");
        write_mode(&token, "sentinel-token-value", 0o600);
        let owners = [
            OperatorKeys::new(OperatorKeysConfig {
                active_version: NonZeroU32::new(907_311).expect("positive"),
                ..OperatorKeysConfig::default()
            })
            .expect("env keys build"),
            file_keys(&sentinel_dir, 1),
            OperatorKeys::new(OperatorKeysConfig {
                source: OperatorKeySource::Vault,
                vault: Some(VaultKeysConfig {
                    addr: vault.uri(),
                    mount: "sentinel-mount".to_owned(),
                    prefix: "sentinel-prefix".to_owned(),
                    token_file: None,
                    token: Some(SecretString::from("sentinel-inline-token")),
                }),
                ..OperatorKeysConfig::default()
            })
            .expect("vault keys build"),
            vault_keys(&vault.uri(), &token),
        ];
        let tenant = DataTenantId::new(Uuid::now_v7()).expect("tenant");
        let view = view();
        let mut public = String::new();
        for keys in &owners {
            let error = keys
                .seal(
                    SecretIdentity::of(tenant, &view, 1),
                    b"sentinel-secret-bytes",
                )
                .await
                .expect_err("no key is readable");
            assert_eq!(error.code(), "WYRD_OPERATOR_503_KEY_UNAVAILABLE");
            assert_eq!(error.status(), 503, "unavailable keys stay retryable");
            public.push_str(&error.to_string());
            public.push_str(&error.as_problem_json().to_string());
        }
        assert_eq!(vault.received_requests().await.expect("recorded").len(), 2);
        for template in [
            "WYRD_OPERATOR_KEK",
            "operator_keys",
            "<dir>",
            "<mount>",
            "<prefix>",
            "<version>",
            "Vault",
            "KV v2",
        ] {
            assert!(
                !public.contains(template),
                "{template} leaked into the public problem: {public}"
            );
        }
        let logs = captured.text();
        assert!(logs.contains("failure="), "{logs}");
        for text in [&public, &logs] {
            for sentinel in [
                "WYRD_OPERATOR_KEK_V907311",
                "sentinel-kek-dir",
                "sentinel-mount",
                "sentinel-prefix",
                "sentinel-token",
                "sentinel-inline-token",
                "sentinel-provider-text",
                "sentinel-secret-bytes",
                &vault.uri(),
                &tenant.to_string(),
            ] {
                assert!(!text.contains(sentinel), "{sentinel} leaked: {text}");
            }
        }
    }

    /// Direct construction keeps `i32::MAX` exact and refuses one past it
    /// with the selector-free typed error instead of unwinding, while a
    /// dedicated Forge worker ignores the oversized setting it does not own
    /// and keeps the default owner without reading any key source.
    #[test]
    fn oversized_active_version_is_typed_or_ignored_by_role() {
        let config = |version: u32| OperatorKeysConfig {
            active_version: NonZeroU32::new(version).expect("positive"),
            ..OperatorKeysConfig::default()
        };
        let at_max = OperatorKeys::new(config(2_147_483_647)).expect("i32::MAX builds");
        assert_eq!(at_max.active_version(), i32::MAX, "the version is exact");

        let oversized = config(2_147_483_648);
        assert!(matches!(
            OperatorKeys::new(oversized.clone()),
            Err(KeyError::VersionOutOfRange)
        ));
        assert!(matches!(
            OperatorKeys::for_role(BifrostTarget::Server, &oversized),
            Err(KeyError::VersionOutOfRange)
        ));
        let forge = OperatorKeys::for_role(BifrostTarget::ForgeWorker, &oversized)
            .expect("a Forge worker keeps the default owner");
        assert_eq!(forge.active_version(), 1);
    }

    /// Vault reads send the owner-only token file's value and decode the key;
    /// a group- or other-readable token file fails before any request, and
    /// missing, malformed, and wrong-length values keep distinct classes.
    ///
    /// # Panics
    /// Panics when a read succeeds or fails differently.
    #[tokio::test]
    async fn vault_reads_require_an_owner_only_token_file() {
        let root = tempfile::tempdir().expect("tempdir");
        let token = root.path().join("token");
        let vault = MockServer::start().await;
        let tenant = DataTenantId::new(Uuid::now_v7()).expect("tenant");
        let engine = base64::engine::general_purpose::STANDARD;
        let respond = |key: String| {
            ResponseTemplate::new(200)
                .set_body_json(serde_json::json!({ "data": { "data": { "key": key } } }))
        };
        for (version, key) in [
            (1, engine.encode([3_u8; 32])),
            (2, "not base64!".to_owned()),
            (3, engine.encode([3_u8; 16])),
        ] {
            Mock::given(method("GET"))
                .and(path(format!(
                    "/v1/secret/data/wyrd/operator-keys/{tenant}/{version}"
                )))
                .and(header("X-Vault-Token", "vault-token"))
                .respond_with(respond(key))
                .mount(&vault)
                .await;
        }
        Mock::given(method("GET"))
            .respond_with(ResponseTemplate::new(404))
            .mount(&vault)
            .await;
        let keys = vault_keys(&vault.uri(), &token);

        for mode in [0o640, 0o604] {
            write_mode(&token, "vault-token\n", mode);
            assert_eq!(
                failure_of(keys.key(tenant, 1).await),
                KeyFailure::LoosePermissions
            );
        }
        assert!(
            vault
                .received_requests()
                .await
                .expect("recorded")
                .is_empty(),
            "a loose token file fails before network use"
        );
        write_mode(&token, "vault-token\n", 0o600);
        assert!(keys.key(tenant, 1).await.is_ok());
        assert_eq!(failure_of(keys.key(tenant, 2).await), KeyFailure::Malformed);
        assert_eq!(
            failure_of(keys.key(tenant, 3).await),
            KeyFailure::WrongLength
        );
        assert_eq!(failure_of(keys.key(tenant, 4).await), KeyFailure::Missing);
    }

    /// A key file that stalls (a FIFO with no writer) holds only a blocking
    /// pool thread: unrelated work on the single executor thread still runs,
    /// and the read completes once the source answers.
    ///
    /// # Panics
    /// Panics when unrelated work is blocked or the read fails.
    #[tokio::test(flavor = "current_thread")]
    async fn stalled_key_file_does_not_block_the_executor() {
        let dir = tempfile::tempdir().expect("tempdir");
        let fifo = dir.path().join("v1");
        let made = std::process::Command::new("mkfifo")
            .arg("-m")
            .arg("600")
            .arg(&fifo)
            .status()
            .expect("mkfifo runs");
        assert!(made.success(), "mkfifo creates the stalled source");
        let keys = Arc::new(file_keys(dir.path(), 1));
        let tenant = DataTenantId::new(Uuid::now_v7()).expect("tenant");
        let reading = tokio::spawn({
            let keys = Arc::clone(&keys);
            async move { keys.key(tenant, 1).await }
        });
        let unrelated = tokio::time::timeout(Duration::from_secs(5), tokio::spawn(async { 7 }))
            .await
            .expect("unrelated work runs while the key file stalls")
            .expect("unrelated task completes");
        assert_eq!(unrelated, 7);
        tokio::time::sleep(Duration::from_millis(50)).await;
        assert!(!reading.is_finished(), "the key read is still stalled");
        let writer = std::thread::spawn(move || {
            let mut pipe = std::fs::OpenOptions::new()
                .write(true)
                .open(&fifo)
                .expect("fifo opens for write");
            pipe.write_all(
                base64::engine::general_purpose::STANDARD
                    .encode([5_u8; 32])
                    .as_bytes(),
            )
            .expect("key writes");
        });
        let read = tokio::time::timeout(Duration::from_secs(5), reading)
            .await
            .expect("the read completes once the source answers")
            .expect("the read task does not panic");
        writer.join().expect("writer finishes");
        assert!(read.is_ok());
    }
}
