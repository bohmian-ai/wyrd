//! Rust SDK gateway administration journey through the public `wyrd_sdk` crate.
//!
//! Builds every input from types re-exported by `wyrd_sdk::gateway`, drives a
//! credential and deployment through a real server, and proves redaction, a
//! referenced-credential conflict, and an under-privileged denial.
//!
//! Every credential mutation here runs on `wyrd_client`'s own credential
//! handle rather than the SDK, because the SDK deliberately re-exports none:
//! only the CLI and the scoped MCP tool may submit, rotate, revoke, or delete
//! a provider credential, and `wyrd_sdk::Gateway` has no method for any of
//! them. That the file needs a separate `wyrd_client` dev dependency to set up
//! and to assert the referenced-credential conflict is the proof. What the SDK
//! must prove is the rest — that it reads the redacted view, administers
//! deployments and policies, and honors the server's denials.

use std::collections::BTreeSet;
use std::num::NonZeroU32;

use secrecy::ExposeSecret;
use wyrd_client::gateway_credential::CredentialWriter;
use wyrd_sdk::Gateway;
use wyrd_sdk::bifrost::client_from_options;
use wyrd_sdk::gateway::{
    CredentialBindingName, GatewayCaptureMode, GatewayCapturePolicyWrite, GatewayOperation,
    ModelId, ModelRef, ProviderAdapter, ProviderAuth, ProviderCredentialName,
    ProviderCredentialSourceView, ProviderCredentialState, ProviderCredentialWrite,
    ProviderCredentialWriteSource, ProviderDeployment, ProviderDeploymentName, ProviderId,
};
use wyrd_testing::Bootstrap;
use wyrd_testing::server::{TEST_GATEWAY_CREDENTIAL_BINDING, WyrdTestServer};

/// Mint a machine API key through the harness's real bootstrap route.
///
/// # Panics
/// Panics when bootstrapping fails or returns a user principal.
async fn machine_key(server: &WyrdTestServer, name: &str, roles: &[&str]) -> String {
    match server
        .bootstrap_service(name, roles)
        .await
        .expect("service bootstraps")
    {
        Bootstrap::Machine { api_key, .. } => api_key.expose_secret().to_owned(),
        Bootstrap::User { .. } => panic!("service bootstrap returned a user principal"),
    }
}

/// Build a public gateway handle from a server URL and credential.
///
/// # Panics
/// Panics when the shared client cannot be assembled.
fn connect(base_url: &str, credential: &str) -> Gateway {
    Gateway::new(
        client_from_options(Some(base_url), Some(credential), None).expect("client builds"),
    )
}

/// Prove an SDK-only caller administers redacted gateway configuration.
///
/// # Panics
/// Panics when any journey step or structured-error expectation fails.
#[tokio::test(flavor = "multi_thread")]
#[ignore = "requires the repository-managed Postgres journey lifecycle"]
async fn administers_redacted_gateway_configuration() {
    let server = Box::pin(WyrdTestServer::start_bound())
        .await
        .expect("test server starts");
    let base_url = server
        .base_url()
        .expect("bound server has a URL")
        .to_owned();
    let gateway = connect(
        &base_url,
        &machine_key(&server, "rust_gateway_admin", &["admin"]).await,
    );
    let name = ProviderCredentialName::new("primary").expect("credential name");
    let write = ProviderCredentialWrite {
        name: name.clone(),
        provider: ProviderId::new("openai").expect("provider"),
        source: ProviderCredentialWriteSource::Environment {
            binding: CredentialBindingName::new(TEST_GATEWAY_CREDENTIAL_BINDING).expect("binding"),
        },
    };

    let credentials = CredentialWriter::new(
        client_from_options(
            Some(&base_url),
            Some(&machine_key(&server, "rust_gateway_writer", &["admin"]).await),
            None,
        )
        .expect("client builds"),
    );
    let view = credentials
        .put_credential(&write)
        .await
        .expect("credential puts");
    assert_eq!(view.state, ProviderCredentialState::Active);
    assert!(matches!(
        view.source,
        ProviderCredentialSourceView::Environment { .. }
    ));
    assert_eq!(
        gateway.credential(&name).await.expect("credential reads"),
        view
    );

    let deployment = ProviderDeployment {
        name: ProviderDeploymentName::new("primary").expect("deployment name"),
        model: ModelRef {
            provider: ProviderId::new("openai").expect("provider"),
            model: ModelId::new("gpt-4o").expect("model"),
        },
        adapter: ProviderAdapter::OpenAi,
        auth: ProviderAuth::Bearer {
            credential: name.clone(),
        },
        capabilities: BTreeSet::from([GatewayOperation::ChatCompletions]),
        routing_weight: NonZeroU32::MIN,
    };
    assert_eq!(
        gateway
            .put_deployment(&deployment)
            .await
            .expect("deployment puts"),
        deployment
    );
    assert_eq!(
        gateway.deployments().await.expect("deployments list"),
        vec![deployment.clone()]
    );

    let conflict = credentials
        .delete_credential(&name)
        .await
        .expect_err("referenced credential is kept");
    assert_eq!(conflict.status(), 409);

    let capture = gateway
        .put_capture_policy(&GatewayCapturePolicyWrite {
            mode: GatewayCaptureMode::Metadata,
            payload_fields: BTreeSet::new(),
        })
        .await
        .expect("capture puts");
    assert_eq!(
        gateway.capture_policy().await.expect("capture reads"),
        capture
    );

    server
        .seed_role(
            "rust_gateway_denied",
            &["bifrost_query:read".parse().expect("permission parses")],
        )
        .await
        .expect("denied role seeds");
    let denied = connect(
        &base_url,
        &machine_key(&server, "rust_gateway_denied", &["rust_gateway_denied"]).await,
    );
    assert_eq!(
        denied
            .credentials()
            .await
            .expect_err("under-privileged listing is refused")
            .status(),
        403
    );

    server.shutdown().await.expect("test server shuts down");
}
