//! An operator writes provider credentials with the CLI, and administers
//! deployments and capture through `wyrd_sdk::Gateway`, which reads
//! credentials only as redacted views.

use std::collections::BTreeSet;
use std::num::NonZeroU32;

use serde_json::json;
use wyrd_sdk::Gateway;
use wyrd_sdk::cli;
use wyrd_sdk::gateway::{
    CredentialBindingName, GatewayCaptureMode, GatewayCapturePolicyWrite, GatewayOperation,
    ModelId, ModelRef, ProviderAdapter, ProviderAuth, ProviderCredentialName,
    ProviderCredentialState, ProviderCredentialView, ProviderCredentialWrite,
    ProviderCredentialWriteSource, ProviderDeployment, ProviderDeploymentName, ProviderId,
};
use wyrd_testing::server::TEST_GATEWAY_CREDENTIAL_BINDING;

use crate::support::{self, Deployment};

/// The credential every story writes.
///
/// # Panics
/// Panics when the fixed name is invalid.
fn credential_name() -> ProviderCredentialName {
    ProviderCredentialName::new("openai-key").expect("credential name")
}

/// Write [`credential_name`], resolved from the server's environment
/// binding, with the CLI, and [`support::report`] its view.
///
/// # Panics
/// Panics when the write is refused.
async fn put_credential() {
    let view = cli::put_provider_credential(
        &ProviderCredentialWrite {
            name: credential_name(),
            provider: ProviderId::new("openai").expect("provider"),
            source: ProviderCredentialWriteSource::Environment {
                binding: CredentialBindingName::new(TEST_GATEWAY_CREDENTIAL_BINDING)
                    .expect("binding"),
            },
        },
        None,
    )
    .await
    .expect("the CLI writes the credential");
    support::report(&serde_json::to_value(view).expect("view serializes"));
}

/// An `openai` Chat Completions deployment that authenticates with
/// [`credential_name`].
///
/// # Panics
/// Panics when a fixed name is invalid.
fn deployment() -> ProviderDeployment {
    ProviderDeployment {
        name: ProviderDeploymentName::new("gpt-4o").expect("deployment name"),
        model: ModelRef {
            provider: ProviderId::new("openai").expect("provider"),
            model: ModelId::new("gpt-4o").expect("model"),
        },
        adapter: ProviderAdapter::OpenAi,
        auth: ProviderAuth::Bearer {
            credential: credential_name(),
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
    if support::is_child() {
        put_credential().await;
        return;
    }
    let deployment = Deployment::start().await;
    let gateway = Gateway::new(deployment.admin());

    let written: ProviderCredentialView = serde_json::from_value(
        deployment
            .run_child(
                "gateway_admin::cli_written_credential_reads_back_through_the_gateway",
                &deployment.key("cli_admin", &["admin"]).await,
                &[],
            )
            .await,
    )
    .expect("the child reports the view");

    assert_eq!(written.state, ProviderCredentialState::Active);
    assert_eq!(
        gateway
            .credential(&credential_name())
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

/// A deployment round trips through the Gateway, and the CLI cannot delete
/// the credential it uses.
///
/// # Panics
/// Panics when a write or read differs, or the delete is not refused with
/// the conflict code.
#[tokio::test(flavor = "multi_thread")]
#[ignore = "requires the repository-managed Postgres journey lifecycle"]
async fn credential_in_use_cannot_be_deleted() {
    if support::is_child() {
        if std::env::var("PHASE").as_deref() == Ok("delete") {
            let refused = cli::delete_provider_credential(&credential_name(), None)
                .await
                .expect_err("a referenced credential is kept");
            support::report(&json!(refused.code()));
        } else {
            put_credential().await;
        }
        return;
    }
    let deployment = Deployment::start().await;
    let gateway = Gateway::new(deployment.admin());
    let key = deployment.key("cli_admin", &["admin"]).await;
    let test = "gateway_admin::credential_in_use_cannot_be_deleted";
    deployment.run_child(test, &key, &[("PHASE", "put")]).await;

    assert_eq!(
        gateway
            .put_deployment(&self::deployment())
            .await
            .expect("deployment puts"),
        self::deployment()
    );
    assert_eq!(
        gateway.deployments().await.expect("deployments list"),
        [self::deployment()]
    );
    assert_eq!(
        deployment
            .run_child(test, &key, &[("PHASE", "delete")])
            .await,
        "WYRD_GATEWAY_409_RESOURCE_CONFLICT"
    );
    deployment.shutdown().await;
}

/// The capture policy the administrator writes is the one read back.
///
/// # Panics
/// Panics when the write or read fails or the policies differ.
#[tokio::test(flavor = "multi_thread")]
#[ignore = "requires the repository-managed Postgres journey lifecycle"]
async fn capture_policy_round_trips() {
    let deployment = Deployment::start().await;
    let gateway = Gateway::new(deployment.admin());

    let capture = gateway
        .put_capture_policy(&GatewayCapturePolicyWrite {
            mode: GatewayCaptureMode::Metadata,
            payload_fields: BTreeSet::new(),
        })
        .await
        .expect("capture puts");

    assert_eq!(capture.mode, GatewayCaptureMode::Metadata);
    assert_eq!(
        gateway.capture_policy().await.expect("capture reads"),
        capture
    );
    deployment.shutdown().await;
}

/// A caller without `gateway:read` cannot list credentials.
///
/// # Panics
/// Panics when the listing succeeds or is refused with another code.
#[tokio::test(flavor = "multi_thread")]
#[ignore = "requires the repository-managed Postgres journey lifecycle"]
async fn caller_without_gateway_read_is_refused() {
    let deployment = Deployment::start().await;
    let denied = Gateway::new(
        deployment.client(
            &deployment
                .scoped_key("query_only", &["bifrost_query:read"])
                .await,
        ),
    );

    let refused = denied
        .credentials()
        .await
        .expect_err("the listing is refused");

    assert_eq!(refused.code(), "WYRD_PERMISSION_403_DENIED_RBAC");
    deployment.shutdown().await;
}
