use async_trait::async_trait;
use serde::{Deserialize, Deserializer, Serialize};
use sqlx::PgPool;
use wyrd_spec::ids::DataTenantId;

use crate::error::BifrostError;

/// A notification emitted after a fresh Bifrost commit is finalized.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum CommitEvent {
    SpanCommitted {
        table_uid: [u8; 16],
        tenant: DataTenantId,
        batch_id: [u8; 16],
    },
}

#[derive(Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
enum WireCommitEvent {
    SpanCommitted {
        table_uid: [u8; 16],
        tenant: uuid::Uuid,
        batch_id: [u8; 16],
    },
}

impl<'de> Deserialize<'de> for CommitEvent {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        match WireCommitEvent::deserialize(deserializer)? {
            WireCommitEvent::SpanCommitted {
                table_uid,
                tenant,
                batch_id,
            } => Ok(Self::SpanCommitted {
                table_uid,
                tenant: DataTenantId::new(tenant).map_err(serde::de::Error::custom)?,
                batch_id,
            }),
        }
    }
}

/// Receives notifications for fresh, finalized Bifrost commits.
#[async_trait]
pub trait CommitNotifier: Send + Sync + 'static {
    async fn notify(&self, event: CommitEvent) -> Result<(), BifrostError>;
}

/// Publishes commit events through `PostgreSQL` `NOTIFY`.
#[derive(Clone)]
pub struct PostgresCommitNotifier {
    pub pool: PgPool,
}

#[async_trait]
impl CommitNotifier for PostgresCommitNotifier {
    async fn notify(&self, event: CommitEvent) -> Result<(), BifrostError> {
        let payload = serde_json::to_string(&event)
            .map_err(|error| BifrostError::Internal(format!("serialize commit event: {error}")))?;
        sqlx::query("SELECT pg_notify('vala_commits', $1)")
            .bind(payload)
            .execute(&self.pool)
            .await
            .map_err(|error| BifrostError::Internal(format!("publish commit event: {error}")))?;
        Ok(())
    }
}

/// A notifier that intentionally discards commit events.
#[derive(Debug, Default, Clone, Copy)]
pub struct NoOpCommitNotifier;

#[async_trait]
impl CommitNotifier for NoOpCommitNotifier {
    async fn notify(&self, _event: CommitEvent) -> Result<(), BifrostError> {
        Ok(())
    }
}
