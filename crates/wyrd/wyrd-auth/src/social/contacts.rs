//! Verified recovery contacts: a freshly signed-in User proves an address by
//! email and it replaces the User's verified contact.
//!
//! Nothing about the pending address is stored before proof: the request
//! seals the User, tenant, address, proof digest and the contact revision it
//! was begun against into the browser's flow cookie. Verification replaces
//! the contact only while that revision is still current, so a proof is used
//! once and a flow begun before a replacement verifies nothing.

use secrecy::SecretString;
use serde_json::json;
use url::Url;
use wyrd_spec::auth::{ContactRequest, SecretBearer, Sha256Hex};
use wyrd_spec::error::WyrdError;
use wyrd_spec::ids::TenantSlug;
use wyrd_sql::queries::auth::{replace_verified_contact, verified_contact_for_update};

use super::{SignupEnvelope, SocialSignIn};
use crate::browser_sessions::{BrowserSession, BrowserSessions, random_secret, session_invalid};
use crate::error::store_error;
use crate::mail::{ApplicationMail, EmailAddress};
use crate::passwords::code_hash;

/// How long a contact proof stays usable.
const CONTACT_EMAIL_TTL: std::time::Duration = std::time::Duration::from_mins(15);

impl SocialSignIn {
    /// Mail a proof of `request.email` to that address for the User signed
    /// in by `session` to `request.tenant`, asking from `source`, and
    /// return the sealed flow cookie the same browser must present to verify
    /// it.
    ///
    /// The session must be a password or provider sign-in from the last five
    /// minutes, and a mail relay must be configured. The message links
    /// `/t/{tenant}/contact/verify` on `origin` with the proof in its
    /// fragment. The request is admitted in its source's, its account's and
    /// its address's email windows first; past the address window nothing is
    /// mailed, though the answer is the same.
    ///
    /// # Errors
    /// Returns [`WyrdError::InvalidToken`] when the session is not
    /// `tenant`'s, [`WyrdError::RecentLoginRequired`] for an older or
    /// recovery session, [`WyrdError::MethodUnavailable`] without a relay,
    /// [`WyrdError::Validation`] for an unparsable address,
    /// [`WyrdError::EmailAttempts`] past the source or account window, and
    /// [`WyrdError::AuthVerifyUnavailable`] when the store, key or relay
    /// fails.
    pub async fn request_contact(
        &self,
        sessions: &BrowserSessions,
        mail: &ApplicationMail,
        session: &BrowserSession,
        source: Option<std::net::IpAddr>,
        request: &ContactRequest,
        origin: &Url,
    ) -> Result<SecretString, WyrdError> {
        let tenant = &request.tenant;
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
        let address = EmailAddress::parse(&request.email, "email")?;
        let send = self
            .admit_email(
                "contact",
                source,
                &address,
                Some((principal.tenant_id, principal.id.as_uuid())),
            )
            .await?;
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
        if send {
            mail.send_contact_proof(&address, link.as_str(), &proof)
                .await?;
        }
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
}
