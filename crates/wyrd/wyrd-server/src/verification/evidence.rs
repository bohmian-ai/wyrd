//! Published deferred evidence: its identity, each subscribed Verifier
//! version's durable acceptance of it, and the read-back every consumer
//! shares.
//!
//! One acceptance is one `bifrost.verification.evidence` row, written through
//! Scribe as its own batch under a batch id its source, input, and
//! subscriber derive. The row is the lasting record that the subscriber
//! accepted evidence for that source and input. Consumers read Bifrost alone;
//! an unreadable acceptance can appear absent, while its fixed batch identity
//! refuses conflicting writes. Failed reads are unavailable and retry. Nothing
//! is mirrored in PostgreSQL; the queue only learns that a consumer is due.

use std::collections::BTreeMap;
use std::sync::{Arc, LazyLock, Mutex};

use arrow::datatypes::{DataType, Field, Schema, SchemaRef};
use chrono::{DateTime, Utc};
use sha2::{Digest as _, Sha256};
use std::time::Instant;
use tokio::sync::oneshot;
use uuid::Uuid;
use vala_bifrost_redux::tables::DomainTable;
use vala_bifrost_redux::tables::staged::{Cell, StagedRow};
use vala_bifrost_redux::tables::verification::EvidenceTable;
use wyrd_queue::variant::EncodedVariant;
use wyrd_spec::DataTenantId;
use wyrd_spec::auth::Attribution;
use wyrd_spec::error::WyrdError;
use wyrd_spec::ids::{CardUid, EvidenceId, RepositoryId, SlotName};
use wyrd_spec::origin::CommitSha;
use wyrd_spec::vala::BifrostError;
use wyrd_spec::vala::managed_columns::{RUN_ID, SUBJECT_CARD_UID, WORKLOAD_CARD_UID};
use wyrd_spec::verification::execution::InputDeclaration;

use super::inputs::InputResolver;
use crate::scribe_outbox::{ScribeTable, ScribeWrite};
use crate::state::AppState;

/// The table every acceptance is a row of, and the logical table of every
/// acceptance's batch fence.
pub(crate) const EVIDENCE_TABLE: &str = "bifrost.verification.evidence";

/// The one schema every staged acceptance row shares: the table's fields,
/// then the workload, subject, and run columns Scribe attributes rows by.
static SCHEMA: LazyLock<SchemaRef> = LazyLock::new(|| {
    let mut fields = EvidenceTable::arrow_fields();
    fields.extend(
        [WORKLOAD_CARD_UID, SUBJECT_CARD_UID, RUN_ID]
            .map(|name| Field::new(name, DataType::Utf8, true)),
    );
    Arc::new(Schema::new(fields))
});

/// The source one piece of deferred evidence is about: a connected
/// repository's exact commit.
#[derive(Debug, Clone, Copy)]
pub(crate) struct EvidenceSource<'a> {
    /// Provider web host.
    pub(crate) host: &'a str,
    /// Stable provider id of the repository.
    pub(crate) repository_id: &'a RepositoryId,
    /// The exact commit.
    pub(crate) commit: &'a str,
}

/// One published value and its content identity.
#[derive(Debug, Clone)]
pub(crate) struct Published {
    /// Derived from the source, input, and content.
    pub(crate) evidence_id: EvidenceId,
    /// Hex SHA-256 of the canonical JSON of the declaration without its
    /// display filename.
    pub(crate) sha256: String,
    /// The declaration itself.
    pub(crate) value: InputDeclaration,
}

/// One subscriber's accepted evidence, as read back.
#[derive(Debug, Clone)]
pub(crate) struct Accepted {
    /// The accepted value and its identity.
    pub(crate) published: Published,
    /// When it was first accepted.
    pub(crate) recorded_at: DateTime<Utc>,
}

/// One tenant's deferred evidence custody.
pub(crate) struct DeferredEvidence<'a> {
    /// Process state: the Scribe outbox and the fence store.
    app: &'a AppState,
    /// The tenant the evidence belongs to.
    tenant: DataTenantId,
}

impl<'a> DeferredEvidence<'a> {
    /// `tenant`'s custody through `app`.
    pub(crate) const fn new(app: &'a AppState, tenant: DataTenantId) -> Self {
        Self { app, tenant }
    }

    /// `value` published for `input` of `source`, with its content identity.
    /// A File is identified by its digest, size, and format; its display
    /// filename is not part of the identity.
    ///
    /// # Errors
    /// Returns an internal error when the declaration cannot be canonicalized.
    pub(crate) fn publish(
        source: EvidenceSource<'_>,
        input: &SlotName,
        value: InputDeclaration,
    ) -> Result<Published, WyrdError> {
        let mut identity = serde_json::to_value(&value)
            .map_err(|error| WyrdError::internal(format!("evidence identity: {error}")))?;
        if let Some(object) = identity.as_object_mut() {
            object.remove("filename");
        }
        let canonical = serde_jcs::to_string(&identity)
            .map_err(|error| WyrdError::internal(format!("evidence identity: {error}")))?;
        let sha256 = hex::encode(Sha256::digest(canonical.as_bytes()));
        let id = derived(&[
            source.host,
            source.repository_id.as_str(),
            source.commit,
            input.as_str(),
            &sha256,
        ]);
        let evidence_id = EvidenceId::new(id)
            .map_err(|error| WyrdError::internal(format!("evidence id: {error}")))?;
        Ok(Published {
            evidence_id,
            sha256,
            value,
        })
    }

    /// Every acceptance `subscribers` hold for `input` of `source`, read
    /// through `records`. A subscriber without one is absent from the map.
    ///
    /// Reads come only from Bifrost. An acceptance Scribe accepted but no
    /// query sees yet is absent here: a waiting execution stays parked until
    /// a later read sees it, and a resubmission is absorbed, or refused when
    /// different, under the acceptance's batch id.
    ///
    /// # Errors
    /// Returns the query error, or an internal error when a row is
    /// malformed.
    pub(crate) async fn accepted(
        &self,
        records: &InputResolver<'_>,
        source: EvidenceSource<'_>,
        input: &SlotName,
        subscribers: &[CardUid],
    ) -> Result<BTreeMap<CardUid, Accepted>, WyrdError> {
        let mut rows = records.published(source, input).await?;
        Ok(subscribers
            .iter()
            .filter_map(|verifier| rows.remove_entry(verifier))
            .collect())
    }

    /// Accept `published` for `input` of `source` for each of `subscribers`,
    /// each retained under its exact subject in `subjects`, recorded at
    /// `recorded_at` under `attribution`, and wait for every
    /// acceptance's own Scribe acknowledgement until `deadline`.
    ///
    /// Each acceptance is staged on its own reserved outbox slot. A
    /// resubmission of the same acceptance is absorbed by its batch fence.
    ///
    /// # Errors
    /// Returns `WYRD_VALA_503_BIFROST_WRITER_UNAVAILABLE` when an acceptance
    /// gets no reserved slot, or is rejected, dropped, or unacknowledged,
    /// before `deadline`; the outcome is unknown and the caller rechecks the
    /// fences. Returns an internal error when a row cannot be built.
    pub(crate) async fn accept(
        &self,
        attribution: Attribution,
        (source, input): (EvidenceSource<'_>, &SlotName),
        published: &Published,
        (subscribers, subjects, recorded_at): (
            &[CardUid],
            &BTreeMap<CardUid, CardUid>,
            DateTime<Utc>,
        ),
        deadline: Instant,
    ) -> Result<(), WyrdError> {
        let outbox = self.app.bifrost.scribe_outbox();
        let writer = || {
            WyrdError::from(BifrostError::WriterUnavailable {
                table: EVIDENCE_TABLE.to_owned(),
            })
        };
        let mut acks = Vec::with_capacity(subscribers.len());
        for verifier in subscribers {
            let subject = subjects.get(verifier).ok_or_else(|| {
                WyrdError::internal("every evidence subscriber names its subject")
            })?;
            let row = row(source, input, (verifier, subject), published, recorded_at)?;
            let slot = outbox.reserve_until(1, deadline).await.map_err(|full| {
                tracing::warn!(
                    admitted = full.admitted,
                    limit = full.limit,
                    "a deferred evidence acceptance got no reserved slot"
                );
                writer()
            })?;
            let (sender, ack) = oneshot::channel();
            slot.stage(
                self.tenant,
                ScribeWrite::Fenced {
                    table: ScribeTable::Evidence,
                    batch: key(source, input, verifier),
                    row,
                    attribution,
                    acked: Mutex::new(Some(sender)),
                },
            );
            acks.push(ack);
        }
        let acked =
            tokio::time::timeout_at(deadline.into(), futures_util::future::try_join_all(acks));
        match acked.await {
            Ok(Ok(_)) => Ok(()),
            outcome => {
                tracing::warn!(
                    timed_out = outcome.is_err(),
                    "a deferred evidence acceptance was not acknowledged"
                );
                Err(writer())
            }
        }
    }
}

/// The batch identity of `verifier`'s acceptance of `input` for `source`:
/// every publisher of it offers the same id, so a retry dedups and different
/// evidence conflicts.
fn key(source: EvidenceSource<'_>, input: &SlotName, verifier: &CardUid) -> Uuid {
    derived(&[
        source.host,
        source.repository_id.as_str(),
        source.commit,
        input.as_str(),
        &verifier.to_string(),
    ])
}

/// A version 7 shaped UUID derived from `parts`, each length-prefixed so no
/// two part lists collide.
fn derived(parts: &[&str]) -> Uuid {
    let mut digest = Sha256::new();
    digest.update(b"wyrd.verification.evidence.v1");
    for part in parts {
        digest.update((part.len() as u64).to_le_bytes());
        digest.update(part.as_bytes());
    }
    let digest: [u8; 32] = digest.finalize().into();
    let mut bytes = [0_u8; 16];
    bytes.copy_from_slice(&digest[..16]);
    bytes[6] = (bytes[6] & 0x0f) | 0x70;
    bytes[8] = (bytes[8] & 0x3f) | 0x80;
    Uuid::from_bytes(bytes)
}

/// `verifier`'s acceptance of `published` as its staged row, retained
/// under `subject`, the exact Card of the work it was accepted for.
///
/// # Errors
/// Returns an internal error when the declaration exceeds a Variant limit
/// or a value does not fit its column.
fn row(
    source: EvidenceSource<'_>,
    input: &SlotName,
    (verifier, subject): (&CardUid, &CardUid),
    published: &Published,
    recorded_at: DateTime<Utc>,
) -> Result<StagedRow, WyrdError> {
    let value = EncodedVariant::from_serialize(&published.value)
        .map_err(|violation| WyrdError::from(violation.into_error("value", 0)))?;
    let (evidence_id, verifier) = (published.evidence_id.to_string(), verifier.to_string());
    let mut value = Some(value);
    let cells = SCHEMA
        .fields()
        .iter()
        .map(|field| match field.name().as_str() {
            "evidence_id" => Cell::text(Some(evidence_id.as_str())),
            "host" => Cell::text(Some(source.host)),
            "repository_id" => Cell::text(Some(source.repository_id.as_str())),
            "commit" => Cell::text(Some(source.commit)),
            "input" => Cell::text(Some(input.as_str())),
            "verifier_card_uid" => Cell::text(Some(verifier.as_str())),
            "sha256" => Cell::text(Some(published.sha256.as_str())),
            "value" => Cell::Variant(value.take()),
            "recorded_at" => Cell::Micros(Some(recorded_at.timestamp_micros())),
            SUBJECT_CARD_UID => Cell::text(Some(subject.as_str())),
            // Shared dimensions and the workload and run columns.
            _ => Cell::text(None::<&str>),
        })
        .collect();
    StagedRow::new(Arc::clone(&SCHEMA), cells)
        .map_err(|error| WyrdError::internal(format!("evidence row: {error}")))
}

/// A full 40-character lowercase hex commit, the only commit a PR execution
/// is admitted for.
#[must_use]
pub(crate) fn full_commit(commit: &CommitSha) -> bool {
    commit.as_str().len() == 40
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The same source, input, and content name the same evidence however
    /// the File is labelled; other content or another source does not.
    ///
    /// # Panics
    /// Panics when identities collide or a display name changes one.
    #[test]
    fn identity_follows_source_and_content_not_filename() {
        let repository = RepositoryId::new("42").expect("repository id");
        let source = EvidenceSource {
            host: "github.com",
            repository_id: &repository,
            commit: "0123456789abcdef0123456789abcdef01234567",
        };
        let input = SlotName::new("ci").expect("slot");
        let file = |filename: Option<&str>, size_bytes| {
            serde_json::from_value::<InputDeclaration>(serde_json::json!({
                "kind": "file",
                "sha256": "a".repeat(64),
                "size_bytes": size_bytes,
                "format": "json",
                "filename": filename,
            }))
            .expect("declaration")
        };
        let named =
            DeferredEvidence::publish(source, &input, file(Some("a.json"), 3)).expect("identity");
        let renamed =
            DeferredEvidence::publish(source, &input, file(Some("b.json"), 3)).expect("identity");
        let bigger = DeferredEvidence::publish(source, &input, file(None, 4)).expect("identity");
        assert_eq!(named.evidence_id, renamed.evidence_id);
        assert_eq!(named.sha256, renamed.sha256);
        assert_ne!(named.sha256, bigger.sha256);
        let other = EvidenceSource {
            commit: "1123456789abcdef0123456789abcdef01234567",
            ..source
        };
        let moved =
            DeferredEvidence::publish(other, &input, file(Some("a.json"), 3)).expect("identity");
        assert_eq!(named.sha256, moved.sha256);
        assert_ne!(named.evidence_id, moved.evidence_id);
        let verifier = CardUid::from_uuid(uuid::Uuid::now_v7()).expect("card uid");
        assert_ne!(
            key(source, &input, &verifier),
            key(other, &input, &verifier)
        );
    }
}
