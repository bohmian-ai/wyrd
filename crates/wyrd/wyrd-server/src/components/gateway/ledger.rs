//! Gateway admission and accounting ledger.
//!
//! [`GatewayLedger::admit`] decides a call's limits and budgets from the
//! cached tenant snapshot. A call no limit or budget covers touches Postgres
//! not at all. A covered call is checked and charged by one
//! `wyrd.gateway_admit` statement, which takes the tenant's admission
//! advisory lock, so concurrent admissions on every replica see each other's
//! committed limit charges and budget reservations before charging their own.
//!
//! Limits use fixed one-minute request and token windows plus concurrency
//! leases. Admission charges one request and holds the call's bounded token
//! exposure in its admission window; settlement replaces each hold by the
//! actual tokens of the attempts its limit target covers, and an abandoned
//! hold lapses with the window. Budgets reserve the bounded cost of every
//! attempt the call may bill and are settled with the call's actual cost,
//! zero when nothing was billed, or the whole reservation when billed cost is
//! unknown. An unsettled reservation counts against its budget at its
//! reserved cost, so a call that never settles is charged in full.
//!
//! [`Accounted::new`] prices the call's attempts in memory; its attempt and
//! call entries travel only with the call's capture.
//! [`GatewayLedger::settle`] persists what admission charged, in one
//! `wyrd.gateway_settle` statement, and only for a call that a limit or budget
//! covered: before a buffered answer is sent, so the caller's next call
//! never meets this call's charges, and once a stream ends.

use std::collections::{BTreeMap, BTreeSet};
use std::num::NonZeroU64;

use chrono::{DateTime, Datelike, Months, NaiveDate, Utc};
use serde_json::json;
use wyrd_gateway::{AttemptRecord, CallExecution, CallPlan, GatewayCost, PricedModel};
use wyrd_runtime::Principal;
use wyrd_spec::DataTenantId;
use wyrd_spec::auth::Attribution;
use wyrd_spec::error::WyrdError;
use wyrd_spec::gateway::{
    CurrencyCode, GatewayAccountingEntryId, GatewayAccountingEntryV1, GatewayBudget,
    GatewayBudgetPeriod, GatewayBudgetReservationId, GatewayCallId, GatewayGovernancePolicy,
    GatewayLimit, GatewayLimitSubject, GatewayPolicySubject, GatewayPolicyTarget,
    GatewayUsageAmount, ModelRef, UnknownCostPolicy,
};
use wyrd_sql::WyrdPostgres;
use wyrd_sql::queries::gateway_admission::{
    GatewayAdmissionOutcome, GatewayAdmissionWrite, GatewayAdmissions, GatewayBudgetCharge,
    GatewayHoldDelta, GatewayLimitCharge, GatewaySettlementEntry, GatewaySettlementWrite,
};

use super::service::{encode, internal, unavailable};

/// Identity, policy, and bounds of one call as the ledger sees it.
#[derive(Debug, Clone, Copy)]
pub(crate) struct LedgerCall<'s> {
    /// Logical call.
    pub call_id: GatewayCallId,
    /// Verified tenant.
    pub tenant: DataTenantId,
    /// Verified subject, whose limits and budgets the call is charged to.
    pub principal: &'s Principal,
    /// Subject and actor pair the accounting entry records.
    pub attribution: Attribution,
    /// Governance policy of the admitted snapshot.
    pub governance: &'s GatewayGovernancePolicy,
    /// Largest usage the call can incur, or `None` when it is unbounded.
    pub usage_bound: Option<&'s [GatewayUsageAmount]>,
    /// Admission time fixing pricing, limit windows, and budget periods.
    pub admitted_at: DateTime<Utc>,
    /// Time after which the call's leases lapse.
    pub expires_at: DateTime<Utc>,
}

/// Charges admission made for one call, settled when it is accounted.
#[derive(Debug, Clone, Default)]
pub(crate) struct Admission {
    /// Token holds charged to limit admission windows.
    holds: Vec<Hold>,
    /// Budget reservations created.
    reservations: Vec<Reservation>,
    /// Whether the call took a concurrency lease.
    leased: bool,
}

/// Tokens one admitted call holds in one limit's admission window.
#[derive(Debug, Clone)]
struct Hold {
    /// Limit window key.
    key: String,
    /// Limit target, so settlement counts only the attempts it covers.
    target: GatewayPolicyTarget,
    /// Tokens held.
    tokens: i64,
}

/// One budget reservation held by an admitted call.
#[derive(Debug, Clone)]
struct Reservation {
    /// Reservation identity.
    id: GatewayBudgetReservationId,
    /// Reserved cost.
    reserved: GatewayCost,
    /// Budget currency.
    currency: CurrencyCode,
}

/// One call's priced accounting, computed in memory from its execution.
#[derive(Debug)]
pub(crate) struct Accounted {
    /// Attempt entries then the call entry, which capture projects instead
    /// of pricing the call a second time.
    pub entries: Vec<GatewayAccountingEntryV1>,
    /// The statement settling what admission charged, or `None` when the
    /// call charged nothing.
    settlement: Option<Settlement>,
}

/// The settlement statement's arguments for one call.
#[derive(Debug)]
struct Settlement {
    /// Logical call.
    call_id: GatewayCallId,
    /// The call's admission window.
    window_start: DateTime<Utc>,
    /// Settlement time.
    now: DateTime<Utc>,
    /// Hold corrections.
    holds: Vec<GatewayHoldDelta>,
    /// Reservation settlements.
    settlements: Vec<GatewaySettlementEntry>,
    /// Whether the call's leases are released.
    release: bool,
}

impl Accounted {
    /// Prices every attempt and the logical call, and prepares the settlement
    /// of `admission`.
    ///
    /// Billable attempts are priced with the pricing version selected at
    /// admission; a non-billable attempt carries unknown usage and cost. The
    /// call's usage is the per-dimension sum and its cost the sum of attempt
    /// costs, each known only when every billable attempt's value is known.
    ///
    /// Reservations settle at the call's cost, zero when no attempt was
    /// billed, or in full when billed cost is unknown. Each token hold is
    /// replaced by the tokens of the billed attempts its limit target covers,
    /// released when none was billed, and kept when a covered billed
    /// attempt's usage is unknown. Pure: no IO.
    ///
    /// # Errors
    /// Returns `Internal` when an entry violates the ledger contract.
    pub(crate) fn new(
        call: &LedgerCall<'_>,
        admission: &Admission,
        execution: &CallExecution,
    ) -> Result<Self, WyrdError> {
        let now = Utc::now();
        let mut entries = Vec::with_capacity(execution.attempts.len() + 1);
        let mut usage: Option<BTreeMap<(String, String), GatewayCost>> = Some(BTreeMap::new());
        let mut cost: Option<(GatewayCost, CurrencyCode)> = None;
        let mut cost_known = true;
        let mut versions = BTreeSet::new();
        let mut billed = false;
        for attempt in &execution.attempts {
            let priced = PricedModel::select(call.governance, &attempt.model, call.admitted_at);
            let normalized = attempt
                .billable
                .then_some(())
                .and(attempt.usage.normalized.as_ref());
            let attempt_cost = priced.and_then(|p| Some((p, p.cost(normalized?)?)));
            if attempt.billable {
                billed = true;
                match (normalized, usage.as_mut()) {
                    (Some(amounts), Some(totals)) => {
                        for amount in amounts {
                            let slot = totals
                                .entry((amount.dimension.clone(), amount.unit.clone()))
                                .or_insert_with(GatewayCost::zero);
                            *slot = slot.clone() + GatewayCost::from_decimal(&amount.quantity);
                        }
                    }
                    _ => usage = None,
                }
                match &attempt_cost {
                    Some((pricing, value)) => {
                        versions.insert(pricing.version().to_owned());
                        let total = cost
                            .take()
                            .map_or_else(GatewayCost::zero, |(total, _)| total);
                        cost = Some((total + value.clone(), pricing.currency().clone()));
                    }
                    None => cost_known = false,
                }
            }
            let entry = GatewayAccountingEntryV1::AttemptAccounted {
                entry_id: GatewayAccountingEntryId::new_v7(),
                call_id: call.call_id,
                data_tenant_id: call.tenant,
                attempt_ordinal: attempt.ordinal,
                deployment: attempt.deployment.clone(),
                model: attempt.model.clone(),
                outcome: attempt.outcome,
                provider_usage_json: attempt.usage.provider_usage_json.clone(),
                normalized_usage: normalized.cloned(),
                cost: attempt_cost.as_ref().map(|(_, value)| value.to_decimal()),
                currency: attempt_cost.as_ref().map(|(p, _)| p.currency().clone()),
                pricing_version: attempt_cost.as_ref().map(|(p, _)| p.version().to_owned()),
                accounted_at: now,
            };
            entry.validate().map_err(internal)?;
            entries.push(entry);
        }

        let usage = usage.filter(|_| billed);
        let cost = cost.filter(|_| billed && cost_known);
        let entry = GatewayAccountingEntryV1::CallAccounted {
            entry_id: GatewayAccountingEntryId::new_v7(),
            call_id: call.call_id,
            data_tenant_id: call.tenant,
            sub_principal_id: call.attribution.sub_principal_id,
            act_principal_id: call.attribution.act_principal_id,
            outcome: execution.outcome,
            normalized_usage: usage.map(|totals| {
                totals
                    .into_iter()
                    .map(|((dimension, unit), quantity)| GatewayUsageAmount {
                        dimension,
                        unit,
                        quantity: quantity.to_decimal(),
                    })
                    .collect()
            }),
            cost: cost.as_ref().map(|(total, _)| total.to_decimal()),
            currency: cost.as_ref().map(|(_, currency)| currency.clone()),
            pricing_versions: if cost.is_some() {
                versions
            } else {
                BTreeSet::new()
            },
            accounted_at: now,
        };
        entry.validate().map_err(internal)?;
        entries.push(entry);

        let charged = !admission.holds.is_empty() || !admission.reservations.is_empty();
        let settlement = (charged || admission.leased)
            .then(|| -> Result<Settlement, WyrdError> {
                let mut settlements = Vec::with_capacity(admission.reservations.len());
                for reservation in &admission.reservations {
                    let actual = match &cost {
                        Some((total, _)) => total.clone(),
                        None if billed => reservation.reserved.clone(),
                        None => GatewayCost::zero(),
                    };
                    settlements.push(settlement(call, reservation, &actual, now)?);
                }
                let holds = admission
                    .holds
                    .iter()
                    .map(|hold| GatewayHoldDelta {
                        key: hold.key.clone(),
                        delta: covered_tokens(&hold.target, &execution.attempts)
                            .map_or(0, |tokens| tokens - hold.tokens),
                    })
                    .collect();
                Ok(Settlement {
                    call_id: call.call_id,
                    window_start: minute_start(call.admitted_at),
                    now,
                    holds,
                    settlements,
                    release: admission.leased,
                })
            })
            .transpose()?;
        Ok(Self {
            entries,
            settlement,
        })
    }
}

/// Admits and settles gateway calls against the tenant pool.
pub(crate) struct GatewayLedger<'p> {
    /// Tenant pool the admission and settlement statements run on.
    postgres: &'p WyrdPostgres,
}

impl<'p> GatewayLedger<'p> {
    /// Borrows the tenant pool.
    pub(crate) fn new(postgres: &'p WyrdPostgres) -> Self {
        Self { postgres }
    }

    /// Admits `call`, narrowing `plan` to candidates its limits and budgets
    /// allow, and charges the result.
    ///
    /// In memory, from the admitted snapshot, it:
    ///
    /// 1. drops candidates covered by a token limit whose exposure (the usage
    ///    bound's tokens per covered attempt) is unbounded;
    /// 2. when a budget applies or unknown cost is rejected, drops candidates
    ///    whose cost cannot be bounded from active pricing and the usage
    ///    bound; and
    /// 3. sums the bounded cost of every remaining candidate as the reserve
    ///    of each applicable budget.
    ///
    /// When no limit covers a remaining candidate and no budget applies, the
    /// call is admitted with no IO. Otherwise one `wyrd.gateway_admit`
    /// statement checks every covering limit's request, token, and
    /// concurrency use and every budget's period spend under the tenant
    /// admission lock, and charges all of them only when all have room. A
    /// limit it reports exhausted removes the candidates it covers, and the
    /// statement runs again for what remains, so each repetition removes at
    /// least one candidate. Nothing is charged by a rejection.
    ///
    /// # Errors
    /// Returns `GatewayLimitExceeded` with `retry_after_seconds`,
    /// `GatewayCostUnbounded` (also for unbounded token exposure), or
    /// `GatewayBudgetExceeded` when no candidate survives,
    /// `ServiceUnavailable` when storage fails, and `Internal` for an entry
    /// that violates the ledger contract.
    pub(crate) async fn admit(
        &self,
        call: &LedgerCall<'_>,
        plan: &mut CallPlan,
    ) -> Result<Admission, WyrdError> {
        let now = call.admitted_at;
        let mut limits = Vec::new();
        for limit in &call.governance.limits {
            if limit_applies(limit, call.principal) {
                limits.push((limit, encode(&(&limit.subject, &limit.target))?.to_string()));
            }
        }
        let budgets: Vec<&GatewayBudget> = call
            .governance
            .budgets
            .iter()
            .filter(|budget| budget_applies(budget, call.principal))
            .collect();
        let attempt_tokens = call.usage_bound.map(|bound| {
            whole_units(
                &bound
                    .iter()
                    .filter(|amount| amount.unit == "tokens")
                    .map(|amount| GatewayCost::from_decimal(&amount.quantity))
                    .sum(),
            )
        });
        let exposure = |plan: &CallPlan, target| {
            attempt_tokens.map(|tokens| tokens.saturating_mul(covered_attempts(plan, target)))
        };
        let bound = |model: &ModelRef| {
            PricedModel::select(call.governance, model, now)?.cost(call.usage_bound?)
        };

        let mut unbounded = false;
        let unbounded_targets: Vec<&GatewayPolicyTarget> = limits
            .iter()
            .filter(|(limit, _)| {
                limit.tokens_per_minute.is_some() && exposure(plan, &limit.target).is_none()
            })
            .map(|(limit, _)| &limit.target)
            .collect();
        for target in &unbounded_targets {
            unbounded |= covered_attempts(plan, target) > 0;
        }
        plan.retain(|model| !unbounded_targets.iter().any(|target| covers(target, model)));
        if plan.candidates.is_empty() {
            return Err(cost_unbounded());
        }
        if !budgets.is_empty() || call.governance.unknown_cost == UnknownCostPolicy::Reject {
            plan.retain(|model| bound(model).is_some());
            if plan.candidates.is_empty() {
                return Err(cost_unbounded());
            }
        }

        loop {
            let covering: Vec<&(&GatewayLimit, String)> = limits
                .iter()
                .filter(|(limit, _)| {
                    plan.candidates
                        .iter()
                        .any(|candidate| covers(&limit.target, &candidate.model))
                })
                .collect();
            let charges: Vec<GatewayLimitCharge> = covering
                .iter()
                .map(|(limit, key)| GatewayLimitCharge {
                    key: key.clone(),
                    requests: limit.requests_per_minute.map(NonZeroU64::get),
                    tokens: limit.tokens_per_minute.map(NonZeroU64::get),
                    concurrent: limit.concurrent_calls.map(NonZeroU64::get),
                    hold: exposure(plan, &limit.target).unwrap_or(0),
                    lease: limit.concurrent_calls.is_some(),
                })
                .collect();
            let reserve: GatewayCost = if budgets.is_empty() {
                GatewayCost::zero()
            } else {
                plan.candidates
                    .iter()
                    .map(|candidate| &candidate.model)
                    .filter_map(bound)
                    .sum()
            };
            let mut reservations = Vec::new();
            let mut budget_charges = Vec::with_capacity(budgets.len());
            for budget in &budgets {
                let (charge, reservation) = reservation(call, budget, &reserve)?;
                budget_charges.push(charge);
                reservations.extend(reservation);
            }
            if charges.is_empty() && budget_charges.is_empty() {
                return Ok(Admission::default());
            }

            let mut conn = self.postgres.conn().await.map_err(unavailable)?;
            let outcome = GatewayAdmissions::new(&mut conn, call.tenant)
                .admit(&GatewayAdmissionWrite {
                    call_id: call.call_id.as_uuid(),
                    window_start: minute_start(now),
                    now,
                    expires_at: call.expires_at,
                    limits: &charges,
                    budgets: &budget_charges,
                })
                .await
                .map_err(unavailable)?;
            match outcome {
                GatewayAdmissionOutcome::Admitted => {
                    return Ok(Admission {
                        leased: charges.iter().any(|charge| charge.lease),
                        holds: covering
                            .iter()
                            .zip(charges)
                            .map(|((limit, _), charge)| Hold {
                                key: charge.key,
                                target: limit.target.clone(),
                                tokens: charge.hold,
                            })
                            .collect(),
                        reservations,
                    });
                }
                GatewayAdmissionOutcome::BudgetExceeded => return Err(budget_exceeded()),
                GatewayAdmissionOutcome::Exhausted(keys) => {
                    let exhausted: Vec<&GatewayPolicyTarget> = covering
                        .iter()
                        .filter(|(_, key)| keys.contains(key))
                        .map(|(limit, _)| &limit.target)
                        .collect();
                    plan.retain(|model| !exhausted.iter().any(|target| covers(target, model)));
                    if plan.candidates.is_empty() {
                        return Err(if unbounded {
                            cost_unbounded()
                        } else {
                            limit_exceeded(now)
                        });
                    }
                }
            }
        }
    }

    /// Persists what admission charged `accounted`'s call in one
    /// `wyrd.gateway_settle` statement, and does nothing for a call that
    /// charged nothing.
    ///
    /// The statement replaces each hold by its actual tokens, appends each
    /// reservation settlement (a replay appends nothing), releases the
    /// call's leases, and prunes windows and leases no admission reads.
    ///
    /// # Errors
    /// Returns `ServiceUnavailable` when storage fails. Nothing commits then:
    /// leases lapse at their expiry and each reservation keeps counting at
    /// its reserved cost.
    pub(crate) async fn settle(
        &self,
        tenant: DataTenantId,
        accounted: &Accounted,
    ) -> Result<(), WyrdError> {
        let Some(settlement) = &accounted.settlement else {
            return Ok(());
        };
        let mut conn = self.postgres.conn().await.map_err(unavailable)?;
        GatewayAdmissions::new(&mut conn, tenant)
            .settle(&GatewaySettlementWrite {
                call_id: settlement.call_id.as_uuid(),
                window_start: settlement.window_start,
                window_floor: minute_start(settlement.now) - chrono::TimeDelta::minutes(1),
                now: settlement.now,
                holds: &settlement.holds,
                settlements: &settlement.settlements,
                release: settlement.release,
            })
            .await
            .map_err(unavailable)
    }
}

/// The admission statement's check of `budget` against `reserve`, and the
/// reservation it creates, which is `None` for a zero reserve.
///
/// # Errors
/// Returns `Internal` when the budget period is out of range or the
/// reservation entry violates the ledger contract.
fn reservation(
    call: &LedgerCall<'_>,
    budget: &GatewayBudget,
    reserve: &GatewayCost,
) -> Result<(GatewayBudgetCharge, Option<Reservation>), WyrdError> {
    let (period_start, period_end) = period_bounds(budget.period, call.admitted_at)
        .ok_or_else(|| internal("budget period is out of range"))?;
    let mut charge = GatewayBudgetCharge {
        subject: encode(&budget.subject)?,
        period_start: encode(&period_start)?,
        period_end: encode(&period_end)?,
        amount: budget.amount.as_str().to_owned(),
        reserve: reserve.to_decimal().as_str().to_owned(),
        entry_id: None,
        entry: None,
    };
    if reserve.is_zero() {
        return Ok((charge, None));
    }
    let reservation = Reservation {
        id: GatewayBudgetReservationId::new_v7(),
        reserved: reserve.clone(),
        currency: budget.currency.clone(),
    };
    let entry_id = GatewayAccountingEntryId::new_v7();
    let entry = GatewayAccountingEntryV1::BudgetReservationCreated {
        entry_id,
        reservation_id: reservation.id,
        call_id: call.call_id,
        data_tenant_id: call.tenant,
        subject: budget.subject.clone(),
        reserved_cost: reserve.to_decimal(),
        currency: budget.currency.clone(),
        created_at: call.admitted_at,
        period_start,
        period_end,
        expires_at: call.expires_at,
    };
    entry.validate().map_err(internal)?;
    charge.entry_id = Some(entry_id.as_uuid());
    charge.entry = Some(encode(&entry)?);
    Ok((charge, Some(reservation)))
}

/// The settlement entry of `reservation` at `actual` cost.
///
/// # Errors
/// Returns `Internal` when the entry violates the ledger contract.
fn settlement(
    call: &LedgerCall<'_>,
    reservation: &Reservation,
    actual: &GatewayCost,
    now: DateTime<Utc>,
) -> Result<GatewaySettlementEntry, WyrdError> {
    let entry_id = GatewayAccountingEntryId::new_v7();
    let entry = GatewayAccountingEntryV1::BudgetReservationSettled {
        entry_id,
        reservation_id: reservation.id,
        call_id: call.call_id,
        data_tenant_id: call.tenant,
        actual_cost: actual.to_decimal(),
        released_cost: reservation.reserved.saturating_sub(actual).to_decimal(),
        currency: reservation.currency.clone(),
        settled_at: now,
    };
    entry.validate().map_err(internal)?;
    Ok(GatewaySettlementEntry {
        entry_id: entry_id.as_uuid(),
        entry: encode(&entry)?,
    })
}

/// True when a limit applies to `principal`.
fn limit_applies(limit: &GatewayLimit, principal: &Principal) -> bool {
    match &limit.subject {
        GatewayLimitSubject::Tenant => true,
        GatewayLimitSubject::Principal { principal_id } => *principal_id == principal.id,
    }
}

/// True when a budget applies to `principal` directly or through a role.
fn budget_applies(budget: &GatewayBudget, principal: &Principal) -> bool {
    match &budget.subject {
        GatewayPolicySubject::Tenant => true,
        GatewayPolicySubject::Principal { principal_id } => *principal_id == principal.id,
        GatewayPolicySubject::Role { role_name } => principal
            .roles
            .iter()
            .any(|role| role.as_str() == role_name.as_str()),
    }
}

/// True when `target` covers calls to `model`.
fn covers(target: &GatewayPolicyTarget, model: &ModelRef) -> bool {
    match target {
        GatewayPolicyTarget::All => true,
        GatewayPolicyTarget::Deployment { deployment } => *deployment == model.deployment,
        GatewayPolicyTarget::Model { model: covered } => covered == model,
    }
}

/// Number of `plan` attempts, one per candidate, whose model `target` covers.
fn covered_attempts(plan: &CallPlan, target: &GatewayPolicyTarget) -> i64 {
    let attempts = plan
        .candidates
        .iter()
        .filter(|candidate| covers(target, &candidate.model))
        .count();
    i64::try_from(attempts).unwrap_or(i64::MAX)
}

/// Whole tokens billed by the `attempts` whose model `target` covers, zero
/// when none was billed, or `None` when a covered billed attempt's usage is
/// unknown.
fn covered_tokens(target: &GatewayPolicyTarget, attempts: &[AttemptRecord]) -> Option<i64> {
    let mut total = GatewayCost::zero();
    for attempt in attempts
        .iter()
        .filter(|attempt| attempt.billable && covers(target, &attempt.model))
    {
        for amount in attempt.usage.normalized.as_ref()? {
            if amount.unit == "tokens" {
                total = total + GatewayCost::from_decimal(&amount.quantity);
            }
        }
    }
    Some(whole_units(&total))
}

/// Start of the UTC minute containing `at`.
fn minute_start(at: DateTime<Utc>) -> DateTime<Utc> {
    let seconds = at.timestamp();
    DateTime::from_timestamp(seconds - seconds.rem_euclid(60), 0).unwrap_or(at)
}

/// Integral part of a non-negative quantity, saturating at `i64::MAX`.
fn whole_units(quantity: &GatewayCost) -> i64 {
    let decimal = quantity.to_decimal();
    let integer = decimal.as_str().split('.').next().unwrap_or_default();
    integer.parse().unwrap_or(i64::MAX)
}

/// UTC calendar period containing `at` as `[start, end)`.
fn period_bounds(
    period: GatewayBudgetPeriod,
    at: DateTime<Utc>,
) -> Option<(DateTime<Utc>, DateTime<Utc>)> {
    let day = at.date_naive();
    let (start, end) = match period {
        GatewayBudgetPeriod::CalendarDayUtc => (day, day.succ_opt()?),
        GatewayBudgetPeriod::CalendarMonthUtc => {
            let first = NaiveDate::from_ymd_opt(day.year(), day.month(), 1)?;
            (first, first.checked_add_months(Months::new(1))?)
        }
    };
    Some((
        start.and_hms_opt(0, 0, 0)?.and_utc(),
        end.and_hms_opt(0, 0, 0)?.and_utc(),
    ))
}

/// Stable limit rejection with seconds until the next minute window.
fn limit_exceeded(now: DateTime<Utc>) -> WyrdError {
    let retry_after = 60 - now.timestamp().rem_euclid(60);
    WyrdError::GatewayLimitExceeded {
        message: "a gateway request, token, or concurrency limit is exhausted".to_owned(),
        details: json!({ "retry_after_seconds": retry_after }),
    }
}

/// Stable budget rejection.
fn budget_exceeded() -> WyrdError {
    WyrdError::GatewayBudgetExceeded {
        message: "a gateway budget cannot cover this call".to_owned(),
        details: json!({}),
    }
}

/// Stable rejection of a call whose cost cannot be bounded.
fn cost_unbounded() -> WyrdError {
    WyrdError::GatewayCostUnbounded {
        message: "gateway call cost cannot be bounded".to_owned(),
        details: json!({}),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Calendar periods and minute windows are the UTC boundaries containing
    /// the admission time.
    #[test]
    fn periods_and_windows_use_utc_boundaries() {
        let at: DateTime<Utc> = "2026-12-31T23:59:59.5Z".parse().expect("time");
        let text = |value: DateTime<Utc>| value.to_rfc3339();
        let (start, end) = period_bounds(GatewayBudgetPeriod::CalendarMonthUtc, at).expect("month");
        assert_eq!(
            (text(start), text(end)),
            (
                "2026-12-01T00:00:00+00:00".to_owned(),
                "2027-01-01T00:00:00+00:00".to_owned()
            )
        );
        let (start, end) = period_bounds(GatewayBudgetPeriod::CalendarDayUtc, at).expect("day");
        assert_eq!(
            (text(start), text(end)),
            (
                "2026-12-31T00:00:00+00:00".to_owned(),
                "2027-01-01T00:00:00+00:00".to_owned()
            )
        );
        assert_eq!(text(minute_start(at)), "2026-12-31T23:59:00+00:00");
        assert_eq!(
            whole_units(&GatewayCost::parse("1500.75").expect("cost")),
            1500
        );
    }
}
