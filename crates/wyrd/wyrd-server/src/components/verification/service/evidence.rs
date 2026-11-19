//! Deferred evidence publication: matching a published input to the
//! Verifiers subscribed to it and accepting it durably for each.

use std::collections::{BTreeMap, BTreeSet};
use std::sync::Arc;
use std::time::Instant;

use chrono::{DateTime, SubsecRound, Utc};
use vala_verify::blocks::{EngineError, EvidenceReader as _, conform};
use wyrd_runtime::Permission;
use wyrd_spec::DataTenantId;
use wyrd_spec::card::repository::RepositoryRemote;
use wyrd_spec::error::WyrdError;
use wyrd_spec::ids::{CardName, CardUid, RepositoryId, SlotName};
use wyrd_spec::verification::execution::{
    EvidenceRepository, EvidenceStatus, InputDeclaration, PublishEvidenceRequest,
    PublishEvidenceResponse,
};
use wyrd_sql::queries::cards::{repository_family_name, selected_repository};
use wyrd_sql::queries::verifier_executions::VerifierExecutionQueue;

use super::{PUBLISH_EVIDENCE, VerificationControl};
use crate::components::auth::Caller;
use crate::components::github::service::GithubConnections;
use crate::components::storage::inventory::ObjectInventory;
use crate::state::registry_db_error;
use crate::verification::cache::Db;
use crate::verification::evidence::{Accepted, DeferredEvidence, EvidenceSource, full_commit};
use crate::verification::hosted::evidence::TenantEvidence;
use crate::verification::inputs::InputResolver;

impl VerificationControl<'_> {
    /// Accept `request`'s evidence for every Verifier version subscribed to
    /// its deferred input on its repository and commit, and return the
    /// durable receipt.
    ///
    /// Any `verifier:run` grant admits the call; the repository must be one
    /// the tenant's GitHub installation selects. Subscribers are the
    /// `pull_request` bindings of the repository's selected Repository Card
    /// and every unfinished execution admitted for the commit, each kept only
    /// when its Verifier declares `input` with `deferred: true` and the caller
    /// may run it on its subject. The value must conform to every kept
    /// subscriber's slot; a File must be a completed upload, and an Artifact
    /// file must resolve under the caller's authority with a format and size
    /// its slot accepts. A subscriber
    /// that already accepted the same content replays; different content is
    /// refused. New acceptances are written through Scribe, and the receipt
    /// is returned only once each is acknowledged; consumers parked on the
    /// commit are then made due, on a replay as well. Publication never
    /// starts an execution and never reopens a settled one.
    ///
    /// # Errors
    /// Returns [`WyrdError::VerificationWrongTarget`] on a target that does
    /// not run queued work; [`WyrdError::PermissionDeniedRbac`] without any
    /// `verifier:run` grant, or when every subscriber is one the caller may not run;
    /// [`WyrdError::VerificationInputInvalid`] for an abbreviated commit, a
    /// trace, a File that is not a completed upload, or a value that does not
    /// conform to a subscriber's slot; [`WyrdError::VerificationInputTooLarge`]
    /// past a slot's size limit; the GitHub errors of
    /// [`GithubConnections::verify_repository`] for a repository the tenant
    /// does not reach; [`WyrdError::VerificationEvidenceUnmatched`] when no
    /// Verifier subscribes, with nothing retained;
    /// [`WyrdError::VerificationEvidenceConflict`] when a subscriber already
    /// accepted different evidence; and [`WyrdError::ServiceUnavailable`] or
    /// `WYRD_VALA_503_BIFROST_WRITER_UNAVAILABLE` when acceptance cannot be
    /// decided or acknowledged now, so the caller publishes again.
    #[tracing::instrument(name = "verification.evidence.publish", skip_all, fields(input = %request.input, matched))]
    pub(crate) async fn publish_evidence(
        &self,
        caller: &Caller,
        request: PublishEvidenceRequest,
    ) -> Result<PublishEvidenceResponse, WyrdError> {
        self.state.settings.role.require_queued(PUBLISH_EVIDENCE)?;
        self.authorize_evidence(caller, PUBLISH_EVIDENCE, "verification.evidence.publish")?;
        let invalid = |reason: &str| WyrdError::VerificationInputInvalid {
            message: format!("input {}: {reason}", request.input),
            details: serde_json::json!({ "input": request.input }),
        };
        if !full_commit(&request.tags.commit) {
            return Err(invalid(
                "tags.commit must be the full 40-character commit SHA",
            ));
        }
        if matches!(request.value, InputDeclaration::Trace { .. }) {
            return Err(invalid("a deferred input is JSON or media, never a trace"));
        }
        let tenant = caller.data_tenant_id;
        let connections = GithubConnections::new(self.state);
        let remote = RepositoryRemote {
            host: connections.host()?,
            full_name: request.repository.clone(),
        };
        let (host, access) = connections
            .verify_repository(tenant, None, None, Some(&remote))
            .await?;
        let repository_id = &RepositoryId::new(access.repository_id)
            .map_err(|error| WyrdError::internal(error.to_string()))?;
        let source = EvidenceSource {
            host: &host,
            repository_id,
            commit: request.tags.commit.as_str(),
        };
        let subscribers = self.subscribers(caller, source, &request).await?;
        if let InputDeclaration::File {
            sha256, size_bytes, ..
        } = &request.value
            && ObjectInventory::new(self.state)
                .file_object(tenant, sha256, *size_bytes)
                .await?
                .is_none()
        {
            return Err(invalid("the File is not a completed evidence upload"));
        }
        let published = DeferredEvidence::publish(source, &request.input, request.value.clone())?;
        let custody = DeferredEvidence::new(self.state, tenant);
        let mut conn = self
            .state
            .postgres
            .wyrd()
            .tenant_conn(tenant)
            .await
            .map_err(registry_db_error)?;
        let query = self
            .records(&mut conn, caller.data_tenant_id, Some(caller))
            .await?;
        conn.commit().await.map_err(registry_db_error)?;
        let records = InputResolver::new(&query, None);
        let held = custody
            .accepted(&records, source, &request.input, &subscribers)
            .await?;
        conflicts(&held, &published.sha256, &request.input)?;
        let fresh: Vec<CardUid> = subscribers
            .iter()
            .filter(|verifier| !held.contains_key(*verifier))
            .cloned()
            .collect();
        // Bifrost keeps microseconds; a fresh receipt states what a replay reads.
        let now = Utc::now().trunc_subsecs(6);
        let mut raced = BTreeMap::new();
        if !fresh.is_empty() {
            let deadline = Instant::now() + self.state.settings.limits.timeout / 2;
            let accepted = custody
                .accept(
                    caller.attribution(),
                    (source, &request.input),
                    &published,
                    (&fresh, now),
                    deadline,
                )
                .await;
            if let Err(error) = accepted {
                // A concurrent publisher may have won the fence first; its
                // acceptance is the receipt this publication replays.
                raced = custody
                    .accepted(&records, source, &request.input, &fresh)
                    .await?;
                conflicts(&raced, &published.sha256, &request.input)?;
                if raced.len() < fresh.len() {
                    return Err(error);
                }
            }
        }
        // A replay wakes too: a publisher retrying an unacknowledged
        // acceptance resumes the consumers that acceptance never woke.
        self.wake(tenant, source, &subscribers).await;
        let (status, recorded_at) = receipt(&held, &fresh, &raced, now);
        tracing::Span::current().record("matched", subscribers.len());
        Ok(PublishEvidenceResponse {
            evidence_id: published.evidence_id,
            repository: EvidenceRepository {
                host,
                repository_id: repository_id.clone(),
            },
            tags: request.tags,
            input: request.input,
            status,
            matched_subscriptions: u32::try_from(subscribers.len()).unwrap_or(u32::MAX),
            recorded_at,
        })
    }

    /// The Verifier versions subscribed to `request.input` for `source`
    /// that the caller may run, each conforming to `request.value`.
    ///
    /// # Errors
    /// Returns [`WyrdError::VerificationEvidenceUnmatched`] when no Verifier
    /// subscribes, the caller's refusal when every subscriber is one it may
    /// not run, the slot nonconformity of a kept subscriber, and a registry
    /// error when a read fails.
    async fn subscribers(
        &self,
        caller: &Caller,
        source: EvidenceSource<'_>,
        request: &PublishEvidenceRequest,
    ) -> Result<Vec<CardUid>, WyrdError> {
        let tenant = caller.data_tenant_id;
        let mut conn = self
            .state
            .postgres
            .wyrd()
            .tenant_conn(tenant)
            .await
            .map_err(registry_db_error)?;
        let selected =
            match repository_family_name(&mut conn, source.repository_id.as_str()).await? {
                Some(name) => {
                    let name = CardName::new(name)
                        .map_err(|error| WyrdError::internal(error.to_string()))?;
                    selected_repository(&mut conn, &name)
                        .await?
                        .map(|selected| selected.uid)
                }
                None => None,
            };
        let candidates = VerifierExecutionQueue::default()
            .evidence_subscribers(
                &mut conn,
                selected.as_ref(),
                (source.host, source.repository_id, source.commit),
            )
            .await
            .map_err(registry_db_error)?;
        conn.commit().await.map_err(registry_db_error)?;
        let evidence = Arc::new(BTreeMap::from([(
            request.input.clone(),
            request.value.clone(),
        )]));
        // An Artifact is resolved under the publisher's authority and
        // admitted against each slot here, before anything is accepted.
        let artifacts = match &request.value {
            InputDeclaration::Artifact { .. } => {
                let blocks = self.blocks()?;
                // A request-path admission, charged to process memory.
                Some(TenantEvidence::new(
                    self.state.clone(),
                    caller.clone(),
                    *blocks.limits(),
                    blocks.resources().memory().clone(),
                ))
            }
            _ => None,
        };
        let (mut kept, mut refused) = (BTreeSet::new(), None);
        for (verifier, subject) in candidates {
            if kept.contains(&verifier) {
                continue;
            }
            let target = self
                .state
                .verification
                .verifier_cards()
                .resolve(
                    Db::Pool(self.state.postgres.wyrd()),
                    tenant,
                    &verifier,
                    &subject,
                )
                .await
                .map_err(registry_db_error)?;
            let Ok(target) = target else { continue };
            if !target
                .spec
                .inputs
                .get(&request.input)
                .is_some_and(wyrd_spec::card::verifier::InputSlot::is_deferred)
            {
                continue;
            }
            if let Err(error) = self.authorize_verifier(
                caller,
                Permission::verifier_run(verifier.clone()),
                PUBLISH_EVIDENCE,
                "verification.evidence.publish",
            ) {
                refused.get_or_insert(error);
                continue;
            }
            conform(&target.spec, &evidence).map_err(nonconforming)?;
            if let Some(artifacts) = &artifacts {
                artifacts
                    .admit(&target.spec.inputs, &evidence)
                    .await
                    .map_err(nonconforming)?;
            }
            kept.insert(verifier);
        }
        if kept.is_empty() {
            return Err(refused.unwrap_or_else(|| WyrdError::VerificationEvidenceUnmatched {
                message: format!(
                    "no Verifier bound to this repository declares the deferred input {}; nothing was retained",
                    request.input
                ),
                details: serde_json::json!({
                    "input": request.input,
                    "repository_id": source.repository_id,
                    "commit": source.commit,
                }),
            }));
        }
        Ok(kept.into_iter().collect())
    }

    /// Make the queued consumers of `verifiers` for `source` due now. A
    /// failure is logged: each parked consumer rechecks its evidence when
    /// its bounded park ends.
    async fn wake(&self, tenant: DataTenantId, source: EvidenceSource<'_>, verifiers: &[CardUid]) {
        let woken = async {
            let mut conn = self.state.postgres.wyrd().tenant_conn(tenant).await?;
            let woken = VerifierExecutionQueue::default()
                .wake_evidence_consumers(
                    &mut conn,
                    (source.host, source.repository_id, source.commit),
                    verifiers,
                )
                .await?;
            conn.commit().await?;
            Ok::<_, wyrd_sql::SqlError>(woken)
        };
        match woken.await {
            Ok(woken) => tracing::info!(woken = woken.len(), "deferred evidence consumers woken"),
            Err(error) => {
                tracing::warn!(%error, "waking deferred evidence consumers failed; their park recheck repairs it");
            }
        }
    }
}

/// Refuse `held` acceptances of content other than `sha256`.
///
/// # Errors
/// Returns [`WyrdError::VerificationEvidenceConflict`] naming `input`.
fn conflicts(
    held: &BTreeMap<CardUid, Accepted>,
    sha256: &str,
    input: &SlotName,
) -> Result<(), WyrdError> {
    match held
        .values()
        .find(|accepted| accepted.published.sha256 != sha256)
    {
        None => Ok(()),
        Some(accepted) => Err(WyrdError::VerificationEvidenceConflict {
            message: format!(
                "different evidence was already accepted for input {input} at this commit"
            ),
            details: serde_json::json!({
                "input": input,
                "evidence_id": accepted.published.evidence_id,
            }),
        }),
    }
}

/// A subscriber slot's refusal of the published value as its public error;
/// evidence that cannot be admitted now is unavailable, so the caller
/// publishes again.
fn nonconforming(error: EngineError) -> WyrdError {
    match error {
        EngineError::InputTooLarge(reason) => WyrdError::VerificationInputTooLarge {
            message: reason.clone(),
            details: serde_json::json!({ "reason": reason }),
        },
        EngineError::EvidenceUnavailable(reason) => WyrdError::ServiceUnavailable {
            message: reason,
            details: serde_json::json!({}),
        },
        error => {
            let reason = error.to_string();
            WyrdError::VerificationInputInvalid {
                message: reason.clone(),
                details: serde_json::json!({ "reason": reason }),
            }
        }
    }
}

/// The receipt status and time a publication answers.
///
/// `held` is every subscriber's acceptance read before this publication
/// offered; `fresh` is the subscribers it offered to at `now`; `raced` is
/// the canonical acceptance a concurrent publisher won for any of them. A
/// publication whose every fresh offer lost replays, and the receipt time is
/// the first acceptance, so a losing offer's own `now` never reaches the
/// receipt.
fn receipt(
    held: &BTreeMap<CardUid, Accepted>,
    fresh: &[CardUid],
    raced: &BTreeMap<CardUid, Accepted>,
    now: DateTime<Utc>,
) -> (EvidenceStatus, DateTime<Utc>) {
    let accepted_now = fresh.iter().any(|verifier| !raced.contains_key(verifier));
    let recorded_at = held
        .values()
        .chain(raced.values())
        .map(|accepted| accepted.recorded_at)
        .chain(accepted_now.then_some(now))
        .min()
        .unwrap_or(now);
    let status = if accepted_now {
        EvidenceStatus::Accepted
    } else {
        EvidenceStatus::Replayed
    };
    (status, recorded_at)
}

#[cfg(test)]
mod tests {
    use chrono::TimeDelta;

    use super::*;

    /// A publication whose fresh offer lost the acceptance race answers the
    /// winner's receipt: replayed, at the first acceptance, not its own
    /// offer time. An offer that won answers accepted at its own time.
    #[test]
    fn a_lost_race_replays_the_first_acceptance() {
        let repository = RepositoryId::new("2").expect("id");
        let commit = "c0".repeat(20);
        let source = EvidenceSource {
            host: "github.com",
            repository_id: &repository,
            commit: &commit,
        };
        let input = SlotName::new("ci").expect("slot");
        let green = || {
            serde_json::from_value(
                serde_json::json!({"kind": "json", "value": {"status": "green"}}),
            )
            .expect("declaration")
        };
        let winner = DeferredEvidence::publish(source, &input, green()).expect("publishes");
        let loser = DeferredEvidence::publish(source, &input, green()).expect("publishes");
        assert_eq!(
            loser.evidence_id, winner.evidence_id,
            "same content, same identity"
        );
        let subscriber = CardUid::from_uuid(uuid::Uuid::now_v7()).expect("uid");
        let first = Utc::now().trunc_subsecs(6);
        let offered = first + TimeDelta::seconds(5);
        let raced = BTreeMap::from([(
            subscriber.clone(),
            Accepted {
                published: winner,
                recorded_at: first,
            },
        )]);
        let fresh = [subscriber];

        assert_eq!(
            receipt(&BTreeMap::new(), &fresh, &raced, offered),
            (EvidenceStatus::Replayed, first)
        );
        assert_eq!(
            receipt(&BTreeMap::new(), &fresh, &BTreeMap::new(), offered),
            (EvidenceStatus::Accepted, offered)
        );
    }
}
