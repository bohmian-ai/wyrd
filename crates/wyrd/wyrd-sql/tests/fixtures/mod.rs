//! Shared fixture helpers for `cards_e2e.rs`.

pub mod asserts;
pub mod per_kind;
pub mod scenarios;

use std::env;

use sqlx::PgPool;
use sqlx::postgres::PgPoolOptions;
use url::Url;

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
        let app_url =
            env::var("WYRD_DATABASE_URL").expect("WYRD_DATABASE_URL must be set for e2e tests");
        let migrator_password = env::var("WYRD_DATABASE_MIGRATOR_PASSWORD")
            .expect("WYRD_DATABASE_MIGRATOR_PASSWORD must be set for e2e tests");
        let migrator_url = synth_role_url(&app_url, "wyrd_migrator", &migrator_password);

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
}

fn synth_role_url(base_url: &str, role: &str, password: &str) -> String {
    let mut url = Url::parse(base_url).expect("WYRD_DATABASE_URL is a valid URL");
    url.set_username(role).expect("role name is URL-safe");
    url.set_password(Some(password))
        .expect("password is URL-encodable");
    url.into()
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
