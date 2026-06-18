//! Shared fixture helpers for `cards_e2e.rs`.

pub mod asserts;
pub mod per_kind;
pub mod scenarios;

use std::env;

use sqlx::PgPool;
use sqlx::postgres::PgPoolOptions;

use wyrd_runtime::permission::PermissionSet;
use wyrd_runtime::principal::{Principal, PrincipalId, PrincipalKind};
use wyrd_semver::{VersionBlock, VersionSpec};
use wyrd_spec::api_version::ApiVersion;
use wyrd_spec::envelope::{Card, CardKind};
use wyrd_spec::ids::{CardName, DataTenantId, SpaceName};
use wyrd_sql::TenantConn;

pub struct TestEnv {
    pub pool: PgPool,
    pub app_pool: PgPool,
}

impl TestEnv {
    pub async fn new() -> Self {
        let migrator_url = env::var("WYRD_TEST_DATABASE_URL")
            .or_else(|_| env::var("WYRD_DATABASE_URL_MIGRATOR"))
            .expect("WYRD_TEST_DATABASE_URL must be set for e2e tests");
        let app_url = env::var("WYRD_TEST_DATABASE_URL_APP").unwrap_or_else(|_| migrator_url.clone());

        let pool = PgPoolOptions::new()
            .max_connections(5)
            .connect(&migrator_url)
            .await
            .expect("failed to connect to test database (migrator)");
        let app_pool = PgPoolOptions::new()
            .max_connections(5)
            .connect(&app_url)
            .await
            .expect("failed to connect to test database (app)");
        Self { pool, app_pool }
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
}

pub fn fixture_principal_user(tenant: DataTenantId) -> Principal {
    Principal::new(
        PrincipalId::new(uuid::Uuid::now_v7()),
        PrincipalKind::User,
        tenant,
        vec![],
        PermissionSet::new(),
    )
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
        },
        spec,
        relationships: Default::default(),
        status: None,
    }
}
