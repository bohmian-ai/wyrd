//! Postgres-backed proofs for governed gateway invocation, cross-replica
//! admission, and ledger settlement.

use std::collections::{BTreeSet, HashMap, VecDeque};
use std::fs::Permissions;
use std::num::{NonZeroU32, NonZeroU64};
use std::os::unix::fs::PermissionsExt as _;
use std::path::Path;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use async_trait::async_trait;
use axum::Json;
use axum::body::Body;
use axum::extract::State;
use axum::http::{HeaderMap, StatusCode};
use axum::response::{IntoResponse as _, Response};
use base64::Engine as _;
use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use chrono::{TimeDelta, Utc};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::sync::{Notify, Semaphore, mpsc};
use tokio::task::{JoinHandle, JoinSet};
use tokio_util::sync::CancellationToken;
use tracing_subscriber::fmt::MakeWriter;
use tracing_subscriber::fmt::format::FmtSpan;
use uuid::Uuid;
use vala_bifrost_redux::catalog::TableRef;
use vala_bifrost_redux::namespaces::BifrostNamespace;
use wiremock::matchers::{body_partial_json, header, method, path};
use wiremock::{Mock, MockServer, Request, ResponseTemplate};
use wyrd_dev_fixtures::pg::PgFixture;
use wyrd_gateway::{
    AttemptRecord, AttemptResult, AttemptUsage, CallExecution, CallPlan, CredentialResolver,
    DeploymentHealth, FailureClass, GatewayCost, GatewayEngine, IngressDialect, ManagedSecretKeys,
    MediaAnswer, ProviderAttempt, ProviderDispatch, ProviderSecret, ResponseBody, ResponseCapture,
};
use wyrd_runtime::{
    Action, Permission, PermissionSet, Principal, PrincipalId, PrincipalKind, Resource, RoleRef,
};
use wyrd_spec::DataTenantId;
use wyrd_spec::auth::GatewayAccess;
use wyrd_spec::error::WyrdError;
use wyrd_spec::gateway::{
    CurrencyCode, FALLBACK_HEADER, GatewayAccountingEntryId, GatewayAccountingEntryV1,
    GatewayBudgetReservationId, GatewayCallId, GatewayCallOutcome, GatewayCaptureMode,
    GatewayCapturePolicyWrite, GatewayDecimal, GatewayFallbackOverride, GatewayLimit,
    GatewayLimitSubject, GatewayOperation, GatewayPayloadField, GatewayPolicySubject,
    GatewayPolicyTarget, GatewayUsageAmount, ModelRef, ProviderDeployment,
};
use wyrd_spec::ids::{ProviderCredentialName, ProviderDeploymentName, ProviderId};
use wyrd_spec::request_id::RequestId;
use wyrd_sql::queries::gateway::{
    GatewayAccountingEntryWrite, append_gateway_accounting_entry, claim_gateway_batch,
    gateway_call_accounting_entries, lock_gateway_admission, record_gateway_batch,
};
use wyrd_sql::row_types::gateway::GatewayBatchRow;
use wyrd_storage::{BackendSigner, LocalSigner, StorageHandle};

use super::capture::object_path;
use super::capture::recording::RecordingScribe;
use super::ledger::{GatewayLedger, LedgerCall};
use super::pg_administration_tests::{
    admin, audit_decisions, await_lock_waiters, keyring, managed_keys, test_state,
};
use super::{GatewayAdministration, GatewayCallRequest, GatewayCallResponse, GatewayInvocation};
use crate::components::auth::Caller;
use crate::http::error::WyrdErrorResponse;
use crate::state::{AppState, LimitsConfig};

/// One scripted provider attempt.
enum Step {
    /// Returns the result immediately.
    Return(AttemptResult),
    /// Signals `entered`, waits for `release`, then returns the result.
    Park(AttemptResult),
    /// Never returns, so only the caller deadline ends the attempt.
    Hang,
}

/// Scripted provider dispatch recording every attempted deployment.
#[derive(Default)]
struct Scripted {
    /// Remaining steps; an exhausted script hangs.
    script: Mutex<VecDeque<Step>>,
    /// Deployment names in dispatch order.
    seen: Mutex<Vec<String>>,
    /// Notified when a parked attempt starts.
    entered: Notify,
    /// Releases a parked attempt.
    release: Notify,
}

impl Scripted {
    /// Builds an empty shared script.
    fn shared() -> Arc<Self> {
        Arc::new(Self::default())
    }

    /// Appends one step.
    fn push(&self, step: Step) {
        self.script.lock().expect("script").push_back(step);
    }

    /// Deployments dispatched so far.
    fn seen(&self) -> Vec<String> {
        self.seen.lock().expect("seen").clone()
    }
}

#[async_trait]
impl ProviderDispatch for Scripted {
    /// Records the deployment and plays the next step.
    async fn dispatch(&self, attempt: ProviderAttempt<'_>) -> AttemptResult {
        self.seen
            .lock()
            .expect("seen")
            .push(attempt.deployment.name.as_str().to_owned());
        let step = self.script.lock().expect("script").pop_front();
        match step {
            Some(Step::Return(result)) => result,
            Some(Step::Park(result)) => {
                self.entered.notify_one();
                self.release.notified().await;
                result
            }
            Some(Step::Hang) | None => std::future::pending().await,
        }
    }
}

/// Dispatch holding every attempt until a permit is released, reporting each
/// entry so a test can tell dispatched calls from rejected ones.
struct Held {
    /// Receives one message per dispatched attempt.
    entered: mpsc::UnboundedSender<()>,
    /// One permit completes one held attempt.
    release: Semaphore,
}

#[async_trait]
impl ProviderDispatch for Held {
    /// Reports entry, waits for a permit, then completes with known usage.
    async fn dispatch(&self, _attempt: ProviderAttempt<'_>) -> AttemptResult {
        self.entered.send(()).expect("test observes dispatch");
        self.release.acquire().await.expect("release open").forget();
        completed(100, 50)
    }
}

/// Builds one server replica over `fixture` whose engine uses `dispatch`.
async fn replica(fixture: &PgFixture, dispatch: Arc<dyn ProviderDispatch>) -> AppState {
    test_state(fixture)
        .await
        .with_gateway_engine(GatewayEngine::new(
            CredentialResolver::default(),
            DeploymentHealth::default(),
            dispatch,
        ))
}

/// Deterministic principal id `n`.
fn principal(n: u128) -> PrincipalId {
    PrincipalId::new(Uuid::from_u128(n))
}

/// Builds caller `n` in `tenant` holding exactly `permissions`.
fn invoker(
    tenant: DataTenantId,
    n: u128,
    permissions: impl IntoIterator<Item = Permission>,
) -> Caller {
    Caller {
        data_tenant_id: tenant,
        principal: Principal::new(
            principal(n),
            PrincipalKind::User,
            tenant,
            Vec::<RoleRef>::new(),
            PermissionSet::from_iter(permissions),
        ),
        request_id: RequestId::now_v7(),
        delegation_chain: Vec::new(),
    }
}

/// Parses a model projection.
fn model(value: &str) -> ModelRef {
    ModelRef::from_projection(value).expect("model")
}

/// Invoke permission for every `acme` model.
fn provider_access() -> Permission {
    Permission::gateway_invoke(GatewayAccess::Provider {
        provider: ProviderId::new("acme").expect("provider"),
    })
}

/// Invoke permission for exactly `value`.
fn model_access(value: &str) -> Permission {
    let model = model(value);
    Permission::gateway_invoke(GatewayAccess::Model {
        provider: model.provider,
        model: model.model,
    })
}

/// One token usage amount.
fn amount(dimension: &str, quantity: u64) -> GatewayUsageAmount {
    GatewayUsageAmount {
        dimension: dimension.to_owned(),
        unit: "tokens".to_owned(),
        quantity: GatewayDecimal::new(&quantity.to_string()).expect("quantity"),
    }
}

/// Known provider-native and normalized usage.
fn usage(input: u64, output: u64) -> AttemptUsage {
    AttemptUsage {
        provider_usage_json: Some(format!(
            r#"{{"completion_tokens":{output},"prompt_tokens":{input}}}"#
        )),
        normalized: Some(vec![
            amount("input_tokens", input),
            amount("output_tokens", output),
        ]),
    }
}

/// Completed attempt with known usage.
fn completed(input: u64, output: u64) -> AttemptResult {
    AttemptResult::Completed {
        body: ResponseBody::Json(
            serde_json::value::to_raw_value(&json!({"id": "chatcmpl-test"})).expect("json"),
        ),
        usage: usage(input, output),
        capture: None,
    }
}

/// Retryable upstream failure with known billed usage.
fn failed(input: u64, output: u64) -> AttemptResult {
    AttemptResult::Failed {
        class: FailureClass::Upstream,
        usage: usage(input, output),
    }
}

/// Chat request for `requested` bounded by `(input, output)` tokens.
fn request(requested: &str, bound: Option<(u64, u64)>, timeout: Duration) -> GatewayCallRequest {
    GatewayCallRequest {
        operation: GatewayOperation::ChatCompletions,
        ingress: IngressDialect::OpenAi,
        model: model(requested),
        fallback: None,
        body: json!({"model": requested, "messages": []}),
        media: None,
        batch: None,
        deployment: None,
        stream: false,
        usage_bound: bound.map(|(input, output)| {
            vec![
                amount("input_tokens", input),
                amount("output_tokens", output),
            ]
        }),
        timeout,
        subject: None,
    }
}

/// Configures deployments `dep-a` (`acme/a`) and `dep-b` (`acme/b`), a global
/// fallback to `acme/b`, pricing of 2 and 8 per million input and output
/// tokens, and the given limits, budgets, and unknown-cost policy.
async fn configure(
    state: &AppState,
    tenant: DataTenantId,
    limits: Value,
    budgets: Value,
    unknown_cost: &str,
) {
    let admin = admin(tenant);
    let gateway = GatewayAdministration::new(state);
    for (name, served) in [("dep-a", "acme/a"), ("dep-b", "acme/b")] {
        let deployment: ProviderDeployment = serde_json::from_value(json!({
            "name": name,
            "model": model(served),
            "adapter": {"openai_compatible": {"base_url": "https://acme.example/v1"}},
            "auth": "none",
            "capabilities": ["chat_completions"],
            "routing_weight": 1,
        }))
        .expect("deployment decodes");
        gateway
            .put_deployment(
                &admin,
                &ProviderDeploymentName::new(name).expect("name"),
                deployment,
            )
            .await
            .expect("deployment stores");
    }
    gateway
        .put_fallback(
            &admin,
            serde_json::from_value(
                json!({"rules": [{"scope": "global", "candidates": [model("acme/b")]}]}),
            )
            .expect("fallback decodes"),
        )
        .await
        .expect("fallback stores");
    let pricing = |served: &str, version: &str| {
        json!({
            "model": model(served), "version": version, "currency": "USD",
            "effective_at": "2020-01-01T00:00:00Z", "active": true,
            "rates": [
                {"dimension": "input_tokens", "unit": "1m_tokens", "price": "2"},
                {"dimension": "output_tokens", "unit": "1m_tokens", "price": "8"},
            ],
        })
    };
    gateway
        .put_governance(
            &admin,
            serde_json::from_value(json!({
                "limits": limits,
                "budgets": budgets,
                "pricing": [pricing("acme/a", "a-v1"), pricing("acme/b", "b-v1")],
                "unknown_cost": unknown_cost,
            }))
            .expect("governance decodes"),
        )
        .await
        .expect("governance stores");
}

/// Daily tenant budget of `amount` USD.
fn daily_budget(amount: &str) -> Value {
    json!([{"subject": "tenant", "period": "calendar_day_utc", "amount": amount, "currency": "USD"}])
}

/// Decodes a handler response body as JSON.
async fn body_json(response: axum::response::Response) -> Value {
    let bytes = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .expect("body reads");
    serde_json::from_slice(&bytes).expect("json body")
}

/// Reads and decodes every ledger entry of `call_id`.
async fn entries(
    fixture: &PgFixture,
    tenant: DataTenantId,
    call_id: GatewayCallId,
) -> Vec<GatewayAccountingEntryV1> {
    let mut conn = fixture.tenant_conn_for(tenant).await.expect("tenant conn");
    let rows = gateway_call_accounting_entries(&mut conn, call_id.as_uuid())
        .await
        .expect("ledger reads");
    conn.commit().await.expect("ledger read commits");
    rows.into_iter()
        .map(|row| serde_json::from_value(row).expect("ledger entry decodes"))
        .collect()
}

/// Appends a raw ledger document as `kind`, returning whether it was new.
async fn append_raw(
    fixture: &PgFixture,
    tenant: DataTenantId,
    kind: &str,
    call_id: Uuid,
    entry: &Value,
) -> bool {
    let mut conn = fixture.tenant_conn_for(tenant).await.expect("tenant conn");
    let inserted = append_gateway_accounting_entry(
        &mut conn,
        GatewayAccountingEntryWrite {
            entry_id: Uuid::now_v7(),
            call_id,
            kind,
            provider: None,
            model: None,
            pricing_version: None,
            entry,
        },
    )
    .await
    .expect("ledger append");
    conn.commit().await.expect("ledger append commits");
    inserted
}

/// Attempt that fails before reaching the provider, so nothing is billed.
fn failed_before_dispatch() -> AttemptResult {
    AttemptResult::Failed {
        class: FailureClass::BeforeDispatch,
        usage: AttemptUsage::default(),
    }
}

/// Sums tokens held or charged across every limit window of `tenant`.
async fn window_tokens(fixture: &PgFixture, tenant: DataTenantId) -> i64 {
    let mut conn = fixture.tenant_conn_for(tenant).await.expect("tenant conn");
    let tokens: i64 = sqlx::query_scalar(
        "SELECT COALESCE(SUM(tokens), 0)::bigint FROM wyrd.gateway_limit_windows",
    )
    .fetch_one(&mut **conn.transaction())
    .await
    .expect("window tokens read");
    conn.commit().await.expect("window read commits");
    tokens
}

/// Asserts a decimal equals `expected` numerically.
fn assert_cost(actual: &GatewayDecimal, expected: &str) {
    assert_eq!(
        GatewayCost::from_decimal(actual),
        GatewayCost::parse(expected).expect("expected cost"),
        "{} != {expected}",
        actual.as_str()
    );
}

/// The settlement entry among `entries`, as `(actual, released)`.
fn settlement(entries: &[GatewayAccountingEntryV1]) -> (&GatewayDecimal, &GatewayDecimal) {
    entries
        .iter()
        .find_map(|entry| match entry {
            GatewayAccountingEntryV1::BudgetReservationSettled {
                actual_cost,
                released_cost,
                ..
            } => Some((actual_cost, released_cost)),
            _ => None,
        })
        .expect("reservation settled")
}

/// Dispatch recording the resolved provider key of every attempt, so a test
/// can tell which tenant's secret reached upstream and prove that a refused
/// call dispatched nothing at all.
#[derive(Default)]
struct RecordingKeys {
    /// Exposed provider key of each dispatched attempt, in order.
    seen: Mutex<Vec<String>>,
}

impl RecordingKeys {
    /// Builds an empty shared recorder.
    fn shared() -> Arc<Self> {
        Arc::new(Self::default())
    }

    /// Keys dispatched so far.
    ///
    /// # Panics
    ///
    /// Panics when a previous dispatch poisoned the lock.
    fn seen(&self) -> Vec<String> {
        self.seen.lock().expect("seen").clone()
    }
}

#[async_trait]
impl ProviderDispatch for RecordingKeys {
    /// Records the attempt's resolved key and completes with known usage.
    ///
    /// # Panics
    ///
    /// Panics when a previous dispatch poisoned the lock.
    async fn dispatch(&self, attempt: ProviderAttempt<'_>) -> AttemptResult {
        self.seen.lock().expect("seen").push(
            attempt
                .credential
                .map(ProviderSecret::expose)
                .unwrap_or_default()
                .to_owned(),
        );
        completed(10, 5)
    }
}

/// Builds one replica whose administration and resolver share `keys`, the
/// shape a restarted or additional process has when it loads the same
/// configured tenant keyrings.
///
/// # Panics
///
/// Panics when the replica's application state cannot be built on `fixture`.
async fn managed_replica(
    fixture: &PgFixture,
    keys: Arc<ManagedSecretKeys>,
    dispatch: Arc<dyn ProviderDispatch>,
) -> AppState {
    test_state(fixture)
        .await
        .with_gateway_secret_keys(Arc::clone(&keys))
        .with_gateway_engine(GatewayEngine::new(
            CredentialResolver::new(std::collections::BTreeMap::new(), keys),
            DeploymentHealth::default(),
            dispatch,
        ))
}

/// Submits `secret` as `tenant`'s managed credential and points the only
/// deployment of `acme/a` at it with bearer authentication.
///
/// # Panics
///
/// Panics when a fixture identifier is invalid, when a fixture body does not
/// decode, or when the credential, deployment, or governance write fails.
async fn configure_managed(state: &AppState, tenant: DataTenantId, secret: &str) {
    let admin = admin(tenant);
    let gateway = GatewayAdministration::new(state);
    gateway
        .put_credential(
            &admin,
            &ProviderCredentialName::new("managed").expect("credential name"),
            serde_json::from_value(json!({
                "name": "managed",
                "provider": "acme",
                "source": {"managed_secret": {"secret": secret}},
            }))
            .expect("credential decodes"),
        )
        .await
        .expect("managed submission stores");
    gateway
        .put_deployment(
            &admin,
            &ProviderDeploymentName::new("dep-a").expect("deployment name"),
            serde_json::from_value(json!({
                "name": "dep-a",
                "model": model("acme/a"),
                "adapter": {"openai_compatible": {"base_url": "https://acme.example/v1"}},
                "auth": {"bearer": {"credential": "managed"}},
                "capabilities": ["chat_completions"],
                "routing_weight": 1,
            }))
            .expect("deployment decodes"),
        )
        .await
        .expect("deployment stores");
    gateway
        .put_governance(
            &admin,
            serde_json::from_value(json!({
                "limits": [], "budgets": [], "pricing": [],
                "unknown_cost": "allow_unpriced",
            }))
            .expect("governance decodes"),
        )
        .await
        .expect("governance stores");
}

/// Invokes `acme/a` in `tenant` as a narrowly scoped caller.
///
/// # Errors
/// Forwards every [`GatewayInvocation::invoke`] failure unchanged — the
/// permission denial, the redacted credential-unavailable class, and each
/// dispatch error — so a caller can assert on the exact refusal.
async fn invoke_managed(
    state: &AppState,
    tenant: DataTenantId,
) -> Result<GatewayCallResponse, WyrdError> {
    GatewayInvocation::new(state)
        .invoke(
            &invoker(tenant, 7, [model_access("acme/a")]),
            request("acme/a", None, Duration::from_secs(10)),
        )
        .await
}

/// Managed credentials resolve per tenant from the locally configured
/// keyring: two tenants send their own key upstream, a restarted replica and
/// a rotated active version keep resolving committed envelopes, and missing,
/// incorrect, or revoked authority fails with the stable 503-class error
/// before any dispatch.
///
/// # Panics
///
/// Panics when a tenant's own key does not reach dispatch, when a refusal
/// dispatches, or when a refusal is not the redacted upstream-unavailable
/// error.
#[tokio::test]
async fn gateway_managed_credentials_resolve_per_tenant_across_restart_and_rotation() {
    let fixture = PgFixture::start().await.expect("fixture starts");
    let first = fixture.data_tenant_id();
    let second = fixture
        .seed_additional_tenant("gateway-managed-second")
        .await
        .expect("second tenant");
    let tenants = [first, second];
    let dispatch = RecordingKeys::shared();
    let live = managed_keys(&tenants, "v1", &["v0", "v1"]);

    let state = managed_replica(&fixture, Arc::clone(&live), dispatch.clone()).await;
    configure_managed(&state, first, "sk-first-tenant").await;
    configure_managed(&state, second, "sk-second-tenant").await;
    for tenant in tenants {
        invoke_managed(&state, tenant).await.expect("invoke");
    }
    assert_eq!(
        dispatch.seen(),
        vec!["sk-first-tenant".to_owned(), "sk-second-tenant".to_owned()],
        "each tenant's own key, and only it, reaches dispatch"
    );

    // A restarted replica loading the same keyrings resolves committed values.
    let restarted = managed_replica(
        &fixture,
        managed_keys(&tenants, "v1", &["v0", "v1"]),
        dispatch.clone(),
    )
    .await;
    invoke_managed(&restarted, first).await.expect("restart");
    assert_eq!(dispatch.seen().len(), 3);

    // Rotating the active version keeps the retained version readable, and a
    // resubmission moves the credential onto the new active version.
    let rotated = managed_replica(
        &fixture,
        managed_keys(&tenants, "v2", &["v1", "v2"]),
        dispatch.clone(),
    )
    .await;
    invoke_managed(&rotated, first).await.expect("retained");
    configure_managed(&rotated, first, "sk-first-rotated").await;
    invoke_managed(&rotated, first).await.expect("resealed");
    assert_eq!(
        dispatch.seen()[3..],
        ["sk-first-tenant", "sk-first-rotated"]
    );

    // Every failure of key authority refuses before dispatch.
    let dispatched = dispatch.seen().len();
    let unconfigured = managed_replica(&fixture, Arc::default(), dispatch.clone()).await;
    let retired = managed_replica(
        &fixture,
        managed_keys(&tenants, "v0", &["v0"]),
        dispatch.clone(),
    )
    .await;
    let foreign = managed_replica(
        &fixture,
        Arc::new(ManagedSecretKeys::new(
            [(first, keyring(second, "v2", &["v2"]))]
                .into_iter()
                .collect(),
        )),
        dispatch.clone(),
    )
    .await;
    for (refusing, reason) in [
        (&unconfigured, "no keyring is configured for the tenant"),
        (&retired, "the sealing version was retired too early"),
        (&foreign, "the keyring holds another tenant's material"),
    ] {
        let refused = invoke_managed(refusing, first)
            .await
            .expect_err("key authority is unusable");
        assert!(
            matches!(refused, WyrdError::GatewayUpstreamUnavailable { .. }),
            "{reason}: {refused:?}"
        );
    }

    // Revocation is equally terminal for a managed credential.
    GatewayAdministration::new(&state)
        .revoke_credential(
            &admin(first),
            &ProviderCredentialName::new("managed").expect("credential name"),
        )
        .await
        .expect("revoke");
    let revoked = invoke_managed(&state, first)
        .await
        .expect_err("a revoked credential never resolves");
    assert!(
        matches!(revoked, WyrdError::GatewayUpstreamUnavailable { .. }),
        "{revoked:?}"
    );
    assert_eq!(
        dispatch.seen().len(),
        dispatched,
        "no refusal reached the provider"
    );
    drain_gateway(&state).await;
    drain_gateway(&restarted).await;
    drain_gateway(&rotated).await;
    drain_gateway(&unconfigured).await;
    drain_gateway(&retired).await;
    drain_gateway(&foreign).await;
}

/// Public handler and internal seam share one pipeline: the requested model
/// is authorized and audited before dispatch, an unauthorized fallback is
/// skipped and audited, fallback preserves requested and resolved identity,
/// and billed failed and successful attempts are priced in the ledger.
#[tokio::test]
async fn gateway_invocation_authorizes_each_model_and_accounts_attempts() {
    let fixture = PgFixture::start().await.expect("fixture starts");
    let tenant = fixture.data_tenant_id();
    let dispatch = Scripted::shared();
    let scribe = Arc::new(RecordingScribe::default());
    let state = scribe.record(replica(&fixture, dispatch.clone()).await);
    configure(&state, tenant, json!([]), json!([]), "allow_unpriced").await;
    let invocation = GatewayInvocation::new(&state);
    let decision = |outcome: &str| ("gateway.invoke".to_owned(), outcome.to_owned());

    let refused = super::routes::chat_completions(
        State(state.clone()),
        Ok(invoker(tenant, 1, [])),
        HeaderMap::new(),
        Ok(Json(
            json!({"model": "acme/a", "max_tokens": 10, "messages": []}),
        )),
    )
    .await;
    assert_eq!(refused.status(), StatusCode::FORBIDDEN);
    assert_eq!(
        body_json(refused).await["error"]["type"],
        "permission_error",
        "public refusals use the OpenAI error envelope"
    );
    assert!(dispatch.seen().is_empty(), "a denial never dispatches");
    drain_gateway(&state).await;
    assert_eq!(
        audit_decisions(&scribe, tenant).last(),
        Some(&decision("denied"))
    );

    dispatch.push(Step::Return(failed(1000, 0)));
    let narrow = invoker(tenant, 1, [model_access("acme/a")]);
    let error = invocation
        .invoke(&narrow, request("acme/a", None, Duration::from_secs(10)))
        .await
        .expect_err("the only authorized candidate fails");
    assert!(
        matches!(error, WyrdError::GatewayUpstreamUnavailable { .. }),
        "{error:?}"
    );
    assert_eq!(
        dispatch.seen(),
        ["dep-a"],
        "unauthorized acme/b is never dispatched"
    );
    drain_gateway(&state).await;
    let decisions = audit_decisions(&scribe, tenant);
    assert_eq!(
        decisions[decisions.len() - 2..],
        [decision("allowed"), decision("denied")]
    );

    dispatch.push(Step::Return(failed(1000, 0)));
    dispatch.push(Step::Return(completed(100, 50)));
    let broad = invoker(tenant, 1, [provider_access()]);
    // The failure above holds dep-a in replica-local cooldown; a fresh replica
    // attempts it again before falling back.
    let fresh = scribe.record(replica(&fixture, dispatch.clone()).await);
    let response = GatewayInvocation::new(&fresh)
        .invoke(
            &broad,
            request("acme/a", Some((1000, 500)), Duration::from_secs(10)),
        )
        .await
        .expect("fallback completes");
    assert_eq!(
        (response.requested, response.resolved),
        (model("acme/a"), model("acme/b"))
    );
    assert_eq!(dispatch.seen(), ["dep-a", "dep-a", "dep-b"]);
    let ledger = entries(&fixture, tenant, response.call_id).await;
    let attempts: Vec<(GatewayCallOutcome, &str)> = ledger
        .iter()
        .filter_map(|entry| match entry {
            GatewayAccountingEntryV1::AttemptAccounted {
                outcome,
                cost: Some(cost),
                ..
            } => Some((*outcome, cost.as_str())),
            _ => None,
        })
        .collect();
    assert_eq!(
        attempts.len(),
        2,
        "both billed attempts are priced: {ledger:?}"
    );
    assert_eq!(
        (attempts[0].0, attempts[1].0),
        (GatewayCallOutcome::Failed, GatewayCallOutcome::Succeeded)
    );
    let call = ledger.iter().find_map(|entry| match entry {
        GatewayAccountingEntryV1::CallAccounted {
            outcome,
            cost: Some(cost),
            pricing_versions,
            normalized_usage: Some(_),
            ..
        } => Some((outcome, cost, pricing_versions)),
        _ => None,
    });
    let (outcome, cost, versions) = call.expect("priced call entry");
    assert_eq!(*outcome, GatewayCallOutcome::Succeeded);
    assert_cost(cost, "0.0026");
    assert_eq!(
        *versions,
        BTreeSet::from(["a-v1".to_owned(), "b-v1".to_owned()])
    );

    dispatch.push(Step::Return(completed(10, 5)));
    let public = super::routes::chat_completions(
        State(state.clone()),
        Ok(broad),
        HeaderMap::new(),
        Ok(Json(
            json!({"model": "acme/a", "max_tokens": 10, "messages": []}),
        )),
    )
    .await;
    assert_eq!(public.status(), StatusCode::OK);
    assert_eq!(body_json(public).await, json!({"id": "chatcmpl-test"}));
    let mut conn = fixture.tenant_conn_for(tenant).await.expect("tenant conn");
    let calls: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM wyrd.gateway_accounting_entries WHERE kind = 'call_accounted'",
    )
    .fetch_one(&mut **conn.transaction())
    .await
    .expect("call entries counted");
    conn.commit().await.expect("count commits");
    assert_eq!(calls, 3, "internal and public calls share one ledger");
    drain_gateway(&fresh).await;
    drain_gateway(&state).await;
}

/// Two replicas share limits and budgets: a parked call's concurrency lease
/// and reservation on one replica refuse admission on the other before any
/// dispatch, settlement releases the unused reservation, and an unbounded
/// call is rejected where a budget applies.
#[tokio::test]
async fn gateway_admission_holds_limits_and_budgets_across_replicas() {
    let fixture = PgFixture::start().await.expect("fixture starts");
    let tenant = fixture.data_tenant_id();
    let (first, second) = (Scripted::shared(), Scripted::shared());
    let a = replica(&fixture, first.clone()).await;
    let b = replica(&fixture, second.clone()).await;
    let limit = serde_json::to_value(GatewayLimit {
        subject: GatewayLimitSubject::Principal {
            principal_id: principal(1),
        },
        target: GatewayPolicyTarget::All,
        requests_per_minute: None,
        tokens_per_minute: None,
        concurrent_calls: NonZeroU64::new(1),
    })
    .expect("limit encodes");
    configure(
        &a,
        tenant,
        json!([limit]),
        daily_budget("0.02"),
        "allow_unpriced",
    )
    .await;
    let held = invoker(tenant, 1, [provider_access()]);
    let other = invoker(tenant, 2, [provider_access()]);
    let bounded = || request("acme/a", Some((1000, 500)), Duration::from_secs(30));

    first.push(Step::Park(completed(100, 50)));
    let parked = tokio::spawn({
        let (a, held, request) = (a.clone(), held.clone(), bounded());
        async move { GatewayInvocation::new(&a).invoke(&held, request).await }
    });
    first.entered.notified().await;

    let on_b = GatewayInvocation::new(&b);
    match on_b.invoke(&held, bounded()).await {
        Err(WyrdError::GatewayLimitExceeded { details, .. }) => {
            let retry = details["retry_after_seconds"]
                .as_i64()
                .expect("retry after");
            assert!((1..=60).contains(&retry), "{details}");
        }
        other => panic!("expected the lease on replica A to refuse replica B: {other:?}"),
    }
    let over = on_b
        .invoke(&other, bounded())
        .await
        .expect_err("reservation spans replicas");
    assert!(
        matches!(over, WyrdError::GatewayBudgetExceeded { .. }),
        "{over:?}"
    );
    let unbounded = on_b
        .invoke(&other, request("acme/a", None, Duration::from_secs(30)))
        .await
        .expect_err("a budget requires a cost bound");
    assert!(
        matches!(unbounded, WyrdError::GatewayCostUnbounded { .. }),
        "{unbounded:?}"
    );
    assert!(
        second.seen().is_empty(),
        "refused admissions never dispatch"
    );

    first.release.notify_one();
    let settled = parked
        .await
        .expect("parked task joins")
        .expect("parked call completes");
    let ledger = entries(&fixture, tenant, settled.call_id).await;
    let (actual, released) = settlement(&ledger);
    assert_cost(actual, "0.0006");
    assert_cost(released, "0.0114");

    second.push(Step::Return(completed(100, 50)));
    second.push(Step::Return(completed(100, 50)));
    on_b.invoke(&other, bounded())
        .await
        .expect("settlement frees the budget");
    on_b.invoke(&held, bounded())
        .await
        .expect("accounting releases the lease");
    assert_eq!(second.seen(), ["dep-a", "dep-a"]);
    drain_gateway(&a).await;
    drain_gateway(&b).await;
}

/// The ledger fences replays, admission reconciles an abandoned expired
/// reservation at its full reserved cost, and a deadline-exceeded call records
/// a billable attempt with unknown usage and settles at the reservation.
#[tokio::test]
async fn gateway_ledger_fences_replays_and_settles_unknown_cost() {
    let fixture = PgFixture::start().await.expect("fixture starts");
    let tenant = fixture.data_tenant_id();
    let dispatch = Scripted::shared();
    let state = replica(&fixture, dispatch.clone()).await;
    configure(&state, tenant, json!([]), daily_budget("0.01"), "reject").await;

    let now = Utc::now();
    let period_start = now
        .date_naive()
        .and_hms_opt(0, 0, 0)
        .expect("midnight")
        .and_utc();
    let abandoned_call = GatewayCallId::new_v7();
    let abandoned = serde_json::to_value(GatewayAccountingEntryV1::BudgetReservationCreated {
        entry_id: GatewayAccountingEntryId::new_v7(),
        reservation_id: GatewayBudgetReservationId::new_v7(),
        call_id: abandoned_call,
        data_tenant_id: tenant,
        subject: GatewayPolicySubject::Tenant,
        reserved_cost: GatewayDecimal::new("0.005").expect("reserved"),
        currency: CurrencyCode::new("USD").expect("currency"),
        created_at: now - TimeDelta::seconds(2),
        period_start,
        period_end: period_start + TimeDelta::days(1),
        expires_at: now - TimeDelta::seconds(1),
    })
    .expect("reservation encodes");
    let kind = "budget_reservation_created";
    assert!(append_raw(&fixture, tenant, kind, abandoned_call.as_uuid(), &abandoned).await);
    assert!(
        !append_raw(&fixture, tenant, kind, abandoned_call.as_uuid(), &abandoned).await,
        "a replayed reservation is fenced"
    );

    dispatch.push(Step::Hang);
    let error = GatewayInvocation::new(&state)
        .invoke(
            &invoker(tenant, 1, [provider_access()]),
            request("acme/a", Some((100, 100)), Duration::from_millis(300)),
        )
        .await
        .expect_err("the attempt outlives the deadline");
    let WyrdError::GatewayDeadlineExceeded { details, .. } = &error else {
        panic!("expected deadline exceeded: {error:?}");
    };
    let call_id: GatewayCallId =
        serde_json::from_value(details["call_id"].clone()).expect("call id detail");

    let reconciled = entries(&fixture, tenant, abandoned_call).await;
    let (actual, released) = settlement(&reconciled);
    assert_cost(actual, "0.005");
    assert_cost(released, "0");

    let ledger = entries(&fixture, tenant, call_id).await;
    assert!(
        ledger.iter().any(|entry| matches!(
            entry,
            GatewayAccountingEntryV1::AttemptAccounted {
                outcome: GatewayCallOutcome::TimedOut,
                normalized_usage: None,
                cost: None,
                ..
            }
        )),
        "{ledger:?}"
    );
    let call = ledger
        .iter()
        .find(|entry| matches!(entry, GatewayAccountingEntryV1::CallAccounted { .. }))
        .expect("call entry");
    assert!(matches!(
        call,
        GatewayAccountingEntryV1::CallAccounted {
            outcome: GatewayCallOutcome::TimedOut,
            cost: None,
            ..
        }
    ));
    let (actual, released) = settlement(&ledger);
    assert_cost(actual, "0.002");
    assert_cost(released, "0");

    let mut replay = serde_json::to_value(call).expect("call entry encodes");
    replay["call_accounted"]["entry_id"] = json!(Uuid::now_v7());
    assert!(
        !append_raw(
            &fixture,
            tenant,
            "call_accounted",
            call_id.as_uuid(),
            &replay
        )
        .await,
        "a replayed call entry is fenced"
    );
    assert_eq!(entries(&fixture, tenant, call_id).await.len(), ledger.len());
    drain_gateway(&state).await;
}

/// A reservation covers every billable `(candidate, deployment)` attempt, a
/// longer period's reservation sharing the day's start never counts against the
/// day budget, and a call that bills nothing releases its whole reservation.
#[tokio::test]
async fn gateway_budget_reservations_cover_attempts_in_exact_periods() {
    let fixture = PgFixture::start().await.expect("fixture starts");
    let tenant = fixture.data_tenant_id();
    let dispatch = Scripted::shared();
    let state = replica(&fixture, dispatch.clone()).await;
    configure(&state, tenant, json!([]), daily_budget("0.03"), "reject").await;
    let now = Utc::now();
    let day_start = now
        .date_naive()
        .and_hms_opt(0, 0, 0)
        .expect("midnight")
        .and_utc();
    let month_call = GatewayCallId::new_v7();
    let month = serde_json::to_value(GatewayAccountingEntryV1::BudgetReservationCreated {
        entry_id: GatewayAccountingEntryId::new_v7(),
        reservation_id: GatewayBudgetReservationId::new_v7(),
        call_id: month_call,
        data_tenant_id: tenant,
        subject: GatewayPolicySubject::Tenant,
        reserved_cost: GatewayDecimal::new("0.025").expect("reserved"),
        currency: CurrencyCode::new("USD").expect("currency"),
        created_at: now,
        period_start: day_start,
        period_end: day_start + TimeDelta::days(31),
        expires_at: now + TimeDelta::hours(1),
    })
    .expect("reservation encodes");
    let kind = "budget_reservation_created";
    assert!(append_raw(&fixture, tenant, kind, month_call.as_uuid(), &month).await);
    let caller = invoker(tenant, 1, [provider_access()]);
    let bounded = || request("acme/a", Some((1000, 500)), Duration::from_secs(10));

    dispatch.push(Step::Return(failed(1000, 500)));
    dispatch.push(Step::Return(completed(1000, 500)));
    let billed = GatewayInvocation::new(&state)
        .invoke(&caller, bounded())
        .await
        .expect("the day budget ignores the longer period's spend");
    let ledger = entries(&fixture, tenant, billed.call_id).await;
    let reserved = ledger
        .iter()
        .find_map(|entry| match entry {
            GatewayAccountingEntryV1::BudgetReservationCreated { reserved_cost, .. } => {
                Some(reserved_cost)
            }
            _ => None,
        })
        .expect("reservation created");
    assert_cost(reserved, "0.012");
    let (actual, released) = settlement(&ledger);
    assert_cost(actual, "0.012");
    assert_cost(released, "0");

    // A fresh replica has no cooldown, so both deployments are attempted.
    let fresh = replica(&fixture, dispatch.clone()).await;
    dispatch.push(Step::Return(failed_before_dispatch()));
    dispatch.push(Step::Return(failed_before_dispatch()));
    let error = GatewayInvocation::new(&fresh)
        .invoke(&caller, bounded())
        .await
        .expect_err("no attempt reaches a provider");
    let WyrdError::GatewayUpstreamUnavailable { details, .. } = &error else {
        panic!("expected upstream unavailable: {error:?}");
    };
    let call_id: GatewayCallId =
        serde_json::from_value(details["call_id"].clone()).expect("call id detail");
    let ledger = entries(&fixture, tenant, call_id).await;
    let (actual, released) = settlement(&ledger);
    assert_cost(actual, "0");
    assert_cost(released, "0.012");
    drain_gateway(&state).await;
    drain_gateway(&fresh).await;
}

/// A token limit holds the call's bounded token exposure at admission so a
/// concurrent call cannot oversubscribe it, rejects unbounded exposure, and
/// settles actual tokens into the admission window even after that minute.
#[tokio::test]
async fn gateway_token_limits_hold_exposure_in_the_admission_window() {
    let fixture = PgFixture::start().await.expect("fixture starts");
    let tenant = fixture.data_tenant_id();
    let dispatch = Scripted::shared();
    let state = replica(&fixture, dispatch.clone()).await;
    let limit = serde_json::to_value(GatewayLimit {
        subject: GatewayLimitSubject::Tenant,
        target: GatewayPolicyTarget::All,
        requests_per_minute: None,
        tokens_per_minute: NonZeroU64::new(3200),
        concurrent_calls: None,
    })
    .expect("limit encodes");
    configure(&state, tenant, json!([limit]), json!([]), "allow_unpriced").await;
    let caller = invoker(tenant, 1, [provider_access()]);
    // 1500 bounded tokens per attempt over two deployments hold 3000.
    let bounded = || request("acme/a", Some((1000, 500)), Duration::from_secs(30));

    dispatch.push(Step::Park(completed(100, 50)));
    let parked = tokio::spawn({
        let (state, caller, request) = (state.clone(), caller.clone(), bounded());
        async move {
            GatewayInvocation::new(&state)
                .invoke(&caller, request)
                .await
        }
    });
    dispatch.entered.notified().await;
    assert_eq!(window_tokens(&fixture, tenant).await, 3000);
    let invocation = GatewayInvocation::new(&state);
    let held = invocation
        .invoke(&caller, bounded())
        .await
        .expect_err("the hold refuses a second exposure");
    assert!(
        matches!(held, WyrdError::GatewayLimitExceeded { .. }),
        "{held:?}"
    );
    let unbounded = invocation
        .invoke(&caller, request("acme/a", None, Duration::from_secs(30)))
        .await
        .expect_err("unbounded exposure cannot pass a token limit");
    assert!(
        matches!(unbounded, WyrdError::GatewayCostUnbounded { .. }),
        "{unbounded:?}"
    );
    dispatch.release.notify_one();
    parked
        .await
        .expect("parked task joins")
        .expect("parked call completes");
    assert_eq!(window_tokens(&fixture, tenant).await, 150);
    dispatch.push(Step::Return(completed(100, 50)));
    invocation
        .invoke(&caller, bounded())
        .await
        .expect("settlement frees the unused hold");

    // Settle a call admitted five minutes ago: its hold and actual tokens stay
    // in its own window, never the accounting minute.
    let snapshot = GatewayAdministration::new(&state)
        .snapshot(tenant)
        .await
        .expect("snapshot");
    let admitted_at = Utc::now() - TimeDelta::minutes(5);
    let usage_bound = [amount("input_tokens", 1000), amount("output_tokens", 500)];
    let call = LedgerCall {
        call_id: GatewayCallId::new_v7(),
        tenant,
        principal: &caller.principal,
        governance: &snapshot.governance,
        usage_bound: Some(&usage_bound),
        admitted_at,
        expires_at: admitted_at + TimeDelta::minutes(1),
    };
    let mut plan = CallPlan::new(
        &snapshot,
        call.call_id,
        GatewayOperation::ChatCompletions,
        model("acme/a"),
        None,
    );
    let mut conn = fixture.tenant_conn_for(tenant).await.expect("tenant conn");
    let admission = GatewayLedger::new(&mut conn)
        .admit(&call, &mut plan)
        .await
        .expect("past admission");
    conn.commit().await.expect("admission commits");
    let execution = CallExecution {
        outcome: GatewayCallOutcome::Succeeded,
        response: None,
        resolved: Some(model("acme/a")),
        attempts: vec![AttemptRecord {
            ordinal: NonZeroU32::MIN,
            deployment: ProviderDeploymentName::new("dep-a").expect("name"),
            model: model("acme/a"),
            outcome: GatewayCallOutcome::Succeeded,
            billable: true,
            usage: usage(40, 2),
            started_at: Utc::now(),
            terminal_at: Utc::now(),
        }],
        refusal: None,
        capture: None,
        attempt_span: tracing::Span::none(),
    };
    let mut conn = fixture.tenant_conn_for(tenant).await.expect("tenant conn");
    GatewayLedger::new(&mut conn)
        .account(&call, &admission, &execution)
        .await
        .expect("late accounting");
    conn.commit().await.expect("accounting commits");
    let mut conn = fixture.tenant_conn_for(tenant).await.expect("tenant conn");
    let admitted_window: i64 = sqlx::query_scalar(
        "SELECT tokens FROM wyrd.gateway_limit_windows WHERE window_start = date_trunc('minute', $1::timestamptz)",
    )
    .bind(admitted_at)
    .fetch_one(&mut **conn.transaction())
    .await
    .expect("admission window read");
    conn.commit().await.expect("window read commits");
    assert_eq!(admitted_window, 42);
    assert_eq!(window_tokens(&fixture, tenant).await, 150 + 150 + 42);
    drain_gateway(&state).await;
}

/// Model-targeted token limits settle only the attempts their target covers:
/// a requested-model failure followed by a fallback charges each window its
/// own attempt's tokens, a covered attempt with unknown usage keeps that
/// window's hold, and an unrelated unknown attempt leaves another window's
/// settlement exact.
///
/// # Panics
///
/// Panics when the Postgres fixture, tenant setup, or ledger reads fail, or
/// when a call outcome or window settlement differs from its covered attempts.
#[tokio::test]
async fn gateway_targeted_token_limits_settle_covered_attempts_only() {
    let fixture = PgFixture::start().await.expect("fixture starts");
    let tenant = fixture.data_tenant_id();
    let state = replica(&fixture, Scripted::shared()).await;
    let target = |served: &str| GatewayPolicyTarget::Model {
        model: model(served),
    };
    let limit = |served: &str| GatewayLimit {
        subject: GatewayLimitSubject::Tenant,
        target: target(served),
        requests_per_minute: None,
        tokens_per_minute: NonZeroU64::new(100_000),
        concurrent_calls: None,
    };
    configure(
        &state,
        tenant,
        serde_json::to_value([limit("acme/a"), limit("acme/b")]).expect("limits encode"),
        json!([]),
        "allow_unpriced",
    )
    .await;
    let caller = invoker(tenant, 1, [provider_access()]);
    let snapshot = GatewayAdministration::new(&state)
        .snapshot(tenant)
        .await
        .expect("snapshot");
    let admitted_at = Utc::now();
    let usage_bound = [amount("input_tokens", 1000), amount("output_tokens", 500)];
    let attempt = |ordinal: u32, served: &str, usage: AttemptUsage| AttemptRecord {
        ordinal: NonZeroU32::new(ordinal).expect("ordinal"),
        deployment: ProviderDeploymentName::new(if served == "acme/a" { "dep-a" } else { "dep-b" })
            .expect("name"),
        model: model(served),
        outcome: GatewayCallOutcome::Succeeded,
        billable: true,
        usage,
        started_at: admitted_at,
        terminal_at: admitted_at,
    };
    let window = |served: &str| {
        let key = serde_json::to_value((GatewayLimitSubject::Tenant, target(served)))
            .expect("key encodes")
            .to_string();
        let fixture = &fixture;
        async move {
            let mut conn = fixture.tenant_conn_for(tenant).await.expect("tenant conn");
            let tokens: i64 = sqlx::query_scalar(
                "SELECT tokens FROM wyrd.gateway_limit_windows WHERE limit_key = $1",
            )
            .bind(key)
            .fetch_one(&mut **conn.transaction())
            .await
            .expect("window read");
            conn.commit().await.expect("window read commits");
            tokens
        }
    };
    let fallbacks = [
        vec![
            attempt(1, "acme/a", usage(100, 0)),
            attempt(2, "acme/b", usage(40, 2)),
        ],
        vec![
            attempt(1, "acme/a", AttemptUsage::default()),
            attempt(2, "acme/b", usage(40, 2)),
        ],
    ];
    // Each call holds 1500 bounded tokens in each model's window.
    let expected = [(100, 42), (100 + 1500, 42 + 42)];
    for (attempts, expected) in fallbacks.into_iter().zip(expected) {
        let call = LedgerCall {
            call_id: GatewayCallId::new_v7(),
            tenant,
            principal: &caller.principal,
            governance: &snapshot.governance,
            usage_bound: Some(&usage_bound),
            admitted_at,
            expires_at: admitted_at + TimeDelta::minutes(1),
        };
        let mut plan = CallPlan::new(
            &snapshot,
            call.call_id,
            GatewayOperation::ChatCompletions,
            model("acme/a"),
            None,
        );
        let mut conn = fixture.tenant_conn_for(tenant).await.expect("tenant conn");
        let admission = GatewayLedger::new(&mut conn)
            .admit(&call, &mut plan)
            .await
            .expect("admission");
        GatewayLedger::new(&mut conn)
            .account(
                &call,
                &admission,
                &CallExecution {
                    outcome: GatewayCallOutcome::Succeeded,
                    response: None,
                    resolved: Some(model("acme/b")),
                    attempts,
                    refusal: None,
                    capture: None,
                    attempt_span: tracing::Span::none(),
                },
            )
            .await
            .expect("accounting");
        conn.commit().await.expect("call commits");
        assert_eq!((window("acme/a").await, window("acme/b").await), expected);
    }
    drain_gateway(&state).await;
}

/// Races [`RACE_CALLS`] bounded, priced calls across two replicas under
/// `limits` and `budgets`, returning how many dispatched and every rejection.
///
/// A superuser `SHARE` lock on the accounting ledger blocks only ledger
/// appends, which every call makes when it reserves budget after reading its
/// limit usage and budget spend, and releases once every call waits. Without
/// the tenant advisory lock each call therefore reads the same capacity before
/// any charge commits and all of them dispatch; with it, one call waits on the
/// append while holding the advisory lock and the rest wait on that lock, so
/// admissions serialize after release. The barrier locks no row admission
/// reads, prunes, or charges.
///
/// # Panics
///
/// Panics when the Postgres fixture, tenant setup, or ledger lock fails, when
/// the calls do not all reach the lock wait, or when a racing task panics.
async fn race_admissions(limits: Value, budgets: Value) -> (usize, Vec<WyrdError>) {
    let fixture = PgFixture::start().await.expect("fixture starts");
    let tenant = fixture.data_tenant_id();
    let (entered, mut dispatched) = mpsc::unbounded_channel();
    let held = Arc::new(Held {
        entered,
        release: Semaphore::new(0),
    });
    let replicas = [
        replica(&fixture, held.clone()).await,
        replica(&fixture, held.clone()).await,
    ];
    configure(&replicas[0], tenant, limits, budgets, "allow_unpriced").await;

    let superuser = fixture.superuser_pool().expect("superuser pool");
    let mut barrier = superuser.begin().await.expect("barrier begins");
    sqlx::query("LOCK TABLE wyrd.gateway_accounting_entries IN SHARE MODE")
        .execute(&mut *barrier)
        .await
        .expect("ledger appends held");
    let mut calls = JoinSet::new();
    for n in 0..RACE_CALLS {
        let state = replicas[n % 2].clone();
        let caller = invoker(tenant, n as u128, [provider_access()]);
        calls.spawn(async move {
            GatewayInvocation::new(&state)
                .invoke(
                    &caller,
                    request("acme/a", Some((1000, 500)), Duration::from_secs(30)),
                )
                .await
        });
    }
    await_lock_waiters(&fixture, i64::try_from(RACE_CALLS).expect("call count")).await;
    barrier.commit().await.expect("barrier releases");

    let (mut admitted, mut rejected) = (0, Vec::new());
    while admitted + rejected.len() < RACE_CALLS {
        tokio::select! {
            Some(()) = dispatched.recv() => admitted += 1,
            Some(joined) = calls.join_next() => match joined.expect("call task joins") {
                Err(error) => rejected.push(error),
                Ok(response) => panic!("an unreleased call cannot finish: {:?}", response.status),
            },
        }
    }
    held.release.add_permits(RACE_CALLS);
    while let Some(joined) = calls.join_next().await {
        joined
            .expect("call task joins")
            .expect("admitted call completes");
    }
    for replica in &replicas {
        drain_gateway(replica).await;
    }
    (admitted, rejected)
}

/// Calls raced by [`race_admissions`]; fits the test lane pool of 8
/// connections.
const RACE_CALLS: usize = 6;

/// Simultaneous admissions on two replicas dispatch exactly a tenant
/// concurrency limit's capacity, and exactly a daily budget's capacity, and
/// reject the rest before dispatch with the stable limit or budget error,
/// because every replica serializes admission on the tenant advisory lock.
///
/// # Panics
///
/// Panics when limit or budget serialization fails, when the admitted count
/// differs from capacity, or when a rejection is not the stable error.
#[tokio::test]
async fn gateway_simultaneous_admissions_across_replicas_never_oversubscribe() {
    const CAPACITY: usize = 2;
    let limit = serde_json::to_value(GatewayLimit {
        subject: GatewayLimitSubject::Tenant,
        target: GatewayPolicyTarget::All,
        requests_per_minute: None,
        tokens_per_minute: None,
        concurrent_calls: NonZeroU64::new(CAPACITY as u64),
    })
    .expect("limit encodes");
    let (admitted, rejected) = race_admissions(json!([limit]), daily_budget("100")).await;
    assert_eq!(admitted, CAPACITY, "{rejected:?}");
    assert_eq!(rejected.len(), RACE_CALLS - CAPACITY);
    assert!(
        rejected
            .iter()
            .all(|error| matches!(error, WyrdError::GatewayLimitExceeded { .. })),
        "{rejected:?}"
    );

    // Each call reserves 0.012 over its two attempts; 0.024 covers two.
    let (admitted, rejected) = race_admissions(json!([]), daily_budget("0.024")).await;
    assert_eq!(admitted, CAPACITY, "{rejected:?}");
    assert_eq!(rejected.len(), RACE_CALLS - CAPACITY);
    assert!(
        rejected
            .iter()
            .all(|error| matches!(error, WyrdError::GatewayBudgetExceeded { .. })),
        "{rejected:?}"
    );
}

/// A tenant onboards a DeepSeek-like `OpenAI`-compatible provider through
/// administration alone: an ordinary unpriced call reaches the local provider
/// authenticated only by the provider credential and is accounted, a provider
/// refusal keeps its status and body, native answer bytes and streamed events
/// reach the caller unchanged with the stream accounted once it ends, an
/// unrepresentable request or an undeclared operation fails before dispatch,
/// and dropping a public stream after one frame aborts the upstream connection
/// while the call and attempt are still accounted with unknown usage.
///
/// # Panics
///
/// Panics when the Postgres fixture, administration, or provider fixtures fail,
/// or when a call, stream, refusal, rejection, or accounting assertion differs.
#[tokio::test]
async fn gateway_onboards_compatible_provider_at_runtime() {
    // Every JSON family refuses a missing or mistyped required member with
    // the stable error envelope before dispatch.
    type Handler = fn(
        State<AppState>,
        Result<Caller, WyrdErrorResponse>,
        Result<Json<Value>, axum::extract::rejection::JsonRejection>,
    ) -> std::pin::Pin<
        Box<dyn std::future::Future<Output = axum::response::Response> + Send>,
    >;
    let fixture = PgFixture::start().await.expect("fixture starts");
    let tenant = fixture.data_tenant_id();
    let upstream = MockServer::start().await;
    let answer = "{ \"id\" : \"ds-1\", \"object\": \"chat.completion\", \"choices\": [],\n \"usage\": {\"prompt_tokens\": 4, \"completion_tokens\": 2, \"total_tokens\": 6} }";
    let events = "data: {\"id\":\"ds-2\",\"choices\":[{\"index\":0,\"delta\":{\"content\":\"hi\"}}]}\n\ndata: {\"id\":\"ds-2\",\"choices\":[],\"usage\":{\"prompt_tokens\":4,\"completion_tokens\":1,\"total_tokens\":5}}\n\ndata: [DONE]\n\n";
    Mock::given(method("POST"))
        .and(path("/v1/chat/completions"))
        .and(body_partial_json(json!({"stream": true})))
        .respond_with(ResponseTemplate::new(200).set_body_raw(events, "text/event-stream"))
        .with_priority(1)
        .mount(&upstream)
        .await;
    Mock::given(method("POST"))
        .and(path("/v1/chat/completions"))
        .and(body_partial_json(json!({"user": "limited"})))
        .respond_with(
            ResponseTemplate::new(429)
                .set_body_json(json!({"error": {"message": "slow down", "type": "rate_limit"}})),
        )
        .with_priority(1)
        .mount(&upstream)
        .await;
    Mock::given(method("POST"))
        .and(path("/v1/chat/completions"))
        .and(header("authorization", "Bearer sk-deepseek"))
        .respond_with(ResponseTemplate::new(200).set_body_raw(answer, "application/json"))
        .mount(&upstream)
        .await;
    let secret = tempfile::NamedTempFile::new().expect("secret file");
    std::fs::write(secret.path(), "sk-deepseek\n").expect("secret writes");
    let state = test_state(&fixture)
        .await
        .with_gateway(crate::config::GatewayConfig {
            credential_bindings: std::collections::BTreeMap::from([(
                wyrd_spec::ids::CredentialBindingName::new("deepseek-file").expect("binding"),
                crate::config::GatewayCredentialBinding {
                    secret: wyrd_spec::security::SecretRef::File {
                        path: secret.path().display().to_string(),
                    },
                    assignment: wyrd_gateway::CredentialAssignment {
                        host: Some("127.0.0.1".to_owned()),
                        ..super::pg_administration_tests::assigned(tenant, "deepseek")
                    },
                },
            )]),
            secret_backends: std::collections::BTreeMap::new(),
            ..Default::default()
        })
        .with_gateway_engine(GatewayEngine::new(
            CredentialResolver::default(),
            DeploymentHealth::default(),
            Arc::new(
                wyrd_gateway::HttpProviderDispatch::new(
                    skald_providers::EndpointPolicy::new(false),
                    wyrd_gateway::BuiltinEndpoints::default(),
                )
                .expect("gateway dispatch builds"),
            ),
        ));
    let admin = admin(tenant);
    let gateway = GatewayAdministration::new(&state);
    gateway
        .put_credential(
            &admin,
            &wyrd_spec::ids::ProviderCredentialName::new("deepseek-key").expect("name"),
            serde_json::from_value(json!({
                "name": "deepseek-key", "provider": "deepseek",
                "source": {"environment": {"binding": "deepseek-file"}}
            }))
            .expect("credential decodes"),
        )
        .await
        .expect("credential stores");
    gateway
        .put_deployment(
            &admin,
            &ProviderDeploymentName::new("deepseek-chat").expect("name"),
            serde_json::from_value(json!({
                "name": "deepseek-chat",
                "model": model("deepseek/deepseek-chat"),
                "adapter": {"openai_compatible": {"base_url": format!("{}/v1", upstream.uri())}},
                "auth": {"bearer": {"credential": "deepseek-key"}},
                "capabilities": ["chat_completions", "responses", "embeddings", "images", "audio"],
                "routing_weight": 1,
            }))
            .expect("deployment decodes"),
        )
        .await
        .expect("deployment stores");
    let caller = invoker(
        tenant,
        7,
        [Permission::gateway_invoke(GatewayAccess::Provider {
            provider: ProviderId::new("deepseek").expect("provider"),
        })],
    );
    let chat = |extra: Value| {
        let mut body = json!({"model": "deepseek/deepseek-chat", "messages": [{"role": "user", "content": "hi"}]});
        body.as_object_mut()
            .expect("object")
            .extend(extra.as_object().expect("object").clone());
        body
    };
    let call = |operation, body| GatewayCallRequest {
        operation,
        ingress: IngressDialect::OpenAi,
        model: model("deepseek/deepseek-chat"),
        fallback: None,
        body,
        media: None,
        batch: None,
        deployment: None,
        stream: false,
        usage_bound: None,
        timeout: Duration::from_secs(10),
        subject: None,
    };

    let response = GatewayInvocation::new(&state)
        .invoke(
            &caller,
            call(GatewayOperation::ChatCompletions, chat(json!({}))),
        )
        .await
        .expect("unpriced compatible call completes");
    assert_eq!(response.status, 200);
    assert!(
        matches!(&response.body, ResponseBody::Json(raw) if raw.get() == answer),
        "native answer bytes are preserved: {:?}",
        response.body
    );
    let received = upstream.received_requests().await.expect("recording");
    assert_eq!(received.len(), 1);
    let sent: Value = serde_json::from_slice(&received[0].body).expect("json");
    assert_eq!(
        sent,
        json!({"model": "deepseek-chat", "messages": [{"role": "user", "content": "hi"}]})
    );
    assert!(
        received[0]
            .headers
            .keys()
            .all(|name| !name.as_str().starts_with("wyrd-")),
        "only provider authentication reaches the provider"
    );
    let ledger = entries(&fixture, tenant, response.call_id).await;
    assert!(
        ledger.iter().any(|entry| matches!(
            entry,
            GatewayAccountingEntryV1::CallAccounted {
                outcome: GatewayCallOutcome::Succeeded,
                normalized_usage: Some(_),
                ..
            }
        )),
        "{ledger:?}"
    );

    let streaming = super::routes::chat_completions(
        State(state.clone()),
        Ok(caller.clone()),
        HeaderMap::new(),
        Ok(Json(chat(json!({"stream": true})))),
    )
    .await;
    assert_eq!(streaming.status(), StatusCode::OK);
    assert_eq!(
        streaming.headers()["content-type"],
        "text/event-stream",
        "streamed answers are server-sent events"
    );
    let relayed = axum::body::to_bytes(streaming.into_body(), usize::MAX)
        .await
        .expect("stream reads");
    assert_eq!(relayed, events.as_bytes());

    let response = GatewayInvocation::new(&state)
        .invoke(
            &caller,
            GatewayCallRequest {
                stream: true,
                ..call(
                    GatewayOperation::ChatCompletions,
                    chat(json!({"stream": true})),
                )
            },
        )
        .await
        .expect("streamed call opens");
    let ResponseBody::Events(mut stream) = response.body else {
        panic!("event stream expected: {:?}", response.body);
    };
    while stream.recv().await.is_some() {}
    // Accounting commits after the relay reports usage; poll the ledger.
    let accounted = tokio::time::timeout(Duration::from_secs(10), async {
        loop {
            let ledger = entries(&fixture, tenant, response.call_id).await;
            if let Some(usage) = ledger.into_iter().find_map(|entry| match entry {
                GatewayAccountingEntryV1::CallAccounted {
                    outcome: GatewayCallOutcome::Succeeded,
                    normalized_usage,
                    ..
                } => Some(normalized_usage),
                _ => None,
            }) {
                break usage;
            }
            tokio::task::yield_now().await;
        }
    })
    .await
    .expect("streamed call is accounted");
    assert_eq!(
        accounted,
        Some(vec![amount("input_tokens", 4), amount("output_tokens", 1)])
    );

    // Responses and embeddings share the public handler: native answers return
    // unchanged and a streamed Responses call is a terminated event stream.
    let responses_events = "event: response.completed\ndata: {\"type\":\"response.completed\",\"response\":{\"id\":\"r1\",\"status\":\"completed\",\"usage\":{\"input_tokens\":2,\"output_tokens\":3,\"total_tokens\":5}}}\n\n";
    Mock::given(method("POST"))
        .and(path("/v1/responses"))
        .respond_with(
            ResponseTemplate::new(200).set_body_raw(responses_events, "text/event-stream"),
        )
        .mount(&upstream)
        .await;
    let embedded = r#"{"object":"list","data":[{"object":"embedding","index":0,"embedding":[0.5]},{"object":"embedding","index":1,"embedding":[0.25]}],"model":"deepseek-chat","usage":{"prompt_tokens":2,"total_tokens":2}}"#;
    Mock::given(method("POST"))
        .and(path("/v1/embeddings"))
        .and(body_partial_json(
            json!({"input": ["b", "a"], "dimensions": 1}),
        ))
        .respond_with(ResponseTemplate::new(200).set_body_raw(embedded, "application/json"))
        .mount(&upstream)
        .await;
    let streamed_responses = super::routes::responses(
        State(state.clone()),
        Ok(caller.clone()),
        HeaderMap::new(),
        Ok(Json(json!({"model": "deepseek/deepseek-chat", "input": "hi", "max_output_tokens": 8, "stream": true}))),
    )
    .await;
    assert_eq!(streamed_responses.status(), StatusCode::OK);
    let relayed = axum::body::to_bytes(streamed_responses.into_body(), usize::MAX)
        .await
        .expect("responses stream reads");
    assert_eq!(relayed, responses_events.as_bytes());
    let embeddings = super::routes::embeddings(
        State(state.clone()),
        Ok(caller.clone()),
        Ok(Json(
            json!({"model": "deepseek/deepseek-chat", "input": ["b", "a"], "dimensions": 1}),
        )),
    )
    .await;
    assert_eq!(embeddings.status(), StatusCode::OK);
    assert_eq!(
        axum::body::to_bytes(embeddings.into_body(), usize::MAX)
            .await
            .expect("embeddings read"),
        embedded.as_bytes(),
        "ordered embeddings return unchanged"
    );
    let dispatched = upstream.received_requests().await.expect("recording").len();
    let zero = super::routes::embeddings(
        State(state.clone()),
        Ok(caller.clone()),
        Ok(Json(
            json!({"model": "deepseek/deepseek-chat", "input": "a", "dimensions": 0}),
        )),
    )
    .await;
    assert_eq!(zero.status(), StatusCode::BAD_REQUEST);
    assert_eq!(body_json(zero).await["error"]["param"], "dimensions");
    assert_eq!(
        upstream.received_requests().await.expect("recording").len(),
        dispatched,
        "invalid embeddings never dispatch"
    );

    // Responses decodes its shared contract: the text shorthand and the item
    // array both reach the provider exactly as sent, with unmodeled extensions.
    let items = json!([{"type": "message", "role": "user", "content": [{"type": "input_text", "text": "hi"}]}]);
    for input in [json!("hi"), items] {
        let sent = json!({"model": "deepseek/deepseek-chat", "input": input, "truncation": "auto", "metadata": {"k": "v"}, "stream": true});
        let answered = super::routes::responses(
            State(state.clone()),
            Ok(caller.clone()),
            HeaderMap::new(),
            Ok(Json(sent.clone())),
        )
        .await;
        assert_eq!(answered.status(), StatusCode::OK, "{input}");
        axum::body::to_bytes(answered.into_body(), usize::MAX)
            .await
            .expect("responses stream reads");
        let requests = upstream.received_requests().await.expect("recording");
        let received: Value =
            serde_json::from_slice(&requests.last().expect("dispatched").body).expect("json");
        let mut expected = sent;
        expected["model"] = json!("deepseek-chat");
        assert_eq!(received, expected, "the Responses body round-trips");
    }

    let model_name = "deepseek/deepseek-chat";
    let cases: [(Handler, Value, &str); 8] = [
        (
            |s, c, r| Box::pin(super::routes::chat_completions(s, c, HeaderMap::new(), r)),
            json!({"model": model_name}),
            "messages",
        ),
        (
            |s, c, r| Box::pin(super::routes::chat_completions(s, c, HeaderMap::new(), r)),
            json!({"model": 7, "messages": []}),
            "body",
        ),
        (
            |s, c, r| Box::pin(super::routes::responses(s, c, HeaderMap::new(), r)),
            json!({"model": model_name}),
            "input",
        ),
        (
            |s, c, r| Box::pin(super::routes::responses(s, c, HeaderMap::new(), r)),
            json!({"model": model_name, "input": 7}),
            "body",
        ),
        (
            |s, c, r| Box::pin(super::routes::embeddings(s, c, r)),
            json!({"model": model_name}),
            "input",
        ),
        (
            |s, c, r| Box::pin(super::routes::image_generations(s, c, r)),
            json!({"model": model_name}),
            "prompt",
        ),
        (
            |s, c, r| Box::pin(super::routes::audio_speech(s, c, r)),
            json!({"model": model_name, "input": "hi"}),
            "voice",
        ),
        (
            |s, c, r| Box::pin(super::routes::create_batch(s, c, r)),
            json!({"endpoint": "/v1/chat/completions", "completion_window": "24h"}),
            "input_file_id",
        ),
    ];
    let dispatched = upstream.received_requests().await.expect("recording").len();
    for (handler, body, param) in cases {
        let refused = handler(
            State(state.clone()),
            Ok(caller.clone()),
            Ok(Json(body.clone())),
        )
        .await;
        assert_eq!(refused.status(), StatusCode::BAD_REQUEST, "{body}");
        let envelope = body_json(refused).await;
        assert_eq!(envelope["error"]["param"], param, "{body}: {envelope}");
        assert_eq!(
            envelope["error"]["type"], "invalid_request_error",
            "{envelope}"
        );
    }
    assert_eq!(
        upstream.received_requests().await.expect("recording").len(),
        dispatched,
        "invalid bodies never dispatch"
    );

    // The model list shows exactly the configured models the caller may
    // invoke; a caller without invoke permission sees none.
    let listed =
        body_json(super::routes::models(State(state.clone()), Ok(caller.clone())).await).await;
    assert_eq!(
        listed,
        json!({"object": "list", "data": [{"id": "deepseek/deepseek-chat", "object": "model", "created": 0, "owned_by": "deepseek"}]})
    );
    let outsider = invoker(tenant, 8, [provider_access()]);
    assert_eq!(
        body_json(super::routes::models(State(state.clone()), Ok(outsider)).await).await,
        json!({"object": "list", "data": []})
    );

    // Images and Audio: JSON answers return unchanged, binary and text answers
    // keep their content type, forms reach the provider with their files, and
    // a form missing its required file never dispatches.
    let image = r#"{"created":1,"data":[{"b64_json":"aGk="}],"usage":{"input_tokens":3,"output_tokens":7,"total_tokens":10}}"#;
    Mock::given(method("POST"))
        .and(path("/v1/images/generations"))
        .and(body_partial_json(json!({"model": "deepseek-chat"})))
        .respond_with(ResponseTemplate::new(200).set_body_raw(image, "application/json"))
        .mount(&upstream)
        .await;
    Mock::given(method("POST"))
        .and(path("/v1/audio/speech"))
        .respond_with(ResponseTemplate::new(200).set_body_raw(vec![255_u8, 0, 1], "audio/mpeg"))
        .mount(&upstream)
        .await;
    Mock::given(method("POST"))
        .and(path("/v1/audio/transcriptions"))
        .respond_with(ResponseTemplate::new(200).set_body_raw("hello", "text/plain"))
        .mount(&upstream)
        .await;
    let generated = super::routes::image_generations(
        State(state.clone()),
        Ok(caller.clone()),
        Ok(Json(
            json!({"model": "deepseek/deepseek-chat", "prompt": "a cat"}),
        )),
    )
    .await;
    assert_eq!(generated.status(), StatusCode::OK);
    assert_eq!(
        axum::body::to_bytes(generated.into_body(), usize::MAX)
            .await
            .expect("image reads"),
        image.as_bytes()
    );
    let spoken = super::routes::audio_speech(
        State(state.clone()),
        Ok(caller.clone()),
        Ok(Json(
            json!({"model": "deepseek/deepseek-chat", "input": "hi", "voice": "alloy"}),
        )),
    )
    .await;
    assert_eq!(spoken.status(), StatusCode::OK);
    assert_eq!(spoken.headers()["content-type"], "audio/mpeg");
    assert_eq!(
        axum::body::to_bytes(spoken.into_body(), usize::MAX)
            .await
            .expect("audio reads")
            .as_ref(),
        [255_u8, 0, 1]
    );
    let form = |file: &str| {
        let mut headers = axum::http::HeaderMap::new();
        headers.insert(
            axum::http::header::CONTENT_TYPE,
            axum::http::HeaderValue::from_static("multipart/form-data; boundary=wyrd"),
        );
        let mut body = b"--wyrd\r\nContent-Disposition: form-data; name=\"model\"\r\n\r\ndeepseek/deepseek-chat\r\n".to_vec();
        body.extend_from_slice(format!("--wyrd\r\nContent-Disposition: form-data; name=\"{file}\"; filename=\"a.wav\"\r\nContent-Type: audio/wav\r\n\r\n").as_bytes());
        body.extend_from_slice(&[0, 159, 146, 150]);
        body.extend_from_slice(b"\r\n--wyrd--\r\n");
        (headers, axum::body::Bytes::from(body))
    };
    let (headers, body) = form("file");
    let transcribed = super::routes::audio_transcriptions(
        State(state.clone()),
        Ok(caller.clone()),
        headers,
        Body::from(body),
    )
    .await;
    assert_eq!(transcribed.status(), StatusCode::OK);
    assert_eq!(transcribed.headers()["content-type"], "text/plain");
    let requests = upstream.received_requests().await.expect("recording");
    let sent = &requests.last().expect("transcription dispatched").body;
    assert!(
        sent.windows(4).any(|window| window == [0, 159, 146, 150])
            && String::from_utf8_lossy(sent).contains("name=\"model\"\r\n\r\ndeepseek-chat\r\n"),
        "the form reaches the provider with its file and native model"
    );

    // Audio uploads stream past the general body limit under their own bound:
    // a 2 MiB file arrives in chunks and reaches the provider byte for byte,
    // while an upload over the Audio bound or a body that breaks mid-upload
    // is refused before any provider request.
    let audio = state.clone().with_limits(LimitsConfig {
        audio_upload_bytes: 3 * 1024 * 1024,
        ..state.limits
    });
    let big_form = |size: usize| {
        let mut headers = axum::http::HeaderMap::new();
        headers.insert(
            axum::http::header::CONTENT_TYPE,
            axum::http::HeaderValue::from_static("multipart/form-data; boundary=wyrd"),
        );
        let file: Vec<u8> = (0..size)
            .map(|i| u8::try_from(i % 251).expect("below 251"))
            .collect();
        let mut body = b"--wyrd\r\nContent-Disposition: form-data; name=\"model\"\r\n\r\ndeepseek/deepseek-chat\r\n--wyrd\r\nContent-Disposition: form-data; name=\"file\"; filename=\"big.wav\"\r\nContent-Type: audio/wav\r\n\r\n".to_vec();
        body.extend_from_slice(&file);
        body.extend_from_slice(b"\r\n--wyrd--\r\n");
        let chunks: Vec<Result<axum::body::Bytes, std::io::Error>> = body
            .chunks(64 * 1024)
            .map(|chunk| Ok(axum::body::Bytes::copy_from_slice(chunk)))
            .collect();
        (headers, file, chunks)
    };
    let (headers, file, chunks) = big_form(2 * 1024 * 1024);
    assert!(
        file.len() > audio.limits.body_bytes,
        "the upload exceeds the general limit"
    );
    let streamed = super::routes::audio_transcriptions(
        State(audio.clone()),
        Ok(caller.clone()),
        headers,
        Body::from_stream(futures_util::stream::iter(chunks)),
    )
    .await;
    assert_eq!(streamed.status(), StatusCode::OK);
    let requests = upstream.received_requests().await.expect("recording");
    let sent = &requests
        .last()
        .expect("streamed transcription dispatched")
        .body;
    let header_end = sent
        .windows(18)
        .position(|window| window == b"filename=\"big.wav\"")
        .and_then(|at| {
            sent[at..]
                .windows(4)
                .position(|window| window == b"\r\n\r\n")
                .map(|end| at + end + 4)
        })
        .expect("file part header");
    let received = sent
        .get(header_end..header_end + file.len())
        .expect("whole file");
    assert_eq!(received.len(), file.len());
    assert_eq!(
        Sha256::digest(received),
        Sha256::digest(&file),
        "the file arrives intact"
    );
    let dispatched = requests.len();
    let (headers, _, chunks) = big_form(4 * 1024 * 1024);
    let oversized = super::routes::audio_transcriptions(
        State(audio.clone()),
        Ok(caller.clone()),
        headers,
        Body::from_stream(futures_util::stream::iter(chunks)),
    )
    .await;
    assert_eq!(oversized.status(), StatusCode::PAYLOAD_TOO_LARGE);
    let (headers, _, mut chunks) = big_form(1024 * 1024);
    chunks.truncate(4);
    chunks.push(Err(std::io::Error::other("caller went away")));
    let broken = super::routes::audio_translations(
        State(audio.clone()),
        Ok(caller.clone()),
        headers,
        Body::from_stream(futures_util::stream::iter(chunks)),
    )
    .await;
    assert_eq!(broken.status(), StatusCode::BAD_REQUEST);
    assert_eq!(
        upstream.received_requests().await.expect("recording").len(),
        dispatched,
        "oversized and broken Audio uploads never dispatch"
    );
    let (headers, body) = form("mask");
    let missing =
        super::routes::image_edits(State(state.clone()), Ok(caller.clone()), headers, Ok(body))
            .await;
    assert_eq!(missing.status(), StatusCode::BAD_REQUEST);
    assert_eq!(body_json(missing).await["error"]["param"], "image");
    assert_eq!(
        upstream.received_requests().await.expect("recording").len(),
        dispatched,
        "an edit without an image never dispatches"
    );

    let limited = super::routes::chat_completions(
        State(state.clone()),
        Ok(caller.clone()),
        HeaderMap::new(),
        Ok(Json(chat(json!({"user": "limited"})))),
    )
    .await;
    assert_eq!(limited.status(), StatusCode::TOO_MANY_REQUESTS);
    assert_eq!(
        body_json(limited).await,
        json!({"error": {"message": "slow down", "type": "rate_limit"}})
    );

    let dispatched = upstream.received_requests().await.expect("recording").len();
    let unrepresentable = GatewayInvocation::new(&state)
        .invoke(
            &caller,
            call(
                GatewayOperation::ChatCompletions,
                chat(json!({"stream": true})),
            ),
        )
        .await
        .expect_err("a body stream member must match the call");
    assert!(
        matches!(&unrepresentable, WyrdError::GatewayInvalidRequest { details, .. } if details["field"] == "stream"),
        "{unrepresentable:?}"
    );
    let undeclared = GatewayInvocation::new(&state)
        .invoke(
            &caller,
            call(
                GatewayOperation::Batches,
                json!({"model": "deepseek/deepseek-chat", "input_file_id": "f"}),
            ),
        )
        .await
        .expect_err("batches are not declared");
    assert!(
        matches!(undeclared, WyrdError::GatewayModelUnavailable { .. }),
        "{undeclared:?}"
    );
    assert_eq!(
        upstream.received_requests().await.expect("recording").len(),
        dispatched,
        "rejections never dispatch"
    );

    // A provider that sends one frame and then holds the stream open: dropping
    // the public body aborts its connection, and the call is still accounted
    // with unknown usage because no terminal usage arrived.
    let frame =
        "data: {\"id\":\"ds-3\",\"choices\":[{\"index\":0,\"delta\":{\"content\":\"h\"}}]}\n\n";
    let held = |name: &'static str| {
        let gateway = &gateway;
        let admin = &admin;
        async move {
            let (port, upstream, _) = held_upstream(frame).await;
            gateway
                .put_deployment(
                    admin,
                    &ProviderDeploymentName::new(name).expect("name"),
                    serde_json::from_value(json!({
                        "name": name,
                        "model": model(&format!("deepseek/{name}")),
                        "adapter": {"openai_compatible": {"base_url": format!("http://127.0.0.1:{port}/v1")}},
                        "auth": {"bearer": {"credential": "deepseek-key"}},
                        "capabilities": ["chat_completions"],
                        "routing_weight": 1,
                    }))
                    .expect("deployment decodes"),
                )
                .await
                .expect("deployment stores");
            (name, upstream)
        }
    };
    let open = |name: &str| {
        super::routes::chat_completions(
            State(state.clone()),
            Ok(caller.clone()),
            HeaderMap::new(),
            Ok(Json(json!({
                "model": format!("deepseek/{name}"),
                "messages": [{"role": "user", "content": "hi"}],
                "stream": true,
            }))),
        )
    };
    let (_, hanging) = held("deepseek-hang").await;
    let dropped = open("deepseek-hang").await;
    assert_eq!(dropped.status(), StatusCode::OK);
    let mut body = dropped.into_body().into_data_stream();
    let first = futures_util::StreamExt::next(&mut body)
        .await
        .expect("one frame")
        .expect("frame reads");
    assert_eq!(first, frame.as_bytes());
    drop(body);
    assert!(
        hanging.await.expect("upstream task"),
        "dropping the public body closes the upstream connection"
    );
    // Accounting commits after the relay ends, on the gateway task tracker
    // that shutdown drain waits for: once it drains, the evidence is present.
    drain_gateway(&state).await;
    let mut conn = fixture.tenant_conn_for(tenant).await.expect("tenant conn");
    let documents: Vec<Value> = sqlx::query_scalar(
        "SELECT entry FROM wyrd.gateway_accounting_entries WHERE kind = 'attempt_accounted'",
    )
    .fetch_all(&mut **conn.transaction())
    .await
    .expect("attempts read");
    conn.commit().await.expect("attempts read commits");
    let attempt = documents
        .into_iter()
        .map(|document| serde_json::from_value::<GatewayAccountingEntryV1>(document).expect("entry"))
        .find(|entry| {
            matches!(entry, GatewayAccountingEntryV1::AttemptAccounted { model: served, .. } if *served == model("deepseek/deepseek-hang"))
        })
        .expect("drained accounting recorded the dropped stream's attempt");
    let GatewayAccountingEntryV1::AttemptAccounted { call_id, .. } = &attempt else {
        unreachable!("the attempt filter matched only attempt entries");
    };
    let call = entries(&fixture, tenant, *call_id)
        .await
        .into_iter()
        .find(|entry| matches!(entry, GatewayAccountingEntryV1::CallAccounted { .. }))
        .expect("drained accounting recorded the dropped stream's call");
    assert!(
        matches!(
            attempt,
            GatewayAccountingEntryV1::AttemptAccounted {
                provider_usage_json: None,
                normalized_usage: None,
                ..
            }
        ),
        "{attempt:?}"
    );
    assert!(
        matches!(
            call,
            GatewayAccountingEntryV1::CallAccounted {
                normalized_usage: None,
                ..
            }
        ),
        "{call:?}"
    );

    // Drain: an open public stream ends with the terminal error frame and
    // `[DONE]` and closes its upstream, and new calls are refused before any
    // dispatch.
    let (_, draining) = held("deepseek-drain").await;
    let open_stream = open("deepseek-drain").await;
    assert_eq!(open_stream.status(), StatusCode::OK);
    let mut body = open_stream.into_body().into_data_stream();
    let first = futures_util::StreamExt::next(&mut body)
        .await
        .expect("one frame")
        .expect("frame reads");
    assert_eq!(first, frame.as_bytes());
    state.shutdown_token.cancel();
    let mut rest = Vec::new();
    while let Some(chunk) = futures_util::StreamExt::next(&mut body).await {
        rest.extend_from_slice(&chunk.expect("terminated stream reads cleanly"));
    }
    let rest = String::from_utf8(rest).expect("utf-8");
    assert!(
        rest.contains("WYRD_SERVER_503_SERVICE_UNAVAILABLE") && rest.ends_with("data: [DONE]\n\n"),
        "{rest}"
    );
    assert!(
        draining.await.expect("upstream task"),
        "drain closes the upstream connection"
    );
    let dispatched = upstream.received_requests().await.expect("recording").len();
    let refused = open("deepseek-chat").await;
    assert_eq!(refused.status(), StatusCode::SERVICE_UNAVAILABLE);
    assert!(
        body_json(refused).await["error"]["code"] == "WYRD_SERVER_503_SERVICE_UNAVAILABLE",
        "a draining server admits no call"
    );
    assert_eq!(
        upstream.received_requests().await.expect("recording").len(),
        dispatched,
        "drain dispatches nothing new"
    );
    drain_gateway(&state).await;
}

/// Serves one streaming chat answer that sends `frame` and then holds the
/// connection open; the task reports whether the gateway closed it, and the
/// returned [`Notify`] fires once the whole request was received.
///
/// # Panics
///
/// Panics when the listener cannot bind, accept, read, or write.
async fn held_upstream(frame: &'static str) -> (u16, JoinHandle<bool>, Arc<Notify>) {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
        .await
        .expect("bind");
    let port = listener.local_addr().expect("addr").port();
    let received = Arc::new(Notify::new());
    let entered = Arc::clone(&received);
    let upstream = tokio::spawn(async move {
        let (mut socket, _) = listener.accept().await.expect("accept");
        let (mut request, mut buffer) = (Vec::new(), [0_u8; 4096]);
        loop {
            let read = socket.read(&mut buffer).await.expect("read");
            request.extend_from_slice(&buffer[..read]);
            let text = String::from_utf8_lossy(&request);
            if let Some(end) = text.find("\r\n\r\n") {
                let length = text[..end]
                    .lines()
                    .find_map(|line| {
                        line.to_ascii_lowercase()
                            .strip_prefix("content-length:")
                            .map(|value| value.trim().parse::<usize>().expect("length"))
                    })
                    .unwrap_or(0);
                if request.len() >= end + 4 + length {
                    break;
                }
            }
        }
        let head = format!(
            "HTTP/1.1 200 OK\r\ncontent-type: text/event-stream\r\ntransfer-encoding: chunked\r\n\r\n{:x}\r\n{frame}\r\n",
            frame.len()
        );
        entered.notify_one();
        socket.write_all(head.as_bytes()).await.expect("write");
        matches!(socket.read(&mut buffer).await, Ok(0) | Err(_))
    });
    (port, upstream, received)
}

/// Builds a replica over `fixture` whose engine dispatches over HTTP and whose
/// `deepseek-file` credential binding reads `secret` for `tenant`.
///
/// # Panics
///
/// Panics when the fixed binding name is invalid or the HTTP gateway dispatch
/// cannot be built.
async fn http_replica(fixture: &PgFixture, tenant: DataTenantId, secret: &Path) -> AppState {
    test_state(fixture)
        .await
        .with_gateway(crate::config::GatewayConfig {
            credential_bindings: std::collections::BTreeMap::from([(
                wyrd_spec::ids::CredentialBindingName::new("deepseek-file").expect("binding"),
                crate::config::GatewayCredentialBinding {
                    secret: wyrd_spec::security::SecretRef::File {
                        path: secret.display().to_string(),
                    },
                    assignment: wyrd_gateway::CredentialAssignment {
                        host: Some("127.0.0.1".to_owned()),
                        ..super::pg_administration_tests::assigned(tenant, "deepseek")
                    },
                },
            )]),
            secret_backends: std::collections::BTreeMap::new(),
            ..Default::default()
        })
        .with_gateway_engine(GatewayEngine::new(
            CredentialResolver::default(),
            DeploymentHealth::default(),
            Arc::new(
                wyrd_gateway::HttpProviderDispatch::new(
                    skald_providers::EndpointPolicy::new(false),
                    wyrd_gateway::BuiltinEndpoints::default(),
                )
                .expect("gateway dispatch builds"),
            ),
        ))
}

/// Proves the Batches lifecycle across two replicas and tenants: an upload
/// reaches the provider with native models and pins its deployment; a
/// creation replayed while pending conflicts, and replayed once created on
/// another replica returns the same batch without a second provider batch; a
/// provider refusal releases its claim; reads, lists, cancellation, output
/// content, and file deletion use Wyrd ids; and another tenant sees nothing.
///
/// # Panics
///
/// Panics when the fixture, provider mock, or configuration cannot be set up,
/// or when a lifecycle, replay, isolation, or provider-request assertion
/// differs.
#[tokio::test]
async fn gateway_batches_converge_replays_on_one_upstream_batch() {
    let fixture = PgFixture::start().await.expect("fixture starts");
    let tenant = fixture.data_tenant_id();
    let upstream = MockServer::start().await;
    let provider_batch = |status: &str| {
        json!({"id": "b-up-1", "object": "batch", "status": status, "endpoint": "/v1/chat/completions",
               "input_file_id": "file-up-1", "output_file_id": "out-up-1", "error_file_id": null})
    };
    Mock::given(method("POST"))
        .and(path("/v1/files"))
        .respond_with(
            ResponseTemplate::new(200)
                .set_body_json(json!({"id": "file-up-1", "object": "file", "purpose": "batch"})),
        )
        .mount(&upstream)
        .await;
    Mock::given(method("POST"))
        .and(path("/v1/batches"))
        .and(body_partial_json(json!({"completion_window": "bad"})))
        .respond_with(ResponseTemplate::new(400).set_body_json(
            json!({"error": {"message": "bad window", "type": "invalid_request_error"}}),
        ))
        .with_priority(1)
        .mount(&upstream)
        .await;
    Mock::given(method("POST"))
        .and(path("/v1/batches"))
        .and(body_partial_json(json!({"input_file_id": "file-up-1"})))
        .respond_with(
            ResponseTemplate::new(200)
                .set_body_json(provider_batch("validating"))
                .set_delay(Duration::from_secs(2)),
        )
        .mount(&upstream)
        .await;
    Mock::given(method("GET"))
        .and(path("/v1/batches/b-up-1"))
        .respond_with(ResponseTemplate::new(200).set_body_json(provider_batch("completed")))
        .mount(&upstream)
        .await;
    Mock::given(method("POST"))
        .and(path("/v1/batches/b-up-1/cancel"))
        .respond_with(ResponseTemplate::new(200).set_body_json(provider_batch("cancelling")))
        .mount(&upstream)
        .await;
    Mock::given(method("GET"))
        .and(path("/v1/files/out-up-1/content"))
        .respond_with(
            ResponseTemplate::new(200)
                .set_body_raw("{\"custom_id\":\"a\"}\n", "application/octet-stream"),
        )
        .mount(&upstream)
        .await;
    Mock::given(method("DELETE"))
        .and(path("/v1/files/file-up-1"))
        .respond_with(
            ResponseTemplate::new(200)
                .set_body_json(json!({"id": "file-up-1", "object": "file", "deleted": true})),
        )
        .mount(&upstream)
        .await;

    let secret = tempfile::NamedTempFile::new().expect("secret file");
    std::fs::write(secret.path(), "sk-deepseek\n").expect("secret writes");
    let a = http_replica(&fixture, tenant, secret.path()).await;
    let b = http_replica(&fixture, tenant, secret.path()).await;
    let admin = admin(tenant);
    let gateway = GatewayAdministration::new(&a);
    gateway
        .put_credential(
            &admin,
            &wyrd_spec::ids::ProviderCredentialName::new("deepseek-key").expect("name"),
            serde_json::from_value(json!({
                "name": "deepseek-key", "provider": "deepseek",
                "source": {"environment": {"binding": "deepseek-file"}}
            }))
            .expect("credential decodes"),
        )
        .await
        .expect("credential stores");
    gateway
        .put_deployment(
            &admin,
            &ProviderDeploymentName::new("deepseek-batch").expect("name"),
            serde_json::from_value(json!({
                "name": "deepseek-batch",
                "model": model("deepseek/deepseek-chat"),
                "adapter": {"openai_compatible": {"base_url": format!("{}/v1", upstream.uri())}},
                "auth": {"bearer": {"credential": "deepseek-key"}},
                "capabilities": ["batches"],
                "routing_weight": 1,
            }))
            .expect("deployment decodes"),
        )
        .await
        .expect("deployment stores");
    let caller = invoker(
        tenant,
        7,
        [Permission::gateway_invoke(GatewayAccess::Provider {
            provider: ProviderId::new("deepseek").expect("provider"),
        })],
    );
    let received = |route: &'static str, verb: &'static str| {
        let upstream = &upstream;
        async move {
            upstream
                .received_requests()
                .await
                .expect("recording")
                .into_iter()
                .filter(|request| request.url.path() == route && request.method.as_str() == verb)
                .count()
        }
    };

    // Upload: the batch JSONL reaches the provider with native models.
    let line = |id: &str, model: &str| {
        json!({"custom_id": id, "method": "POST", "url": "/v1/chat/completions",
               "body": {"model": model, "messages": [{"role": "user", "content": "hi"}]}})
        .to_string()
    };
    let form = |jsonl: &str| {
        let mut headers = axum::http::HeaderMap::new();
        headers.insert(
            axum::http::header::CONTENT_TYPE,
            axum::http::HeaderValue::from_static("multipart/form-data; boundary=wyrd"),
        );
        let body = format!(
            "--wyrd\r\nContent-Disposition: form-data; name=\"purpose\"\r\n\r\nbatch\r\n--wyrd\r\nContent-Disposition: form-data; name=\"file\"; filename=\"in.jsonl\"\r\nContent-Type: application/jsonl\r\n\r\n{jsonl}\r\n--wyrd--\r\n"
        );
        (headers, axum::body::Bytes::from(body))
    };
    let mixed = format!(
        "{}\n{}",
        line("a", "deepseek/deepseek-chat"),
        line("b", "deepseek/other")
    );
    let (headers, body) = form(&mixed);
    let rejected =
        super::routes::upload_file(State(a.clone()), Ok(caller.clone()), headers, Ok(body)).await;
    assert_eq!(rejected.status(), StatusCode::BAD_REQUEST);
    assert_eq!(
        body_json(rejected).await["error"]["param"],
        "file[1].body.model"
    );
    assert_eq!(
        received("/v1/files", "POST").await,
        0,
        "invalid input never uploads"
    );
    let jsonl = format!(
        "{}\n{}",
        line("a", "deepseek/deepseek-chat"),
        line("b", "deepseek/deepseek-chat")
    );
    let (headers, body) = form(&jsonl);
    let uploaded =
        super::routes::upload_file(State(a.clone()), Ok(caller.clone()), headers, Ok(body)).await;
    assert_eq!(uploaded.status(), StatusCode::OK);
    let file = body_json(uploaded).await;
    let file_id = file["id"].as_str().expect("file id").to_owned();
    assert!(
        file_id.starts_with("file-") && file_id != "file-up-1",
        "{file}"
    );
    assert_eq!(file["bytes"], jsonl.len());
    let sent = upstream
        .received_requests()
        .await
        .expect("recording")
        .into_iter()
        .find(|request| request.url.path() == "/v1/files")
        .expect("upload dispatched");
    let sent = String::from_utf8_lossy(&sent.body);
    assert!(
        sent.contains("\"model\":\"deepseek-chat\"") && !sent.contains("deepseek/deepseek-chat"),
        "lines reach the provider with native models: {sent}"
    );

    // A refused creation releases its claim.
    let create = |state: &AppState, window: &str| {
        super::routes::create_batch(
            State(state.clone()),
            Ok(caller.clone()),
            Ok(Json(json!({
                "input_file_id": file_id,
                "endpoint": "/v1/chat/completions",
                "completion_window": window,
            }))),
        )
    };
    let refused = create(&a, "bad").await;
    assert_eq!(refused.status(), StatusCode::BAD_REQUEST);
    let pending = || async {
        let mut conn = fixture.tenant_conn_for(tenant).await.expect("tenant conn");
        let rows: Vec<Option<String>> =
            sqlx::query_scalar("SELECT upstream_batch_id FROM wyrd.gateway_batches")
                .fetch_all(&mut **conn.transaction())
                .await
                .expect("batches read");
        conn.commit().await.expect("read commits");
        rows
    };
    assert!(
        pending().await.is_empty(),
        "a provider refusal releases its claim"
    );

    // Replica A's creation is pending: B's identical replay conflicts without
    // dispatch; once created, B's replay returns the same batch.
    let first = tokio::spawn(create(&a, "24h"));
    tokio::time::timeout(Duration::from_secs(10), async {
        while pending().await != [None] {
            tokio::task::yield_now().await;
        }
    })
    .await
    .expect("the creation claims its fence");
    let conflict = create(&b, "24h").await;
    assert_eq!(conflict.status(), StatusCode::CONFLICT);
    assert_eq!(
        body_json(conflict).await["error"]["code"],
        "WYRD_GATEWAY_409_RESOURCE_CONFLICT"
    );
    let created = first.await.expect("creation task");
    assert_eq!(created.status(), StatusCode::OK);
    let created = body_json(created).await;
    let batch_id = created["id"].as_str().expect("batch id").to_owned();
    assert!(batch_id.starts_with("batch_"), "{created}");
    assert_eq!(created["input_file_id"], file_id.as_str());
    assert_eq!(created["status"], "validating");
    let replayed = create(&b, "24h").await;
    assert_eq!(replayed.status(), StatusCode::OK);
    let replayed = body_json(replayed).await;
    assert_eq!(replayed["id"], batch_id.as_str());
    assert_eq!(
        replayed["status"], "completed",
        "a replay returns the refreshed batch"
    );
    assert_eq!(
        received("/v1/batches", "POST").await,
        2,
        "one refused and one created provider batch; the replays created none"
    );

    // Reads, list, output content, and cancellation use Wyrd ids.
    let read = super::routes::get_batch(
        State(b.clone()),
        Ok(caller.clone()),
        axum::extract::Path(batch_id.clone()),
    )
    .await;
    assert_eq!(read.status(), StatusCode::OK);
    let read = body_json(read).await;
    let output = format!("file-{}-output", batch_id.trim_start_matches("batch_"));
    assert_eq!(read["output_file_id"], output.as_str());
    assert_eq!(read["error_file_id"], Value::Null);
    let listed = super::routes::list_batches(
        State(a.clone()),
        Ok(caller.clone()),
        Ok(axum::extract::Query(
            wyrd_spec::gateway::openai::GatewayBatchListQuery {
                after: None,
                limit: None,
            },
        )),
    )
    .await;
    let listed = body_json(listed).await;
    assert_eq!(listed["data"].as_array().map(Vec::len), Some(1), "{listed}");
    assert_eq!(listed["data"][0]["id"], batch_id.as_str());
    assert_eq!(listed["has_more"], false);
    let content = super::routes::file_content(
        State(a.clone()),
        Ok(caller.clone()),
        axum::extract::Path(output),
    )
    .await;
    assert_eq!(content.status(), StatusCode::OK);
    assert_eq!(
        axum::body::to_bytes(content.into_body(), usize::MAX)
            .await
            .expect("content reads"),
        "{\"custom_id\":\"a\"}\n".as_bytes()
    );
    let cancelled = super::routes::cancel_batch(
        State(a.clone()),
        Ok(caller.clone()),
        axum::extract::Path(batch_id.clone()),
    )
    .await;
    assert_eq!(body_json(cancelled).await["status"], "cancelling");

    // Another tenant sees neither the batch nor the file.
    let other = fixture
        .seed_additional_tenant("gateway-batches-other")
        .await
        .expect("tenant b");
    let outsider = invoker(
        other,
        9,
        [Permission::gateway_invoke(GatewayAccess::Provider {
            provider: ProviderId::new("deepseek").expect("provider"),
        })],
    );
    let hidden = super::routes::get_batch(
        State(a.clone()),
        Ok(outsider.clone()),
        axum::extract::Path(batch_id.clone()),
    )
    .await;
    assert_eq!(hidden.status(), StatusCode::NOT_FOUND);
    let hidden_file = super::routes::get_file(
        State(a.clone()),
        Ok(outsider.clone()),
        axum::extract::Path(file_id.clone()),
    )
    .await;
    assert_eq!(hidden_file.status(), StatusCode::NOT_FOUND);
    let empty = super::routes::list_batches(
        State(a.clone()),
        Ok(outsider),
        Ok(axum::extract::Query(
            wyrd_spec::gateway::openai::GatewayBatchListQuery {
                after: None,
                limit: None,
            },
        )),
    )
    .await;
    assert_eq!(body_json(empty).await["data"], json!([]));

    // Deleting the input file keeps its batch.
    let metadata = super::routes::get_file(
        State(b.clone()),
        Ok(caller.clone()),
        axum::extract::Path(file_id.clone()),
    )
    .await;
    assert_eq!(body_json(metadata).await["filename"], "in.jsonl");
    let deleted = super::routes::delete_file(
        State(b.clone()),
        Ok(caller.clone()),
        axum::extract::Path(file_id.clone()),
    )
    .await;
    assert_eq!(
        body_json(deleted).await,
        json!({"id": file_id, "object": "file", "deleted": true})
    );
    let gone = super::routes::get_file(
        State(a.clone()),
        Ok(caller.clone()),
        axum::extract::Path(file_id.clone()),
    )
    .await;
    assert_eq!(gone.status(), StatusCode::NOT_FOUND);
    let kept =
        super::routes::get_batch(State(a.clone()), Ok(caller), axum::extract::Path(batch_id)).await;
    assert_eq!(kept.status(), StatusCode::OK);
    drain_gateway(&a).await;
    drain_gateway(&b).await;
}

/// Stores the `deepseek-batch` deployment serving `deepseek/deepseek-chat`
/// Batches from `base_url` through `replica`, and returns a caller that may
/// invoke every `deepseek` model.
///
/// # Panics
///
/// Panics when the credential or deployment cannot be stored.
async fn batch_deployment(replica: &AppState, tenant: DataTenantId, base_url: &str) -> Caller {
    let admin = admin(tenant);
    let gateway = GatewayAdministration::new(replica);
    gateway
        .put_credential(
            &admin,
            &wyrd_spec::ids::ProviderCredentialName::new("deepseek-key").expect("name"),
            serde_json::from_value(json!({
                "name": "deepseek-key", "provider": "deepseek",
                "source": {"environment": {"binding": "deepseek-file"}}
            }))
            .expect("credential decodes"),
        )
        .await
        .expect("credential stores");
    gateway
        .put_deployment(
            &admin,
            &ProviderDeploymentName::new("deepseek-batch").expect("name"),
            serde_json::from_value(json!({
                "name": "deepseek-batch",
                "model": model("deepseek/deepseek-chat"),
                "adapter": {"openai_compatible": {"base_url": base_url}},
                "auth": {"bearer": {"credential": "deepseek-key"}},
                "capabilities": ["batches"],
                "routing_weight": 1,
            }))
            .expect("deployment decodes"),
        )
        .await
        .expect("deployment stores");
    invoker(
        tenant,
        7,
        [Permission::gateway_invoke(GatewayAccess::Provider {
            provider: ProviderId::new("deepseek").expect("provider"),
        })],
    )
}

/// Batch creations release their replay claim exactly when no provider can
/// have created the batch: a drain refusal on one replica and an unusable
/// credential both leave nothing pending, so the identical creation succeeds
/// with one provider batch, while a provider `5xx` keeps the claim and the
/// identical replay conflicts.
///
/// # Panics
///
/// Panics when the fixture, provider mock, or configuration cannot be set up,
/// or when a status, claim, or provider-request assertion differs.
#[tokio::test]
async fn gateway_batch_creations_release_claims_only_without_dispatch() {
    let _telemetry = wyrd_telemetry::init(wyrd_telemetry::TelemetryConfig {
        filter: "info,wyrd_server::components::gateway=debug".to_owned(),
        ..Default::default()
    })
    .expect("gateway test tracing installs");
    let fixture = PgFixture::start().await.expect("fixture starts");
    let tenant = fixture.data_tenant_id();
    let upstream = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/v1/files"))
        .respond_with(
            ResponseTemplate::new(200)
                .set_body_json(json!({"id": "file-up-1", "object": "file", "purpose": "batch"})),
        )
        .mount(&upstream)
        .await;
    Mock::given(method("POST"))
        .and(path("/v1/batches"))
        .and(body_partial_json(json!({"metadata": {"n": "5xx"}})))
        .respond_with(
            ResponseTemplate::new(500)
                .set_body_json(json!({"error": {"message": "overloaded", "type": "server_error"}})),
        )
        .with_priority(1)
        .mount(&upstream)
        .await;
    Mock::given(method("POST"))
        .and(path("/v1/batches"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "id": "b-up-1", "object": "batch", "status": "validating",
            "endpoint": "/v1/chat/completions", "input_file_id": "file-up-1",
        })))
        .mount(&upstream)
        .await;
    let secret = tempfile::NamedTempFile::new().expect("secret file");
    std::fs::write(secret.path(), "sk-deepseek\n").expect("secret writes");
    let scribe = Arc::new(RecordingScribe::default());
    let a = scribe.record(http_replica(&fixture, tenant, secret.path()).await);
    let b = scribe.record(http_replica(&fixture, tenant, secret.path()).await);
    let c = scribe.record(http_replica(&fixture, tenant, secret.path()).await);
    let d = scribe.record(http_replica(&fixture, tenant, secret.path()).await);
    let caller = batch_deployment(&b, tenant, &format!("{}/v1", upstream.uri())).await;
    let line = json!({"custom_id": "a", "method": "POST", "url": "/v1/chat/completions",
        "body": {"model": "deepseek/deepseek-chat", "messages": [{"role": "user", "content": "hi"}]}});
    let mut headers = axum::http::HeaderMap::new();
    headers.insert(
        axum::http::header::CONTENT_TYPE,
        axum::http::HeaderValue::from_static("multipart/form-data; boundary=wyrd"),
    );
    let form = format!(
        "--wyrd\r\nContent-Disposition: form-data; name=\"purpose\"\r\n\r\nbatch\r\n--wyrd\r\nContent-Disposition: form-data; name=\"file\"; filename=\"in.jsonl\"\r\nContent-Type: application/jsonl\r\n\r\n{line}\r\n--wyrd--\r\n"
    );
    let uploaded = super::routes::upload_file(
        State(b.clone()),
        Ok(caller.clone()),
        headers,
        Ok(axum::body::Bytes::from(form)),
    )
    .await;
    assert_eq!(uploaded.status(), StatusCode::OK);
    let file_id = body_json(uploaded).await["id"]
        .as_str()
        .expect("file id")
        .to_owned();
    let create = |state: &AppState, n: &str| {
        super::routes::create_batch(
            State(state.clone()),
            Ok(caller.clone()),
            Ok(Json(json!({
                "input_file_id": file_id, "endpoint": "/v1/chat/completions",
                "completion_window": "24h", "metadata": {"n": n},
            }))),
        )
    };
    let pending = || async {
        let mut conn = fixture.tenant_conn_for(tenant).await.expect("tenant conn");
        let count: i64 = sqlx::query_scalar(
            "SELECT count(*) FROM wyrd.gateway_batches WHERE upstream_batch_id IS NULL",
        )
        .fetch_one(&mut **conn.transaction())
        .await
        .expect("claims read");
        conn.commit().await.expect("read commits");
        count
    };
    let created = || async {
        upstream
            .received_requests()
            .await
            .expect("recording")
            .into_iter()
            .filter(|request| request.url.path() == "/v1/batches")
            .count()
    };

    // A draining replica refuses before dispatch; the replay on B creates once.
    a.shutdown_token.cancel();
    let drained = create(&a, "drain").await;
    assert_eq!(drained.status(), StatusCode::SERVICE_UNAVAILABLE);
    assert_eq!(pending().await, 0, "a drain refusal releases its claim");
    assert_eq!(create(&b, "drain").await.status(), StatusCode::OK);
    assert_eq!(created().await, 1);

    // A credential readable by others is refused before dispatch and cools B's deployment;
    // once restored, the identical creation on C creates once.
    std::fs::set_permissions(secret.path(), Permissions::from_mode(0o644))
        .expect("secret becomes readable by others");
    let unusable = create(&b, "credential").await;
    assert!(unusable.status().is_server_error(), "{}", unusable.status());
    assert_eq!(
        pending().await,
        0,
        "a credential failure releases its claim"
    );
    assert_eq!(created().await, 1, "no provider request was sent");
    std::fs::set_permissions(secret.path(), Permissions::from_mode(0o600))
        .expect("secret becomes owner-only");
    let restored = create(&c, "credential").await;
    let status = restored.status();
    assert_eq!(status, StatusCode::OK, "{}", body_json(restored).await);
    assert_eq!(created().await, 2);

    // A provider 5xx may have created the batch: the claim stays and the
    // identical replay conflicts without another provider request.
    let overloaded = create(&c, "5xx").await;
    assert_eq!(overloaded.status(), StatusCode::INTERNAL_SERVER_ERROR);
    assert_eq!(pending().await, 1, "a provider 5xx keeps its claim");
    assert_eq!(create(&c, "5xx").await.status(), StatusCode::CONFLICT);
    assert_eq!(created().await, 3);

    // A denied creation is refused by its one audited decision before any
    // claim exists.
    drain_gateway(&a).await;
    drain_gateway(&b).await;
    drain_gateway(&c).await;
    let decisions = audit_decisions(&scribe, tenant).len();
    let denied = super::routes::create_batch(
        State(c.clone()),
        Ok(invoker(tenant, 9, [])),
        Ok(Json(json!({
            "input_file_id": file_id, "endpoint": "/v1/chat/completions",
            "completion_window": "24h", "metadata": {"n": "denied"},
        }))),
    )
    .await;
    assert_eq!(denied.status(), StatusCode::FORBIDDEN);
    assert_eq!(pending().await, 1, "a denial claims nothing");
    drain_gateway(&c).await;
    assert_eq!(audit_decisions(&scribe, tenant).len(), decisions + 1);

    // A caller that leaves after the claim commits and before dispatch, here
    // while admission on healthy replica D waits on the tenant's admission
    // lock, has its claim
    // released by tracked server work; the identical replay then creates once.
    let mut lock = fixture.tenant_conn_for(tenant).await.expect("tenant conn");
    lock_gateway_admission(&mut lock)
        .await
        .expect("admission lock");
    let abandoned = tokio::spawn(create(&d, "abandoned"));
    tokio::time::timeout(Duration::from_secs(10), async {
        while pending().await != 2 {
            tokio::task::yield_now().await;
        }
    })
    .await
    .expect("the creation claims its fence");
    abandoned.abort();
    assert!(abandoned.await.is_err_and(|error| error.is_cancelled()));
    lock.commit().await.expect("admission lock releases");
    drain_gateway(&d).await;
    assert_eq!(pending().await, 1, "the abandoned claim is released");
    assert_eq!(
        created().await,
        3,
        "the abandoned creation never dispatched"
    );
    let replayed = create(&d, "abandoned").await;
    let status = replayed.status();
    assert_eq!(status, StatusCode::OK, "{}", body_json(replayed).await);
    assert_eq!(created().await, 4);
    drain_gateway(&a).await;
    drain_gateway(&b).await;
    drain_gateway(&c).await;
    drain_gateway(&d).await;
}

/// Waits for every spawned gateway task — accounting and post-answer
/// capture — then reopens the tracker, and settles the audit and Scribe
/// outboxes.
///
/// Invocation audit and capture are non-blocking, so a decision row or a
/// captured frame exists only once its outbox has written it.
///
/// # Panics
///
/// Panics when the tracked work does not settle within ten seconds.
async fn drain_gateway(state: &AppState) {
    state.gateway_tasks.close();
    tokio::time::timeout(Duration::from_secs(10), state.gateway_tasks.wait())
        .await
        .expect("gateway tasks drain");
    state.gateway_tasks.reopen();
    let deadline = std::time::Instant::now() + Duration::from_secs(10);
    assert_eq!(
        state.scribe_outbox.settle(deadline).await,
        0,
        "audit settles"
    );
    assert_eq!(
        state.scribe_outbox.settle(deadline).await,
        0,
        "capture settles"
    );
}

/// Proves capture follows the admitted policy without changing the call:
/// `Disabled` submits nothing, `Metadata` writes one row plus one span per
/// attempt under the admitting request, and a pod reaching no Scribe drops
/// capture while the call still answers and accounts.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn gateway_capture_follows_policy_and_never_affects_the_call() {
    let fixture = PgFixture::start().await.expect("fixture starts");
    let tenant = fixture.data_tenant_id();
    let dispatch = Scripted::shared();
    let state = replica(&fixture, dispatch.clone()).await;
    configure(&state, tenant, json!([]), json!([]), "allow_unpriced").await;
    let caller = invoker(tenant, 1, [provider_access()]);
    let call = || request("acme/a", Some((1000, 500)), Duration::from_secs(10));

    let (disabled, scribe) = RecordingScribe::attach(replica(&fixture, dispatch.clone()).await);
    dispatch.push(Step::Return(completed(100, 50)));
    GatewayInvocation::new(&disabled)
        .invoke(&caller, call())
        .await
        .expect("disabled call completes");
    drain_gateway(&disabled).await;
    assert_eq!(scribe.attempts(), 0, "a disabled call submits nothing");

    GatewayAdministration::new(&state)
        .put_capture(
            &admin(tenant),
            GatewayCapturePolicyWrite {
                mode: GatewayCaptureMode::Metadata,
                payload_fields: BTreeSet::new(),
            },
        )
        .await
        .expect("metadata policy stores");

    let (captured, scribe) = RecordingScribe::attach(replica(&fixture, dispatch.clone()).await);
    dispatch.push(Step::Return(failed(1000, 0)));
    dispatch.push(Step::Return(completed(100, 50)));
    let response = GatewayInvocation::new(&captured)
        .invoke(&caller, call())
        .await
        .expect("captured call completes");
    drain_gateway(&captured).await;
    let received = scribe.received();
    assert_eq!(
        scribe.rows(),
        3,
        "one call row and two attempt spans deliver"
    );
    let tables = received
        .iter()
        .map(|frame| frame.table.as_str())
        .collect::<Vec<_>>();
    assert_eq!(
        tables,
        ["vala.gateway.calls", "vala.traces.spans"],
        "the call row delivers before its spans"
    );
    for frame in &received {
        assert_eq!(
            frame.request_id, caller.request_id,
            "{} delivers under the admitting request",
            frame.table
        );
        assert_eq!(frame.tenant, tenant);
    }
    assert!(
        entries(&fixture, tenant, response.call_id)
            .await
            .iter()
            .any(|entry| matches!(entry, GatewayAccountingEntryV1::CallAccounted { .. })),
        "capture never replaces accounting"
    );

    let unavailable = replica(&fixture, dispatch.clone()).await;
    dispatch.push(Step::Return(completed(100, 50)));
    let answered = GatewayInvocation::new(&unavailable)
        .invoke(&caller, call())
        .await
        .expect("a pod reaching no Scribe never fails the call");
    drain_gateway(&unavailable).await;
    assert!(
        entries(&fixture, tenant, answered.call_id)
            .await
            .iter()
            .any(|entry| matches!(entry, GatewayAccountingEntryV1::CallAccounted { .. }))
    );
}

/// Proves a call's Run and Card attribution is authorized before dispatch
/// and recorded on its captured row.
///
/// A caller bound to no Card attributes a registered Agent: the call row's
/// `card_ref` names that Agent with its registry UID, its `run_id` is the
/// Run, and Scribe receives the frame scoped to exactly that Card. An
/// unregistered Card is refused with `WYRD_VALA_403_CARD_UNRESOLVED`, and a
/// Card-bound Agent naming a Card outside its signed scope with
/// `WYRD_VALA_403_BIFROST_CARD_SCOPE`; neither refusal dispatches or
/// captures.
///
/// # Panics
///
/// Panics when the fixture, an answer, a refusal code, the dispatch count, or
/// a captured correlation value differs.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn gateway_correlation_is_authorized_before_dispatch_and_captured() {
    use arrow::array::AsArray as _;
    use wyrd_spec::reference::{CardRef, CardRefScope};
    use wyrd_spec::vala::ids::RunId;

    let fixture = PgFixture::start().await.expect("fixture starts");
    let tenant = fixture.data_tenant_id();
    let dispatch = Scripted::shared();
    let (state, scribe) = RecordingScribe::attach(replica(&fixture, dispatch.clone()).await);
    configure(&state, tenant, json!([]), json!([]), "allow_unpriced").await;
    GatewayAdministration::new(&state)
        .put_capture(
            &admin(tenant),
            GatewayCapturePolicyWrite {
                mode: GatewayCaptureMode::Metadata,
                payload_fields: BTreeSet::new(),
            },
        )
        .await
        .expect("metadata policy stores");
    let agent: CardRef = "prod/Agent/support-agent@1.0.0".parse().expect("agent ref");
    let mut conn = fixture.tenant_conn_for(tenant).await.expect("tenant conn");
    wyrd_dev_fixtures::cards::seed_backing_card(&mut conn, &agent, Uuid::now_v7()).await;
    let uid = wyrd_sql::queries::cards::get_card_by_ref(
        &mut conn,
        agent.kind.clone(),
        agent.space.as_ref().expect("space"),
        &agent.name,
        &agent.version,
    )
    .await
    .expect("agent registered")
    .card_uid;
    conn.commit().await.expect("seed commits");
    let run = RunId::new();
    let call = |card: &CardRef| GatewayCallRequest {
        subject: Some(super::GatewayCallSubject {
            run_id: run.clone(),
            card: card.clone(),
        }),
        ..request("acme/a", Some((1000, 500)), Duration::from_secs(10))
    };
    let invocation = GatewayInvocation::new(&state);
    let unbound = invoker(tenant, 1, [provider_access()]);

    dispatch.push(Step::Return(completed(100, 50)));
    invocation
        .invoke(&unbound, call(&agent))
        .await
        .expect("an attributed call completes");
    drain_gateway(&state).await;
    let received = scribe.received();
    let calls = received
        .iter()
        .find(|frame| frame.table == "vala.gateway.calls")
        .expect("the call row delivers");
    assert_eq!(
        calls.card_scope,
        std::slice::from_ref(&uid),
        "Scribe stamps card_uid from exactly the attributed Agent"
    );
    let row =
        arrow::ipc::reader::StreamReader::try_new(std::io::Cursor::new(calls.ipc.clone()), None)
            .expect("the call stream opens")
            .next()
            .expect("one batch")
            .expect("the batch decodes");
    let text = |name: &str| {
        row.column_by_name(name)
            .expect(name)
            .as_string::<i32>()
            .value(0)
            .to_owned()
    };
    assert_eq!(text("card_uid"), uid.as_str());
    assert_eq!(text("run_id"), run.as_str());

    let unregistered: CardRef = "prod/Agent/ghost@1.0.0".parse().expect("ghost ref");
    let scoped = Caller {
        principal: Principal::new(
            principal(2),
            PrincipalKind::Agent {
                card_ref: unregistered.clone(),
                card_ref_scope: CardRefScope::own(&unregistered),
            },
            tenant,
            Vec::<RoleRef>::new(),
            PermissionSet::from_iter([provider_access()]),
        ),
        ..invoker(tenant, 2, [])
    };
    for (caller, card, code) in [
        (&unbound, &unregistered, "WYRD_VALA_403_CARD_UNRESOLVED"),
        (&scoped, &agent, "WYRD_VALA_403_BIFROST_CARD_SCOPE"),
    ] {
        let refused = invocation
            .invoke(caller, call(card))
            .await
            .expect_err("an unauthorized attribution is refused");
        assert_eq!(refused.code(), code);
    }
    drain_gateway(&state).await;
    assert_eq!(
        dispatch.seen().len(),
        1,
        "no refused attribution dispatched"
    );
    assert_eq!(
        scribe
            .received()
            .iter()
            .filter(|frame| frame.table == "vala.gateway.calls")
            .count(),
        1,
        "no refused attribution captured"
    );
}

/// Builds a gateway replica whose Bifrost catalog is bound to `fixture`'s own
/// Postgres.
///
/// The shared unit-test catalog lives over its own embedded fixture, so this
/// fixture's tenant does not exist there and cannot register a table. Capture
/// retrieval authorization resolves the tenant's registered `vala.gateway.calls`,
/// so it needs a catalog that shares the tenant registry the rest of the state
/// uses.
async fn replica_with_fixture_catalog(
    fixture: &PgFixture,
    dispatch: Arc<dyn ProviderDispatch>,
) -> AppState {
    let postgres = Arc::new(crate::postgres::ServerPostgres::from_parts(
        fixture.wyrd_postgres().clone(),
        fixture.vala_postgres().clone(),
    ));
    let root = tempfile::tempdir()
        .expect("gateway storage tempdir")
        .keep()
        .join("gateway-storage");
    std::fs::create_dir_all(&root).expect("storage root creates");
    let storage = Arc::new(StorageHandle::new(BackendSigner::Local(
        LocalSigner::new(root).expect("local signer creates"),
    )));
    let bifrost_storage = Arc::new(vala_bifrost_redux::storage::BifrostStorage::new(
        Arc::clone(&storage),
        vala_bifrost_redux::storage::BifrostStoragePolicy::resolve(
            vala_bifrost_redux::storage::BifrostStorageConfig::default(),
            u64::from(u32::MAX),
            false,
        )
        .expect("the default storage policy is valid"),
        None,
    ));
    let catalog = Arc::new(
        vala_bifrost_redux::catalog::BifrostCatalog::new(
            secrecy::ExposeSecret::expose_secret(fixture.catalog_dsn()),
            bifrost_storage,
            fixture.vala_postgres().clone(),
        )
        .await
        .expect("the catalog builds against the fixture"),
    );
    crate::test_support::test_app_state(postgres, storage, catalog).with_gateway_engine(
        GatewayEngine::new(
            CredentialResolver::default(),
            DeploymentHealth::default(),
            dispatch,
        ),
    )
}

/// Exact bytes of the captured binary answer; the marker makes their absence
/// from Bifrost meaningful.
const PAYLOAD_OBJECT_BYTES: &[u8] = b"\x89PNG\r\n\x1a\ncaptured-object-bytes";

/// A completed attempt answering with `bytes` as binary media, carrying the
/// adapter's credential-free media capture of the same bytes.
fn media_completed(bytes: &[u8]) -> AttemptResult {
    let answer = MediaAnswer {
        content_type: "image/png".to_owned(),
        bytes: bytes.to_vec(),
    };
    AttemptResult::Completed {
        capture: Some(ResponseCapture::Media(answer.clone())),
        body: ResponseBody::Media(answer),
        usage: usage(100, 50),
    }
}

/// Proves the captured payload object's whole lifecycle under Revision 14:
/// a selected binary answer is persisted and resolvable byte-identically,
/// repeated identical content converges on one object, retrieval requires both
/// grants and audits every verdict, an absent, lifecycle-expired, or foreign
/// object returns one stable not-found outcome while its Bifrost reference
/// remains, replaced bytes and catalog or storage outages return the stable
/// unavailable error without a locator, and a storage failure drops the
/// capture without touching the call.
///
/// # Panics
///
/// Panics when a call fails, bytes differ, a retrieval verdict or status
/// differs, a locator is exposed, or a dropped capture publishes a row.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn gateway_payload_objects_are_authorized_convergent_and_stable_when_expired() {
    let fixture = PgFixture::start().await.expect("fixture starts");
    let tenant = fixture.data_tenant_id();
    let dispatch = Scripted::shared();
    let state = replica_with_fixture_catalog(&fixture, dispatch.clone()).await;
    configure(&state, tenant, json!([]), json!([]), "allow_unpriced").await;
    GatewayAdministration::new(&state)
        .put_capture(
            &admin(tenant),
            GatewayCapturePolicyWrite {
                mode: GatewayCaptureMode::Payload,
                payload_fields: BTreeSet::from([GatewayPayloadField::Response]),
            },
        )
        .await
        .expect("payload policy stores");
    let caller = invoker(tenant, 1, [provider_access()]);
    let (state, scribe) = RecordingScribe::attach(state);
    let invocation = GatewayInvocation::new(&state);
    for _ in 0..2 {
        dispatch.push(Step::Return(media_completed(PAYLOAD_OBJECT_BYTES)));
        invocation
            .invoke(&caller, request("acme/a", None, Duration::from_secs(10)))
            .await
            .expect("the captured call completes");
    }
    drain_gateway(&state).await;

    let digest = format!(
        "sha256:{}",
        hex::encode(Sha256::digest(PAYLOAD_OBJECT_BYTES))
    );
    let published = scribe.published();
    assert!(
        published.contains(&digest),
        "the row carries the typed reference: {published}"
    );
    assert!(
        !published.contains("captured-object-bytes"),
        "no captured bytes reach Bifrost: {published}"
    );
    assert!(
        !published.contains("payload-objects") && !published.contains(&tenant.to_string()),
        "no storage path or backend locator reaches Bifrost: {published}"
    );

    let path = object_path(tenant, &digest).expect("the digest derives a key");
    let parent = path.full.rsplit_once('/').expect("the key has a parent").0;
    let prefix = wyrd_storage::tenant_path::validate(parent, tenant).expect("the prefix validates");
    assert_eq!(
        state
            .storage
            .list_objects(&prefix)
            .await
            .expect("objects list")
            .len(),
        1,
        "two calls carrying identical bytes converge on one stored object"
    );

    let table = TableRef::new(BifrostNamespace::Gateway, "calls");
    let catalog = state.bifrost.catalog().expect("catalog");
    // The recording Scribe stands in for the pod's own, so nothing registered
    // the capture destination; register it here so the retrieval path resolves
    // the same table a real tenant has.
    catalog
        .ensure_builtin(
            tenant,
            vala_bifrost_redux::tables::builtin_table("gateway", "calls").expect("built-in table"),
        )
        .await
        .expect("the calls table registers");
    let table_uid = catalog
        .table_uid(&table, tenant)
        .await
        .expect("a lookup resolves the registered table without provisioning");
    let scoped = Permission {
        resource: Resource::BifrostQuery,
        action: Action::Read,
        scope: table.permission_scope(&table_uid),
    };
    let fetch = |caller: Caller, digest: String| {
        let state = state.clone();
        async move {
            super::routes::gateway_payload_object(State(state), caller, axum::extract::Path(digest))
                .await
        }
    };
    let status = |result: Result<axum::response::Response, WyrdErrorResponse>| {
        result.expect_err("refused").into_response().status()
    };
    let unavailable = |result: Result<axum::response::Response, WyrdErrorResponse>| async move {
        let response = result.expect_err("refused").into_response();
        assert_eq!(response.status(), StatusCode::SERVICE_UNAVAILABLE);
        let body = axum::body::to_bytes(response.into_body(), usize::MAX)
            .await
            .expect("problem reads");
        let body = String::from_utf8_lossy(&body).into_owned();
        assert!(
            !body.contains("payload-objects") && !body.contains(&tenant.to_string()),
            "no storage locator is exposed: {body}"
        );
    };
    let reader = invoker(
        tenant,
        2,
        [Permission::gateway_payload_read(), scoped.clone()],
    );
    let response = fetch(reader.clone(), digest.clone())
        .await
        .expect("an authorized reader retrieves the object");
    assert_eq!(response.status(), StatusCode::OK);
    let bytes = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .expect("body reads");
    assert_eq!(
        bytes.as_ref(),
        PAYLOAD_OBJECT_BYTES,
        "retrieval is byte-identical"
    );

    state
        .storage
        .put_object(&path, b"replaced-object-bytes".to_vec())
        .await
        .expect("the stored bytes are replaced under the digest key");
    unavailable(fetch(reader.clone(), digest.clone()).await).await;
    state
        .storage
        .put_object(&path, PAYLOAD_OBJECT_BYTES.to_vec())
        .await
        .expect("the original bytes are restored");
    let foreign_bytes = b"another-tenant-object-bytes";
    let foreign_digest = format!("sha256:{}", hex::encode(Sha256::digest(foreign_bytes)));
    state
        .storage
        .put_object(
            &object_path(
                fixture
                    .seed_additional_tenant("payload-foreign")
                    .await
                    .expect("another tenant seeds"),
                &foreign_digest,
            )
            .expect("the digest derives a key"),
            foreign_bytes.to_vec(),
        )
        .await
        .expect("another tenant's object is stored");
    assert_eq!(
        status(fetch(reader.clone(), foreign_digest).await),
        StatusCode::NOT_FOUND,
        "another tenant's object is concealed as absent"
    );

    assert_eq!(
        status(
            fetch(
                invoker(tenant, 3, [Permission::gateway_payload_read()]),
                digest.clone()
            )
            .await
        ),
        StatusCode::FORBIDDEN,
        "table-scoped query access is required as well"
    );
    assert_eq!(
        status(fetch(invoker(tenant, 4, [scoped.clone()]), digest.clone()).await),
        StatusCode::FORBIDDEN,
        "tenant-wide payload read is required as well"
    );
    assert_eq!(
        status(fetch(reader.clone(), format!("sha256:{}", "b".repeat(64))).await),
        StatusCode::NOT_FOUND,
        "an object that was never captured is simply absent"
    );

    state
        .storage
        .delete_object(&path)
        .await
        .expect("the bucket lifecycle expires the object");
    assert_eq!(
        status(fetch(reader.clone(), digest.clone()).await),
        StatusCode::NOT_FOUND,
        "an expired object returns the same stable not-found outcome"
    );
    assert!(
        published.contains(&digest),
        "the Bifrost reference outlives the object it names"
    );
    let wyrd_storage::settings::BackendConfig::Local { root } = state.storage.backend_config()
    else {
        panic!("the test replica stores locally");
    };
    let unreadable = root.join(&path.full);
    std::os::unix::fs::symlink(&unreadable, &unreadable)
        .expect("a self-referencing link makes the object unreadable");
    unavailable(fetch(reader.clone(), digest.clone()).await).await;
    std::fs::remove_file(&unreadable).expect("the unreadable link is removed");
    let owner = fixture.superuser_pool().expect("superuser pool");
    sqlx::query("ALTER TABLE vala.bifrost_tables RENAME TO bifrost_tables_offline")
        .execute(&owner)
        .await
        .expect("the catalog table goes offline");
    // A node that already resolved the table keeps serving it from its
    // registration cache, so the outage is observed by a replica that has not.
    let cold = replica_with_fixture_catalog(&fixture, dispatch.clone()).await;
    unavailable(
        super::routes::gateway_payload_object(
            State(cold),
            reader.clone(),
            axum::extract::Path(digest.clone()),
        )
        .await,
    )
    .await;
    sqlx::query("ALTER TABLE vala.bifrost_tables_offline RENAME TO bifrost_tables")
        .execute(&owner)
        .await
        .expect("the catalog table is restored");

    drain_gateway(&state).await;
    let decisions = audit_decisions(&scribe, tenant);
    let retrievals = decisions
        .iter()
        .filter(|(operation, _)| operation == "gateway.payload_object.get")
        .collect::<Vec<_>>();
    assert!(
        retrievals
            .iter()
            .any(|(_, outcome)| outcome.eq_ignore_ascii_case("allowed")),
        "every allowed retrieval is audited: {decisions:?}"
    );
    assert!(
        retrievals
            .iter()
            .any(|(_, outcome)| outcome.eq_ignore_ascii_case("denied")),
        "every denied retrieval is audited: {decisions:?}"
    );

    let failing = replica(&fixture, dispatch.clone()).await;
    failing
        .storage
        .put_object(&prefix, b"not a directory".to_vec())
        .await
        .expect("the blocker writes where the object directory must go");
    let (failing, unpublished) = RecordingScribe::attach(failing);
    dispatch.push(Step::Return(media_completed(PAYLOAD_OBJECT_BYTES)));
    let unaffected = GatewayInvocation::new(&failing)
        .invoke(&caller, request("acme/a", None, Duration::from_secs(10)))
        .await
        .expect("unavailable storage never fails the call");
    drain_gateway(&failing).await;
    assert!(
        matches!(&unaffected.body, ResponseBody::Media(answer) if answer.bytes == PAYLOAD_OBJECT_BYTES),
        "the caller's answer is unchanged"
    );
    assert!(
        unpublished.attempts() == 0,
        "the whole capture is dropped before any row is submitted"
    );
    assert!(
        entries(&fixture, tenant, unaffected.call_id)
            .await
            .iter()
            .any(|entry| matches!(entry, GatewayAccountingEntryV1::CallAccounted { .. })),
        "accounting is unaffected"
    );
}

/// Proves selected generated speech reaches the governed object store: the
/// caller receives the relayed bytes unchanged, the published row carries one
/// typed reference, the object is byte-identical and retrievable through both
/// grants, and speech whose response is unselected keeps no object.
///
/// # Panics
///
/// Panics when a fixture fails, a call fails, bytes differ, the reference or
/// object is missing, or unselected speech is stored.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn gateway_selected_speech_persists_one_retrievable_object() {
    const SPOKEN: &[u8] = b"ID3\x04spoken-speech-bytes";
    const UNSELECTED: &[u8] = b"ID3\x04unselected-speech-bytes";
    let fixture = PgFixture::start().await.expect("fixture starts");
    let tenant = fixture.data_tenant_id();
    let upstream = MockServer::start().await;
    for (voice, bytes) in [("alloy", SPOKEN), ("echo", UNSELECTED)] {
        Mock::given(method("POST"))
            .and(path("/v1/audio/speech"))
            .and(body_partial_json(json!({"voice": voice})))
            .respond_with(ResponseTemplate::new(200).set_body_raw(bytes.to_vec(), "audio/mpeg"))
            .mount(&upstream)
            .await;
    }
    let (state, scribe) = RecordingScribe::attach(
        replica_with_fixture_catalog(
            &fixture,
            Arc::new(
                wyrd_gateway::HttpProviderDispatch::new(
                    skald_providers::EndpointPolicy::new(false),
                    wyrd_gateway::BuiltinEndpoints::default(),
                )
                .expect("gateway dispatch builds"),
            ),
        )
        .await,
    );
    configure(&state, tenant, json!([]), json!([]), "allow_unpriced").await;
    let admin = admin(tenant);
    let gateway = GatewayAdministration::new(&state);
    gateway
        .put_deployment(
            &admin,
            &ProviderDeploymentName::new("speech").expect("name"),
            serde_json::from_value(json!({
                "name": "speech",
                "model": model("acme/speech"),
                "adapter": {"openai_compatible": {"base_url": format!("{}/v1", upstream.uri())}},
                "auth": "none",
                "capabilities": ["audio"],
                "routing_weight": 1,
            }))
            .expect("deployment decodes"),
        )
        .await
        .expect("deployment stores");
    let capture = |fields: BTreeSet<GatewayPayloadField>| {
        let gateway = &gateway;
        let admin = &admin;
        async move {
            gateway
                .put_capture(
                    admin,
                    GatewayCapturePolicyWrite {
                        mode: GatewayCaptureMode::Payload,
                        payload_fields: fields,
                    },
                )
                .await
                .expect("payload policy stores");
        }
    };
    let speak = |voice: &'static str| {
        let state = state.clone();
        async move {
            let response = super::routes::audio_speech(
                State(state),
                Ok(invoker(tenant, 1, [provider_access()])),
                Ok(Json(
                    json!({"model": "acme/speech", "input": "hi", "voice": voice}),
                )),
            )
            .await;
            assert_eq!(response.status(), StatusCode::OK);
            axum::body::to_bytes(response.into_body(), usize::MAX)
                .await
                .expect("audio reads")
        }
    };

    capture(BTreeSet::from([GatewayPayloadField::Request])).await;
    assert_eq!(speak("echo").await.as_ref(), UNSELECTED);
    capture(BTreeSet::from([GatewayPayloadField::Response])).await;
    assert_eq!(
        speak("alloy").await.as_ref(),
        SPOKEN,
        "caller bytes are unchanged"
    );
    drain_gateway(&state).await;

    let digest_of = |bytes: &[u8]| format!("sha256:{}", hex::encode(Sha256::digest(bytes)));
    let digest = digest_of(SPOKEN);
    let published = scribe.published();
    assert!(published.contains(&digest), "{published}");
    assert!(!published.contains("spoken-speech-bytes"), "{published}");
    let unselected = object_path(tenant, &digest_of(UNSELECTED)).expect("key derives");
    assert!(
        matches!(
            state.storage.get_object(&unselected).await,
            Err(wyrd_storage::StorageError::ObjectNotFound { .. })
        ),
        "unselected speech keeps no object"
    );

    let table = TableRef::new(BifrostNamespace::Gateway, "calls");
    let catalog = state.bifrost.catalog().expect("catalog");
    catalog
        .ensure_builtin(
            tenant,
            vala_bifrost_redux::tables::builtin_table("gateway", "calls").expect("built-in table"),
        )
        .await
        .expect("the calls table registers");
    let table_uid = catalog
        .table_uid(&table, tenant)
        .await
        .expect("the registered table resolves");
    let reader = invoker(
        tenant,
        2,
        [
            Permission::gateway_payload_read(),
            Permission {
                resource: Resource::BifrostQuery,
                action: Action::Read,
                scope: table.permission_scope(&table_uid),
            },
        ],
    );
    let response = super::routes::gateway_payload_object(
        State(state.clone()),
        reader,
        axum::extract::Path(digest),
    )
    .await
    .expect("an authorized reader retrieves the speech object");
    assert_eq!(
        axum::body::to_bytes(response.into_body(), usize::MAX)
            .await
            .expect("body reads")
            .as_ref(),
        SPOKEN,
        "the stored speech is byte-identical"
    );
}

/// Proves post-response capture work is bounded by the admitted call's
/// absolute deadline. With the captured object's storage read parked on a
/// FIFO, the call still answers and accounts, its capture is dropped once as
/// unavailable when the deadline passes, nothing is staged, and the capture
/// task ends so tracked gateway work drains while the dependency is still
/// parked.
///
/// # Panics
///
/// Panics when a call fails, a parked capture outlives the call's deadline,
/// or a row is delivered.
#[tokio::test]
async fn gateway_capture_work_ends_at_the_call_deadline() {
    let recorder = SeriesRecorder::default();
    let _metrics = metrics::set_default_local_recorder(&recorder);
    let fixture = PgFixture::start().await.expect("fixture starts");
    let tenant = fixture.data_tenant_id();
    let dispatch = Scripted::shared();
    let (state, scribe) =
        RecordingScribe::attach(replica_with_fixture_catalog(&fixture, dispatch.clone()).await);
    configure(&state, tenant, json!([]), json!([]), "allow_unpriced").await;
    let caller = invoker(tenant, 1, [provider_access()]);
    let deadline = Duration::from_millis(500);
    let dropped = "wyrd_gateway_capture_total{outcome=unavailable}";

    // Object get: the selected answer's object path is a FIFO no writer opens.
    GatewayAdministration::new(&state)
        .put_capture(
            &admin(tenant),
            GatewayCapturePolicyWrite {
                mode: GatewayCaptureMode::Payload,
                payload_fields: BTreeSet::from([GatewayPayloadField::Response]),
            },
        )
        .await
        .expect("payload policy stores");
    let digest = format!(
        "sha256:{}",
        hex::encode(Sha256::digest(PAYLOAD_OBJECT_BYTES))
    );
    let wyrd_storage::settings::BackendConfig::Local { root } = state.storage.backend_config()
    else {
        panic!("the test replica stores locally");
    };
    let fifo = root.join(
        &object_path(tenant, &digest)
            .expect("the digest derives a key")
            .full,
    );
    std::fs::create_dir_all(fifo.parent().expect("the key has a parent")).expect("parent creates");
    let made = std::process::Command::new("mkfifo")
        .arg(&fifo)
        .status()
        .expect("mkfifo runs");
    assert!(made.success(), "the parked object path is a FIFO");
    dispatch.push(Step::Return(media_completed(PAYLOAD_OBJECT_BYTES)));
    let answered = GatewayInvocation::new(&state)
        .invoke(&caller, request("acme/a", None, deadline))
        .await
        .expect("a parked object read never fails the call");
    assert!(
        matches!(&answered.body, ResponseBody::Media(answer) if answer.bytes == PAYLOAD_OBJECT_BYTES),
        "the caller's answer is unchanged"
    );
    drain_gateway(&state).await;
    assert_eq!(scribe.attempts(), 0, "no row is submitted");
    assert!(
        recorder.series.lock().expect("series").contains(dropped),
        "the expired capture is dropped as unavailable"
    );
    assert!(
        entries(&fixture, tenant, answered.call_id)
            .await
            .iter()
            .any(|entry| matches!(entry, GatewayAccountingEntryV1::CallAccounted { .. })),
        "accounting is unaffected"
    );
    // Release the blocking reader still parked on the FIFO.
    drop(
        std::fs::OpenOptions::new()
            .write(true)
            .open(&fifo)
            .expect("the FIFO opens"),
    );
}

/// Proves capture follows the policy admitted with each call: disabling
/// capture while a `Metadata` call is in flight still captures that call, and
/// only calls admitted afterwards go uncaptured.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn gateway_capture_policy_change_affects_only_later_admissions() {
    let fixture = PgFixture::start().await.expect("fixture starts");
    let tenant = fixture.data_tenant_id();
    let dispatch = Scripted::shared();
    let (state, scribe) = RecordingScribe::attach(replica(&fixture, dispatch.clone()).await);
    configure(&state, tenant, json!([]), json!([]), "allow_unpriced").await;
    let gateway = GatewayAdministration::new(&state);
    let write = |mode| GatewayCapturePolicyWrite {
        mode,
        payload_fields: BTreeSet::new(),
    };
    gateway
        .put_capture(&admin(tenant), write(GatewayCaptureMode::Metadata))
        .await
        .expect("metadata policy stores");
    let caller = invoker(tenant, 1, [provider_access()]);
    let call = || request("acme/a", Some((1000, 500)), Duration::from_secs(10));

    dispatch.push(Step::Park(completed(100, 50)));
    let in_flight = tokio::spawn({
        let state = state.clone();
        let caller = caller.clone();
        let request = call();
        async move {
            GatewayInvocation::new(&state)
                .invoke(&caller, request)
                .await
        }
    });
    dispatch.entered.notified().await;
    gateway
        .put_capture(&admin(tenant), write(GatewayCaptureMode::Disabled))
        .await
        .expect("disabled policy stores");
    dispatch.release.notify_one();
    in_flight
        .await
        .expect("call task joins")
        .expect("the call admitted under Metadata completes");

    dispatch.push(Step::Return(completed(100, 50)));
    GatewayInvocation::new(&state)
        .invoke(&caller, call())
        .await
        .expect("the call admitted under Disabled completes");
    drain_gateway(&state).await;
    assert_eq!(
        scribe.rows(),
        2,
        "only the Metadata admission delivers its call row and attempt span"
    );
}

/// Recorder keeping every registered metric series and each gauge's value.
#[derive(Default)]
struct SeriesRecorder {
    /// Every registered series rendered as `name{label=value,...}`.
    series: Mutex<BTreeSet<String>>,
    /// Gauge values by rendered series.
    gauges: Mutex<HashMap<String, Arc<metrics::atomics::AtomicU64>>>,
}

impl SeriesRecorder {
    /// Records `key` and returns its rendered series.
    fn register(&self, key: &metrics::Key) -> String {
        let mut labels = key
            .labels()
            .map(|label| format!("{}={}", label.key(), label.value()))
            .collect::<Vec<_>>();
        labels.sort();
        let rendered = format!("{}{{{}}}", key.name(), labels.join(","));
        self.series.lock().expect("series").insert(rendered.clone());
        rendered
    }

    /// Current value of the gauge `series`, or zero when never registered.
    fn gauge(&self, series: &str) -> f64 {
        self.gauges
            .lock()
            .expect("gauges")
            .get(series)
            .map_or(0.0, |value| {
                f64::from_bits(value.load(std::sync::atomic::Ordering::Relaxed))
            })
    }
}

impl metrics::Recorder for SeriesRecorder {
    /// Ignores descriptions.
    fn describe_counter(
        &self,
        _: metrics::KeyName,
        _: Option<metrics::Unit>,
        _: metrics::SharedString,
    ) {
    }
    /// Ignores descriptions.
    fn describe_gauge(
        &self,
        _: metrics::KeyName,
        _: Option<metrics::Unit>,
        _: metrics::SharedString,
    ) {
    }
    /// Ignores descriptions.
    fn describe_histogram(
        &self,
        _: metrics::KeyName,
        _: Option<metrics::Unit>,
        _: metrics::SharedString,
    ) {
    }
    /// Records the series of a counter.
    fn register_counter(&self, key: &metrics::Key, _: &metrics::Metadata<'_>) -> metrics::Counter {
        self.register(key);
        metrics::Counter::noop()
    }
    /// Records the series of a gauge and tracks its value.
    fn register_gauge(&self, key: &metrics::Key, _: &metrics::Metadata<'_>) -> metrics::Gauge {
        let series = self.register(key);
        metrics::Gauge::from_arc(Arc::clone(
            self.gauges
                .lock()
                .expect("gauges")
                .entry(series)
                .or_default(),
        ))
    }
    /// Records the series of a histogram.
    fn register_histogram(
        &self,
        key: &metrics::Key,
        _: &metrics::Metadata<'_>,
    ) -> metrics::Histogram {
        self.register(key);
        metrics::Histogram::noop()
    }
}

/// Shared buffer receiving formatted trace output.
#[derive(Clone, Default)]
struct TraceBuffer(Arc<Mutex<Vec<u8>>>);

impl TraceBuffer {
    /// Everything written so far.
    fn text(&self) -> String {
        String::from_utf8_lossy(&self.0.lock().expect("trace buffer")).into_owned()
    }
}

impl std::io::Write for TraceBuffer {
    /// Appends formatted trace output.
    fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
        self.0
            .lock()
            .expect("trace buffer")
            .extend_from_slice(bytes);
        Ok(bytes.len())
    }

    /// Nothing is buffered outside the shared vector.
    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}

impl<'a> MakeWriter<'a> for TraceBuffer {
    type Writer = Self;

    /// Hands out a handle to the shared buffer.
    fn make_writer(&'a self) -> Self::Writer {
        self.clone()
    }
}

/// Integrated security and observability proof over a real provider adapter
/// and credential: the provider credential never appears in a response,
/// error, log, trace, metric, audit record, ledger entry, published Bifrost
/// batch, or public configuration response, even when the provider echoes
/// it in a refusal or under innocuous nested keys of a successful buffered or
/// streamed answer, whose caller bytes stay unchanged while selected capture
/// keeps the scrubbed content. An image encoding it as base64 in a value or
/// member name keeps its caller bytes but publishes no row or object, and a
/// non-JSON refusal holding it as plain base64 or a base64 `data:` URL relays
/// only its status and persists neither. Disabled capture keeps the operational traces;
/// metrics use only closed labels and the active-calls gauge returns to zero;
/// one call's ingress, authorization, routing, admission, execution, attempt,
/// translation, accounting, and capture spans share its `gateway.call` span.
/// A stream settles to its terminal result: a completed stream succeeds, a
/// truncated one fails after output, in the ledger, metrics, and the closed
/// `outcome`, `failure_class`, and `error.code` fields of its call and attempt
/// spans. A dropped stream's cancellation is proven against a held upstream by
/// `adapter::tests::dropping_a_stream_aborts_the_upstream_request`, since a
/// finite mock body can end before the drop is observed.
///
/// # Panics
///
/// Panics when a fixture fails or a leak, label, gauge, correlation, capture,
/// or terminal-result assertion differs.
#[tokio::test]
#[expect(
    clippy::float_cmp,
    reason = "Prometheus renders these metrics as whole numbers, so f64 equality is exact"
)]
async fn gateway_observations_are_bounded_correlated_and_secret_free() {
    use std::fmt::Write as _;
    const CANARY: &str = "sk-gateway-canary-5e1d";
    let recorder = SeriesRecorder::default();
    let _metrics = metrics::set_default_local_recorder(&recorder);
    let traces = TraceBuffer::default();
    let _traces = tracing::subscriber::set_default(
        tracing_subscriber::fmt()
            .with_writer(traces.clone())
            .with_ansi(false)
            .with_max_level(tracing::Level::TRACE)
            .with_span_events(FmtSpan::CLOSE)
            .finish(),
    );

    let fixture = PgFixture::start().await.expect("fixture starts");
    let tenant = fixture.data_tenant_id();
    let upstream = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/v1/chat/completions"))
        .and(body_partial_json(json!({"user": "echo"})))
        .respond_with(ResponseTemplate::new(401).set_body_json(json!({"error": {
            "message": format!("Incorrect API key provided: {CANARY}"),
            "type": "invalid_request_error",
        }})))
        .with_priority(1)
        .mount(&upstream)
        .await;
    let echoing = |user: &str, template: ResponseTemplate| {
        Mock::given(method("POST"))
            .and(path("/v1/chat/completions"))
            .and(body_partial_json(json!({"user": user})))
            .respond_with(template)
            .with_priority(1)
    };
    echoing(
        "nested",
        ResponseTemplate::new(200).set_body_json(json!({
            "id": "nested-marker", "object": "chat.completion", "choices": [],
            "system_fingerprint": {"note": format!("echo {CANARY}")},
            "usage": {"prompt_tokens": 4, "completion_tokens": 2, "total_tokens": 6},
        })),
    )
    .mount(&upstream)
    .await;
    let stream_frame = format!(
        "data: {{\"id\":\"stream-marker\",\"choices\":[{{\"index\":0,\"delta\":{{\"content\":\"{CANARY}\"}}}}]}}\n\n"
    );
    echoing(
        "streamed",
        ResponseTemplate::new(200).set_body_raw(
            format!("{stream_frame}data: [DONE]\n\n"),
            "text/event-stream",
        ),
    )
    .mount(&upstream)
    .await;
    echoing(
        "truncated",
        ResponseTemplate::new(200).set_body_raw(stream_frame.clone(), "text/event-stream"),
    )
    .mount(&upstream)
    .await;
    let encoded = base64::engine::general_purpose::STANDARD.encode(CANARY);
    for (user, body) in [
        ("plain-refusal", encoded.clone()),
        ("url-refusal", format!("data:text/plain;base64,{encoded}")),
    ] {
        echoing(
            user,
            ResponseTemplate::new(401).set_body_raw(body, "text/plain"),
        )
        .mount(&upstream)
        .await;
    }
    let encoded_image = json!({"created": 1, "data": [{
        "b64_json": base64::engine::general_purpose::STANDARD.encode(format!("PNG{CANARY}PNG")),
        "revised_prompt": "encoded-marker",
    }]})
    .to_string();
    let keyed_image =
        json!({"created": 1, "data": [{encoded.as_str(): "keyed-marker"}]}).to_string();
    for (prompt, answer) in [("a cat", &encoded_image), ("a key", &keyed_image)] {
        Mock::given(method("POST"))
            .and(path("/v1/images/generations"))
            .and(body_partial_json(json!({"prompt": prompt})))
            .respond_with(
                ResponseTemplate::new(200).set_body_raw(answer.clone(), "application/json"),
            )
            .mount(&upstream)
            .await;
    }
    Mock::given(method("POST"))
        .and(path("/v1/chat/completions"))
        .and(header("authorization", format!("Bearer {CANARY}").as_str()))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "id": "ds-1", "object": "chat.completion", "choices": [],
            "usage": {"prompt_tokens": 4, "completion_tokens": 2, "total_tokens": 6},
        })))
        .mount(&upstream)
        .await;
    let secret = tempfile::NamedTempFile::new().expect("secret file");
    std::fs::write(secret.path(), format!("{CANARY}\n")).expect("secret writes");
    let (state, scribe) = RecordingScribe::attach(
        test_state(&fixture)
            .await
            .with_gateway(crate::config::GatewayConfig {
                credential_bindings: std::collections::BTreeMap::from([(
                    wyrd_spec::ids::CredentialBindingName::new("canary-file").expect("binding"),
                    crate::config::GatewayCredentialBinding {
                        secret: wyrd_spec::security::SecretRef::File {
                            path: secret.path().display().to_string(),
                        },
                        assignment: wyrd_gateway::CredentialAssignment {
                            host: Some("127.0.0.1".to_owned()),
                            ..super::pg_administration_tests::assigned(tenant, "deepseek")
                        },
                    },
                )]),
                secret_backends: std::collections::BTreeMap::new(),
                ..Default::default()
            })
            .with_gateway_engine(GatewayEngine::new(
                CredentialResolver::default(),
                DeploymentHealth::default(),
                Arc::new(
                    wyrd_gateway::HttpProviderDispatch::new(
                        skald_providers::EndpointPolicy::new(false),
                        wyrd_gateway::BuiltinEndpoints::default(),
                    )
                    .expect("gateway dispatch builds"),
                ),
            )),
    );
    let admin = admin(tenant);
    let gateway = GatewayAdministration::new(&state);
    gateway
        .put_credential(
            &admin,
            &wyrd_spec::ids::ProviderCredentialName::new("canary-key").expect("name"),
            serde_json::from_value(json!({
                "name": "canary-key", "provider": "deepseek",
                "source": {"environment": {"binding": "canary-file"}}
            }))
            .expect("credential decodes"),
        )
        .await
        .expect("credential stores");
    gateway
        .put_deployment(
            &admin,
            &ProviderDeploymentName::new("canary-chat").expect("name"),
            serde_json::from_value(json!({
                "name": "canary-chat",
                "model": model("deepseek/deepseek-chat"),
                "adapter": {"openai_compatible": {"base_url": format!("{}/v1", upstream.uri())}},
                "auth": {"bearer": {"credential": "canary-key"}},
                "capabilities": ["chat_completions", "images"],
                "routing_weight": 1,
            }))
            .expect("deployment decodes"),
        )
        .await
        .expect("deployment stores");
    let caller = invoker(
        tenant,
        7,
        [Permission::gateway_invoke(GatewayAccess::Provider {
            provider: ProviderId::new("deepseek").expect("provider"),
        })],
    );
    let call = |user: &str| GatewayCallRequest {
        operation: GatewayOperation::ChatCompletions,
        ingress: IngressDialect::OpenAi,
        model: model("deepseek/deepseek-chat"),
        fallback: None,
        body: json!({
            "model": "deepseek/deepseek-chat",
            "user": user,
            "messages": [{"role": "user", "content": "hi"}],
        }),
        media: None,
        batch: None,
        deployment: None,
        stream: false,
        usage_bound: None,
        timeout: Duration::from_secs(10),
        subject: None,
    };
    let invocation = GatewayInvocation::new(&state);

    let disabled = invocation
        .invoke(&caller, call("disabled"))
        .await
        .expect("disabled call completes");
    drain_gateway(&state).await;
    let disabled_traces = traces.text();
    for span in ["gateway.call", "gateway.attempt", "gateway.account"] {
        assert!(
            disabled_traces.contains(span),
            "Disabled capture keeps the {span} span: {disabled_traces}"
        );
    }
    assert!(!disabled_traces.contains("gateway.capture"));

    gateway
        .put_capture(
            &admin,
            GatewayCapturePolicyWrite {
                mode: GatewayCaptureMode::Payload,
                payload_fields: BTreeSet::from([
                    GatewayPayloadField::Request,
                    GatewayPayloadField::Response,
                ]),
            },
        )
        .await
        .expect("payload policy stores");
    let answered = invocation
        .invoke(&caller, call("answered"))
        .await
        .expect("captured call completes");
    let echoed = invocation
        .invoke(&caller, call("echo"))
        .await
        .expect("a provider refusal is relayed");
    assert_eq!(echoed.status, 401);
    let nested = invocation
        .invoke(&caller, call("nested"))
        .await
        .expect("a nested echo answers");
    assert!(
        matches!(&nested.body, ResponseBody::Json(raw) if raw.get().contains(CANARY)),
        "the caller's successful answer is unchanged: {:?}",
        nested.body
    );
    let stream = |user: &str| {
        let mut request = call(user);
        request.stream = true;
        request.body["stream"] = json!(true);
        request
    };
    let mut streams = Vec::new();
    for user in ["streamed", "truncated"] {
        let response = invocation
            .invoke(&caller, stream(user))
            .await
            .expect("the stream opens");
        let ResponseBody::Events(mut events) = response.body else {
            panic!("event stream expected: {:?}", response.body);
        };
        while events.recv().await.is_some() {}
        drop(events);
        streams.push(response.call_id);
    }
    for (prompt, answer) in [("a cat", &encoded_image), ("a key", &keyed_image)] {
        let image = invocation
            .invoke(
                &caller,
                GatewayCallRequest {
                    operation: GatewayOperation::Images,
                    body: json!({"model": "deepseek/deepseek-chat", "prompt": prompt}),
                    media: Some(wyrd_gateway::MediaRequest {
                        route: wyrd_gateway::OpenAiMediaRoute::ImageGenerations,
                        files: Vec::new(),
                    }),
                    ..call("image")
                },
            )
            .await
            .expect("an image encoding the credential answers");
        assert!(
            matches!(&image.body, ResponseBody::Json(raw) if raw.get() == answer.as_str()),
            "the caller's {prompt} image answer is unchanged: {:?}",
            image.body
        );
    }
    let mut encoded_refusals = String::new();
    for user in ["plain-refusal", "url-refusal"] {
        let refusal = invocation
            .invoke(&caller, call(user))
            .await
            .expect("a non-JSON refusal is relayed");
        assert_eq!(refusal.status, 401);
        let _ = write!(encoded_refusals, "{:?}", refusal.body);
    }
    assert!(
        encoded_refusals.contains("HTTP 401") && !encoded_refusals.contains(&encoded),
        "a non-JSON refusal encoding the credential relays only its status: {encoded_refusals}"
    );
    let refused = invocation
        .invoke(&invoker(tenant, 8, []), call("denied"))
        .await
        .expect_err("an unauthorized caller is refused");
    drain_gateway(&state).await;

    let published = scribe.published();
    for marker in [
        "Incorrect API key provided",
        "nested-marker",
        "stream-marker",
    ] {
        assert!(
            published.contains(marker),
            "the scrubbed {marker} content was published, so absence below is meaningful"
        );
    }
    let encoded_digest = format!(
        "sha256:{}",
        hex::encode(Sha256::digest(format!("PNG{CANARY}PNG")))
    );
    let refusal_digest = format!("sha256:{}", hex::encode(Sha256::digest(CANARY)));
    assert!(
        !published.contains("encoded-marker")
            && !published.contains("keyed-marker")
            && !published.contains(&encoded)
            && !published.contains(&encoded_digest)
            && !published.contains(&refusal_digest),
        "an answer encoding the credential publishes no row or reference: {published}"
    );
    for digest in [&encoded_digest, &refusal_digest] {
        assert!(
            matches!(
                state
                    .storage
                    .get_object(&object_path(tenant, digest).expect("key derives"))
                    .await,
                Err(wyrd_storage::StorageError::ObjectNotFound { .. })
            ),
            "an answer encoding the credential stores no object {digest}"
        );
    }
    let audit: Vec<String> = scribe
        .audit_decisions()
        .iter()
        .filter(|decision| decision.tenant == tenant)
        .map(|decision| format!("{decision:?}"))
        .collect();
    assert!(!audit.is_empty(), "authorization decisions were audited");
    let configuration = serde_json::to_string(&(
        gateway.credentials(&admin).await.expect("credentials read"),
        gateway.deployments(&admin).await.expect("deployments read"),
        gateway.capture(&admin).await.expect("capture policy reads"),
    ))
    .expect("configuration serializes");
    let mut ledger = String::new();
    for call_id in [
        disabled.call_id,
        answered.call_id,
        echoed.call_id,
        nested.call_id,
    ]
    .into_iter()
    .chain(streams.iter().copied())
    {
        let _ = write!(ledger, "{:?}", entries(&fixture, tenant, call_id).await);
    }
    let series = recorder
        .series
        .lock()
        .expect("series")
        .iter()
        .cloned()
        .collect::<Vec<_>>()
        .join("\n");
    let logs = traces.text();
    for (surface, text) in [
        (
            "responses other than the echoing answers",
            format!("{:?}{:?}{:?}", disabled.body, answered.body, echoed.body),
        ),
        ("errors", format!("{refused:?}{refused}")),
        ("logs and traces", logs.clone()),
        ("metrics", series.clone()),
        ("audit", audit.join("\n")),
        ("ledger", ledger),
        ("bifrost", published),
        ("configuration", configuration),
    ] {
        assert!(
            !text.contains(CANARY),
            "{surface} leak the credential: {text}"
        );
    }

    let allowed = [
        "class",
        "code",
        "currency",
        "direction",
        "operation",
        "outcome",
        "result",
    ];
    let tenant_text = tenant.to_string();
    let call_text = answered.call_id.as_uuid().to_string();
    for line in series
        .lines()
        .filter(|line| line.starts_with("wyrd_gateway_"))
    {
        let labels = line
            .split_once('{')
            .map_or("", |(_, rest)| rest.trim_end_matches('}'));
        for label in labels.split(',').filter(|label| !label.is_empty()) {
            let (key, value) = label.split_once('=').expect("label pair");
            assert!(allowed.contains(&key), "unbounded label {key} in {line}");
            assert!(
                !value.contains(&tenant_text)
                    && !value.contains(&call_text)
                    && !value.contains("deepseek"),
                "tenant, call, or model value in {line}"
            );
        }
    }
    for name in [
        "wyrd_gateway_requests_total",
        "wyrd_gateway_attempts_total",
        "wyrd_gateway_time_to_first_byte_seconds",
        "wyrd_gateway_queue_delay_seconds",
        "wyrd_gateway_refusals_total",
        "wyrd_gateway_routing_total",
        "wyrd_gateway_tokens_total",
        "wyrd_gateway_capture_total",
    ] {
        assert!(series.contains(name), "{name} is recorded: {series}");
    }
    assert_eq!(recorder.gauge("wyrd_gateway_active_calls{}"), 0.0);

    let correlated = logs
        .lines()
        .filter(|line| line.contains(&format!("call_id={call_text}")))
        .collect::<Vec<_>>()
        .join("\n");
    for span in [
        "gateway.authorize",
        "gateway.route",
        "gateway.admit",
        "gateway.execute",
        "gateway.attempt",
        "gateway.translate",
        "gateway.account",
        "gateway.capture",
    ] {
        assert!(
            correlated.contains(span),
            "{span} is linked under the answered call's gateway.call span: {correlated}"
        );
    }

    // Close lines of `name` spans whose line contains `scope`.
    let closes = |scope: &str, name: &str| {
        logs.lines()
            .filter(|line| line.contains(scope) && line.contains(": close"))
            .filter(|line| {
                // The closing span is the last `name{fields}` group of the path.
                line.split_once("}: ")
                    .and_then(|(path, _)| path.rsplit("}:").next()?.split_once('{'))
                    .is_some_and(|(span, _)| span.rsplit([':', ' ']).next() == Some(name))
            })
            .map(str::to_owned)
            .collect::<Vec<_>>()
    };
    let expected = [
        (answered.call_id, GatewayCallOutcome::Succeeded, None, None),
        (
            echoed.call_id,
            GatewayCallOutcome::Failed,
            Some("rejected"),
            Some("WYRD_GATEWAY_502_UPSTREAM_UNAVAILABLE"),
        ),
        (streams[0], GatewayCallOutcome::Succeeded, None, None),
        (
            streams[1],
            GatewayCallOutcome::Failed,
            Some("after_output"),
            Some("WYRD_GATEWAY_502_UPSTREAM_UNAVAILABLE"),
        ),
    ];
    for (call_id, outcome, class, code) in expected {
        let name = wyrd_gateway::outcome_name(outcome);
        let field = |key: &str, value: &str| format!("{key}={value:?}");
        let scope = format!("call_id={}", call_id.as_uuid());
        let attempt = closes(&scope, "gateway.attempt");
        let call = closes(&scope, "gateway.call");
        assert_eq!((attempt.len(), call.len()), (1, 1), "{logs}");
        assert!(attempt[0].contains(&field("outcome", name)), "{attempt:?}");
        assert_eq!(
            class.map(|class| attempt[0].contains(&field("failure_class", class))),
            class.map(|_| true),
            "{attempt:?}"
        );
        assert!(call[0].contains(&field("outcome", name)), "{call:?}");
        assert!(
            call[0].contains(&format!("request_id={}", caller.request_id)),
            "the call span carries the admitting request: {call:?}"
        );
        assert!(
            code.map_or(!call[0].contains("error.code"), |code| call[0]
                .contains(&field("error.code", code))),
            "{call:?}"
        );
        let ledger = entries(&fixture, tenant, call_id).await;
        assert!(
            ledger.iter().any(|entry| matches!(entry,
                GatewayAccountingEntryV1::CallAccounted { outcome: accounted, .. } if *accounted == outcome)),
            "{ledger:?}"
        );
        assert!(
            ledger.iter().any(|entry| matches!(entry,
                GatewayAccountingEntryV1::AttemptAccounted { outcome: accounted, .. } if *accounted == outcome)),
            "{ledger:?}"
        );
        assert!(
            series.contains(&format!(
                "wyrd_gateway_requests_total{{operation=chat_completions,outcome={name}}}"
            )),
            "{series}"
        );
    }
    let refused_call = closes("error.code=", "gateway.call")
        .iter()
        .any(|line| line.contains("outcome=\"failed\""));
    assert!(
        refused_call,
        "a refusal before execution records its failed outcome and code: {logs}"
    );
}

/// Batch listing reads one bounded, grant-pruned page: more batches of a
/// model the caller cannot invoke than the largest page lie between visible
/// batches, yet each page returns the right visible batches and `has_more`
/// after exactly one audited decision per distinct visible model, never one
/// for the unreachable model. A provider-wide grant sees every batch.
///
/// # Panics
///
/// Panics when a fixture fails, a page lists the wrong batches or `has_more`,
/// or the audited decision count differs.
#[tokio::test]
async fn gateway_batch_listing_is_one_bounded_pruned_read() {
    let fixture = PgFixture::start().await.expect("fixture starts");
    let tenant = fixture.data_tenant_id();
    let (state, scribe) = RecordingScribe::attach(replica(&fixture, Scripted::shared()).await);
    let mut conn = fixture.tenant_conn_for(tenant).await.expect("tenant conn");
    // Denied batches 101..=220 lie between visible batches 300, 250 and 50.
    let visible = [300_u128, 250, 50];
    for (id, served) in (101..=220)
        .map(|id| (id, "acme/b"))
        .chain(visible.map(|id| (id, "acme/a")))
    {
        let row = GatewayBatchRow {
            batch_id: Uuid::from_u128(id),
            file_id: Uuid::from_u128(id),
            model: served.to_owned(),
            deployment: "dep".to_owned(),
            upstream_batch_id: None,
            batch: None,
        };
        assert!(
            claim_gateway_batch(&mut conn, &row, &id.to_string())
                .await
                .expect("claim")
        );
        assert!(
            record_gateway_batch(
                &mut conn,
                row.batch_id,
                &format!("up-{id}"),
                &json!({"object": "batch", "status": "completed"}),
            )
            .await
            .expect("record")
        );
    }
    conn.commit().await.expect("batches commit");
    let list = |caller: Caller, after: Option<String>, limit: u32| {
        let state = state.clone();
        async move {
            let listed = super::routes::list_batches(
                State(state),
                Ok(caller),
                Ok(axum::extract::Query(
                    wyrd_spec::gateway::openai::GatewayBatchListQuery {
                        after,
                        limit: Some(limit),
                    },
                )),
            )
            .await;
            assert_eq!(listed.status(), StatusCode::OK);
            body_json(listed).await
        }
    };
    let ids = |listed: &Value| {
        listed["data"]
            .as_array()
            .expect("data")
            .iter()
            .map(|batch| batch["id"].as_str().expect("id").to_owned())
            .collect::<Vec<_>>()
    };
    let public = |id: u128| format!("batch_{}", Uuid::from_u128(id).simple());
    let decisions = || async {
        // The invoke decision is staged on the gateway tracker, so it is
        // readable once that tracker drains.
        drain_gateway(&state).await;
        audit_decisions(&scribe, tenant).len()
    };

    let caller = invoker(tenant, 1, [model_access("acme/a")]);
    let before = decisions().await;
    let first = list(caller.clone(), None, 2).await;
    assert_eq!(ids(&first), [public(300), public(250)], "{first}");
    assert_eq!(first["has_more"], true);
    assert_eq!(
        decisions().await,
        before + 1,
        "one decision for acme/a only"
    );
    let second = list(caller, first["last_id"].as_str().map(str::to_owned), 2).await;
    assert_eq!(ids(&second), [public(50)], "{second}");
    assert_eq!(second["has_more"], false);
    assert_eq!(decisions().await, before + 2);

    let wide = list(invoker(tenant, 2, [provider_access()]), None, 100).await;
    assert_eq!(wide["data"].as_array().map(Vec::len), Some(100));
    assert_eq!(wide["has_more"], true);
    assert_eq!(
        decisions().await,
        before + 4,
        "one decision per listed model"
    );
    drain_gateway(&state).await;
}

/// Every terminal call and attempt closes with classifiable span evidence:
/// refusals before execution (authorization, validation, routing, and drain)
/// close their `gateway.call` span with `failed` or, for drain,
/// `cancelled` plus the stable code; buffered and opened-stream calls that
/// time out or are cancelled in flight close both the call and the attempt
/// with that outcome and the stable code. A native stream whose provider
/// holds the connection open after `[DONE]` settles promptly as a success,
/// delivers no later byte, and records a matching ledger, request metric,
/// selected capture, and call and attempt spans.
///
/// # Panics
///
/// Panics when a fixture fails, a call returns the wrong result, or a close
/// line, ledger entry, metric, or capture differs from the terminal result.
#[tokio::test]
async fn gateway_terminal_spans_classify_every_call_and_attempt() {
    let recorder = SeriesRecorder::default();
    let _metrics = metrics::set_default_local_recorder(&recorder);
    let traces = TraceBuffer::default();
    let _traces = tracing::subscriber::set_default(
        tracing_subscriber::fmt()
            .with_writer(traces.clone())
            .with_ansi(false)
            .with_max_level(tracing::Level::TRACE)
            .with_span_events(FmtSpan::CLOSE)
            .finish(),
    );
    let fixture = PgFixture::start().await.expect("fixture starts");
    let tenant = fixture.data_tenant_id();
    let dispatch: Arc<dyn ProviderDispatch> = Arc::new(
        wyrd_gateway::HttpProviderDispatch::new(
            skald_providers::EndpointPolicy::new(false),
            wyrd_gateway::BuiltinEndpoints::default(),
        )
        .expect("gateway dispatch builds"),
    );
    let (state, scribe) = RecordingScribe::attach(replica(&fixture, Arc::clone(&dispatch)).await);
    configure(&state, tenant, json!([]), json!([]), "allow_unpriced").await;
    let admin = admin(tenant);
    let gateway = GatewayAdministration::new(&state);
    gateway
        .put_capture(
            &admin,
            GatewayCapturePolicyWrite {
                mode: GatewayCaptureMode::Payload,
                payload_fields: BTreeSet::from([GatewayPayloadField::Response]),
            },
        )
        .await
        .expect("payload policy stores");
    // Deploys `acme/<name>` against a provider that sends `frame` and holds.
    let held = |name: &'static str, frame: &'static str| {
        let gateway = &gateway;
        let admin = &admin;
        async move {
            let (port, upstream, received) = held_upstream(frame).await;
            gateway
                .put_deployment(
                    admin,
                    &ProviderDeploymentName::new(name).expect("name"),
                    serde_json::from_value(json!({
                        "name": name,
                        "model": model(&format!("acme/{name}")),
                        "adapter": {"openai_compatible": {"base_url": format!("http://127.0.0.1:{port}/v1")}},
                        "auth": "none",
                        "capabilities": ["chat_completions"],
                        "routing_weight": 1,
                    }))
                    .expect("deployment decodes"),
                )
                .await
                .expect("deployment stores");
            (upstream, received)
        }
    };
    let call = |name: &str, stream: bool| {
        let mut request = request(&format!("acme/{name}"), None, Duration::from_secs(2));
        request.stream = stream;
        request.body["stream"] = json!(stream);
        request
    };
    let caller = |n: u128| invoker(tenant, n, [provider_access()]);
    let invocation = GatewayInvocation::new(&state);
    let deadline = wyrd_gateway::outcome_error_code(GatewayCallOutcome::TimedOut);
    let unavailable = wyrd_gateway::outcome_error_code(GatewayCallOutcome::Cancelled);
    // (caller, call outcome, call code, attempt outcome when one ran)
    let mut expected = Vec::new();

    let (upstream, _) = held(
        "held-done",
        "data: {\"id\":\"held-marker\",\"choices\":[],\"usage\":{\"prompt_tokens\":1,\"completion_tokens\":2,\"total_tokens\":3}}\n\ndata: [DONE]\n\ndata: {\"id\":\"late-marker\"}\n\n",
    )
    .await;
    let done = caller(1);
    let response = invocation
        .invoke(&done, call("held-done", true))
        .await
        .expect("the held stream opens");
    let done_call = response.call_id;
    let ResponseBody::Events(mut events) = response.body else {
        panic!("event stream expected: {:?}", response.body);
    };
    let mut delivered = Vec::new();
    tokio::time::timeout(Duration::from_secs(1), async {
        while let Some(frame) = events.recv().await {
            delivered.extend(frame.expect("the stream terminates cleanly"));
        }
    })
    .await
    .expect("the stream settles at its terminator, before its deadline");
    let delivered = String::from_utf8(delivered).expect("utf-8");
    assert!(
        delivered.ends_with("data: [DONE]\n\n") && !delivered.contains("late-marker"),
        "{delivered}"
    );
    assert!(upstream.await.expect("upstream task"), "upstream closed");
    expected.push((done, GatewayCallOutcome::Succeeded, None, Some(None)));

    let (upstream, _) = held("held-buffered-timeout", "data: {}\n\n").await;
    let timed_out = caller(2);
    let error = invocation
        .invoke(&timed_out, call("held-buffered-timeout", false))
        .await
        .expect_err("a held buffered call times out");
    assert!(
        matches!(error, WyrdError::GatewayDeadlineExceeded { .. }),
        "{error:?}"
    );
    assert!(upstream.await.expect("upstream task"), "upstream closed");
    expected.push((
        timed_out,
        GatewayCallOutcome::TimedOut,
        deadline,
        Some(deadline),
    ));

    let (upstream, received) = held("held-buffered-cancel", "data: {}\n\n").await;
    let cancelled = caller(3);
    let token = CancellationToken::new();
    let cancelling = {
        let token = token.clone();
        tokio::spawn(async move {
            received.notified().await;
            token.cancel();
        })
    };
    let failure = invocation
        .run(
            &cancelled,
            call("held-buffered-cancel", false),
            false,
            &token,
        )
        .await
        .expect_err("a cancelled buffered call fails");
    cancelling.await.expect("cancellation task");
    assert!(
        matches!(failure.error, WyrdError::ServiceUnavailable { .. }),
        "{:?}",
        failure.error
    );
    assert!(upstream.await.expect("upstream task"), "upstream closed");
    expected.push((
        cancelled,
        GatewayCallOutcome::Cancelled,
        unavailable,
        Some(unavailable),
    ));

    for (n, name, outcome, code) in [
        (
            4,
            "held-stream-timeout",
            GatewayCallOutcome::TimedOut,
            deadline,
        ),
        (
            5,
            "held-stream-cancel",
            GatewayCallOutcome::Cancelled,
            unavailable,
        ),
    ] {
        let (upstream, _) = held(name, "data: {\"choices\":[]}\n\n").await;
        let stream_caller = caller(n);
        let token = CancellationToken::new();
        let response = invocation
            .run(&stream_caller, call(name, true), false, &token)
            .await
            .expect("the held stream opens");
        let ResponseBody::Events(mut events) = response.body else {
            panic!("event stream expected: {:?}", response.body);
        };
        events.recv().await.expect("first frame").expect("frame");
        if outcome == GatewayCallOutcome::Cancelled {
            token.cancel();
        }
        while events.recv().await.is_some() {}
        assert!(upstream.await.expect("upstream task"), "upstream closed");
        expected.push((stream_caller, outcome, code, Some(code)));
    }

    let unauthorized = invoker(tenant, 6, []);
    let error = invocation
        .invoke(
            &unauthorized,
            request("acme/a", None, Duration::from_secs(2)),
        )
        .await
        .expect_err("an unauthorized call is refused");
    expected.push((
        unauthorized,
        GatewayCallOutcome::Failed,
        Some(error.code()),
        None,
    ));
    let invalid = caller(7);
    let error = invocation
        .invoke(&invalid, request("acme/a", None, Duration::MAX))
        .await
        .expect_err("an out-of-range timeout is refused");
    assert!(
        matches!(error, WyrdError::GatewayInvalidRequest { .. }),
        "{error:?}"
    );
    expected.push((
        invalid,
        GatewayCallOutcome::Failed,
        Some(error.code()),
        None,
    ));
    let unroutable = caller(8);
    let mut pinned = request("acme/a", None, Duration::from_secs(2));
    pinned.deployment = Some(ProviderDeploymentName::new("missing").expect("name"));
    let error = invocation
        .invoke(&unroutable, pinned)
        .await
        .expect_err("a pin no deployment matches is refused");
    assert!(
        matches!(error, WyrdError::GatewayModelUnavailable { .. }),
        "{error:?}"
    );
    expected.push((
        unroutable,
        GatewayCallOutcome::Failed,
        Some(error.code()),
        None,
    ));

    drain_gateway(&state).await;
    let drained = caller(10);
    state.shutdown_token.cancel();
    let error = invocation
        .invoke(&drained, request("acme/a", None, Duration::from_secs(2)))
        .await
        .expect_err("a draining server refuses");
    expected.push((
        drained,
        GatewayCallOutcome::Cancelled,
        Some(error.code()),
        None,
    ));

    let logs = traces.text();
    let closes = |scope: &str, name: &str| {
        logs.lines()
            .filter(|line| line.contains(scope) && line.contains(": close"))
            .filter(|line| {
                line.split_once("}: ")
                    .and_then(|(path, _)| path.rsplit("}:").next()?.split_once('{'))
                    .is_some_and(|(span, _)| span.rsplit([':', ' ']).next() == Some(name))
            })
            .map(str::to_owned)
            .collect::<Vec<_>>()
    };
    let field = |key: &str, value: &str| format!("{key}={value:?}");
    for (caller, outcome, code, attempt) in expected {
        let scope = format!("request_id={}", caller.request_id);
        let name = wyrd_gateway::outcome_name(outcome);
        let calls = closes(&scope, "gateway.call");
        assert_eq!(calls.len(), 1, "{scope}: {logs}");
        assert!(calls[0].contains(&field("outcome", name)), "{calls:?}");
        assert_eq!(
            calls[0].contains("error.code="),
            code.is_some(),
            "{calls:?}"
        );
        if let Some(code) = code {
            assert!(calls[0].contains(&field("error.code", code)), "{calls:?}");
        }
        let attempts = closes(&scope, "gateway.attempt");
        match attempt {
            None => assert!(attempts.is_empty(), "{attempts:?}"),
            Some(attempt_code) => {
                assert_eq!(attempts.len(), 1, "{scope}: {logs}");
                assert!(
                    attempts[0].contains(&field("outcome", name)),
                    "{attempts:?}"
                );
                assert_eq!(
                    attempt_code.map(|code| attempts[0].contains(&field("error.code", code))),
                    attempt_code.map(|_| true),
                    "{attempts:?}"
                );
            }
        }
    }

    let ledger = entries(&fixture, tenant, done_call).await;
    assert!(
        ledger
            .iter()
            .any(|entry| matches!(entry, GatewayAccountingEntryV1::CallAccounted { .. })),
        "{ledger:?}"
    );
    for accounted in &ledger {
        match accounted {
            GatewayAccountingEntryV1::CallAccounted { outcome, .. }
            | GatewayAccountingEntryV1::AttemptAccounted { outcome, .. } => {
                assert_eq!(*outcome, GatewayCallOutcome::Succeeded, "{ledger:?}");
            }
            _ => {}
        }
    }
    let series = recorder
        .series
        .lock()
        .expect("series")
        .iter()
        .cloned()
        .collect::<Vec<_>>()
        .join("\n");
    assert!(
        series
            .contains("wyrd_gateway_requests_total{operation=chat_completions,outcome=succeeded}"),
        "{series}"
    );
    let published = scribe.published();
    assert!(
        published.contains("held-marker") && !published.contains("late-marker"),
        "the capture holds exactly the delivered events"
    );
    drain_gateway(&state).await;
}

/// A buffered call whose provider attempt completed but whose accounting then
/// fails returns the stable error, and its closed call span, request metrics,
/// and published Metadata call row say `failed` with that code and unknown
/// usage and cost, while its provider attempt span, attempt metrics, routing
/// metric, and published attempt span still say `succeeded`.
///
/// Accounting is faulted by a `NOT VALID` check constraint on the fixture's
/// own database refusing attempt entries, added while the attempt is parked
/// after admission committed, so only the post-provider append fails.
///
/// # Panics
///
/// Panics when a fixture or the fault fails, the call answers, or a closed
/// span, metric series, or published row differs from those outcomes.
#[tokio::test]
async fn gateway_accounting_failure_after_a_completed_attempt_fails_the_call_span() {
    let recorder = SeriesRecorder::default();
    let _metrics = metrics::set_default_local_recorder(&recorder);
    let traces = TraceBuffer::default();
    let _traces = tracing::subscriber::set_default(
        tracing_subscriber::fmt()
            .with_writer(traces.clone())
            .with_ansi(false)
            .with_max_level(tracing::Level::TRACE)
            .with_span_events(FmtSpan::CLOSE)
            .finish(),
    );
    let fixture = PgFixture::start().await.expect("fixture starts");
    let tenant = fixture.data_tenant_id();
    let dispatch = Scripted::shared();
    let (state, scribe) = RecordingScribe::attach(replica(&fixture, dispatch.clone()).await);
    configure(&state, tenant, json!([]), json!([]), "allow_unpriced").await;
    GatewayAdministration::new(&state)
        .put_capture(
            &admin(tenant),
            GatewayCapturePolicyWrite {
                mode: GatewayCaptureMode::Metadata,
                payload_fields: BTreeSet::new(),
            },
        )
        .await
        .expect("metadata policy stores");
    let caller = invoker(tenant, 1, [provider_access()]);

    dispatch.push(Step::Park(completed(10, 5)));
    let call = tokio::spawn({
        let (state, caller) = (state.clone(), caller.clone());
        async move {
            GatewayInvocation::new(&state)
                .invoke(&caller, request("acme/a", None, Duration::from_secs(30)))
                .await
        }
    });
    dispatch.entered.notified().await;
    let superuser = fixture.superuser_pool().expect("superuser pool");
    sqlx::query(
        "ALTER TABLE wyrd.gateway_accounting_entries ADD CONSTRAINT attempt_accounting_fault \
         CHECK (kind <> 'attempt_accounted') NOT VALID",
    )
    .execute(&superuser)
    .await
    .expect("accounting fault installs");
    dispatch.release.notify_one();
    let error = call
        .await
        .expect("call task joins")
        .expect_err("failed accounting prevents the answer");
    drain_gateway(&state).await;

    let series = recorder
        .series
        .lock()
        .expect("series")
        .iter()
        .cloned()
        .collect::<Vec<_>>()
        .join("\n");
    for expected in [
        "wyrd_gateway_requests_total{operation=chat_completions,outcome=failed}",
        "wyrd_gateway_request_duration_seconds{operation=chat_completions,outcome=failed}",
        "wyrd_gateway_attempts_total{operation=chat_completions,outcome=succeeded}",
        "wyrd_gateway_routing_total{result=primary}",
    ] {
        assert!(series.contains(expected), "{expected} in {series}");
    }
    assert!(
        !series
            .contains("wyrd_gateway_requests_total{operation=chat_completions,outcome=succeeded}")
            && !series.contains("wyrd_gateway_tokens_total")
            && !series.contains("wyrd_gateway_call_cost"),
        "no success or accounted usage and cost is reported: {series}"
    );
    let published = |table: &str| {
        let frames = scribe
            .received()
            .into_iter()
            .filter(|frame| frame.table.contains(table))
            .collect::<Vec<_>>();
        assert_eq!(frames.len(), 1, "one {table} batch");
        String::from_utf8_lossy(&frames[0].ipc).into_owned()
    };
    let row = published("calls");
    let upstream = wyrd_gateway::outcome_error_code(GatewayCallOutcome::Failed)
        .expect("a failed outcome has a code");
    assert_ne!(error.code(), upstream, "the accounting error is distinct");
    assert!(
        row.contains("failed") && !row.contains("succeeded"),
        "the call row says failed: {row}"
    );
    assert!(
        row.contains(error.code()) && !row.contains(upstream),
        "the call row carries the caller's stable error: {row}"
    );
    assert!(
        !row.contains("input_tokens") && !row.contains("output_tokens"),
        "the call row leaves usage unknown: {row}"
    );
    assert!(
        published("spans").contains("succeeded"),
        "the published attempt span keeps the provider outcome"
    );

    let logs = traces.text();
    let closed = |name: &str| {
        logs.lines()
            .filter(|line| line.contains(": close"))
            .filter(|line| {
                line.split_once("}: ")
                    .and_then(|(path, _)| path.rsplit("}:").next()?.split_once('{'))
                    .is_some_and(|(span, _)| span.rsplit([':', ' ']).next() == Some(name))
            })
            .map(str::to_owned)
            .collect::<Vec<_>>()
    };
    let (call, attempt) = (closed("gateway.call"), closed("gateway.attempt"));
    assert_eq!((call.len(), attempt.len()), (1, 1), "{logs}");
    assert!(call[0].contains("outcome=\"failed\""), "{call:?}");
    assert!(
        call[0].contains(&format!("error.code={:?}", error.code())),
        "{call:?}"
    );
    assert!(attempt[0].contains("outcome=\"succeeded\""), "{attempt:?}");
    drain_gateway(&state).await;
}

/// One public inference ingress that reads the `wyrd-gateway-fallback`
/// header, with the built-in deployments that serve it.
#[derive(Debug, Clone, Copy)]
enum FallbackIngress {
    /// `POST /v1/chat/completions` served by `openai`.
    ChatCompletions,
    /// `POST /v1/responses` served by `openai`.
    Responses,
    /// `POST /v1/messages` served by `anthropic`.
    AnthropicMessages,
    /// `POST /v1beta/models/{model}:generateContent` served by `gemini`.
    GeminiGenerateContent,
}

impl FallbackIngress {
    /// Every ingress the header applies to.
    const ALL: [Self; 4] = [
        Self::ChatCompletions,
        Self::Responses,
        Self::AnthropicMessages,
        Self::GeminiGenerateContent,
    ];

    /// Provider whose built-in deployments serve this ingress.
    const fn provider(self) -> &'static str {
        match self {
            Self::ChatCompletions | Self::Responses => "openai",
            Self::AnthropicMessages => "anthropic",
            Self::GeminiGenerateContent => "gemini",
        }
    }

    /// Model name of this ingress's deployment `suffix`; `a` always fails
    /// upstream and `b` always answers.
    fn model(self, suffix: &str) -> String {
        let family = match self {
            Self::ChatCompletions | Self::Responses => "gpt",
            Self::AnthropicMessages => "claude",
            Self::GeminiGenerateContent => "gemini",
        };
        format!("{family}-{suffix}")
    }

    /// Exact model reference of deployment `suffix`.
    ///
    /// # Panics
    /// Panics when the fixed provider and model names stop forming a valid
    /// model projection.
    fn model_ref(self, suffix: &str) -> ModelRef {
        model(&format!("{}/{}", self.provider(), self.model(suffix)))
    }

    /// Calls this ingress's handler for model `a` with `headers`.
    async fn call(
        self,
        state: &AppState,
        caller: Result<Caller, WyrdErrorResponse>,
        headers: HeaderMap,
    ) -> Response {
        let state = State(state.clone());
        let requested = self.model("a");
        let messages = json!([{"role": "user", "content": "hi"}]);
        match self {
            Self::ChatCompletions => {
                let body = json!({"model": format!("openai/{requested}"), "messages": messages});
                super::routes::chat_completions(state, caller, headers, Ok(Json(body))).await
            }
            Self::Responses => {
                let body = json!({"model": format!("openai/{requested}"), "input": "hi"});
                super::routes::responses(state, caller, headers, Ok(Json(body))).await
            }
            Self::AnthropicMessages => {
                let body = json!({"model": requested, "max_tokens": 8, "messages": messages});
                super::ingress::anthropic_messages(state, caller, headers, Ok(Json(body))).await
            }
            Self::GeminiGenerateContent => {
                let body = json!({"contents": [{"role": "user", "parts": [{"text": "hi"}]}]});
                super::ingress::gemini_generate_content(
                    state,
                    caller,
                    axum::extract::Path(format!("{requested}:generateContent")),
                    axum::extract::RawQuery(None),
                    headers,
                    Ok(Json(body)),
                )
                .await
            }
        }
    }

    /// Stable Wyrd code of this ingress's error envelope `body`.
    fn code(self, body: &Value) -> Value {
        match self {
            Self::ChatCompletions | Self::Responses | Self::AnthropicMessages => {
                body["error"]["code"].clone()
            }
            Self::GeminiGenerateContent => body["error"]["details"][0]["reason"].clone(),
        }
    }
}

/// Mounts the upstream answers of every [`FallbackIngress`]: each `a` model
/// refuses retryably with `503` and each `b` model answers.
async fn mount_fallback_upstream(upstream: &MockServer) {
    let chat = json!({"id": "c-1", "object": "chat.completion", "choices": [], "usage": {"prompt_tokens": 1, "completion_tokens": 1, "total_tokens": 2}});
    let response = json!({"id": "r-1", "object": "response", "output": [], "usage": {"input_tokens": 1, "output_tokens": 1, "total_tokens": 2}});
    let message = json!({"id": "msg_1", "type": "message", "role": "assistant", "model": "claude-b", "content": [], "stop_reason": "end_turn", "stop_sequence": null, "usage": {"input_tokens": 1, "output_tokens": 1}});
    let generated = json!({"candidates": [], "usageMetadata": {"promptTokenCount": 1, "candidatesTokenCount": 1, "totalTokenCount": 2}});
    let refused = ResponseTemplate::new(503)
        .set_body_json(json!({"error": {"message": "overloaded", "type": "server_error"}}));
    for (route, answer) in [
        ("/v1/chat/completions", chat),
        ("/v1/responses", response),
        ("/v1/messages", message),
    ] {
        let family = if route == "/v1/messages" {
            "claude"
        } else {
            "gpt"
        };
        Mock::given(method("POST"))
            .and(path(route))
            .and(body_partial_json(json!({"model": format!("{family}-a")})))
            .respond_with(refused.clone())
            .mount(upstream)
            .await;
        Mock::given(method("POST"))
            .and(path(route))
            .and(body_partial_json(json!({"model": format!("{family}-b")})))
            .respond_with(ResponseTemplate::new(200).set_body_json(answer))
            .mount(upstream)
            .await;
    }
    Mock::given(method("POST"))
        .and(path("/v1beta/models/gemini-a:generateContent"))
        .respond_with(refused)
        .mount(upstream)
        .await;
    Mock::given(method("POST"))
        .and(path("/v1beta/models/gemini-b:generateContent"))
        .respond_with(ResponseTemplate::new(200).set_body_json(generated))
        .mount(upstream)
        .await;
}

/// Encodes `candidates` as a `wyrd-gateway-fallback` header map.
///
/// # Panics
/// Panics when the override cannot be serialized or its unpadded base64url
/// encoding is not a valid header value; both hold for every model reference.
fn fallback_headers(candidates: &[ModelRef]) -> HeaderMap {
    let fallback = GatewayFallbackOverride {
        candidates: candidates.to_vec(),
    };
    let value = fallback.to_header_value().expect("override encodes");
    let mut headers = HeaderMap::new();
    headers.insert(
        FALLBACK_HEADER,
        value.parse().expect("base64url is a header value"),
    );
    headers
}

/// Proves every public inference ingress consumes `wyrd-gateway-fallback`.
///
/// Without the header a failing model gets only tenant policy, which has no
/// fallback, so nothing reaches model `b`. With an override naming `b` the
/// call completes on `b`, and no provider ever sees the header. A malformed,
/// empty, repeated, oversized, empty-list, or self-referential value is
/// refused with `WYRD_GATEWAY_400_INVALID_REQUEST` naming `fallback` and
/// dispatches nothing, and authentication is reported before the header.
///
/// # Panics
/// Panics when the fixture, administration, a status, an envelope, or an
/// upstream expectation fails.
#[tokio::test]
async fn public_ingress_workflow_fallback() {
    let fixture = PgFixture::start().await.expect("fixture starts");
    let tenant = fixture.data_tenant_id();
    let upstream = MockServer::start().await;
    mount_fallback_upstream(&upstream).await;
    let root = url::Url::parse(&upstream.uri()).expect("mock url");
    let mut openai = root.clone();
    openai.set_path("/v1");
    let dispatch = wyrd_gateway::HttpProviderDispatch::new(
        skald_providers::EndpointPolicy::new(false),
        wyrd_gateway::BuiltinEndpoints {
            openai: Some(openai),
            anthropic: Some(root.clone()),
            gemini: Some(root.clone()),
            vertex: Some(root),
        },
    )
    .expect("gateway dispatch builds");
    let keys = super::pg_administration_tests::managed_keys(&[tenant], "v1", &["v1"]);
    let state = managed_replica(&fixture, keys, Arc::new(dispatch)).await;
    let admin = admin(tenant);
    let gateway = GatewayAdministration::new(&state);
    for (provider, auth, capabilities, served) in [
        (
            "openai",
            json!({"bearer": {"credential": "openai-key"}}),
            json!(["chat_completions", "responses"]),
            "gpt",
        ),
        (
            "anthropic",
            json!({"api_key_header": {"header": "x-api-key", "credential": "anthropic-key"}}),
            json!(["chat_completions"]),
            "claude",
        ),
        (
            "gemini",
            json!({"api_key_header": {"header": "x-goog-api-key", "credential": "gemini-key"}}),
            json!(["chat_completions"]),
            "gemini",
        ),
    ] {
        let credential = format!("{provider}-key");
        gateway
            .put_credential(
                &admin,
                &ProviderCredentialName::new(&credential).expect("credential name"),
                serde_json::from_value(json!({
                    "name": credential,
                    "provider": provider,
                    "source": {"managed_secret": {"secret": "sk-upstream"}},
                }))
                .expect("credential decodes"),
            )
            .await
            .expect("credential stores");
        for suffix in ["a", "b"] {
            let name = format!("{served}-{suffix}");
            gateway
                .put_deployment(
                    &admin,
                    &ProviderDeploymentName::new(&name).expect("name"),
                    serde_json::from_value(json!({
                        "name": name,
                        "model": model(&format!("{provider}/{name}")),
                        "adapter": provider,
                        "auth": auth,
                        "capabilities": capabilities,
                        "routing_weight": 1,
                    }))
                    .expect("deployment decodes"),
                )
                .await
                .expect("deployment stores");
        }
    }
    gateway
        .put_governance(
            &admin,
            serde_json::from_value(json!({
                "limits": [], "budgets": [], "pricing": [], "unknown_cost": "allow_unpriced",
            }))
            .expect("governance decodes"),
        )
        .await
        .expect("governance stores");
    let caller = invoker(
        tenant,
        11,
        ["openai", "anthropic", "gemini"].map(|provider| {
            Permission::gateway_invoke(GatewayAccess::Provider {
                provider: ProviderId::new(provider).expect("provider"),
            })
        }),
    );
    let reached = |requests: &[Request], name: &str| {
        requests.iter().any(|request| {
            request.url.path().contains(name)
                || serde_json::from_slice::<Value>(&request.body)
                    .is_ok_and(|body| body["model"] == name)
        })
    };

    for ingress in FallbackIngress::ALL {
        let b = ingress.model("b");
        let earlier = upstream.received_requests().await.expect("recording").len();
        let unmodified = ingress
            .call(&state, Ok(caller.clone()), HeaderMap::new())
            .await;
        assert_ne!(unmodified.status(), StatusCode::OK, "{ingress:?}");
        let requests = upstream.received_requests().await.expect("recording");
        assert!(
            !reached(&requests[earlier..], &b),
            "{ingress:?}: without the header only tenant policy applies"
        );

        let overridden = ingress
            .call(
                &state,
                Ok(caller.clone()),
                fallback_headers(&[ingress.model_ref("b")]),
            )
            .await;
        let status = overridden.status();
        assert_eq!(
            status,
            StatusCode::OK,
            "{ingress:?} {}",
            body_json(overridden).await
        );
        let requests = upstream.received_requests().await.expect("recording");
        assert!(
            reached(&requests[earlier..], &b),
            "{ingress:?}: the override reaches b"
        );

        let encode = |document: Value| URL_SAFE_NO_PAD.encode(document.to_string());
        let requested = json!({"provider": ingress.provider(), "model": ingress.model("a")});
        let refused = [
            ("malformed", vec!["%%%".to_owned()]),
            ("empty value", vec![String::new()]),
            ("repeated", {
                let value = fallback_headers(&[ingress.model_ref("b")])[FALLBACK_HEADER]
                    .to_str()
                    .expect("ascii")
                    .to_owned();
                vec![value.clone(), value]
            }),
            ("oversized", vec!["A".repeat(8 * 1024 + 1)]),
            ("empty list", vec![encode(json!({"candidates": []}))]),
            (
                "self-referential",
                vec![encode(json!({"candidates": [requested]}))],
            ),
        ];
        for (case, values) in refused {
            let mut headers = HeaderMap::new();
            for value in values {
                headers.append(FALLBACK_HEADER, value.parse().expect("header value"));
            }
            let dispatched = upstream.received_requests().await.expect("recording").len();
            let answer = ingress.call(&state, Ok(caller.clone()), headers).await;
            assert_eq!(
                answer.status(),
                StatusCode::BAD_REQUEST,
                "{ingress:?} {case}"
            );
            let body = body_json(answer).await;
            assert_eq!(
                ingress.code(&body),
                "WYRD_GATEWAY_400_INVALID_REQUEST",
                "{ingress:?} {case}: {body}"
            );
            assert!(
                body["error"]["message"]
                    .as_str()
                    .is_some_and(|message| message.contains("fallback")),
                "{ingress:?} {case}: {body}"
            );
            if matches!(
                ingress,
                FallbackIngress::ChatCompletions | FallbackIngress::Responses
            ) {
                assert_eq!(body["error"]["param"], "fallback", "{ingress:?} {case}");
            }
            assert_eq!(
                upstream.received_requests().await.expect("recording").len(),
                dispatched,
                "{ingress:?} {case}: a refused header dispatches nothing"
            );
        }

        let unauthenticated = ingress
            .call(
                &state,
                Err(WyrdErrorResponse::from(WyrdError::Unauthenticated {
                    message: "missing token".to_owned(),
                    details: json!({}),
                })),
                fallback_headers(&[]),
            )
            .await;
        assert_eq!(
            unauthenticated.status(),
            StatusCode::UNAUTHORIZED,
            "{ingress:?}: authentication precedes the header"
        );
    }

    let requests = upstream.received_requests().await.expect("recording");
    assert!(
        requests
            .iter()
            .all(|request| !request.headers.contains_key(FALLBACK_HEADER)),
        "the header never reaches a provider"
    );
    drain_gateway(&state).await;
}
