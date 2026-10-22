//! Gateway admission and settlement, each in one round trip.
//!
//! A gateway call that a limit or budget covers is checked and charged by
//! `wyrd.gateway_admit` before it runs and settled by `wyrd.gateway_settle`
//! once it ends. Both functions bind their tenant for RLS
//! themselves, so [`GatewayAdmissions`] runs each as one autocommit statement
//! on a plain connection with no transaction around it. A call nothing covers
//! never reaches this module.
// raw-query grep allowlist: the functions post-date the sqlx offline cache; run `mise run sqlx:prepare` to promote to macros.

use chrono::{DateTime, Utc};
use serde::Serialize;
use serde_json::Value;
use sqlx::types::Json;
use uuid::Uuid;
use wyrd_spec::DataTenantId;

/// One limit an admission checks and charges.
#[derive(Debug, Serialize)]
pub struct GatewayLimitCharge {
    /// Canonical JSON of the limit's `(subject, target)` key.
    pub key: String,
    /// Requests per window, or `None` when unset.
    pub requests: Option<u64>,
    /// Tokens per window, or `None` when unset.
    pub tokens: Option<u64>,
    /// Concurrent calls, or `None` when unset.
    pub concurrent: Option<u64>,
    /// Token exposure the call holds in the window.
    pub hold: i64,
    /// Whether the call takes a concurrency lease on the limit.
    pub lease: bool,
}

/// One budget an admission checks and reserves against.
#[derive(Debug, Serialize)]
pub struct GatewayBudgetCharge {
    /// Budget subject JSON, matched exactly against stored reservations.
    pub subject: Value,
    /// RFC 3339 period start stored on the budget's reservations.
    pub period_start: Value,
    /// RFC 3339 period end stored on the budget's reservations.
    pub period_end: Value,
    /// Budget amount as decimal text.
    pub amount: String,
    /// Reserved cost as decimal text.
    pub reserve: String,
    /// Reservation-creation entry identity, when the reserve is non-zero.
    pub entry_id: Option<Uuid>,
    /// Reservation-creation ledger document, when the reserve is non-zero.
    pub entry: Option<Value>,
}

/// The decision of one admission statement.
#[derive(Debug, PartialEq, Eq)]
pub enum GatewayAdmissionOutcome {
    /// Every limit and budget had room; all were charged.
    Admitted,
    /// These limit keys are exhausted; nothing was charged.
    Exhausted(Vec<String>),
    /// No limit is exhausted but a budget cannot cover its reserve; nothing
    /// was charged.
    BudgetExceeded,
}

/// One admission's call identity, times, and charges.
#[derive(Debug)]
pub struct GatewayAdmissionWrite<'a> {
    /// Logical call, which leases and reservations name.
    pub call_id: Uuid,
    /// Start of the admission's limit window.
    pub window_start: DateTime<Utc>,
    /// Admission time; leases unexpired at it count against concurrency.
    pub now: DateTime<Utc>,
    /// Time the call's leases lapse.
    pub expires_at: DateTime<Utc>,
    /// Limits covering at least one remaining candidate.
    pub limits: &'a [GatewayLimitCharge],
    /// Budgets that apply to the call.
    pub budgets: &'a [GatewayBudgetCharge],
}

/// One token hold's correction to the call's actual tokens.
#[derive(Debug, Serialize)]
pub struct GatewayHoldDelta {
    /// Limit window key.
    pub key: String,
    /// Signed token delta; the window never goes below zero.
    pub delta: i64,
}

/// One reservation settlement entry.
#[derive(Debug, Serialize)]
pub struct GatewaySettlementEntry {
    /// Entry identity.
    pub entry_id: Uuid,
    /// Reservation-settlement ledger document.
    pub entry: Value,
}

/// One accounted call's settlement.
#[derive(Debug)]
pub struct GatewaySettlementWrite<'a> {
    /// Logical call whose leases are released.
    pub call_id: Uuid,
    /// The call's admission window, where its holds were charged.
    pub window_start: DateTime<Utc>,
    /// Windows starting before this are pruned.
    pub window_floor: DateTime<Utc>,
    /// Leases expired at this time are pruned.
    pub now: DateTime<Utc>,
    /// Hold corrections.
    pub holds: &'a [GatewayHoldDelta],
    /// Reservation settlements.
    pub settlements: &'a [GatewaySettlementEntry],
    /// Whether the call took a concurrency lease.
    pub release: bool,
}

/// Runs gateway admission statements for one tenant on one connection.
///
/// `conn` must not be inside a transaction bound to another tenant; the
/// statements refuse that rather than rebinding.
pub struct GatewayAdmissions<'conn> {
    /// Connection the statements run on.
    conn: &'conn mut sqlx::PgConnection,
    /// Tenant the statements bind.
    data_tenant_id: DataTenantId,
}

impl<'conn> GatewayAdmissions<'conn> {
    /// Binds the owner to one connection and the tenant it charges.
    pub fn new(conn: &'conn mut sqlx::PgConnection, data_tenant_id: DataTenantId) -> Self {
        Self {
            conn,
            data_tenant_id,
        }
    }

    /// Checks and, when everything has room, charges one admission in one
    /// statement under the tenant's admission advisory lock.
    ///
    /// # Errors
    /// Returns the database error when the statement fails, including the
    /// refusal of a connection bound to another tenant.
    pub async fn admit(
        &mut self,
        write: &GatewayAdmissionWrite<'_>,
    ) -> Result<GatewayAdmissionOutcome, sqlx::Error> {
        let (exhausted, budget_exceeded): (Vec<String>, bool) =
            sqlx::query_as("SELECT * FROM wyrd.gateway_admit($1, $2, $3, $4, $5, $6, $7)")
                .bind(self.data_tenant_id.as_uuid())
                .bind(write.call_id)
                .bind(write.window_start)
                .bind(write.now)
                .bind(write.expires_at)
                .bind(Json(write.limits))
                .bind(Json(write.budgets))
                .fetch_one(&mut *self.conn)
                .await?;
        Ok(if !exhausted.is_empty() {
            GatewayAdmissionOutcome::Exhausted(exhausted)
        } else if budget_exceeded {
            GatewayAdmissionOutcome::BudgetExceeded
        } else {
            GatewayAdmissionOutcome::Admitted
        })
    }

    /// Settles one accounted call in one statement and prunes the tenant's
    /// windows and leases no admission reads any more.
    ///
    /// # Errors
    /// Returns the database error when the statement fails.
    pub async fn settle(&mut self, write: &GatewaySettlementWrite<'_>) -> Result<(), sqlx::Error> {
        sqlx::query("SELECT wyrd.gateway_settle($1, $2, $3, $4, $5, $6, $7, $8)")
            .bind(self.data_tenant_id.as_uuid())
            .bind(write.call_id)
            .bind(write.window_start)
            .bind(write.window_floor)
            .bind(write.now)
            .bind(Json(write.holds))
            .bind(Json(write.settlements))
            .bind(write.release)
            .execute(&mut *self.conn)
            .await
            .map(|_| ())
    }
}
