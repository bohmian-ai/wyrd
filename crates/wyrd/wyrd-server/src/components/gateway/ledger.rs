//! Tenant accounting ledger handle for gateway admission and settlement.
//!
//! [`GatewayLedger`] borrows the caller's tenant transaction and never
//! commits it. Admission first takes the tenant's admission advisory lock, so
//! concurrent admissions on every replica see each other's committed limit
//! charges and budget reservations before charging their own.
//!
//! Limits use fixed one-minute request and token windows plus concurrency
//! leases. Admission charges one request and holds the call's bounded token
//! exposure in its admission window; accounting settles each window to the
//! actual tokens of the attempts its limit target covers, and an abandoned
//! hold lapses with the window. Budgets
//! reserve the bounded cost of every attempt the call may bill and are settled
//! with the call's actual cost, zero when nothing was billed, or the whole
//! reservation when billed cost is unknown.

use std::collections::{BTreeMap, BTreeSet};
use std::num::NonZeroU64;

use chrono::{DateTime, Datelike, Months, NaiveDate, Utc};
use serde_json::json;
use wyrd_gateway::{AttemptRecord, CallExecution, CallPlan, GatewayCost, PricedModel};
use wyrd_runtime::Principal;
use wyrd_spec::DataTenantId;
use wyrd_spec::error::WyrdError;
use wyrd_spec::gateway::{
    CurrencyCode, GatewayAccountingEntryId, GatewayAccountingEntryV1, GatewayBudget,
    GatewayBudgetPeriod, GatewayBudgetReservationId, GatewayCallId, GatewayGovernancePolicy,
    GatewayLimit, GatewayLimitSubject, GatewayPolicySubject, GatewayPolicyTarget,
    GatewayUsageAmount, ModelRef, UnknownCostPolicy,
};
use wyrd_sql::TenantConn;
use wyrd_sql::queries::gateway::{
    GatewayAccountingEntryWrite, append_gateway_accounting_entry, charge_gateway_limit_window,
    expired_gateway_reservations, gateway_budget_spend, gateway_limit_usage,
    insert_gateway_call_lease, lock_gateway_admission, prune_gateway_admission,
    release_gateway_call_leases,
};

use super::service::{decode, encode, internal, unavailable};

/// Expired reservations one admission settles; the rest wait for the next.
const RECONCILE_BATCH: i64 = 32;

/// Identity, policy, and bounds of one call as the ledger sees it.
#[derive(Debug, Clone, Copy)]
pub(crate) struct LedgerCall<'s> {
    /// Logical call.
    pub call_id: GatewayCallId,
    /// Verified tenant.
    pub tenant: DataTenantId,
    /// Verified caller.
    pub principal: &'s Principal,
    /// Governance policy of the admitted snapshot.
    pub governance: &'s GatewayGovernancePolicy,
    /// Largest usage the call can incur, or `None` when it is unbounded.
    pub usage_bound: Option<&'s [GatewayUsageAmount]>,
    /// Admission time fixing pricing, limit windows, and budget periods.
    pub admitted_at: DateTime<Utc>,
    /// Time after which leases lapse and reconciliation may settle.
    pub expires_at: DateTime<Utc>,
}

/// Charges admission made for one call, released when it is accounted.
#[derive(Debug, Clone, Default)]
pub(crate) struct Admission {
    /// Token holds charged to limit admission windows.
    holds: Vec<Hold>,
    /// Budget reservations created.
    reservations: Vec<Reservation>,
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

/// Borrowed tenant transaction carrying every ledger and admission write.
pub(crate) struct GatewayLedger<'c, 'a> {
    /// Caller-owned tenant transaction.
    conn: &'c mut TenantConn<'a>,
}

impl<'c, 'a> GatewayLedger<'c, 'a> {
    /// Borrows `conn` for ledger work; the caller commits it.
    pub(crate) fn new(conn: &'c mut TenantConn<'a>) -> Self {
        Self { conn }
    }

    /// Admits `call`, narrowing `plan` to candidates its limits and budgets
    /// allow, and charges the result.
    ///
    /// Under the tenant admission lock it prunes stale windows and leases,
    /// settles a bounded batch of expired reservations, then:
    ///
    /// 1. drops candidates covered by an exhausted request or concurrency
    ///    limit, or by a token limit whose held tokens plus the call's token
    ///    exposure (the usage bound's tokens per covered attempt) exceed it or
    ///    whose exposure is unbounded;
    /// 2. when a budget applies or unknown cost is rejected, drops candidates
    ///    whose cost cannot be bounded from active pricing and the usage
    ///    bound;
    /// 3. checks every applicable budget's spend plus the summed bound of
    ///    every remaining `(candidate, deployment)` attempt and reserves that
    ///    sum against each; and
    /// 4. charges one request, the covered token exposure as a hold, and,
    ///    where configured, a concurrency lease to every limit covering a
    ///    remaining candidate.
    ///
    /// Nothing is durable until the caller commits, so a rejection charges
    /// nothing.
    ///
    /// # Errors
    /// Returns `GatewayLimitExceeded` with `retry_after_seconds`,
    /// `GatewayCostUnbounded` (also for unbounded token exposure), or
    /// `GatewayBudgetExceeded` when no candidate survives, `ServiceUnavailable` when storage fails, and `Internal` for an
    /// undecodable ledger entry.
    pub(crate) async fn admit(
        &mut self,
        call: &LedgerCall<'_>,
        plan: &mut CallPlan,
    ) -> Result<Admission, WyrdError> {
        let now = call.admitted_at;
        let window = minute_start(now);
        lock_gateway_admission(self.conn)
            .await
            .map_err(unavailable)?;
        prune_gateway_admission(self.conn, window - chrono::TimeDelta::minutes(1), now)
            .await
            .map_err(unavailable)?;
        self.reconcile(call.tenant, now).await?;

        let mut limits = Vec::new();
        for limit in &call.governance.limits {
            if limit_applies(limit, call.principal) {
                limits.push((limit, encode(&(&limit.subject, &limit.target))?.to_string()));
            }
        }
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
        let mut exhausted = Vec::new();
        let mut unbounded = false;
        for (limit, key) in &limits {
            let (requests, tokens, leases) = gateway_limit_usage(self.conn, key, window, now)
                .await
                .map_err(unavailable)?;
            let over_tokens =
                limit
                    .tokens_per_minute
                    .is_some_and(|cap| match exposure(plan, &limit.target) {
                        Some(held) => u64::try_from(tokens.saturating_add(held))
                            .is_ok_and(|total| total > cap.get()),
                        None => {
                            unbounded |= covered_attempts(plan, &limit.target) > 0;
                            true
                        }
                    });
            if reached(limit.requests_per_minute, requests)
                || over_tokens
                || reached(limit.concurrent_calls, leases)
            {
                exhausted.push(&limit.target);
            }
        }
        plan.retain(|model| !exhausted.iter().any(|target| covers(target, model)));
        if plan.candidates.is_empty() {
            return Err(if unbounded {
                cost_unbounded()
            } else {
                limit_exceeded(now)
            });
        }

        let budgets: Vec<&GatewayBudget> = call
            .governance
            .budgets
            .iter()
            .filter(|budget| budget_applies(budget, call.principal))
            .collect();
        let bound = |model: &ModelRef| {
            PricedModel::select(call.governance, model, now)?.cost(call.usage_bound?)
        };
        if !budgets.is_empty() || call.governance.unknown_cost == UnknownCostPolicy::Reject {
            plan.retain(|model| bound(model).is_some());
            if plan.candidates.is_empty() {
                return Err(cost_unbounded());
            }
        }
        let mut admission = Admission::default();
        if !budgets.is_empty() {
            let reserve = plan
                .candidates
                .iter()
                .flat_map(|candidate| candidate.deployments.iter().map(|_| &candidate.model))
                .filter_map(bound)
                .sum();
            for budget in budgets {
                admission
                    .reservations
                    .extend(self.reserve(call, budget, &reserve).await?);
            }
        }

        for (limit, key) in limits {
            if !plan
                .candidates
                .iter()
                .any(|candidate| covers(&limit.target, &candidate.model))
            {
                continue;
            }
            let hold = exposure(plan, &limit.target).unwrap_or(0);
            charge_gateway_limit_window(self.conn, &key, window, 1, hold)
                .await
                .map_err(unavailable)?;
            if limit.concurrent_calls.is_some() {
                insert_gateway_call_lease(self.conn, &key, call.call_id.as_uuid(), call.expires_at)
                    .await
                    .map_err(unavailable)?;
            }
            admission.holds.push(Hold {
                key,
                target: limit.target.clone(),
                tokens: hold,
            });
        }
        Ok(admission)
    }

    /// Records every attempt and the logical call, settles the call's
    /// reservations, settles its token holds, and releases its leases.
    ///
    /// Reservations settle at the call's cost, zero when no attempt was
    /// billed, or in full when billed cost is unknown. Each token hold is
    /// replaced in its admission window by the tokens of the billed attempts
    /// its limit target covers, released when none was billed, and kept when
    /// a covered billed attempt's usage is unknown.
    ///
    /// Billable attempts are priced with the pricing version selected at
    /// admission; a non-billable attempt carries unknown usage and cost. The
    /// call's usage is the per-dimension sum and its cost the sum of attempt
    /// costs, each known only when every billable attempt's value is known.
    /// Every entry is fenced, so replaying accounting writes nothing twice.
    ///
    /// Returns the attempt and call entries in append order, which capture
    /// projects instead of pricing the call a second time.
    ///
    /// # Errors
    /// Returns `ServiceUnavailable` when storage fails and `Internal` when an
    /// entry violates the ledger contract.
    pub(crate) async fn account(
        &mut self,
        call: &LedgerCall<'_>,
        admission: &Admission,
        execution: &CallExecution,
    ) -> Result<Vec<GatewayAccountingEntryV1>, WyrdError> {
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
            self.append(&entry).await?;
            entries.push(entry);
        }

        let usage = usage.filter(|_| billed);
        let cost = cost.filter(|_| billed && cost_known);
        let entry = GatewayAccountingEntryV1::CallAccounted {
            entry_id: GatewayAccountingEntryId::new_v7(),
            call_id: call.call_id,
            data_tenant_id: call.tenant,
            caller_principal_id: call.principal.id,
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
        self.append(&entry).await?;
        entries.push(entry);

        for reservation in &admission.reservations {
            let actual = match &cost {
                Some((total, _)) => total.clone(),
                None if billed => reservation.reserved.clone(),
                None => GatewayCost::zero(),
            };
            self.settle(call.call_id, call.tenant, reservation, &actual, now)
                .await?;
        }
        for hold in &admission.holds {
            let delta = covered_tokens(&hold.target, &execution.attempts)
                .map_or(0, |tokens| tokens - hold.tokens);
            charge_gateway_limit_window(
                self.conn,
                &hold.key,
                minute_start(call.admitted_at),
                0,
                delta,
            )
            .await
            .map_err(unavailable)?;
        }
        release_gateway_call_leases(self.conn, call.call_id.as_uuid())
            .await
            .map_err(unavailable)?;
        Ok(entries)
    }

    /// Settles up to [`RECONCILE_BATCH`] reservations that expired unsettled.
    ///
    /// Their call never reached accounting, so its cost is unknown: the whole
    /// reservation is charged rather than released as zero.
    ///
    /// # Errors
    /// Returns `ServiceUnavailable` when storage fails and `Internal` for an
    /// undecodable reservation entry.
    pub(crate) async fn reconcile(
        &mut self,
        tenant: DataTenantId,
        now: DateTime<Utc>,
    ) -> Result<(), WyrdError> {
        let expired = expired_gateway_reservations(self.conn, now, RECONCILE_BATCH)
            .await
            .map_err(unavailable)?;
        for document in expired {
            let GatewayAccountingEntryV1::BudgetReservationCreated {
                reservation_id,
                call_id,
                reserved_cost,
                currency,
                ..
            } = decode(document)?
            else {
                return Err(internal(
                    "expired reservation query returned another entry kind",
                ));
            };
            let reservation = Reservation {
                id: reservation_id,
                reserved: GatewayCost::from_decimal(&reserved_cost),
                currency,
            };
            let actual = reservation.reserved.clone();
            self.settle(call_id, tenant, &reservation, &actual, now)
                .await?;
        }
        Ok(())
    }

    /// Checks `budget` can absorb `reserve` this period and reserves it.
    ///
    /// Returns no reservation for a zero bound, which consumes nothing.
    ///
    /// # Errors
    /// Returns `GatewayBudgetExceeded` when period spend plus `reserve`
    /// exceeds the budget, and storage or contract errors.
    async fn reserve(
        &mut self,
        call: &LedgerCall<'_>,
        budget: &GatewayBudget,
        reserve: &GatewayCost,
    ) -> Result<Option<Reservation>, WyrdError> {
        let (period_start, period_end) = period_bounds(budget.period, call.admitted_at)
            .ok_or_else(|| internal("budget period is out of range"))?;
        let (start_text, end_text) = (encode(&period_start)?, encode(&period_end)?);
        let spend = gateway_budget_spend(
            self.conn,
            &encode(&budget.subject)?,
            start_text.as_str().unwrap_or_default(),
            end_text.as_str().unwrap_or_default(),
        )
        .await
        .map_err(unavailable)?;
        let spend = GatewayCost::parse(&spend).map_err(internal)?;
        if spend + reserve.clone() > GatewayCost::from_decimal(&budget.amount) {
            return Err(budget_exceeded());
        }
        if reserve.is_zero() {
            return Ok(None);
        }
        let reservation = Reservation {
            id: GatewayBudgetReservationId::new_v7(),
            reserved: reserve.clone(),
            currency: budget.currency.clone(),
        };
        self.append(&GatewayAccountingEntryV1::BudgetReservationCreated {
            entry_id: GatewayAccountingEntryId::new_v7(),
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
        })
        .await?;
        Ok(Some(reservation))
    }

    /// Appends the settlement of `reservation` at `actual` cost.
    ///
    /// # Errors
    /// Returns storage or contract errors.
    async fn settle(
        &mut self,
        call_id: GatewayCallId,
        tenant: DataTenantId,
        reservation: &Reservation,
        actual: &GatewayCost,
        now: DateTime<Utc>,
    ) -> Result<(), WyrdError> {
        self.append(&GatewayAccountingEntryV1::BudgetReservationSettled {
            entry_id: GatewayAccountingEntryId::new_v7(),
            reservation_id: reservation.id,
            call_id,
            data_tenant_id: tenant,
            actual_cost: actual.to_decimal(),
            released_cost: reservation.reserved.saturating_sub(actual).to_decimal(),
            currency: reservation.currency.clone(),
            settled_at: now,
        })
        .await
        .map(|_| ())
    }

    /// Validates and appends one entry; a fenced replay is a no-op.
    ///
    /// # Errors
    /// Returns `Internal` when the entry violates its contract and
    /// `ServiceUnavailable` when storage fails.
    async fn append(&mut self, entry: &GatewayAccountingEntryV1) -> Result<bool, WyrdError> {
        entry.validate().map_err(internal)?;
        let (entry_id, call_id, kind, priced) = match entry {
            GatewayAccountingEntryV1::BudgetReservationCreated {
                entry_id, call_id, ..
            } => (entry_id, call_id, "budget_reservation_created", None),
            GatewayAccountingEntryV1::BudgetReservationSettled {
                entry_id, call_id, ..
            } => (entry_id, call_id, "budget_reservation_settled", None),
            GatewayAccountingEntryV1::AttemptAccounted {
                entry_id,
                call_id,
                model,
                pricing_version,
                ..
            } => (
                entry_id,
                call_id,
                "attempt_accounted",
                pricing_version.as_deref().map(|version| (model, version)),
            ),
            GatewayAccountingEntryV1::CallAccounted {
                entry_id, call_id, ..
            } => (entry_id, call_id, "call_accounted", None),
        };
        let document = encode(entry)?;
        append_gateway_accounting_entry(
            self.conn,
            GatewayAccountingEntryWrite {
                entry_id: entry_id.as_uuid(),
                call_id: call_id.as_uuid(),
                kind,
                provider: priced.map(|(model, _)| model.provider.as_str()),
                model: priced.map(|(model, _)| model.model.as_str()),
                pricing_version: priced.map(|(_, version)| version),
                entry: &document,
            },
        )
        .await
        .map_err(unavailable)
    }
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
        GatewayPolicyTarget::Provider { provider } => *provider == model.provider,
        GatewayPolicyTarget::Model { model: covered } => covered == model,
    }
}

/// True when a configured cap is already consumed by `used`.
fn reached(cap: Option<NonZeroU64>, used: i64) -> bool {
    cap.is_some_and(|cap| u64::try_from(used).unwrap_or(0) >= cap.get())
}

/// Number of `plan` attempts, one per `(candidate, deployment)`, whose model
/// `target` covers.
fn covered_attempts(plan: &CallPlan, target: &GatewayPolicyTarget) -> i64 {
    let attempts: usize = plan
        .candidates
        .iter()
        .filter(|candidate| covers(target, &candidate.model))
        .map(|candidate| candidate.deployments.len())
        .sum();
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
        assert!(reached(NonZeroU64::new(2), 2) && !reached(NonZeroU64::new(2), 1));
    }
}
