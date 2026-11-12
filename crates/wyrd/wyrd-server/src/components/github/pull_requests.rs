//! Pull-request admission into the Verifier queue, and failed-delivery
//! recovery.
//!
//! A signature-verified `pull_request` delivery for a tenant's connected,
//! selected repository is admitted in three steps, each owned elsewhere:
//!
//! 1. [`GithubConnections::pull_request`] reads the database observation
//!    time, then GitHub's current state of the PR with a narrowed
//!    installation token, outside any transaction. The delivered payload
//!    only says which PR to look at: an old delivery never makes old source
//!    current.
//! 2. Under the PR's lock it decides on that state: an observation older
//!    than the PR's newest is obsolete; a closed or draft PR cancels its
//!    unfinished work; a source the newest generation already reviews
//!    replays it; any other open source becomes a new generation, frozen per
//!    `pull_request` binding of the selected Repository with the selecting
//!    declaration's accountable principal, and supersedes the older one.
//!    Every outcome commits before anything else happens.
//! 3. Each reserved execution's input record is staged on the Scribe outbox
//!    under a batch identity derived from its execution ID, and the queue
//!    writer activates it only after Scribe acknowledges the record. The
//!    delivery is answered only once every activation committed, so a
//!    non-2xx answer leaves GitHub's delivery failed and redeliverable, and
//!    a redelivery replays the same reserved executions.
//!
//! GitHub never redelivers a failed delivery itself.
//! [`GithubConnections::recover_deliveries`] scans the App's own delivery
//! log and asks GitHub to redeliver each recent delivery that never got a
//! 2xx answer, so recovery always replays GitHub's original delivery.

use std::time::Duration;

use chrono::Utc;
use serde::Deserialize;
use tokio_util::sync::CancellationToken;
use uuid::Uuid;
use wyrd_runtime::Permission;
use wyrd_spec::DataTenantId;
use wyrd_spec::auth::Attribution;
use wyrd_spec::error::WyrdError;
use wyrd_spec::ids::{CardName, ExecutionId, RepositoryId};
use wyrd_spec::verification::VerificationError;
use wyrd_spec::verification::execution::{PullRequestActivation, PullRequestRef};
use wyrd_sql::queries::cards::{repository_family_name, selected_repository};
use wyrd_sql::queries::verifier_executions::{
    Admission, PullRequestAdmission, PullRequestEnd, PullRequestExecution, PullRequestGeneration,
    PullRequestKey, PullRequestTarget, VerifierExecutionQueue,
};

use super::app::{GithubApp, PullRequestBody, unavailable};
use super::service::GithubConnections;
use crate::audit;
use crate::components::auth::Caller;
use crate::scribe_outbox::ScribeWrite;
use crate::state::registry_db_error;
use crate::verification::authority::{AuthorityRefusal, BackgroundAuthority};
use crate::verification::results::{InputRecord, ResultPayloadBuilder, ResultRun};
use crate::verification::submissions::{AdmissionReply, Queued};

/// Audit operation of a pull-request admission's `verifier:run` check.
const ADMIT: &str = "verification.pull_request.admit";

/// How often [`GithubConnections::run_delivery_recovery`] scans the App's
/// delivery log.
const RECOVERY_INTERVAL: Duration = Duration::from_secs(60);

/// The most redeliveries asked for one delivery; GitHub's log records each.
const MAX_REDELIVERIES: usize = 3;

/// A `pull_request` webhook body, as far as admission reads it.
#[derive(Debug, Deserialize)]
pub(crate) struct PullRequestEvent {
    /// Event action.
    action: String,
    /// The PR number.
    number: u64,
    /// The repository the PR targets.
    repository: EventRepository,
    /// What an `edited` action changed.
    #[serde(default)]
    changes: EditChanges,
}

/// The repository a `pull_request` event names.
#[derive(Debug, Deserialize)]
struct EventRepository {
    /// Stable repository id.
    id: u64,
}

/// The changed fields of an `edited` action.
#[derive(Debug, Default, Deserialize)]
struct EditChanges {
    /// Present when the base branch changed.
    base: Option<serde_json::Value>,
}

impl PullRequestEvent {
    /// Whether the event can change what the PR's current source is: open,
    /// reopen, ready for review, a new head, a base change, draft
    /// conversion, or close. Metadata-only edits, labels, assignments and
    /// reviews cannot.
    pub(crate) fn observable(&self) -> bool {
        match self.action.as_str() {
            "opened" | "reopened" | "ready_for_review" | "synchronize" | "converted_to_draft"
            | "closed" => true,
            "edited" => self.changes.base.is_some(),
            _ => false,
        }
    }
}

/// What GitHub's current state of a PR asks for.
#[derive(Debug, PartialEq, Eq)]
enum Observed {
    /// An open, ready PR under review at this source.
    Source(PullRequestRef, PullRequestRef, Option<RepositoryId>),
    /// A PR that ended or went back to draft.
    Ended(PullRequestEnd),
}

impl Observed {
    /// Classify GitHub's current `body`.
    ///
    /// # Errors
    /// Returns [`WyrdError::GithubUnavailable`] when GitHub reported a
    /// repository id Wyrd cannot hold.
    fn of(body: PullRequestBody) -> Result<Self, WyrdError> {
        if body.state != "open" {
            return Ok(Self::Ended(PullRequestEnd::Closed));
        }
        if body.draft {
            return Ok(Self::Ended(PullRequestEnd::Draft));
        }
        let head_repository_id = body
            .head
            .repo
            .map(|repo| RepositoryId::new(repo.id.to_string()))
            .transpose()
            .map_err(|_| unavailable("the pull request read"))?;
        Ok(Self::Source(
            PullRequestRef {
                ref_name: body.base.ref_name,
                sha: body.base.sha,
            },
            PullRequestRef {
                ref_name: body.head.ref_name,
                sha: body.head.sha,
            },
            head_repository_id,
        ))
    }
}

/// One entry of the App's delivery log.
#[derive(Debug, Deserialize)]
struct Delivery {
    /// The attempt's id, which redelivery names.
    id: u64,
    /// The delivery's id, shared by every attempt of it.
    guid: String,
    /// The answer's status; 0 when Wyrd never answered.
    status_code: u16,
    /// Whether this attempt was a redelivery.
    redelivery: bool,
    /// When this attempt was made.
    delivered_at: chrono::DateTime<Utc>,
}

impl GithubConnections<'_> {
    /// Admit `event`, delivery `delivery_id` for `tenant`, into the Verifier
    /// queue, returning once every execution it owes was durably admitted.
    ///
    /// See the [module](self) for the steps. An event that cannot change the
    /// PR's source, a repository the connection no longer reaches, one
    /// without a selected Repository, and an obsolete observation owe no
    /// work and return at once. A binding whose accountable principal may
    /// not run its Verifier settles `errored` with the refusal and runs
    /// nothing.
    ///
    /// # Errors
    /// Returns [`WyrdError::GithubUnavailable`] when GitHub cannot report
    /// the PR, [`WyrdError::ServiceUnavailable`] when the accountable
    /// authority cannot be decided now, this process queues no Verifier work,
    /// or an admission was lost before it committed, the staging refusal,
    /// or a registry error. Each leaves the delivery retryable.
    #[tracing::instrument(skip(self, app, event), fields(operation = "github_connection.pull_request", %tenant, number = event.number))]
    pub(crate) async fn pull_request(
        &self,
        app: &GithubApp,
        tenant: DataTenantId,
        delivery_id: &str,
        event: &PullRequestEvent,
    ) -> Result<Vec<ExecutionId>, WyrdError> {
        if !event.observable() {
            return Ok(Vec::new());
        }
        let host = app.settings().host();
        let repository_id = RepositoryId::new(event.repository.id.to_string())
            .map_err(|_| unavailable("the pull request event"))?;
        let key = PullRequestKey {
            host: &host,
            repository_id: &repository_id,
            number: event.number,
        };
        let queue = VerifierExecutionQueue::default();
        let mut conn = self.conn(tenant).await?;
        let observed_at = queue
            .observe_pull_request(&mut conn)
            .await
            .map_err(registry_db_error)?;
        conn.commit().await.map_err(registry_db_error)?;
        let token = match self.installation_token(tenant, event.repository.id).await {
            Ok(token) => token,
            Err(
                WyrdError::GithubRepositoryInaccessible { .. }
                | WyrdError::GithubNotConnected { .. },
            ) => {
                tracing::info!("the connection no longer reaches the repository; nothing admitted");
                return Ok(Vec::new());
            }
            Err(error) => return Err(error),
        };
        let Some(current) = app
            .pull_request(&token, event.repository.id, event.number)
            .await?
        else {
            tracing::info!("GitHub no longer shows the pull request; nothing admitted");
            return Ok(Vec::new());
        };
        drop(token);
        let observed = Observed::of(current)?;

        let mut conn = self.conn(tenant).await?;
        let history = queue
            .lock_pull_request(&mut conn, key)
            .await
            .map_err(registry_db_error)?;
        if history.obsolete(observed_at) {
            conn.commit().await.map_err(registry_db_error)?;
            tracing::info!("a newer observation of the pull request already decided it");
            return Ok(Vec::new());
        }
        let (base, head, head_repository_id) = match observed {
            Observed::Ended(end) => {
                let cancelled = queue
                    .deactivate_pull_request(&mut conn, key, observed_at, end)
                    .await
                    .map_err(registry_db_error)?;
                conn.commit().await.map_err(registry_db_error)?;
                tracing::info!(
                    ?end,
                    cancelled = cancelled.len(),
                    "pull request deactivated"
                );
                return Ok(Vec::new());
            }
            Observed::Source(base, head, head_repository_id) => (base, head, head_repository_id),
        };
        let source = PullRequestActivation {
            host: host.clone(),
            repository_id: repository_id.clone(),
            number: event.number,
            base,
            head,
            head_repository_id,
            delivery_id: delivery_id.to_owned(),
            generation: Uuid::now_v7(),
        };
        let replay = history
            .current
            .as_ref()
            .is_some_and(|generation| !generation.deactivated && generation.reviews(&source));
        if !replay {
            let Some(targets) = self.targets(&mut conn, &repository_id).await? else {
                conn.commit().await.map_err(registry_db_error)?;
                tracing::info!("no selected Repository binds this repository's pull requests");
                return Ok(Vec::new());
            };
            let (reserved, superseded) = queue
                .reserve_pull_request(&mut conn, &source, observed_at, &targets)
                .await
                .map_err(registry_db_error)?;
            tracing::info!(
                reserved = reserved.len(),
                superseded = superseded.len(),
                "pull request generation admitted"
            );
        }
        let generation = queue
            .lock_pull_request(&mut conn, key)
            .await
            .map_err(registry_db_error)?
            .current;
        conn.commit().await.map_err(registry_db_error)?;
        match generation {
            Some(generation) => self.hand_off(tenant, &generation).await,
            None => Ok(Vec::new()),
        }
    }

    /// The frozen target of every `pull_request` binding of the Repository
    /// selected for `repository_id`, each under the selecting declaration's
    /// accountable principal; `None` when no Repository is selected or it
    /// binds no pull requests.
    ///
    /// The principal's current authority is resolved once and `verifier:run`
    /// is decided, and audited, for each binding's Verifier; a refusal
    /// becomes that binding's settled error.
    ///
    /// # Errors
    /// Returns [`WyrdError::ServiceUnavailable`] when the authority cannot be
    /// decided now, or a registry error.
    async fn targets(
        &self,
        conn: &mut wyrd_sql::TenantConn<'_>,
        repository_id: &RepositoryId,
    ) -> Result<Option<Vec<PullRequestTarget>>, WyrdError> {
        let Some(name) = repository_family_name(conn, repository_id.as_str()).await? else {
            return Ok(None);
        };
        let name = CardName::new(name).map_err(|error| WyrdError::internal(error.to_string()))?;
        let Some(selected) = selected_repository(conn, &name).await? else {
            return Ok(None);
        };
        let bindings = VerifierExecutionQueue::default()
            .pull_request_bindings(conn, &selected.uid)
            .await
            .map_err(registry_db_error)?;
        if bindings.is_empty() {
            return Ok(None);
        }
        let principal = wyrd_runtime::principal::PrincipalId::new(selected.principal);
        let issuer = self
            .state
            .auth
            .tenant_issuer(self.state.bifrost.scribe_outbox());
        let authority = match BackgroundAuthority::new(issuer)
            .accountable(conn, principal)
            .await
            .map_err(crate::components::auth::routes::store_unavailable)?
        {
            Ok(authority) => Ok(authority),
            Err(AuthorityRefusal::Rejected(error)) => Err(error),
            Err(AuthorityRefusal::Unavailable) => {
                return Err(WyrdError::ServiceUnavailable {
                    message: "the pull request's accountable authority cannot be decided now"
                        .to_owned(),
                    details: serde_json::json!({}),
                });
            }
        };
        let refused = |error: &WyrdError| VerificationError {
            code: error.code().to_owned(),
            message: error.to_string(),
        };
        Ok(Some(
            bindings
                .into_iter()
                .map(|binding| {
                    let authority = match &authority {
                        Ok(authority) => {
                            let caller = Caller::accepted(
                                authority.authority(),
                                authority.attribution().act_principal_id,
                            );
                            audit::authorize(
                                self.state,
                                &caller,
                                &Permission::verifier_run(binding.verifier_card_uid.clone()),
                                ADMIT,
                                &format!("binding:{}", binding.binding_id),
                            )
                            .map(|()| authority.clone())
                            .map_err(|error| (authority.attribution(), refused(&error)))
                        }
                        Err(error) => Err((Attribution::direct(principal), refused(error))),
                    };
                    PullRequestTarget {
                        binding_id: binding.binding_id,
                        authority,
                    }
                })
                .collect(),
        ))
    }

    /// Stage the input record of every still-reserved execution of
    /// `generation` and wait until the queue writer activated each.
    ///
    /// Each record is built from the generation's stored pins and
    /// observation time only, so a replay stages byte-identical rows under
    /// the same batch identities and Scribe keeps each once.
    ///
    /// # Errors
    /// Returns [`WyrdError::ServiceUnavailable`] when this process queues no
    /// Verifier work or an admission was lost before it committed, the
    /// staging refusal, or an internal error for an unencodable record.
    async fn hand_off(
        &self,
        tenant: DataTenantId,
        generation: &PullRequestGeneration,
    ) -> Result<Vec<ExecutionId>, WyrdError> {
        let reserved: Vec<&PullRequestExecution> = generation
            .executions
            .iter()
            .filter(|execution| execution.reserved)
            .collect();
        let executions: Vec<ExecutionId> = generation
            .executions
            .iter()
            .map(|execution| execution.execution_id)
            .collect();
        if reserved.is_empty() {
            return Ok(executions);
        }
        let unavailable = || WyrdError::ServiceUnavailable {
            message: "the pull request was not admitted; GitHub may redeliver it".to_owned(),
            details: serde_json::json!({}),
        };
        let scribe = self.state.bifrost.scribe_outbox();
        if !scribe.sink().reaches_scribe() {
            return Err(unavailable());
        }
        let submissions = self.state.verification.submissions();
        let staged = scribe
            .reserve(reserved.len())
            .map_err(crate::components::verification::service::staging_full)?;
        let mut replies = Vec::with_capacity(reserved.len());
        for execution in reserved {
            let row = ResultPayloadBuilder::new(
                ResultRun {
                    execution_id: execution.execution_id,
                    run_id: None,
                    // The inputs row records no Verifier version.
                    verifier_version: "",
                    card_uid: &execution.card_uid,
                    owner_card_uid: Some(&execution.owner_card_uid),
                    group_id: None,
                    deployment_environment: None,
                    binding_id: Some(execution.binding_id),
                    trigger: None,
                    window: None,
                },
                &execution.verifier_card_uid,
                generation.observed_at,
                generation.observed_at,
            )
            .inputs(InputRecord::PullRequest(&generation.activation))
            .map_err(|error| WyrdError::internal(error.to_string()))?;
            let queued = submissions
                .reserve(1)
                .map_err(crate::components::verification::service::staging_full)?;
            let (reply, admitted) = AdmissionReply::channel();
            staged.stage(
                tenant,
                ScribeWrite::ExecutionInputs {
                    row,
                    attribution: execution.attribution,
                    queued: Some((
                        queued,
                        Box::new(Queued {
                            admission: Admission::PullRequest(PullRequestAdmission {
                                execution_id: execution.execution_id,
                                attribution: execution.attribution,
                            }),
                            claim: None,
                            reply: Some(reply),
                        }),
                    )),
                },
            );
            replies.push(admitted);
        }
        for admitted in replies {
            admitted.await.map_err(|_lost| unavailable())?;
        }
        Ok(executions)
    }

    /// Ask GitHub to redeliver every recent delivery of the App that never
    /// received a 2xx answer, at most [`MAX_REDELIVERIES`] times each;
    /// returns how many redeliveries were asked for.
    ///
    /// GitHub's own delivery log is the progress record: a delivery with any
    /// 2xx attempt is done, and one already redelivered the most times is
    /// left to the operator. Redelivery replays the original delivery, so
    /// admission rediscovers the same reserved executions.
    ///
    /// # Errors
    /// Returns [`WyrdError::GithubNotConfigured`] or
    /// [`WyrdError::GithubUnavailable`] when the log cannot be read.
    pub async fn recover_deliveries(&self) -> Result<usize, WyrdError> {
        let app = self.app()?;
        let mut attempts: std::collections::BTreeMap<String, Vec<Delivery>> =
            std::collections::BTreeMap::new();
        for delivery in app.deliveries::<Delivery>().await? {
            attempts
                .entry(delivery.guid.clone())
                .or_default()
                .push(delivery);
        }
        let mut asked = 0;
        for (guid, attempts) in attempts {
            let done = attempts
                .iter()
                .any(|attempt| (200..300).contains(&attempt.status_code));
            let redelivered = attempts.iter().filter(|attempt| attempt.redelivery).count();
            if done || redelivered >= MAX_REDELIVERIES {
                continue;
            }
            let Some(latest) = attempts.iter().max_by_key(|attempt| attempt.delivered_at) else {
                continue;
            };
            match app.redeliver(latest.id).await {
                Ok(()) => asked += 1,
                Err(error) => {
                    tracing::warn!(%guid, %error, "GitHub refused a redelivery; next scan retries");
                }
            }
        }
        Ok(asked)
    }

    /// Scan for failed deliveries every [`RECOVERY_INTERVAL`] until
    /// `shutdown`.
    ///
    /// Shutdown also abandons a scan still waiting on GitHub, so recovery
    /// never spends the process's shared drain budget; GitHub's delivery log
    /// still shows whatever was not redelivered, and the next process's scan
    /// asks again.
    pub async fn run_delivery_recovery(self, shutdown: CancellationToken) {
        // ponytail: every replica scans the App-wide log, so a failed delivery
        // may be redelivered once per replica per scan, up to MAX_REDELIVERIES
        // in all; admission replays them idempotently. Elect one scanner if
        // GitHub's redelivery rate limit is reached.
        let mut ticks = tokio::time::interval(RECOVERY_INTERVAL);
        ticks.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
        loop {
            tokio::select! {
                () = shutdown.cancelled() => break,
                _ = ticks.tick() => {}
            }
            tokio::select! {
                () = shutdown.cancelled() => break,
                scanned = self.recover_deliveries() => match scanned {
                    Ok(0) => {}
                    Ok(asked) => tracing::info!(asked, "failed GitHub deliveries redelivered"),
                    Err(error) => tracing::warn!(%error, "GitHub delivery log unavailable; next scan retries"),
                },
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The event `action` with `changes`.
    fn event(action: &str, changes: serde_json::Value) -> PullRequestEvent {
        serde_json::from_value(serde_json::json!({
            "action": action,
            "number": 7,
            "repository": {"id": 1},
            "changes": changes,
        }))
        .expect("the event decodes")
    }

    /// Open, reopen, ready, new head, base change, draft conversion and
    /// close are observed; metadata-only edits and other actions are not.
    ///
    /// # Panics
    /// Panics when an action's eligibility differs.
    #[test]
    fn only_source_and_lifecycle_actions_are_observed() {
        for action in [
            "opened",
            "reopened",
            "ready_for_review",
            "synchronize",
            "converted_to_draft",
            "closed",
        ] {
            assert!(
                event(action, serde_json::json!({})).observable(),
                "{action}"
            );
        }
        assert!(
            event(
                "edited",
                serde_json::json!({"base": {"ref": {"from": "main"}}})
            )
            .observable()
        );
        assert!(!event("edited", serde_json::json!({"title": {"from": "old"}})).observable());
        for action in ["labeled", "assigned", "review_requested", "locked"] {
            assert!(
                !event(action, serde_json::json!({})).observable(),
                "{action}"
            );
        }
    }

    /// GitHub's current state, not the delivered action, decides: closed
    /// ends, an open draft ends as draft, and an open ready PR is reviewed
    /// at its current base, head and head repository.
    ///
    /// # Panics
    /// Panics when a state is classified otherwise.
    #[test]
    fn current_provider_state_decides_the_observation() {
        let body = |state: &str, draft: bool| -> PullRequestBody {
            serde_json::from_value(serde_json::json!({
                "state": state,
                "draft": draft,
                "base": {"ref": "main", "sha": "b", "repo": {"id": 1}},
                "head": {"ref": "feature", "sha": "h", "repo": {"id": 9}},
            }))
            .expect("the body decodes")
        };
        assert_eq!(
            Observed::of(body("closed", false)).expect("classifies"),
            Observed::Ended(PullRequestEnd::Closed)
        );
        assert_eq!(
            Observed::of(body("closed", true)).expect("classifies"),
            Observed::Ended(PullRequestEnd::Closed)
        );
        assert_eq!(
            Observed::of(body("open", true)).expect("classifies"),
            Observed::Ended(PullRequestEnd::Draft)
        );
        assert_eq!(
            Observed::of(body("open", false)).expect("classifies"),
            Observed::Source(
                PullRequestRef {
                    ref_name: "main".to_owned(),
                    sha: "b".to_owned()
                },
                PullRequestRef {
                    ref_name: "feature".to_owned(),
                    sha: "h".to_owned()
                },
                Some(RepositoryId::new("9").expect("valid")),
            )
        );
    }
}
