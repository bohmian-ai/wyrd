//! Key-encryption keys (KEKs) for Operator connection credentials.
//!
//! Wyrd stores every Slack, PagerDuty, and HTTP credential in Postgres as
//! envelope ciphertext. [`OperatorKeys`] owns the only other half: reading the
//! 32-byte KEK that wraps each credential's data key from the configured
//! source (environment, owner-only file, or HashiCorp Vault KV v2), sealing
//! and opening secrets under the canonical context, and rewrapping rows onto
//! the active key version. Keys are read at use and never cached, so a
//! rotated file or Vault secret takes effect on the next read.

use std::path::Path;
use std::time::Duration;

use base64::Engine as _;
use secrecy::ExposeSecret as _;
use wyrd_crypt::{EncryptedPayload, Envelope, SecretKey};
use wyrd_spec::DataTenantId;
use wyrd_spec::error::WyrdError;
use wyrd_spec::ids::{ConnectionName, OperatorConnectionId};
use wyrd_spec::operator_connection::{OperatorConnectionView, OperatorProvider};
use wyrd_sql::queries::operator_connections::{
    SealedSecret, StoredConnection, referenced_key_versions, rewrap_connection,
    stale_key_connections,
};
use wyrd_sql::{OperatorPool, TenantConn, WyrdPostgres};
use zeroize::Zeroizing;

use crate::config::{OperatorKeySource, OperatorKeysConfig};

/// Deadline of one Vault key read.
const VAULT_TIMEOUT: Duration = Duration::from_secs(10);

/// Rows one tenant rewrap transaction moves at most.
pub const REWRAP_BATCH: i64 = 100;

/// Why a key could not be produced or used. Never carries key bytes.
#[derive(Debug, thiserror::Error)]
pub enum KeyError {
    /// The configured source has no usable key for this tenant and version.
    #[error("operator key version {version} is unavailable from {location}: {reason}")]
    Unavailable {
        /// Key version requested.
        version: i32,
        /// Where the key was expected, naming the setting, never its value.
        location: String,
        /// Why it could not be used.
        reason: String,
    },
    /// The key opened nothing: wrong key, tampered row, or moved context.
    #[error("operator credential ciphertext did not authenticate under key version {version}")]
    Authentication {
        /// Key version used.
        version: i32,
    },
}

impl From<KeyError> for WyrdError {
    fn from(error: KeyError) -> Self {
        tracing::warn!(%error, "operator connection key unavailable");
        Self::OperatorKeyUnavailable {
            message: error.to_string(),
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
#[derive(Debug)]
pub struct OperatorKeys {
    /// Configured source and active version.
    config: OperatorKeysConfig,
    /// HTTP client for Vault reads; unused by other sources.
    http: reqwest::Client,
}

impl OperatorKeys {
    /// Build the owner over `config`.
    #[must_use]
    pub fn new(config: OperatorKeysConfig) -> Self {
        let http = reqwest::Client::builder()
            .timeout(VAULT_TIMEOUT)
            .redirect(reqwest::redirect::Policy::none())
            .build()
            .unwrap_or_default();
        Self { config, http }
    }

    /// Version new and rotated secrets are wrapped under.
    #[must_use]
    pub fn active_version(&self) -> i32 {
        i32::try_from(self.config.active_version.get()).unwrap_or(i32::MAX)
    }

    /// Read the KEK for `tenant` at `version` from the configured source.
    ///
    /// Env and file sources hold one deployment key per version; the tenant
    /// is still bound by the authenticated context. Vault holds one key per
    /// tenant and version.
    ///
    /// # Errors
    /// Returns [`KeyError::Unavailable`] when the source is unreadable, the
    /// file is readable by group or others, or the value is not a base64
    /// 32-byte key.
    pub async fn key(&self, tenant: DataTenantId, version: i32) -> Result<SecretKey, KeyError> {
        let unavailable = |location: String, reason: String| KeyError::Unavailable {
            version,
            location,
            reason,
        };
        let (location, encoded) = match self.config.source {
            OperatorKeySource::Env => {
                let name = format!("WYRD_OPERATOR_KEK_V{version}");
                let value = std::env::var(&name)
                    .map_err(|_| unavailable(name.clone(), "not set".to_owned()))?;
                (name, Zeroizing::new(value))
            }
            OperatorKeySource::File => {
                let dir = self.config.dir.as_deref().unwrap_or(Path::new("."));
                let path = dir.join(format!("v{version}"));
                let location = path.display().to_string();
                let value = read_owner_only(&path)
                    .map_err(|reason| unavailable(location.clone(), reason))?;
                (location, value)
            }
            OperatorKeySource::Vault => self.vault_key(tenant, version).await?,
        };
        decode_key(&encoded).map_err(|reason| unavailable(location, reason))
    }

    /// Read `<mount>/data/<prefix>/<tenant>/<version>` field `key` from Vault.
    ///
    /// # Errors
    /// Returns [`KeyError::Unavailable`] for a missing Vault section, an
    /// unreadable token, a transport failure, a non-success status, or a
    /// response without a string `key`.
    async fn vault_key(
        &self,
        tenant: DataTenantId,
        version: i32,
    ) -> Result<(String, Zeroizing<String>), KeyError> {
        let unavailable = |location: &str, reason: String| KeyError::Unavailable {
            version,
            location: location.to_owned(),
            reason,
        };
        let Some(vault) = &self.config.vault else {
            return Err(unavailable(
                "verification.operator_keys.vault",
                "not configured".to_owned(),
            ));
        };
        let location = format!(
            "vault {}/data/{}/{tenant}/{version}",
            vault.mount.trim_matches('/'),
            vault.prefix.trim_matches('/')
        );
        let token = match (&vault.token_file, &vault.token) {
            (Some(path), _) => Zeroizing::new(
                std::fs::read_to_string(path)
                    .map_err(|error| unavailable(&location, format!("token file: {error}")))?
                    .trim()
                    .to_owned(),
            ),
            (None, Some(token)) => Zeroizing::new(token.expose_secret().to_owned()),
            (None, None) => return Err(unavailable(&location, "no Vault token".to_owned())),
        };
        let url = format!(
            "{}/v1/{}",
            vault.addr.trim_end_matches('/'),
            location.trim_start_matches("vault ")
        );
        let response = self
            .http
            .get(url)
            .header("X-Vault-Token", token.as_str())
            .send()
            .await
            .map_err(|error| {
                unavailable(
                    &location,
                    format!("request failed: {}", error.without_url()),
                )
            })?;
        let status = response.status();
        if !status.is_success() {
            return Err(unavailable(&location, format!("Vault answered {status}")));
        }
        let body: serde_json::Value = response.json().await.map_err(|error| {
            unavailable(
                &location,
                format!("unreadable response: {}", error.without_url()),
            )
        })?;
        let key = body
            .pointer("/data/data/key")
            .and_then(serde_json::Value::as_str)
            .ok_or_else(|| unavailable(&location, "secret has no string field `key`".to_owned()))?;
        Ok((location, Zeroizing::new(key.to_owned())))
    }

    /// Seal `plaintext` as the secret version `identity` names under the
    /// active key version.
    ///
    /// # Errors
    /// Returns [`WyrdError::OperatorKeyUnavailable`] when the active key
    /// cannot be read or sealing fails.
    pub async fn seal(
        &self,
        identity: SecretIdentity<'_>,
        plaintext: &[u8],
    ) -> Result<SealedSecret, WyrdError> {
        let key_version = self.active_version();
        let kek = self.key(identity.tenant, key_version).await?;
        let context = identity.context();
        let envelope = wyrd_crypt::seal(&kek, plaintext, &parts(&context)).map_err(|error| {
            WyrdError::from(KeyError::Unavailable {
                version: key_version,
                location: "wyrd-crypt".to_owned(),
                reason: error.to_string(),
            })
        })?;
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
    /// rotation wins. Nothing commits here; the caller owns `conn`.
    ///
    /// # Errors
    /// Returns [`KeyError`] when a key cannot be read or a row does not
    /// authenticate, and the database error text as
    /// [`KeyError::Unavailable`] when a read or write fails.
    pub async fn rewrap_tenant(
        &self,
        conn: &mut TenantConn<'_>,
        tenant: DataTenantId,
    ) -> Result<usize, KeyError> {
        let active = self.active_version();
        let database = |error: sqlx::Error| KeyError::Unavailable {
            version: active,
            location: "wyrd.operator_connections".to_owned(),
            reason: error.to_string(),
        };
        let stale = stale_key_connections(conn, active, REWRAP_BATCH)
            .await
            .map_err(database)?;
        let new_kek = self.key(tenant, active).await?;
        let mut moved = 0;
        for row in &stale {
            let old_version = row.sealed.key_version;
            let old_kek = self.key(tenant, old_version).await?;
            let context = SecretIdentity::of(tenant, &row.view, row.secret_version).context();
            let wrapped = wyrd_crypt::rewrap(
                &old_kek,
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
    /// A tenant whose keys are unavailable is logged and skipped so it cannot
    /// stall the others; the next pass retries it. Returns the rows moved.
    ///
    /// # Errors
    /// Returns the database error when the cross-tenant discovery read fails.
    pub async fn rewrap_pass(
        &self,
        postgres: &WyrdPostgres,
        operator: &OperatorPool,
    ) -> Result<usize, sqlx::Error> {
        let active = self.active_version();
        let mut tenants: Vec<DataTenantId> = referenced_key_versions(operator)
            .await?
            .into_iter()
            .filter(|(_, version)| *version != active)
            .map(|(tenant, _)| tenant)
            .collect();
        tenants.dedup();
        let mut moved = 0;
        for tenant in tenants {
            let result = async {
                let mut conn = postgres
                    .tenant_conn(tenant)
                    .await
                    .map_err(|error| error.to_string())?;
                let count = self
                    .rewrap_tenant(&mut conn, tenant)
                    .await
                    .map_err(|error| error.to_string())?;
                conn.commit().await.map_err(|error| error.to_string())?;
                Ok::<_, String>(count)
            }
            .await;
            match result {
                Ok(count) => moved += count,
                Err(error) => {
                    tracing::warn!(%tenant, %error, "operator key rewrap skipped this pass");
                }
            }
        }
        Ok(moved)
    }
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

/// Read a key file that only its owner may read.
///
/// # Errors
/// Returns the reason when the file is unreadable or group/other readable.
fn read_owner_only(path: &Path) -> Result<Zeroizing<String>, String> {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt as _;
        let mode = std::fs::metadata(path)
            .map_err(|error| error.to_string())?
            .permissions()
            .mode();
        if mode & 0o077 != 0 {
            return Err(format!(
                "file mode {:o} is readable beyond its owner; chmod 600 it",
                mode & 0o777
            ));
        }
    }
    std::fs::read_to_string(path)
        .map(Zeroizing::new)
        .map_err(|error| error.to_string())
}

/// Decode a base64 32-byte key, trimming surrounding whitespace.
///
/// # Errors
/// Returns the reason when the value is not base64 or not 32 bytes.
fn decode_key(encoded: &str) -> Result<SecretKey, String> {
    let bytes = Zeroizing::new(
        base64::engine::general_purpose::STANDARD
            .decode(encoded.trim())
            .map_err(|_| "value is not base64".to_owned())?,
    );
    let key: [u8; 32] = bytes
        .as_slice()
        .try_into()
        .map_err(|_| format!("key is {} bytes; expected 32", bytes.len()))?;
    Ok(SecretKey::from_bytes(key))
}

#[cfg(test)]
mod tests {
    //! Key decoding, file permissions, sealing, and context binding.

    use chrono::Utc;
    use uuid::Uuid;
    use wyrd_spec::operator_connection::{OperatorConnectionConfig, OperatorConnectionStatus};

    use super::*;

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
            active_version: std::num::NonZeroU32::new(active).expect("positive"),
            dir: Some(dir.to_path_buf()),
            vault: None,
        })
    }

    /// Write key `byte` as version `version`, owner-only.
    fn write_key(dir: &Path, version: u32, byte: u8) {
        use std::os::unix::fs::PermissionsExt as _;
        let path = dir.join(format!("v{version}"));
        std::fs::write(
            &path,
            base64::engine::general_purpose::STANDARD.encode([byte; 32]),
        )
        .expect("key writes");
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o600)).expect("chmod");
    }

    /// Only a base64 32-byte key decodes.
    #[test]
    fn decode_key_requires_32_base64_bytes() {
        let engine = base64::engine::general_purpose::STANDARD;
        assert!(decode_key(&format!("{}\n", engine.encode([1_u8; 32]))).is_ok());
        assert!(
            decode_key(&engine.encode([1_u8; 31]))
                .unwrap_err()
                .contains("31 bytes")
        );
        assert!(decode_key("not base64!").is_err());
    }

    /// A sealed secret opens under the same tenant and version, fails for
    /// another tenant, a group-readable key file is refused, a missing key
    /// names its file, and rewrap moves a secret to a new key version
    /// without changing what it opens to.
    ///
    /// # Panics
    /// Panics when any key or context binding breaks.
    #[tokio::test]
    async fn file_keys_seal_open_and_bind_context() {
        use std::os::unix::fs::PermissionsExt as _;
        let dir = tempfile::tempdir().expect("tempdir");
        let keys = file_keys(dir.path(), 1);
        let tenant = DataTenantId::new(Uuid::now_v7()).expect("tenant");
        let view = view();

        let missing = keys
            .seal(SecretIdentity::of(tenant, &view, 1), b"secret")
            .await
            .unwrap_err();
        assert_eq!(missing.code(), "WYRD_OPERATOR_503_KEY_UNAVAILABLE");
        assert!(missing.to_string().contains("v1"));

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
        let loose = keys.key(tenant, 1).await.unwrap_err().to_string();
        assert!(loose.contains("chmod 600"), "{loose}");
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
}
