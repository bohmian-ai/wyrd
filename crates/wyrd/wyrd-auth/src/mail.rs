//! Application mail through the deployment's one operator-configured SMTP
//! relay.
//!
//! A platform administrator stores the relay on the platform login settings
//! record, its password sealed by the deployment keyring, and can send a test
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
use wyrd_crypt::SealingKeyring;
use wyrd_spec::auth::{
    ConfigureMailRelayRequest, InvitationDelivery, MailRelayView, MailSettings, MailTestResponse,
    SmtpTls,
};
use wyrd_spec::error::WyrdError;
use wyrd_sql::queries::platform::social::{MailRelayRow, mail_relay, put_mail_relay};
use wyrd_sql::{OperatorPool, TenantConn};

use crate::error::store_error;

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
    /// Sealing keyring for the relay password; `None` refuses a password.
    keyring: Option<Arc<SealingKeyring>>,
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
    /// Build the owner over the operator pool and keyring.
    #[must_use]
    pub fn new(operator: OperatorPool, keyring: Option<Arc<SealingKeyring>>) -> Self {
        Self { operator, keyring }
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

    /// Validate, seal and store `request` as the relay on the audited
    /// operator `conn`, replacing any earlier relay.
    ///
    /// # Errors
    /// Returns [`WyrdError::Validation`] for an empty host, a zero port, an
    /// unparsable sender, a password without a username or a password on a
    /// deployment without a sealing keyring, and
    /// [`WyrdError::AuthVerifyUnavailable`] when the write fails.
    pub async fn configure(
        &self,
        conn: &mut TenantConn<'_>,
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
            (Some(password), Some(_)) => {
                let keyring = self.keyring.as_ref().ok_or_else(|| {
                    validation(
                        "this deployment has no sealing key, so a relay password cannot be stored",
                        "password",
                    )
                })?;
                Some(
                    keyring
                        .seal(password.expose().as_bytes())
                        .map_err(|_| validation("the password could not be sealed", "password"))?,
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
        put_mail_relay(conn, &row).await.map_err(store_error)?;
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
                Some(sealed) => self
                    .keyring
                    .as_ref()
                    .and_then(|keyring| keyring.open(&sealed).ok())
                    .and_then(|bytes| String::from_utf8(bytes).ok())
                    .ok_or_else(|| unusable("password"))?,
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
fn validation(message: &str, field: &str) -> WyrdError {
    WyrdError::Validation {
        message: message.to_owned(),
        details: json!({ "field": field }),
    }
}
