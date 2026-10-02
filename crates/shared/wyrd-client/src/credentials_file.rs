//! The one Wyrd local credential file, `{wyrd_config_dir}/credentials.toml`.
//!
//! It holds the user's `[default].api_key`, the access token cached beside
//! that key ([`CredentialsFile::cache_api_key_token`]), and saved user logins
//! ([`crate::saved_login`]). Protection is the file's user-only ownership and
//! mode (`0600`) inside a directory only the user can change; a symlinked,
//! foreign-owned, or group/world-accessible file is refused rather than read.
//! The file is only ever replaced atomically (temporary file, `fsync`,
//! rename, directory `fsync`) under an exclusive OS lock on the configuration
//! directory, and every write carries the user's other keys and comments
//! through unchanged.

use std::fs::File;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use chrono::{DateTime, Utc};
use secrecy::{ExposeSecret, SecretString};
use serde::Deserialize;
use wyrd_spec::auth::SecretBearer;

use crate::error::WyrdClientError;
use crate::saved_login::saved_login;

/// File name of the one Wyrd credential file, under the configuration
/// directory.
const CREDENTIALS_FILE: &str = "credentials.toml";

/// The `credentials.toml` table holding the user's API key and its cached
/// access token.
const DEFAULT_KEY: &str = "default";

/// Longest a process waits for another process's hold on the file lock.
const LOCK_DEADLINE: Duration = Duration::from_secs(30);

/// Pause between attempts to take a held file lock.
const LOCK_RETRY: Duration = Duration::from_millis(25);

/// The `[default]` profile as the token cache reads it.
#[derive(Deserialize)]
struct DefaultProfileFile {
    /// `[default]`; absent means no profile.
    default: Option<DefaultProfile>,
}

/// `[default].api_key` and the access token cached beside it.
#[derive(Deserialize)]
struct DefaultProfile {
    /// The user's API key.
    api_key: Option<String>,
    /// Access token last issued for `api_key`.
    access_token: Option<String>,
    /// When `access_token` expires.
    access_expires_at: Option<DateTime<Utc>>,
}

/// The Wyrd credential file in one configuration directory.
#[derive(Debug, Clone)]
pub(crate) struct CredentialsFile {
    /// `{wyrd_config_dir}`, or a caller-chosen directory.
    dir: PathBuf,
}

/// An exclusive hold on the configuration directory's OS lock, released on
/// drop.
pub(crate) struct CredentialsLock {
    /// The open directory; the OS lock lives as long as this handle.
    _dir: File,
}

impl CredentialsFile {
    /// The credential file in the user's Wyrd configuration directory, or
    /// `None` when no configuration directory can be resolved.
    pub(crate) fn locate() -> Option<Self> {
        wyrd_utils::config_dir::wyrd_config_dir().map(Self::at)
    }

    /// The credential file `dir/credentials.toml`.
    pub(crate) fn at(dir: PathBuf) -> Self {
        Self { dir }
    }

    /// The access token cached beside `[default].api_key`, with its expiry,
    /// when that key is `api_key`.
    ///
    /// Any other key, an unsafe or unreadable file, or no cached token
    /// yields `None`, so a token is only ever reused by the key it was
    /// issued for. Freshness is the caller's decision.
    pub(crate) fn cached_api_key_token(
        &self,
        api_key: &SecretString,
    ) -> Option<(SecretBearer, DateTime<Utc>)> {
        let text = self.read_text().ok()??;
        let profile = toml::from_str::<DefaultProfileFile>(&text).ok()?.default?;
        if profile.api_key.as_deref() != Some(api_key.expose_secret()) {
            return None;
        }
        Some((
            SecretBearer::new(profile.access_token?),
            profile.access_expires_at?,
        ))
    }

    /// Cache `access_token` beside `[default].api_key` when that key is
    /// `api_key`, so every process using that key reuses the token.
    ///
    /// Under the file lock the file is reread; when its key is another or
    /// absent nothing is written. Otherwise `access_token` and
    /// `access_expires_at` are set in `[default]` and the file is replaced
    /// atomically with the user's other content unchanged.
    ///
    /// # Errors
    /// Returns [`WyrdClientError::SavedLogin`] with reason `unsafe_store`,
    /// `lock_timeout`, `corrupt`, or `io` from [`Self::lock`],
    /// [`Self::document`], and [`Self::replace`]; the file is then unchanged.
    pub(crate) fn cache_api_key_token(
        &self,
        api_key: &SecretString,
        access_token: &SecretBearer,
        expires_at: DateTime<Utc>,
    ) -> Result<(), WyrdClientError> {
        let _lock = self.lock()?;
        let mut document = self.document()?;
        let Some(profile) = document
            .get_mut(DEFAULT_KEY)
            .and_then(toml_edit::Item::as_table_like_mut)
        else {
            return Ok(());
        };
        if profile.get("api_key").and_then(toml_edit::Item::as_str) != Some(api_key.expose_secret())
        {
            return Ok(());
        }
        profile.insert("access_token", toml_edit::value(access_token.expose()));
        profile.insert(
            "access_expires_at",
            toml_edit::value(expires_at.to_rfc3339()),
        );
        self.replace(&document)
    }

    /// The current file as an editable document; empty when the file or the
    /// configuration directory does not exist.
    ///
    /// # Errors
    /// The errors of [`Self::read_text`], and `corrupt` when the file is not
    /// TOML.
    pub(crate) fn document(&self) -> Result<toml_edit::DocumentMut, WyrdClientError> {
        let Some(text) = self.read_text()? else {
            return Ok(toml_edit::DocumentMut::new());
        };
        text.parse::<toml_edit::DocumentMut>().map_err(|_| {
            saved_login(
                "corrupt",
                format!("{} is not valid TOML", self.path().display()),
            )
        })
    }

    /// Atomically replace the file with `document`.
    ///
    /// The text goes to a `0600` temporary file in the same directory, is
    /// `fsync`ed, renamed over the file, and the directory is `fsync`ed.
    /// Callers hold [`Self::lock`] and built `document` from
    /// [`Self::document`] under it, so the user's other content survives.
    ///
    /// # Errors
    /// Returns [`WyrdClientError::SavedLogin`] with reason `io` when any
    /// write, `fsync`, or rename fails; the previous file is then unchanged.
    pub(crate) fn replace(&self, document: &toml_edit::DocumentMut) -> Result<(), WyrdClientError> {
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

    /// Path of `credentials.toml`.
    pub(crate) fn path(&self) -> PathBuf {
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
    pub(crate) fn lock(&self) -> Result<CredentialsLock, WyrdClientError> {
        self.check_dir(true)?;
        // ponytail: Windows cannot prove a user-only file or lock a directory;
        // credential-file writes fail closed there until a Windows ACL check is added.
        #[cfg(not(unix))]
        return Err(unsafe_store(
            &self.dir,
            "cannot write the credential file on a platform without POSIX file permissions",
        ));
        #[cfg(unix)]
        {
            let dir = File::open(&self.dir).map_err(|error| io_failure(&error))?;
            let deadline = Instant::now() + LOCK_DEADLINE;
            loop {
                match dir.try_lock() {
                    Ok(()) => return Ok(CredentialsLock { _dir: dir }),
                    Err(std::fs::TryLockError::WouldBlock) if Instant::now() < deadline => {
                        std::thread::sleep(LOCK_RETRY);
                    }
                    Err(std::fs::TryLockError::WouldBlock) => {
                        return Err(saved_login(
                            "lock_timeout",
                            "another Wyrd process held the credential file too long",
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
    pub(crate) fn read_text(&self) -> Result<Option<String>, WyrdClientError> {
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
}

/// An unsafe credential-file entry.
pub(crate) fn unsafe_store(path: &Path, problem: &str) -> WyrdClientError {
    saved_login("unsafe_store", format!("{} {problem}", path.display()))
}

/// A local IO failure on the credential file.
pub(crate) fn io_failure(error: &std::io::Error) -> WyrdClientError {
    saved_login("io", format!("credential file IO failed: {error}"))
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
