//! Shared card-seeding helpers for auth integration tests.

use uuid::Uuid;
use wyrd_spec::envelope::{CardKind, Spec};
use wyrd_spec::reference::CardRef;
use wyrd_sql::TenantConn;

/// Insert a card row for `card_ref` using an auto-generated spec for its kind.
///
/// Handles `Service` and `Agent` card kinds. Panics for unsupported kinds —
/// call `seed_card_with_spec` directly when the spec must be explicit.
///
/// Uses `ON CONFLICT ... DO NOTHING` so multiple test helpers seeding the
/// same card (e.g. during fixture setup) are idempotent.
pub async fn seed_backing_card(conn: &mut TenantConn<'_>, card_ref: &CardRef, created_by: Uuid) {
    let spec = match &card_ref.kind {
        CardKind::Service => Spec::from_kind_and_value(&CardKind::Service, serde_json::json!({}))
            .expect("service fixture spec decodes"),
        CardKind::Agent => Spec::from_kind_and_value(&CardKind::Agent, serde_json::json!({}))
            .expect("agent fixture spec decodes"),
        other => panic!("seed_backing_card: unsupported card kind {other:?}"),
    };
    seed_card_with_spec(conn, card_ref, &spec, created_by).await;
}

/// Insert a card row for `card_ref` using the provided `spec`.
///
/// Uses `ON CONFLICT ... DO NOTHING` for idempotency.
pub async fn seed_card_with_spec(
    conn: &mut TenantConn<'_>,
    card_ref: &CardRef,
    spec: &Spec,
    created_by: Uuid,
) {
    let (spec_hash, _) = spec
        .canonical_hash_with_bytes()
        .expect("fixture spec hashes");
    let spec_json = serde_json::to_value(spec).expect("fixture spec serializes");

    sqlx::query(
        r#"
        INSERT INTO wyrd.cards (
            card_uid, data_tenant_id, kind, space, name, version, spec,
            spec_hash, artifact_hash, labels, annotations, status, created_by
        )
        VALUES (
            $1, wyrd.current_tenant(), $2, $3, $4, $5, $6,
            $7, NULL, '{}'::jsonb, '{}'::jsonb, 'active', $8
        )
        ON CONFLICT (data_tenant_id, kind, space, name, version) DO NOTHING
        "#,
    )
    .bind(Uuid::now_v7())
    .bind(card_ref.kind.wire_name())
    .bind(card_ref.space.as_str())
    .bind(card_ref.name.as_str())
    .bind(card_ref.version.as_str())
    .bind(spec_json)
    .bind(spec_hash.as_str())
    .bind(created_by)
    .execute(&mut **conn.transaction())
    .await
    .expect("fixture card inserts");
}
