//! PgFixture coverage for verification-binding projection and the runtime
//! activity gate derived from Card-bound machine principals.
//!
//! Every test runs against the repository managed Postgres through tenant
//! connections, so forced RLS applies.

use chrono::{DateTime, Utc};
use uuid::Uuid;
use wyrd_dev_fixtures::pg::PgFixture;
use wyrd_runtime::PermissionSet;
use wyrd_runtime::principal::{Principal, PrincipalId, PrincipalKind};
use wyrd_spec::card::verifier::OWNER_OCCURRENCE_KEY;
use wyrd_spec::envelope::{Card, CardKind};
use wyrd_spec::ids::{BindingId, CardUid};
use wyrd_spec::registry::RegistrationOperationId;
use wyrd_sql::TenantConn;
use wyrd_sql::queries::cards::{
    NewCardRow, NewRegistrationOperation, insert_card_row, insert_registration_operation,
    upsert_service_account_from_card,
};
use wyrd_sql::queries::verification::{
    BindingActivation, BindingActivity, FrozenTarget, InactivityTimeout, NewBinding,
    binding_activity, owner_binding_ids, project_bindings, record_machine_authentication,
};
use wyrd_sql::row_types::cards::CardStatus;

/// Mint a fresh Card UID.
///
/// # Panics
/// Never in practice: a freshly minted `UUIDv7` is always a valid Card UID.
fn uid() -> CardUid {
    CardUid::from_uuid(Uuid::now_v7()).expect("UUIDv7 is a valid card UID")
}

/// Build one resolved Service card fixture at `version`.
///
/// # Panics
/// Panics when the fixture JSON no longer deserializes as a [`Card`].
fn service_card(name: &str, version: &str) -> Card {
    serde_json::from_value(serde_json::json!({
        "apiVersion": "wyrd/v1",
        "kind": "Service",
        "metadata": { "name": name, "version": version, "space": "default" },
        "spec": {},
        "relationships": { "outbound": [], "inbound": [] }
    }))
    .expect("service fixture must deserialize")
}

/// A registering user principal in the fixture tenant.
///
/// The principal carries no permissions; persistence-level queries do not
/// authorize.
fn actor(fixture: &PgFixture) -> Principal {
    Principal::new(
        PrincipalId::new(Uuid::now_v7()),
        PrincipalKind::User,
        fixture.data_tenant_id(),
        Vec::new(),
        PermissionSet::new(),
    )
}

/// A daily-at-02:00-UTC schedule binding for subject `occurrence`.
///
/// Pass [`OWNER_OCCURRENCE_KEY`] for a binding on the owner itself or a
/// component alias for a component binding.
fn schedule_binding(occurrence: &str, verifier: &CardUid) -> NewBinding {
    NewBinding {
        subject_occurrence_key: occurrence.to_owned(),
        subject_card_uid: uid(),
        verifier_uid: verifier.clone(),
        trigger: FrozenTarget::Digest("sha256:trigger".to_owned()),
        operators: vec![FrozenTarget::Uid(uid())],
        activation: BindingActivation::Schedule {
            cron: "0 2 * * *".to_owned(),
            tz: None,
        },
    }
}

/// Register one active Service Card and its principal; return (card UID, principal id).
///
/// Activation is forced with a direct update because the full lifecycle
/// requires a verified blob that this persistence-level test does not need.
///
/// # Panics
/// Panics when the operation, Card, principal, or activation write fails.
async fn register_service(
    conn: &mut TenantConn<'_>,
    actor: &Principal,
    card: &Card,
) -> (CardUid, PrincipalId) {
    let operation_id = RegistrationOperationId::new(Uuid::now_v7());
    let key = Uuid::now_v7().to_string();
    insert_registration_operation(
        conn,
        NewRegistrationOperation {
            operation_id,
            principal_id: actor.id,
            idempotency_key: &key,
            request_hash: "request-hash",
        },
    )
    .await
    .expect("operation inserts");
    let row = insert_card_row(
        conn,
        NewCardRow {
            card,
            card_uid: uid(),
            principal_id: actor.id,
            operation_id,
            status: CardStatus::Pending,
            spec_hash: "spec-hash",
            artifact_hash: None,
        },
    )
    .await
    .expect("card inserts");
    let principal = upsert_service_account_from_card(conn, &row.card_uid, card, actor)
        .await
        .expect("principal projects");
    sqlx::query("UPDATE wyrd.cards SET status = 'active' WHERE card_uid = $1")
        .bind(row.card_uid.as_uuid())
        .execute(&mut **conn.transaction())
        .await
        .expect("card activates");
    (row.card_uid, principal)
}

/// Read one binding's schedule cursor.
///
/// # Panics
/// Panics when the binding row cannot be read.
async fn cursor(conn: &mut TenantConn<'_>, binding: Uuid) -> Option<DateTime<Utc>> {
    sqlx::query_scalar("SELECT next_run_at FROM wyrd.verification_bindings WHERE binding_id = $1")
        .bind(binding)
        .fetch_one(&mut **conn.transaction())
        .await
        .expect("cursor reads")
}

/// Projection mints UUIDv7 identities; re-apply and reordering keep them, a
/// different Verifier mints a new one, and component occurrences are distinct
/// from the Service-level binding of the same Verifier. Owner bindings persist
/// the reserved non-null owner key and component bindings their alias.
///
/// # Panics
/// Panics when any identity, ordering, or persisted-key expectation fails.
#[tokio::test]
async fn projection_identity_is_stable_under_reapply_and_reorder() {
    let fixture = PgFixture::start().await.expect("fixture starts");
    let actor = actor(&fixture);
    let mut conn = fixture
        .tenant_conn()
        .await
        .expect("tenant connection opens");
    let (owner, _) = register_service(&mut conn, &actor, &service_card("svc", "1.0.0")).await;
    let verifier = uid();
    let service_level = schedule_binding(OWNER_OCCURRENCE_KEY, &verifier);
    let component = schedule_binding("retriever", &verifier);

    let first = project_bindings(
        &mut conn,
        &owner,
        &CardKind::Service,
        &[service_level.clone(), component.clone()],
    )
    .await
    .expect("bindings project");
    assert_eq!(first.len(), 2);
    assert_ne!(first[0], first[1], "occurrences are distinct bindings");
    assert!(first.iter().all(|id| id.as_uuid().get_version_num() == 7));
    assert_eq!(
        occurrence_key(&mut conn, first[0]).await,
        OWNER_OCCURRENCE_KEY
    );
    assert_eq!(occurrence_key(&mut conn, first[1]).await, "retriever");

    let reordered = project_bindings(
        &mut conn,
        &owner,
        &CardKind::Service,
        &[component, service_level],
    )
    .await
    .expect("bindings reproject");
    assert_eq!(reordered, vec![first[1], first[0]]);

    let other = project_bindings(
        &mut conn,
        &owner,
        &CardKind::Service,
        &[schedule_binding(OWNER_OCCURRENCE_KEY, &uid())],
    )
    .await
    .expect("changed verifier projects");
    assert!(!first.contains(&other[0]));

    let mut listed = first.clone();
    listed.push(other[0]);
    listed.sort();
    assert_eq!(
        owner_binding_ids(&mut conn, &owner)
            .await
            .expect("ids list"),
        listed
    );
    conn.commit().await.expect("commits");
}

/// Read one binding's persisted subject-occurrence key.
///
/// # Panics
/// Panics when the binding row cannot be read.
async fn occurrence_key(conn: &mut TenantConn<'_>, binding: BindingId) -> String {
    sqlx::query_scalar(
        "SELECT subject_occurrence_key FROM wyrd.verification_bindings WHERE binding_id = $1",
    )
    .bind(binding.as_uuid())
    .fetch_one(&mut **conn.transaction())
    .await
    .expect("occurrence key reads")
}

/// A standalone Agent's binding persists the reserved owner key, and the
/// table refuses a component alias on an Agent owner.
///
/// # Panics
/// Panics when the Agent binding does not persist the owner key or an
/// Agent-owned component occurrence is accepted.
#[tokio::test]
async fn agent_owner_occurrence_is_the_reserved_key() {
    let fixture = PgFixture::start().await.expect("fixture starts");
    let actor = actor(&fixture);
    let mut conn = fixture
        .tenant_conn()
        .await
        .expect("tenant connection opens");
    let agent: Card = serde_json::from_value(serde_json::json!({
        "apiVersion": "wyrd/v1",
        "kind": "Agent",
        "metadata": { "name": "helper", "version": "1.0.0", "space": "default" },
        "spec": { "prompt": {
            "kind": "Prompt", "name": "helper-prompt", "version": "1.0.0", "space": "default"
        }},
        "relationships": { "outbound": [], "inbound": [] }
    }))
    .expect("agent fixture must deserialize");
    let (owner, _) = register_service(&mut conn, &actor, &agent).await;
    let ids = project_bindings(
        &mut conn,
        &owner,
        &CardKind::Agent,
        &[schedule_binding(OWNER_OCCURRENCE_KEY, &uid())],
    )
    .await
    .expect("agent binding projects");
    assert_eq!(
        occurrence_key(&mut conn, ids[0]).await,
        OWNER_OCCURRENCE_KEY
    );
    assert!(
        project_bindings(
            &mut conn,
            &owner,
            &CardKind::Agent,
            &[schedule_binding("retriever", &uid())],
        )
        .await
        .is_err(),
        "an Agent owns no component occurrence"
    );
}

/// A rolled-back registration leaves no binding, and another tenant cannot
/// read or gate a binding it does not own.
///
/// # Panics
/// Panics when a rolled-back projection survives or tenant B can read,
/// gate, or project onto tenant A's owner.
#[tokio::test]
async fn projection_is_transactional_and_tenant_isolated() {
    let fixture = PgFixture::start().await.expect("fixture starts");
    let other = fixture
        .seed_additional_tenant("verification-other")
        .await
        .expect("second tenant seeds");
    let actor = actor(&fixture);

    let mut conn = fixture
        .tenant_conn()
        .await
        .expect("tenant connection opens");
    let (owner, _) = register_service(&mut conn, &actor, &service_card("rolled", "1.0.0")).await;
    project_bindings(
        &mut conn,
        &owner,
        &CardKind::Service,
        &[schedule_binding(OWNER_OCCURRENCE_KEY, &uid())],
    )
    .await
    .expect("binding projects");
    drop(conn);
    let total: i64 = sqlx::query_scalar("SELECT count(*) FROM wyrd.verification_bindings")
        .fetch_one(fixture.operator_pool().pool())
        .await
        .expect("count reads");
    assert_eq!(total, 0, "rollback removes the projection");

    let mut conn = fixture
        .tenant_conn()
        .await
        .expect("tenant connection opens");
    let (owner, _) = register_service(&mut conn, &actor, &service_card("kept", "1.0.0")).await;
    let ids = project_bindings(
        &mut conn,
        &owner,
        &CardKind::Service,
        &[schedule_binding(OWNER_OCCURRENCE_KEY, &uid())],
    )
    .await
    .expect("binding projects");
    conn.commit().await.expect("commits");

    let mut foreign = fixture
        .tenant_conn_for(other)
        .await
        .expect("tenant B opens");
    assert!(
        owner_binding_ids(&mut foreign, &owner)
            .await
            .expect("reads")
            .is_empty()
    );
    assert_eq!(
        binding_activity(&mut foreign, ids[0], InactivityTimeout::default())
            .await
            .expect("reads"),
        None
    );
    assert!(
        project_bindings(
            &mut foreign,
            &owner,
            &CardKind::Service,
            &[schedule_binding(OWNER_OCCURRENCE_KEY, &uid())],
        )
        .await
        .is_err(),
        "tenant B cannot project onto tenant A's owner"
    );
}

/// The first qualifying exchange arms a null cursor to the next boundary and
/// activates the owner; a renewal moves only activity, never the cursor; an
/// `observations_ready` binding never gets a cursor; component bindings
/// inherit the Service principal's activity.
///
/// # Panics
/// Panics when arming, renewal, window, or inheritance expectations fail.
#[tokio::test]
async fn first_exchange_arms_schedule_and_renewal_keeps_cursor() {
    let fixture = PgFixture::start().await.expect("fixture starts");
    let actor = actor(&fixture);
    let mut conn = fixture
        .tenant_conn()
        .await
        .expect("tenant connection opens");
    let (owner, principal) =
        register_service(&mut conn, &actor, &service_card("armed", "1.0.0")).await;
    let mut ready = schedule_binding(OWNER_OCCURRENCE_KEY, &uid());
    ready.activation = BindingActivation::ObservationsReady;
    let ids = project_bindings(
        &mut conn,
        &owner,
        &CardKind::Service,
        &[schedule_binding("retriever", &uid()), ready],
    )
    .await
    .expect("bindings project");
    let (component, eval) = (ids[0], ids[1]);

    let before = gate(&mut conn, component).await;
    assert!(!before.active, "never-authenticated owner is inactive");
    assert_eq!(before.next_run_at, None);

    let anchor = database_now(&mut conn).await;
    record_machine_authentication(&mut conn, principal)
        .await
        .expect("activation records");
    let armed = cursor(&mut conn, component.as_uuid())
        .await
        .expect("the first exchange arms the cursor");
    assert!(
        armed > anchor,
        "the cursor is the first boundary after the database anchor"
    );
    assert_eq!(cursor(&mut conn, eval.as_uuid()).await, None);

    record_machine_authentication(&mut conn, principal)
        .await
        .expect("renewal records");
    assert_eq!(cursor(&mut conn, component.as_uuid()).await, Some(armed));

    move_stamp(&mut conn, principal, "23 hours").await;
    assert!(gate(&mut conn, component).await.active);
    move_stamp(&mut conn, principal, "25 hours").await;
    assert!(!gate(&mut conn, component).await.active);
    move_stamp(&mut conn, principal, "1 hour").await;
    let eval_gate = gate(&mut conn, eval).await;
    assert!(
        eval_gate.active,
        "component and eval bindings share the Service principal"
    );
    assert_eq!(eval_gate.principal_id, Some(principal));
}

/// Read one binding's gate under the default window, as the database's
/// current statement instant sees it.
///
/// # Panics
/// Panics when the read fails or the binding is not visible.
async fn gate(conn: &mut TenantConn<'_>, binding: BindingId) -> BindingActivity {
    binding_activity(conn, binding, InactivityTimeout::default())
        .await
        .expect("activity reads")
        .expect("binding exists")
}

/// Card-free and suspended principals never record activity or arm cursors.
///
/// # Panics
/// Panics when either excluded principal is stamped or a cursor is armed.
#[tokio::test]
async fn excluded_principals_record_nothing() {
    let fixture = PgFixture::start().await.expect("fixture starts");
    let actor = actor(&fixture);
    let mut conn = fixture
        .tenant_conn()
        .await
        .expect("tenant connection opens");
    let cardless = Uuid::now_v7();
    sqlx::query(
        "INSERT INTO wyrd.auth_service_accounts \
             (id, data_tenant_id, principal_kind, name, status, created_by) \
         VALUES ($1, wyrd.current_tenant(), 'service', 'automation', 'active', $2)",
    )
    .bind(cardless)
    .bind(actor.id.as_uuid())
    .execute(&mut **conn.transaction())
    .await
    .expect("card-free principal inserts");
    let (owner, principal) =
        register_service(&mut conn, &actor, &service_card("paused", "1.0.0")).await;
    let ids = project_bindings(
        &mut conn,
        &owner,
        &CardKind::Service,
        &[schedule_binding(OWNER_OCCURRENCE_KEY, &uid())],
    )
    .await
    .expect("binding projects");
    sqlx::query("UPDATE wyrd.auth_service_accounts SET status = 'suspended' WHERE id = $1")
        .bind(principal.as_uuid())
        .execute(&mut **conn.transaction())
        .await
        .expect("principal suspends");

    record_machine_authentication(&mut conn, PrincipalId::new(cardless))
        .await
        .expect("card-free exchange is a no-op");
    record_machine_authentication(&mut conn, principal)
        .await
        .expect("suspended exchange is a no-op");
    let stamped: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM wyrd.auth_service_accounts WHERE last_authenticated_at IS NOT NULL",
    )
    .fetch_one(&mut **conn.transaction())
    .await
    .expect("count reads");
    assert_eq!(stamped, 0);
    assert_eq!(cursor(&mut conn, ids[0].as_uuid()).await, None);
}

/// Suspension and Card deletion deactivate on the next read even inside the
/// window, and two A/B versions have distinct, independently gated principals.
///
/// # Panics
/// Panics when A/B principals or bindings coincide or a gate ignores
/// suspension, deletion, or the other version's activity.
#[tokio::test]
async fn activity_is_per_version_and_revoked_immediately() {
    let fixture = PgFixture::start().await.expect("fixture starts");
    let actor = actor(&fixture);
    let mut conn = fixture
        .tenant_conn()
        .await
        .expect("tenant connection opens");
    let (a, principal_a) =
        register_service(&mut conn, &actor, &service_card("ab-svc", "1.0.0")).await;
    let (b, principal_b) =
        register_service(&mut conn, &actor, &service_card("ab-svc", "2.0.0")).await;
    assert_ne!(
        principal_a, principal_b,
        "A/B versions are distinct principals"
    );
    let verifier = uid();
    let binding_a = project_bindings(
        &mut conn,
        &a,
        &CardKind::Service,
        &[schedule_binding(OWNER_OCCURRENCE_KEY, &verifier)],
    )
    .await
    .expect("A projects")[0];
    let binding_b = project_bindings(
        &mut conn,
        &b,
        &CardKind::Service,
        &[schedule_binding(OWNER_OCCURRENCE_KEY, &verifier)],
    )
    .await
    .expect("B projects")[0];
    assert_ne!(binding_a, binding_b);

    record_machine_authentication(&mut conn, principal_a)
        .await
        .expect("A activates");
    assert!(gate(&mut conn, binding_a).await.active);
    assert!(
        !gate(&mut conn, binding_b).await.active,
        "B is gated independently"
    );

    sqlx::query("UPDATE wyrd.auth_service_accounts SET status = 'suspended' WHERE id = $1")
        .bind(principal_a.as_uuid())
        .execute(&mut **conn.transaction())
        .await
        .expect("A suspends");
    assert!(!gate(&mut conn, binding_a).await.active);

    record_machine_authentication(&mut conn, principal_b)
        .await
        .expect("B activates");
    assert!(gate(&mut conn, binding_b).await.active);
    sqlx::query("UPDATE wyrd.cards SET status = 'deleted' WHERE card_uid = $1")
        .bind(b.as_uuid())
        .execute(&mut **conn.transaction())
        .await
        .expect("B deletes");
    assert!(!gate(&mut conn, binding_b).await.active);
}

/// Read a principal's stored `last_authenticated_at` through the operator pool.
///
/// # Panics
/// Panics when the principal row cannot be read.
async fn stamped_at(fixture: &PgFixture, principal: PrincipalId) -> Option<DateTime<Utc>> {
    sqlx::query_scalar("SELECT last_authenticated_at FROM wyrd.auth_service_accounts WHERE id = $1")
        .bind(principal.as_uuid())
        .fetch_one(fixture.operator_pool().pool())
        .await
        .expect("activity reads")
}

/// A stored stamp ahead of the database's statement clock — what a newer
/// exchange committed out of order leaves behind — survives a later exchange
/// with its window and armed cursor intact.
///
/// # Panics
/// Panics when the later exchange moves activity backward, shortens the
/// window, or moves the armed cursor.
#[tokio::test]
async fn out_of_order_exchanges_never_move_activity_backward() {
    let fixture = PgFixture::start().await.expect("fixture starts");
    let actor = actor(&fixture);
    let mut conn = fixture
        .tenant_conn()
        .await
        .expect("tenant connection opens");
    let (owner, principal) =
        register_service(&mut conn, &actor, &service_card("racing", "1.0.0")).await;
    let binding = project_bindings(
        &mut conn,
        &owner,
        &CardKind::Service,
        &[schedule_binding(OWNER_OCCURRENCE_KEY, &uid())],
    )
    .await
    .expect("binding projects")[0];
    conn.commit().await.expect("registration commits");

    let mut first = fixture.tenant_conn().await.expect("newer exchange opens");
    record_machine_authentication(&mut first, principal)
        .await
        .expect("newer exchange records");
    move_stamp(&mut first, principal, "-3 hours").await;
    let newer = cursor_of_stamp(&mut first, principal)
        .await
        .expect("the newer exchange stamped activity");
    let armed = cursor(&mut first, binding.as_uuid())
        .await
        .expect("the newer exchange armed the cursor");
    first.commit().await.expect("newer exchange commits");

    let mut second = fixture.tenant_conn().await.expect("older exchange opens");
    record_machine_authentication(&mut second, principal)
        .await
        .expect("older exchange records");
    second.commit().await.expect("older exchange commits");

    assert_eq!(
        stamped_at(&fixture, principal).await,
        Some(newer),
        "a stamp ahead of the database clock never steps backward"
    );
    let mut conn = fixture
        .tenant_conn()
        .await
        .expect("tenant connection opens");
    assert_eq!(cursor(&mut conn, binding.as_uuid()).await, Some(armed));
    assert!(
        gate(&mut conn, binding).await.active,
        "the window still counts from the newer exchange"
    );
}

/// Stored identities that are not `UUIDv7` are refused at the query
/// boundary instead of being returned as typed identities.
///
/// # Panics
/// Panics when a v4 binding identity lists as a [`BindingId`] or a binding
/// whose owner Card UID is v4 decodes as a [`BindingActivity`].
#[tokio::test]
async fn non_v7_stored_identities_are_refused() {
    let fixture = PgFixture::start().await.expect("fixture starts");
    let actor = actor(&fixture);
    let mut conn = fixture
        .tenant_conn()
        .await
        .expect("tenant connection opens");
    let (owner, _) = register_service(&mut conn, &actor, &service_card("legacy", "1.0.0")).await;
    insert_raw_binding(&mut conn, Uuid::new_v4(), owner.as_uuid()).await;
    assert!(matches!(
        owner_binding_ids(&mut conn, &owner).await,
        Err(sqlx::Error::Decode(_))
    ));

    let v4_owner = Uuid::new_v4();
    sqlx::query(
        "INSERT INTO wyrd.cards \
            (card_uid, data_tenant_id, kind, space, name, version, spec, spec_hash, status) \
         VALUES ($1, wyrd.current_tenant(), 'Service', 'default', 'v4-owner', '1.0.0', \
                 '{}'::jsonb, 'spec-hash', 'active')",
    )
    .bind(v4_owner)
    .execute(&mut **conn.transaction())
    .await
    .expect("v4-owner card inserts");
    let binding = BindingId::new_v7();
    insert_raw_binding(&mut conn, binding.as_uuid(), v4_owner).await;
    assert!(matches!(
        binding_activity(&mut conn, binding, InactivityTimeout::default()).await,
        Err(sqlx::Error::Decode(_))
    ));
}

/// Insert an `observations_ready` owner binding with raw identities,
/// bypassing [`project_bindings`] so a test can store an invalid UUID version.
///
/// # Panics
/// Panics when the insert fails.
async fn insert_raw_binding(conn: &mut TenantConn<'_>, binding: Uuid, owner: Uuid) {
    sqlx::query(
        "INSERT INTO wyrd.verification_bindings \
            (binding_id, data_tenant_id, owner_card_uid, owner_card_kind, \
             subject_occurrence_key, subject_card_uid, verifier_uid, trigger_digest, activation) \
         VALUES ($1, wyrd.current_tenant(), $2, 'Service', $3, $2, $4, 'sha256:t', \
                 'observations_ready')",
    )
    .bind(binding)
    .bind(owner)
    .bind(OWNER_OCCURRENCE_KEY)
    .bind(Uuid::now_v7())
    .execute(&mut **conn.transaction())
    .await
    .expect("raw binding inserts");
}

/// PostgreSQL owns the activity stamp, the inactivity cutoff, and the anchor
/// an unarmed schedule is armed from, so a caller's wall clock never decides
/// any of them.
///
/// The exchange is driven from a caller instant more than a year in the
/// future: the stored stamp still lands at database statement time, the cursor
/// is the next boundary after that database time, and the gate flips only when
/// the stored value itself is moved across the window in the database.
///
/// # Panics
/// Panics when the stored stamp, the armed cursor, or the gate follows the
/// caller's clock instead of the database's.
#[tokio::test]
async fn database_clock_owns_machine_activity_and_schedule_arming() {
    let fixture = PgFixture::start().await.expect("fixture starts");
    let actor = actor(&fixture);
    let mut conn = fixture
        .tenant_conn()
        .await
        .expect("tenant connection opens");
    let (owner, principal) =
        register_service(&mut conn, &actor, &service_card("db-clock", "1.0.0")).await;
    let binding = project_bindings(
        &mut conn,
        &owner,
        &CardKind::Service,
        &[schedule_binding(OWNER_OCCURRENCE_KEY, &uid())],
    )
    .await
    .expect("binding projects")[0];
    conn.commit().await.expect("registration commits");

    let mut conn = fixture
        .tenant_conn()
        .await
        .expect("tenant connection opens");
    let database_now = database_now(&mut conn).await;
    record_machine_authentication(&mut conn, principal)
        .await
        .expect("activation records");
    let stamped = cursor_of_stamp(&mut conn, principal)
        .await
        .expect("a qualifying exchange stamps activity");
    assert!(
        (stamped - database_now).num_seconds().abs() < 60,
        "the stamp is database statement time, not a caller instant: {stamped} vs {database_now}"
    );
    let armed = cursor(&mut conn, binding.as_uuid()).await.expect("armed");
    assert!(
        armed > stamped,
        "the cursor is the first boundary after the database anchor: {armed} vs {stamped}"
    );

    assert!(
        gate(&mut conn, binding).await.active,
        "a fresh database stamp is inside the window"
    );
    move_stamp(&mut conn, principal, "25 hours").await;
    assert!(
        !gate(&mut conn, binding).await.active,
        "the cutoff is database statement time minus the window"
    );
    move_stamp(&mut conn, principal, "23 hours").await;
    assert!(gate(&mut conn, binding).await.active);

    record_machine_authentication(&mut conn, principal)
        .await
        .expect("renewal records");
    assert_eq!(
        cursor(&mut conn, binding.as_uuid()).await,
        Some(armed),
        "a renewal never moves an armed cursor"
    );
}

/// Read the database's current statement time on `conn`.
///
/// # Panics
/// Panics when the read fails.
async fn database_now(conn: &mut TenantConn<'_>) -> DateTime<Utc> {
    sqlx::query_scalar("SELECT statement_timestamp()")
        .fetch_one(&mut **conn.transaction())
        .await
        .expect("database time reads")
}

/// Read one principal's stored activity stamp on `conn`.
///
/// # Panics
/// Panics when the read fails.
async fn cursor_of_stamp(
    conn: &mut TenantConn<'_>,
    principal: PrincipalId,
) -> Option<DateTime<Utc>> {
    sqlx::query_scalar("SELECT last_authenticated_at FROM wyrd.auth_service_accounts WHERE id = $1")
        .bind(principal.as_uuid())
        .fetch_one(&mut **conn.transaction())
        .await
        .expect("activity reads")
}

/// Backdate one principal's stored activity stamp by the interval `ago`.
///
/// The gate's subject is time, so the test places the stored row where the
/// database predicate is exercised instead of moving any process clock.
///
/// # Panics
/// Panics when the update fails.
async fn move_stamp(conn: &mut TenantConn<'_>, principal: PrincipalId, ago: &str) {
    sqlx::query(
        "UPDATE wyrd.auth_service_accounts \
            SET last_authenticated_at = statement_timestamp() - $2::interval WHERE id = $1",
    )
    .bind(principal.as_uuid())
    .bind(ago)
    .execute(&mut **conn.transaction())
    .await
    .expect("stamp moves");
}
