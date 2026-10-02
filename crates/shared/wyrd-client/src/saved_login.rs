//! Saved human user logins: the CLI-established, renewable Wyrd user
//! credential every SDK resolves through the shared credential chain.
//!
//! `wyrd auth login` writes one [`SavedLogin`] record per canonical server
//! origin and tenant into the `[[logins]]` tables of the one Wyrd credential
//! file, `{wyrd_config_dir}/credentials.toml`, beside the user's own content
//! such as `[default].api_key`. Protection is the file's user-only ownership
//! and mode (`0600`) inside a directory only the user can change; a
//! symlinked, foreign-owned, or group/world-accessible file is refused rather
//! than read. The file is only ever replaced atomically (temporary file,
//! `fsync`, rename, directory `fsync`), and every write carries the user's
//! other keys and comments through unchanged.
//!
//! Every read-modify-write — renewal, login, logout — holds an exclusive OS
//! lock on the configuration directory itself, which no rename replaces.
//! Renewal rereads the record under the lock, so a process that lost
//! a race uses the winner's newer generation instead of replaying its rotated
//! refresh token. Before a refresh token is sent, the record is durably moved
//! to [`SavedLoginState::RefreshPending`]; only a successful rotation moves it
//! back to [`SavedLoginState::Ready`] with the next generation. A crash or an
//! uncertain outcome leaves it pending, and every later process fails closed
//! and asks the person to log in again, because nobody can know whether the
//! server already consumed that token. Logout first persists a
//! [`SavedLoginState::LoggedOut`] tombstone under the lock, so no concurrent
//! renewal can resurrect the login, then revokes it remotely and deletes it.

use std::fs::File;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::{Duration, Instant};

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
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

    /// Whether this is the login for `origin` and `tenant_id`.
    fn is_for(&self, origin: &str, tenant_id: DataTenantId) -> bool {
        self.origin == origin && self.tenant_id == tenant_id
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

/// File name of the one Wyrd credential file, under the configuration
/// directory.
const CREDENTIALS_FILE: &str = "credentials.toml";

/// The `credentials.toml` array of tables holding saved logins.
const LOGINS_KEY: &str = "logins";

/// The saved-login part of `credentials.toml`; every other key is the
/// user's and is only ever carried through unchanged.
#[derive(Deserialize)]
struct StoredLogins {
    /// Every saved login, `[[logins]]`; absent means none.
    #[serde(default)]
    logins: Vec<SavedLogin>,
}

/// The borrowed form of [`StoredLogins`] a write renders.
#[derive(Serialize)]
struct StoredLoginsRef<'a> {
    /// Every saved login to keep.
    logins: &'a [SavedLogin],
}

/// The saved logins in one Wyrd configuration directory's
/// `credentials.toml`.
#[derive(Debug, Clone)]
pub struct SavedLogins {
    /// `{wyrd_config_dir}`, or a caller-chosen directory.
    dir: PathBuf,
}

/// An exclusive hold on the configuration directory's OS lock, released on
/// drop.
struct StoreLock {
    /// The open directory; the OS lock lives as long as this handle.
    _dir: File,
}

impl SavedLogins {
    /// The saved logins in the user's Wyrd configuration directory, or
    /// `None` when no configuration directory can be resolved.
    #[must_use]
    pub fn locate() -> Option<Self> {
        wyrd_utils::config_dir::wyrd_config_dir().map(Self::at)
    }

    /// The saved logins in `dir/credentials.toml`.
    #[must_use]
    pub fn at(dir: PathBuf) -> Self {
        Self { dir }
    }

    /// Save a login the CLI just completed, replacing any earlier login for
    /// the same origin and tenant under the store lock.
    ///
    /// The new record continues the old record's generation, so a process
    /// holding the old generation rereads instead of reusing it.
    ///
    /// # Errors
    /// Returns [`WyrdClientError::SavedLogin`] when the file is unsafe or
    /// corrupt, the lock cannot be taken in time, or the write fails.
    pub fn save(&self, mut login: SavedLogin) -> Result<(), WyrdClientError> {
        let _lock = self.lock()?;
        let mut logins = self.read()?;
        match logins
            .iter_mut()
            .find(|saved| saved.is_for(&login.origin, login.tenant_id))
        {
            Some(previous) => {
                login.generation = previous.generation.saturating_add(1);
                *previous = login;
            }
            None => logins.push(login),
        }
        self.write(&logins)
    }

    /// Every saved login, in file order. An absent file or configuration
    /// directory holds none.
    ///
    /// # Errors
    /// Returns [`WyrdClientError::SavedLogin`] when the file or directory is
    /// unsafe or a login is corrupt.
    pub fn list(&self) -> Result<Vec<SavedLogin>, WyrdClientError> {
        self.read()
    }

    /// Select the saved login for `origin` and the optional tenant selector.
    ///
    /// With a selector (a tenant route key, the one `wyrd auth login` takes),
    /// the one record for this origin whose tenant key equals it is selected; when other tenants' records exist for this
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
            candidates.retain(|login| login.tenant_key.as_str() == selector);
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
            origin: login.origin.clone(),
            tenant_id: login.tenant_id,
            identity: format!("saved-login:{}:{}", login.origin, login.tenant_id),
            exchange,
        })
    }

    /// Mark the login for `origin` and `tenant_id` logged out and return the
    /// refresh token to revoke, if the login carried one.
    ///
    /// The tombstone is durable before this returns, so no renewal that
    /// starts afterwards can use or resurrect the login.
    ///
    /// # Errors
    /// Returns [`WyrdClientError::SavedLogin`] when the file is unsafe or
    /// corrupt, the lock cannot be taken in time, or the write fails.
    pub fn begin_logout(
        &self,
        origin: &str,
        tenant_id: DataTenantId,
    ) -> Result<Option<SecretBearer>, WyrdClientError> {
        let _lock = self.lock()?;
        let mut logins = self.read()?;
        let Some(login) = logins
            .iter_mut()
            .find(|login| login.is_for(origin, tenant_id))
        else {
            return Ok(None);
        };
        let refresh = match std::mem::replace(&mut login.state, SavedLoginState::LoggedOut) {
            SavedLoginState::Ready { refresh_token, .. }
            | SavedLoginState::RefreshPending { refresh_token, .. } => Some(refresh_token),
            SavedLoginState::LoggedOut => None,
        };
        self.write(&logins)?;
        Ok(refresh)
    }

    /// Remove the logged-out login for `origin` and `tenant_id`.
    ///
    /// A login a new `wyrd auth login` saved in the meantime is kept.
    ///
    /// # Errors
    /// Returns [`WyrdClientError::SavedLogin`] when the file is unsafe or
    /// corrupt, the lock cannot be taken in time, or the write fails.
    pub fn finish_logout(
        &self,
        origin: &str,
        tenant_id: DataTenantId,
    ) -> Result<(), WyrdClientError> {
        let _lock = self.lock()?;
        let mut logins = self.read()?;
        let before = logins.len();
        logins.retain(|login| {
            !(login.is_for(origin, tenant_id) && login.state == SavedLoginState::LoggedOut)
        });
        if logins.len() == before {
            return Ok(());
        }
        self.write(&logins)
    }

    /// Renew under the store lock and return a usable access token.
    ///
    /// # Errors
    /// See [`SavedLoginSource::mint`].
    fn renew(
        &self,
        origin: &str,
        tenant_id: DataTenantId,
        exchange: &TokenExchange,
    ) -> Result<MintedAccessToken, WyrdClientError> {
        let _lock = self.lock()?;
        let mut logins = self.read()?;
        let index = logins
            .iter()
            .position(|login| login.is_for(origin, tenant_id))
            .ok_or_else(|| {
                saved_login(
                    "logged_out",
                    "the saved login was removed; run `wyrd auth login`",
                )
            })?;
        let refresh_token = match &logins[index].state {
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
        logins[index].state = SavedLoginState::RefreshPending {
            started_at: Utc::now(),
            refresh_token: refresh_token.clone(),
        };
        self.write(&logins)?;
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
        if access_token_tenant(&rotated.access_token) != Some(tenant_id) {
            return Err(saved_login(
                "tenant_mismatch",
                "the server renewed the saved login for another tenant; run `wyrd auth login`",
            ));
        }
        let login = &mut logins[index];
        login.generation = login.generation.saturating_add(1);
        login.state = SavedLoginState::Ready {
            access_token: rotated.access_token.clone(),
            access_expires_at: rotated.expires_at,
            refresh_token,
        };
        self.write(&logins)?;
        Ok(MintedAccessToken {
            access_token: rotated.access_token,
            expires_at: rotated.expires_at,
        })
    }

    /// Path of `credentials.toml`.
    fn path(&self) -> PathBuf {
        self.dir.join(CREDENTIALS_FILE)
    }

    /// Check the configuration directory, creating it private to the user
    /// when `create` is set, and report whether it exists.
    ///
    /// The directory may be readable by others and writable by the user's
    /// group (a plain `~/.config/wyrd` under a `002` umask is): the file's
    /// own ownership, mode, and symlink checks are what keep the secrets
    /// private, so the directory need only be the user's and not
    /// world-writable.
    ///
    /// # Errors
    /// Returns [`WyrdClientError::SavedLogin`] with reason `unsafe_store` for
    /// a symlinked, foreign-owned, or world-writable directory.
    fn check_dir(&self, create: bool) -> Result<bool, WyrdClientError> {
        match std::fs::symlink_metadata(&self.dir) {
            Ok(metadata) => {
                if !metadata.is_dir() {
                    return Err(unsafe_store(&self.dir, "is not a directory"));
                }
                check_owned(&self.dir, &metadata, 0o002)?;
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

    /// Take the exclusive OS lock on the configuration directory, waiting at
    /// most [`LOCK_DEADLINE`] for another holder.
    ///
    /// The directory, not `credentials.toml`, carries the lock: every write
    /// atomically replaces the file with a new inode, while the directory is
    /// stable and adds no file of its own.
    ///
    /// # Errors
    /// Returns [`WyrdClientError::SavedLogin`] with reason `lock_timeout`
    /// when the deadline passes, `unsafe_store` for an unsafe directory or a
    /// platform without POSIX file permissions, and an IO failure otherwise.
    fn lock(&self) -> Result<StoreLock, WyrdClientError> {
        self.check_dir(true)?;
        // ponytail: Windows cannot prove a user-only file or lock a directory;
        // saved logins fail closed there until a Windows ACL check is added.
        #[cfg(not(unix))]
        return Err(unsafe_store(
            &self.dir,
            "cannot hold saved logins on a platform without POSIX file permissions",
        ));
        #[cfg(unix)]
        {
            let dir = File::open(&self.dir).map_err(|error| io_failure(&error))?;
            let deadline = Instant::now() + LOCK_DEADLINE;
            loop {
                match dir.try_lock() {
                    Ok(()) => return Ok(StoreLock { _dir: dir }),
                    Err(std::fs::TryLockError::WouldBlock) if Instant::now() < deadline => {
                        std::thread::sleep(LOCK_RETRY);
                    }
                    Err(std::fs::TryLockError::WouldBlock) => {
                        return Err(saved_login(
                            "lock_timeout",
                            "another Wyrd process held the saved logins too long",
                        ));
                    }
                    Err(std::fs::TryLockError::Error(error)) => return Err(io_failure(&error)),
                }
            }
        }
    }

    /// The text of `credentials.toml`, `None` when it or the configuration
    /// directory does not exist.
    ///
    /// # Errors
    /// Returns [`WyrdClientError::SavedLogin`] with reason `unsafe_store`
    /// when the directory is unsafe or the file is not a regular file owned
    /// by and private to the user (`0600`), and `io` when it cannot be read.
    fn read_text(&self) -> Result<Option<String>, WyrdClientError> {
        if !self.check_dir(false)? {
            return Ok(None);
        }
        let path = self.path();
        let metadata = match std::fs::symlink_metadata(&path) {
            Ok(metadata) => metadata,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
            Err(error) => return Err(io_failure(&error)),
        };
        if !metadata.is_file() {
            return Err(unsafe_store(&path, "is not a regular file"));
        }
        check_owned(&path, &metadata, 0o077)?;
        std::fs::read_to_string(&path)
            .map(Some)
            .map_err(|error| io_failure(&error))
    }

    /// Every saved login in `credentials.toml`.
    ///
    /// # Errors
    /// The errors of [`Self::read_text`], and `corrupt` when the file is not
    /// TOML, a login does not decode, or a login has an unknown format
    /// version.
    fn read(&self) -> Result<Vec<SavedLogin>, WyrdClientError> {
        let Some(text) = self.read_text()? else {
            return Ok(Vec::new());
        };
        let path = self.path();
        let stored: StoredLogins = toml::from_str(&text).map_err(|_| {
            saved_login(
                "corrupt",
                format!("{} has saved logins that do not decode", path.display()),
            )
        })?;
        if stored
            .logins
            .iter()
            .any(|login| login.format_version != SAVED_LOGIN_FORMAT_VERSION)
        {
            return Err(saved_login(
                "corrupt",
                format!(
                    "{} has a saved login of an unknown format version",
                    path.display()
                ),
            ));
        }
        Ok(stored.logins)
    }

    /// Atomically replace `credentials.toml` with its current content and
    /// `logins` as its saved logins.
    ///
    /// The current file is reread and edited in place, so every other key,
    /// comment, and layout the user wrote survives; an empty `logins`
    /// removes the `[[logins]]` tables. The new text goes to a `0600`
    /// temporary file in the same directory, is `fsync`ed, renamed over the
    /// file, and the directory is `fsync`ed. Callers hold the store lock.
    ///
    /// # Errors
    /// Returns [`WyrdClientError::SavedLogin`] with the errors of
    /// [`Self::read_text`], `corrupt` when the current file is not TOML or a
    /// login does not encode, and `io` when any write, `fsync`, or rename
    /// fails; the previous file is then unchanged.
    fn write(&self, logins: &[SavedLogin]) -> Result<(), WyrdClientError> {
        let corrupt = || {
            saved_login(
                "corrupt",
                format!("{} is not valid TOML", self.path().display()),
            )
        };
        let mut document = match self.read_text()? {
            Some(text) => text
                .parse::<toml_edit::DocumentMut>()
                .map_err(|_| corrupt())?,
            None => toml_edit::DocumentMut::new(),
        };
        if logins.is_empty() {
            document.remove(LOGINS_KEY);
        } else {
            let mut rendered = toml::to_string(&StoredLoginsRef { logins })
                .map_err(|_| saved_login("corrupt", "the saved logins do not encode"))?
                .parse::<toml_edit::DocumentMut>()
                .map_err(|_| saved_login("corrupt", "the saved logins do not encode"))?;
            if let Some(item) = rendered.remove(LOGINS_KEY) {
                document.insert(LOGINS_KEY, item);
            }
        }
        let mut temp =
            tempfile::NamedTempFile::new_in(&self.dir).map_err(|error| io_failure(&error))?;
        temp.write_all(document.to_string().as_bytes())
            .map_err(|error| io_failure(&error))?;
        temp.as_file()
            .sync_all()
            .map_err(|error| io_failure(&error))?;
        temp.persist(self.path())
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
    /// Canonical origin the record is saved under.
    origin: String,
    /// Tenant the record belongs to.
    tenant_id: DataTenantId,
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
    /// Runs on the middleware's blocking pool: it takes the store's OS lock,
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
    /// `tenant_mismatch` when it renews into another tenant, `lock_timeout`,
    /// `unsafe_store`, or `corrupt`. A failed renewal leaves the record
    /// `RefreshPending`, so it never retries the token.
    fn mint(&self) -> Result<MintedAccessToken, WyrdClientError> {
        self.store
            .renew(&self.origin, self.tenant_id, &self.exchange)
    }

    /// Always: another process can rotate, mark pending, or log out the
    /// record, so every use rereads it under the store lock through
    /// [`Self::mint`] instead of reusing the middleware's cached token.
    fn revalidates_cache(&self) -> bool {
        true
    }
}

/// The `principal.tenant_id` claim of a Wyrd access token, read without
/// verifying it.
///
/// Only a consistency check on a token the server just issued over TLS: the
/// server still verifies every token it receives. `None` when the token is
/// not a decodable JWT with that claim.
fn access_token_tenant(token: &SecretBearer) -> Option<DataTenantId> {
    /// The one claim this check reads.
    #[derive(Deserialize)]
    struct Claims {
        /// Subject the token acts as.
        principal: Principal,
    }
    /// The token subject, reduced to its tenant.
    #[derive(Deserialize)]
    struct Principal {
        /// Tenant the token acts in.
        tenant_id: DataTenantId,
    }
    let payload = token.expose().split('.').nth(1)?;
    let bytes =
        base64::Engine::decode(&base64::engine::general_purpose::URL_SAFE_NO_PAD, payload).ok()?;
    serde_json::from_slice::<Claims>(&bytes)
        .ok()
        .map(|claims| claims.principal.tenant_id)
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

/// Refuse an entry that is a symlink, owned by another user, or has any of
/// the `forbidden` group/other mode bits set.
///
/// # Errors
/// Returns [`WyrdClientError::SavedLogin`] with reason `unsafe_store`.
fn check_owned(
    path: &Path,
    metadata: &std::fs::Metadata,
    forbidden: u32,
) -> Result<(), WyrdClientError> {
    if metadata.file_type().is_symlink() {
        return Err(unsafe_store(path, "is a symlink"));
    }
    #[cfg(not(unix))]
    let _ = forbidden;
    #[cfg(unix)]
    {
        if std::os::unix::fs::MetadataExt::uid(metadata) != rustix::process::geteuid().as_raw() {
            return Err(unsafe_store(path, "is owned by another user"));
        }
        if std::os::unix::fs::MetadataExt::mode(metadata) & forbidden != 0 {
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
        SAVED_LOGIN_FORMAT_VERSION, SavedLogin, SavedLoginState, SavedLogins, access_token_tenant,
        canonical_origin,
    };

    /// The renewal tenant check reads the access token's `principal.tenant_id`
    /// claim, and a token without it never passes as some tenant.
    #[test]
    fn access_token_tenant_reads_the_principal_claim() {
        let tenant = DataTenantId::new_v7();
        let token = |claims: serde_json::Value| {
            let payload = base64::Engine::encode(
                &base64::engine::general_purpose::URL_SAFE_NO_PAD,
                claims.to_string(),
            );
            SecretBearer::new(format!("e30.{payload}.sig"))
        };
        assert_eq!(
            access_token_tenant(&token(serde_json::json!({
                "sub": "user",
                "principal": { "id": "user", "tenant_id": tenant },
            }))),
            Some(tenant)
        );
        assert_eq!(
            access_token_tenant(&token(serde_json::json!({ "tenant_id": tenant }))),
            None
        );
        assert_eq!(
            access_token_tenant(&SecretBearer::new("opaque".to_owned())),
            None
        );
    }

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

    /// Selection by origin and tenant: one record selects, a tenant route key
    /// picks its tenant while a tenant id selects nothing, several without a selector are ambiguous, a selector naming
    /// no record fails, and another origin selects nothing. A saved login is
    /// continued at the next generation, and the status projection carries no
    /// token.
    #[test]
    fn selection_is_exact_and_never_guesses_a_tenant() {
        let dir = tempfile::tempdir().expect("tempdir");
        let store = SavedLogins::at(dir.path().to_path_buf());
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
            .expect_err("a tenant id is not a selector");
        assert!(by_id.to_string().contains("tenant_mismatch"), "{by_id}");
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
        let store = SavedLogins::at(dir.path().to_path_buf());
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

    /// The saved logins share `credentials.toml` with the user's own
    /// content: a save and a logout carry every other key and comment
    /// through, and no other file or directory is left behind.
    #[test]
    fn logins_live_in_credentials_toml_beside_user_content() {
        let dir = tempfile::tempdir().expect("tempdir");
        let file = dir.path().join("credentials.toml");
        let user = "# my machine key\n[default]\napi_key = \"wyrd_sk_user\" # keep\n";
        std::fs::write(&file, user).expect("writes");
        #[cfg(unix)]
        std::fs::set_permissions(
            &file,
            <std::fs::Permissions as std::os::unix::fs::PermissionsExt>::from_mode(0o600),
        )
        .expect("chmod");
        let store = SavedLogins::at(dir.path().to_path_buf());
        let origin = "https://wyrd.example.com";
        let acme = login(origin, "acme");
        let globex = login(origin, "globex");
        store.save(acme.clone()).expect("saves");
        store.save(globex.clone()).expect("saves");

        let text = std::fs::read_to_string(&file).expect("reads");
        assert!(text.starts_with(user), "{text}");
        assert_eq!(
            store.list().expect("lists"),
            vec![
                SavedLogin {
                    generation: acme.generation,
                    ..acme.clone()
                },
                globex.clone()
            ]
        );
        let names: Vec<_> = std::fs::read_dir(dir.path())
            .expect("lists dir")
            .map(|entry| entry.expect("entry").file_name())
            .collect();
        assert_eq!(names, ["credentials.toml"]);

        for record in [&acme, &globex] {
            store
                .begin_logout(&record.origin, record.tenant_id)
                .expect("tombstones");
            store
                .finish_logout(&record.origin, record.tenant_id)
                .expect("removes");
        }
        assert_eq!(std::fs::read_to_string(&file).expect("reads"), user);
    }

    /// A file that does not decode, a file other users can read, a
    /// symlinked file, and a world-writable directory are refused,
    /// never skipped.
    #[cfg(unix)]
    #[test]
    fn unsafe_and_corrupt_stores_fail_closed() {
        let dir = tempfile::tempdir().expect("tempdir");
        let store = SavedLogins::at(dir.path().to_path_buf());
        let record = login("https://wyrd.example.com", "acme");
        store.save(record.clone()).expect("saves");
        let file = dir.path().join("credentials.toml");
        let chmod = |path: &std::path::Path, mode: u32| {
            std::fs::set_permissions(
                path,
                <std::fs::Permissions as std::os::unix::fs::PermissionsExt>::from_mode(mode),
            )
            .expect("chmod");
        };

        chmod(&file, 0o644);
        let error = store.select(&record.origin, None).expect_err("unsafe");
        assert!(error.to_string().contains("unsafe_store"), "{error}");
        let error = store.save(record.clone()).expect_err("unsafe write");
        assert!(error.to_string().contains("unsafe_store"), "{error}");

        chmod(&file, 0o600);
        std::fs::write(&file, b"[[logins]]\nformat_version = 1\n").expect("corrupts");
        let error = store.select(&record.origin, None).expect_err("corrupt");
        assert!(error.to_string().contains("corrupt"), "{error}");

        std::fs::remove_file(&file).expect("removes");
        let target = dir.path().join("elsewhere.toml");
        std::fs::write(&target, b"").expect("writes");
        chmod(&target, 0o600);
        std::os::unix::fs::symlink(&target, &file).expect("links");
        let error = store.select(&record.origin, None).expect_err("symlink");
        assert!(error.to_string().contains("unsafe_store"), "{error}");
        std::fs::remove_file(&file).expect("unlinks");

        chmod(dir.path(), 0o777);
        let error = store.select(&record.origin, None).expect_err("open dir");
        assert!(error.to_string().contains("unsafe_store"), "{error}");
        chmod(dir.path(), 0o775);
        assert!(
            store
                .select(&record.origin, None)
                .expect("readable dir")
                .is_none()
        );
    }
}
