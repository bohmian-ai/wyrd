//! An operator writes provider credentials with the CLI, and administers
//! deployments and capture through `wyrd_sdk::Gateway`, which reads
//! credentials only as redacted views.

use std::collections::BTreeSet;
use std::num::NonZeroU32;

use wyrd_sdk::cli;
use wyrd_sdk::gateway::{
    CredentialBindingName, GatewayCaptureMode, GatewayCapturePolicyWrite, GatewayOperation,
    GatewayPayloadField, ModelId, ModelRef, ProviderAdapter, ProviderAuth, ProviderCredentialName,
    ProviderCredentialState, ProviderCredentialView, ProviderCredentialWrite,
    ProviderCredentialWriteSource, ProviderDeployment, ProviderDeploymentName, ProviderId,
};
use wyrd_sdk::{Gateway, WyrdClient};
use wyrd_testing::server::TEST_GATEWAY_CREDENTIAL_BINDING;

use crate::support::Deployment;

/// The provider credential `name`.
///
/// # Panics
/// Panics when `name` is not a valid credential name.
fn credential(name: &str) -> ProviderCredentialName {
    ProviderCredentialName::new(name).expect("credential name")
}

/// Write the credential `name`, resolved from the server's
/// `test-provider-key` environment binding, with the CLI as `client`, and
/// return its redacted view.
///
/// # Panics
/// Panics when the write is refused.
async fn put_credential(client: WyrdClient, name: &str) -> ProviderCredentialView {
    cli::put_provider_credential(
        &ProviderCredentialWrite {
            name: credential(name),
            provider: ProviderId::new("openai").expect("provider"),
            source: ProviderCredentialWriteSource::Environment {
                binding: CredentialBindingName::new(TEST_GATEWAY_CREDENTIAL_BINDING)
                    .expect("binding"),
            },
        },
        Some(client),
    )
    .await
    .expect("the CLI writes the credential")
}

/// An `openai` Chat Completions deployment `name` that authenticates with
/// the credential `credential_name`.
///
/// # Panics
/// Panics when a name is invalid.
fn deployment(name: &str, credential_name: &str) -> ProviderDeployment {
    ProviderDeployment {
        name: ProviderDeploymentName::new(name).expect("deployment name"),
        model: ModelRef {
            provider: ProviderId::new("openai").expect("provider"),
            model: ModelId::new("gpt-4o").expect("model"),
        },
        adapter: ProviderAdapter::OpenAi,
        auth: ProviderAuth::Bearer {
            credential: credential(credential_name),
        },
        capabilities: BTreeSet::from([GatewayOperation::ChatCompletions]),
        routing_weight: NonZeroU32::MIN,
    }
}

/// A credential the CLI wrote reads back through the Gateway as the same
/// redacted, active view.
///
/// # Panics
/// Panics when the write or read fails or the views differ.
#[tokio::test(flavor = "multi_thread")]
#[ignore = "requires the repository-managed Postgres journey lifecycle"]
async fn cli_written_credential_reads_back_through_the_gateway() {
    let deployment = Deployment::start().await;
    let gateway = Gateway::with_client(deployment.admin());

    let written = put_credential(deployment.admin(), "read-back-key").await;

    assert_eq!(written.state, ProviderCredentialState::Active);
    assert_eq!(
        gateway
            .credential(&credential("read-back-key"))
            .await
            .expect("credential reads"),
        written
    );
    assert_eq!(
        gateway.credentials().await.expect("credentials list"),
        [written]
    );
    deployment.shutdown().await;
}

/// A deployment written through the Gateway reads back and lists, and once
/// deleted is gone.
///
/// # Panics
/// Panics when a write, read, or delete differs, or the deleted deployment
/// is not refused with the not-found code.
#[tokio::test(flavor = "multi_thread")]
#[ignore = "requires the repository-managed Postgres journey lifecycle"]
async fn deployment_round_trips() {
    let deployment = Deployment::start().await;
    let gateway = Gateway::with_client(deployment.admin());
    put_credential(deployment.admin(), "round-trip-key").await;
    let primary = self::deployment("round-trip", "round-trip-key");

    assert_eq!(
        gateway
            .put_deployment(&primary)
            .await
            .expect("deployment puts"),
        primary
    );
    assert_eq!(
        gateway
            .deployment(&primary.name)
            .await
            .expect("deployment reads"),
        primary
    );
    assert_eq!(
        gateway.deployments().await.expect("deployments list"),
        std::slice::from_ref(&primary)
    );
    gateway
        .delete_deployment(&primary.name)
        .await
        .expect("deployment deletes");
    let gone = gateway
        .deployment(&primary.name)
        .await
        .expect_err("a deleted deployment is gone");
    assert_eq!(gone.code(), "WYRD_GATEWAY_404_RESOURCE_NOT_FOUND");
    deployment.shutdown().await;
}

/// A deployment that offers no operation is refused.
///
/// # Panics
/// Panics when the write succeeds or is refused with another code.
#[tokio::test(flavor = "multi_thread")]
#[ignore = "requires the repository-managed Postgres journey lifecycle"]
async fn deployment_without_capabilities_is_refused() {
    let deployment = Deployment::start().await;
    put_credential(deployment.admin(), "no-capability-key").await;

    let refused = Gateway::with_client(deployment.admin())
        .put_deployment(&ProviderDeployment {
            capabilities: BTreeSet::new(),
            ..self::deployment("no-capability", "no-capability-key")
        })
        .await
        .expect_err("a deployment needs a capability");

    assert_eq!(refused.code(), "WYRD_GATEWAY_400_INVALID_CONFIGURATION");
    deployment.shutdown().await;
}

/// The CLI cannot delete a credential a deployment uses.
///
/// # Panics
/// Panics when a setup step fails, or the delete succeeds or is refused with
/// another code.
#[tokio::test(flavor = "multi_thread")]
#[ignore = "requires the repository-managed Postgres journey lifecycle"]
async fn credential_in_use_cannot_be_deleted() {
    let deployment = Deployment::start().await;
    put_credential(deployment.admin(), "in-use-key").await;
    Gateway::with_client(deployment.admin())
        .put_deployment(&self::deployment("in-use", "in-use-key"))
        .await
        .expect("deployment puts");

    let refused =
        cli::delete_provider_credential(&credential("in-use-key"), Some(deployment.admin()))
            .await
            .expect_err("a referenced credential is kept");

    assert_eq!(refused.code(), "WYRD_GATEWAY_409_RESOURCE_CONFLICT");
    deployment.shutdown().await;
}

/// Deleting a credential, then deleting it again, succeeds both times and
/// leaves it gone.
///
/// # Panics
/// Panics when a delete fails or the credential still reads.
#[tokio::test(flavor = "multi_thread")]
#[ignore = "requires the repository-managed Postgres journey lifecycle"]
async fn deleting_a_credential_twice_succeeds() {
    let deployment = Deployment::start().await;
    put_credential(deployment.admin(), "deleted-key").await;

    for _ in 0..2 {
        cli::delete_provider_credential(&credential("deleted-key"), Some(deployment.admin()))
            .await
            .expect("the delete succeeds");
    }

    let gone = Gateway::with_client(deployment.admin())
        .credential(&credential("deleted-key"))
        .await
        .expect_err("a deleted credential is gone");
    assert_eq!(gone.code(), "WYRD_GATEWAY_404_RESOURCE_NOT_FOUND");
    deployment.shutdown().await;
}

/// Capture starts disabled, and the payload policy the administrator writes
/// is the one read back.
///
/// # Panics
/// Panics when a write or read fails or the policies differ.
#[tokio::test(flavor = "multi_thread")]
#[ignore = "requires the repository-managed Postgres journey lifecycle"]
async fn capture_policy_round_trips() {
    let deployment = Deployment::start().await;
    let gateway = Gateway::with_client(deployment.admin());
    assert_eq!(
        gateway.capture_policy().await.expect("capture reads").mode,
        GatewayCaptureMode::Disabled
    );

    let capture = gateway
        .put_capture_policy(&GatewayCapturePolicyWrite {
            mode: GatewayCaptureMode::Payload,
            payload_fields: BTreeSet::from([GatewayPayloadField::Request]),
        })
        .await
        .expect("capture puts");

    assert_eq!(
        (capture.mode, &capture.payload_fields),
        (
            GatewayCaptureMode::Payload,
            &BTreeSet::from([GatewayPayloadField::Request])
        )
    );
    assert_eq!(
        gateway.capture_policy().await.expect("capture reads"),
        capture
    );
    deployment.shutdown().await;
}

/// A caller holding only `gateway:read` reads a deployment but cannot delete
/// it.
///
/// # Panics
/// Panics when the read fails, or the delete succeeds or is refused with
/// another code.
#[tokio::test(flavor = "multi_thread")]
#[ignore = "requires the repository-managed Postgres journey lifecycle"]
async fn gateway_reader_cannot_delete_a_deployment() {
    let deployment = Deployment::start().await;
    put_credential(deployment.admin(), "reader-key").await;
    let read_only = self::deployment("read-only", "reader-key");
    Gateway::with_client(deployment.admin())
        .put_deployment(&read_only)
        .await
        .expect("deployment puts");
    let reader = Gateway::with_client(
        deployment.client(
            &deployment
                .scoped_key("gateway_reader", &["gateway:read"])
                .await,
        ),
    );

    assert_eq!(
        reader
            .deployment(&read_only.name)
            .await
            .expect("the reader reads"),
        read_only
    );
    let refused = reader
        .delete_deployment(&read_only.name)
        .await
        .expect_err("a reader cannot delete");

    assert_eq!(refused.code(), "WYRD_PERMISSION_403_DENIED_RBAC");
    deployment.shutdown().await;
}
