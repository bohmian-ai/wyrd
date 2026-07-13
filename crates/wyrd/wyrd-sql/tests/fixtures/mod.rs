//! Shared fixture helpers for `cards_e2e.rs`.
#![allow(dead_code)]

pub mod asserts;
pub mod per_kind;
pub mod scenarios;

use sqlx::PgPool;
use wyrd_dev_fixtures::pg::PgFixture;
use wyrd_runtime::permission::PermissionSet;
use wyrd_runtime::principal::{Principal, PrincipalId, PrincipalKind};
use wyrd_semver::{VersionBlock, VersionBump, VersionSpec};
use wyrd_spec::api_version::ApiVersion;
use wyrd_spec::envelope::{Card, CardKind};
use wyrd_spec::ids::{CardName, DataTenantId, SpaceName};
use wyrd_sql::TenantConn;

pub struct TestEnv {
    pub pool: PgPool,
    pub app_pool: PgPool,
    _fixture: PgFixture,
}

impl TestEnv {
    pub async fn new() -> Self {
        let fixture = PgFixture::start()
            .await
            .expect("shared test DB must be configured for e2e tests");
        let pool = fixture.superuser_pool().await.expect("superuser pool");
        let app_pool = fixture.app_pool().clone();
        Self {
            pool,
            app_pool,
            _fixture: fixture,
        }
    }

    pub async fn fresh_tenant(&self) -> DataTenantId {
        let id = DataTenantId::new_v7();
        sqlx::query(
            "INSERT INTO platform.tenants (data_tenant_id, slug, display_name, status) \
             VALUES ($1, $2, $3, 'active') ON CONFLICT DO NOTHING",
        )
        .bind(id.as_uuid())
        .bind(format!("test-{}", id.as_uuid().simple()))
        .bind("Test Tenant")
        .execute(&self.pool)
        .await
        .expect("failed to insert test tenant");
        id
    }

    pub async fn tenant_conn(&self, tenant: DataTenantId) -> TenantConn<'_> {
        TenantConn::acquire(&self.app_pool, tenant)
            .await
            .expect("failed to open TenantConn")
    }

    /// Insert an active human user under `tenant` and return a `Principal`
    /// pointing at it. Service / Agent card registration writes
    /// `created_by = principal.id` into `wyrd.auth_service_accounts`, which
    /// FK-references `wyrd.auth_users(data_tenant_id, id)`; the row must exist
    /// before registration runs.
    pub async fn fixture_user(&self, tenant: DataTenantId) -> Principal {
        let id = uuid::Uuid::now_v7();
        sqlx::query(
            "INSERT INTO wyrd.auth_users \
                 (id, data_tenant_id, email, auth_type, status) \
             VALUES ($1, $2, $3, 'password', 'active')",
        )
        .bind(id)
        .bind(tenant.as_uuid())
        .bind(format!("test-{}@wyrd.local", id.simple()))
        .execute(&self.pool)
        .await
        .expect("failed to insert fixture user");
        Principal::new(
            PrincipalId::new(id),
            PrincipalKind::User,
            tenant,
            vec![],
            PermissionSet::new(),
        )
    }

    pub async fn set_card_audit_insert_enabled(&self, enabled: bool) {
        let statement = if enabled {
            "GRANT INSERT ON vala.audit_outbox TO wyrd_app"
        } else {
            "REVOKE INSERT ON vala.audit_outbox FROM wyrd_app"
        };
        sqlx::query(statement)
            .execute(&self.pool)
            .await
            .expect("set card audit insert privilege");
    }
}

pub fn fixture_card(kind: CardKind, space: &str, name: &str, version: &str) -> Card {
    let spec = per_kind::minimal_spec(kind.clone());
    Card {
        api_version: ApiVersion::v1(),
        kind,
        metadata: wyrd_spec::envelope::Metadata {
            name: CardName::new(name).expect("fixture card name"),
            version: Some(VersionSpec::Pin(
                VersionBlock::parse(version).expect("fixture version"),
            )),
            bump: None,
            space: Some(SpaceName::new(space).expect("fixture space name")),
            uid: None,
            labels: Default::default(),
            annotations: Default::default(),
            spec_hash: None,
            artifact_hash: None,
            origin: None,
        },
        spec,
        relationships: Default::default(),
        status: None,
    }
}

pub fn fixture_card_auto(kind: CardKind, space: &str, name: &str) -> Card {
    let mut card = fixture_card(kind, space, name, "1.0.0");
    card.metadata.version = None;
    card.metadata.bump = None;
    card
}

pub fn fixture_card_scoped(kind: CardKind, space: &str, name: &str, scope: &str) -> Card {
    let mut card = fixture_card(kind, space, name, "1.0.0");
    card.metadata.version = Some(VersionSpec::parse(scope).expect("fixture scope"));
    card.metadata.bump = None;
    card
}

pub fn with_bump(mut card: Card, bump: VersionBump) -> Card {
    card.metadata.bump = Some(bump);
    card
}

pub fn with_labels(mut card: Card, pairs: &[(&str, &str)]) -> Card {
    for (key, value) in pairs {
        card.metadata.labels.insert(
            wyrd_spec::metadata::LabelKey::new(*key).expect("label key"),
            wyrd_spec::metadata::LabelValue::new(*value).expect("label value"),
        );
    }
    card
}

pub fn with_annotations(mut card: Card, pairs: &[(&str, &str)]) -> Card {
    for (key, value) in pairs {
        card.metadata.annotations.insert(
            wyrd_spec::metadata::AnnotationKey::new(*key).expect("annotation key"),
            wyrd_spec::metadata::AnnotationValue::new(*value).expect("annotation value"),
        );
    }
    card
}
