//! Postgres-backed proofs for tenant gateway administration.

use std::collections::BTreeMap;
use std::sync::Arc;

use axum::Json;
use axum::body::Body;
use axum::extract::FromRequest;
use axum::extract::{Path, State};
use axum::http::Request;
use axum::http::StatusCode;
use axum::response::IntoResponse;
use secrecy::ExposeSecret;
use serde_json::{Value, json};
use sqlx::postgres::{PgConnectOptions, PgPoolOptions};
use url::Url;
use uuid::Uuid;
use vala_sql::ValaPostgres;
use wyrd_crypt::SecretKey;
use wyrd_dev_fixtures::pg::PgFixture;
use wyrd_gateway::{
    CredentialAssignment, CredentialError, ManagedSecretBinding, ManagedSecretKeys, TenantKeyring,
};
use wyrd_runtime::{Permission, PermissionSet, Principal, PrincipalId, PrincipalKind, RoleRef};
use wyrd_spec::DataTenantId;
use wyrd_spec::auth::GatewayAccess;
use wyrd_spec::error::WyrdError;
use wyrd_spec::gateway::{
    GatewayCaptureMode, GatewayCapturePolicyWrite, GatewayFallbackPolicy, GatewayGovernancePolicy,
    GatewayPayloadField, ProviderCredentialState, ProviderCredentialWrite, ProviderDeployment,
};
use wyrd_spec::ids::{
    CredentialBindingName, ProviderCredentialName, ProviderDeploymentName, ProviderId,
    SecretBackendName,
};
use wyrd_spec::request_id::RequestId;
use wyrd_spec::security::SecretRef;
use wyrd_spec::vala::BifrostError;
use wyrd_sql::queries::gateway::{GatewayAccountingEntryWrite, append_gateway_accounting_entry};
use wyrd_storage::{BackendSigner, LocalSigner, StorageHandle};

use super::{GatewayAdministration, GatewayCredentialSource};
use crate::components::auth::Caller;
use crate::config::{GatewayConfig, GatewayCredentialBinding, VaultBackendConfig};
use crate::state::AppState;

/// Operator environment variable behind the `openai-env` binding; tenant
/// views must never disclose it.
const BINDING_VARIABLE: &str = "WYRD_TEST_OPENAI_KEY";

/// Builds server state with one environment binding and one external secret
/// backend.
pub(super) async fn test_state(fixture: &PgFixture) -> AppState {
    state_with_vala(fixture, fixture.vala_postgres().clone()).await
}

/// Builds the gateway test state over `vala`, the pool behind standalone audit
/// appends, so a test can make audit recovery unreachable.
pub(super) async fn state_with_vala(fixture: &PgFixture, vala: ValaPostgres) -> AppState {
    let postgres = Arc::new(crate::postgres::ServerPostgres::from_parts(
        fixture.wyrd_postgres().clone(),
        vala,
    ));
    let root = tempfile::tempdir()
        .expect("gateway storage tempdir")
        .keep()
        .join("gateway-storage");
    std::fs::create_dir_all(&root).expect("storage root creates");
    let signer = LocalSigner::new(root).expect("local signer creates");
    crate::test_support::test_app_state(
        postgres,
        Arc::new(StorageHandle::new(BackendSigner::Local(signer))),
        crate::test_support::test_catalog().await,
    )
    .with_gateway(gateway_config(fixture.data_tenant_id()))
}

/// Assigns a credential source to `tenant` and `provider` without a host.
pub(super) fn assigned(tenant: DataTenantId, provider: &str) -> CredentialAssignment {
    CredentialAssignment {
        tenant,
        provider: ProviderId::new(provider).expect("provider"),
        host: None,
    }
}

/// Operator gateway configuration for `tenant`: `openai-env` and
/// `anthropic-env` bindings plus a `vault` backend whose `openai` and
/// `anthropic` prefixes are assigned to the matching provider.
fn gateway_config(tenant: DataTenantId) -> GatewayConfig {
    let binding = |variable: &str, provider| GatewayCredentialBinding {
        secret: SecretRef::Env {
            name: variable.to_owned(),
        },
        assignment: assigned(tenant, provider),
    };
    GatewayConfig {
        credential_bindings: BTreeMap::from([
            (
                CredentialBindingName::new("openai-env").expect("binding name"),
                binding(BINDING_VARIABLE, "openai"),
            ),
            (
                CredentialBindingName::new("anthropic-env").expect("binding name"),
                binding("WYRD_TEST_ANTHROPIC_KEY", "anthropic"),
            ),
        ]),
        secret_backends: BTreeMap::from([(
            SecretBackendName::new("vault").expect("backend"),
            VaultBackendConfig {
                address: Url::parse("http://127.0.0.1:1").expect("address"),
                mount: "secret".to_owned(),
                token: SecretRef::Env {
                    name: "WYRD_TEST_VAULT_TOKEN".to_owned(),
                },
                namespace: None,
                ca_cert: None,
                paths: BTreeMap::from([
                    ("openai".to_owned(), assigned(tenant, "openai")),
                    ("anthropic".to_owned(), assigned(tenant, "anthropic")),
                ]),
            },
        )]),
        ..Default::default()
    }
}

/// Builds a caller in `tenant` holding exactly `permissions`.
pub(super) fn caller(
    tenant: DataTenantId,
    permissions: impl IntoIterator<Item = Permission>,
) -> Caller {
    Caller {
        data_tenant_id: tenant,
        principal: Principal::new(
            PrincipalId::new(Uuid::nil()),
            PrincipalKind::User,
            tenant,
            Vec::<RoleRef>::new(),
            PermissionSet::from_iter(permissions),
        ),
        request_id: RequestId::now_v7(),
        delegation_chain: Vec::new(),
    }
}

/// Builds a caller holding every gateway administration permission.
pub(super) fn admin(tenant: DataTenantId) -> Caller {
    caller(
        tenant,
        [
            Permission::gateway_read(),
            Permission::gateway_write(),
            Permission::gateway_delete(),
        ],
    )
}

/// Decodes a credential write from its wire JSON.
fn credential(name: &str, provider: &str, source: Value) -> ProviderCredentialWrite {
    serde_json::from_value(json!({"name": name, "provider": provider, "source": source}))
        .expect("credential write decodes")
}

/// Decodes a bearer-authenticated OpenAI deployment from its wire JSON.
fn deployment(name: &str, credential: &str) -> ProviderDeployment {
    serde_json::from_value(json!({
        "name": name,
        "model": {"provider": "openai", "model": "gpt-4o"},
        "adapter": "openai",
        "auth": {"bearer": {"credential": credential}},
        "capabilities": ["chat_completions"],
        "routing_weight": 1,
    }))
    .expect("deployment decodes")
}

/// Parses a credential name fixture.
fn credential_name(name: &str) -> ProviderCredentialName {
    ProviderCredentialName::new(name).expect("credential name")
}

/// Parses a deployment name fixture.
fn deployment_name(name: &str) -> ProviderDeploymentName {
    ProviderDeploymentName::new(name).expect("deployment name")
}

/// Asserts `error` is an invalid-configuration rejection for `field`.
fn assert_invalid(error: &WyrdError, field: &str) {
    match error {
        WyrdError::GatewayInvalidConfiguration { details, .. } => {
            assert_eq!(details["field"], field, "{error:?}");
        }
        other => panic!("expected invalid configuration for {field}, got {other:?}"),
    }
}

/// Reads `(operation, outcome)` audit decisions recorded for `tenant`.
pub(super) async fn audit_decisions(
    fixture: &PgFixture,
    tenant: DataTenantId,
) -> Vec<(String, String)> {
    let mut conn = fixture
        .vala_postgres()
        .tenant_conn(tenant)
        .await
        .expect("vala tenant conn");
    let rows = sqlx::query_as::<_, (String, String)>(
        "SELECT operation, outcome FROM vala.audit_staging \
         WHERE data_tenant_id = $1 AND operation LIKE 'gateway.%' ORDER BY seq",
    )
    .bind(tenant.as_uuid())
    .fetch_all(&mut **conn.transaction())
    .await
    .expect("audit rows");
    conn.commit().await.expect("audit read commits");
    rows
}

/// Proves create, idempotent rotation, terminal revoke, and redaction.
#[tokio::test]
async fn gateway_credential_lifecycle_rotates_revokes_and_redacts() {
    let fixture = PgFixture::start().await.expect("fixture starts");
    let state = test_state(&fixture).await;
    let tenant = fixture.data_tenant_id();
    let admin = admin(tenant);
    let gateway = GatewayAdministration::new(&state);
    let name = credential_name("openai-env");
    let write = || {
        credential(
            "openai-env",
            "openai",
            json!({"environment": {"binding": "openai-env"}}),
        )
    };

    let created = gateway
        .put_credential(&admin, &name, write())
        .await
        .expect("create");
    assert_eq!(created.state, ProviderCredentialState::Active);
    assert!(created.rotated_at.is_none());
    let rotated = gateway
        .put_credential(&admin, &name, write())
        .await
        .expect("rotate");
    assert!(rotated.rotated_at.is_some());
    assert_eq!(rotated.created_at, created.created_at);
    assert_eq!(gateway.credentials(&admin).await.expect("list").len(), 1);

    let rendered = serde_json::to_string(&rotated).expect("view encodes");
    assert!(
        !rendered.contains(BINDING_VARIABLE),
        "view leaks operator reference: {rendered}"
    );

    let revoked = gateway
        .revoke_credential(&admin, &name)
        .await
        .expect("revoke");
    assert_eq!(revoked.state, ProviderCredentialState::Revoked);
    let again = gateway
        .revoke_credential(&admin, &name)
        .await
        .expect("revoke is idempotent");
    assert_eq!(again.revoked_at, revoked.revoked_at);
    assert_eq!(again.updated_at, revoked.updated_at);
    assert!(matches!(
        gateway.put_credential(&admin, &name, write()).await,
        Err(WyrdError::GatewayResourceConflict { .. })
    ));
    assert!(matches!(
        gateway
            .revoke_credential(&admin, &credential_name("missing"))
            .await,
        Err(WyrdError::GatewayResourceNotFound { .. })
    ));
    assert!(matches!(
        gateway
            .credential(&admin, &credential_name("missing"))
            .await,
        Err(WyrdError::GatewayResourceNotFound { .. })
    ));
}

/// Proves source validation and the resolution inputs of both credential
/// sources.
#[tokio::test]
async fn gateway_credential_sources_resolve_from_operator_config() {
    let fixture = PgFixture::start().await.expect("fixture starts");
    let state = test_state(&fixture).await;
    let tenant = fixture.data_tenant_id();
    let admin = admin(tenant);
    let gateway = GatewayAdministration::new(&state);

    let unknown_binding = gateway
        .put_credential(
            &admin,
            &credential_name("env"),
            credential(
                "env",
                "openai",
                json!({"environment": {"binding": "other"}}),
            ),
        )
        .await
        .expect_err("undeclared binding");
    assert_invalid(&unknown_binding, "source");
    let unknown_backend = gateway
        .put_credential(
            &admin,
            &credential_name("ext"),
            credential(
                "ext",
                "openai",
                json!({"external_secret": {"backend": "aws", "reference": "openai#api_key"}}),
            ),
        )
        .await
        .expect_err("undeclared backend");
    assert_invalid(&unknown_backend, "source");
    let mismatch = gateway
        .put_credential(
            &admin,
            &credential_name("other"),
            credential(
                "env",
                "openai",
                json!({"environment": {"binding": "openai-env"}}),
            ),
        )
        .await
        .expect_err("path/body mismatch");
    assert_invalid(&mismatch, "name");

    for write in [
        credential(
            "env",
            "openai",
            json!({"environment": {"binding": "openai-env"}}),
        ),
        credential(
            "ext",
            "anthropic",
            json!({"external_secret": {"backend": "vault", "reference": "anthropic/team#api_key"}}),
        ),
    ] {
        let name = write.name.clone();
        gateway
            .put_credential(&admin, &name, write)
            .await
            .expect("valid source stores");
    }

    let snapshot = gateway.snapshot(tenant).await.expect("snapshot");
    let sources: Vec<_> = snapshot
        .credentials
        .iter()
        .map(|c| (c.name.as_str(), &c.source))
        .collect();
    assert_eq!(sources.len(), 2);
    assert!(matches!(
        sources[0],
        ("env", GatewayCredentialSource::Environment { secret: Some(SecretRef::Env { name }), .. })
            if name == BINDING_VARIABLE
    ));
    assert!(matches!(
        sources[1],
        ("ext", GatewayCredentialSource::ExternalSecret { backend, reference })
            if backend.as_str() == "vault" && reference.as_str() == "anthropic/team#api_key"
    ));
}

/// Builds one tenant keyring whose per-version material derives from `seed`,
/// so a test can hand a tenant another tenant's material to prove incorrect
/// key authority fails closed.
///
/// # Panics
///
/// Panics when `active` names no version in `versions`.
pub(super) fn keyring(seed: DataTenantId, active: &str, versions: &[&str]) -> TenantKeyring {
    let material = |version: &str| {
        let mut bytes = [version.as_bytes()[version.len() - 1]; 32];
        bytes[..16].copy_from_slice(seed.as_uuid().as_bytes());
        SecretKey::from_bytes(bytes)
    };
    TenantKeyring::new(
        active.to_owned(),
        versions
            .iter()
            .map(|version| ((*version).to_owned(), material(version)))
            .collect(),
    )
    .expect("keyring")
}

/// Builds the keyrings of `tenants`, each seeded by itself so no two tenants
/// share key material, as the boot-time isolation check requires.
///
/// # Panics
///
/// Panics when `active` names no version in `versions`.
pub(super) fn managed_keys(
    tenants: &[DataTenantId],
    active: &str,
    versions: &[&str],
) -> Arc<ManagedSecretKeys> {
    Arc::new(ManagedSecretKeys::new(
        tenants
            .iter()
            .map(|tenant| (*tenant, keyring(*tenant, active, versions)))
            .collect(),
    ))
}

/// Reads the persisted credential row of `name` as `(json, key_version,
/// nonce, ciphertext)`, where `json` is the whole row rendered as text so a
/// test can assert no column holds submitted plaintext.
///
/// # Panics
///
/// Panics when the tenant connection, the row query, or its commit fails.
async fn stored_credential(
    fixture: &PgFixture,
    tenant: DataTenantId,
    name: &str,
) -> (String, Option<String>, Option<Vec<u8>>, Option<Vec<u8>>) {
    let mut conn = fixture.tenant_conn_for(tenant).await.expect("tenant conn");
    let row = sqlx::query_as::<_, (String, Option<String>, Option<Vec<u8>>, Option<Vec<u8>>)>(
        "SELECT row_to_json(c)::text, c.secret_key_version, c.secret_nonce, c.secret_ciphertext \
         FROM wyrd.gateway_provider_credentials c WHERE c.name = $1",
    )
    .bind(name)
    .fetch_one(&mut **conn.transaction())
    .await
    .expect("credential row");
    conn.commit().await.expect("row read commits");
    row
}

/// Proves a managed submission persists only a sealed envelope, that the
/// envelope is bound to its own credential row, and that a rejected
/// replacement leaves the previously sealed value resolvable.
///
/// # Panics
///
/// Panics when the fixture or any administration call fails, when the view or
/// the durable row discloses the submitted plaintext, when the envelope is not
/// sealed under the active version, when a copied envelope opens, or when the
/// rejected replacement disturbs the stored envelope.
#[tokio::test]
async fn gateway_managed_secrets_persist_sealed_and_bound_envelopes() {
    const SUBMITTED: &str = "sk-live-managed-submission";
    let fixture = PgFixture::start().await.expect("fixture starts");
    let tenant = fixture.data_tenant_id();
    let state = test_state(&fixture)
        .await
        .with_gateway_secret_keys(managed_keys(&[tenant], "v1", &["v0", "v1"]));
    let admin = admin(tenant);
    let gateway = GatewayAdministration::new(&state);
    let managed = |name: &str, secret: &str| {
        credential(
            name,
            "openai",
            json!({"managed_secret": {"secret": secret}}),
        )
    };

    for name in ["managed", "copied"] {
        gateway
            .put_credential(&admin, &credential_name(name), managed(name, SUBMITTED))
            .await
            .expect("managed submission stores");
    }

    let view = gateway
        .credential(&admin, &credential_name("managed"))
        .await
        .expect("read back");
    assert_eq!(
        serde_json::to_value(&view.source).expect("source"),
        json!("managed_secret"),
        "the view carries no member a caller could read the secret from"
    );
    assert!(
        !format!("{view:?}").contains(SUBMITTED),
        "the view never renders the submitted value"
    );

    let (row, key_version, nonce, ciphertext) =
        stored_credential(&fixture, tenant, "managed").await;
    assert!(
        !row.contains(SUBMITTED),
        "no column of the durable row holds the submitted plaintext"
    );
    assert_eq!(
        key_version.as_deref(),
        Some("v1"),
        "sealed under the active version"
    );
    assert_eq!(nonce.as_deref().map(<[u8]>::len), Some(12));
    let ciphertext = ciphertext.expect("sealed ciphertext");

    // The envelope of `managed` copied onto another credential row stays shut.
    let mut conn = fixture.tenant_conn_for(tenant).await.expect("tenant conn");
    sqlx::query(
        "UPDATE wyrd.gateway_provider_credentials SET secret_key_version = $1, \
         secret_nonce = $2, secret_ciphertext = $3 WHERE name = 'copied'",
    )
    .bind(key_version.as_deref())
    .bind(nonce.as_deref())
    .bind(ciphertext.as_slice())
    .execute(&mut **conn.transaction())
    .await
    .expect("envelope copied");
    conn.commit().await.expect("copy commits");

    let snapshot = gateway.snapshot(tenant).await.expect("snapshot");
    let open = |name: &str| {
        let credential = snapshot
            .credential(&credential_name(name))
            .expect("admitted");
        let GatewayCredentialSource::ManagedSecret { envelope } = &credential.source else {
            panic!("managed source");
        };
        state.gateway_secret_keys.open(
            ManagedSecretBinding {
                tenant,
                name: &credential.name,
                provider: &credential.provider,
            },
            envelope,
        )
    };
    assert_eq!(
        open("managed").expect("own envelope opens").expose_secret(),
        SUBMITTED
    );
    assert!(
        matches!(open("copied"), Err(CredentialError::Unavailable)),
        "an envelope copied onto another credential name never opens"
    );

    // A rejected replacement leaves the previously sealed value in place.
    let rejected = gateway
        .put_credential(
            &admin,
            &credential_name("managed"),
            managed("managed", "  "),
        )
        .await
        .expect_err("blank secret");
    assert_invalid(&rejected, "source.secret");
    let (_, replaced_version, _, replaced) = stored_credential(&fixture, tenant, "managed").await;
    assert_eq!(replaced_version, key_version);
    assert_eq!(replaced, Some(ciphertext));
    assert_eq!(
        open("managed").expect("still opens").expose_secret(),
        SUBMITTED
    );
}

/// Returns the details of an invalid-configuration rejection.
///
/// # Panics
///
/// Panics when `error` is not `WyrdError::GatewayInvalidConfiguration`.
fn invalid_details(error: WyrdError) -> Value {
    match error {
        WyrdError::GatewayInvalidConfiguration { details, .. } => details,
        other => panic!("expected invalid configuration, got {other:?}"),
    }
}

/// Proves operator assignments confine credential sources to one tenant,
/// provider, Vault path prefix, and compatible endpoint host: another
/// tenant's binding or prefix is rejected exactly like an unknown one, and the
/// admission snapshot carries the assignment the resolver re-checks.
#[tokio::test]
async fn gateway_credential_sources_stay_within_operator_assignments() {
    let fixture = PgFixture::start().await.expect("fixture starts");
    let tenant = fixture.data_tenant_id();
    let other = DataTenantId::new_v7();
    let mut config = gateway_config(tenant);
    config.credential_bindings.extend([
        (
            CredentialBindingName::new("other-env").expect("binding name"),
            GatewayCredentialBinding {
                secret: SecretRef::Env {
                    name: "WYRD_TEST_OTHER_KEY".to_owned(),
                },
                assignment: assigned(other, "openai"),
            },
        ),
        (
            CredentialBindingName::new("deepseek-env").expect("binding name"),
            GatewayCredentialBinding {
                secret: SecretRef::Env {
                    name: "WYRD_TEST_DEEPSEEK_KEY".to_owned(),
                },
                assignment: CredentialAssignment {
                    host: Some("api.deepseek.example".to_owned()),
                    ..assigned(tenant, "deepseek")
                },
            },
        ),
    ]);
    config
        .secret_backends
        .get_mut(&SecretBackendName::new("vault").expect("backend"))
        .expect("vault backend")
        .paths
        .insert("other".to_owned(), assigned(other, "openai"));
    let state = test_state(&fixture).await.with_gateway(config);
    let admin = admin(tenant);
    let gateway = GatewayAdministration::new(&state);
    let put = |name: &str, provider: &str, source: Value| {
        let write = credential(name, provider, source);
        let gateway = GatewayAdministration::new(&state);
        let admin = admin.clone();
        async move {
            gateway
                .put_credential(&admin, &write.name.clone(), write)
                .await
        }
    };
    let env = |binding: &str| json!({"environment": {"binding": binding}});
    let vault =
        |reference: &str| json!({"external_secret": {"backend": "vault", "reference": reference}});

    let unknown = invalid_details(
        put("key", "openai", env("absent-env"))
            .await
            .expect_err("unknown"),
    );
    assert_eq!(unknown["field"], "source", "{unknown}");
    for (provider, source) in [
        ("openai", env("other-env")),
        ("anthropic", env("openai-env")),
        ("openai", vault("absent/team#api_key")),
        ("openai", vault("other/team#api_key")),
        ("anthropic", vault("openai/team#api_key")),
        ("openai", vault("openai-team/key#api_key")),
        (
            "openai",
            json!({"external_secret": {"backend": "absent", "reference": "openai#api_key"}}),
        ),
    ] {
        let rejected = put("key", provider, source.clone())
            .await
            .expect_err("unassigned source");
        assert_eq!(invalid_details(rejected), unknown, "{provider} {source}");
    }
    put("key", "openai", vault("openai/team#api_key"))
        .await
        .expect("assigned prefix stores");
    put("deepseek-key", "deepseek", env("deepseek-env"))
        .await
        .expect("assigned binding stores");

    let compatible = |host: &str| -> ProviderDeployment {
        serde_json::from_value(json!({
            "name": "deepseek-chat",
            "model": {"provider": "deepseek", "model": "deepseek-chat"},
            "adapter": {"openai_compatible": {"base_url": format!("https://{host}/v1")}},
            "auth": {"bearer": {"credential": "deepseek-key"}},
            "capabilities": ["chat_completions"],
            "routing_weight": 1,
        }))
        .expect("deployment decodes")
    };
    let name = deployment_name("deepseek-chat");
    let wrong_host = gateway
        .put_deployment(&admin, &name, compatible("evil.example"))
        .await
        .expect_err("unassigned host");
    assert_invalid(&wrong_host, "auth.credential");
    gateway
        .put_deployment(&admin, &name, compatible("api.deepseek.example"))
        .await
        .expect("assigned host stores");

    let snapshot = gateway.snapshot(tenant).await.expect("snapshot");
    let deepseek = snapshot
        .credentials
        .iter()
        .find(|c| c.name.as_str() == "deepseek-key")
        .expect("deepseek credential");
    assert_eq!(
        deepseek.assignment.as_ref().and_then(|a| a.host.as_deref()),
        Some("api.deepseek.example")
    );
}

/// Proves deployment credential checks, reference-guarded delete, idempotent
/// deletes, and admission-snapshot immutability.
#[tokio::test]
async fn gateway_deployments_guard_credential_references() {
    let fixture = PgFixture::start().await.expect("fixture starts");
    let state = test_state(&fixture).await;
    let tenant = fixture.data_tenant_id();
    let admin = admin(tenant);
    let gateway = GatewayAdministration::new(&state);
    let key = credential_name("openai-key");
    gateway
        .put_credential(
            &admin,
            &key,
            credential(
                "openai-key",
                "openai",
                json!({"environment": {"binding": "openai-env"}}),
            ),
        )
        .await
        .expect("credential");
    gateway
        .put_credential(
            &admin,
            &credential_name("claude-key"),
            credential(
                "claude-key",
                "anthropic",
                json!({"environment": {"binding": "anthropic-env"}}),
            ),
        )
        .await
        .expect("other provider credential");

    let missing = gateway
        .put_deployment(
            &admin,
            &deployment_name("primary"),
            deployment("primary", "nope"),
        )
        .await
        .expect_err("missing credential");
    assert_invalid(&missing, "auth.credential");
    let wrong_provider = gateway
        .put_deployment(
            &admin,
            &deployment_name("primary"),
            deployment("primary", "claude-key"),
        )
        .await
        .expect_err("provider mismatch");
    assert_invalid(&wrong_provider, "auth.credential");

    let name = deployment_name("primary");
    let stored = gateway
        .put_deployment(&admin, &name, deployment("primary", "openai-key"))
        .await
        .expect("deployment");
    gateway
        .put_deployment(&admin, &name, deployment("primary", "openai-key"))
        .await
        .expect("replace");
    assert_eq!(
        gateway.deployments(&admin).await.expect("list"),
        vec![stored.clone()]
    );
    let admitted = gateway.snapshot(tenant).await.expect("snapshot");

    assert!(matches!(
        gateway.delete_credential(&admin, &key).await,
        Err(WyrdError::GatewayResourceConflict { .. })
    ));
    let status = super::routes::delete_deployment(
        State(state.clone()),
        admin.clone(),
        Path("primary".to_owned()),
    )
    .await
    .expect("delete");
    assert_eq!(status, StatusCode::NO_CONTENT);
    gateway
        .delete_deployment(&admin, &name)
        .await
        .expect("idempotent delete");
    assert_eq!(
        admitted.deployments,
        vec![stored],
        "admitted snapshot is immutable"
    );
    gateway
        .delete_credential(&admin, &key)
        .await
        .expect("unreferenced delete");
    gateway
        .delete_credential(&admin, &key)
        .await
        .expect("idempotent delete");

    gateway
        .put_credential(
            &admin,
            &key,
            credential(
                "openai-key",
                "openai",
                json!({"environment": {"binding": "openai-env"}}),
            ),
        )
        .await
        .expect("recreate");
    gateway
        .revoke_credential(&admin, &key)
        .await
        .expect("revoke");
    let revoked = gateway
        .put_deployment(&admin, &name, deployment("primary", "openai-key"))
        .await
        .expect_err("revoked credential");
    assert_invalid(&revoked, "auth.credential");
    assert!(matches!(
        gateway.deployment(&admin, &name).await,
        Err(WyrdError::GatewayResourceNotFound { .. })
    ));
}

/// Proves RBAC per operation class, decision audit, and tenant isolation.
#[tokio::test]
async fn gateway_administration_is_authorized_audited_and_tenant_isolated() {
    let fixture = PgFixture::start().await.expect("fixture starts");
    let state = test_state(&fixture).await;
    let tenant = fixture.data_tenant_id();
    let other = fixture
        .seed_additional_tenant("gateway-other")
        .await
        .expect("tenant b");
    let gateway = GatewayAdministration::new(&state);
    let key = credential_name("openai-key");
    let write = || {
        credential(
            "openai-key",
            "openai",
            json!({"environment": {"binding": "openai-env"}}),
        )
    };

    let invoker = caller(
        tenant,
        [Permission::gateway_invoke(GatewayAccess::Provider {
            provider: "openai".parse().expect("provider"),
        })],
    );
    assert!(
        gateway.credentials(&invoker).await.is_err(),
        "invoke-only cannot list"
    );
    assert!(gateway.deployments(&invoker).await.is_err());
    let reader = caller(tenant, [Permission::gateway_read()]);
    assert!(
        gateway
            .put_credential(&reader, &key, write())
            .await
            .is_err()
    );
    let writer = caller(
        tenant,
        [Permission::gateway_read(), Permission::gateway_write()],
    );
    gateway
        .put_credential(&writer, &key, write())
        .await
        .expect("writer puts");
    assert!(
        gateway.delete_credential(&writer, &key).await.is_err(),
        "delete needs gateway:delete"
    );

    let other_admin = admin(other);
    assert!(
        gateway
            .credentials(&other_admin)
            .await
            .expect("other list")
            .is_empty()
    );
    assert!(matches!(
        gateway.credential(&other_admin, &key).await,
        Err(WyrdError::GatewayResourceNotFound { .. })
    ));
    gateway
        .delete_credential(&other_admin, &key)
        .await
        .expect("no-op in other tenant");
    assert_eq!(
        gateway
            .credentials(&reader)
            .await
            .expect("still present")
            .len(),
        1
    );

    let decisions = audit_decisions(&fixture, tenant).await;
    let expected: Vec<(String, String)> = [
        ("gateway.provider_credential.list", "denied"),
        ("gateway.provider_deployment.list", "denied"),
        ("gateway.provider_credential.put", "denied"),
        ("gateway.provider_credential.put", "allowed"),
        ("gateway.provider_credential.delete", "denied"),
        ("gateway.provider_credential.list", "allowed"),
    ]
    .into_iter()
    .map(|(op, outcome)| (op.to_owned(), outcome.to_owned()))
    .collect();
    assert_eq!(decisions, expected);
}

/// Proves fallback defaults, replacement, and idempotent delete.
#[tokio::test]
async fn gateway_fallback_policy_replaces_and_resets() {
    let fixture = PgFixture::start().await.expect("fixture starts");
    let state = test_state(&fixture).await;
    let admin = admin(fixture.data_tenant_id());
    let gateway = GatewayAdministration::new(&state);
    assert_eq!(
        gateway.fallback(&admin).await.expect("default"),
        GatewayFallbackPolicy::default()
    );
    let policy: GatewayFallbackPolicy = serde_json::from_value(json!({"rules": [{
        "scope": "global",
        "candidates": [{"provider": "openai", "model": "gpt-4o"}, {"provider": "anthropic", "model": "claude"}],
    }]}))
    .expect("policy decodes");
    gateway
        .put_fallback(&admin, policy.clone())
        .await
        .expect("put");
    assert_eq!(gateway.fallback(&admin).await.expect("get"), policy);
    gateway.delete_fallback(&admin).await.expect("delete");
    gateway.delete_fallback(&admin).await.expect("idempotent");
    assert_eq!(
        gateway.fallback(&admin).await.expect("reset"),
        GatewayFallbackPolicy::default()
    );
}

/// Proves governance validation, pricing immutability, and retention of
/// pricing referenced by accounting history across replace and delete.
#[tokio::test]
async fn gateway_governance_retains_referenced_pricing() {
    let fixture = PgFixture::start().await.expect("fixture starts");
    let state = test_state(&fixture).await;
    let tenant = fixture.data_tenant_id();
    let admin = admin(tenant);
    let gateway = GatewayAdministration::new(&state);
    let pricing = |version: &str, at: &str, price: &str| {
        json!({
            "model": {"provider": "openai", "model": "gpt-4o"}, "version": version, "currency": "USD",
            "effective_at": at, "active": true,
            "rates": [{"dimension": "input_tokens", "unit": "1m_tokens", "price": price}],
        })
    };
    let policy = |pricing: Vec<Value>| -> GatewayGovernancePolicy {
        serde_json::from_value(json!({
            "limits": [{"subject": "tenant", "target": "all", "requests_per_minute": 60, "tokens_per_minute": null, "concurrent_calls": null}],
            "budgets": [{"subject": "tenant", "period": "calendar_month_utc", "amount": "100", "currency": "USD"}],
            "pricing": pricing,
            "unknown_cost": "reject",
        }))
        .expect("governance decodes")
    };

    assert_eq!(
        gateway.governance(&admin).await.expect("default"),
        GatewayGovernancePolicy::default()
    );
    let duplicate = gateway
        .put_governance(
            &admin,
            policy(vec![
                pricing("v1", "2026-01-01T00:00:00Z", "2.50"),
                pricing("v1", "2026-02-01T00:00:00Z", "2.50"),
            ]),
        )
        .await
        .expect_err("duplicate version");
    assert!(matches!(
        duplicate,
        WyrdError::GatewayInvalidConfiguration { .. }
    ));

    let stored = gateway
        .put_governance(
            &admin,
            policy(vec![
                pricing("v1", "2026-01-01T00:00:00Z", "2.50"),
                pricing("v2", "2026-03-01T00:00:00Z", "3.00"),
            ]),
        )
        .await
        .expect("put");
    assert_eq!(stored.pricing.len(), 2);
    let changed = gateway
        .put_governance(
            &admin,
            policy(vec![pricing("v1", "2026-01-01T00:00:00Z", "9.99")]),
        )
        .await
        .expect_err("pricing version is immutable");
    assert_invalid(&changed, "pricing[0]");

    let mut conn = fixture.tenant_conn_for(tenant).await.expect("tenant conn");
    append_gateway_accounting_entry(
        &mut conn,
        GatewayAccountingEntryWrite {
            entry_id: Uuid::now_v7(),
            call_id: Uuid::now_v7(),
            kind: "attempt_accounted",
            provider: Some("openai"),
            model: Some("gpt-4o"),
            pricing_version: Some("v1"),
            entry: &json!({}),
        },
    )
    .await
    .expect("ledger entry");
    conn.commit().await.expect("ledger commits");

    let replaced = gateway
        .put_governance(
            &admin,
            policy(vec![pricing("v3", "2026-04-01T00:00:00Z", "4.00")]),
        )
        .await
        .expect("replace");
    let versions: Vec<(&str, bool)> = replaced
        .pricing
        .iter()
        .map(|p| (p.version.as_str(), p.active))
        .collect();
    assert_eq!(
        versions,
        vec![("v1", false), ("v2", false), ("v3", true)],
        "omitted versions are retained inactive"
    );
    let mut conn = fixture.tenant_conn_for(tenant).await.expect("tenant conn");
    append_gateway_accounting_entry(
        &mut conn,
        GatewayAccountingEntryWrite {
            entry_id: Uuid::now_v7(),
            call_id: Uuid::now_v7(),
            kind: "attempt_accounted",
            provider: Some("openai"),
            model: Some("gpt-4o"),
            pricing_version: Some("v2"),
            entry: &json!({}),
        },
    )
    .await
    .expect("a call admitted under unreferenced v2 still accounts after replacement");
    conn.commit().await.expect("late ledger commits");

    gateway.delete_governance(&admin).await.expect("delete");
    gateway.delete_governance(&admin).await.expect("idempotent");
    let reset = gateway.governance(&admin).await.expect("reset");
    assert!(reset.limits.is_empty() && reset.budgets.is_empty());
    assert_eq!(
        reset.unknown_cost,
        GatewayGovernancePolicy::default().unknown_cost
    );
    let versions: Vec<(&str, bool)> = reset
        .pricing
        .iter()
        .map(|p| (p.version.as_str(), p.active))
        .collect();
    assert_eq!(versions, vec![("v1", false), ("v2", false), ("v3", false)]);
}

/// Proves capture validation and content-driven versioning.
#[tokio::test]
async fn gateway_capture_policy_versions_only_on_change() {
    let fixture = PgFixture::start().await.expect("fixture starts");
    let state = test_state(&fixture).await;
    let admin = admin(fixture.data_tenant_id());
    let gateway = GatewayAdministration::new(&state);
    let write = |mode, fields: &[GatewayPayloadField]| GatewayCapturePolicyWrite {
        mode,
        payload_fields: fields.iter().copied().collect(),
    };

    let initial = gateway.capture(&admin).await.expect("default");
    assert_eq!(
        (initial.mode, initial.version.get()),
        (GatewayCaptureMode::Disabled, 1)
    );
    let same = gateway
        .put_capture(&admin, write(GatewayCaptureMode::Disabled, &[]))
        .await
        .expect("unchanged");
    assert_eq!(same.version.get(), 1);
    assert!(
        gateway
            .put_capture(&admin, write(GatewayCaptureMode::Payload, &[]))
            .await
            .is_err()
    );
    assert!(
        gateway
            .put_capture(
                &admin,
                write(
                    GatewayCaptureMode::Metadata,
                    &[GatewayPayloadField::Request]
                )
            )
            .await
            .is_err()
    );
    let payload = gateway
        .put_capture(
            &admin,
            write(GatewayCaptureMode::Payload, &[GatewayPayloadField::Request]),
        )
        .await
        .expect("payload");
    assert_eq!(payload.version.get(), 2);
    let repeat = gateway
        .put_capture(
            &admin,
            write(GatewayCaptureMode::Payload, &[GatewayPayloadField::Request]),
        )
        .await
        .expect("repeat");
    assert_eq!(repeat.version.get(), 2);
    let Json(read) = super::routes::get_capture(State(state.clone()), admin.clone())
        .await
        .expect("handler read");
    assert_eq!(read, repeat);
}

/// Proves every authorized outcome — success, validation, not-found, and
/// conflict — records exactly one allowed decision, and that an unreachable
/// standalone audit append refuses a failed operation.
#[tokio::test]
async fn gateway_failed_operations_keep_one_allowed_decision_and_fail_closed() {
    let fixture = PgFixture::start().await.expect("fixture starts");
    let state = test_state(&fixture).await;
    let tenant = fixture.data_tenant_id();
    let admin = admin(tenant);
    let gateway = GatewayAdministration::new(&state);
    let key = credential_name("openai-key");

    gateway
        .put_credential(
            &admin,
            &key,
            credential(
                "openai-key",
                "openai",
                json!({"environment": {"binding": "openai-env"}}),
            ),
        )
        .await
        .expect("put");
    gateway
        .put_deployment(
            &admin,
            &deployment_name("primary"),
            deployment("primary", "openai-key"),
        )
        .await
        .expect("deployment");
    let undeclared = gateway
        .put_credential(
            &admin,
            &key,
            credential(
                "openai-key",
                "openai",
                json!({"environment": {"binding": "undeclared"}}),
            ),
        )
        .await
        .expect_err("validation");
    assert!(matches!(
        undeclared,
        WyrdError::GatewayInvalidConfiguration { .. }
    ));
    assert!(matches!(
        gateway
            .revoke_credential(&admin, &credential_name("absent"))
            .await,
        Err(WyrdError::GatewayResourceNotFound { .. })
    ));
    assert!(matches!(
        gateway.delete_credential(&admin, &key).await,
        Err(WyrdError::GatewayResourceConflict { .. })
    ));

    let expected: Vec<(String, String)> = [
        ("gateway.provider_credential.put", "allowed"),
        ("gateway.provider_deployment.put", "allowed"),
        ("gateway.provider_credential.put", "allowed"),
        ("gateway.provider_credential.revoke", "allowed"),
        ("gateway.provider_credential.delete", "allowed"),
    ]
    .into_iter()
    .map(|(op, outcome)| (op.to_owned(), outcome.to_owned()))
    .collect();
    assert_eq!(audit_decisions(&fixture, tenant).await, expected);

    let unreachable = PgPoolOptions::new()
        .acquire_timeout(std::time::Duration::from_secs(1))
        .connect_lazy_with(PgConnectOptions::new().host("127.0.0.1").port(1));
    let broken = state_with_vala(&fixture, ValaPostgres::from_pool(unreachable)).await;
    let refused = GatewayAdministration::new(&broken)
        .revoke_credential(&admin, &credential_name("absent"))
        .await
        .expect_err("audit recovery fails");
    assert!(
        matches!(
            refused,
            WyrdError::Vala {
                error: BifrostError::AuditUnavailable { .. }
            }
        ),
        "{refused:?}"
    );
    assert_eq!(
        audit_decisions(&fixture, tenant).await,
        expected,
        "no decision survives an unrecorded failure"
    );
}

/// Proves policy resets are writes: write-only principals reset both policies
/// and delete-only principals are refused.
#[tokio::test]
async fn gateway_policy_resets_require_write() {
    let fixture = PgFixture::start().await.expect("fixture starts");
    let state = test_state(&fixture).await;
    let tenant = fixture.data_tenant_id();
    let gateway = GatewayAdministration::new(&state);
    let writer = caller(tenant, [Permission::gateway_write()]);
    let deleter = caller(tenant, [Permission::gateway_delete()]);

    gateway
        .delete_fallback(&writer)
        .await
        .expect("writer resets fallback");
    gateway
        .delete_governance(&writer)
        .await
        .expect("writer resets governance");
    assert!(gateway.delete_fallback(&deleter).await.is_err());
    assert!(gateway.delete_governance(&deleter).await.is_err());
}

/// Proves malformed, shape-invalid, and untyped gateway bodies become the
/// stable invalid-configuration problem instead of Axum's plain rejection.
#[tokio::test]
async fn gateway_body_rejections_render_wyrd_problems() {
    let fixture = PgFixture::start().await.expect("fixture starts");
    let state = test_state(&fixture).await;
    let admin = admin(fixture.data_tenant_id());
    for (content_type, body) in [
        ("application/json", "{not json"),
        ("application/json", r#"{"rules": "wrong"}"#),
        ("text/plain", r#"{"rules": []}"#),
    ] {
        let request = Request::put("/v1/admin/gateway/fallback-policy")
            .header("content-type", content_type)
            .body(Body::from(body))
            .expect("request builds");
        let extracted = Json::<GatewayFallbackPolicy>::from_request(request, &()).await;
        assert!(extracted.is_err(), "{body} must be rejected by extraction");
        let response = super::routes::put_fallback(State(state.clone()), admin.clone(), extracted)
            .await
            .expect_err("rejected body")
            .into_response();
        assert_eq!(response.status(), StatusCode::BAD_REQUEST);
        assert_eq!(
            response.headers()["content-type"],
            "application/problem+json"
        );
        let bytes = axum::body::to_bytes(response.into_body(), usize::MAX)
            .await
            .expect("problem body");
        let problem: Value = serde_json::from_slice(&bytes).expect("problem json");
        assert_eq!(problem["code"], "WYRD_GATEWAY_400_INVALID_CONFIGURATION");
        assert_eq!(problem["details"]["field"], "body");
    }
}

/// Waits until `waiters` lock requests in `fixture`'s database are blocked on a
/// heavyweight lock, the barrier that fixes the order concurrent writers
/// acquire a held row.
///
/// # Panics
///
/// Panics when `pg_locks` cannot be read or the barrier is not reached
/// within ten seconds.
pub(super) async fn await_lock_waiters(fixture: &PgFixture, waiters: i64) {
    let superuser = fixture.superuser_pool().await.expect("superuser pool");
    tokio::time::timeout(std::time::Duration::from_secs(10), async {
        loop {
            let blocked: i64 = sqlx::query_scalar(
                "SELECT count(*) FROM pg_locks l JOIN pg_stat_activity a USING (pid) \
                 WHERE NOT l.granted AND a.datname = current_database()",
            )
            .fetch_one(&superuser)
            .await
            .expect("lock waiters counted");
            if blocked >= waiters {
                return;
            }
            tokio::task::yield_now().await;
        }
    })
    .await
    .expect("writers reach the lock barrier");
}

/// Proves last-committed-write-wins for one credential and one deployment and
/// that concurrent fallback and governance updates both survive the shared
/// policy row. A held row lock queues the writers in a known order.
#[tokio::test]
async fn gateway_concurrent_replacements_are_last_committed_and_family_independent() {
    let fixture = PgFixture::start().await.expect("fixture starts");
    let state = test_state(&fixture).await;
    let tenant = fixture.data_tenant_id();
    let admin = admin(tenant);
    let gateway = GatewayAdministration::new(&state);
    let key = credential_name("shared");
    let env = || {
        credential(
            "shared",
            "openai",
            json!({"environment": {"binding": "openai-env"}}),
        )
    };
    let external = || {
        credential(
            "shared",
            "openai",
            json!({"external_secret": {"backend": "vault", "reference": "openai#api_key"}}),
        )
    };
    gateway
        .put_credential(&admin, &key, env())
        .await
        .expect("seed");
    gateway
        .put_deployment(
            &admin,
            &deployment_name("primary"),
            deployment("primary", "shared"),
        )
        .await
        .expect("seed deployment");
    gateway
        .delete_fallback(&admin)
        .await
        .expect("seed policy row");

    // Credential: the env write queues first, the external write second.
    let mut holder = fixture.tenant_conn_for(tenant).await.expect("holder");
    sqlx::query("SELECT 1 FROM wyrd.gateway_provider_credentials WHERE name = 'shared' FOR UPDATE")
        .execute(&mut **holder.transaction())
        .await
        .expect("credential row held");
    let first = tokio::spawn({
        let (state, admin, key, write) = (state.clone(), admin.clone(), key.clone(), env());
        async move {
            GatewayAdministration::new(&state)
                .put_credential(&admin, &key, write)
                .await
        }
    });
    await_lock_waiters(&fixture, 1).await;
    let second = tokio::spawn({
        let (state, admin, key, write) = (state.clone(), admin.clone(), key.clone(), external());
        async move {
            GatewayAdministration::new(&state)
                .put_credential(&admin, &key, write)
                .await
        }
    });
    await_lock_waiters(&fixture, 2).await;
    holder.commit().await.expect("release credential");
    first.await.expect("first joins").expect("first commits");
    let last = second.await.expect("second joins").expect("second commits");
    assert_eq!(gateway.credential(&admin, &key).await.expect("read"), last);
    assert_eq!(
        serde_json::to_value(&last.source).expect("source")["external_secret"]["backend"],
        "vault"
    );

    // Deployment: weight 2 queues first, weight 3 commits last.
    let weighted = |weight: u32| {
        let mut value = serde_json::to_value(deployment("primary", "shared")).expect("encode");
        value["routing_weight"] = json!(weight);
        serde_json::from_value::<ProviderDeployment>(value).expect("decode")
    };
    let mut holder = fixture.tenant_conn_for(tenant).await.expect("holder");
    sqlx::query(
        "SELECT 1 FROM wyrd.gateway_provider_deployments WHERE name = 'primary' FOR UPDATE",
    )
    .execute(&mut **holder.transaction())
    .await
    .expect("deployment row held");
    let name = deployment_name("primary");
    let first = tokio::spawn({
        let (state, admin, name, write) = (state.clone(), admin.clone(), name.clone(), weighted(2));
        async move {
            GatewayAdministration::new(&state)
                .put_deployment(&admin, &name, write)
                .await
        }
    });
    await_lock_waiters(&fixture, 1).await;
    let second = tokio::spawn({
        let (state, admin, name, write) = (state.clone(), admin.clone(), name.clone(), weighted(3));
        async move {
            GatewayAdministration::new(&state)
                .put_deployment(&admin, &name, write)
                .await
        }
    });
    await_lock_waiters(&fixture, 2).await;
    holder.commit().await.expect("release deployment");
    first.await.expect("first joins").expect("first commits");
    second.await.expect("second joins").expect("second commits");
    assert_eq!(
        gateway.deployment(&admin, &name).await.expect("read"),
        weighted(3)
    );

    // Policy families: fallback and governance wait on the same row; both land.
    let fallback: GatewayFallbackPolicy = serde_json::from_value(json!({"rules": [{
        "scope": "global",
        "candidates": [{"provider": "openai", "model": "gpt-4o"}, {"provider": "anthropic", "model": "claude"}],
    }]}))
    .expect("fallback decodes");
    let governance: GatewayGovernancePolicy = serde_json::from_value(json!({
        "limits": [{"subject": "tenant", "target": "all", "requests_per_minute": 60, "tokens_per_minute": null, "concurrent_calls": null}],
        "budgets": [],
        "pricing": [],
        "unknown_cost": "reject",
    }))
    .expect("governance decodes");
    let mut holder = fixture.tenant_conn_for(tenant).await.expect("holder");
    sqlx::query("SELECT 1 FROM wyrd.gateway_policies FOR UPDATE")
        .execute(&mut **holder.transaction())
        .await
        .expect("policy row held");
    let first = tokio::spawn({
        let (state, admin, policy) = (state.clone(), admin.clone(), fallback.clone());
        async move {
            GatewayAdministration::new(&state)
                .put_fallback(&admin, policy)
                .await
        }
    });
    await_lock_waiters(&fixture, 1).await;
    let second = tokio::spawn({
        let (state, admin, policy) = (state.clone(), admin.clone(), governance.clone());
        async move {
            GatewayAdministration::new(&state)
                .put_governance(&admin, policy)
                .await
        }
    });
    await_lock_waiters(&fixture, 2).await;
    holder.commit().await.expect("release policies");
    first
        .await
        .expect("fallback joins")
        .expect("fallback commits");
    second
        .await
        .expect("governance joins")
        .expect("governance commits");
    assert_eq!(gateway.fallback(&admin).await.expect("fallback"), fallback);
    assert_eq!(
        gateway.governance(&admin).await.expect("governance"),
        governance
    );
}
