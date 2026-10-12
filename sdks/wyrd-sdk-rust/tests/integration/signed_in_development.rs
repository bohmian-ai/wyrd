//! A person signs in once with the CLI and completes the local workflow from
//! the saved login, on the Keycloak identity lane; the telemetry exporter and
//! Gateway caller keep working after the original access token expires.

use std::time::Duration;

use wyrd_sdk::bifrost::QueryParam;
use wyrd_sdk::config::ClientConfig;
use wyrd_sdk::environment::Environment;
use wyrd_sdk::{Bifrost, GlobalConfig, WyrdClient};
use wyrd_testing::human_login::{FIXTURE_TENANT_SLUG, HUMAN_PUBLIC_ORIGIN, HumanSso};
use wyrd_testing::server::WyrdTestServer;

use crate::local_development::{Local, export, invoke, work};

/// Access-token lifetime the workflow outlives.
const ACCESS_TTL: chrono::Duration = chrono::Duration::seconds(2);

/// One span row: the Card it is attributed to.
#[derive(Debug, serde::Deserialize)]
struct Attributed {
    /// The stamped Card UID.
    card_uid: Option<String>,
}

/// Alice's saved administrator login completes the workflow, and the stock
/// clients built from it keep working after its first access token lapses.
///
/// # Panics
/// Panics when a step fails, the lapsed token is still accepted, or a later
/// export does not read back.
#[tokio::test(flavor = "multi_thread")]
#[ignore = "requires the Keycloak identity lane; run via `mise run test:identity:journey`"]
async fn saved_login_completes_the_workflow_past_token_expiry() {
    let local = Box::pin(Local::start(
        WyrdTestServer::builder()
            .without_process_telemetry_for_test()
            .with_public_origin(HUMAN_PUBLIC_ORIGIN.parse().expect("origin parses"))
            .with_access_ttl(ACCESS_TTL)
            .with_auth_verify_settings(wyrd_auth_verify::WyrdAuthVerifySettings {
                allowed_clock_skew: Duration::ZERO,
            }),
    ))
    .await;
    let server = local.deployment.server();
    let sso = HumanSso::new(server.base_url().expect("bound server has a URL"));
    sso.activate_keycloak(&local.deployment.key("sso_admin", &["admin"]).await)
        .await;
    let config_home = wyrd_testing::human_login::private_config_home();
    sso.save_login(
        config_home.path(),
        FIXTURE_TENANT_SLUG,
        "alice",
        "alice-password",
    )
    .await;
    let mut config = ClientConfig::from_environment(
        Environment::from([(
            "WYRD_CONFIG_HOME",
            config_home.path().to_str().expect("UTF-8 config home"),
        )]),
        &GlobalConfig::default(),
        server.base_url(),
        None,
    );
    config.grpc.endpoint = server.grpc_url().expect("bound server has a gRPC URL");
    let client = WyrdClient::with_config(config).expect("the saved login resolves");
    let original = client.access_token().await.expect("a first token mints");

    let worked = work(&local.deployment, &client).await;
    tokio::time::sleep(
        (ACCESS_TTL + chrono::Duration::seconds(1))
            .to_std()
            .expect("positive"),
    )
    .await;

    let lapsed = reqwest::Client::new()
        .get(format!("{}/v1/cards", server.base_url().expect("URL")))
        .bearer_auth(original.expose())
        .send()
        .await
        .expect("the server answers");
    assert_eq!(lapsed.status(), reqwest::StatusCode::UNAUTHORIZED);
    assert_eq!(invoke(&local.deployment, &client).await, "hi");
    let run = worked.state.run_for_card("agent").expect("agent run");
    export(&run).await;
    worked.telemetry.shutdown().expect("the later span exports");
    let rows: Vec<Attributed> = Bifrost::connect(&client)
        .await
        .expect("Bifrost connects")
        .sql_as(
            "SELECT card_uid FROM vala.traces.spans WHERE run_id = $1",
            &[QueryParam::String(run.run_id().to_string())],
        )
        .await
        .expect("the later span reads back");
    assert_eq!(rows.len(), 1, "{rows:?}");
    assert_eq!(rows[0].card_uid.as_ref(), Some(&worked.agent_uid));

    local.deployment.shutdown().await;
}
