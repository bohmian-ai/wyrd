//! Verified recovery contacts: a freshly signed-in User proves an address by
//! email and it replaces the User's verified contact, which can then receive
//! that User's password-reset codes.
//!
//! Nothing about the pending address is stored before proof: the request
//! seals the User, tenant, address, proof digest and the contact revision it
//! was begun against into the browser's flow cookie. Verification replaces
//! the contact only while that revision is still current, so a proof is used
//! once and a flow begun before a replacement verifies nothing.
//!
//! A reset request names a workspace and username and is answered before any
//! account is looked up: the lookup, the code and its message run afterwards
//! as bounded process work, so every request costs and answers the same. A
//! lost send is not retried; the person asks again.

use secrecy::SecretString;
use serde_json::json;
use tokio::sync::Semaphore;
use url::Url;
use wyrd_spec::auth::{SecretBearer, Sha256Hex};
use wyrd_spec::error::WyrdError;
use wyrd_spec::ids::TenantSlug;
use wyrd_sql::queries::auth::{
    local_credential_by_username, replace_verified_contact, verified_contact_for_update,
};
use wyrd_sql::queries::platform::signup::{SignupAdmissionScope, admit_signup};

use super::{SIGNUP_WINDOW, SignupEnvelope, SocialSignIn, source_unknown};
use crate::browser_sessions::{BrowserSession, BrowserSessions, random_secret, session_invalid};
use crate::error::store_error;
use crate::mail::{ApplicationMail, EmailAddress};
use crate::passwords::{canonical_username, code_hash, issue_password_reset};

/// How long a contact proof stays usable.
const CONTACT_EMAIL_TTL: std::time::Duration = std::time::Duration::from_mins(15);

/// Reset requests admitted per trusted source in one window.
const RESET_SOURCE_ATTEMPTS: i32 = 30;

/// Reset messages sent per tenant account in one window; later requests are
/// answered the same and send nothing.
const RESET_ACCOUNT_ATTEMPTS: i32 = 3;

/// Reset sends one process runs at once; a request beyond them sends
/// nothing, and its caller asks again.
const RESET_SENDS: usize = 16;

/// The process's reset-send permits.
static RESET_SENDERS: Semaphore = Semaphore::const_new(RESET_SENDS);

impl SocialSignIn {
    /// Mail a proof of `typed` to that address for the User signed in by
    /// `session` to `tenant`, and return the sealed flow cookie the same
    /// browser must present to verify it.
    ///
    /// The session must be a password or provider sign-in from the last five
    /// minutes, and a mail relay must be configured. The message links
    /// `/t/{tenant}/contact/verify` on `origin` with the proof in its
    /// fragment.
    ///
    /// # Errors
    /// Returns [`WyrdError::InvalidToken`] when the session is not
    /// `tenant`'s, [`WyrdError::RecentLoginRequired`] for an older or
    /// recovery session, [`WyrdError::MethodUnavailable`] without a relay,
    /// [`WyrdError::Validation`] for an unparsable address, and
    /// [`WyrdError::AuthVerifyUnavailable`] when the store, key or relay
    /// fails.
    pub async fn request_contact(
        &self,
        sessions: &BrowserSessions,
        mail: &ApplicationMail,
        session: &BrowserSession,
        tenant: &TenantSlug,
        typed: &str,
        origin: &Url,
    ) -> Result<SecretString, WyrdError> {
        let postgres = sessions.connections.postgres();
        let principal = &session.verified.principal;
        let routed = postgres
            .resolve_tenant_slug(tenant)
            .await
            .map_err(store_error)?;
        if routed != Some(principal.tenant_id) {
            return Err(session_invalid());
        }
        if !session.recent || session.anchor.is_none() {
            return Err(WyrdError::RecentLoginRequired {
                message: "verifying a recovery address needs a sign-in from the last five \
                          minutes; sign in again"
                    .to_owned(),
                details: json!({}),
            });
        }
        if mail.settings().await?.relay.is_none() {
            return Err(WyrdError::MethodUnavailable {
                message: "email verification is not available on this deployment".to_owned(),
                details: json!({ "reason": "method_unavailable" }),
            });
        }
        let address = EmailAddress::parse(typed, "email")?;
        let revision = {
            let mut conn = postgres
                .tenant_conn(principal.tenant_id)
                .await
                .map_err(store_error)?;
            let current = verified_contact_for_update(&mut conn, principal.id.as_uuid())
                .await
                .map_err(store_error)?;
            conn.commit().await.map_err(store_error)?;
            current.map_or(0, |contact| contact.revision)
        };
        let proof = random_secret();
        let cookie = self
            .seal_envelope(&SignupEnvelope::ContactEmail {
                proof_hash: Sha256Hex::digest(proof.as_bytes()),
                tenant: principal.tenant_id.as_uuid(),
                user: principal.id.as_uuid(),
                address: address.delivery.to_string(),
                revision,
                expires_at: self.envelope_deadline(CONTACT_EMAIL_TTL).await?,
            })
            .await?;
        let mut link = origin
            .join(&format!("t/{tenant}/contact/verify"))
            .map_err(|_| super::invalid_state("the deployment origin is invalid"))?;
        link.set_fragment(Some(&format!("proof={proof}")));
        mail.send_contact_proof(&address, link.as_str(), &proof)
            .await?;
        Ok(cookie)
    }

    /// Make the address the flow `cookie` was begun for the verified contact
    /// of the User signed in by `session`, once `proof` matches it.
    ///
    /// The flow must belong to the same User and tenant, be live on the
    /// database clock, and have been begun against the contact revision
    /// still current. The same browser repeating a verification that already
    /// took effect, as after a lost response, succeeds without a change.
    /// Replacing an earlier contact tells that address, after the commit;
    /// a refused notice is logged and changes nothing.
    ///
    /// # Errors
    /// Returns [`WyrdError::InvalidToken`] when the session is not
    /// `tenant`'s, [`WyrdError::OneTimeCodeInvalid`] for a missing, foreign
    /// or stale flow or a wrong proof, [`WyrdError::InvalidState`] once
    /// expired, and [`WyrdError::AuthVerifyUnavailable`] when the store or
    /// key fails.
    pub async fn verify_contact(
        &self,
        sessions: &BrowserSessions,
        mail: &ApplicationMail,
        session: &BrowserSession,
        tenant: &TenantSlug,
        cookie: Option<&str>,
        proof: &SecretBearer,
    ) -> Result<(), WyrdError> {
        let refused = || WyrdError::OneTimeCodeInvalid {
            message: "this email link is not valid in this browser; ask for a new one".to_owned(),
            details: json!({}),
        };
        let postgres = sessions.connections.postgres();
        let principal = &session.verified.principal;
        let routed = postgres
            .resolve_tenant_slug(tenant)
            .await
            .map_err(store_error)?;
        if routed != Some(principal.tenant_id) {
            return Err(session_invalid());
        }
        let opened = match cookie {
            Some(cookie) => self.open_envelope(cookie).await?,
            None => None,
        };
        let Some(SignupEnvelope::ContactEmail {
            proof_hash,
            tenant: flow_tenant,
            user,
            address,
            revision,
            expires_at,
        }) = opened
        else {
            return Err(refused());
        };
        if flow_tenant != principal.tenant_id.as_uuid()
            || user != principal.id.as_uuid()
            || code_hash(proof).ok() != Some(proof_hash)
        {
            return Err(refused());
        }
        self.require_live(expires_at).await?;
        let mut conn = postgres
            .tenant_conn(principal.tenant_id)
            .await
            .map_err(store_error)?;
        let current = verified_contact_for_update(&mut conn, user)
            .await
            .map_err(store_error)?;
        let replaced = match current {
            None if revision == 0 => None,
            Some(contact) if contact.revision == revision => Some(contact.address),
            Some(contact) if contact.revision == revision + 1 && contact.address == address => {
                return conn.commit().await.map_err(store_error);
            }
            _ => return Err(refused()),
        };
        if replace_verified_contact(&mut conn, user, &address, revision)
            .await
            .map_err(store_error)?
            .is_none()
        {
            return Err(refused());
        }
        conn.commit().await.map_err(store_error)?;
        if let Some(old) = replaced.filter(|old| *old != address)
            && let Ok(old) = EmailAddress::parse(&old, "email")
            && let Err(error) = mail.send_contact_changed(&old).await
        {
            tracing::warn!(error = %error, "the contact change notice was not sent");
        }
        Ok(())
    }

    /// Count one password-reset request from `source` in its shared
    /// per-source window, before anything else runs.
    ///
    /// # Errors
    /// Returns [`WyrdError::PasswordAttempts`] with `retry_after_seconds`
    /// while the window is exhausted, and [`WyrdError::AuthVerifyUnavailable`]
    /// when the source is unknown or the derivation key or admission state is
    /// unavailable, which refuses rather than admits.
    pub async fn admit_password_reset(
        &self,
        source: Option<std::net::IpAddr>,
    ) -> Result<(), WyrdError> {
        let source = source.ok_or_else(source_unknown)?;
        let key = self
            .pseudonym("reset", crate::admission::source_key(source).as_bytes())
            .await?;
        let refused = admit_signup(
            &self.operator,
            SignupAdmissionScope::Reset,
            &key,
            RESET_SOURCE_ATTEMPTS,
            SIGNUP_WINDOW,
        )
        .await
        .map_err(store_error)?;
        match refused {
            None => Ok(()),
            Some(wait) => Err(WyrdError::PasswordAttempts {
                message: "too many password reset requests; wait before retrying".to_owned(),
                details: json!({ "retry_after_seconds": wait.as_secs() }),
            }),
        }
    }

    /// Start mailing a reset code for `username` at `tenant` and return at
    /// once, before anything about the account is read.
    ///
    /// The send runs afterwards on one of the process's bounded reset
    /// permits; with none free, or when the process stops, the request sends
    /// nothing and is not retried. The caller has admitted the request and
    /// checked that the tenant offers password reset.
    pub fn request_password_reset(
        &self,
        sessions: &BrowserSessions,
        mail: &ApplicationMail,
        tenant: TenantSlug,
        username: String,
        origin: &Url,
    ) {
        let Ok(permit) = RESET_SENDERS.try_acquire() else {
            tracing::warn!("every reset send permit is busy; a reset request sent nothing");
            return;
        };
        let (social, sessions, mail, origin) =
            (self.clone(), sessions.clone(), mail.clone(), origin.clone());
        tokio::spawn(async move {
            let _permit = permit;
            if let Err(error) = social
                .mail_password_reset(&sessions, &mail, &tenant, &username, &origin)
                .await
            {
                tracing::warn!(error = %error, "a password reset was not sent");
            }
        });
    }

    /// Mail a reset code to the verified contact of `username` at `tenant`
    /// when it is an active local password User with one, within the
    /// account's window; otherwise do nothing.
    ///
    /// The code is bound to the User's password revision and contact
    /// revision, replaces the User's earlier mailed codes, and is mailed with
    /// `/t/{tenant}/reset` on `origin` after its transaction commits.
    ///
    /// # Errors
    /// Returns [`WyrdError::AuthVerifyUnavailable`] when the store, key or
    /// relay fails.
    async fn mail_password_reset(
        &self,
        sessions: &BrowserSessions,
        mail: &ApplicationMail,
        tenant: &TenantSlug,
        username: &str,
        origin: &Url,
    ) -> Result<(), WyrdError> {
        let postgres = sessions.connections.postgres();
        let Some(tenant_id) = postgres
            .resolve_tenant_slug(tenant)
            .await
            .map_err(store_error)?
        else {
            return Ok(());
        };
        let Ok(username) = canonical_username(username) else {
            return Ok(());
        };
        let account = self
            .pseudonym(
                "reset_account",
                format!("{}/{username}", tenant_id.as_uuid()).as_bytes(),
            )
            .await?;
        if admit_signup(
            &self.operator,
            SignupAdmissionScope::ResetAccount,
            &account,
            RESET_ACCOUNT_ATTEMPTS,
            SIGNUP_WINDOW,
        )
        .await
        .map_err(store_error)?
        .is_some()
        {
            return Ok(());
        }
        let mut conn = postgres.tenant_conn(tenant_id).await.map_err(store_error)?;
        let Some(user) = local_credential_by_username(&mut conn, &username)
            .await
            .map_err(store_error)?
            .filter(|user| user.status == "active")
        else {
            return Ok(());
        };
        let Some(contact) = verified_contact_for_update(&mut conn, user.id)
            .await
            .map_err(store_error)?
        else {
            return Ok(());
        };
        let Some((_, code)) =
            issue_password_reset(&mut conn, user.id, user.id, Some(contact.revision)).await?
        else {
            return Ok(());
        };
        conn.commit().await.map_err(store_error)?;
        let to = EmailAddress::parse(&contact.address, "email")?;
        let page = origin
            .join(&format!("t/{tenant}/reset"))
            .map_err(|_| super::invalid_state("the deployment origin is invalid"))?;
        mail.send_password_reset(&to, page.as_str(), code.expose())
            .await
    }
}
