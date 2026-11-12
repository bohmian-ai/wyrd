//! Background persistence and refresh of per-server Gateway quota usage.
//!
//! A server admits Gateway calls against memory and, off the request path,
//! writes what it charged and reads every other server's contribution on one
//! tenant transaction: [`sync_gateway_usage`]. Window and concurrency rows
//! are absolute per server, and ledger entries are fenced by their unique
//! indexes, so a retried or uncertain write never adds a charge twice and no
//! server overwrites another's usage. FORCE RLS is the tenant boundary, so
//! statements carry no tenant-only predicate.
//!
//! During a rolling deployment the previous release still charges its
//! shared counters and leases (`gateway_limit_windows`,
//! `gateway_call_leases`); the shared view counts them beside other
//! servers' rows, and that release's admission counts this one's rows
//! (migration `20261111000001`), so each side sees the other's usage once.
// raw-query grep allowlist: the usage tables post-date the sqlx offline cache; run `mise run sqlx:prepare` to promote to macros.

use chrono::{DateTime, Utc};
use serde_json::Value;
use sqlx::Error;
use uuid::Uuid;

use crate::TenantConn;

/// One server's absolute requests and tokens in one limit window.
#[derive(Debug, Clone, PartialEq, Eq, sqlx::FromRow)]
pub struct GatewayUsageWindow {
    /// Canonical JSON of the limit's `(subject, target)` key.
    pub limit_key: String,
    /// Start of the fixed one-minute window.
    pub window_start: DateTime<Utc>,
    /// Requests charged.
    pub requests: i64,
    /// Tokens charged.
    pub tokens: i64,
}

/// Calls running under one concurrency limit.
#[derive(Debug, Clone, PartialEq, Eq, sqlx::FromRow)]
pub struct GatewayUsageConcurrency {
    /// Canonical JSON of the limit's `(subject, target)` key.
    pub limit_key: String,
    /// Calls running.
    pub active: i64,
}

/// One budget reservation ledger entry to append.
#[derive(Debug, Clone)]
pub struct GatewayUsageEntry {
    /// Entry identity.
    pub entry_id: Uuid,
    /// Logical call the entry belongs to.
    pub call_id: Uuid,
    /// `budget_reservation_created` or `budget_reservation_settled`.
    pub kind: &'static str,
    /// The complete ledger document.
    pub entry: Value,
}

/// Committed spend of one budget subject in one period.
#[derive(Debug, Clone, PartialEq, Eq, sqlx::FromRow)]
pub struct GatewayBudgetSpend {
    /// Budget subject JSON, as reservations store it.
    pub subject: Value,
    /// RFC 3339 period start, as reservations store it.
    pub period_start: String,
    /// RFC 3339 period end, as reservations store it.
    pub period_end: String,
    /// Settled actual cost plus unsettled reserved cost, as decimal text.
    pub spend: String,
}

/// One server's usage to persist.
#[derive(Debug)]
pub struct GatewayUsageWrite<'a> {
    /// The contributing server process.
    pub server_id: Uuid,
    /// The server's absolute window totals; each replaces its row.
    pub windows: &'a [GatewayUsageWindow],
    /// Windows starting before this are pruned, for every server.
    pub window_floor: DateTime<Utc>,
    /// The server's running calls per concurrency limit; its rows for other
    /// limits are deleted.
    pub concurrency: &'a [GatewayUsageConcurrency],
    /// How long a concurrency row counts unless renewed, in seconds.
    pub concurrency_lease_seconds: f64,
    /// Ledger entries to append; a replay appends nothing.
    pub entries: &'a [GatewayUsageEntry],
}

/// Every other server's usage and the committed budget spend.
#[derive(Debug, Default)]
pub struct GatewayUsageView {
    /// Other servers' window totals at or after the write's floor, summed.
    pub windows: Vec<GatewayUsageWindow>,
    /// Other servers' unexpired running calls, summed per limit.
    pub concurrency: Vec<GatewayUsageConcurrency>,
    /// Spend of every budget period that has not ended, every server's
    /// committed reservations included.
    pub spend: Vec<GatewayBudgetSpend>,
}

/// Replaces the server's window rows with its absolute totals.
const UPSERT_WINDOWS_SQL: &str = r"
    INSERT INTO wyrd.gateway_usage_windows AS w (
        data_tenant_id, limit_key, window_start, server_id, requests, tokens
    )
    SELECT $1, u.limit_key, u.window_start, $2, u.requests, u.tokens
      FROM UNNEST($3::text[], $4::timestamptz[], $5::int8[], $6::int8[])
           AS u(limit_key, window_start, requests, tokens)
    ON CONFLICT (data_tenant_id, limit_key, window_start, server_id) DO UPDATE SET
        requests = EXCLUDED.requests,
        tokens = EXCLUDED.tokens
";

/// Prunes windows no admission reads.
const PRUNE_WINDOWS_SQL: &str = r"
    DELETE FROM wyrd.gateway_usage_windows WHERE window_start < $1
";

/// Deletes the server's concurrency rows for limits it no longer runs, and
/// every lapsed row.
const PRUNE_CONCURRENCY_SQL: &str = r"
    DELETE FROM wyrd.gateway_usage_concurrency
     WHERE (server_id = $1 AND NOT (limit_key = ANY($2::text[])))
        OR expires_at <= statement_timestamp()
";

/// Replaces and renews the server's concurrency rows.
const UPSERT_CONCURRENCY_SQL: &str = r"
    INSERT INTO wyrd.gateway_usage_concurrency AS c (
        data_tenant_id, limit_key, server_id, active, expires_at
    )
    SELECT $1, u.limit_key, $2, u.active,
           statement_timestamp() + make_interval(secs => $5)
      FROM UNNEST($3::text[], $4::int8[]) AS u(limit_key, active)
    ON CONFLICT (data_tenant_id, limit_key, server_id) DO UPDATE SET
        active = EXCLUDED.active,
        expires_at = EXCLUDED.expires_at
";

/// Appends reservation entries; the reservation fences make a replay a no-op.
const INSERT_ENTRIES_SQL: &str = r"
    INSERT INTO wyrd.gateway_accounting_entries (data_tenant_id, entry_id, call_id, kind, entry)
    SELECT $1, u.entry_id, u.call_id, u.kind, u.entry
      FROM UNNEST($2::uuid[], $3::uuid[], $4::text[], $5::jsonb[])
           AS u(entry_id, call_id, kind, entry)
    ON CONFLICT DO NOTHING
";

/// Other servers' window totals: every other current server's rows plus the
/// previous release's shared counters, which carry no server identity and
/// are never this server's.
const WINDOWS_SQL: &str = r"
    SELECT limit_key, window_start,
           SUM(requests)::int8 AS requests, SUM(tokens)::int8 AS tokens
      FROM (
          SELECT limit_key, window_start, requests, tokens
            FROM wyrd.gateway_usage_windows
           WHERE server_id <> $1 AND window_start >= $2
          UNION ALL
          SELECT limit_key, window_start, requests, tokens
            FROM wyrd.gateway_limit_windows
           WHERE window_start >= $2
      ) AS w
     GROUP BY limit_key, window_start
";

/// Other servers' unexpired running calls: every other current server's
/// counts plus one per unexpired lease of the previous release.
const CONCURRENCY_SQL: &str = r"
    SELECT limit_key, SUM(active)::int8 AS active
      FROM (
          SELECT limit_key, active
            FROM wyrd.gateway_usage_concurrency
           WHERE server_id <> $1 AND expires_at > statement_timestamp()
          UNION ALL
          SELECT limit_key, 1
            FROM wyrd.gateway_call_leases
           WHERE expires_at > statement_timestamp()
      ) AS c
     GROUP BY limit_key
";

/// Committed spend per budget subject and unfinished period: settled actual
/// cost, or reserved cost while unsettled.
// ponytail: scans the tenant's reservations of unfinished periods each
// refresh; keep a per-period spend row if reservation volume makes it slow.
const SPEND_SQL: &str = r"
    SELECT c.entry -> 'budget_reservation_created' -> 'subject' AS subject,
           c.entry -> 'budget_reservation_created' ->> 'period_start' AS period_start,
           c.entry -> 'budget_reservation_created' ->> 'period_end' AS period_end,
           SUM(COALESCE(
               (s.entry -> 'budget_reservation_settled' ->> 'actual_cost')::numeric,
               (c.entry -> 'budget_reservation_created' ->> 'reserved_cost')::numeric
           ))::text AS spend
      FROM wyrd.gateway_accounting_entries c
      LEFT JOIN wyrd.gateway_accounting_entries s
        ON s.data_tenant_id = c.data_tenant_id
       AND s.kind = 'budget_reservation_settled'
       AND s.entry -> 'budget_reservation_settled' ->> 'reservation_id'
         = c.entry -> 'budget_reservation_created' ->> 'reservation_id'
     WHERE c.kind = 'budget_reservation_created'
       AND (c.entry -> 'budget_reservation_created' ->> 'period_end')::timestamptz
           > statement_timestamp()
     GROUP BY 1, 2, 3
";

/// Persists one server's usage and reads every other server's on the
/// caller's transaction; the caller commits, and only a committed result is
/// a shared view that includes this write.
///
/// # Errors
/// Returns the SQLx error of the first statement Postgres rejects.
pub async fn sync_gateway_usage(
    conn: &mut TenantConn<'_>,
    write: &GatewayUsageWrite<'_>,
) -> Result<GatewayUsageView, Error> {
    let tenant = conn.data_tenant_id().as_uuid();
    let tx = conn.transaction();
    if !write.windows.is_empty() {
        sqlx::query(UPSERT_WINDOWS_SQL)
            .bind(tenant)
            .bind(write.server_id)
            .bind(
                write
                    .windows
                    .iter()
                    .map(|w| w.limit_key.as_str())
                    .collect::<Vec<_>>(),
            )
            .bind(
                write
                    .windows
                    .iter()
                    .map(|w| w.window_start)
                    .collect::<Vec<_>>(),
            )
            .bind(write.windows.iter().map(|w| w.requests).collect::<Vec<_>>())
            .bind(write.windows.iter().map(|w| w.tokens).collect::<Vec<_>>())
            .execute(&mut **tx)
            .await?;
    }
    sqlx::query(PRUNE_WINDOWS_SQL)
        .bind(write.window_floor)
        .execute(&mut **tx)
        .await?;
    let keys: Vec<&str> = write
        .concurrency
        .iter()
        .map(|c| c.limit_key.as_str())
        .collect();
    sqlx::query(PRUNE_CONCURRENCY_SQL)
        .bind(write.server_id)
        .bind(&keys)
        .execute(&mut **tx)
        .await?;
    if !write.concurrency.is_empty() {
        sqlx::query(UPSERT_CONCURRENCY_SQL)
            .bind(tenant)
            .bind(write.server_id)
            .bind(&keys)
            .bind(
                write
                    .concurrency
                    .iter()
                    .map(|c| c.active)
                    .collect::<Vec<_>>(),
            )
            .bind(write.concurrency_lease_seconds)
            .execute(&mut **tx)
            .await?;
    }
    if !write.entries.is_empty() {
        sqlx::query(INSERT_ENTRIES_SQL)
            .bind(tenant)
            .bind(write.entries.iter().map(|e| e.entry_id).collect::<Vec<_>>())
            .bind(write.entries.iter().map(|e| e.call_id).collect::<Vec<_>>())
            .bind(write.entries.iter().map(|e| e.kind).collect::<Vec<_>>())
            .bind(
                write
                    .entries
                    .iter()
                    .map(|e| e.entry.clone())
                    .collect::<Vec<_>>(),
            )
            .execute(&mut **tx)
            .await?;
    }
    Ok(GatewayUsageView {
        windows: sqlx::query_as(WINDOWS_SQL)
            .bind(write.server_id)
            .bind(write.window_floor)
            .fetch_all(&mut **tx)
            .await?,
        concurrency: sqlx::query_as(CONCURRENCY_SQL)
            .bind(write.server_id)
            .fetch_all(&mut **tx)
            .await?,
        spend: sqlx::query_as(SPEND_SQL).fetch_all(&mut **tx).await?,
    })
}
