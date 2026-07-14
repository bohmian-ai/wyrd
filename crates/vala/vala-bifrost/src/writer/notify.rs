use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use sqlx::PgPool;
use wyrd_spec::ids::DataTenantId;

/// A notification emitted after a fresh Bifrost commit is finalized.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum CommitEvent {
    SpanCommitted {
        table_uid: [u8; 16],
        tenant: DataTenantId,
        batch_id: [u8; 16],
    },
}

/// Receives notifications for fresh, finalized Bifrost commits.
#[async_trait]
pub trait CommitNotifier: Send + Sync + 'static {
    async fn notify(&self, event: CommitEvent);
}

/// Publishes commit events through `PostgreSQL` `NOTIFY`.
#[derive(Clone)]
pub struct PostgresCommitNotifier {
    pub pool: PgPool,
}

#[async_trait]
impl CommitNotifier for PostgresCommitNotifier {
    async fn notify(&self, event: CommitEvent) {
        let payload = match serde_json::to_string(&event) {
            Ok(payload) => payload,
            Err(error) => {
                tracing::warn!(error = %error, "serialize commit notification failed");
                return;
            }
        };
        if let Err(error) = sqlx::query("SELECT pg_notify('vala_commits', $1)")
            .bind(payload)
            .execute(&self.pool)
            .await
        {
            tracing::warn!(error = %error, "publish commit notification failed");
        }
    }
}

/// A notifier that intentionally discards commit events.
#[derive(Debug, Default, Clone, Copy)]
pub struct NoOpCommitNotifier;

#[async_trait]
impl CommitNotifier for NoOpCommitNotifier {
    async fn notify(&self, _event: CommitEvent) {}
}
