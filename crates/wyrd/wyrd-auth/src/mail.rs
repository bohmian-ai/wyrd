//! Application mail through the deployment's one operator-configured SMTP
//! relay.
//!
//! A platform administrator stores the relay on the platform login settings
//! record, its password sealed through `[kms]`, and can send a test
//! message. Tenant invitations then use it: the invitation handlers commit
//! first and call [`ApplicationMail::send_invitations`] afterwards, so no
//! tenant transaction waits on SMTP. Each message is sent separately and
//! reported as relay acceptance or failure, never as inbox delivery. Nothing
//! here queues or retries; message content exists only while it is sent.

use std::sync::Arc;
use std::time::Duration;

use chrono::{DateTime, Utc};
use futures_util::future::join_all;
use lettre::message::header::ContentType;
use lettre::message::{Mailbox, Message};
use lettre::transport::smtp::authentication::Credentials;
use lettre::{Address, AsyncSmtpTransport, AsyncTransport, Tokio1Executor};
use serde_json::json;
use wyrd_kms::{SecretContext, SecretPurpose, SecretScope, WyrdKms};
use wyrd_spec::auth::{
    ConfigureMailRelayRequest, InvitationDelivery, MailRelayView, MailSettings, MailTestResponse,
    SmtpTls,
};
use wyrd_spec::error::WyrdError;
use wyrd_sql::OperatorPool;
use wyrd_sql::queries::platform::social::{MailRelayRow, mail_relay, put_mail_relay};

use crate::error::store_error;

/// The envelope context of the relay password bound to its `username`, as
/// stored-secret maintenance rebuilds it.
const fn relay_context<'a>(username: &'a [&'a str]) -> SecretContext<'a> {
    SecretContext {
        scope: SecretScope::Platform,
        purpose: SecretPurpose::ApplicationMail,
        owner: "smtp_relay",
        authority: username,
        version: None,
    }
}

/// How long one SMTP read or write may wait before the send fails.
const SMTP_TIMEOUT: Duration = Duration::from_secs(10);

/// One invitation to email: its recipient, link and expiry.
#[derive(Debug, Clone, Copy)]
pub struct InvitationMail<'a> {
    /// Recipient address.
    pub recipient: &'a str,
    /// The single-use invitation link, shown once.
    pub link: &'a str,
    /// When the link expires.
    pub expires_at: DateTime<Utc>,
}

/// The deployment's application mail: relay management, test sends and
/// invitation delivery.
#[derive(Clone)]
pub struct ApplicationMail {
    /// Operator pool; `platform.*` admits only operator sessions.
    operator: OperatorPool,
    /// Stored-secret facade for the relay password; `None` refuses one.
    kms: Option<Arc<WyrdKms>>,
}

impl std::fmt::Debug for ApplicationMail {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ApplicationMail").finish_non_exhaustive()
    }
}

/// An opened relay, ready to send for one request.
struct Relay {
    /// Pooled SMTP connection to the relay.
    transport: AsyncSmtpTransport<Tokio1Executor>,
    /// Envelope and header sender.
    sender: Mailbox,
}

impl ApplicationMail {
    /// Build the owner over the operator pool and stored-secret facade.
    #[must_use]
    pub fn new(operator: OperatorPool, kms: Option<Arc<WyrdKms>>) -> Self {
        Self { operator, kms }
    }

    /// The configured relay as management reads it, without its password.
    ///
    /// # Errors
    /// Returns [`WyrdError::AuthVerifyUnavailable`] when the read fails.
    pub async fn settings(&self) -> Result<MailSettings, WyrdError> {
        let row = mail_relay(&self.operator).await.map_err(store_error)?;
        Ok(MailSettings {
            relay: row.as_ref().map(view),
        })
    }

    /// Validate, seal and store `request` as the relay, replacing any
    /// earlier relay, for a caller platform authorization already allowed.
    ///
    /// The password is sealed before the audited operator transaction opens,
    /// so no transaction waits on the key service; a refused seal leaves the
    /// committed relay unchanged.
    ///
    /// # Errors
    /// Returns [`WyrdError::Validation`] for an empty host, a zero port, an
    /// unparsable sender, a password without a username, a password on a
    /// deployment without `[kms]` or a seal the key service refuses, and
    /// [`WyrdError::AuthVerifyUnavailable`] when the write fails.
    pub async fn configure(
        &self,
        request: &ConfigureMailRelayRequest,
    ) -> Result<MailSettings, WyrdError> {
        let host = request.host.trim();
        if host.is_empty() || host.contains(char::is_whitespace) {
            return Err(validation("host must be a host name or address", "host"));
        }
        if request.port == 0 {
            return Err(validation("port must be between 1 and 65535", "port"));
        }
        let sender = request.sender.trim();
        if sender.parse::<Mailbox>().is_err() {
            return Err(validation(
                "sender must be an email address, such as invites@example.com",
                "sender",
            ));
        }
        let username = request
            .username
            .as_deref()
            .map(str::trim)
            .filter(|name| !name.is_empty());
        let password_enc = match (&request.password, username) {
            (None, _) => None,
            (Some(_), None) => {
                return Err(validation("a password needs a username", "username"));
            }
            (Some(password), Some(username)) => {
                let kms = self.kms.as_deref().ok_or_else(|| {
                    validation(
                        "this deployment has no [kms], so a relay password cannot be stored",
                        "password",
                    )
                })?;
                Some(
                    kms.seal(&relay_context(&[username]), password.expose().as_bytes())
                        .await
                        .map_err(|error| {
                            tracing::warn!(%error, "relay password could not be sealed");
                            validation("the password could not be sealed", "password")
                        })?,
                )
            }
        };
        let row = MailRelayRow {
            host: host.to_owned(),
            port: i32::from(request.port),
            tls: request.tls.as_str().to_owned(),
            username: username.map(str::to_owned),
            password_enc,
            sender: sender.to_owned(),
        };
        let mut conn = self
            .operator
            .begin_platform_audited()
            .await
            .map_err(store_error)?;
        put_mail_relay(&mut conn, &row).await.map_err(store_error)?;
        conn.commit().await.map_err(store_error)?;
        Ok(MailSettings {
            relay: Some(view(&row)),
        })
    }

    /// Send one test message to `recipient` through the configured relay.
    ///
    /// # Errors
    /// Returns [`WyrdError::Validation`] for an unparsable recipient,
    /// [`WyrdError::Conflict`] when no relay is configured, and
    /// [`WyrdError::AuthVerifyUnavailable`] when the read fails.
    pub async fn test(&self, recipient: &str) -> Result<MailTestResponse, WyrdError> {
        let to = recipient
            .trim()
            .parse::<Address>()
            .map_err(|_| validation("recipient must be an email address", "recipient"))?;
        let relay = self.relay().await?.ok_or_else(|| WyrdError::Conflict {
            message: "no mail relay is configured; configure one first".to_owned(),
            details: json!({}),
        })?;
        let message = relay.message(
            to,
            "Wyrd mail relay test",
            "This is a test message from Wyrd. Your mail relay accepted it.\n".to_owned(),
        );
        Ok(match relay.send(message).await {
            Ok(()) => MailTestResponse {
                accepted: true,
                error: None,
            },
            Err(error) => MailTestResponse {
                accepted: false,
                error: Some(error),
            },
        })
    }

    /// Email each invitation separately for `workspace`, returning one
    /// outcome per invitation in order: `not_emailed` with no relay,
    /// otherwise relay acceptance or failure. Call only after the issuing
    /// transaction commits. Never fails: an unreadable or unusable relay
    /// reports every send as failed.
    pub async fn send_invitations(
        &self,
        workspace: &str,
        invitations: &[InvitationMail<'_>],
    ) -> Vec<InvitationDelivery> {
        let relay = match self.relay().await {
            Ok(Some(relay)) => relay,
            Ok(None) => return vec![InvitationDelivery::NotEmailed; invitations.len()],
            Err(error) => {
                tracing::error!(error = %error, "the mail relay could not be opened");
                return vec![InvitationDelivery::Failed; invitations.len()];
            }
        };
        let subject = format!("You are invited to {workspace} on Wyrd");
        // ponytail: one request's sends share the relay's connection pool;
        // a relay stalling every connection costs SMTP_TIMEOUT per pool wave.
        join_all(invitations.iter().map(|invitation| {
            let relay = &relay;
            let subject = subject.as_str();
            async move {
                let Ok(to) = invitation.recipient.parse::<Address>() else {
                    return InvitationDelivery::Failed;
                };
                let body = format!(
                    "You have been invited to the {workspace} workspace on Wyrd.\n\n\
                     Open this link to create your account:\n\n{}\n\n\
                     The link works once and expires at {} UTC. If you did not expect \
                     this invitation, ignore this message.\n",
                    invitation.link,
                    invitation.expires_at.format("%Y-%m-%d %H:%M"),
                );
                match relay.send(relay.message(to, subject, body)).await {
                    Ok(()) => InvitationDelivery::Accepted,
                    Err(error) => {
                        tracing::warn!(error = %error, "the mail relay refused an invitation");
                        InvitationDelivery::Failed
                    }
                }
            }
        }))
        .await
    }

    /// Email `to` the link that proves its mailbox for a workspace signup.
    /// The link carries its proof in the fragment, so it never reaches a
    /// server log; the proof is also shown to paste.
    ///
    /// # Errors
    /// Returns [`WyrdError::AuthVerifyUnavailable`] when no relay is
    /// configured, it cannot be opened, or it refuses the message: one
    /// deployment-wide refusal that says nothing about the address.
    pub async fn send_signup_proof(
        &self,
        to: &EmailAddress,
        link: &str,
        proof: &str,
    ) -> Result<(), WyrdError> {
        let body = format!(
            "Open this link in the browser where you started creating your Wyrd \
             workspace:\n\n{link}\n\nOr paste this code there:\n\n{proof}\n\n\
             It works once and expires in fifteen minutes. If you did not ask to \
             create a workspace, ignore this message.\n"
        );
        self.send_notice(to, "Confirm your email for Wyrd", body)
            .await
    }

    /// Mail `to` the link and proof that verify it as the sender's recovery
    /// address.
    ///
    /// # Errors
    /// As [`Self::send_signup_proof`].
    pub async fn send_contact_proof(
        &self,
        to: &EmailAddress,
        link: &str,
        proof: &str,
    ) -> Result<(), WyrdError> {
        let body = format!(
            "Open this link in the browser where you asked to verify this address \
             for Wyrd account recovery:\n\n{link}\n\nOr paste this code there:\n\n\
             {proof}\n\nIt works once and expires in fifteen minutes. If you did not \
             ask for this, ignore this message.\n"
        );
        self.send_notice(to, "Verify your recovery email for Wyrd", body)
            .await
    }

    /// Tell `to`, a replaced recovery address, that it no longer recovers
    /// its account. The message names neither the new address nor the
    /// workspace.
    ///
    /// # Errors
    /// As [`Self::send_signup_proof`].
    pub async fn send_contact_changed(&self, to: &EmailAddress) -> Result<(), WyrdError> {
        let body = "The recovery email of your Wyrd account was changed, so this \
                    address no longer recovers it. If you did not make this change, \
                    contact your workspace administrator.\n"
            .to_owned();
        self.send_notice(to, "Your Wyrd recovery email changed", body)
            .await
    }

    /// Mail `to`, a User's verified contact, the code that resets the
    /// User's password and the workspace `reset_page` to paste it into. The
    /// code is never part of a URL.
    ///
    /// # Errors
    /// As [`Self::send_signup_proof`].
    pub async fn send_password_reset(
        &self,
        to: &EmailAddress,
        reset_page: &str,
        code: &str,
    ) -> Result<(), WyrdError> {
        let body = format!(
            "Someone asked to reset the password of your Wyrd account. Open this \
             page:\n\n{reset_page}\n\nand paste this code there:\n\n{code}\n\nIt works \
             once and expires in fifteen minutes. If you did not ask for this, ignore \
             this message; your password stays as it is.\n"
        );
        self.send_notice(to, "Reset your Wyrd password", body).await
    }

    /// Tell `to`, a User's verified contact, that the User's password was
    /// reset. The message carries no password or code.
    ///
    /// # Errors
    /// As [`Self::send_signup_proof`].
    pub async fn send_password_reset_done(&self, to: &EmailAddress) -> Result<(), WyrdError> {
        let body = "The password of your Wyrd account was reset, and every browser and \
                    CLI sign-in with the old password has ended. Sign in again with the \
                    new password. If you did not do this, contact your workspace \
                    administrator.\n"
            .to_owned();
        self.send_notice(to, "Your Wyrd password was reset", body)
            .await
    }

    /// Send one message through the configured relay.
    ///
    /// # Errors
    /// Returns [`WyrdError::AuthVerifyUnavailable`] when no relay is
    /// configured, it cannot be opened, or it refuses the message.
    async fn send_notice(
        &self,
        to: &EmailAddress,
        subject: &str,
        body: String,
    ) -> Result<(), WyrdError> {
        let unavailable = || WyrdError::AuthVerifyUnavailable {
            message: "email could not be sent; try again later".to_owned(),
            details: json!({ "retry_after_seconds": 60 }),
        };
        let relay = match self.relay().await {
            Ok(Some(relay)) => relay,
            Ok(None) => return Err(unavailable()),
            Err(error) => {
                tracing::error!(error = %error, "the mail relay could not be opened");
                return Err(unavailable());
            }
        };
        relay
            .send(relay.message(to.delivery.clone(), subject, body))
            .await
            .map_err(|error| {
                tracing::warn!(error = %error, "the mail relay refused a message");
                unavailable()
            })
    }

    /// Open the configured relay; `None` when none is configured.
    ///
    /// # Errors
    /// Returns [`WyrdError::AuthVerifyUnavailable`] when the read fails and
    /// [`WyrdError::Internal`] when the stored relay cannot be opened.
    async fn relay(&self) -> Result<Option<Relay>, WyrdError> {
        let Some(row) = mail_relay(&self.operator).await.map_err(store_error)? else {
            return Ok(None);
        };
        let unusable = |reason: &str| WyrdError::Internal {
            message: format!("the stored mail relay is unusable: {reason}"),
            details: json!({}),
        };
        let sender = row
            .sender
            .parse::<Mailbox>()
            .map_err(|_| unusable("sender"))?;
        let port = u16::try_from(row.port).map_err(|_| unusable("port"))?;
        let builder = match SmtpTls::parse(&row.tls).ok_or_else(|| unusable("tls"))? {
            SmtpTls::None => AsyncSmtpTransport::<Tokio1Executor>::builder_dangerous(&row.host),
            SmtpTls::Starttls => AsyncSmtpTransport::<Tokio1Executor>::starttls_relay(&row.host)
                .map_err(|_| unusable("tls"))?,
            SmtpTls::Tls => AsyncSmtpTransport::<Tokio1Executor>::relay(&row.host)
                .map_err(|_| unusable("tls"))?,
        };
        let mut builder = builder.port(port).timeout(Some(SMTP_TIMEOUT));
        if let Some(username) = row.username {
            let password = match row.password_enc {
                Some(sealed) => {
                    let kms = self.kms.as_deref().ok_or_else(|| unusable("password"))?;
                    let opened = kms
                        .open(&relay_context(&[username.as_str()]), &sealed)
                        .await
                        .map_err(|error| {
                            tracing::warn!(%error, "relay password could not be opened");
                            unusable("password")
                        })?;
                    String::from_utf8(opened.to_vec()).map_err(|_| unusable("password"))?
                }
                None => String::new(),
            };
            builder = builder.credentials(Credentials::new(username, password));
        }
        Ok(Some(Relay {
            transport: builder.build(),
            sender,
        }))
    }
}

impl Relay {
    /// A plain-text message from the sender to `to`.
    fn message(&self, to: Address, subject: &str, body: String) -> Message {
        Message::builder()
            .from(self.sender.clone())
            .to(Mailbox::new(None, to))
            .subject(subject)
            .header(ContentType::TEXT_PLAIN)
            .body(body)
            .expect("invariant: parsed addresses and a text body always build a message")
    }

    /// Send `message`; the relay's acceptance or its error text.
    async fn send(&self, message: Message) -> Result<(), String> {
        self.transport
            .send(message)
            .await
            .map(|_| ())
            .map_err(|error| error.to_string())
    }
}

/// Project a stored relay without its password.
fn view(row: &MailRelayRow) -> MailRelayView {
    MailRelayView {
        host: row.host.clone(),
        port: u16::try_from(row.port).unwrap_or_default(),
        tls: SmtpTls::parse(&row.tls).unwrap_or(SmtpTls::Tls),
        username: row.username.clone(),
        password_set: row.password_enc.is_some(),
        sender: row.sender.clone(),
    }
}

/// A field validation refusal.
/// An email address as typed and as compared.
#[derive(Debug, Clone)]
pub struct EmailAddress {
    /// The trimmed address as typed, which mail is delivered to.
    pub delivery: Address,
    /// The comparison form: the local part unchanged, the domain lowercased.
    /// Dots, plus tags and provider aliases are never collapsed, so
    /// differently written addresses stay distinct rather than merge.
    pub canonical: String,
}

impl EmailAddress {
    /// Parse a typed address.
    ///
    /// # Errors
    /// Returns [`WyrdError::Validation`] naming `field` when the trimmed
    /// value is not an email address.
    pub fn parse(typed: &str, field: &str) -> Result<Self, WyrdError> {
        let delivery = typed
            .trim()
            .parse::<Address>()
            .map_err(|_| validation(&format!("{field} must be an email address"), field))?;
        let canonical = format!(
            "{}@{}",
            delivery.user(),
            delivery.domain().to_ascii_lowercase()
        );
        Ok(Self {
            delivery,
            canonical,
        })
    }
}

fn validation(message: &str, field: &str) -> WyrdError {
    WyrdError::Validation {
        message: message.to_owned(),
        details: json!({ "field": field }),
    }
}

#[cfg(test)]
mod tests {
    use super::EmailAddress;

    /// Trimming and the domain's case are the only normalization.
    #[test]
    fn addresses_compare_by_local_part_and_lowercase_domain() {
        let address = EmailAddress::parse("  Ada.Lovelace+wyrd@Example.TEST ", "email")
            .expect("valid address");
        assert_eq!(address.canonical, "Ada.Lovelace+wyrd@example.test");
        assert_eq!(
            address.delivery.to_string(),
            "Ada.Lovelace+wyrd@Example.TEST"
        );
        assert!(EmailAddress::parse("not an address", "email").is_err());
        assert!(EmailAddress::parse("", "email").is_err());
    }
}
