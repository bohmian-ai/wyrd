//! Pull-request admission into the Verifier queue, and failed-delivery
//! recovery.
//!
//! A signature-verified `pull_request` delivery for a tenant's connected,
//! selected repository is admitted in three steps, each owned elsewhere:
//!
//! 1. [`GithubConnections::pull_request`] mints a narrowed installation
//!    token for the installation the delivery was routed by, then takes the
//!    PR's lock, reads the database observation time and GitHub's current
//!    state of the PR while holding it. The delivered payload only says
//!    which PR to look at: an old delivery never makes old source current,
//!    and a later delivery of the same PR waits until this one decided.
//! 2. Still under the PR's lock, and only while the tenant's connection is
//!    at the revision the token was minted under, it decides on that state:
//!    an observation older than the PR's newest is obsolete; a closed or
//!    draft PR cancels its unfinished work; a source the newest generation
//!    already reviews replays it; any other open source becomes a new
//!    generation, frozen per `pull_request` binding of the selected
//!    Repository with the selecting declaration's accountable principal,
//!    and supersedes the older one, which is deactivated outright when the
//!    selected policy binds nothing. Every outcome commits before anything
//!    else happens.
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
use wyrd_spec::github_connection::{
    GithubPullRequestReviewRequest, GithubPullRequestReviewResponse,
};
use wyrd_spec::ids::{CardName, CardUid, ExecutionId, IdempotencyKey, RepositoryId};
use wyrd_spec::verification::VerificationError;
use wyrd_spec::verification::VerificationExecutionStatus;
use wyrd_spec::verification::execution::{PullRequestActivation, PullRequestRef};
use wyrd_sql::queries::cards::{repository_family_name, selected_repository};
use wyrd_sql::queries::verifier_executions::{
    Admission, Admitted, PullRequestAdmission, PullRequestEnd, PullRequestExecution,
    PullRequestGeneration, PullRequestKey, PullRequestTarget, VerifierExecutionQueue,
};

use super::app::{GithubApp, PullRequestBody, unavailable};
use super::service::{GithubConnections, InstallationAccess};
use crate::audit;
use crate::components::auth::Caller;
use crate::scribe_outbox::{ScribeWrite, stage_cancelled};
use crate::state::registry_db_error;
use crate::verification::authority::{AuthorityRefusal, BackgroundAuthority};
use crate::verification::results::{InputRecord, ResultPayloadBuilder, ResultRun};
use crate::verification::submissions::{AdmissionReply, Queued};

/// Audit operation of a pull-request admission's `verifier:run` check.
const ADMIT: &str = "verification.pull_request.admit";

/// The audited operation of an explicit fresh-review request.
const REVIEW: &str = "github_connection.review_pull_request";

/// The audited resource of a review request for PR `number` of
/// `repository_id`.
fn review_resource(repository_id: &RepositoryId, number: u64) -> String {
    format!("pull_request:{repository_id}#{number}")
}

/// What asks for a PR observation to be decided.
#[derive(Clone, Copy)]
enum Admitting<'r> {
    /// A provider delivery with this id.
    Delivery(&'r str),
    /// An explicit fresh-review request by `caller` under `key`.
    Request {
        /// The authenticated requester.
        caller: &'r Caller,
        /// The request's idempotency key.
        key: &'r str,
    },
}

impl Admitting<'_> {
    /// The answer when the observation admits nothing for `reason`: no
    /// work for a delivery, [`WyrdError::GithubPullRequestNotReviewable`]
    /// for a request.
    ///
    /// # Errors
    /// Returns the refusal for a request.
    fn nothing<T>(self, reason: &str) -> Result<Option<T>, WyrdError> {
        match self {
            Self::Delivery(_) => Ok(None),
            Self::Request { .. } => Err(WyrdError::GithubPullRequestNotReviewable {
                message: format!("the pull request cannot be reviewed now: {reason}"),
                details: serde_json::json!({ "reason": reason }),
            }),
        }
    }
}

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
    /// Admit `event`, delivery `delivery_id` for `tenant` routed by its
    /// `installation`, into the Verifier queue, returning once every
    /// execution it owes was durably admitted.
    ///
    /// See the [module](self) for the steps. An event that cannot change the
    /// PR's source, a repository the connection no longer reaches, one
    /// without a selected Repository, and an obsolete observation owe no
    /// work and return at once. A binding whose accountable principal may
    /// not run its Verifier settles `errored` with the refusal and runs
    /// nothing.
    ///
    /// One tenant connection and the PR's lock are held across GitHub's one
    /// deadline-bounded PR read; other PRs take other locks, and no
    /// connection-slot or execution row is locked while GitHub answers.
    ///
    /// # Errors
    /// Returns [`WyrdError::GithubUnavailable`] when GitHub cannot report
    /// the PR, [`WyrdError::GithubConnectionConflict`]
    /// (`connection_changed`) when the tenant's connection changed while
    /// GitHub answered, [`WyrdError::ServiceUnavailable`] when the
    /// accountable authority cannot be decided now, this process queues no
    /// Verifier work, or an admission was lost before it committed, the
    /// staging refusal, or a registry error. Each leaves the delivery
    /// retryable.
    #[tracing::instrument(skip(self, app, event), fields(operation = "github_connection.pull_request", %tenant, number = event.number))]
    pub(crate) async fn pull_request(
        &self,
        app: &GithubApp,
        tenant: DataTenantId,
        installation: i64,
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
        let access = match self
            .installation_access(tenant, Some(installation), event.repository.id)
            .await
        {
            Ok(access) => access,
            Err(
                WyrdError::GithubRepositoryInaccessible { .. }
                | WyrdError::GithubNotConnected { .. },
            ) => {
                tracing::info!("the connection no longer reaches the repository; nothing admitted");
                return Ok(Vec::new());
            }
            Err(error) => return Err(error),
        };
        let admitted = self
            .admit(app, tenant, key, access, Admitting::Delivery(delivery_id))
            .await?;
        Ok(admitted
            .map(|(_, executions)| executions)
            .unwrap_or_default())
    }

    /// Admit a fresh review of one connected pull request by its selected
    /// Repository's whole `pull_request` binding set, as `caller` requested
    /// under `key`, and return the accepted generation and every member
    /// once each is durably admitted or settled.
    ///
    /// The caller needs `cards:read`, then `verifier:read` and
    /// `verifier:run` on every selected Verifier; members still run under
    /// the selecting declaration's accountable authority. A distinct key
    /// admits a new generation even at the PR's unchanged source and
    /// supersedes the older one. A retry of the same key for the same PR,
    /// concurrent or after a restart, returns the generation it admitted,
    /// even after the PR moved on, and never reactivates superseded work.
    ///
    /// # Errors
    /// Returns [`WyrdError::GithubRequestInvalid`] for another host or a
    /// non-numeric repository id, the permission denial,
    /// [`WyrdError::GithubPullRequestNotReviewable`] for a missing, closed
    /// or draft PR or a policy binding no pull requests, the connection
    /// errors of [`Self::installation_access`], and the retryable errors of
    /// [`Self::pull_request`]. None accepts any work.
    #[tracing::instrument(skip(self, caller, key), fields(operation = REVIEW, number = request.number))]
    pub(crate) async fn review_pull_request(
        &self,
        caller: &Caller,
        request: &GithubPullRequestReviewRequest,
        key: &IdempotencyKey,
    ) -> Result<GithubPullRequestReviewResponse, WyrdError> {
        let app = self.app()?;
        let host = app.settings().host();
        let invalid = |message: &str| WyrdError::GithubRequestInvalid {
            message: message.to_owned(),
            details: serde_json::json!({ "host": host }),
        };
        if request.host != host {
            return Err(invalid(
                "the request names a host this server is not connected to",
            ));
        }
        let repository = request
            .repository_id
            .as_str()
            .parse::<u64>()
            .map_err(|_| invalid("repository_id is not a GitHub repository id"))?;
        audit::authorize(
            self.state,
            caller,
            &Permission::card_read(),
            REVIEW,
            &review_resource(&request.repository_id, request.number),
        )?;
        let tenant = caller.data_tenant_id;
        let access = self.installation_access(tenant, None, repository).await?;
        let key_of = PullRequestKey {
            host: &host,
            repository_id: &request.repository_id,
            number: request.number,
        };
        let (generation, executions) = self
            .admit(
                app,
                tenant,
                key_of,
                access,
                Admitting::Request {
                    caller,
                    key: key.as_str(),
                },
            )
            .await?
            .ok_or_else(|| WyrdError::internal("an accepted review admitted no generation"))?;
        Ok(GithubPullRequestReviewResponse {
            generation,
            executions,
        })
    }

    /// Decide one observation of the PR `key` under its lock, as the
    /// [module](self) describes, for a provider delivery or an explicit
    /// review request, and hand the decided generation off.
    ///
    /// A delivery replays a current generation that already reviews the
    /// source and returns `None` when it owes no work. A request first
    /// replays the generation its key admitted, then never replays a
    /// source, and refuses, without changing state, every outcome that
    /// would admit nothing.
    ///
    /// # Errors
    /// The errors of [`Self::pull_request`] and
    /// [`Self::review_pull_request`].
    async fn admit(
        &self,
        app: &GithubApp,
        tenant: DataTenantId,
        key: PullRequestKey<'_>,
        access: InstallationAccess,
        admitting: Admitting<'_>,
    ) -> Result<Option<(Uuid, Vec<ExecutionId>)>, WyrdError> {
        let InstallationAccess { token, binding, .. } = access;
        let queue = VerifierExecutionQueue::default();
        let mut conn = self.conn(tenant).await?;
        queue
            .lock_pull_request(&mut conn, key)
            .await
            .map_err(registry_db_error)?;
        if let Admitting::Request {
            caller,
            key: request_key,
        } = admitting
            && let Some(generation) = queue
                .requested_pull_request(&mut conn, key, request_key)
                .await
                .map_err(registry_db_error)?
        {
            conn.commit().await.map_err(registry_db_error)?;
            for execution in &generation.executions {
                self.may_request(caller, &execution.verifier_card_uid, key)?;
            }
            tracing::info!("the review request replays the generation its key admitted");
            let executions = self.hand_off(tenant, &generation).await?;
            return Ok(Some((generation.activation.generation, executions)));
        }
        let observed_at = queue
            .observe_pull_request(&mut conn)
            .await
            .map_err(registry_db_error)?;
        let current = app
            .pull_request(
                &token,
                key.repository_id
                    .as_str()
                    .parse()
                    .map_err(|_| unavailable("the pull request key"))?,
                key.number,
            )
            .await?;
        drop(token);
        let Some(current) = current else {
            conn.commit().await.map_err(registry_db_error)?;
            tracing::info!("GitHub no longer shows the pull request; nothing admitted");
            return admitting.nothing("not_found");
        };
        let observed = Observed::of(current)?;
        Self::hold_binding(&mut conn, binding).await?;
        let history = queue
            .lock_pull_request(&mut conn, key)
            .await
            .map_err(registry_db_error)?;
        if history.obsolete(observed_at) {
            conn.commit().await.map_err(registry_db_error)?;
            tracing::info!("a newer observation of the pull request already decided it");
            return match admitting {
                Admitting::Delivery(_) => Ok(None),
                Admitting::Request { .. } => Err(WyrdError::ServiceUnavailable {
                    message: "a concurrent observation decided the pull request; retry the request"
                        .to_owned(),
                    details: serde_json::json!({}),
                }),
            };
        }
        let (base, head, head_repository_id) = match observed {
            Observed::Ended(end) => {
                if let Admitting::Request { .. } = admitting {
                    conn.commit().await.map_err(registry_db_error)?;
                    return admitting.nothing(match end {
                        PullRequestEnd::Draft => "draft",
                        PullRequestEnd::Closed | PullRequestEnd::Superseded => "closed",
                    });
                }
                let cancelled = queue
                    .deactivate_pull_request(&mut conn, key, observed_at, end)
                    .await
                    .map_err(registry_db_error)?;
                let occurred_at = chrono::Utc::now();
                conn.commit().await.map_err(registry_db_error)?;
                tracing::info!(
                    ?end,
                    cancelled = cancelled.len(),
                    "pull request deactivated"
                );
                stage_cancelled(
                    self.state.bifrost.scribe_outbox(),
                    tenant,
                    cancelled,
                    occurred_at,
                );
                return Ok(None);
            }
            Observed::Source(base, head, head_repository_id) => (base, head, head_repository_id),
        };
        let (delivery_id, request_key, requester) = match admitting {
            Admitting::Delivery(delivery_id) => (Some(delivery_id.to_owned()), None, None),
            Admitting::Request { caller, key } => (None, Some(key.to_owned()), Some(caller)),
        };
        let source = PullRequestActivation {
            host: key.host.to_owned(),
            repository_id: key.repository_id.clone(),
            number: key.number,
            base,
            head,
            head_repository_id,
            delivery_id,
            request_key,
            generation: Uuid::now_v7(),
            members: Vec::new(),
        };
        let mut fenced = Vec::new();
        let replay = requester.is_none()
            && history
                .current
                .as_ref()
                .is_some_and(|generation| !generation.deactivated && generation.reviews(&source));
        if !replay {
            let targets = match self.targets(&mut conn, key, requester).await? {
                Some(targets) => targets,
                None if requester.is_some() => {
                    conn.commit().await.map_err(registry_db_error)?;
                    return admitting.nothing("no_bindings");
                }
                None => {
                    let superseded = queue
                        .deactivate_pull_request(
                            &mut conn,
                            key,
                            observed_at,
                            PullRequestEnd::Superseded,
                        )
                        .await
                        .map_err(registry_db_error)?;
                    let occurred_at = chrono::Utc::now();
                    conn.commit().await.map_err(registry_db_error)?;
                    tracing::info!(
                        superseded = superseded.len(),
                        "no selected Repository binds this repository's pull requests"
                    );
                    stage_cancelled(
                        self.state.bifrost.scribe_outbox(),
                        tenant,
                        superseded,
                        occurred_at,
                    );
                    return Ok(None);
                }
            };
            let Admitted {
                outcome: (reserved, superseded),
                cancelled,
            } = queue
                .reserve_pull_request(&mut conn, &source, observed_at, &targets)
                .await
                .map_err(registry_db_error)?;
            tracing::info!(
                reserved = reserved.len(),
                superseded = superseded.len(),
                "pull request generation admitted"
            );
            fenced = superseded;
            fenced.extend(cancelled);
        }
        let generation = queue
            .lock_pull_request(&mut conn, key)
            .await
            .map_err(registry_db_error)?
            .current;
        let occurred_at = chrono::Utc::now();
        conn.commit().await.map_err(registry_db_error)?;
        stage_cancelled(
            self.state.bifrost.scribe_outbox(),
            tenant,
            fenced,
            occurred_at,
        );
        match generation {
            Some(generation) => {
                let executions = self.hand_off(tenant, &generation).await?;
                Ok(Some((generation.activation.generation, executions)))
            }
            None => Ok(None),
        }
    }

    /// Decide, and audit, that `caller` may request a review run by
    /// Verifier `verifier` of PR `key`: `verifier:read` then
    /// `verifier:run`.
    ///
    /// # Errors
    /// Returns [`WyrdError::PermissionDeniedRbac`] for either denial.
    fn may_request(
        &self,
        caller: &Caller,
        verifier: &CardUid,
        key: PullRequestKey<'_>,
    ) -> Result<(), WyrdError> {
        let resource = review_resource(key.repository_id, key.number);
        audit::authorize(
            self.state,
            caller,
            &Permission::verifier_read(verifier.clone()),
            REVIEW,
            &resource,
        )?;
        audit::authorize(
            self.state,
            caller,
            &Permission::verifier_run(verifier.clone()),
            REVIEW,
            &resource,
        )
    }

    /// The frozen target of every `pull_request` binding of the Repository
    /// selected for `repository_id`, each under the selecting declaration's
    /// accountable principal; `None` when no Repository is selected or it
    /// binds no pull requests.
    ///
    /// A `requester` of an explicit review must first hold
    /// [`Self::may_request`] for every binding's Verifier, or nothing is
    /// admitted. The principal's current authority is then resolved once
    /// and `verifier:run` is decided, and audited, for each binding's
    /// Verifier; a refusal becomes that binding's settled error.
    ///
    /// # Errors
    /// Returns the requester's permission denial,
    /// [`WyrdError::ServiceUnavailable`] when the authority cannot be
    /// decided now, or a registry error.
    async fn targets(
        &self,
        conn: &mut wyrd_sql::TenantConn<'_>,
        key: PullRequestKey<'_>,
        requester: Option<&Caller>,
    ) -> Result<Option<Vec<PullRequestTarget>>, WyrdError> {
        let Some(name) = repository_family_name(conn, key.repository_id.as_str()).await? else {
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
        if let Some(caller) = requester {
            for binding in &bindings {
                self.may_request(caller, &binding.verifier_card_uid, key)?;
            }
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

    /// Stage the input record of every still-reserved, refused or cancelled
    /// execution of `generation` and wait until the queue writer activated
    /// or durably retained each.
    ///
    /// A cancelled member may have been superseded before its handoff was
    /// retained, so a replay of its generation, such as a retried review
    /// request key, retains it then; its activation is a no-op. An active
    /// member was activated only after its record was retained, so it is
    /// not staged again.
    ///
    /// Each record is built from the generation's stored pins and
    /// observation time only, so a replay stages byte-identical rows under
    /// the same batch identities and Scribe keeps each once. A refused
    /// member also stages its `errored` status at the observation time; its
    /// activation is a no-op, so it never becomes runnable.
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
            .filter(|execution| {
                execution.reserved
                    || execution.refused
                    || execution.status == VerificationExecutionStatus::Cancelled
            })
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
            if execution.refused {
                scribe.stage(
                    tenant,
                    ScribeWrite::status(
                        execution.execution_id,
                        &execution.verifier_card_uid,
                        execution.status,
                        execution.attribution,
                        generation.observed_at,
                    ),
                );
            }
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
    ///
    /// # Panics
    /// Panics when `changes` does not decode as an event's changes.
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
