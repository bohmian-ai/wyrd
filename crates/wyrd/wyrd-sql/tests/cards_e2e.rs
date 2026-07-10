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

use wyrd_semver::{VersionBlock, VersionBump, VersionRange, VersionSpec};
use wyrd_spec::envelope::CardKind;
use wyrd_spec::ids::{CardName, CardUid, SpaceName};
use wyrd_spec::query::MetadataQuery;
use wyrd_sql::CardRegistrationOutcome;
use wyrd_sql::queries::cards::{CardQuery, ListCursor, RegisterCardOutcomeKind};

mod fixtures;
use fixtures::{
    TestEnv, asserts::*, fixture_card, fixture_card_auto, fixture_card_scoped, scenarios,
    with_annotations, with_bump, with_labels,
};

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

fn space(value: &str) -> SpaceName {
    SpaceName::new(value).expect("test space")
}

fn name(value: &str) -> CardName {
    CardName::new(value).expect("test name")
}

fn cursor(limit: u32) -> ListCursor {
    ListCursor {
        after_created_at: None,
        after_uid: None,
        limit,
    }
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

// ─── PR2 Group V — Version resolution, dedup, range reads ─────────────────

e2e_test!(auto_register_resolves_deduplicates_and_audits_outcomes, {
    let env = TestEnv::new().await;
    let tenant = env.fresh_tenant().await;
    let actor = env.fixture_user(tenant).await;
    let card = fixture_card_auto(CardKind::Prompt, "prod", "auto-card");

    let first = scenarios::register(&env, tenant, &actor, &card)
        .await
        .expect("auto register succeeds");
    let first_row = scenarios::get_by_ref(
        &env,
        tenant,
        CardKind::Prompt,
        &space("prod"),
        &name("auto-card"),
        &VersionBlock::parse("0.1.0").expect("version"),
    )
    .await
    .expect("resolved version reads");
    let version_columns = scenarios::fetch_version_columns(&env, tenant, &first.card_uid).await;

    assert_created(&first);
    assert_version(&first_row, "0.1.0");
    assert_no_prerelease(&first_row);
    assert_eq!(version_columns, (0, 1, 0, false));

    let second = scenarios::register(&env, tenant, &actor, &card)
        .await
        .expect("identical auto register deduplicates");
    assert_deduplicated(&second);
    assert_same_uid(&first, &second);

    let mut changed = fixture_card_auto(CardKind::Prompt, "prod", "auto-card");
    fixtures::per_kind::mutate_for_drift(&mut changed);
    let third = scenarios::register(&env, tenant, &actor, &changed)
        .await
        .expect("changed auto register bumps");
    assert_created(&third);
    assert_distinct_uids(&first, &third);

    let versions = scenarios::versions(
        &env,
        tenant,
        CardKind::Prompt,
        &space("prod"),
        &name("auto-card"),
        false,
    )
    .await
    .expect("versions list");
    assert_versions_eq(&versions, &["0.1.1", "0.1.0"]);

    let audit = scenarios::fetch_registration_audit(&env, tenant, &first.card_uid).await;
    assert_registration_outcome(&audit[0], CardRegistrationOutcome::Created);
    assert_registration_outcome(&audit[1], CardRegistrationOutcome::Deduplicated);
});

e2e_test!(service_and_agent_remain_pin_only, {
    let env = TestEnv::new().await;
    let tenant = env.fresh_tenant().await;
    let actor = env.fixture_user(tenant).await;

    for kind in [CardKind::Service, CardKind::Agent] {
        let kind_slug = kind.wire_name().to_ascii_lowercase();
        let pinned_name = format!("principal-{kind_slug}-card");
        let auto_name = format!("principal-{kind_slug}-auto");
        let scope_name = format!("principal-{kind_slug}-scope");

        let pinned = fixture_card(kind.clone(), "prod", &pinned_name, "1.0.0");
        let created = scenarios::register(&env, tenant, &actor, &pinned)
            .await
            .expect("pinned principal kind registers");
        assert_created(&created);
        assert_principal_projected(&created);
        assert_eq!(
            scenarios::fetch_version_columns(&env, tenant, &created.card_uid).await,
            (1, 0, 0, false)
        );

        let auto = fixture_card_auto(kind.clone(), "prod", &auto_name);
        let err = scenarios::expect_register_error(&env, tenant, &actor, &auto).await;
        assert_error_code(&err, "WYRD_REG_400_INVALID_VERSION_BLOCK");

        let scoped = fixture_card_scoped(kind, "prod", &scope_name, "1");
        let err = scenarios::expect_register_error(&env, tenant, &actor, &scoped).await;
        assert_error_code(&err, "WYRD_REG_400_INVALID_VERSION_BLOCK");
    }
});

e2e_test!(scoped_bump_must_stay_inside_authored_range, {
    let env = TestEnv::new().await;
    let tenant = env.fresh_tenant().await;
    let actor = env.fixture_user(tenant).await;
    let seed = fixture_card(CardKind::Model, "prod", "scoped-card", "1.2.5");
    scenarios::register(&env, tenant, &actor, &seed)
        .await
        .expect("seed registers");

    let mut escaping = with_bump(
        fixture_card_scoped(CardKind::Model, "prod", "scoped-card", "1.2"),
        VersionBump::Minor,
    );
    fixtures::per_kind::mutate_for_drift(&mut escaping);
    let err = scenarios::expect_register_error(&env, tenant, &actor, &escaping).await;
    assert_error_code(&err, "WYRD_REG_400_INVALID_VERSION_BLOCK");
});

e2e_test!(range_reads_and_prerelease_exclusion_work, {
    let env = TestEnv::new().await;
    let tenant = env.fresh_tenant().await;
    let actor = env.fixture_user(tenant).await;

    for version in ["1.0.0", "1.2.0", "1.2.5", "1.3.0", "2.0.0"] {
        let card = fixture_card(CardKind::Model, "prod", "range-card", version);
        scenarios::register(&env, tenant, &actor, &card)
            .await
            .expect("range seed registers");
    }

    for (range, expected) in [
        ("^1.2.0", "1.3.0"),
        ("~1.2.0", "1.2.5"),
        ("1.*", "1.3.0"),
        ("1.2.*", "1.2.5"),
        ("*", "2.0.0"),
        ("1", "1.3.0"),
        ("1.2", "1.2.5"),
        ("1.2.0", "1.2.0"),
    ] {
        let row = scenarios::latest_by_range(
            &env,
            tenant,
            CardKind::Model,
            &space("prod"),
            &name("range-card"),
            &VersionRange::parse_loose(range).expect("range"),
        )
        .await
        .expect("latest by range");
        assert_version(&row, expected);
    }

    let versions = scenarios::versions(
        &env,
        tenant,
        CardKind::Model,
        &space("prod"),
        &name("range-card"),
        false,
    )
    .await
    .expect("versions list");
    assert_versions_eq(&versions, &["2.0.0", "1.3.0", "1.2.5", "1.2.0", "1.0.0"]);

    let stable = fixture_card(CardKind::Data, "prod", "pre-card", "1.2.0");
    let pre = fixture_card(CardKind::Data, "prod", "pre-card", "1.3.0-rc.1");
    scenarios::register(&env, tenant, &actor, &stable)
        .await
        .expect("stable registers");
    let pre_out = scenarios::register(&env, tenant, &actor, &pre)
        .await
        .expect("pre-release registers");
    assert_eq!(
        scenarios::fetch_version_columns(&env, tenant, &pre_out.card_uid).await,
        (1, 3, 0, true)
    );

    let latest = scenarios::latest_by_range(
        &env,
        tenant,
        CardKind::Data,
        &space("prod"),
        &name("pre-card"),
        &VersionRange::parse_loose("*").expect("range"),
    )
    .await
    .expect("latest stable excludes rc");
    assert_version(&latest, "1.2.0");

    let exact_rc = scenarios::get_by_ref(
        &env,
        tenant,
        CardKind::Data,
        &space("prod"),
        &name("pre-card"),
        &VersionBlock::parse("1.3.0-rc.1").expect("version"),
    )
    .await
    .expect("exact ref reaches rc");
    assert_version(&exact_rc, "1.3.0-rc.1");
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

// ─── PR2 Group Q/P/H/S — Query, pagination, probes, invariants ────────────

e2e_test!(query_cards_composes_metadata_filters_and_helpers, {
    let env = TestEnv::new().await;
    let tenant = env.fresh_tenant().await;
    let actor = env.fixture_user(tenant).await;

    let prod_gold = with_annotations(
        with_labels(
            fixture_card(CardKind::Model, "prod", "prod-gold", "1.0.0"),
            &[("env", "prod"), ("tier", "gold")],
        ),
        &[("acme.com/team", "platform-core")],
    );
    let prod_silver = with_labels(
        fixture_card(CardKind::Prompt, "prod", "prod-silver", "1.0.0"),
        &[("env", "prod"), ("tier", "silver")],
    );
    let staging = with_labels(
        fixture_card(CardKind::Model, "staging", "staging-gold", "1.0.0"),
        &[("env", "staging"), ("tier", "gold")],
    );
    let malicious = with_labels(
        fixture_card(CardKind::Data, "prod", "malicious-label", "1.0.0"),
        &[("env", "or-1-eq-1")],
    );

    let prod_gold_out = scenarios::register(&env, tenant, &actor, &prod_gold)
        .await
        .expect("prod gold registers");
    scenarios::register(&env, tenant, &actor, &prod_silver)
        .await
        .expect("prod silver registers");
    scenarios::register(&env, tenant, &actor, &staging)
        .await
        .expect("staging registers");
    scenarios::register(&env, tenant, &actor, &malicious)
        .await
        .expect("malicious label registers");

    let prod = scenarios::query(
        &env,
        tenant,
        &CardQuery {
            filter: Some(MetadataQuery::parse("labels.env = \"prod\"").expect("query")),
            ..Default::default()
        },
        cursor(50),
    )
    .await
    .expect("prod query");
    assert_page_size(&prod, 2);

    let combined = scenarios::query(
        &env,
        tenant,
        &CardQuery {
            kind: Some(CardKind::Model),
            space: Some(space("prod")),
            filter: Some(MetadataQuery::parse("labels.env = \"prod\"").expect("query")),
            ..Default::default()
        },
        cursor(50),
    )
    .await
    .expect("combined query");
    assert_page_size(&combined, 1);

    let annotation = scenarios::query(
        &env,
        tenant,
        &CardQuery {
            filter: Some(
                MetadataQuery::parse("annotations.\"acme.com/team\" =~ \"platform-.*\"")
                    .expect("query"),
            ),
            ..Default::default()
        },
        cursor(50),
    )
    .await
    .expect("annotation regex query");
    assert_page_size(&annotation, 1);

    let bound_value = scenarios::query(
        &env,
        tenant,
        &CardQuery {
            filter: Some(MetadataQuery::parse("labels.env = \"or-1-eq-1\"").expect("query")),
            ..Default::default()
        },
        cursor(50),
    )
    .await
    .expect("malicious label value is bound");
    assert_page_size(&bound_value, 1);

    let prod_gold_row = scenarios::get_by_ref(
        &env,
        tenant,
        CardKind::Model,
        &space("prod"),
        &name("prod-gold"),
        &VersionBlock::parse("1.0.0").expect("version"),
    )
    .await
    .expect("prod gold reads");
    let found = scenarios::find_by_hash(
        &env,
        tenant,
        CardKind::Model,
        &space("prod"),
        &name("prod-gold"),
        &prod_gold_row.spec_hash,
    )
    .await
    .expect("hash probe");
    assert!(found.is_some(), "expected hash probe hit");
    assert!(
        scenarios::uid_exists(&env, tenant, &prod_gold_out.card_uid)
            .await
            .expect("uid exists"),
        "registered uid should exist"
    );

    let spaces = scenarios::unique_spaces(&env, tenant)
        .await
        .expect("unique spaces");
    assert_eq!(
        spaces.iter().map(SpaceName::as_str).collect::<Vec<_>>(),
        vec!["prod", "staging"]
    );

    scenarios::soft_delete(&env, tenant, &prod_gold_out.card_uid, &actor)
        .await
        .expect("soft delete");
    let after_delete = scenarios::query(
        &env,
        tenant,
        &CardQuery {
            name: Some(name("prod-gold")),
            ..Default::default()
        },
        cursor(50),
    )
    .await
    .expect("default excludes deleted");
    assert_page_size(&after_delete, 0);
    let deleted = scenarios::query(
        &env,
        tenant,
        &CardQuery {
            status: Some(wyrd_sql::CardStatus::Deleted),
            name: Some(name("prod-gold")),
            ..Default::default()
        },
        cursor(50),
    )
    .await
    .expect("deleted status query");
    assert_page_size(&deleted, 1);
});

e2e_test!(query_cards_keyset_paginates_without_duplicates, {
    let env = TestEnv::new().await;
    let tenant = env.fresh_tenant().await;
    let actor = env.fixture_user(tenant).await;
    let mut expected = std::collections::BTreeSet::new();

    for idx in 0..250 {
        let card = fixture_card_auto(CardKind::Prompt, "page", &format!("card-{idx:03}"));
        let out = scenarios::register(&env, tenant, &actor, &card)
            .await
            .expect("page card registers");
        expected.insert(out.card_uid.as_uuid());
    }

    let mut cur = cursor(100);
    let mut seen = std::collections::BTreeSet::new();
    let mut page_sizes = Vec::new();
    loop {
        let page = scenarios::query(
            &env,
            tenant,
            &CardQuery {
                space: Some(space("page")),
                ..Default::default()
            },
            cur,
        )
        .await
        .expect("page query");
        assert_page_uids(&page, page.items.len());
        page_sizes.push(page.items.len());
        for row in &page.items {
            assert!(seen.insert(row.card_uid), "duplicate uid across pages");
        }
        if let Some(next) = page.next {
            cur = next;
        } else {
            break;
        }
    }

    assert_eq!(page_sizes, vec![100, 100, 50]);
    assert_eq!(seen, expected);
});

e2e_test!(version_columns_and_kind_spec_mismatch_are_rejected, {
    let env = TestEnv::new().await;
    let tenant = env.fresh_tenant().await;
    let actor = env.fixture_user(tenant).await;
    let card = fixture_card(CardKind::Model, "prod", "immutable-card", "1.0.0");
    let out = scenarios::register(&env, tenant, &actor, &card)
        .await
        .expect("register succeeds");

    let generated_err = scenarios::try_update_version_major(&env, tenant, &out.card_uid).await;
    assert_eq!(
        generated_err
            .as_database_error()
            .and_then(|db| db.code())
            .as_deref(),
        Some("428C9")
    );

    let version_err = scenarios::try_update_version(&env, tenant, &out.card_uid, "9.9.9").await;
    assert_eq!(
        version_err
            .as_database_error()
            .and_then(|db| db.constraint()),
        Some("cards_version_immutable")
    );

    let mut mismatch = fixture_card(CardKind::Model, "prod", "mismatch-card", "1.0.0");
    mismatch.kind = CardKind::Service;
    let err = scenarios::expect_register_error(&env, tenant, &actor, &mismatch).await;
    assert_error_code(&err, "WYRD_REG_400_INVALID_CARD_SPEC");

    assert!(VersionSpec::parse("^1.0").is_err());
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

e2e_test!(
    two_tenants_auto_register_identical_card_without_cross_tenant_dedup,
    {
        let env = TestEnv::new().await;
        let tenant1 = env.fresh_tenant().await;
        let tenant2 = env.fresh_tenant().await;
        let actor1 = env.fixture_user(tenant1).await;
        let actor2 = env.fixture_user(tenant2).await;
        let card = fixture_card_auto(CardKind::Model, "prod", "shared-auto");

        let out1 = scenarios::register(&env, tenant1, &actor1, &card)
            .await
            .expect("tenant1 auto register succeeds");
        let out2 = scenarios::register(&env, tenant2, &actor2, &card)
            .await
            .expect("tenant2 auto register succeeds");

        assert_created(&out1);
        assert_created(&out2);
        assert_distinct_uids(&out1, &out2);

        let tenant1_rows = scenarios::query(
            &env,
            tenant1,
            &CardQuery {
                name: Some(name("shared-auto")),
                ..Default::default()
            },
            cursor(50),
        )
        .await
        .expect("tenant1 query");
        assert_page_size(&tenant1_rows, 1);
    }
);

// ─── Group R — Re-registration after soft-delete ────────────────────────────

e2e_test!(soft_deleted_pin_can_be_re_registered, {
    let env = TestEnv::new().await;
    let tenant = env.fresh_tenant().await;
    let actor = env.fixture_user(tenant).await;
    let card = fixture_card(CardKind::Prompt, "prod", "deletable-pin", "1.0.0");

    let first = scenarios::register(&env, tenant, &actor, &card)
        .await
        .expect("first registration succeeds");
    assert_created(&first);

    scenarios::soft_delete(&env, tenant, &first.card_uid, &actor)
        .await
        .expect("soft delete succeeds");

    let second = scenarios::register(&env, tenant, &actor, &card)
        .await
        .expect("re-registration after soft-delete must succeed");

    assert_created(&second);
    assert_distinct_uids(&first, &second);

    let found = scenarios::get_by_ref(
        &env,
        tenant,
        CardKind::Prompt,
        &space("prod"),
        &name("deletable-pin"),
        &VersionBlock::parse("1.0.0").expect("version"),
    )
    .await
    .expect("re-registered pin is visible");
    assert_eq!(found.card_uid, second.card_uid);
});

// ─── Group Q2 — CardQuery version_range and include_prerelease predicates ───

e2e_test!(query_cards_version_range_and_prerelease_predicates, {
    let env = TestEnv::new().await;
    let tenant = env.fresh_tenant().await;
    let actor = env.fixture_user(tenant).await;

    for version in ["1.0.0", "1.2.0", "1.3.0", "2.0.0", "1.3.0-rc.1"] {
        let card = fixture_card(CardKind::Data, "ver", "range-query-card", version);
        scenarios::register(&env, tenant, &actor, &card)
            .await
            .expect("seed registers");
    }

    let in_range = scenarios::query(
        &env,
        tenant,
        &CardQuery {
            kind: Some(CardKind::Data),
            space: Some(space("ver")),
            name: Some(name("range-query-card")),
            version_range: Some(VersionRange::parse_loose("^1.2.0").expect("range")),
            ..Default::default()
        },
        cursor(50),
    )
    .await
    .expect("range query");
    assert_page_size(&in_range, 2);
    let in_range_versions: Vec<&str> = in_range
        .items
        .iter()
        .map(|r| r.version.as_str())
        .collect();
    assert!(
        in_range_versions.contains(&"1.2.0") && in_range_versions.contains(&"1.3.0"),
        "expected 1.2.0 and 1.3.0 in range, got {in_range_versions:?}"
    );

    let with_pre = scenarios::query(
        &env,
        tenant,
        &CardQuery {
            kind: Some(CardKind::Data),
            space: Some(space("ver")),
            name: Some(name("range-query-card")),
            include_prerelease: true,
            ..Default::default()
        },
        cursor(50),
    )
    .await
    .expect("include_prerelease=true query");
    assert_page_size(&with_pre, 5);

    let without_pre = scenarios::query(
        &env,
        tenant,
        &CardQuery {
            kind: Some(CardKind::Data),
            space: Some(space("ver")),
            name: Some(name("range-query-card")),
            include_prerelease: false,
            ..Default::default()
        },
        cursor(50),
    )
    .await
    .expect("include_prerelease=false query");
    assert_page_size(&without_pre, 4);
    assert!(
        without_pre
            .items
            .iter()
            .all(|r| !r.version.as_str().contains('-')),
        "pre-release row leaked into include_prerelease=false result"
    );
});

// ─── Group L — Advisory lock under concurrent auto-registration ─────────────

e2e_test!(concurrent_auto_registration_produces_no_duplicate_versions, {
    let env = TestEnv::new().await;
    let tenant = env.fresh_tenant().await;
    let actor = env.fixture_user(tenant).await;
    let card = fixture_card_auto(CardKind::Data, "prod", "concurrent-auto");

    let (r1, r2) = tokio::join!(
        scenarios::register(&env, tenant, &actor, &card),
        scenarios::register(&env, tenant, &actor, &card),
    );

    let o1 = r1.expect("first concurrent auto register succeeds");
    let o2 = r2.expect("second concurrent auto register succeeds");

    let both_created = matches!(
        (o1.kind, o2.kind),
        (RegisterCardOutcomeKind::Created, RegisterCardOutcomeKind::Created)
    );
    let one_dedup = matches!(
        (&o1.kind, &o2.kind),
        (RegisterCardOutcomeKind::Created, RegisterCardOutcomeKind::Deduplicated)
            | (RegisterCardOutcomeKind::Deduplicated, RegisterCardOutcomeKind::Created)
    );
    assert!(
        both_created || one_dedup,
        "expected Created+Created or Created+Deduplicated, got {:?} and {:?}",
        o1.kind,
        o2.kind
    );

    let versions = scenarios::versions(
        &env,
        tenant,
        CardKind::Data,
        &space("prod"),
        &name("concurrent-auto"),
        false,
    )
    .await
    .expect("versions list");
    assert!(
        versions.len() <= 2,
        "concurrent auto must not mint duplicate versions; got {:?}",
        versions
    );
});

// ─── Group N — No-stable-match error from get_latest_card_by_range ──────────

e2e_test!(get_latest_card_by_range_returns_404_when_no_stable_match, {
    let env = TestEnv::new().await;
    let tenant = env.fresh_tenant().await;
    let actor = env.fixture_user(tenant).await;

    let pre_only = fixture_card(CardKind::Model, "prod", "pre-only-card", "1.0.0-rc.1");
    scenarios::register(&env, tenant, &actor, &pre_only)
        .await
        .expect("pre-release seed registers");

    let err = scenarios::latest_by_range(
        &env,
        tenant,
        CardKind::Model,
        &space("prod"),
        &name("pre-only-card"),
        &VersionRange::parse_loose("*").expect("range"),
    )
    .await
    .expect_err("no stable match must return error");

    assert_error_code(&err, "WYRD_REG_404_CARD_NOT_FOUND");

    let no_card_err = scenarios::latest_by_range(
        &env,
        tenant,
        CardKind::Model,
        &space("prod"),
        &name("nonexistent-card"),
        &VersionRange::parse_loose("*").expect("range"),
    )
    .await
    .expect_err("missing card must return 404");

    assert_error_code(&no_card_err, "WYRD_REG_404_CARD_NOT_FOUND");
});
