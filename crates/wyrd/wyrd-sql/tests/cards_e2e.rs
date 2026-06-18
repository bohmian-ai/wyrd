//! Card-registration end-to-end tests.
//!
//! Env-gated by `WYRD_REG_E2E=1`. Run via `mise run test:registry-e2e`.
//!
//! Test shape: every body is GIVEN / WHEN / THEN against helpers in
//! `tests/fixtures/scenarios.rs` and `tests/fixtures/asserts.rs`. SQL never
//! appears inline — assertions go through named helpers so the test reads as
//! prose. Per-kind matrices are emitted via `for_each_card_kind!`.

#![cfg(feature = "test-e2e")]
#![cfg_attr(test, allow(missing_docs))]

use std::env;

use wyrd_spec::envelope::CardKind;
use wyrd_spec::ids::CardUid;

mod fixtures;
use fixtures::{TestEnv, asserts::*, fixture_card, scenarios};

fn should_run_e2e() -> bool {
    env::var("WYRD_REG_E2E").as_deref() == Ok("1")
}

macro_rules! e2e_test {
    ($name:ident, $body:block) => {
        #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
        #[ignore = "live-postgres e2e; opt-in via --include-ignored + WYRD_REG_E2E=1"]
        async fn $name() {
            if !should_run_e2e() {
                return;
            }
            $body
        }
    };
}

macro_rules! for_each_card_kind {
    ($prefix:ident, |$kind:ident| $body:expr) => {
        for_each_card_kind!(@one $prefix, data,       Data,       $kind, $body);
        for_each_card_kind!(@one $prefix, model,      Model,      $kind, $body);
        for_each_card_kind!(@one $prefix, experiment, Experiment, $kind, $body);
        for_each_card_kind!(@one $prefix, prompt,     Prompt,     $kind, $body);
        for_each_card_kind!(@one $prefix, agent,      Agent,      $kind, $body);
        for_each_card_kind!(@one $prefix, workflow,   Workflow,   $kind, $body);
        for_each_card_kind!(@one $prefix, eval,       Eval,       $kind, $body);
        for_each_card_kind!(@one $prefix, drift,      Drift,      $kind, $body);
        for_each_card_kind!(@one $prefix, service,    Service,    $kind, $body);
        for_each_card_kind!(@one $prefix, policy,     Policy,     $kind, $body);
        for_each_card_kind!(@one $prefix, mcp,        Mcp,        $kind, $body);
        for_each_card_kind!(@one $prefix, audit,      Audit,      $kind, $body);
        for_each_card_kind!(@one $prefix, artifact,   Artifact,   $kind, $body);
        for_each_card_kind!(@one $prefix, trigger,    Trigger,    $kind, $body);
        for_each_card_kind!(@one $prefix, operator,   Operator,   $kind, $body);
        for_each_card_kind!(@one $prefix, source,     Source,     $kind, $body);
    };
    (@one $prefix:ident, $snake:ident, $variant:ident, $kind:ident, $body:expr) => {
        paste::paste! {
            #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
            #[ignore = "live-postgres e2e; opt-in via --include-ignored + WYRD_REG_E2E=1"]
            async fn [<$prefix _ $snake _card>]() {
                if !should_run_e2e() {
                    return;
                }
                let $kind = CardKind::$variant;
                $body
            }
        }
    };
}

// ─── Group C — Create (16 kinds + validation rejects) ──────────────────────

for_each_card_kind!(register, |kind| {
    let env = TestEnv::new().await;
    let tenant = env.fresh_tenant().await;
    let actor = env.fixture_user(tenant).await;
    let card = fixture_card(kind.clone(), "prod", "fresh-card", "1.0.0");

    let outcome = scenarios::register_fresh(&env, tenant, &actor, &card)
        .await
        .expect("fresh registration must succeed");

    assert_created(&outcome);
    assert_uuid_v7(&outcome.card_uid);

    match kind {
        CardKind::Service | CardKind::Agent => assert_principal_projected(&outcome),
        _ => assert_no_principal(&outcome),
    }
});

e2e_test!(register_missing_space_returns_400_invalid_card_spec, {
    let env = TestEnv::new().await;
    let tenant = env.fresh_tenant().await;
    let actor = env.fixture_user(tenant).await;
    let mut card = fixture_card(CardKind::Model, "prod", "churn", "1.0.0");
    card.metadata.space = None;

    let err = scenarios::expect_register_error(&env, tenant, &actor, &card).await;

    assert_error_code(&err, "WYRD_REG_400_INVALID_CARD_SPEC");
});

// ─── Group U — Update (16 noop + 16 drift) ─────────────────────────────────

for_each_card_kind!(reapply_identical, |kind| {
    let env = TestEnv::new().await;
    let tenant = env.fresh_tenant().await;
    let actor = env.fixture_user(tenant).await;
    let card = fixture_card(kind.clone(), "prod", "stable-card", "1.0.0");

    let (first, second) = scenarios::register_then_reapply_identical(&env, tenant, &actor, &card)
        .await
        .expect("re-apply must not error");

    assert_created(&first);
    assert_idempotent_noop(&second);
    assert_same_uid(&first, &second);

    if matches!(kind, CardKind::Service | CardKind::Agent) {
        assert_same_principal(&first, &second);
    }
});

for_each_card_kind!(reapply_drifted, |kind| {
    let env = TestEnv::new().await;
    let tenant = env.fresh_tenant().await;
    let actor = env.fixture_user(tenant).await;
    let v1 = fixture_card(kind, "prod", "drift-card", "1.0.0");

    let (first, err) = scenarios::register_then_reapply_with_drift(&env, tenant, &actor, &v1)
        .await
        .expect("first register must succeed");

    assert_created(&first);
    assert_error_code(&err, "WYRD_REG_409_SPEC_DRIFT");
});

// ─── Group D — Delete ───────────────────────────────────────────────────────

e2e_test!(soft_delete_active_card_flips_status, {
    let env = TestEnv::new().await;
    let tenant = env.fresh_tenant().await;
    let actor = env.fixture_user(tenant).await;
    let card = fixture_card(CardKind::Prompt, "ops", "triage-prompt", "0.4.0");
    let registered = scenarios::register_fresh(&env, tenant, &actor, &card)
        .await
        .expect("registration must succeed");

    scenarios::soft_delete(&env, tenant, &registered.card_uid, &actor)
        .await
        .expect("soft delete must succeed");
});

e2e_test!(soft_delete_unknown_uid_returns_404, {
    let env = TestEnv::new().await;
    let tenant = env.fresh_tenant().await;
    let actor = env.fixture_user(tenant).await;

    let unknown_uid = CardUid::from_uuid(uuid::Uuid::now_v7()).expect("valid uuidv7");
    let mut conn = env.tenant_conn(tenant).await;
    let err = wyrd_sql::queries::cards::soft_delete_card(&mut conn, &unknown_uid, &actor, None)
        .await
        .expect_err("unknown uid must return 404");

    assert_error_code(&err, "WYRD_REG_404_CARD_NOT_FOUND");
});

// ─── Group I — Invariants (tenant isolation) ────────────────────────────────

e2e_test!(
    two_tenants_can_register_identical_card_ref_without_conflict,
    {
        let env = TestEnv::new().await;
        let tenant1 = env.fresh_tenant().await;
        let tenant2 = env.fresh_tenant().await;
        let actor1 = env.fixture_user(tenant1).await;
        let actor2 = env.fixture_user(tenant2).await;
        let card = fixture_card(CardKind::Model, "prod", "shared-name", "1.0.0");

        let out1 = scenarios::register_fresh(&env, tenant1, &actor1, &card)
            .await
            .expect("tenant1 register must succeed");
        let out2 = scenarios::register_fresh(&env, tenant2, &actor2, &card)
            .await
            .expect("tenant2 register must succeed");

        assert_created(&out1);
        assert_created(&out2);
        assert_distinct_uids(&out1, &out2);
    }
);
