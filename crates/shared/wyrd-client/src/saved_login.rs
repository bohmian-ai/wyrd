//! Saved human user logins: the CLI-established, renewable Wyrd user
//! credential every SDK resolves through the shared credential chain.
//!
//! `wyrd auth login` writes one [`SavedLogin`] record per canonical server
//! origin and tenant under `{wyrd_config_dir}/logins`. The directory is
//! private to the user (`0700`), every record and lock file is `0600`, and a
//! symlinked, foreign-owned, or group/world-accessible entry is refused rather
//! than read. Records are only ever replaced atomically (temporary file,
//! `fsync`, rename, directory `fsync`).
//!
//! Every read-modify-write of a record — renewal, login, logout — holds an
//! exclusive OS lock on that record's stable lock file, which is never
//! deleted. Renewal rereads the record under the lock, so a process that lost
//! a race uses the winner's newer generation instead of replaying its rotated
//! refresh token. Before a refresh token is sent, the record is durably moved
//! to [`SavedLoginState::RefreshPending`]; only a successful rotation moves it
//! back to [`SavedLoginState::Ready`] with the next generation. A crash or an
//! uncertain outcome leaves it pending, and every later process fails closed
//! and asks the person to log in again, because nobody can know whether the
//! server already consumed that token. Logout first persists a
//! [`SavedLoginState::LoggedOut`] tombstone under the lock, so no concurrent
//! renewal can resurrect the login, then revokes it remotely and deletes it.

use std::fs::{File, OpenOptions};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::{Duration, Instant};

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use wyrd_spec::DataTenantId;
use wyrd_spec::auth::{CliLogin, PrincipalId, SecretBearer, TokenRequest};
use wyrd_spec::ids::TenantSlug;

use crate::auth::{AuthError, TokenExchange};
use crate::error::WyrdClientError;
use crate::transport::credential::{AccessTokenSource, MintedAccessToken};

/// Format version of a [`SavedLogin`] record; any other version is refused.
pub const SAVED_LOGIN_FORMAT_VERSION: u32 = 1;

/// Longest a process waits for another process's hold on one record's lock.
const LOCK_DEADLINE: Duration = Duration::from_secs(30);

/// Pause between attempts to take a held record lock.
const LOCK_RETRY: Duration = Duration::from_millis(25);

/// A saved access token this close to expiry is renewed instead of returned,
/// comfortably outside the middleware's own 30-second refresh skew.
const RENEW_MARGIN: chrono::Duration = chrono::Duration::seconds(60);

/// One saved human user login for one server origin and tenant.
///
/// `Debug` is redacted: every token is a [`SecretBearer`].
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SavedLogin {
    /// Always [`SAVED_LOGIN_FORMAT_VERSION`].
    pub format_version: u32,
    /// Canonical server URL the login was made against ([`canonical_origin`]).
    pub origin: String,
    /// The tenant the credential belongs to.
    pub tenant_id: DataTenantId,
    /// The tenant's route key the person logged in at.
    pub tenant_key: TenantSlug,
    /// The tenant `User` the credential acts as.
    pub principal_id: PrincipalId,
    /// Incremented by every successful rotation and every new login.
    pub generation: u64,
    /// Where the record is in its renewal lifecycle.
    pub state: SavedLoginState,
}

/// Renewal lifecycle of a [`SavedLogin`].
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "status", rename_all = "snake_case", deny_unknown_fields)]
pub enum SavedLoginState {
    /// Usable: the current access token and the refresh token that renews it.
    Ready {
        /// Current Wyrd access token.
        access_token: SecretBearer,
        /// Its expiry.
        access_expires_at: DateTime<Utc>,
        /// The refresh token not yet sent to the server.
        refresh_token: SecretBearer,
    },
    /// A process began sending this refresh token and has not recorded the
    /// outcome. It is never sent again; the login must be repeated.
    RefreshPending {
        /// When the renewal began.
        started_at: DateTime<Utc>,
        /// The refresh token in flight, kept only so logout can revoke its
        /// chain.
        refresh_token: SecretBearer,
    },
    /// Logout began; the record must not be used or renewed.
    LoggedOut,
}

/// The safe projection `wyrd auth status` prints: no token.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct SavedLoginSummary {
    /// Canonical server URL.
    pub origin: String,
    /// Tenant id.
    pub tenant_id: DataTenantId,
    /// Tenant route key.
    pub tenant_key: TenantSlug,
    /// The `User` principal.
    pub principal_id: PrincipalId,
    /// Access-token expiry while the record is usable, else `None`.
    pub access_expires_at: Option<DateTime<Utc>>,
    /// `ready`, `refresh_pending`, or `logged_out`.
    pub status: &'static str,
}

impl SavedLogin {
    /// Build the first record of a login the CLI handoff just completed.
    #[must_use]
    pub fn from_cli_login(origin: String, tenant_key: TenantSlug, login: CliLogin) -> Self {
        Self {
            format_version: SAVED_LOGIN_FORMAT_VERSION,
            origin,
            tenant_id: login.tenant_id,
            tenant_key,
            principal_id: login.principal_id,
            generation: 1,
            state: SavedLoginState::Ready {
                access_token: login.access_token,
                access_expires_at: login.access_expires_at,
                refresh_token: login.refresh_token,
            },
        }
    }

    /// The token-free projection of this record.
    #[must_use]
    pub fn summary(&self) -> SavedLoginSummary {
        let (access_expires_at, status) = match &self.state {
            SavedLoginState::Ready {
                access_expires_at, ..
            } => (Some(*access_expires_at), "ready"),
            SavedLoginState::RefreshPending { .. } => (None, "refresh_pending"),
            SavedLoginState::LoggedOut => (None, "logged_out"),
        };
        SavedLoginSummary {
            origin: self.origin.clone(),
            tenant_id: self.tenant_id,
            tenant_key: self.tenant_key.clone(),
            principal_id: self.principal_id,
            access_expires_at,
            status,
        }
    }

    /// Whether `selector` (a tenant route key or tenant id) names this
    /// record's tenant.
    #[must_use]
    pub fn matches_tenant(&self, selector: &str) -> bool {
        selector == self.tenant_key.as_str() || selector == self.tenant_id.to_string()
    }
}

/// Canonical form of a server URL, the key saved logins are stored and
/// selected under: parsed and normalized, without query, fragment, or a
/// trailing slash.
///
/// # Errors
/// Returns [`WyrdClientError::Config`] when `server_url` is not an absolute
/// URL.
pub fn canonical_origin(server_url: &str) -> Result<String, WyrdClientError> {
    let mut url = reqwest::Url::parse(server_url).map_err(|error| WyrdClientError::Config {
        field: "http_config.base_url".to_owned(),
        reason: format!("not an absolute URL: {error}"),
    })?;
    url.set_query(None);
    url.set_fragment(None);
    Ok(url.as_str().trim_end_matches('/').to_owned())
}

/// The saved-login store under one directory.
#[derive(Debug, Clone)]
pub struct SavedLogins {
    /// `{wyrd_config_dir}/logins`, or a caller-chosen directory.
    dir: PathBuf,
}

/// An exclusive hold on one record's lock file, released on drop.
struct RecordLock {
    /// The open lock file; the OS lock lives as long as this handle.
    _file: File,
}

impl SavedLogins {
    /// The store under the user's Wyrd configuration directory, or `None`
    /// when no configuration directory can be resolved.
    #[must_use]
    pub fn locate() -> Option<Self> {
        wyrd_utils::config_dir::wyrd_config_dir().map(|dir| Self::at(dir.join("logins")))
    }

    /// The store under `dir`.
    #[must_use]
    pub fn at(dir: PathBuf) -> Self {
        Self { dir }
    }

    /// Save a login the CLI just completed, replacing any earlier record for
    /// the same origin and tenant under its lock.
    ///
    /// The new record continues the old record's generation, so a process
    /// holding the old generation rereads instead of reusing it.
    ///
    /// # Errors
    /// Returns [`WyrdClientError::SavedLogin`] when the store is unsafe or
    /// corrupt, the lock cannot be taken in time, or the write fails.
    pub fn save(&self, mut login: SavedLogin) -> Result<(), WyrdClientError> {
        self.prepare_dir(true)?;
        let stem = record_stem(&login.origin, login.tenant_id);
        let _lock = self.lock(&stem)?;
        if let Some(previous) = self.read(&stem)? {
            login.generation = previous.generation.saturating_add(1);
        }
        self.write(&stem, &login)
    }

    /// Every saved record, in no particular order. An absent store is empty.
    ///
    /// # Errors
    /// Returns [`WyrdClientError::SavedLogin`] when the store or a record is
    /// unsafe or corrupt.
    pub fn list(&self) -> Result<Vec<SavedLogin>, WyrdClientError> {
        if !self.prepare_dir(false)? {
            return Ok(Vec::new());
        }
        let entries = std::fs::read_dir(&self.dir).map_err(|error| io_failure(&error))?;
        let mut logins = Vec::new();
        for entry in entries {
            let path = entry.map_err(|error| io_failure(&error))?.path();
            if path.extension().is_some_and(|ext| ext == "json")
                && let Some(stem) = path.file_stem().and_then(|stem| stem.to_str())
                && let Some(login) = self.read(stem)?
            {
                logins.push(login);
            }
        }
        Ok(logins)
    }

    /// Select the saved login for `origin` and the optional tenant selector.
    ///
    /// With a selector, the one record for this origin whose tenant key or id
    /// equals it is selected; when other tenants' records exist for this
    /// origin but none matches, selection fails rather than falling through.
    /// Without a selector, exactly one record for this origin is selected and
    /// several fail as ambiguous. No record for this origin selects nothing.
    ///
    /// # Errors
    /// Returns [`WyrdClientError::SavedLogin`] with reason `tenant_mismatch`
    /// or `ambiguous`, and the errors of [`Self::list`].
    pub fn select(
        &self,
        origin: &str,
        tenant: Option<&str>,
    ) -> Result<Option<SavedLogin>, WyrdClientError> {
        let mut candidates: Vec<SavedLogin> = self
            .list()?
            .into_iter()
            .filter(|login| login.origin == origin)
            .collect();
        if candidates.is_empty() {
            return Ok(None);
        }
        if let Some(selector) = tenant {
            candidates.retain(|login| login.matches_tenant(selector));
            if candidates.is_empty() {
                return Err(saved_login(
                    "tenant_mismatch",
                    format!("no saved login for {origin} matches tenant {selector}"),
                ));
            }
        }
        if candidates.len() > 1 {
            return Err(saved_login(
                "ambiguous",
                format!(
                    "several tenants have saved logins for {origin}; select one with the client \
                     tenant option or WYRD_TENANT"
                ),
            ));
        }
        Ok(candidates.pop())
    }

    /// The renewing access-token source for `login`, exchanging through
    /// `exchange`.
    #[must_use]
    pub fn source(&self, login: &SavedLogin, exchange: TokenExchange) -> Arc<SavedLoginSource> {
        Arc::new(SavedLoginSource {
            store: self.clone(),
            stem: record_stem(&login.origin, login.tenant_id),
            identity: format!("saved-login:{}:{}", login.origin, login.tenant_id),
            exchange,
        })
    }

    /// Mark the record for `origin` and `tenant_id` logged out and return the
    /// refresh token to revoke, if the record carried one.
    ///
    /// The tombstone is durable before this returns, so no renewal that
    /// starts afterwards can use or resurrect the login.
    ///
    /// # Errors
    /// Returns [`WyrdClientError::SavedLogin`] when the store is unsafe or
    /// corrupt, the lock cannot be taken in time, or the write fails.
    pub fn begin_logout(
        &self,
        origin: &str,
        tenant_id: DataTenantId,
    ) -> Result<Option<SecretBearer>, WyrdClientError> {
        let stem = record_stem(origin, tenant_id);
        let _lock = self.lock(&stem)?;
        let Some(mut login) = self.read(&stem)? else {
            return Ok(None);
        };
        let refresh = match std::mem::replace(&mut login.state, SavedLoginState::LoggedOut) {
            SavedLoginState::Ready { refresh_token, .. }
            | SavedLoginState::RefreshPending { refresh_token, .. } => Some(refresh_token),
            SavedLoginState::LoggedOut => None,
        };
        self.write(&stem, &login)?;
        Ok(refresh)
    }

    /// Delete the logged-out record for `origin` and `tenant_id`.
    ///
    /// A record a new login replaced in the meantime is kept.
    ///
    /// # Errors
    /// Returns [`WyrdClientError::SavedLogin`] when the store is unsafe or
    /// corrupt, the lock cannot be taken in time, or the delete fails.
    pub fn finish_logout(
        &self,
        origin: &str,
        tenant_id: DataTenantId,
    ) -> Result<(), WyrdClientError> {
        let stem = record_stem(origin, tenant_id);
        let _lock = self.lock(&stem)?;
        if let Some(login) = self.read(&stem)?
            && login.state == SavedLoginState::LoggedOut
        {
            std::fs::remove_file(self.record_path(&stem)).map_err(|error| io_failure(&error))?;
            sync_dir(&self.dir)?;
        }
        Ok(())
    }

    /// Renew under the record lock and return a usable access token.
    ///
    /// # Errors
    /// See [`SavedLoginSource::mint`].
    fn renew(
        &self,
        stem: &str,
        exchange: &TokenExchange,
    ) -> Result<MintedAccessToken, WyrdClientError> {
        let _lock = self.lock(stem)?;
        let mut login = self.read(stem)?.ok_or_else(|| {
            saved_login(
                "logged_out",
                "the saved login was removed; run `wyrd auth login`",
            )
        })?;
        let refresh_token = match &login.state {
            SavedLoginState::Ready {
                access_token,
                access_expires_at,
                ..
            } if *access_expires_at - RENEW_MARGIN > Utc::now() => {
                return Ok(MintedAccessToken {
                    access_token: access_token.clone(),
                    expires_at: *access_expires_at,
                });
            }
            SavedLoginState::Ready { refresh_token, .. } => refresh_token.clone(),
            SavedLoginState::RefreshPending { .. } => {
                return Err(saved_login(
                    "refresh_pending",
                    "an earlier renewal of the saved login did not finish; run `wyrd auth login`",
                ));
            }
            SavedLoginState::LoggedOut => {
                return Err(saved_login(
                    "logged_out",
                    "the saved login was logged out; run `wyrd auth login`",
                ));
            }
        };
        login.state = SavedLoginState::RefreshPending {
            started_at: Utc::now(),
            refresh_token: refresh_token.clone(),
        };
        self.write(stem, &login)?;
        let handle = tokio::runtime::Handle::try_current().map_err(|_| {
            saved_login(
                "refresh_pending",
                "saved login renewal needs the client's async runtime",
            )
        })?;
        let rotated = handle
            .block_on(exchange.exchange(&TokenRequest::RefreshToken { refresh_token }))
            .map_err(|error| match error {
                AuthError::Server(wyrd) => saved_login(
                    "refresh_refused",
                    format!(
                        "the server refused to renew the saved login ({}); run `wyrd auth login`",
                        wyrd.code()
                    ),
                ),
                AuthError::Client(client) => saved_login(
                    "refresh_pending",
                    format!(
                        "renewing the saved login did not complete ({client}); run `wyrd auth \
                         login`"
                    ),
                ),
            })?;
        let refresh_token = rotated.refresh_token.ok_or_else(|| {
            saved_login(
                "refresh_refused",
                "the server renewed the saved login without a refresh token",
            )
        })?;
        if access_token_tenant(&rotated.access_token) != Some(login.tenant_id) {
            return Err(saved_login(
                "tenant_mismatch",
                "the server renewed the saved login for another tenant; run `wyrd auth login`",
            ));
        }
        login.generation = login.generation.saturating_add(1);
        login.state = SavedLoginState::Ready {
            access_token: rotated.access_token.clone(),
            access_expires_at: rotated.expires_at,
            refresh_token,
        };
        self.write(stem, &login)?;
        Ok(MintedAccessToken {
            access_token: rotated.access_token,
            expires_at: rotated.expires_at,
        })
    }

    /// Check the store directory, creating it when `create` is set, and
    /// report whether it exists.
    ///
    /// # Errors
    /// Returns [`WyrdClientError::SavedLogin`] with reason `unsafe_store` for
    /// a symlinked, foreign-owned, or group/world-accessible directory.
    fn prepare_dir(&self, create: bool) -> Result<bool, WyrdClientError> {
        match std::fs::symlink_metadata(&self.dir) {
            Ok(metadata) => {
                if !metadata.is_dir() {
                    return Err(unsafe_store(&self.dir, "is not a directory"));
                }
                check_private(&self.dir, &metadata)?;
                Ok(true)
            }
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                if !create {
                    return Ok(false);
                }
                create_private_dir(&self.dir)?;
                Ok(true)
            }
            Err(error) => Err(io_failure(&error)),
        }
    }

    /// Path of the record file `stem` names.
    fn record_path(&self, stem: &str) -> PathBuf {
        self.dir.join(format!("{stem}.json"))
    }

    /// Take the exclusive OS lock on `stem`'s lock file, waiting at most
    /// [`LOCK_DEADLINE`] for another holder.
    ///
    /// # Errors
    /// Returns [`WyrdClientError::SavedLogin`] with reason `lock_timeout`
    /// when the deadline passes, `unsafe_store` for an unsafe lock file, and
    /// an IO failure otherwise.
    fn lock(&self, stem: &str) -> Result<RecordLock, WyrdClientError> {
        self.prepare_dir(true)?;
        let path = self.dir.join(format!("{stem}.lock"));
        if let Ok(metadata) = std::fs::symlink_metadata(&path) {
            check_private(&path, &metadata)?;
        }
        let file = private_options()
            .read(true)
            .write(true)
            .create(true)
            .truncate(false)
            .open(&path)
            .map_err(|error| io_failure(&error))?;
        let deadline = Instant::now() + LOCK_DEADLINE;
        loop {
            match file.try_lock() {
                Ok(()) => return Ok(RecordLock { _file: file }),
                Err(std::fs::TryLockError::WouldBlock) if Instant::now() < deadline => {
                    std::thread::sleep(LOCK_RETRY);
                }
                Err(std::fs::TryLockError::WouldBlock) => {
                    return Err(saved_login(
                        "lock_timeout",
                        "another Wyrd process held the saved login too long",
                    ));
                }
                Err(std::fs::TryLockError::Error(error)) => return Err(io_failure(&error)),
            }
        }
    }

    /// Read the record `stem` names, `None` when it does not exist.
    ///
    /// # Errors
    /// Returns [`WyrdClientError::SavedLogin`] with reason `unsafe_store` for
    /// an unsafe file and `corrupt` for an undecodable or unknown-version
    /// record.
    fn read(&self, stem: &str) -> Result<Option<SavedLogin>, WyrdClientError> {
        let path = self.record_path(stem);
        let metadata = match std::fs::symlink_metadata(&path) {
            Ok(metadata) => metadata,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
            Err(error) => return Err(io_failure(&error)),
        };
        if !metadata.is_file() {
            return Err(unsafe_store(&path, "is not a regular file"));
        }
        check_private(&path, &metadata)?;
        let bytes = std::fs::read(&path).map_err(|error| io_failure(&error))?;
        let login: SavedLogin = serde_json::from_slice(&bytes)
            .map_err(|_| saved_login("corrupt", format!("{} does not decode", path.display())))?;
        if login.format_version != SAVED_LOGIN_FORMAT_VERSION {
            return Err(saved_login(
                "corrupt",
                format!("{} has an unknown format version", path.display()),
            ));
        }
        Ok(Some(login))
    }

    /// Atomically replace the record `stem` names with `login`.
    ///
    /// # Errors
    /// Returns [`WyrdClientError::SavedLogin`] when any write, `fsync`, or
    /// rename fails; the previous record is then unchanged.
    fn write(&self, stem: &str, login: &SavedLogin) -> Result<(), WyrdClientError> {
        let mut temp =
            tempfile::NamedTempFile::new_in(&self.dir).map_err(|error| io_failure(&error))?;
        let bytes = serde_json::to_vec(login)
            .map_err(|_| saved_login("corrupt", "the saved login does not encode"))?;
        temp.write_all(&bytes).map_err(|error| io_failure(&error))?;
        temp.as_file()
            .sync_all()
            .map_err(|error| io_failure(&error))?;
        temp.persist(self.record_path(stem))
            .map_err(|error| io_failure(&error.error))?;
        sync_dir(&self.dir)
    }
}

/// The renewing [`AccessTokenSource`] for one saved login.
///
/// Holds only where the record lives; every mint rereads it under its lock.
pub struct SavedLoginSource {
    /// The store holding the record.
    store: SavedLogins,
    /// The record's file stem.
    stem: String,
    /// Non-secret identity: origin and tenant id.
    identity: String,
    /// The unauthenticated `/auth` surface renewal exchanges through.
    exchange: TokenExchange,
}

impl std::fmt::Debug for SavedLoginSource {
    /// Prints only the non-secret identity.
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("SavedLoginSource")
            .field("identity", &self.identity)
            .finish_non_exhaustive()
    }
}

impl AccessTokenSource for SavedLoginSource {
    /// `saved-login:{origin}:{tenant_id}`.
    fn identity(&self) -> &str {
        &self.identity
    }

    /// Return the saved access token, renewing it first when it is near
    /// expiry.
    ///
    /// Runs on the middleware's blocking pool: it takes the record's OS lock,
    /// rereads the record, and returns a fresh access token as stored, which
    /// may be another process's newer generation. A stale `Ready` record is
    /// durably moved to `RefreshPending`, its refresh token is exchanged once
    /// on the client's runtime, and the rotated pair is stored as `Ready` with
    /// the next generation before the lock is released.
    ///
    /// # Errors
    /// Returns [`WyrdClientError::SavedLogin`] with reason `logged_out` for a
    /// removed or logged-out record, `refresh_pending` for a record an earlier
    /// renewal left uncertain or a renewal whose outcome is unknown,
    /// `refresh_refused` when the server refuses the refresh token,
    /// `tenant_mismatch` when it renews into another tenant, `lock_timeout`, `unsafe_store`, or `corrupt`. A failed renewal leaves
    /// the record `RefreshPending`, so it never retries the token.
    fn mint(&self) -> Result<MintedAccessToken, WyrdClientError> {
        self.store.renew(&self.stem, &self.exchange)
    }
}

/// The `tenant_id` claim of a Wyrd access token, read without verifying it.
///
/// Only a consistency check on a token the server just issued over TLS: the
/// server still verifies every token it receives. `None` when the token is
/// not a decodable JWT with that claim.
fn access_token_tenant(token: &SecretBearer) -> Option<DataTenantId> {
    /// The one claim this check reads.
    #[derive(Deserialize)]
    struct TenantClaim {
        /// Tenant the token acts in.
        tenant_id: DataTenantId,
    }
    let payload = token.expose().split('.').nth(1)?;
    let bytes =
        base64::Engine::decode(&base64::engine::general_purpose::URL_SAFE_NO_PAD, payload).ok()?;
    serde_json::from_slice::<TenantClaim>(&bytes)
        .ok()
        .map(|claim| claim.tenant_id)
}

/// File stem of the record for `origin` and `tenant_id`: a hash, so neither
/// value has to be a safe file name.
fn record_stem(origin: &str, tenant_id: DataTenantId) -> String {
    let digest = Sha256::digest(format!("{origin}\n{tenant_id}").as_bytes());
    hex::encode(&digest[..16])
}

/// The client-local saved-login error.
fn saved_login(reason: &'static str, message: impl Into<String>) -> WyrdClientError {
    WyrdClientError::SavedLogin {
        reason,
        message: message.into(),
    }
}

/// An unsafe store entry.
fn unsafe_store(path: &Path, problem: &str) -> WyrdClientError {
    saved_login("unsafe_store", format!("{} {problem}", path.display()))
}

/// A local IO failure on the store.
fn io_failure(error: &std::io::Error) -> WyrdClientError {
    saved_login("io", format!("saved login store IO failed: {error}"))
}

/// `fsync` a directory so a rename or delete inside it is durable.
///
/// # Errors
/// Returns the IO failure.
fn sync_dir(dir: &Path) -> Result<(), WyrdClientError> {
    #[cfg(unix)]
    {
        File::open(dir)
            .and_then(|handle| handle.sync_all())
            .map_err(|error| io_failure(&error))?;
    }
    #[cfg(not(unix))]
    let _ = dir;
    Ok(())
}

/// Open options that create files readable and writable by the owner only.
fn private_options() -> OpenOptions {
    let mut options = OpenOptions::new();
    #[cfg(unix)]
    std::os::unix::fs::OpenOptionsExt::mode(&mut options, 0o600);
    options
}

/// Create `dir` (and its parents) with the leaf private to the owner.
///
/// # Errors
/// Returns the IO failure.
fn create_private_dir(dir: &Path) -> Result<(), WyrdClientError> {
    let mut builder = std::fs::DirBuilder::new();
    builder.recursive(true);
    #[cfg(unix)]
    std::os::unix::fs::DirBuilderExt::mode(&mut builder, 0o700);
    builder.create(dir).map_err(|error| io_failure(&error))
}

/// Refuse an entry that is a symlink, owned by another user, or accessible to
/// the group or others.
///
/// # Errors
/// Returns [`WyrdClientError::SavedLogin`] with reason `unsafe_store`.
fn check_private(path: &Path, metadata: &std::fs::Metadata) -> Result<(), WyrdClientError> {
    if metadata.file_type().is_symlink() {
        return Err(unsafe_store(path, "is a symlink"));
    }
    #[cfg(unix)]
    {
        if std::os::unix::fs::MetadataExt::uid(metadata) != rustix::process::geteuid().as_raw() {
            return Err(unsafe_store(path, "is owned by another user"));
        }
        if std::os::unix::fs::MetadataExt::mode(metadata) & 0o077 != 0 {
            return Err(unsafe_store(path, "is accessible to other users"));
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use std::str::FromStr;

    use chrono::Utc;
    use uuid::Uuid;
    use wyrd_spec::DataTenantId;
    use wyrd_spec::auth::{PrincipalId, SecretBearer};
    use wyrd_spec::ids::TenantSlug;

    use super::{
        SAVED_LOGIN_FORMAT_VERSION, SavedLogin, SavedLoginState, SavedLogins, canonical_origin,
    };

    /// A ready record for `origin` and the tenant `key`.
    fn login(origin: &str, key: &str) -> SavedLogin {
        SavedLogin {
            format_version: SAVED_LOGIN_FORMAT_VERSION,
            origin: origin.to_owned(),
            tenant_id: DataTenantId::new_v7(),
            tenant_key: TenantSlug::from_str(key).expect("slug"),
            principal_id: PrincipalId::new(Uuid::now_v7()),
            generation: 7,
            state: SavedLoginState::Ready {
                access_token: SecretBearer::new("access-sentinel".to_owned()),
                access_expires_at: Utc::now(),
                refresh_token: SecretBearer::new("refresh-sentinel".to_owned()),
            },
        }
    }

    /// The canonical origin drops query, fragment, and trailing slash.
    #[test]
    fn canonical_origin_normalizes() {
        assert_eq!(
            canonical_origin("HTTPS://Wyrd.Example.com:443/?x=1#f").expect("parses"),
            "https://wyrd.example.com"
        );
        assert!(canonical_origin("not a url").is_err());
    }

    /// Selection by origin and tenant: one record selects, a selector picks
    /// its tenant, several without a selector are ambiguous, a selector naming
    /// no record fails, and another origin selects nothing. A saved login is
    /// continued at the next generation, and the status projection carries no
    /// token.
    #[test]
    fn selection_is_exact_and_never_guesses_a_tenant() {
        let dir = tempfile::tempdir().expect("tempdir");
        let store = SavedLogins::at(dir.path().join("logins"));
        let origin = "https://wyrd.example.com";
        let acme = login(origin, "acme");
        store.save(acme.clone()).expect("saves");
        store.save(acme.clone()).expect("saves again");

        let only = store.select(origin, None).expect("selects").expect("one");
        assert_eq!(only.generation, acme.generation + 1);
        assert!(
            store
                .select("https://other.example.com", None)
                .expect("selects")
                .is_none()
        );

        let globex = login(origin, "globex");
        store.save(globex.clone()).expect("saves");
        let ambiguous = store.select(origin, None).expect_err("ambiguous");
        assert_eq!(ambiguous.code(), "WYRD_CLIENT_401_SAVED_LOGIN_UNUSABLE");
        assert!(ambiguous.to_string().contains("ambiguous"));
        let by_key = store
            .select(origin, Some("globex"))
            .expect("selects")
            .expect("one");
        assert_eq!(by_key.tenant_id, globex.tenant_id);
        let by_id = store
            .select(origin, Some(&acme.tenant_id.to_string()))
            .expect("selects")
            .expect("one");
        assert_eq!(by_id.tenant_key, acme.tenant_key);
        let mismatch = store.select(origin, Some("initech")).expect_err("mismatch");
        assert!(mismatch.to_string().contains("tenant_mismatch"));

        let printed = format!("{:?} {:?}", acme.summary(), acme);
        assert!(!printed.contains("sentinel"), "{printed}");
    }

    /// Logout tombstones the record and hands back the refresh token, then
    /// deletes the tombstone; a record a new login wrote in between is kept.
    #[test]
    fn logout_tombstones_before_deleting() {
        let dir = tempfile::tempdir().expect("tempdir");
        let store = SavedLogins::at(dir.path().join("logins"));
        let record = login("https://wyrd.example.com", "acme");
        store.save(record.clone()).expect("saves");

        let refresh = store
            .begin_logout(&record.origin, record.tenant_id)
            .expect("tombstones")
            .expect("refresh token");
        assert_eq!(refresh.expose(), "refresh-sentinel");
        let tombstone = store.list().expect("lists").pop().expect("record kept");
        assert_eq!(tombstone.state, SavedLoginState::LoggedOut);
        store
            .finish_logout(&record.origin, record.tenant_id)
            .expect("deletes");
        assert!(store.list().expect("lists").is_empty());

        store.save(record.clone()).expect("saves");
        store
            .finish_logout(&record.origin, record.tenant_id)
            .expect("keeps a live login");
        assert_eq!(store.list().expect("lists").len(), 1);
    }

    /// A record that does not decode and a store or record other users can
    /// read are refused, never skipped.
    #[cfg(unix)]
    #[test]
    fn unsafe_and_corrupt_stores_fail_closed() {
        let dir = tempfile::tempdir().expect("tempdir");
        let store = SavedLogins::at(dir.path().join("logins"));
        let record = login("https://wyrd.example.com", "acme");
        store.save(record.clone()).expect("saves");
        let file = std::fs::read_dir(dir.path().join("logins"))
            .expect("lists")
            .map(|entry| entry.expect("entry").path())
            .find(|path| path.extension().is_some_and(|ext| ext == "json"))
            .expect("record file");

        std::fs::set_permissions(
            &file,
            <std::fs::Permissions as std::os::unix::fs::PermissionsExt>::from_mode(0o644),
        )
        .expect("chmod");
        let error = store.select(&record.origin, None).expect_err("unsafe");
        assert!(error.to_string().contains("unsafe_store"), "{error}");

        std::fs::set_permissions(
            &file,
            <std::fs::Permissions as std::os::unix::fs::PermissionsExt>::from_mode(0o600),
        )
        .expect("chmod");
        std::fs::write(&file, b"{not json").expect("corrupts");
        let error = store.select(&record.origin, None).expect_err("corrupt");
        assert!(error.to_string().contains("corrupt"), "{error}");

        std::fs::remove_file(&file).expect("removes");
        let link = file.clone();
        std::os::unix::fs::symlink(dir.path(), &link).expect("links");
        let error = store.select(&record.origin, None).expect_err("symlink");
        assert!(error.to_string().contains("unsafe_store"), "{error}");
        std::fs::remove_file(&link).expect("unlinks");

        std::fs::set_permissions(
            dir.path().join("logins"),
            <std::fs::Permissions as std::os::unix::fs::PermissionsExt>::from_mode(0o755),
        )
        .expect("chmod");
        let error = store.select(&record.origin, None).expect_err("open dir");
        assert!(error.to_string().contains("unsafe_store"), "{error}");
    }
}
