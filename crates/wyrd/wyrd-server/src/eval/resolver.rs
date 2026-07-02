//! Tenant-scoped card resolver for eval runs (net-new).
//!
//! wyrd-server has no assembled `CardRef → card` resolver today. This module is
//! the load-bearing seam: **every** card row reachable from a run is loaded
//! through `TenantConn::acquire(pool, tenant_id)` so Postgres RLS — not a
//! `space`-string filter — is the enforcing backstop (`CardRef` carries `space`,
//! and space→tenant is not 1:1). Resolution **fails closed**: a ref that does
//! not resolve, or resolves outside the tenant, returns
//! `registry_card_not_found` (mapped to `WYRD_EVAL_404` at the handler edge).
//!
//! Scenario bytes are a three-hop chain and RLS protects the card *rows*, not the
//! object bytes:
//! 1. `eval_ref → Eval card` under `TenantConn`.
//! 2. `Eval.dataset → Data card` under the **same** tenant bind — a second
//!    explicit RLS hop, never a reuse of the client `CardRef`.
//! 3. The object-store location is derived **only** from the tenant-verified
//!    Data card (`tenant_path::build(tenant, data_card.uid, …)`) and fetched from
//!    that tenant-scoped path — never from the client `eval_ref` or an un-rebound
//!    `CardRef.space`.
//!
//! Server-side scoring (the judge/prompt `CardRef` path in `ScenarioScoring` /
//! `SkaldJudgeInvoker`) is **deferred** on this surface. When it is wired, its
//! judge resolution MUST route through [`resolve_card`] with the run's
//! `tenant_id`, exactly like `eval_ref` and `dataset` — otherwise a tenant-A run
//! naming a tenant-B judge is a cross-tenant read plus provider spend. The
//! `WYRD_EVAL` acceptance suite exercises this via [`resolve_card`] against a
//! Prompt card to prove the resolver these paths share is tenant-bound.

use sqlx::PgPool;
use wyrd_spec::DataTenantId;
use wyrd_spec::envelope::{CardKind, Spec};
use wyrd_spec::error::WyrdError;
use wyrd_spec::reference::CardRef;
use wyrd_spec::vala::eval::spec::DatasetRef;
use wyrd_spec::vala::eval::{EvalScenario, EvalScenarioCollection};
use wyrd_sql::TenantConn;
use wyrd_sql::queries::cards::get_card_by_ref;
use wyrd_sql::row_types::cards::ParsedCardRow;
use wyrd_storage::StorageHandle;
use wyrd_storage::tenant_path;

use super::error::eval_internal_error;

/// **Provisional** relative object key for a Data card's serialized
/// `EvalScenarioCollection`.
///
/// Eval scenario storage is not yet a settled part of the eval domain model, and
/// there is **no production writer** for this key today — only the acceptance
/// suite seeds it (via [`scenario_object_path`]). Until the storage model is
/// defined, `/v1/eval` scenario-open against a normally-registered Data card fails
/// closed with `WYRD_EVAL_500`. Tracked as a follow-up; do not treat this
/// convention as the durable contract.
const SCENARIO_OBJECT_RELATIVE: &str = "eval/scenario_collection.json";

/// Load one card by reference under the caller's tenant RLS binding.
///
/// The `WHERE … AND data_tenant_id = wyrd.current_tenant()` clause on
/// [`get_card_by_ref`] means a foreign-tenant ref returns
/// `registry_card_not_found`, which the caller maps to `WYRD_EVAL_404`. This is
/// the single tenant-bound card read every eval path funnels through — including
/// any future judge/prompt resolution.
///
/// # Errors
/// Propagates the raw resolution `WyrdError` (typically `registry_card_not_found`
/// on a missing/foreign ref); callers map it via the eval error edge.
pub(crate) async fn resolve_card(
    conn: &mut TenantConn<'_>,
    kind: CardKind,
    card_ref: &CardRef,
) -> Result<ParsedCardRow, WyrdError> {
    get_card_by_ref(
        conn,
        kind,
        &card_ref.space,
        &card_ref.name,
        &card_ref.version,
    )
    .await
}

/// Resolve one card by reference under a fresh tenant-bound transaction.
///
/// The single tenant-scoped card read every eval path shares — `eval_ref`,
/// `dataset`, and any future judge/prompt `CardRef` from server-side scoring.
/// A ref outside `tenant_id` returns `registry_card_not_found`. Exposed so the
/// acceptance suite can prove the resolver is tenant-bound for the (currently
/// deferred) judge path without standing up a scoring engine or a provider.
///
/// # Errors
/// Returns the raw resolution `WyrdError` (e.g. `registry_card_not_found`), or a
/// scrubbed internal error when the tenant transaction cannot be acquired.
pub async fn resolve_card_for_tenant(
    pool: &PgPool,
    tenant_id: DataTenantId,
    kind: CardKind,
    card_ref: &CardRef,
) -> Result<ParsedCardRow, WyrdError> {
    let mut conn = TenantConn::acquire(pool, tenant_id)
        .await
        .map_err(|error| eval_internal_error(format!("tenant conn: {error}")))?;
    let card = resolve_card(&mut conn, kind, card_ref).await?;
    // Read-only; drop rolls back. Commit is unnecessary but keeps the pool tidy.
    let _ = conn.commit().await;
    Ok(card)
}

/// Extract the required `dataset` reference from a resolved Eval card.
///
/// Fails closed when the card is not an Eval spec or declares no dataset — an
/// eval run on this offline pull-protocol surface needs a scenario dataset.
///
/// # Errors
/// Returns a scrubbed `WYRD_EVAL_500` when the spec kind is wrong or no dataset
/// is declared.
pub(crate) fn dataset_ref(eval_card: &ParsedCardRow) -> Result<DatasetRef, WyrdError> {
    let Spec::Eval(eval_spec) = &eval_card.spec else {
        return Err(eval_internal_error(format!(
            "resolved eval_ref is not an Eval card: {:?}",
            eval_card.kind
        )));
    };
    eval_spec
        .dataset
        .clone()
        .ok_or_else(|| eval_internal_error("eval card declares no dataset for offline scenarios"))
}

/// Fetch and parse the scenario collection for a tenant-verified Data card.
///
/// The object path is derived solely from `tenant_id` and the Data card's uid,
/// so bytes can only ever be read from the tenant's own object-store prefix.
///
/// # Errors
/// Returns a scrubbed `WYRD_EVAL_500` when the path is malformed, the object is
/// absent, or the payload fails to parse.
pub(crate) async fn load_scenarios(
    storage: &StorageHandle,
    tenant_id: DataTenantId,
    data_card: &ParsedCardRow,
) -> Result<Vec<EvalScenario>, WyrdError> {
    let card_uid = data_card.card_uid.as_uuid().to_string();
    let path = tenant_path::build(tenant_id, &card_uid, SCENARIO_OBJECT_RELATIVE);
    let validated = tenant_path::validate(&path, tenant_id)
        .map_err(|error| eval_internal_error(format!("scenario object path invalid: {error}")))?;
    let bytes = storage
        .get_object(&validated)
        .await
        .map_err(|error| eval_internal_error(format!("scenario object fetch failed: {error}")))?;
    let collection: EvalScenarioCollection = serde_json::from_slice(&bytes).map_err(|error| {
        eval_internal_error(format!("scenario collection parse failed: {error}"))
    })?;
    if collection.scenarios.is_empty() {
        return Err(eval_internal_error("scenario collection is empty"));
    }
    Ok(collection.scenarios)
}

/// Build the object-store key a Data card's scenario collection lives at.
///
/// Exposed for the acceptance suite, which seeds the collection object at the
/// exact tenant-scoped path this resolver reads from.
#[must_use]
pub fn scenario_object_path(tenant_id: DataTenantId, data_card_uid: &str) -> String {
    tenant_path::build(tenant_id, data_card_uid, SCENARIO_OBJECT_RELATIVE)
}
