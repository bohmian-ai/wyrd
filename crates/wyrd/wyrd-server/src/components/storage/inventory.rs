//! The server's one owner of `bifrost.storage.objects`, the completed-object
//! inventory.
//!
//! Completion, Card reconciliation, and envelope writes record each object
//! through [`ObjectInventory::record`], which stages the row on the process
//! Scribe outbox and waits, within half the request timeout, for that row's
//! own acknowledgement; no other producer waits on it. Card deletion records
//! one `deleted` row the same way. Readers resolve one Card's objects through
//! [`ObjectInventory::card_objects`] under a SYSTEM principal granted only
//! this table, after the caller's Card authorization; an absent row is
//! retryable readiness, never proof of deletion.

use std::collections::BTreeMap;
use std::sync::{Arc, LazyLock, Mutex};
use std::time::Duration;

use arrow::array::{Array as _, AsArray as _, RecordBatch};
use arrow::datatypes::{DataType, Field, Int64Type, Schema, SchemaRef};
use base64::Engine as _;
use chrono::Utc;
use tokio::sync::oneshot;
use tokio_util::sync::CancellationToken;
use vala_bifrost_redux::catalog::TableRef;
use vala_bifrost_redux::namespaces::BifrostNamespace;
use vala_bifrost_redux::tables::DomainTable as _;
use vala_bifrost_redux::tables::staged::{Cell, StagedRow};
pub use vala_bifrost_redux::tables::storage::{ObjectOrigin, ObjectState};
use vala_bifrost_redux::tables::storage::ObjectsTable;
use wyrd_runtime::principal::PrincipalId;
use wyrd_spec::DataTenantId;
use wyrd_spec::auth::Attribution;
use wyrd_spec::error::WyrdError;
use wyrd_spec::ids::CardUid;
use wyrd_spec::registry::{RelativeArtifactPath, StoredArtifactEntry};
use wyrd_spec::vala::BifrostError;
use wyrd_spec::vala::api::BifrostQueryRequest;
use wyrd_spec::vala::managed_columns::{RUN_ID, SUBJECT_CARD_UID, WORKLOAD_CARD_UID};
use wyrd_sql::queries::auth::service_accounts::system_principal_id;
use wyrd_storage::tenant_path;

use crate::query::scheduled::ScheduledQueryCaller;
use crate::scribe_outbox::ScribeWrite;
use crate::state::AppState;
use crate::verification::authority::SystemReadAuthority;
use crate::verification::drift::ObservationWindow;
use crate::verification::inputs::TABLE_NOT_FOUND;

/// Fully qualified name of the inventory table.
const OBJECTS: &str = "bifrost.storage.objects";

/// The one schema every staged inventory row shares: the table's fields,
/// then the workload, subject, and run columns Scribe attributes rows by.
static SCHEMA: LazyLock<SchemaRef> = LazyLock::new(|| {
    let mut fields = ObjectsTable::arrow_fields();
    fields.extend(
        [WORKLOAD_CARD_UID, SUBJECT_CARD_UID, RUN_ID]
            .map(|name| Field::new(name, DataType::Utf8, true)),
    );
    Arc::new(Schema::new(fields))
});

/// One object fact to record.
#[derive(Debug, Clone)]
pub struct ObjectFact {
    /// Tenant-qualified stored object path, the object's logical identity.
    pub storage_path: String,
    /// Upload session that completed the object; `None` for an envelope.
    pub upload_id: Option<String>,
    /// Configured storage backend spelling.
    pub backend: String,
    /// Declared SHA-256 as storage's base64; `None` for an envelope or a
    /// deletion.
    pub sha256: Option<String>,
    /// Stored byte length.
    pub size_bytes: i64,
    /// Content type, when known.
    pub content_type: Option<String>,
    /// Server-side-encryption marker, when known.
    pub sse_marker: Option<String>,
    /// Where the object came from.
    pub origin: ObjectOrigin,
    /// Owning Card, which Scribe also stamps as the row's subject.
    pub card_uid: Option<CardUid>,
    /// Card-relative path; `None` for an envelope or deletion.
    pub relative_path: Option<String>,
    /// Completion or deletion.
    pub state: ObjectState,
}

impl ObjectFact {
    /// The deletion of every object of `card_uid`, recorded at its envelope
    /// path so it lanes and dedupes like the Card's other rows.
    pub fn deleted(card_uid: &CardUid, storage_path: String, backend: String) -> Self {
        Self {
            storage_path,
            upload_id: None,
            backend,
            sha256: None,
            size_bytes: 0,
            content_type: None,
            sse_marker: None,
            origin: ObjectOrigin::CardEnvelope,
            card_uid: Some(card_uid.clone()),
            relative_path: None,
            state: ObjectState::Deleted,
        }
    }

    /// This fact as its staged row, stamped `recorded_at` now.
    ///
    /// # Errors
    /// Returns an internal error when the digest is not base64 or a value
    /// does not fit its column.
    pub(crate) fn row(&self) -> Result<StagedRow, WyrdError> {
        let sha256 = self
            .sha256
            .as_deref()
            .map(|digest| {
                base64::engine::general_purpose::STANDARD
                    .decode(digest)
                    .map(hex::encode)
                    .map_err(|error| WyrdError::internal(format!("object digest: {error}")))
            })
            .transpose()?;
        let card_uid = self.card_uid.as_ref().map(ToString::to_string);
        let cells = SCHEMA
            .fields()
            .iter()
            .map(|field| match field.name().as_str() {
                "storage_path" => Cell::text(Some(self.storage_path.as_str())),
                "upload_id" => Cell::text(self.upload_id.as_deref()),
                "backend" => Cell::text(Some(self.backend.as_str())),
                "sha256" => Cell::text(sha256.as_deref()),
                "size_bytes" => Cell::Int(self.size_bytes),
                "content_type" => Cell::text(self.content_type.as_deref()),
                "sse_marker" => Cell::text(self.sse_marker.as_deref()),
                "origin" => Cell::text(Some(self.origin.as_str())),
                "card_uid" | SUBJECT_CARD_UID => Cell::text(card_uid.as_deref()),
                "relative_path" => Cell::text(self.relative_path.as_deref()),
                "state" => Cell::text(Some(self.state.as_str())),
                "recorded_at" => Cell::Micros(Some(Utc::now().timestamp_micros())),
                // Shared dimensions and the workload and run columns.
                _ => Cell::text(None::<&str>),
            })
            .collect();
        StagedRow::new(Arc::clone(&SCHEMA), cells)
            .map_err(|error| WyrdError::internal(format!("object row: {error}")))
    }
}

/// One Card's recorded objects, as readers serve them.
#[derive(Debug, Default)]
pub struct CardObjects {
    /// Completed files by Card-relative path, one per stored object;
    /// envelopes are excluded, and nothing remains once deleted.
    pub files: BTreeMap<String, StoredArtifactEntry>,
    /// Whether a deletion was recorded, which dominates every completion.
    pub deleted: bool,
}

/// Records and reads `bifrost.storage.objects` for one server.
pub struct ObjectInventory<'s> {
    /// The server whose outbox and query plane the inventory uses.
    state: &'s AppState,
}

impl<'s> ObjectInventory<'s> {
    /// The inventory of `state`.
    pub const fn new(state: &'s AppState) -> Self {
        Self { state }
    }

    /// Longest an inventory write or read waits: half the request timeout,
    /// so the request ends inside its own deadline.
    fn budget(&self) -> Duration {
        self.state.settings.limits.timeout / 2
    }

    /// Record `facts` for `tenant` under `attribution` and wait for each
    /// row's own Scribe acknowledgement.
    ///
    /// # Errors
    /// Returns `WYRD_VALA_503_BIFROST_WRITER_UNAVAILABLE` when any row is
    /// shed, rejected, dropped, or unacknowledged within [`Self::budget`];
    /// the outcome is unknown and the caller retries. Returns an internal
    /// error when a fact cannot be projected.
    #[tracing::instrument(skip_all, fields(%tenant, rows = facts.len()))]
    pub async fn record(
        &self,
        tenant: DataTenantId,
        attribution: Attribution,
        facts: &[ObjectFact],
    ) -> Result<(), WyrdError> {
        let outbox = self.state.bifrost.scribe_outbox();
        let mut acks = Vec::with_capacity(facts.len());
        for fact in facts {
            let (sender, ack) = oneshot::channel();
            outbox.stage(
                tenant,
                ScribeWrite::Object {
                    row: fact.row()?,
                    attribution,
                    acked: Mutex::new(Some(sender)),
                },
            );
            acks.push(ack);
        }
        let acked = tokio::time::timeout(self.budget(), futures_util::future::try_join_all(acks));
        match acked.await {
            Ok(Ok(_)) => Ok(()),
            outcome => {
                tracing::warn!(timed_out = outcome.is_err(), "an object inventory write was not acknowledged");
                Err(BifrostError::WriterUnavailable {
                    table: OBJECTS.to_owned(),
                }
                .into())
            }
        }
    }

    /// Read `card_uid`'s recorded objects in `tenant`, after the caller's
    /// Card authorization.
    ///
    /// The read runs under the tenant's SYSTEM principal granted only this
    /// table. A table the tenant never wrote reads as no rows; callers treat
    /// a missing expected object as retryable readiness.
    ///
    /// # Errors
    /// Returns `WYRD_VALA_503_QUERY_VISIBILITY_UNAVAILABLE` when the SYSTEM
    /// principal or read authority is unavailable, the query's own error
    /// when it fails, and an internal error when a stored row is malformed.
    #[tracing::instrument(skip_all, fields(%tenant, %card_uid))]
    pub async fn card_objects(
        &self,
        tenant: DataTenantId,
        card_uid: &CardUid,
    ) -> Result<CardObjects, WyrdError> {
        let unavailable = |error: &dyn std::fmt::Display| {
            tracing::warn!(%error, "the object inventory cannot be read");
            WyrdError::from(BifrostError::QueryVisibilityUnavailable)
        };
        let system = {
            let mut conn = self.state.registry_tenant_conn(tenant).await?;
            let system = system_principal_id(&mut conn)
                .await
                .map_err(|error| unavailable(&error))?;
            conn.commit().await.map_err(|error| unavailable(&error))?;
            system.map(PrincipalId::new)
        };
        let table = [TableRef::new(BifrostNamespace::Storage, ObjectsTable::NAME)];
        let read = SystemReadAuthority::resolve(self.state, tenant, system, &table)
            .await
            .map_err(|error| unavailable(&error))?;
        let query = ScheduledQueryCaller::new(
            self.state.clone(),
            read.into_context(),
            CancellationToken::new(),
        );
        let mut batches = Vec::new();
        let sql = format!(
            "SELECT storage_path, sha256, size_bytes, content_type, origin, relative_path, state \
             FROM {OBJECTS} WHERE card_uid = {}",
            ObservationWindow::text(card_uid.as_str())
        );
        let run = query.run_with(
            BifrostQueryRequest {
                params: Vec::new(),
                sql,
                deadline_ms: Some(i64::try_from(self.budget().as_millis()).unwrap_or(i64::MAX)),
            },
            |batch| {
                batches.push(batch);
                Ok(())
            },
        );
        match run.await {
            Err(error) if error.code() == TABLE_NOT_FOUND => {}
            Err(error) => return Err(error),
            Ok(_) => {}
        }
        Self::fold(tenant, card_uid, &batches)
    }

    /// Fold `card_uid`'s inventory rows into one object per path, a deletion
    /// dominating.
    ///
    /// # Errors
    /// Returns `WYRD_REGISTRY_400_INVALID_CARD_SPEC` when a row is malformed
    /// or its stored path is not `card_uid`'s file in `tenant`.
    fn fold(
        tenant: DataTenantId,
        card_uid: &CardUid,
        batches: &[RecordBatch],
    ) -> Result<CardObjects, WyrdError> {
        let invalid = |what: &str| {
            WyrdError::registry_invalid_card_spec(format!("stored object row is invalid: {what}"))
        };
        let mut objects = CardObjects::default();
        for batch in batches {
            let text = |name: &str| {
                batch
                    .column_by_name(name)
                    .and_then(|column| column.as_string_opt::<i32>())
                    .ok_or_else(|| invalid(name))
            };
            let (paths, digests, types) = (
                text("storage_path")?,
                text("sha256")?,
                text("content_type")?,
            );
            let (origins, relatives, states) = (text("origin")?, text("relative_path")?, text("state")?);
            let sizes = batch
                .column_by_name("size_bytes")
                .and_then(|column| column.as_primitive_opt::<Int64Type>())
                .ok_or_else(|| invalid("size_bytes"))?;
            for row in 0..batch.num_rows() {
                let cell = |column: &arrow::array::StringArray| {
                    (!column.is_null(row)).then(|| column.value(row).to_owned())
                };
                if states.value(row) == ObjectState::Deleted.as_str() {
                    objects.deleted = true;
                    continue;
                }
                if ObjectOrigin::parse(origins.value(row)) == Some(ObjectOrigin::CardEnvelope) {
                    continue;
                }
                let relative = cell(relatives).ok_or_else(|| invalid("relative_path"))?;
                let bound = tenant_path::validate(paths.value(row), tenant).is_ok_and(|path| {
                    path.card_uid == card_uid.as_str() && path.relative_path == relative
                });
                if !bound {
                    return Err(invalid("storage_path is not bound to its Card"));
                }
                let digest = cell(digests).ok_or_else(|| invalid("sha256"))?;
                let digest = hex::decode(&digest).map_err(|_| invalid("sha256"))?;
                objects.files.insert(
                    relative.clone(),
                    StoredArtifactEntry {
                        relative_path: RelativeArtifactPath::new(&relative)
                            .map_err(|_| invalid("relative_path"))?,
                        sha256: base64::engine::general_purpose::STANDARD.encode(digest),
                        size_bytes: sizes.value(row),
                        content_type: cell(types),
                    },
                );
            }
        }
        if objects.deleted {
            objects.files.clear();
        }
        Ok(objects)
    }
}
