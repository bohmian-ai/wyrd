//! Durable proof that Gateway quota usage is counted once across a rolling
//! deployment.
//!
//! The previous release charges shared counters and leases through
//! `wyrd.gateway_admit` and `wyrd.gateway_settle`; current servers persist
//! absolute per-server rows through `sync_gateway_usage`. Each reader must
//! count the other's committed usage once, and only its own tenant's.

use chrono::{DateTime, TimeDelta, Utc};
use serde_json::{Value, json};
use uuid::Uuid;
use wyrd_dev_fixtures::pg::PgFixture;
use wyrd_spec::DataTenantId;
use wyrd_sql::queries::gateway_admission::{
    GatewayUsageConcurrency, GatewayUsageView, GatewayUsageWindow, GatewayUsageWrite,
    sync_gateway_usage,
};

/// The one limit key these proofs charge.
const KEY: &str = "k";

/// Start of the minute containing now.
fn this_minute() -> DateTime<Utc> {
    let now = Utc::now().timestamp();
    DateTime::from_timestamp(now - now.rem_euclid(60), 0).expect("minute start is valid")
}

/// One previous-release limit document for [`KEY`].
fn limit(requests: Option<u64>, concurrent: Option<u64>, hold: i64, lease: bool) -> Value {
    json!([{
        "key": KEY,
        "requests": requests,
        "tokens": null,
        "concurrent": concurrent,
        "hold": hold,
        "lease": lease,
    }])
}

/// Admits `call` against `limits` in `window` the way the previous release
/// does, returning the exhausted keys and whether a budget was exceeded.
///
/// # Panics
/// Panics when the tenant connection, the admission, or its commit fails.
async fn old_admit(
    fixture: &PgFixture,
    tenant: DataTenantId,
    call: Uuid,
    window: DateTime<Utc>,
    limits: Value,
) -> (Vec<String>, bool) {
    let mut conn = fixture.tenant_conn_for(tenant).await.expect("tenant conn");
    let outcome = sqlx::query_as(
        "SELECT * FROM wyrd.gateway_admit($1, $2, $3, now(), now() + interval '30 seconds', \
         $4, '[]'::jsonb)",
    )
    .bind(tenant.as_uuid())
    .bind(call)
    .bind(window)
    .bind(limits)
    .fetch_one(&mut **conn.transaction())
    .await
    .expect("previous-release admission runs");
    conn.commit().await.expect("admission commits");
    outcome
}

/// Settles `call` the way the previous release does: corrects its [`KEY`]
/// token hold by `delta` and releases its leases.
///
/// # Panics
/// Panics when the tenant connection, the settlement, or its commit fails.
async fn old_settle(
    fixture: &PgFixture,
    tenant: DataTenantId,
    call: Uuid,
    window: DateTime<Utc>,
    delta: i64,
) {
    let mut conn = fixture.tenant_conn_for(tenant).await.expect("tenant conn");
    sqlx::query("SELECT wyrd.gateway_settle($1, $2, $3, $4, now(), $5, '[]'::jsonb, true)")
        .bind(tenant.as_uuid())
        .bind(call)
        .bind(window)
        .bind(window - TimeDelta::minutes(1))
        .bind(json!([{"key": KEY, "delta": delta}]))
        .execute(&mut **conn.transaction())
        .await
        .expect("previous-release settlement runs");
    conn.commit().await.expect("settlement commits");
}

/// Persists `server`'s absolute `requests` and `tokens` in `window` and its
/// `running` calls under [`KEY`] lasting `lease_seconds`, returning the
/// committed shared view.
///
/// # Panics
/// Panics when the tenant connection, the synchronization, or its commit
/// fails.
async fn sync(
    fixture: &PgFixture,
    tenant: DataTenantId,
    server: Uuid,
    window: DateTime<Utc>,
    (requests, tokens): (i64, i64),
    (running, lease_seconds): (i64, f64),
) -> GatewayUsageView {
    let windows = [GatewayUsageWindow {
        limit_key: KEY.to_owned(),
        window_start: window,
        requests,
        tokens,
    }];
    let concurrency = [GatewayUsageConcurrency {
        limit_key: KEY.to_owned(),
        active: running,
    }];
    let mut conn = fixture.tenant_conn_for(tenant).await.expect("tenant conn");
    let view = sync_gateway_usage(
        &mut conn,
        &GatewayUsageWrite {
            server_id: server,
            windows: if requests > 0 { &windows } else { &[] },
            window_floor: window - TimeDelta::minutes(1),
            concurrency: if running > 0 { &concurrency } else { &[] },
            concurrency_lease_seconds: lease_seconds,
            entries: &[],
        },
    )
    .await
    .expect("usage synchronizes");
    conn.commit().await.expect("synchronization commits");
    view
}

/// The requests, tokens, and running calls `view` counts for [`KEY`].
fn counted(view: &GatewayUsageView, window: DateTime<Utc>) -> (i64, i64, i64) {
    let (requests, tokens) = view
        .windows
        .iter()
        .filter(|w| w.limit_key == KEY && w.window_start == window)
        .fold((0, 0), |(r, t), w| (r + w.requests, t + w.tokens));
    let running = view
        .concurrency
        .iter()
        .filter(|c| c.limit_key == KEY)
        .map(|c| c.active)
        .sum();
    (requests, tokens, running)
}

/// Previous-release and current usage are each counted once by both
/// readers, settlement and expiry change each count once, and neither
/// reader counts another tenant's usage.
///
/// # Panics
/// Panics when the fixture or a statement fails, or a reader counts usage
/// other than once.
#[tokio::test]
async fn previous_and_current_usage_count_once_in_both_readers() {
    let fixture = PgFixture::start().await.expect("fixture starts");
    let tenant = fixture.data_tenant_id();
    let other = fixture
        .seed_additional_tenant("gateway-usage-other")
        .await
        .expect("other tenant seeds");
    let window = this_minute();
    let (current, observer) = (Uuid::new_v4(), Uuid::new_v4());
    let admitted = (Vec::new(), false);
    let exhausted = (vec![KEY.to_owned()], false);

    let old_call = Uuid::new_v4();
    let charge = limit(None, None, 5, true);
    assert_eq!(
        old_admit(&fixture, tenant, old_call, window, charge).await,
        admitted
    );
    let view = sync(&fixture, tenant, current, window, (2, 7), (1, 30.0)).await;
    assert_eq!(
        counted(&view, window),
        (1, 5, 1),
        "current reads the previous release"
    );
    let view = sync(&fixture, tenant, observer, window, (0, 0), (0, 30.0)).await;
    assert_eq!(
        counted(&view, window),
        (3, 12, 2),
        "each representation counts once"
    );

    let requests = || limit(Some(4), None, 0, false);
    let first = old_admit(&fixture, tenant, Uuid::new_v4(), window, requests()).await;
    assert_eq!(
        first, admitted,
        "1 previous plus 2 current requests leave room under 4"
    );
    let second = old_admit(&fixture, tenant, Uuid::new_v4(), window, requests()).await;
    assert_eq!(
        second, exhausted,
        "2 previous plus 2 current requests reach 4"
    );
    let calls = limit(None, Some(2), 0, true);
    let running = old_admit(&fixture, tenant, Uuid::new_v4(), window, calls).await;
    assert_eq!(running, exhausted, "1 lease plus 1 current call reach 2");

    old_settle(&fixture, tenant, old_call, window, -5).await;
    sync(&fixture, tenant, current, window, (2, 7), (0, 30.0)).await;
    let view = sync(&fixture, tenant, observer, window, (0, 0), (0, 30.0)).await;
    assert_eq!(
        counted(&view, window),
        (4, 7, 0),
        "settlement releases the hold and lease, and the current call ended"
    );

    let one_call = || limit(None, Some(1), 0, true);
    let leased = old_admit(&fixture, tenant, Uuid::new_v4(), window, one_call()).await;
    assert_eq!(leased, admitted, "no call runs");
    sync(&fixture, tenant, current, window, (2, 7), (1, 0.0)).await;
    let two_calls = limit(None, Some(2), 0, true);
    let beside = old_admit(&fixture, tenant, Uuid::new_v4(), window, two_calls).await;
    assert_eq!(beside, admitted, "an expired current call is not counted");
    let view = sync(&fixture, tenant, observer, window, (0, 0), (0, 30.0)).await;
    assert_eq!(
        counted(&view, window).2,
        2,
        "both leases count, the expired call does not"
    );

    let view = sync(&fixture, other, observer, window, (0, 0), (0, 30.0)).await;
    assert_eq!(
        counted(&view, window),
        (0, 0, 0),
        "another tenant reads nothing"
    );
    let isolated = limit(Some(1), Some(1), 0, true);
    let other_admit = old_admit(&fixture, other, Uuid::new_v4(), window, isolated).await;
    assert_eq!(
        other_admit, admitted,
        "another tenant's admission counts nothing"
    );
}
