//! Human approval steps over HTTP: a pull request execution parks on a
//! pending occurrence, a person with `verifier:approve` decides it through
//! `POST /v1/verification/executions/{execution_id}/approvals/{approval_id}`,
//! and the same execution continues to its verdict.

use std::time::Duration;

use axum::http::StatusCode;
use serde_json::{Value, json};
use tokio_util::sync::CancellationToken;
use uuid::Uuid;
use wyrd_runtime::Permission;
use wyrd_server::state::LimitsConfig;
use wyrd_server::verification::VerificationRuntime;
use wyrd_spec::auth::{CardPermissionScope, PermissionScope, TokenAudience};
use wyrd_spec::envelope::CardKind;
use wyrd_spec::ids::{CardName, CardUid};
use wyrd_sql::queries::cards::card_scopes;
use wyrd_testing::{Bootstrap, WyrdTestServer};

use super::pg_deferred_evidence::limits;
use super::pg_github_pull_requests::{Deployment, REPOSITORY};

/// The first head a signoff pull request opens at.
const H1: &str = "a1a1a1a1a1a1a1a1a1a1a1a1a1a1a1a1a1a1a1a1";
/// A second, independent head.
const H2: &str = "a2a2a2a2a2a2a2a2a2a2a2a2a2a2a2a2a2a2a2a2";
/// The head that supersedes [`H2`].
const H3: &str = "a3a3a3a3a3a3a3a3a3a3a3a3a3a3a3a3a3a3a3a3";

/// The decide operation every approval permission check audits under.
const DECIDE: &str = "verification.approval.decide";

/// The `signoff` Verifier: it passes when the deferred `ci` evidence reports
/// green and a person approves the `signoff` step. The approval does not
/// depend on `ci`, so each progresses independently.
const SIGNOFF: &str = "apiVersion: wyrd/v1\nkind: Verifier\nmetadata:\n  name: signoff\n  version: 1.0.0\nspec:\n  description: CI is green and a person signs off.\n  inputs:\n    ci: {type: json, deferred: true, schema: {type: object, required: [status]}}\n  steps:\n    - id: signoff\n      action: {kind: approval, message: Ship this change?}\n  requirements:\n    - id: green\n      statement: CI is green\n      check: {kind: assert, path: inputs.ci.status, operator: equals, expected: green}\n    - id: approved\n      statement: A person approved the change\n      check: {kind: assert, path: steps.signoff.output.status, operator: equals, expected: approved}\n";

/// The Repository binding `signoff` to pull requests.
const POLICY: &str = "apiVersion: wyrd/v1\nkind: Repository\nmetadata:\n  name: policy\n  version: 1.1.0\nspec:\n  repository_id: \"4242\"\n  verified_by:\n    - verifier: ./signoff.yaml\n      runs_on: {kind: pull_request}\n";

impl Deployment {
    /// Declare the `signoff` policy.
    ///
    /// # Panics
    /// Panics when the declaration is refused.
    async fn signoff(&self) {
        self.declare_files(
            &[("signoff.yaml", SIGNOFF.to_owned())],
            POLICY,
            "signoff-policy",
        )
        .await;
    }

    /// Start a verification runtime over this server, returning the token
    /// that stops it and its task.
    ///
    /// # Panics
    /// Panics when the runtime does not compose.
    fn runtime(&self) -> (CancellationToken, tokio::task::JoinHandle<()>) {
        let stop = CancellationToken::new();
        let task = tokio::spawn(
            VerificationRuntime::builder(self.server.state())
                .limits(limits(Duration::from_mins(5)))
                .build()
                .expect("the runtime composes")
                .run(stop.clone()),
        );
        (stop, task)
    }

    /// Decide `approval` of `execution` as the user holding `jwt` and return
    /// the status and JSON answer.
    ///
    /// # Panics
    /// Panics when the request cannot be served.
    async fn decide(
        &self,
        jwt: &str,
        execution: Uuid,
        approval: &str,
        body: &Value,
    ) -> (StatusCode, Value) {
        self.post(
            jwt,
            &format!("/v1/verification/executions/{execution}/approvals/{approval}"),
            body,
        )
        .await
    }

    /// Wait until `execution` waits for nothing but its approvals and return
    /// it.
    ///
    /// # Panics
    /// Panics when it does not within two minutes.
    async fn awaiting_approval(&self, execution: Uuid) -> Value {
        let deadline = tokio::time::Instant::now() + Duration::from_mins(2);
        loop {
            let served = self.status(execution).await;
            if served["status"] == "waiting" && served["waiting"].is_null() {
                return served;
            }
            assert!(
                tokio::time::Instant::now() < deadline,
                "{execution} still waits for evidence: {served}"
            );
            tokio::time::sleep(Duration::from_millis(100)).await;
        }
    }

    /// The exact scope of the Verifier Card `execution` froze.
    ///
    /// # Panics
    /// Panics when the read fails or the Verifier is not registered.
    async fn verifier_of(&self, execution: Uuid) -> CardPermissionScope {
        let mut conn = self.conn().await;
        let (uid,): (Uuid,) = sqlx::query_as(
            "SELECT verifier_card_uid FROM wyrd.verifier_executions WHERE execution_id = $1",
        )
        .bind(execution)
        .fetch_one(&mut **conn.transaction())
        .await
        .expect("execution reads");
        let uid = CardUid::from_uuid(uid).expect("card uid");
        let mut scopes = card_scopes(&mut conn, std::slice::from_ref(&uid))
            .await
            .expect("scope reads");
        conn.commit().await.expect("read commits");
        scopes.remove(&uid).expect("the Verifier is registered")
    }

    /// A user of this tenant holding `roles`, as its id and JWT.
    ///
    /// # Panics
    /// Panics when the bootstrap fails.
    async fn user(&self, name: &str, roles: &[&str]) -> (Uuid, String) {
        self.user_at(name, roles, None).await
    }

    /// A user of this tenant holding `roles`, bound at `scope` when given and
    /// tenant-wide otherwise, as its id and JWT.
    ///
    /// # Panics
    /// Panics when the bootstrap or binding fails.
    async fn user_at(
        &self,
        name: &str,
        roles: &[&str],
        scope: Option<&PermissionScope>,
    ) -> (Uuid, String) {
        let Some(scope) = scope else {
            return self.bootstrapped(name, roles).await;
        };
        let (id, jwt) = self.bootstrapped(name, &[]).await;
        let principal = Bootstrap::User {
            id: wyrd_spec::auth::PrincipalId::new(id),
            jwt: jwt.clone(),
        };
        for role in roles {
            self.server
                .grant_role_at(&principal, role, scope)
                .await
                .expect("scoped role binds");
        }
        (id, jwt)
    }

    /// A bootstrapped user holding tenant-wide `roles`.
    ///
    /// # Panics
    /// Panics when the bootstrap fails.
    async fn bootstrapped(&self, name: &str, roles: &[&str]) -> (Uuid, String) {
        let Bootstrap::User { id, jwt } = self
            .server
            .bootstrap_user(name, roles)
            .await
            .expect("user bootstraps")
        else {
            panic!("user bootstrap returned a non-user principal");
        };
        (id.as_uuid(), jwt)
    }

    /// Wait until `execution` serves its `step` approval occurrence as
    /// `status`, and return the execution.
    ///
    /// # Panics
    /// Panics when it is not within two minutes, longer than a parked
    /// execution's one-minute recheck.
    async fn until_approval(&self, execution: Uuid, step: &str, status: &str) -> Value {
        let deadline = tokio::time::Instant::now() + Duration::from_mins(2);
        loop {
            let served = self.status(execution).await;
            let listed = served["approvals"].as_array().is_some_and(|approvals| {
                approvals
                    .iter()
                    .any(|approval| approval["step_id"] == step && approval["status"] == status)
            });
            if listed {
                return served;
            }
            assert!(
                tokio::time::Instant::now() < deadline,
                "{execution} does not serve {step} as {status}: {served}"
            );
            tokio::time::sleep(Duration::from_millis(100)).await;
        }
    }

    /// The audited outcomes of `principal`'s `verifier:approve` checks.
    ///
    /// # Panics
    /// Panics when the audit does not settle or the read fails.
    async fn decide_audits(&self, principal: Uuid) -> Vec<String> {
        self.server
            .await_audit_retained()
            .await
            .expect("audit is retained");
        let mut outcomes: Vec<String> = self
            .server
            .retained_audit_records(
                self.server.data_tenant_id(),
                "outcome",
                &format!(
                    "operation = '{DECIDE}' AND permission = 'verifier:approve' AND sub_principal_id = '{principal}'"
                ),
            )
            .await
            .expect("retained audit reads")
            .into_iter()
            .flatten()
            .flatten()
            .collect();
        outcomes.sort();
        outcomes
    }
}

/// The one approval occurrence `served` lists.
///
/// # Panics
/// Panics when it lists other than one.
fn occurrence(served: &Value) -> &Value {
    let approvals = served["approvals"].as_array().expect("approvals listed");
    assert_eq!(approvals.len(), 1, "{served}");
    &approvals[0]
}

/// The status `served` lists for its `step` approval occurrence.
///
/// # Panics
/// Panics when it lists no occurrence of `step`.
fn occurrence_status<'a>(served: &'a Value, step: &str) -> &'a Value {
    &served["approvals"]
        .as_array()
        .expect("approvals listed")
        .iter()
        .find(|approval| approval["step_id"] == step)
        .unwrap_or_else(|| panic!("{step} is listed: {served}"))["status"]
}

/// A pending approval is decided once, by an authorized person, and the
/// same execution continues to the verdict that decision implies.
///
/// The pull request's execution waits for `ci` and a pending `signoff`
/// occurrence. Publishing `ci` resumes it, but it still waits for the
/// approval alone. A viewer, a person allowed to approve only another
/// Verifier, and another tenant cannot decide; an unknown occurrence is not
/// found. An approver approves with a reason and receives the durable receipt;
/// the same request replays it, while a different decision or decider is
/// refused as not pending. The execution passes under its original id and
/// inspection shows who approved. A second pull request is rejected by a
/// person granted only `verifier:approve` on exactly this Verifier, and
/// fails.
/// Every decide check is audited.
///
/// # Panics
/// Panics when a decision, refusal, receipt, verdict or audit differs.
#[tokio::test(flavor = "multi_thread")]
async fn a_pending_approval_is_decided_once_and_resumes_the_same_execution() {
    let deployment = Deployment::start_limited(limits(Duration::from_mins(5))).await;
    deployment.signoff().await;
    let execution = deployment.open(1, H1).await;
    let served = deployment.until(execution, "waiting").await;
    assert_eq!(served["waiting"], json!(["ci"]), "{served}");
    let pending = occurrence(&served);
    assert_eq!(pending["step_id"], "signoff");
    assert_eq!(pending["message"], "Ship this change?");
    assert_eq!(pending["status"], "pending");
    assert!(pending["actor_id"].is_null(), "{pending}");
    let approval = pending["approval_id"]
        .as_str()
        .expect("approval id")
        .to_owned();

    let (status, receipt) = deployment.publish_ci(H1, json!({"status": "green"})).await;
    assert_eq!(status, StatusCode::OK, "{receipt}");
    let served = deployment.awaiting_approval(execution).await;
    assert_eq!(
        occurrence(&served)["approval_id"],
        approval.as_str(),
        "the occurrence keeps its id across resumptions"
    );

    let approve = json!({"decision": "approved", "reason": "reviewed the diff"});
    let (viewer, viewer_jwt) = deployment.user("approval-viewer", &["viewer"]).await;
    let (status, denied) = deployment
        .decide(&viewer_jwt, execution, &approval, &approve)
        .await;
    assert_eq!(status, StatusCode::FORBIDDEN, "{denied}");
    assert_eq!(denied["code"], "WYRD_PERMISSION_403_DENIED_RBAC");

    let verifier = deployment.verifier_of(execution).await;
    let other = PermissionScope::Card(CardPermissionScope::family(
        CardKind::Verifier,
        CardName::new("other-signoff").expect("name"),
    ));
    deployment
        .server
        .seed_role(
            "other_approver",
            &[
                Permission::executions_read_any(),
                Permission::verifier_approve_any(),
            ],
        )
        .await
        .expect("role seeds");
    let (_, misscoped) = deployment
        .user_at("approval-misscoped", &["other_approver"], Some(&other))
        .await;
    let (status, denied) = deployment
        .decide(&misscoped, execution, &approval, &approve)
        .await;
    assert_eq!(
        status,
        StatusCode::FORBIDDEN,
        "approving another Verifier does not cover this one: {denied}"
    );

    let foreign_tenant = deployment
        .server
        .seed_tenant("approval-other")
        .await
        .expect("second tenant seeds");
    let Bootstrap::Machine { api_key, .. } = deployment
        .server
        .bootstrap_service_in_tenant(foreign_tenant, "approval-other-admin", &["admin"])
        .await
        .expect("other tenant's admin bootstraps")
    else {
        panic!("service bootstrap returned a non-machine principal");
    };
    let foreign = deployment
        .server
        .exchange_api_key(&api_key)
        .await
        .expect("other tenant's key exchanges");
    let (status, refused) = deployment
        .decide(&foreign, execution, &approval, &approve)
        .await;
    assert_eq!(status, StatusCode::NOT_FOUND, "{refused}");

    let (status, refused) = deployment
        .decide(
            &deployment.jwt,
            execution,
            &Uuid::now_v7().to_string(),
            &approve,
        )
        .await;
    assert_eq!(status, StatusCode::NOT_FOUND, "{refused}");
    assert_eq!(refused["code"], "WYRD_VERIFICATION_404_APPROVAL_NOT_FOUND");

    let (approver, approver_jwt) = deployment
        .user("approval-editor", &["approver", "viewer"])
        .await;
    let (status, receipt) = deployment
        .decide(&approver_jwt, execution, &approval, &approve)
        .await;
    assert_eq!(status, StatusCode::OK, "{receipt}");
    assert_eq!(receipt["approval_id"], approval.as_str());
    assert_eq!(receipt["status"], "approved");
    assert_eq!(receipt["actor_id"], approver.to_string());
    assert_eq!(receipt["reason"], "reviewed the diff");
    assert!(receipt["decided_at"].is_string(), "{receipt}");

    let (status, replayed) = deployment
        .decide(&approver_jwt, execution, &approval, &approve)
        .await;
    assert_eq!(status, StatusCode::OK, "{replayed}");
    assert_eq!(replayed, receipt, "a retry replays the durable receipt");
    for (jwt, body) in [
        (
            approver_jwt.as_str(),
            json!({"decision": "rejected", "reason": "reviewed the diff"}),
        ),
        (approver_jwt.as_str(), json!({"decision": "approved"})),
        (deployment.jwt.as_str(), approve.clone()),
    ] {
        let (status, refused) = deployment.decide(jwt, execution, &approval, &body).await;
        assert_eq!(status, StatusCode::CONFLICT, "{body}: {refused}");
        assert_eq!(
            refused["code"],
            "WYRD_VERIFICATION_409_APPROVAL_NOT_PENDING"
        );
        assert_eq!(refused["details"]["status"], "approved");
    }

    let served = deployment.until(execution, "completed").await;
    assert_eq!(served["execution_id"], execution.to_string());
    assert_eq!(served["result"]["outcome"]["verdict"], "passed", "{served}");
    let decided = occurrence(&served);
    assert_eq!(decided["status"], "approved");
    assert_eq!(decided["actor_id"], approver.to_string());
    assert_eq!(decided["reason"], "reviewed the diff");
    assert_eq!(decided["decided_at"], receipt["decided_at"]);

    deployment
        .server
        .seed_role("signoff_approver", &[Permission::verifier_approve_any()])
        .await
        .expect("role seeds");
    let (scoped, scoped_jwt) = deployment
        .user_at(
            "approval-scoped",
            &["signoff_approver"],
            Some(&PermissionScope::Card(verifier)),
        )
        .await;
    deployment
        .server
        .grant_role_at(
            &Bootstrap::User {
                id: wyrd_spec::auth::PrincipalId::new(scoped),
                jwt: scoped_jwt.clone(),
            },
            "viewer",
            &PermissionScope::All,
        )
        .await
        .expect("the scoped approver reads the parent");
    let rejected = deployment.open(2, H2).await;
    deployment.until(rejected, "waiting").await;
    let (status, receipt) = deployment.publish_ci(H2, json!({"status": "green"})).await;
    assert_eq!(status, StatusCode::OK, "{receipt}");
    let served = deployment.awaiting_approval(rejected).await;
    let approval = occurrence(&served)["approval_id"]
        .as_str()
        .expect("approval id")
        .to_owned();
    let (status, receipt) = deployment
        .decide(
            &scoped_jwt,
            rejected,
            &approval,
            &json!({"decision": "rejected"}),
        )
        .await;
    assert_eq!(status, StatusCode::OK, "{receipt}");
    assert_eq!(receipt["status"], "rejected");
    assert!(receipt["reason"].is_null(), "{receipt}");
    let served = deployment.until(rejected, "completed").await;
    assert_eq!(
        served["result"]["outcome"]["verdict"], "failed",
        "a rejection is a verdict, not a crash: {served}"
    );
    assert_eq!(occurrence(&served)["status"], "rejected");

    assert_eq!(deployment.decide_audits(viewer).await, ["denied"]);
    assert_eq!(
        deployment.decide_audits(approver).await,
        ["allowed"; 4],
        "the decision, its replay and both conflicts are each checked"
    );
    assert_eq!(deployment.decide_audits(scoped).await, ["allowed"]);
}

/// An approval expires with its execution and can no longer be decided.
///
/// A superseded head's execution is cancelled while its approval is
/// pending; the next head's execution reaches its original deadline still
/// waiting. Both list the occurrence as expired with no actor, and a
/// decision of either is refused as not pending.
///
/// # Panics
/// Panics when an ended execution's approval is pending or accepts a
/// decision.
#[tokio::test(flavor = "multi_thread")]
async fn an_approval_expires_with_its_execution() {
    let deployment = Deployment::start_limited(limits(Duration::from_secs(8))).await;
    deployment.signoff().await;
    let superseded = deployment.open(3, H2).await;
    let served = deployment.until(superseded, "waiting").await;
    let first = occurrence(&served)["approval_id"]
        .as_str()
        .expect("approval id")
        .to_owned();
    deployment
        .mock
        .edit_pull_request(REPOSITORY, 3, |pr| H3.clone_into(&mut pr.head_sha));
    assert_eq!(
        deployment.send(3, "synchronize").await.0,
        StatusCode::ACCEPTED
    );
    deployment.until(superseded, "cancelled").await;
    let current = deployment
        .rows(3)
        .await
        .into_iter()
        .find(|row| row.head == H3)
        .expect("the new head is admitted")
        .execution_id;
    let served = deployment.until(current, "waiting").await;
    let second = occurrence(&served)["approval_id"]
        .as_str()
        .expect("approval id")
        .to_owned();
    assert_ne!(first, second, "each execution has its own occurrence");
    deployment.until(current, "timed_out").await;

    let (_, approver) = deployment
        .user("approval-late", &["approver", "viewer"])
        .await;
    for (execution, approval) in [(superseded, &first), (current, &second)] {
        let served = deployment.status(execution).await;
        let expired = occurrence(&served);
        assert_eq!(expired["status"], "expired", "{served}");
        assert!(expired["actor_id"].is_null(), "{served}");
        let (status, refused) = deployment
            .decide(
                &approver,
                execution,
                approval,
                &json!({"decision": "approved"}),
            )
            .await;
        assert_eq!(status, StatusCode::CONFLICT, "{refused}");
        assert_eq!(
            refused["code"],
            "WYRD_VERIFICATION_409_APPROVAL_NOT_PENDING"
        );
        assert_eq!(refused["details"]["status"], "expired");
    }
}

/// A decision accepted while no runtime runs survives a restart, and the
/// same execution finishes.
///
/// # Panics
/// Panics when the decision is refused or the restarted runtime does not
/// pass the same execution.
#[tokio::test(flavor = "multi_thread")]
async fn a_decision_made_while_stopped_finishes_after_a_restart() {
    let deployment = Deployment::start_with(false).await;
    deployment.signoff().await;

    let (stop, task) = deployment.runtime();
    let waiting = deployment.open(5, H1).await;
    let served = deployment.until(waiting, "waiting").await;
    let approval = occurrence(&served)["approval_id"]
        .as_str()
        .expect("approval id")
        .to_owned();
    stop.cancel();
    task.await.expect("the runtime task does not panic");

    let (status, receipt) = deployment.publish_ci(H1, json!({"status": "green"})).await;
    assert_eq!(status, StatusCode::OK, "{receipt}");
    let (status, receipt) = deployment
        .decide(
            &deployment.jwt,
            waiting,
            &approval,
            &json!({"decision": "approved"}),
        )
        .await;
    assert_eq!(status, StatusCode::OK, "{receipt}");

    let (stop, task) = deployment.runtime();
    let served = deployment.until(waiting, "completed").await;
    assert_eq!(served["result"]["outcome"]["verdict"], "passed", "{served}");
    stop.cancel();
    task.await.expect("the runtime task does not panic");
}

/// A decision accepted before its head is superseded, but not yet consumed,
/// never releases the superseded execution.
///
/// While no runtime runs, CI reports green and a person approves the parked
/// execution, so it would pass if it resumed; then a new head supersedes it.
/// After a restart the approved execution is cancelled, never completed, and
/// keeps the accepted receipt as its record.
///
/// # Panics
/// Panics when the decision is refused or the superseded execution passes.
#[tokio::test(flavor = "multi_thread")]
async fn a_decision_racing_supersession_releases_nothing() {
    let deployment = Deployment::start_with(false).await;
    deployment.signoff().await;

    let (stop, task) = deployment.runtime();
    let waiting = deployment.open(6, H2).await;
    let served = deployment.until(waiting, "waiting").await;
    let approval = occurrence(&served)["approval_id"]
        .as_str()
        .expect("approval id")
        .to_owned();
    stop.cancel();
    task.await.expect("the runtime task does not panic");

    let (status, receipt) = deployment.publish_ci(H2, json!({"status": "green"})).await;
    assert_eq!(status, StatusCode::OK, "{receipt}");
    let (status, receipt) = deployment
        .decide(
            &deployment.jwt,
            waiting,
            &approval,
            &json!({"decision": "approved"}),
        )
        .await;
    assert_eq!(status, StatusCode::OK, "{receipt}");
    deployment
        .mock
        .edit_pull_request(REPOSITORY, 6, |pr| H3.clone_into(&mut pr.head_sha));
    assert_eq!(
        deployment.send(6, "synchronize").await.0,
        StatusCode::ACCEPTED
    );

    let (stop, task) = deployment.runtime();
    let served = deployment.until(waiting, "cancelled").await;
    assert!(
        served["result"]["outcome"]["verdict"] != "passed",
        "{served}"
    );
    assert_eq!(occurrence(&served)["status"], "approved", "{served}");
    stop.cancel();
    task.await.expect("the runtime task does not panic");
}

/// No receipt is given before Scribe acknowledges the decision: an
/// unacknowledged decision answers unavailable and the execution keeps
/// waiting. Once the held write lands, the same decision replays the
/// accepted receipt, makes the execution due, and it passes. CI evidence is
/// published before the pull request opens, so only the approval holds it.
///
/// # Panics
/// Panics when a receipt precedes the acknowledgement, or the retry does not
/// replay and resume.
#[tokio::test(flavor = "multi_thread")]
async fn an_unacknowledged_decision_is_unavailable_and_its_retry_replays() {
    let deployment = Deployment::launch(|builder| {
        builder
            .with_verification_runtime_for_test()
            .with_verification_runtime_limits_for_test(limits(Duration::from_mins(5)))
            .with_limits_for_test(LimitsConfig {
                timeout: Duration::from_secs(4),
                ..LimitsConfig::default()
            })
    })
    .await;
    deployment.signoff().await;
    let (status, receipt) = deployment.publish_ci(H1, json!({"status": "green"})).await;
    assert_eq!(status, StatusCode::OK, "{receipt}");
    let waiting = deployment.open(7, H1).await;
    let served = deployment.until(waiting, "waiting").await;
    let approval = occurrence(&served)["approval_id"]
        .as_str()
        .expect("approval id")
        .to_owned();

    let stall = deployment
        .server
        .stall_next_bifrost_write_into("bifrost.verification.approvals")
        .expect("Scribe stalls");
    let decision = json!({"decision": "approved", "reason": "ship it"});
    let (status, unavailable) = deployment
        .decide(&deployment.jwt, waiting, &approval, &decision)
        .await;
    stall.wait_entered().await;
    assert_eq!(status, StatusCode::SERVICE_UNAVAILABLE, "{unavailable}");
    assert_eq!(
        unavailable["code"],
        "WYRD_VALA_503_BIFROST_WRITER_UNAVAILABLE"
    );
    assert_eq!(deployment.status(waiting).await["status"], "waiting");

    stall.release();
    stall.wait_completed().await;
    let (status, replay) = deployment
        .decide(&deployment.jwt, waiting, &approval, &decision)
        .await;
    assert_eq!(status, StatusCode::OK, "{replay}");
    assert_eq!(replay["status"], "approved", "{replay}");
    assert_eq!(replay["reason"], "ship it", "{replay}");
    let served = deployment.until(waiting, "completed").await;
    assert_eq!(served["result"]["outcome"]["verdict"], "passed", "{served}");
}

/// A decision is refused once the Verifier's own shorter timeout passes,
/// even while no runtime has ended the execution yet.
///
/// The Verifier authors an 8 second timeout under a 5 minute runtime
/// ceiling. A decision soon after admission is accepted. With the runtime
/// stopped, a second execution's approval is decided after its authored
/// timeout and refused as expired; the restarted runtime times it out.
///
/// # Panics
/// Panics when the early decision is refused, the late one is accepted, or
/// the late execution does not time out.
#[tokio::test(flavor = "multi_thread")]
async fn a_decision_after_the_authored_timeout_is_refused() {
    const AUTHORED: Duration = Duration::from_secs(8);
    let deployment = Deployment::start_with(false).await;
    deployment
        .declare_files(
            &[(
                "signoff.yaml",
                SIGNOFF.replace("spec:\n", "spec:\n  limits: {timeout_seconds: 8}\n"),
            )],
            POLICY,
            "brief-signoff-policy",
        )
        .await;
    let approve = json!({"decision": "approved"});
    let pending = |served: &Value| {
        occurrence(served)["approval_id"]
            .as_str()
            .expect("approval id")
            .to_owned()
    };

    let (stop, task) = deployment.runtime();
    let early = deployment.open(10, H1).await;
    let served = deployment.until(early, "waiting").await;
    let (status, receipt) = deployment
        .decide(&deployment.jwt, early, &pending(&served), &approve)
        .await;
    assert_eq!(status, StatusCode::OK, "{receipt}");
    let admitted = tokio::time::Instant::now();
    let late = deployment.open(11, H2).await;
    let served = deployment.until(late, "waiting").await;
    let approval = pending(&served);
    stop.cancel();
    task.await.expect("the runtime task does not panic");

    tokio::time::sleep_until(admitted + AUTHORED + Duration::from_secs(1)).await;
    let (status, refused) = deployment
        .decide(&deployment.jwt, late, &approval, &approve)
        .await;
    assert_eq!(status, StatusCode::CONFLICT, "{refused}");
    assert_eq!(
        refused["code"],
        "WYRD_VERIFICATION_409_APPROVAL_NOT_PENDING"
    );
    assert_eq!(refused["details"]["status"], "expired");

    let (stop, task) = deployment.runtime();
    let served = deployment.until(late, "timed_out").await;
    assert!(occurrence(&served)["actor_id"].is_null(), "{served}");
    stop.cancel();
    task.await.expect("the runtime task does not panic");
}

/// The `gated` Verifier: a person first says whether the change is needed,
/// and the `signoff` approval runs speculatively before that answer, kept
/// only when it is approved. Deferred `ci` keeps the execution open.
const GATED: &str = "apiVersion: wyrd/v1\nkind: Verifier\nmetadata:\n  name: signoff\n  version: 1.0.0\nspec:\n  description: A needed change is signed off once CI is green.\n  inputs:\n    ci: {type: json, deferred: true, schema: {type: object, required: [status]}}\n  steps:\n    - id: gate\n      action: {kind: approval, message: Is this change needed?}\n    - id: signoff\n      speculative: true\n      when: {path: steps.gate.output.status, operator: equals, expected: approved}\n      action: {kind: approval, message: Ship this change?}\n  requirements:\n    - id: green\n      statement: CI is green\n      check: {kind: assert, path: inputs.ci.status, operator: equals, expected: green}\n";

/// An approval its speculative guard discarded stops being pending and
/// refuses a first decision, while the execution keeps waiting for its
/// other input.
///
/// Both approvals are pending at first. Rejecting `gate` discards the
/// speculative `signoff`: it stays listed, now expired with no actor, and
/// approving it is refused as not pending. The execution still waits for
/// `ci` under the same id.
///
/// # Panics
/// Panics when the discarded approval stays pending or accepts a decision,
/// or the execution stops waiting.
#[tokio::test(flavor = "multi_thread")]
async fn a_discarded_speculative_approval_refuses_a_decision() {
    let deployment = Deployment::start_limited(limits(Duration::from_mins(5))).await;
    deployment
        .declare_files(
            &[("signoff.yaml", GATED.to_owned())],
            POLICY,
            "gated-policy",
        )
        .await;
    let waiting = deployment.open(12, H1).await;
    let served = deployment.until(waiting, "waiting").await;
    let id = |served: &Value, step: &str| {
        served["approvals"]
            .as_array()
            .expect("approvals listed")
            .iter()
            .find(|approval| approval["step_id"] == step)
            .unwrap_or_else(|| panic!("{step} is listed: {served}"))["approval_id"]
            .as_str()
            .expect("approval id")
            .to_owned()
    };
    let (gate, signoff) = (id(&served, "gate"), id(&served, "signoff"));

    let (status, receipt) = deployment
        .decide(
            &deployment.jwt,
            waiting,
            &gate,
            &json!({"decision": "rejected"}),
        )
        .await;
    assert_eq!(status, StatusCode::OK, "{receipt}");
    deployment
        .until_approval(waiting, "signoff", "expired")
        .await;
    let served = deployment.until(waiting, "waiting").await;
    assert_eq!(occurrence_status(&served, "signoff"), "expired", "{served}");
    assert_eq!(served["waiting"], json!(["ci"]), "{served}");

    let (status, refused) = deployment
        .decide(
            &deployment.jwt,
            waiting,
            &signoff,
            &json!({"decision": "approved"}),
        )
        .await;
    assert_eq!(status, StatusCode::CONFLICT, "{refused}");
    assert_eq!(
        refused["code"],
        "WYRD_VERIFICATION_409_APPROVAL_NOT_PENDING"
    );
    assert_eq!(refused["details"]["status"], "expired");
    let served = deployment.status(waiting).await;
    assert_eq!(served["status"], "waiting", "{served}");
}

/// A decision of an execution whose exact target no longer resolves is
/// refused as a missing target, never accepted under the wider runtime
/// ceiling.
///
/// The execution parks on its approval and the runtime stops. The editor
/// deletes the Repository Card it verifies; deciding the still-listed
/// occurrence is
/// then refused with the target-not-found error, and the occurrence stays
/// undecided.
///
/// # Panics
/// Panics when the Repository is not deleted, the decision is accepted, or
/// the occurrence gains a decision.
#[tokio::test(flavor = "multi_thread")]
async fn a_decision_whose_target_no_longer_resolves_is_refused() {
    let deployment = Deployment::start_with(false).await;
    deployment.signoff().await;
    let (stop, task) = deployment.runtime();
    let waiting = deployment.open(13, H1).await;
    let served = deployment.until(waiting, "waiting").await;
    let approval = occurrence(&served)["approval_id"]
        .as_str()
        .expect("approval id")
        .to_owned();
    stop.cancel();
    task.await.expect("the runtime task does not panic");

    let mut conn = deployment.conn().await;
    let (subject,): (Uuid,) =
        sqlx::query_as("SELECT card_uid FROM wyrd.verifier_executions WHERE execution_id = $1")
            .bind(waiting)
            .fetch_one(&mut **conn.transaction())
            .await
            .expect("execution reads");
    conn.commit().await.expect("read commits");
    let request = axum::http::Request::builder()
        .method(axum::http::Method::DELETE)
        .uri(format!("/v1/cards/by-uid/Repository/{subject}"))
        .body(axum::body::Body::empty())
        .expect("delete request builds");
    let response = deployment
        .server
        .oneshot_authenticated(&deployment.jwt, request)
        .await
        .expect("delete responds");
    assert!(response.status().is_success(), "{}", response.status());

    let (status, refused) = deployment
        .decide(
            &deployment.jwt,
            waiting,
            &approval,
            &json!({"decision": "approved"}),
        )
        .await;
    assert_eq!(status, StatusCode::NOT_FOUND, "{refused}");
    assert_eq!(refused["code"], "WYRD_VERIFICATION_404_TARGET_NOT_FOUND");
    let served = deployment.status(waiting).await;
    assert!(occurrence(&served)["actor_id"].is_null(), "{served}");
    assert!(occurrence(&served)["decided_at"].is_null(), "{served}");
}

/// A decision whose target is not cached needs only the one connection it
/// already holds.
///
/// The execution parks on its approval and the server restarts, so its
/// target cache is empty and the decision resolves the exact Verifier from
/// Postgres. The test holds every app-pool connection but one; the decision
/// is still accepted on that one.
///
/// # Panics
/// Panics when the pool cannot be narrowed or the decision is not accepted.
#[tokio::test(flavor = "multi_thread")]
async fn a_cold_decision_needs_only_its_own_connection() {
    let deployment = Deployment::start_with(false).await;
    deployment.signoff().await;
    let (stop, task) = deployment.runtime();
    let waiting = deployment.open(14, H1).await;
    let served = deployment.until(waiting, "waiting").await;
    let approval = occurrence(&served)["approval_id"]
        .as_str()
        .expect("approval id")
        .to_owned();
    stop.cancel();
    task.await.expect("the runtime task does not panic");
    let Deployment { mock, server, .. } = deployment;
    let server =
        Box::pin(server.restart_bound(WyrdTestServer::builder().with_github_app_for_test(&mock)))
            .await
            .expect("server restarts");
    let Bootstrap::User { jwt, .. } = server
        .bootstrap_user("pull-request-editor-again", &["approver", "viewer"])
        .await
        .expect("editor bootstraps")
    else {
        panic!("user bootstrap returned a non-user principal");
    };
    let deployment = Deployment { mock, server, jwt };

    let state = deployment.server.state();
    let pool = state.postgres.wyrd().app_pool();
    let max = pool.options().get_max_connections();
    let mut held = Vec::new();
    while max - pool.size() + u32::try_from(pool.num_idle()).expect("idle fits") > 1 {
        held.push(pool.acquire().await.expect("a connection is held"));
    }
    let (status, receipt) = deployment
        .decide(
            &deployment.jwt,
            waiting,
            &approval,
            &json!({"decision": "approved"}),
        )
        .await;
    drop(held);
    assert_eq!(status, StatusCode::OK, "{receipt}");
    assert_eq!(receipt["status"], "approved", "{receipt}");
}

/// A service account deciding on a user's behalf is the decision's actor.
///
/// A user delegates to an approving Service rooted at its own Card, whose Card
/// scope does not carry the Repository subject; the delegated token approves,
/// and the receipt and inspection name the Service as the actor. The same
/// delegated request replays the receipt, while the user directly and a second
/// Service acting for the same user are refused as not pending.
///
/// # Panics
/// Panics when a decision, refusal, receipt or inspection differs.
#[tokio::test(flavor = "multi_thread")]
async fn a_service_account_decides_on_a_users_behalf() {
    let deployment = Deployment::start_limited(limits(Duration::from_mins(5))).await;
    deployment.signoff().await;
    let execution = deployment.open(1, H1).await;
    let served = deployment.until(execution, "waiting").await;
    let approval = occurrence(&served)["approval_id"]
        .as_str()
        .expect("approval id")
        .to_owned();

    let (_, user_jwt) = deployment
        .user("approval-delegator", &["approver", "viewer"])
        .await;
    let on_behalf = async |name: &str| {
        let actor = deployment
            .server
            .bootstrap_service(name, &["approver", "viewer"])
            .await
            .expect("actor Service bootstraps");
        let actor_jwt = deployment
            .server
            .exchange_rooted(&actor)
            .await
            .expect("actor exchanges at its own Card");
        let delegated = deployment
            .server
            .delegate(&user_jwt, &actor_jwt, TokenAudience::Wyrd)
            .await
            .expect("the Service acts for the user");
        (actor.id().as_uuid(), delegated)
    };
    let (actor, delegated) = on_behalf("approval-actor").await;
    let approve = json!({"decision": "approved", "reason": "bot verified"});
    let (status, receipt) = deployment
        .decide(&delegated, execution, &approval, &approve)
        .await;
    assert_eq!(status, StatusCode::OK, "{receipt}");
    assert_eq!(receipt["actor_id"], actor.to_string(), "{receipt}");
    let (status, replayed) = deployment
        .decide(&delegated, execution, &approval, &approve)
        .await;
    assert_eq!(status, StatusCode::OK, "{replayed}");
    assert_eq!(replayed, receipt, "the actor's retry replays");

    let (_, other) = on_behalf("approval-other-actor").await;
    for jwt in [&user_jwt, &other] {
        let (status, refused) = deployment.decide(jwt, execution, &approval, &approve).await;
        assert_eq!(status, StatusCode::CONFLICT, "{refused}");
        assert_eq!(
            refused["code"],
            "WYRD_VERIFICATION_409_APPROVAL_NOT_PENDING"
        );
    }
    let served = deployment.status(execution).await;
    assert_eq!(
        occurrence(&served)["actor_id"],
        actor.to_string(),
        "{served}"
    );
}
