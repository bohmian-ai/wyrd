//! Saved human user logins: the CLI-established, renewable Wyrd user
//! credential every SDK resolves through the shared credential chain.
//!
//! `wyrd auth login` writes one [`SavedLogin`] record per canonical server
//! origin and tenant into the `[[logins]]` tables of the one Wyrd credential
//! file, `{wyrd_config_dir}/credentials.toml`, beside the user's own content
//! such as `[default].api_key`. That file's protection, atomic replacement,
//! and content preservation belong to [`crate::credentials_file`].
//!
//! Renewal works the way `gh`, `gcloud`, and `aws` do. A client uses the saved
//! access token until it nears expiry; then, holding the credential file's
//! exclusive lock, it rereads the record, uses a token another client already
//! renewed, or else sends the refresh token once and saves the rotated pair.
//! A refused refresh asks the person to log in again. Logout deletes the
//! record and then revokes it on the server best-effort. The newest login for
//! a server is the last record for it in the file, and it is the one used
//! when no tenant is selected.

use std::path::PathBuf;
use std::sync::Arc;

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use wyrd_spec::auth::{SecretBearer, TokenRequest, TokenResponse};
use wyrd_spec::ids::TenantSlug;

use crate::auth::{AuthError, TokenExchange};
use crate::credentials_file::CredentialsFile;
use crate::error::WyrdClientError;
use crate::transport::credential::{AccessTokenSource, MintedAccessToken};

/// A saved access token this close to expiry is renewed instead of returned,
/// comfortably outside the middleware's own 30-second refresh skew.
const RENEW_MARGIN: chrono::Duration = chrono::Duration::seconds(60);

/// One saved human user login for one server origin and tenant.
///
/// `Debug` is redacted: every token is a [`SecretBearer`].
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SavedLogin {
    /// Canonical server URL the login was made against ([`canonical_origin`]).
    pub origin: String,
    /// The tenant's route key the person logged in at.
    pub tenant_key: TenantSlug,
    /// Current Wyrd access token.
    pub access_token: SecretBearer,
    /// Its expiry.
    pub access_expires_at: DateTime<Utc>,
    /// The refresh token that renews it.
    pub refresh_token: SecretBearer,
}

/// The safe projection `wyrd auth status` prints: no token.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct SavedLoginSummary {
    /// Canonical server URL.
    pub origin: String,
    /// Tenant route key.
    pub tenant_key: TenantSlug,
    /// Access-token expiry.
    pub access_expires_at: DateTime<Utc>,
}

impl SavedLogin {
    /// Build the record of a login the device-code grant just issued.
    ///
    /// # Errors
    /// Returns [`WyrdClientError::SavedLogin`] with reason `refresh_refused`
    /// when the server issued no refresh token, so the login could not renew.
    pub fn from_token(
        origin: String,
        tenant_key: TenantSlug,
        token: TokenResponse,
    ) -> Result<Self, WyrdClientError> {
        let refresh_token = token.refresh_token.ok_or_else(|| {
            saved_login(
                "refresh_refused",
                "the server issued the login without a refresh token",
            )
        })?;
        Ok(Self {
            origin,
            tenant_key,
            access_token: token.access_token,
            access_expires_at: token.expires_at,
            refresh_token,
        })
    }

    /// The token-free projection of this record.
    #[must_use]
    pub fn summary(&self) -> SavedLoginSummary {
        SavedLoginSummary {
            origin: self.origin.clone(),
            tenant_key: self.tenant_key.clone(),
            access_expires_at: self.access_expires_at,
        }
    }

    /// Whether this is the login for `origin` and `tenant_key`.
    fn is_for(&self, origin: &str, tenant_key: &TenantSlug) -> bool {
        self.origin == origin && self.tenant_key == *tenant_key
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
    /// The credential file the logins live in.
    file: CredentialsFile,
}

impl SavedLogins {
    /// The saved logins in the user's Wyrd configuration directory, or
    /// `None` when no configuration directory can be resolved.
    #[must_use]
    pub fn locate() -> Option<Self> {
        CredentialsFile::locate().map(|file| Self { file })
    }

    /// The saved logins in `dir/credentials.toml`.
    #[must_use]
    pub fn at(dir: PathBuf) -> Self {
        Self {
            file: CredentialsFile::at(dir),
        }
    }

    /// Save a login the CLI just completed under the file lock, replacing
    /// any earlier login for the same origin and tenant.
    ///
    /// The record is written last, so it becomes the newest login for its
    /// server.
    ///
    /// # Errors
    /// Returns [`WyrdClientError::SavedLogin`] when the file is unsafe or
    /// corrupt or the write fails.
    pub fn save(&self, login: SavedLogin) -> Result<(), WyrdClientError> {
        let _lock = self.file.lock()?;
        let mut logins = self.read()?;
        logins.retain(|saved| !saved.is_for(&login.origin, &login.tenant_key));
        logins.push(login);
        self.write(&logins)
    }

    /// Every saved login, oldest first. An absent file or configuration
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
    /// the record for this origin with that tenant key is selected; when
    /// other tenants' records exist for this origin but none matches,
    /// selection fails rather than falling through. Without a selector, the
    /// newest login for this origin is selected. No record for this origin
    /// selects nothing.
    ///
    /// # Errors
    /// Returns [`WyrdClientError::SavedLogin`] with reason `tenant_mismatch`,
    /// and the errors of [`Self::list`].
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
        Ok(candidates.pop())
    }

    /// The renewing access-token source for `login`, exchanging through
    /// `exchange`.
    #[must_use]
    pub fn source(&self, login: &SavedLogin, exchange: TokenExchange) -> Arc<SavedLoginSource> {
        Arc::new(SavedLoginSource {
            store: self.clone(),
            origin: login.origin.clone(),
            tenant_key: login.tenant_key.clone(),
            identity: format!("saved-login:{}:{}", login.origin, login.tenant_key),
            exchange,
        })
    }

    /// Delete the login for `origin` and `tenant_key` under the file lock and
    /// return it, so the caller can revoke its refresh token.
    ///
    /// # Errors
    /// Returns [`WyrdClientError::SavedLogin`] when the file is unsafe or
    /// corrupt or the write fails.
    pub fn remove(
        &self,
        origin: &str,
        tenant_key: &TenantSlug,
    ) -> Result<Option<SavedLogin>, WyrdClientError> {
        let _lock = self.file.lock()?;
        let mut logins = self.read()?;
        let Some(index) = logins
            .iter()
            .position(|login| login.is_for(origin, tenant_key))
        else {
            return Ok(None);
        };
        let removed = logins.remove(index);
        self.write(&logins)?;
        Ok(Some(removed))
    }

    /// Return a usable access token for the login, renewing it under the file
    /// lock when it nears expiry.
    ///
    /// # Errors
    /// See [`SavedLoginSource::mint`].
    fn renew(
        &self,
        origin: &str,
        tenant_key: &TenantSlug,
        exchange: &TokenExchange,
    ) -> Result<MintedAccessToken, WyrdClientError> {
        let _lock = self.file.lock()?;
        let mut logins = self.read()?;
        let login = logins
            .iter_mut()
            .find(|login| login.is_for(origin, tenant_key))
            .ok_or_else(|| {
                saved_login(
                    "logged_out",
                    "the saved login was removed; run `wyrd auth login`",
                )
            })?;
        if login.access_expires_at - RENEW_MARGIN > Utc::now() {
            return Ok(MintedAccessToken {
                access_token: login.access_token.clone(),
                expires_at: login.access_expires_at,
            });
        }
        let handle = tokio::runtime::Handle::try_current().map_err(|_| {
            saved_login(
                "refresh_refused",
                "saved login renewal needs the client's async runtime",
            )
        })?;
        let rotated = handle
            .block_on(exchange.exchange(&TokenRequest::RefreshToken {
                refresh_token: login.refresh_token.clone(),
            }))
            .map_err(|error| match error {
                AuthError::Server(wyrd) => saved_login(
                    "refresh_refused",
                    format!(
                        "the server refused to renew the saved login ({}); run `wyrd auth login`",
                        wyrd.code()
                    ),
                ),
                AuthError::Client(client) => client,
            })?;
        login.refresh_token = rotated.refresh_token.ok_or_else(|| {
            saved_login(
                "refresh_refused",
                "the server renewed the saved login without a refresh token",
            )
        })?;
        login.access_token = rotated.access_token.clone();
        login.access_expires_at = rotated.expires_at;
        self.write(&logins)?;
        Ok(MintedAccessToken {
            access_token: rotated.access_token,
            expires_at: rotated.expires_at,
        })
    }

    /// Every saved login in `credentials.toml`.
    ///
    /// # Errors
    /// The errors of [`CredentialsFile::read_text`], and `corrupt` when the
    /// file is not TOML or a login does not decode.
    fn read(&self) -> Result<Vec<SavedLogin>, WyrdClientError> {
        let Some(text) = self.file.read_text()? else {
            return Ok(Vec::new());
        };
        let stored: StoredLogins = toml::from_str(&text).map_err(|_| {
            saved_login(
                "corrupt",
                format!(
                    "{} has saved logins that do not decode",
                    self.file.path().display()
                ),
            )
        })?;
        Ok(stored.logins)
    }

    /// Atomically replace `credentials.toml` with its current content and
    /// `logins` as its saved logins.
    ///
    /// The current file is reread and edited in place, so every other key,
    /// comment, and layout the user wrote survives; an empty `logins`
    /// removes the `[[logins]]` tables. Callers hold the file lock.
    ///
    /// # Errors
    /// Returns [`WyrdClientError::SavedLogin`] with the errors of
    /// [`CredentialsFile::document`] and [`CredentialsFile::replace`], and
    /// `corrupt` when a login does not encode; the previous file is then
    /// unchanged.
    fn write(&self, logins: &[SavedLogin]) -> Result<(), WyrdClientError> {
        let mut document = self.file.document()?;
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
        self.file.replace(&document)
    }
}

/// The renewing [`AccessTokenSource`] for one saved login.
///
/// Holds only where the record lives; every mint rereads it under the file
/// lock.
pub struct SavedLoginSource {
    /// The store holding the record.
    store: SavedLogins,
    /// Canonical origin the record is saved under.
    origin: String,
    /// Tenant route key the record is saved under.
    tenant_key: TenantSlug,
    /// Non-secret identity: origin and tenant key.
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
    /// `saved-login:{origin}:{tenant_key}`.
    fn identity(&self) -> &str {
        &self.identity
    }

    /// Return the saved access token, renewing it first when it nears expiry.
    ///
    /// Runs on the middleware's blocking pool: it takes the file lock,
    /// rereads the record, and returns its access token when it is still
    /// fresh, which may be one another client just saved. Otherwise the
    /// refresh token is exchanged once on the client's runtime and the
    /// rotated pair is saved before the lock is released.
    ///
    /// # Errors
    /// Returns [`WyrdClientError::SavedLogin`] with reason `logged_out` for a
    /// removed record, `refresh_refused` when the server refuses the refresh
    /// token, or `unsafe_store`, `corrupt`, or `io` from the file; and the
    /// transport error when the server cannot be reached, in which case the
    /// record is unchanged and a later call retries.
    fn mint(&self) -> Result<MintedAccessToken, WyrdClientError> {
        self.store
            .renew(&self.origin, &self.tenant_key, &self.exchange)
    }
}

/// The client-local saved-login error.
pub(crate) fn saved_login(reason: &'static str, message: impl Into<String>) -> WyrdClientError {
    WyrdClientError::SavedLogin {
        reason,
        message: message.into(),
    }
}

#[cfg(test)]
mod tests {
    use std::str::FromStr;

    use chrono::Utc;
    use wyrd_spec::auth::SecretBearer;
    use wyrd_spec::ids::TenantSlug;

    use super::{SavedLogin, SavedLogins, canonical_origin};

    /// A login record for `origin` and the tenant `key`.
    fn login(origin: &str, key: &str) -> SavedLogin {
        SavedLogin {
            origin: origin.to_owned(),
            tenant_key: TenantSlug::from_str(key).expect("slug"),
            access_token: SecretBearer::new("access-sentinel".to_owned()),
            access_expires_at: Utc::now(),
            refresh_token: SecretBearer::new("refresh-sentinel".to_owned()),
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

    /// Selection by origin and tenant: a tenant route key picks its tenant
    /// while a selector naming no record fails, no selector picks the newest
    /// login for the origin, and another origin selects nothing. A repeated
    /// login replaces its record and becomes the newest, and the status
    /// projection carries no token.
    #[test]
    fn selection_picks_the_named_or_newest_login() {
        let dir = tempfile::tempdir().expect("tempdir");
        let store = SavedLogins::at(dir.path().to_path_buf());
        let origin = "https://wyrd.example.com";
        let acme = login(origin, "acme");
        let globex = login(origin, "globex");
        store.save(acme.clone()).expect("saves");
        store.save(globex.clone()).expect("saves");
        assert!(
            store
                .select("https://other.example.com", None)
                .expect("selects")
                .is_none()
        );
        let newest = store.select(origin, None).expect("selects").expect("one");
        assert_eq!(newest.tenant_key, globex.tenant_key);
        let named = store
            .select(origin, Some("acme"))
            .expect("selects")
            .expect("one");
        assert_eq!(named.tenant_key, acme.tenant_key);
        let mismatch = store.select(origin, Some("initech")).expect_err("mismatch");
        assert_eq!(mismatch.code(), "WYRD_CLIENT_401_SAVED_LOGIN_UNUSABLE");
        assert!(mismatch.to_string().contains("tenant_mismatch"));

        store.save(acme.clone()).expect("logs in again");
        assert_eq!(store.list().expect("lists").len(), 2);
        let newest = store.select(origin, None).expect("selects").expect("one");
        assert_eq!(newest.tenant_key, acme.tenant_key);

        let printed = format!("{:?} {:?}", acme.summary(), acme);
        assert!(!printed.contains("sentinel"), "{printed}");
    }

    /// The saved logins share `credentials.toml` with the user's own
    /// content: a save and a removal carry every other key and comment
    /// through, removal hands back the record, and no other file or
    /// directory is left behind.
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
            vec![acme.clone(), globex.clone()]
        );
        let names: Vec<_> = std::fs::read_dir(dir.path())
            .expect("lists dir")
            .map(|entry| entry.expect("entry").file_name())
            .collect();
        assert_eq!(names, ["credentials.toml"]);

        for record in [&acme, &globex] {
            let removed = store
                .remove(&record.origin, &record.tenant_key)
                .expect("removes");
            assert_eq!(removed.as_ref(), Some(record));
        }
        assert!(
            store
                .remove(origin, &acme.tenant_key)
                .expect("removes nothing")
                .is_none()
        );
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
        std::fs::write(&file, b"[[logins]]\norigin = 1\n").expect("corrupts");
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
