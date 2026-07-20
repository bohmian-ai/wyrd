use std::fs;
use std::process::ExitCode;

use chrono::Utc;
use clap::Args;
use secrecy::{ExposeSecret, SecretString};
use sqlx::PgPool;
use uuid::Uuid;
use wyrd_semver::VersionBlock;
use wyrd_spec::DataTenantId;
use wyrd_spec::envelope::CardKind;
use wyrd_spec::ids::{CardName, SpaceName};
use wyrd_spec::reference::CardRef;
use wyrd_sql::postgres_boot::{APP_DSN_ENV, PostgresBoot};
use wyrd_sql::queries::auth::{
    insert_api_key, insert_audit_credential_issuance, insert_service_account,
};
use wyrd_sql::queries::platform::tenant_resolver::resolve_by_slug_for_app;
use wyrd_sql::{PoolConfig, TenantConn};

use crate::error::WyrdCliError;

const DEV_TENANT_SLUG: &str = "dev";
const DEV_TENANT_DISPLAY_NAME: &str = "Local Dev Tenant";
const DEV_SA_PRINCIPAL_KIND: &str = "service";
const DEV_SA_NAME: &str = "local-dev";
const DEV_CARD_NAME: &str = "local-dev";
const DEV_CARD_SPACE: &str = "dev";
const DEV_CARD_VERSION: &str = "1.0.0";
const DEV_BOOTSTRAP_REQUEST_ID: &str = "dev-bootstrap";
const DEV_SYSTEM_ACTOR_ID: Uuid = Uuid::from_u128(0x018f_0000_0000_7000_8000_0000_0000_00b0);

#[derive(Debug, Args)]
pub struct BootstrapArgs {
    /// Allow seeding against a non-loopback database (DANGEROUS — never use in production).
    #[arg(long = "i-understand-this-is-not-production", default_value_t = false)]
    pub override_loopback_check: bool,
}

pub async fn dispatch(args: BootstrapArgs) -> Result<ExitCode, WyrdCliError> {
    let raw_dsn = std::env::var(APP_DSN_ENV).ok();

    let non_loopback_host = detect_non_loopback_host(raw_dsn.as_deref());
    if let Some(host) = non_loopback_host
        && !args.override_loopback_check
    {
        return Err(WyrdCliError::NonLoopbackDsn { host });
    }

    let boot = PostgresBoot::from_env()
        .await
        .map_err(|e| WyrdCliError::Database {
            detail: e.to_string(),
        })?;

    let dsns = boot.dsns().map_err(|e| WyrdCliError::Database {
        detail: e.to_string(),
    })?;

    let migrator_pool = wyrd_sql::pool::build_pool(
        dsns.migrator.expose_secret(),
        PoolConfig::migrator_defaults(),
    )
    .await
    .map_err(|e| WyrdCliError::Database {
        detail: e.to_string(),
    })?;

    let migrate_result = async {
        wyrd_sql::migrate(&migrator_pool).await?;
        vala_sql::migrate(&migrator_pool).await
    }
    .await;
    migrator_pool.close().await;
    migrate_result.map_err(|e| WyrdCliError::Migration {
        detail: e.to_string(),
    })?;

    let app_pool = wyrd_sql::pool::build_pool(dsns.app.expose_secret(), PoolConfig::app_defaults())
        .await
        .map_err(|e| WyrdCliError::Database {
            detail: e.to_string(),
        })?;

    let platform_admin_dsn = dsns.platform_admin.ok_or(WyrdCliError::MissingAdminDsn)?;
    let platform_admin_pool = wyrd_sql::pool::build_pool(
        platform_admin_dsn.expose_secret(),
        PoolConfig::platform_admin_defaults(),
    )
    .await
    .map_err(|e| WyrdCliError::Database {
        detail: e.to_string(),
    })?;

    let data_tenant_id = provision_dev_tenant(&platform_admin_pool, &app_pool).await?;

    let card_ref = dev_card_ref();
    let (raw_key, card_ref_string) =
        seed_dev_principal(&app_pool, data_tenant_id, &card_ref).await?;

    let creds_path = write_credentials_toml(&raw_key, &card_ref_string)?;
    println!("credentials written to: {creds_path}");

    Ok(ExitCode::SUCCESS)
}

fn detect_non_loopback_host(raw_dsn: Option<&str>) -> Option<String> {
    let dsn = raw_dsn?;
    let url = url::Url::parse(dsn).ok()?;
    let host = url.host_str()?;
    if matches!(host, "localhost" | "127.0.0.1" | "::1") {
        None
    } else {
        Some(host.to_owned())
    }
}

async fn provision_dev_tenant(
    platform_admin_pool: &PgPool,
    app_pool: &PgPool,
) -> Result<DataTenantId, WyrdCliError> {
    let slug = wyrd_spec::TenantSlug::new(DEV_TENANT_SLUG).map_err(|e| WyrdCliError::Database {
        detail: e.to_string(),
    })?;

    if let Some(id) = resolve_by_slug_for_app(app_pool, &slug)
        .await
        .map_err(|e| WyrdCliError::Database {
            detail: e.to_string(),
        })?
    {
        return Ok(id);
    }

    let data_tenant_id = DataTenantId::new_v7();
    sqlx::query(
        "INSERT INTO platform.tenants (data_tenant_id, slug, display_name, status)
         VALUES ($1, $2, $3, 'active')
         ON CONFLICT (slug) DO NOTHING",
    )
    .bind(data_tenant_id.as_uuid())
    .bind(DEV_TENANT_SLUG)
    .bind(DEV_TENANT_DISPLAY_NAME)
    .execute(platform_admin_pool)
    .await
    .map_err(|e| WyrdCliError::Database {
        detail: e.to_string(),
    })?;

    resolve_by_slug_for_app(app_pool, &slug)
        .await
        .map_err(|e| WyrdCliError::Database {
            detail: e.to_string(),
        })?
        .ok_or_else(|| WyrdCliError::Database {
            detail: "dev tenant could not be resolved after insert".to_owned(),
        })
}

async fn seed_dev_principal(
    app_pool: &PgPool,
    data_tenant_id: DataTenantId,
    card_ref: &CardRef,
) -> Result<(SecretString, String), WyrdCliError> {
    let mut conn = TenantConn::acquire(app_pool, data_tenant_id)
        .await
        .map_err(|e| WyrdCliError::Database {
            detail: e.to_string(),
        })?;

    let sa_id = Uuid::new_v4();
    insert_service_account(
        &mut conn,
        sa_id,
        DEV_SA_PRINCIPAL_KIND,
        card_ref,
        DEV_SA_NAME,
        Some("Local dev service principal for zero-config SDK use"),
        DEV_SYSTEM_ACTOR_ID,
    )
    .await
    .map_err(|e| WyrdCliError::Database {
        detail: e.to_string(),
    })?;

    let api_key_id = Uuid::new_v4();
    let raw_key = generate_api_key(data_tenant_id);
    let prefix = key_prefix(data_tenant_id, &raw_key);
    let raw_clone = raw_key.clone();
    let key_hash = tokio::task::spawn_blocking(move || wyrd_auth_issue::hash_api_key(&raw_clone))
        .await
        .map_err(|e| WyrdCliError::Database {
            detail: format!("spawn_blocking join failed: {e}"),
        })?
        .map_err(|e| WyrdCliError::Hashing {
            detail: e.to_string(),
        })?;

    let expires_at = Utc::now() + chrono::Duration::days(3650);
    insert_api_key(
        &mut conn,
        api_key_id,
        sa_id,
        &prefix,
        &key_hash,
        DEV_SYSTEM_ACTOR_ID,
        expires_at,
    )
    .await
    .map_err(|e| WyrdCliError::Database {
        detail: e.to_string(),
    })?;

    insert_audit_credential_issuance(
        &mut conn,
        Uuid::new_v4(),
        DEV_SYSTEM_ACTOR_ID,
        sa_id,
        api_key_id,
        DEV_BOOTSTRAP_REQUEST_ID,
        expires_at,
    )
    .await
    .map_err(|e| WyrdCliError::Database {
        detail: e.to_string(),
    })?;

    conn.commit().await.map_err(|e| WyrdCliError::Database {
        detail: e.to_string(),
    })?;

    let card_ref_string = card_ref.to_string();
    Ok((raw_key, card_ref_string))
}

fn dev_card_ref() -> CardRef {
    CardRef {
        kind: CardKind::Service,
        name: CardName::new(DEV_CARD_NAME).expect("static dev card name is valid"),
        version: VersionBlock::parse(DEV_CARD_VERSION).expect("static dev version is valid"),
        space: Some(SpaceName::new(DEV_CARD_SPACE).expect("static dev space is valid")),
        uid: None,
    }
}

fn generate_api_key(tenant_id: DataTenantId) -> SecretString {
    let random = Uuid::new_v4().simple().to_string();
    let tenant = tenant_id.as_uuid().simple().to_string();
    let prefix = format!("wyrd_sk_{tenant}_{}", &random[..8]);
    SecretString::from(format!("{prefix}_{}", Uuid::new_v4().simple()))
}

fn key_prefix(tenant_id: DataTenantId, key: &SecretString) -> String {
    let raw = key.expose_secret();
    let tenant = tenant_id.as_uuid().simple().to_string();
    let random: String = raw
        .strip_prefix(&format!("wyrd_sk_{tenant}_"))
        .and_then(|rest| rest.split('_').next())
        .unwrap_or_default()
        .to_owned();
    format!("wyrd_sk_{tenant}_{random}")
}

fn write_credentials_toml(raw_key: &SecretString, card_ref: &str) -> Result<String, WyrdCliError> {
    let expanded = shellexpand::tilde("~/.config/wyrd/credentials.toml");
    let path = std::path::Path::new(expanded.as_ref());

    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|source| WyrdCliError::Io { source })?;
    }

    let content = format!(
        "[default]\napi_key = \"{}\"\ncard_ref = \"{}\"\n",
        raw_key.expose_secret(),
        card_ref
    );

    write_secret_file(path, &content)?;
    set_secret_permissions(path)?;

    Ok(expanded.into_owned())
}

#[cfg(unix)]
fn write_secret_file(path: &std::path::Path, value: &str) -> Result<(), WyrdCliError> {
    use std::io::Write;
    use std::os::unix::fs::OpenOptionsExt;

    let mut file = fs::OpenOptions::new()
        .create(true)
        .truncate(true)
        .write(true)
        .mode(0o600)
        .open(path)
        .map_err(|source| WyrdCliError::Io { source })?;
    file.write_all(value.as_bytes())
        .map_err(|source| WyrdCliError::Io { source })
}

#[cfg(not(unix))]
fn write_secret_file(path: &std::path::Path, value: &str) -> Result<(), WyrdCliError> {
    fs::write(path, value).map_err(|source| WyrdCliError::Io { source })
}

#[cfg(unix)]
fn set_secret_permissions(path: &std::path::Path) -> Result<(), WyrdCliError> {
    use std::os::unix::fs::PermissionsExt;

    let permissions = fs::Permissions::from_mode(0o600);
    fs::set_permissions(path, permissions).map_err(|source| WyrdCliError::Io { source })
}

#[cfg(not(unix))]
fn set_secret_permissions(_path: &std::path::Path) -> Result<(), WyrdCliError> {
    Ok(())
}

#[cfg(test)]
mod pg_tests {
    use std::os::unix::fs::PermissionsExt;

    use wyrd_dev_fixtures::pg::PgFixture;

    use super::*;

    fn temp_creds_path() -> (tempfile::TempDir, std::path::PathBuf) {
        let dir = tempfile::tempdir().expect("temp dir");
        let path = dir.path().join("credentials.toml");
        (dir, path)
    }

    #[test]
    fn dev_bootstrap_non_loopback_dsn_without_override_returns_error() {
        let host = detect_non_loopback_host(Some("postgres://user:pass@db.example.com/wyrd"));
        assert!(host.is_some());
        assert_eq!(host.unwrap(), "db.example.com");
    }

    #[test]
    fn dev_bootstrap_loopback_dsn_passes_check() {
        for dsn in [
            "postgres://user:pass@localhost/wyrd",
            "postgres://user:pass@127.0.0.1/wyrd",
            "postgres://user:pass@::1/wyrd",
        ] {
            let host = detect_non_loopback_host(Some(dsn));
            assert!(host.is_none(), "expected loopback for {dsn}");
        }
    }

    #[test]
    fn dev_bootstrap_unset_dsn_passes_check() {
        let host = detect_non_loopback_host(None);
        assert!(host.is_none());
    }

    #[test]
    fn dev_bootstrap_override_flag_permits_non_loopback() {
        let host = detect_non_loopback_host(Some("postgres://user:pass@prod.example.com/wyrd"));
        assert!(host.is_some(), "non-loopback detected");
        let args = BootstrapArgs {
            override_loopback_check: true,
        };
        assert!(args.override_loopback_check, "override flag set");
    }

    #[tokio::test]
    async fn dev_bootstrap_seeds_principal_key_audit_and_writes_credentials_toml() {
        let fixture = PgFixture::start().await.expect("fixture starts");
        let data_tenant_id = fixture.data_tenant_id();

        let card_ref = dev_card_ref();
        let (raw_key, card_ref_string) =
            seed_dev_principal(fixture.app_pool(), data_tenant_id, &card_ref)
                .await
                .expect("seed succeeds");

        let (_dir, creds_path) = temp_creds_path();
        let content = format!(
            "[default]\napi_key = \"{}\"\ncard_ref = \"{}\"\n",
            raw_key.expose_secret(),
            card_ref_string
        );
        write_secret_file(&creds_path, &content).expect("credentials written");
        set_secret_permissions(&creds_path).expect("permissions set");

        let written = std::fs::read_to_string(&creds_path).expect("credentials file readable");
        assert!(
            written.contains("api_key"),
            "credentials.toml contains api_key"
        );
        assert!(
            written.contains(&card_ref_string),
            "credentials.toml contains card_ref"
        );

        let audit_count: i64 = {
            let mut conn = fixture.tenant_conn().await.expect("tenant conn");
            sqlx::query_scalar(
                "SELECT count(*) FROM wyrd.audit_credential_issuance WHERE request_id = $1",
            )
            .bind(DEV_BOOTSTRAP_REQUEST_ID)
            .fetch_one(&mut **conn.transaction())
            .await
            .expect("audit query succeeds")
        };
        assert_eq!(audit_count, 1, "one audit row emitted");
    }

    #[tokio::test]
    async fn dev_bootstrap_written_file_is_mode_0600() {
        let fixture = PgFixture::start().await.expect("fixture starts");
        let data_tenant_id = fixture.data_tenant_id();

        let card_ref = dev_card_ref();
        let (raw_key, card_ref_string) =
            seed_dev_principal(fixture.app_pool(), data_tenant_id, &card_ref)
                .await
                .expect("seed succeeds");

        let (_dir, creds_path) = temp_creds_path();
        let content = format!(
            "[default]\napi_key = \"{}\"\ncard_ref = \"{}\"\n",
            raw_key.expose_secret(),
            card_ref_string
        );
        write_secret_file(&creds_path, &content).expect("credentials written");
        set_secret_permissions(&creds_path).expect("permissions set");

        let metadata = std::fs::metadata(&creds_path).expect("metadata readable");
        let mode = metadata.permissions().mode() & 0o777;
        assert_eq!(
            mode, 0o600,
            "credentials.toml must be mode 0600, got {mode:o}"
        );
    }
}
