//! Every server-facing handle opens with `connect` / `connect_with` against a
//! real server, from environment-only or saved-only configuration, through
//! both credential paths, as a delegated subject, and across token renewal.
//!
//! The ambient chain is driven through `WyrdClient::from_environment` with a
//! fixed [`Environment`]: the one resolver `connect()` reaches through
//! `WyrdClient::from_global` with the process environment, so no test reads
//! or changes the process environment.

use std::path::Path;
use std::time::Duration;

use secrecy::SecretString;
use serde_json::json;
use tempfile::TempDir;
use wyrd_ai::auth::Principals;
use wyrd_ai::bifrost::{BifrostOptions, TableConfig};
use wyrd_ai::cards::{CardKind, Cards, ListCardsRequest};
use wyrd_ai::environment::Environment;
use wyrd_ai::{
    Bifrost, Gateway, GlobalConfig, OperatorConnections, Platform, QueueConfig, WyrdClient,
    WyrdError,
};
use wyrd_spec::auth::{PrincipalQuery, TokenAudience};
use wyrd_testing::human_login::private_config_home;
use wyrd_testing::server::WyrdTestServer;

use crate::support::Deployment;

/// The user table the discovery story registers in the deployment's tenant.
const EVENTS: &str = "bifrost.datasets.instantiation_events";

/// The table registered in another tenant that the listing must not show.
const OTHER_TENANT_EVENTS: &str = "bifrost.datasets.other_tenant_events";

/// Access-token lifetime of the renewal story: five seconds longer than the
/// client's 30-second refresh skew, so a token is reused for five seconds and
/// then renewed.
const ACCESS_TTL: chrono::Duration = chrono::Duration::seconds(35);

/// Wait after which the renewal story's cached token is inside the refresh
/// skew.
const PAST_REUSE_WINDOW: Duration = Duration::from_secs(6);

/// UTF-8 text of `path`, for an [`Environment`] value.
///
/// # Panics
/// Panics when the path is not UTF-8.
fn utf8(path: &Path) -> &str {
    path.to_str().expect("UTF-8 path")
}

/// Configuration home holding only `deployment`'s endpoints and `key`, as
/// local setup saves them.
///
/// # Panics
/// Panics when the files cannot be written.
fn saved_config(deployment: &Deployment, key: &str) -> TempDir {
    let home = private_config_home();
    let server = deployment.server();
    GlobalConfig::save_local(
        &Environment::from([("WYRD_CONFIG_HOME", utf8(home.path()))]),
        server.base_url().expect("bound server has a URL"),
        &server.grpc_url().expect("bound server has a gRPC URL"),
        Some(&SecretString::from(key)),
    )
    .expect("configuration saves");
    home
}

/// A listing of the registered Prompts, a harmless read.
fn prompts() -> ListCardsRequest {
    ListCardsRequest {
        kind: Some(CardKind::Prompt),
        name: None,
        version_range: None,
        status: None,
        filter: None,
        include_prerelease: false,
        limit: None,
        cursor: None,
    }
}

/// Connect every tenant handle over `client`, Bifrost with a non-default byte
/// budget, and complete one real server call on each.
///
/// # Errors
/// Returns the first connection or call failure.
async fn open_every_handle(client: &WyrdClient) -> Result<(), WyrdError> {
    Cards::connect_with(client).await?.list(prompts()).await?;
    Gateway::connect_with(client).await?.credentials().await?;
    Principals::connect_with(client)
        .await?
        .list(&PrincipalQuery::default())
        .await?;
    OperatorConnections::connect_with(client)
        .await?
        .list()
        .await?;
    let options = BifrostOptions {
        table: None,
        queue: QueueConfig::with_client_byte_limit(Some(64 * 1024 * 1024)),
    };
    Bifrost::connect_with(client, options)
        .await?
        .list_tables()
        .await?;
    Ok(())
}

/// Environment-only and saved-only configuration each open every tenant
/// handle, and an environment platform credential opens `Platform`.
///
/// # Panics
/// Panics when a resolution, connection, or call fails.
#[tokio::test(flavor = "multi_thread")]
#[ignore = "requires the repository-managed Postgres journey lifecycle"]
async fn ambient_connect_opens_every_handle() {
    let deployment = Deployment::start().await;
    let server = deployment.server();
    let url = server
        .base_url()
        .expect("bound server has a URL")
        .to_owned();
    let grpc = server.grpc_url().expect("bound server has a gRPC URL");
    let key = deployment.key("ambient_admin", &["admin"]).await;
    let empty = private_config_home();

    let from_environment = WyrdClient::from_environment(Environment::from([
        ("WYRD_CONFIG_HOME", utf8(empty.path())),
        ("WYRD_SERVER_URL", url.as_str()),
        ("WYRD_GRPC_URL", grpc.as_str()),
        ("WYRD_API_KEY", key.as_str()),
    ]))
    .expect("the environment resolves");
    open_every_handle(&from_environment)
        .await
        .expect("environment-only handles open");

    let saved = saved_config(&deployment, &key);
    let from_saved = WyrdClient::from_environment(Environment::from([(
        "WYRD_CONFIG_HOME",
        utf8(saved.path()),
    )]))
    .expect("the saved configuration resolves");
    open_every_handle(&from_saved)
        .await
        .expect("saved-only handles open");

    let platform_credential = server
        .initialize_platform_root()
        .await
        .expect("platform root initializes");
    let platform_client = WyrdClient::from_environment(Environment::from([
        ("WYRD_CONFIG_HOME", utf8(empty.path())),
        ("WYRD_SERVER_URL", url.as_str()),
        (
            "WYRD_ACCESS_TOKEN",
            secrecy::ExposeSecret::expose_secret(&platform_credential),
        ),
    ]))
    .expect("the platform environment resolves");
    Platform::connect_with(&platform_client)
        .await
        .expect("platform session exchanges")
        .list_tenants()
        .await
        .expect("platform lists tenants");

    deployment.shutdown().await;
}

/// With no credential anywhere in the chain, resolution refuses with the
/// stable client code before any handle is opened.
///
/// # Panics
/// Panics when the empty chain resolves or is refused with another code.
#[tokio::test(flavor = "multi_thread")]
#[ignore = "requires the repository-managed Postgres journey lifecycle"]
async fn connect_without_a_credential_fails_eagerly() {
    let deployment = Deployment::start().await;
    let empty = private_config_home();

    let refused = WyrdClient::from_environment(Environment::from([
        ("WYRD_CONFIG_HOME", utf8(empty.path())),
        (
            "WYRD_SERVER_URL",
            deployment
                .server()
                .base_url()
                .expect("bound server has a URL"),
        ),
    ]))
    .expect_err("an empty chain is refused");

    assert_eq!(
        WyrdError::from(refused).code(),
        "WYRD_CLIENT_401_NO_CREDENTIALS"
    );
    deployment.shutdown().await;
}

/// A refused API-key exchange fails at `connect`; a supplied bearer connects
/// with no request and the first protected call is refused; a fresh cached
/// token is reused by every handle without another exchange.
///
/// # Panics
/// Panics when a credential path connects, refuses, or exchanges otherwise.
#[tokio::test(flavor = "multi_thread")]
#[ignore = "requires the repository-managed Postgres journey lifecycle"]
async fn credential_paths_preserve_exchange_and_bearer_behavior() {
    let deployment = Deployment::start().await;

    let admin_key = deployment.key("forged_tenant", &["admin"]).await;
    let tenant = admin_key
        .strip_prefix("wyrd_sk_")
        .and_then(|rest| rest.split('_').next())
        .expect("an API key names its tenant");
    let rejected = deployment.client(&format!("wyrd_sk_{tenant}_forged_secret"));
    for refused in [
        Cards::connect_with(&rejected).await.err(),
        Gateway::connect_with(&rejected).await.err(),
        Principals::connect_with(&rejected).await.err(),
        OperatorConnections::connect_with(&rejected).await.err(),
        Bifrost::connect_with(&rejected, BifrostOptions::default())
            .await
            .err()
            .map(WyrdError::from),
    ] {
        let refused = refused.expect("a refused exchange fails at connect");
        assert_eq!(
            refused.code(),
            "WYRD_AUTH_401_API_KEY_INVALID",
            "{refused:?}"
        );
    }

    let issued = deployment
        .admin()
        .access_token()
        .await
        .expect("the key exchanges");
    let (signed, signature) = issued.expose().rsplit_once('.').expect("a JWT");
    let bearer = deployment.client(&format!("{signed}.{}", "A".repeat(signature.len())));
    let refused = Cards::connect_with(&bearer)
        .await
        .expect("a bearer connects without a request")
        .list(prompts())
        .await
        .expect_err("the invalid bearer is refused on use");
    assert_eq!(refused.status(), 401, "{refused:?}");
    let refused = Bifrost::connect_with(&bearer, BifrostOptions::default())
        .await
        .expect("a bearer connects without a request")
        .list_tables()
        .await
        .expect_err("the invalid bearer is refused on use");
    assert_eq!(WyrdError::from(refused).status(), 401);

    let admin = deployment.admin();
    let first = admin.access_token().await.expect("the key exchanges");
    open_every_handle(&admin).await.expect("every handle opens");
    let reused = admin.access_token().await.expect("the cache answers");
    assert_eq!(first.expose(), reused.expose(), "no second exchange");

    deployment.shutdown().await;
}

/// Handles connected with a delegated client act with the subject's
/// authority, not the actor's, and the server's retained audit attributes a
/// delegated query to the subject with the actor as `act_principal_id`.
///
/// # Panics
/// Panics when the delegation fails, a call is decided with the actor's
/// authority, or the retained decision names another principal.
#[tokio::test(flavor = "multi_thread")]
#[ignore = "requires the repository-managed Postgres journey lifecycle"]
async fn delegated_connect_uses_the_subject_principal() {
    let deployment = Deployment::start().await;
    let reader = deployment
        .client(
            &deployment
                .scoped_key("card_reader", &["cards:read", "bifrost_query:read"])
                .await,
        )
        .access_token()
        .await
        .expect("the subject's key exchanges");
    let admin = deployment.admin();
    let delegated = deployment
        .rooted_admin()
        .on_behalf_of(SecretString::from(reader.expose()), TokenAudience::Wyrd)
        .await
        .expect("the actor delegates");

    Cards::connect_with(&delegated)
        .await
        .expect("cards connect")
        .list(prompts())
        .await
        .expect("the subject reads Cards");
    let refused = OperatorConnections::connect_with(&delegated)
        .await
        .expect("operator connections connect")
        .list()
        .await
        .expect_err("the subject holds no operator read");
    assert_eq!(refused.code(), "WYRD_PERMISSION_403_DENIED_RBAC");
    OperatorConnections::connect_with(&admin)
        .await
        .expect("operator connections connect")
        .list()
        .await
        .expect("the actor alone may read operators");

    let to_bifrost = deployment
        .rooted_admin()
        .on_behalf_of(SecretString::from(reader.expose()), TokenAudience::Bifrost)
        .await
        .expect("the actor delegates to Bifrost");
    Bifrost::connect_with(&to_bifrost, BifrostOptions::default())
        .await
        .expect("Bifrost connects")
        .sql("SELECT COUNT(*) AS n FROM bifrost.system.audit_log", &[])
        .await
        .expect("the subject queries");
    let principals = Principals::connect_with(&admin)
        .await
        .expect("principals connect");
    let mut ids = Vec::new();
    for name in ["card_reader", "journey_admin"] {
        let page = principals
            .list(&PrincipalQuery {
                name: Some(name.to_owned()),
                ..PrincipalQuery::default()
            })
            .await
            .expect("principals list");
        ids.push(page.principals[0].principal_id.to_string());
    }
    let server = deployment.server();
    server.await_audit_retained().await.expect("audit settles");
    let decisions = server
        .retained_audit_records(
            server.data_tenant_id(),
            "act_principal_id",
            &format!(
                "operation = 'bifrost.query.read_decision' AND sub_principal_id = '{}'",
                ids[0]
            ),
        )
        .await
        .expect("retained audit reads");
    let actors: Vec<String> = decisions
        .iter()
        .map(|record| record[0].clone().expect("the decision names its actor"))
        .collect();
    assert_eq!(
        actors,
        [ids[1].clone()],
        "attributed to the subject, acted by the actor"
    );

    deployment.shutdown().await;
}

/// Handles keep the client they connected with after the saved configuration
/// changes, reuse its cached token within validity, and renew it after
/// expiry.
///
/// # Panics
/// Panics when a call fails, the cache is not reused, or no renewal happens.
#[tokio::test(flavor = "multi_thread")]
#[ignore = "requires the repository-managed Postgres journey lifecycle"]
async fn connected_handles_reuse_the_client_cache_and_renew() {
    let deployment = Deployment::start_with(
        WyrdTestServer::builder()
            .with_access_ttl(ACCESS_TTL)
            .with_auth_verify_settings(wyrd_auth_verify::WyrdAuthVerifySettings {
                allowed_clock_skew: Duration::ZERO,
            }),
    )
    .await;
    let key = deployment.key("renewing_admin", &["admin"]).await;
    let saved = saved_config(&deployment, &key);
    let client = WyrdClient::from_environment(Environment::from([(
        "WYRD_CONFIG_HOME",
        utf8(saved.path()),
    )]))
    .expect("the saved configuration resolves");
    let cards = Cards::connect_with(&client).await.expect("cards connect");
    let operators = OperatorConnections::connect_with(&client)
        .await
        .expect("operator connections connect");
    let first = client.access_token().await.expect("a token is cached");

    std::fs::write(
        saved.path().join("config.toml"),
        "[client]\nhttp_url = \"http://127.0.0.1:9\"\n",
    )
    .expect("configuration changes");
    std::fs::remove_file(saved.path().join("credentials.toml")).expect("key removed");
    cards
        .list(prompts())
        .await
        .expect("cards keep their client");
    operators.list().await.expect("operators keep their client");
    let reused = client.access_token().await.expect("the cache answers");
    assert_eq!(first.expose(), reused.expose(), "no second exchange");

    tokio::time::sleep(PAST_REUSE_WINDOW).await;
    cards.list(prompts()).await.expect("cards renew");
    operators.list().await.expect("operators renew");
    let renewed = client.access_token().await.expect("the renewal is cached");
    assert_ne!(first.expose(), renewed.expose(), "the token renewed");

    deployment.shutdown().await;
}

/// A connected Bifrost lists its tenant's tables and the built-ins compactly,
/// never another tenant's, refuses a caller without table read, and describes
/// one table's declared fields on demand.
///
/// # Panics
/// Panics when a registration, listing, refusal, or description differs.
#[tokio::test(flavor = "multi_thread")]
#[ignore = "requires the repository-managed Postgres journey lifecycle"]
async fn bifrost_lists_tables_and_describes_on_demand() {
    let deployment = Deployment::start().await;
    let schema = json!({
        "type": "object",
        "properties": { "id": { "type": "integer" }, "value": { "type": "string" } },
        "required": ["id", "value"],
    });
    for (client, fqn) in [
        (deployment.admin(), EVENTS),
        (
            deployment.client(&deployment.other_tenant_admin("instantiation-other").await),
            OTHER_TENANT_EVENTS,
        ),
    ] {
        Bifrost::connect_with(
            &client,
            BifrostOptions {
                table: Some(TableConfig::from_json_schema(fqn, &schema).expect("table declares")),
                ..BifrostOptions::default()
            },
        )
        .await
        .expect("writer connects")
        .register()
        .await
        .expect("table registers");
    }

    let bifrost = Bifrost::connect_with(&deployment.admin(), BifrostOptions::default())
        .await
        .expect("Bifrost connects");
    let listed: Vec<String> = bifrost
        .list_tables()
        .await
        .expect("tables list")
        .into_iter()
        .map(|entry| format!("{}.{}", entry.namespace, entry.name))
        .collect();
    assert!(listed.iter().any(|fqn| fqn == EVENTS), "{listed:?}");
    assert!(
        listed.iter().any(|fqn| fqn == "bifrost.drift.observations"),
        "{listed:?}"
    );
    assert!(
        !listed.iter().any(|fqn| fqn == OTHER_TENANT_EVENTS),
        "{listed:?}"
    );

    let refused = Bifrost::connect_with(
        &deployment.client(
            &deployment
                .scoped_key("no_table_read", &["cards:read"])
                .await,
        ),
        BifrostOptions::default(),
    )
    .await
    .expect("Bifrost connects")
    .list_tables()
    .await
    .expect_err("listing needs table read");
    assert_eq!(
        WyrdError::from(refused).code(),
        "WYRD_PERMISSION_403_DENIED_RBAC"
    );

    let described = bifrost
        .describe_table("bifrost.datasets", "instantiation_events")
        .await
        .expect("the table describes");
    let fields: Vec<&str> = described
        .user_fields
        .iter()
        .map(|field| field.name.as_str())
        .collect();
    assert_eq!(fields, ["id", "value"]);

    deployment.shutdown().await;
}
