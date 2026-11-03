//! Local password policy and verification for tenant Users.
//!
//! A local User signs in with a canonical tenant-local username and a
//! password. Passwords are normalized to NFC, must be 15–128 characters with
//! no control characters, and are refused when they appear in the bundled
//! common-password list, compared case-insensitively. The list is the
//! 15–128-character, control-free, lowercase-deduplicated subset of
//! `xato-net-10-million-passwords-1000000.txt` from `SecLists` (MIT licence,
//! Daniel Miessler and contributors). The verifier is one Argon2id PHC string
//! at the library's baseline parameters (19456 KiB, two passes, one lane) with
//! a fresh random salt; a verifier at older parameters is replaced after the
//! next successful verification.
//!
//! Every key-derivation call holds one of four process-wide permits and runs
//! off the async workers, and raw input above 1024 bytes is refused before
//! any normalization or derivation, so password work stays bounded.

use std::net::IpAddr;
use std::sync::LazyLock;

use argon2::password_hash::rand_core::OsRng;
use argon2::password_hash::{PasswordHash, PasswordHasher as _, PasswordVerifier as _, SaltString};
use argon2::{Argon2, Params};
use secrecy::{ExposeSecret as _, SecretString};
use serde_json::json;
use tokio::sync::Semaphore;
use unicode_normalization::UnicodeNormalization as _;
use uuid::Uuid;
use wyrd_runtime::Permission;
use wyrd_spec::auth::{
    BrowserCredential, PasswordChangeRequest, PasswordLoginRequest, PasswordResetRequest,
    PasswordSignupRequest, RoleSource, SecretBearer, Sha256Hex,
};
use wyrd_spec::error::WyrdError;
use wyrd_sql::TenantConn;
use wyrd_sql::queries::auth::{
    AttemptScope, BrowserSessionAuthority, InvitationRow, PasswordResetRow, attempt_refusal,
    consume_invitation, consume_password_reset, grant_role_to_user, insert_invitation,
    insert_local_user, insert_password_reset, local_credential_by_username,
    local_credential_for_update, record_attempt_failure, role_by_id, set_password_hash,
};

use crate::browser_sessions::{
    BrowserSession, BrowserSessions, CreatedBrowserSession, create_session, random_secret,
    session_invalid,
};
use crate::error::store_error;
use crate::issuance::TenantGrant;

/// Largest raw password or code input accepted, in bytes, before any costly
/// processing.
pub const PASSWORD_INPUT_MAX_BYTES: usize = 1024;

/// Fewest characters in a new password, after NFC normalization.
pub const PASSWORD_MIN_CHARS: usize = 15;

/// Most characters in a new password, after NFC normalization.
pub const PASSWORD_MAX_CHARS: usize = 128;

/// Concurrent key-derivation operations allowed per process.
const KDF_PERMITS: usize = 4;

/// The process-wide key-derivation permits.
static KDF: Semaphore = Semaphore::const_new(KDF_PERMITS);

/// The bundled common-password list, one lowercase NFC entry per line, sorted.
static COMMON_PASSWORDS: &str = include_str!("common_passwords.txt");

/// A verifier of a random secret no one holds, checked when a username has no
/// usable local credential so that refusal costs one derivation either way.
static DUMMY_VERIFIER: LazyLock<String> = LazyLock::new(|| {
    hash_now(&random_secret())
        .expect("baseline Argon2id derivation of a 43-byte secret cannot fail")
});

/// The common-password entries, in the list's sorted order.
static COMMON: LazyLock<Vec<&'static str>> = LazyLock::new(|| COMMON_PASSWORDS.lines().collect());

/// The canonical form of a tenant-local username: surrounding whitespace
/// trimmed and ASCII lowercased, then 3–64 characters, the first a letter or
/// digit and the rest letters, digits, `.`, `_` or `-`.
///
/// # Errors
/// Returns [`WyrdError::Validation`] for any other value.
pub fn canonical_username(raw: &str) -> Result<String, WyrdError> {
    let name = raw.trim().to_ascii_lowercase();
    let mut chars = name.chars();
    let valid = (3..=64).contains(&name.len())
        && chars.next().is_some_and(|c| c.is_ascii_alphanumeric())
        && chars.all(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '_' | '-'));
    if valid {
        Ok(name)
    } else {
        Err(WyrdError::Validation {
            message: "username must be 3-64 characters: a letter or digit, then letters, \
                      digits, '.', '_' or '-'"
                .to_owned(),
            details: json!({ "field": "username" }),
        })
    }
}

/// The NFC form of a presented password, refused above
/// [`PASSWORD_INPUT_MAX_BYTES`] raw bytes before normalization. No policy is
/// applied: a presented current password is only compared.
///
/// # Errors
/// Returns `oversize` for an oversized input.
fn normalized(
    raw: &SecretString,
    oversize: impl FnOnce() -> WyrdError,
) -> Result<SecretString, WyrdError> {
    let raw = raw.expose_secret();
    if raw.len() > PASSWORD_INPUT_MAX_BYTES {
        return Err(oversize());
    }
    Ok(SecretString::from(raw.nfc().collect::<String>()))
}

/// The NFC form of a password presented for comparison.
///
/// # Errors
/// Returns [`WyrdError::PasswordInvalid`] for an input over
/// [`PASSWORD_INPUT_MAX_BYTES`].
pub fn presented_password(raw: &SecretString) -> Result<SecretString, WyrdError> {
    normalized(raw, password_invalid)
}

/// The NFC form of a new password that satisfies the policy.
///
/// # Errors
/// Returns [`WyrdError::Validation`] naming `field` for an oversized,
/// too short or too long, control-character or common password.
pub fn new_password(raw: &SecretString, field: &str) -> Result<SecretString, WyrdError> {
    let refuse = |message: &str| WyrdError::Validation {
        message: message.to_owned(),
        details: json!({ "field": field }),
    };
    let password = normalized(raw, || refuse("password is longer than 1024 bytes"))?;
    let text = password.expose_secret();
    let chars = text.chars().count();
    if !(PASSWORD_MIN_CHARS..=PASSWORD_MAX_CHARS).contains(&chars) {
        return Err(refuse("password must be 15-128 characters"));
    }
    if text.chars().any(char::is_control) {
        return Err(refuse("password must not contain control characters"));
    }
    if is_common(text) {
        return Err(refuse(
            "password is too common; choose a less predictable one",
        ));
    }
    Ok(password)
}

/// Whether `password` appears in the bundled common-password list.
fn is_common(password: &str) -> bool {
    COMMON
        .binary_search(&password.to_lowercase().as_str())
        .is_ok()
}

/// A fresh Argon2id PHC verifier for an already admitted password.
///
/// # Errors
/// Returns [`WyrdError::Internal`] when derivation fails.
pub async fn hash_password(password: SecretString) -> Result<String, WyrdError> {
    bounded(move || hash_now(password.expose_secret())).await?
}

/// The outcome of checking a presented password.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PasswordCheck {
    /// The password does not match, or there was no usable verifier.
    Refused,
    /// The password matches; `rehash` carries a replacement verifier when the
    /// stored one uses older parameters.
    Matched {
        /// Replacement verifier at the current parameters, when needed.
        rehash: Option<String>,
    },
}

/// Check `password` against `verifier`, or against a dummy verifier when
/// there is none, so an unknown user costs the same derivation.
///
/// # Errors
/// Returns [`WyrdError::Internal`] when derivation cannot run.
///
/// # Panics
/// Never in practice: only the dummy verifier's one-time derivation of a
/// random secret uses `expect`, and baseline derivation cannot fail.
pub async fn check_password(
    password: SecretString,
    verifier: Option<String>,
) -> Result<PasswordCheck, WyrdError> {
    bounded(move || {
        let known = verifier.is_some();
        let stored = verifier.unwrap_or_else(|| DUMMY_VERIFIER.clone());
        let Ok(parsed) = PasswordHash::new(&stored) else {
            return Ok(PasswordCheck::Refused);
        };
        let matched = Argon2::default()
            .verify_password(password.expose_secret().as_bytes(), &parsed)
            .is_ok();
        if !(known && matched) {
            return Ok(PasswordCheck::Refused);
        }
        let current = parsed.algorithm == argon2::Algorithm::Argon2id.ident()
            && Params::try_from(&parsed).is_ok_and(|params| {
                let baseline = Params::default();
                (params.m_cost(), params.t_cost(), params.p_cost())
                    == (baseline.m_cost(), baseline.t_cost(), baseline.p_cost())
            });
        let rehash = if current {
            None
        } else {
            Some(hash_now(password.expose_secret())?)
        };
        Ok(PasswordCheck::Matched { rehash })
    })
    .await?
}

/// Run one derivation under a process permit on the blocking pool.
///
/// # Errors
/// Returns [`WyrdError::Internal`] when the permit or blocking task fails.
async fn bounded<T: Send + 'static>(
    work: impl FnOnce() -> T + Send + 'static,
) -> Result<T, WyrdError> {
    let _permit = KDF.acquire().await.map_err(|_| kdf_failed())?;
    tokio::task::spawn_blocking(work)
        .await
        .map_err(|_| kdf_failed())
}

/// Derive a PHC verifier synchronously.
///
/// # Errors
/// Returns [`WyrdError::Internal`] when derivation fails.
fn hash_now(password: &str) -> Result<String, WyrdError> {
    Argon2::default()
        .hash_password(password.as_bytes(), &SaltString::generate(&mut OsRng))
        .map(|hash| hash.to_string())
        .map_err(|_| kdf_failed())
}

/// The internal failure of password derivation.
fn kdf_failed() -> WyrdError {
    WyrdError::Internal {
        message: "password derivation failed".to_owned(),
        details: json!({}),
    }
}

/// The one public refusal for any unusable username/password pair.
#[must_use]
pub fn password_invalid() -> WyrdError {
    WyrdError::PasswordInvalid {
        message: "username or password not accepted".to_owned(),
        details: json!({}),
    }
}

/// Failures admitted per tenant and canonical username in one window.
const ACCOUNT_ATTEMPTS: i32 = 10;

/// Failures admitted per tenant and connection source in one window.
const SOURCE_ATTEMPTS: i32 = 100;

/// Length of an encoded one-time code: 32 random bytes, unpadded base64url.
const ONE_TIME_CODE_CHARS: usize = 43;

/// The SHA-256 of a well-formed one-time code, which names its row.
///
/// # Errors
/// Returns [`WyrdError::OneTimeCodeInvalid`] for any value that is not 43
/// base64url characters, before any lookup or derivation.
fn code_hash(code: &SecretBearer) -> Result<Sha256Hex, WyrdError> {
    let code = code.expose();
    let well_formed = code.len() == ONE_TIME_CODE_CHARS
        && code
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'-' | b'_'));
    if well_formed {
        Ok(Sha256Hex::digest(code.as_bytes()))
    } else {
        Err(one_time_code_invalid())
    }
}

/// The one refusal for an unusable invitation or reset code.
fn one_time_code_invalid() -> WyrdError {
    WyrdError::OneTimeCodeInvalid {
        message: "the code is not valid; ask the tenant operator for a new one".to_owned(),
        details: json!({}),
    }
}

/// Map a local-User insert failure: a taken username is a validation refusal.
fn enrollment_error(error: sqlx::Error) -> WyrdError {
    match &error {
        sqlx::Error::Database(db) if db.code().as_deref() == Some("23505") => {
            WyrdError::Validation {
                message: "that username is already taken in this tenant".to_owned(),
                details: json!({ "field": "username" }),
            }
        }
        _ => store_error(error),
    }
}

/// Create an active local User named by the canonical `username` with the
/// verifier of `password`, holding the direct Roles `role_ids`, on `conn`;
/// returns the new User id.
///
/// The one creation owner behind signup and operator setup. The caller owns
/// authority for the grants and commits `conn`.
///
/// # Errors
/// Returns [`WyrdError::Validation`] for a taken username or a missing Role,
/// and [`WyrdError::AuthVerifyUnavailable`] when the store fails.
pub async fn create_local_user(
    conn: &mut TenantConn<'_>,
    username: &str,
    verifier: &str,
    role_ids: &[Uuid],
) -> Result<Uuid, WyrdError> {
    let user_id = Uuid::now_v7();
    insert_local_user(conn, user_id, username, verifier)
        .await
        .map_err(enrollment_error)?;
    grant_roles(conn, user_id, role_ids).await?;
    Ok(user_id)
}

/// Grant `user_id` each existing Role in `role_ids` directly.
///
/// # Errors
/// Returns [`WyrdError::Validation`] for a Role id that names no Role, and
/// [`WyrdError::AuthVerifyUnavailable`] when the store fails.
async fn grant_roles(
    conn: &mut TenantConn<'_>,
    user_id: Uuid,
    role_ids: &[Uuid],
) -> Result<(), WyrdError> {
    for role_id in role_ids {
        if role_by_id(conn, *role_id)
            .await
            .map_err(store_error)?
            .is_none()
        {
            return Err(WyrdError::Validation {
                message: "a Role does not exist in this tenant".to_owned(),
                details: json!({ "role_id": role_id }),
            });
        }
        grant_role_to_user(conn, user_id, *role_id, RoleSource::Direct)
            .await
            .map_err(store_error)?;
    }
    Ok(())
}

/// Record a one-use enrollment invitation for the existing `role_ids`,
/// issued by `issuer`, on `conn`; returns it with its code, shown once.
///
/// The caller has authorized the issuer to grant those Roles and commits.
///
/// # Errors
/// Returns [`WyrdError::Validation`] for an empty, repeated or missing Role
/// id, and [`WyrdError::AuthVerifyUnavailable`] when the store fails.
pub async fn issue_invitation(
    conn: &mut TenantConn<'_>,
    issuer: Uuid,
    role_ids: &[Uuid],
) -> Result<(InvitationRow, SecretBearer), WyrdError> {
    let mut distinct = role_ids.to_vec();
    distinct.sort_unstable();
    distinct.dedup();
    if role_ids.is_empty() || distinct.len() != role_ids.len() {
        return Err(WyrdError::Validation {
            message: "role_ids must be a nonempty list of distinct Role ids".to_owned(),
            details: json!({ "field": "role_ids" }),
        });
    }
    for role_id in &distinct {
        if role_by_id(conn, *role_id)
            .await
            .map_err(store_error)?
            .is_none()
        {
            return Err(WyrdError::Validation {
                message: "a Role does not exist in this tenant".to_owned(),
                details: json!({ "role_id": role_id }),
            });
        }
    }
    let code = SecretBearer::new(random_secret());
    let row = insert_invitation(
        conn,
        Uuid::now_v7(),
        &Sha256Hex::digest(code.expose().as_bytes()),
        issuer,
        &distinct,
    )
    .await
    .map_err(store_error)?;
    Ok((row, code))
}

/// Record a one-use password reset for the local User `user_id`, issued by
/// `issuer`, on `conn`; returns it with its code, shown once. `None` when the
/// id names no local password User.
///
/// # Errors
/// Returns [`WyrdError::AuthVerifyUnavailable`] when the store fails.
pub async fn issue_password_reset(
    conn: &mut TenantConn<'_>,
    issuer: Uuid,
    user_id: Uuid,
) -> Result<Option<(PasswordResetRow, SecretBearer)>, WyrdError> {
    let Some(user) = local_credential_for_update(conn, user_id)
        .await
        .map_err(store_error)?
    else {
        return Ok(None);
    };
    let code = SecretBearer::new(random_secret());
    let row = insert_password_reset(
        conn,
        Uuid::now_v7(),
        &Sha256Hex::digest(code.expose().as_bytes()),
        user.id,
        user.password_revision,
        issuer,
    )
    .await
    .map_err(store_error)?;
    Ok(Some((row, code)))
}

impl BrowserSessions {
    /// Enroll a local User with a one-use invitation and sign the browser in.
    ///
    /// The code's shape, the username and the password policy are checked
    /// before any lookup or derivation. One tenant transaction then creates
    /// the User, consumes the invitation (exactly one of two concurrent
    /// enrollments with a code succeeds), requires its issuer to still hold
    /// tenant administration, grants its Roles and creates the session; any
    /// refusal rolls back the whole enrollment, so no partial User or grant
    /// remains.
    ///
    /// # Errors
    /// Returns [`WyrdError::Validation`] for a username or password outside
    /// policy or a taken username, [`WyrdError::OneTimeCodeInvalid`] for an
    /// unknown tenant or an unusable invitation, and
    /// [`WyrdError::AuthVerifyUnavailable`] when the store fails.
    pub async fn password_signup(
        &self,
        request: PasswordSignupRequest,
        request_id: &str,
    ) -> Result<CreatedBrowserSession, WyrdError> {
        let code = code_hash(&request.invitation_code)?;
        let username = canonical_username(&request.username)?;
        let password = new_password(&request.password.into_secret_string(), "password")?;
        let postgres = self.connections.postgres();
        let tenant = postgres
            .resolve_tenant_slug(&request.tenant)
            .await
            .map_err(store_error)?
            .ok_or_else(one_time_code_invalid)?;
        let verifier = hash_password(password).await?;
        let mut conn = postgres.tenant_conn(tenant).await.map_err(store_error)?;
        let user_id = Uuid::now_v7();
        insert_local_user(&mut conn, user_id, &username, &verifier)
            .await
            .map_err(enrollment_error)?;
        let invitation = consume_invitation(&mut conn, &code, user_id)
            .await
            .map_err(store_error)?
            .ok_or_else(one_time_code_invalid)?;
        if !self
            .issuer
            .currently_holds(
                &mut conn,
                invitation.issuer_principal_id,
                &Permission::wildcard(),
            )
            .await?
        {
            tracing::info!("invitation issuer no longer holds tenant administration");
            return Err(one_time_code_invalid());
        }
        grant_roles(&mut conn, user_id, &invitation.role_ids)
            .await
            .map_err(|_| one_time_code_invalid())?;
        let created = self.password_session(&mut conn, user_id, 0).await?;
        conn.commit().await.map_err(store_error)?;
        self.stage_login(&created.session, request_id);
        Ok(created)
    }

    /// Sign a local User in with username and password.
    ///
    /// The tenant's account and source windows are checked before any
    /// derivation; an unknown tenant, malformed or unknown username, missing
    /// local credential, inactive User and wrong password all cost one
    /// derivation and give the one [`WyrdError::PasswordInvalid`] refusal,
    /// and each counts one failure in both windows. The admission read and
    /// the session write run in separate transactions around the derivation,
    /// so no pooled connection waits on password work; the session is
    /// created only when the credential revision is still the one verified.
    ///
    /// # Errors
    /// Returns [`WyrdError::PasswordInvalid`] for any refusal above,
    /// [`WyrdError::PasswordAttempts`] while a window is exhausted, and
    /// [`WyrdError::AuthVerifyUnavailable`] when the store fails.
    pub async fn password_login(
        &self,
        request: PasswordLoginRequest,
        source: Option<IpAddr>,
        request_id: &str,
    ) -> Result<CreatedBrowserSession, WyrdError> {
        let password = presented_password(&request.password.into_secret_string())?;
        let postgres = self.connections.postgres();
        let Some(tenant) = postgres
            .resolve_tenant_slug(&request.tenant)
            .await
            .map_err(store_error)?
        else {
            check_password(password, None).await?;
            return Err(password_invalid());
        };
        let username = canonical_username(&request.username).unwrap_or_default();
        let windows = Windows::new(&username, source);
        let stored = {
            let mut conn = postgres.tenant_conn(tenant).await.map_err(store_error)?;
            windows.admit(&mut conn).await?;
            let stored = local_credential_by_username(&mut conn, &username)
                .await
                .map_err(store_error)?
                .filter(|row| row.status == "active");
            conn.commit().await.map_err(store_error)?;
            stored
        };
        let check = check_password(
            password,
            stored.as_ref().map(|row| row.password_hash.clone()),
        )
        .await?;
        let mut conn = postgres.tenant_conn(tenant).await.map_err(store_error)?;
        let (PasswordCheck::Matched { rehash }, Some(stored)) = (check, stored) else {
            return Err(windows.refuse(conn).await);
        };
        let current = local_credential_for_update(&mut conn, stored.id)
            .await
            .map_err(store_error)?
            .filter(|row| row.password_revision == stored.password_revision);
        if current.is_none() {
            return Err(windows.refuse(conn).await);
        }
        if let Some(rehash) = rehash {
            set_password_hash(&mut conn, stored.id, &rehash, false)
                .await
                .map_err(store_error)?;
        }
        let created = self
            .password_session(&mut conn, stored.id, stored.password_revision)
            .await?;
        conn.commit().await.map_err(store_error)?;
        self.stage_login(&created.session, request_id);
        Ok(created)
    }

    /// Change the signed-in local User's password after verifying the
    /// current one.
    ///
    /// The session must be a password session of `request.tenant`'s tenant.
    /// The current password is rate-limited as a login is; the new password
    /// must satisfy the policy. Success advances the password revision, so
    /// every local session of the User — this one included — stops
    /// resolving and every outstanding reset code is spent; API keys are
    /// untouched.
    ///
    /// # Errors
    /// Returns [`WyrdError::InvalidToken`] for a session of another tenant or
    /// another kind, [`WyrdError::Validation`] for a new password outside
    /// policy, [`WyrdError::PasswordInvalid`] for a wrong current password,
    /// [`WyrdError::PasswordAttempts`] while a window is exhausted, and
    /// [`WyrdError::AuthVerifyUnavailable`] when the store fails.
    pub async fn password_change(
        &self,
        session: &BrowserSession,
        request: PasswordChangeRequest,
        source: Option<IpAddr>,
    ) -> Result<(), WyrdError> {
        let principal = &session.verified.principal;
        let postgres = self.connections.postgres();
        let routed = postgres
            .resolve_tenant_slug(&request.tenant)
            .await
            .map_err(store_error)?;
        if session.credential != BrowserCredential::Password || routed != Some(principal.tenant_id)
        {
            return Err(session_invalid());
        }
        let current = presented_password(&request.current_password.into_secret_string())?;
        let replacement = new_password(&request.new_password.into_secret_string(), "new_password")?;
        let user_id = principal.id.as_uuid();
        let stored = {
            let mut conn = postgres
                .tenant_conn(principal.tenant_id)
                .await
                .map_err(store_error)?;
            let stored = local_credential_for_update(&mut conn, user_id)
                .await
                .map_err(store_error)?
                .ok_or_else(session_invalid)?;
            Windows::new(&stored.username, source)
                .admit(&mut conn)
                .await?;
            conn.commit().await.map_err(store_error)?;
            stored
        };
        let windows = Windows::new(&stored.username, source);
        let check = check_password(current, Some(stored.password_hash.clone())).await?;
        let mut conn = postgres
            .tenant_conn(principal.tenant_id)
            .await
            .map_err(store_error)?;
        if check == PasswordCheck::Refused {
            return Err(windows.refuse(conn).await);
        }
        let verifier = hash_password(replacement).await?;
        let unchanged = local_credential_for_update(&mut conn, user_id)
            .await
            .map_err(store_error)?
            .is_some_and(|row| row.password_revision == stored.password_revision);
        if !unchanged {
            return Err(session_invalid());
        }
        set_password_hash(&mut conn, user_id, &verifier, true)
            .await
            .map_err(store_error)?;
        conn.commit().await.map_err(store_error)
    }

    /// Set a local User's password with a one-use operator reset code.
    ///
    /// The code is checked for shape and the new password for policy before
    /// any lookup or derivation. One transaction then consumes the code and
    /// requires it to name an active local User whose password revision is
    /// still the one it was issued at; success advances the revision,
    /// ending every local session and spending every other reset code. Reset
    /// never signs the browser in.
    ///
    /// # Errors
    /// Returns [`WyrdError::Validation`] for a new password outside policy,
    /// [`WyrdError::OneTimeCodeInvalid`] for an unknown tenant or an unusable
    /// code, and [`WyrdError::AuthVerifyUnavailable`] when the store fails.
    pub async fn password_reset(&self, request: PasswordResetRequest) -> Result<(), WyrdError> {
        let code = code_hash(&request.reset_code)?;
        let replacement = new_password(&request.new_password.into_secret_string(), "new_password")?;
        let postgres = self.connections.postgres();
        let tenant = postgres
            .resolve_tenant_slug(&request.tenant)
            .await
            .map_err(store_error)?
            .ok_or_else(one_time_code_invalid)?;
        let verifier = hash_password(replacement).await?;
        let mut conn = postgres.tenant_conn(tenant).await.map_err(store_error)?;
        let reset = consume_password_reset(&mut conn, &code)
            .await
            .map_err(store_error)?
            .ok_or_else(one_time_code_invalid)?;
        let current = local_credential_for_update(&mut conn, reset.user_id)
            .await
            .map_err(store_error)?
            .is_some_and(|row| {
                row.status == "active" && row.password_revision == reset.password_revision
            });
        if !current {
            return Err(one_time_code_invalid());
        }
        set_password_hash(&mut conn, reset.user_id, &verifier, true)
            .await
            .map_err(store_error)?;
        conn.commit().await.map_err(store_error)
    }

    /// Create a password session for `user_id` at `revision` on `conn`.
    ///
    /// # Errors
    /// Every [`TenantTokenIssuer::verify`] refusal, and
    /// [`WyrdError::AuthVerifyUnavailable`] when the insert fails.
    async fn password_session(
        &self,
        conn: &mut TenantConn<'_>,
        user_id: Uuid,
        revision: i64,
    ) -> Result<CreatedBrowserSession, WyrdError> {
        let verified = self
            .issuer
            .verify(conn, user_id, TenantGrant::PasswordLogin)
            .await?;
        create_session(conn, verified, BrowserSessionAuthority::Password(revision)).await
    }
}

/// The two failure windows one password verification is admitted under.
struct Windows {
    /// The canonical username, empty for a malformed one.
    account: String,
    /// The connection's peer address, when known.
    source: Option<String>,
}

impl Windows {
    /// The windows for `username` from `source`.
    ///
    /// ponytail: the source is the TCP peer, so behind the UI's server it
    /// aggregates every browser that server forwards; a trusted-proxy
    /// setting would key it by the forwarded client.
    fn new(username: &str, source: Option<IpAddr>) -> Self {
        Self {
            account: if username.is_empty() {
                "-".to_owned()
            } else {
                username.to_owned()
            },
            source: source.map(|ip| ip.to_string()),
        }
    }

    /// Refuse when either window is exhausted, with the longer wait.
    ///
    /// # Errors
    /// Returns [`WyrdError::PasswordAttempts`] with `retry_after_seconds`, and
    /// [`WyrdError::AuthVerifyUnavailable`] when the read fails.
    async fn admit(&self, conn: &mut TenantConn<'_>) -> Result<(), WyrdError> {
        let mut wait =
            attempt_refusal(conn, AttemptScope::Account, &self.account, ACCOUNT_ATTEMPTS)
                .await
                .map_err(store_error)?;
        if let Some(source) = &self.source {
            let source_wait = attempt_refusal(conn, AttemptScope::Source, source, SOURCE_ATTEMPTS)
                .await
                .map_err(store_error)?;
            wait = wait.max(source_wait);
        }
        match wait {
            None => Ok(()),
            Some(wait) => Err(WyrdError::PasswordAttempts {
                message: "too many password attempts; wait before retrying".to_owned(),
                details: json!({ "retry_after_seconds": wait.as_secs() }),
            }),
        }
    }

    /// Count one failure in both windows on `conn`, commit it, and return the
    /// public refusal.
    async fn refuse(&self, mut conn: TenantConn<'_>) -> WyrdError {
        let mut recorded =
            record_attempt_failure(&mut conn, AttemptScope::Account, &self.account).await;
        if let (Ok(()), Some(source)) = (&recorded, &self.source) {
            recorded = record_attempt_failure(&mut conn, AttemptScope::Source, source).await;
        }
        match recorded {
            Ok(()) => match conn.commit().await {
                Ok(()) => password_invalid(),
                Err(error) => store_error(error),
            },
            Err(error) => store_error(error),
        }
    }
}

#[cfg(test)]
mod tests {
    use secrecy::{ExposeSecret as _, SecretString};
    use wyrd_spec::error::WyrdError;

    use super::{PasswordCheck, canonical_username, check_password, hash_password, new_password};

    /// Usernames trim and lowercase, and keep only the canonical handle shape.
    #[test]
    fn usernames_canonicalize_or_refuse() {
        assert_eq!(canonical_username("  Dev.User_1 ").unwrap(), "dev.user_1");
        for bad in ["ab", "-dev", "dév", "a b c", &"a".repeat(65)] {
            assert!(canonical_username(bad).is_err(), "{bad} accepted");
        }
    }

    /// New passwords are NFC, length-bounded, control-free and uncommon.
    #[test]
    fn new_passwords_follow_the_policy() {
        let ok = |raw: &str| new_password(&SecretString::from(raw.to_owned()), "password");
        // "e" + combining acute normalizes to one character.
        let composed = ok("Tr0ub4dor & e\u{301}xtra").unwrap();
        assert!(composed.expose_secret().contains('\u{e9}'));
        assert!(ok("Correct Horse Battery").is_ok());
        for bad in [
            "short pass",
            &"x".repeat(129),
            "fifteen chars\u{7}ok",
            "Password123456789",
            &"y".repeat(1025),
        ] {
            assert!(
                matches!(ok(bad), Err(WyrdError::Validation { .. })),
                "{bad:?} accepted"
            );
        }
    }

    /// A verifier matches only its password and needs no rehash at baseline.
    #[tokio::test]
    async fn verifiers_match_only_their_password() {
        let password = SecretString::from("Correct Horse Battery".to_owned());
        let stored = hash_password(password.clone()).await.unwrap();
        assert!(stored.starts_with("$argon2id$v=19$m=19456,t=2,p=1$"));
        assert_eq!(
            check_password(password.clone(), Some(stored.clone()))
                .await
                .unwrap(),
            PasswordCheck::Matched { rehash: None }
        );
        let wrong = SecretString::from("Incorrect Horse Battery".to_owned());
        assert_eq!(
            check_password(wrong, Some(stored)).await.unwrap(),
            PasswordCheck::Refused
        );
        assert_eq!(
            check_password(password, None).await.unwrap(),
            PasswordCheck::Refused
        );
    }
}
