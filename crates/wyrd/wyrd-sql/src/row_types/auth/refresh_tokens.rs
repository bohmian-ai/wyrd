//! Row mirrors for `wyrd.auth_refresh_tokens`.

use chrono::{DateTime, Utc};
use sqlx::types::Uuid;
use wyrd_spec::auth::OAuthClientId;

use crate::row_types::auth::{HumanConnectionBinding, HumanSessionBinding};

/// SQL row for `wyrd.auth_refresh_tokens`.
#[derive(Debug, Clone, sqlx::FromRow)]
pub struct RefreshTokenRow {
    /// Refresh token identifier.
    pub id: Uuid,
    /// Tenant isolation UUID as stored by Postgres.
    pub data_tenant_id: Uuid,
    /// Principal kind: `user`, `service`, or `agent`.
    pub principal_kind: String,
    /// Stable principal identifier.
    pub principal_id: Uuid,
    /// SHA-256 hex digest of the raw JWT string.
    pub token_hash: String,
    /// Issuance timestamp.
    pub issued_at: DateTime<Utc>,
    /// Expiration timestamp.
    pub expires_at: DateTime<Utc>,
    /// Previous refresh token id when this row was created by rotation.
    pub rotated_from: Option<Uuid>,
    /// Revocation timestamp; `None` when the token is still active.
    pub revoked_at: Option<DateTime<Utc>>,
    /// Human-readable revocation reason, for example `"rotated"` or `"reuse_detected"`.
    pub revoked_reason: Option<String>,
    /// Tenant human connection a `user` session was established through;
    /// `None` for machine rows. Rotation copies it to every successor, and
    /// renewal is refused unless this exact connection is still Active.
    pub human_connection_id: Option<Uuid>,
    /// Revision of [`Self::human_connection_id`] at login; set exactly when
    /// the id is.
    pub human_connection_revision: Option<i64>,
    /// OAuth client a `user` session was issued to (`wyrd-ui` or
    /// `wyrd-cli`); set exactly when [`Self::human_connection_id`] is.
    pub client_id: Option<String>,
}

impl RefreshTokenRow {
    /// The family's login connection and client, or `None` for a row that
    /// carries no complete human binding (a machine row, a legacy unbound
    /// family, or an unknown client).
    #[must_use]
    pub fn human_session(&self) -> Option<HumanSessionBinding> {
        Some(HumanSessionBinding {
            connection: HumanConnectionBinding {
                connection_id: self.human_connection_id?,
                connection_revision: self.human_connection_revision?,
            },
            client: OAuthClientId::parse(self.client_id.as_deref()?)?,
        })
    }
}
