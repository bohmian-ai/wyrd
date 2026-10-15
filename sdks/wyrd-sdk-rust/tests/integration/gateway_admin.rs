//! An operator writes provider credentials with the CLI, and administers
//! deployments and capture through `wyrd_ai::Gateway`, which reads
//! credentials only as redacted views.

use std::collections::BTreeSet;

use wyrd_ai::cli;
use wyrd_ai::gateway::{
    CredentialBindingName, DeploymentAccess, GatewayCaptureMode, GatewayCapturePolicyWrite,
    GatewayOperation, GatewayPayloadField, ModelId, ProviderAdapter, ProviderAuth,
    ProviderCredentialName, ProviderCredentialState, ProviderCredentialView,
    ProviderCredentialWrite, ProviderCredentialWriteSource, ProviderDeployment,
    ProviderDeploymentName, ProviderId,
};
use wyrd_ai::{Gateway, WyrdClient};
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
        adapter: ProviderAdapter::OpenAi,
        auth: ProviderAuth::Bearer {
            credential: credential(credential_name),
        },
        models: None,
        capabilities: Some(BTreeSet::from([GatewayOperation::ChatCompletions])),
        access: DeploymentAccess::Open,
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
    let gateway = Gateway::connect_with(&deployment.admin())
        .await
        .expect("handle connects");

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
    let gateway = Gateway::connect_with(&deployment.admin())
        .await
        .expect("handle connects");
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

    let refused = Gateway::connect_with(&deployment.admin())
        .await
        .expect("handle connects")
        .put_deployment(&ProviderDeployment {
            capabilities: Some(BTreeSet::new()),
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
    Gateway::connect_with(&deployment.admin())
        .await
        .expect("handle connects")
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

    let gone = Gateway::connect_with(&deployment.admin())
        .await
        .expect("handle connects")
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
    let gateway = Gateway::connect_with(&deployment.admin())
        .await
        .expect("handle connects");
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
    Gateway::connect_with(&deployment.admin())
        .await
        .expect("handle connects")
        .put_deployment(&read_only)
        .await
        .expect("deployment puts");
    let reader = Gateway::connect_with(
        &deployment.client(
            &deployment
                .scoped_key("gateway_reader", &["gateway:read"])
                .await,
        ),
    )
    .await
    .expect("handle connects");

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

/// Upstream key of the open deployment in the access journey.
const OPEN_KEY: &str = "sk-open-upstream";
/// Upstream key of the granted deployment in the access journey.
const DEDICATED_KEY: &str = "sk-dedicated-upstream";
/// Message the access journey's upstream refuses retryably.
const REFUSED: &str = "refuse-me";

/// The access journey's server, its mock upstream, and the open
/// `openai-primary` and granted `openai-dedicated` deployments that both
/// permit only `gpt-4o`.
struct AccessJourney {
    /// The real server.
    deployment: Deployment,
    /// The upstream both deployments reach, told apart by their keys.
    upstream: wiremock::MockServer,
}

impl AccessJourney {
    /// Starts the upstream and server, writes one managed credential per
    /// deployment, and puts both deployments.
    ///
    /// The upstream answers every Chat Completions call, except that it
    /// refuses a call saying [`REFUSED`] with a retryable `503`.
    ///
    /// # Panics
    /// Panics when a setup step fails.
    async fn start() -> Self {
        use wiremock::matchers::{body_string_contains, method, path};
        use wiremock::{Mock, ResponseTemplate};

        let upstream = wiremock::MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/v1/chat/completions"))
            .and(body_string_contains(REFUSED))
            .respond_with(ResponseTemplate::new(503).set_body_json(
                serde_json::json!({"error": {"message": "busy", "type": "server_error"}}),
            ))
            .with_priority(1)
            .mount(&upstream)
            .await;
        Mock::given(method("POST"))
            .and(path("/v1/chat/completions"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "id": "chatcmpl-1", "object": "chat.completion", "created": 1, "model": "gpt-4o",
                "choices": [{"index": 0, "message": {"role": "assistant", "content": "hi"}, "finish_reason": "stop"}],
                "usage": {"prompt_tokens": 1, "completion_tokens": 1, "total_tokens": 2},
            })))
            .with_priority(2)
            .mount(&upstream)
            .await;
        let deployment = Deployment::start_with(
            wyrd_testing::server::WyrdTestServer::builder().with_gateway_provider_root_for_test(
                upstream.uri().parse().expect("upstream URL parses"),
            ),
        )
        .await;
        let gateway = Gateway::connect_with(&deployment.admin())
            .await
            .expect("handle connects");
        for (name, key, access) in [
            ("openai-primary", OPEN_KEY, DeploymentAccess::Open),
            ("openai-dedicated", DEDICATED_KEY, DeploymentAccess::Granted),
        ] {
            cli::put_provider_credential(
                &ProviderCredentialWrite {
                    name: credential(name),
                    provider: ProviderId::new("openai").expect("provider"),
                    source: ProviderCredentialWriteSource::ManagedSecret {
                        secret: wyrd_ai::operator_connections::SecretBearer::new(key.to_owned()),
                    },
                },
                Some(deployment.admin()),
            )
            .await
            .expect("the CLI writes the credential");
            gateway
                .put_deployment(&ProviderDeployment {
                    models: Some(vec![ModelId::new("gpt-4o").expect("model")]),
                    access,
                    ..self::deployment(name, name)
                })
                .await
                .expect("deployment puts");
        }
        Self {
            deployment,
            upstream,
        }
    }

    /// A client of a fresh Service principal `name` holding exactly the
    /// `gateway:invoke` grant scoped by `scope`.
    ///
    /// # Panics
    /// Panics when the grant does not parse or the principal cannot seed.
    async fn invoker(&self, name: &str, scope: serde_json::Value) -> WyrdClient {
        let key = self
            .deployment
            .granted_key(
                name,
                &[serde_json::json!({"resource": "gateway", "action": "invoke", "scope": scope})],
            )
            .await;
        self.deployment.client(&key)
    }

    /// Sends one Chat Completions call saying `hi` and addressing `model` as
    /// `client`, the way an OpenAI client does, and returns its status and
    /// Wyrd error code.
    ///
    /// # Panics
    /// Panics when no token mints or the gateway does not answer with JSON.
    async fn chat(&self, client: &WyrdClient, model: &str) -> (u16, Option<String>) {
        self.say(client, model, "hi").await
    }

    /// [`Self::chat`] saying `content`.
    ///
    /// # Panics
    /// Panics when no token mints or the gateway does not answer with JSON.
    async fn say(&self, client: &WyrdClient, model: &str, content: &str) -> (u16, Option<String>) {
        let token = client.access_token().await.expect("access token mints");
        let response = reqwest::Client::new()
            .post(format!("{}/v1/chat/completions", self.base()))
            .bearer_auth(token.expose())
            .json(&serde_json::json!({
                "model": model,
                "max_completion_tokens": 8,
                "messages": [{"role": "user", "content": content}],
            }))
            .send()
            .await
            .expect("the gateway answers");
        let status = response.status().as_u16();
        let body: serde_json::Value = response.json().await.expect("the reply is JSON");
        (status, body["error"]["code"].as_str().map(str::to_owned))
    }

    /// The model addresses `/v1/models` lists to `client`.
    ///
    /// # Panics
    /// Panics when no token mints or the listing fails.
    async fn models(&self, client: &WyrdClient) -> Vec<String> {
        let token = client.access_token().await.expect("access token mints");
        let listing: serde_json::Value = reqwest::Client::new()
            .get(format!("{}/v1/models", self.base()))
            .bearer_auth(token.expose())
            .send()
            .await
            .expect("the gateway answers")
            .json()
            .await
            .expect("the listing is JSON");
        listing["data"]
            .as_array()
            .expect("the listing has data")
            .iter()
            .map(|model| model["id"].as_str().expect("model id").to_owned())
            .collect()
    }

    /// The upstream key of every provider call so far, in arrival order.
    ///
    /// # Panics
    /// Panics when request recording is off.
    async fn upstream_keys(&self) -> Vec<String> {
        self.upstream
            .received_requests()
            .await
            .expect("request recording is on")
            .iter()
            .filter_map(|request| request.headers.get("authorization"))
            .map(|value| value.to_str().unwrap_or_default().to_owned())
            .collect()
    }

    /// The bound server's base URL.
    ///
    /// # Panics
    /// Panics when the server is not bound.
    fn base(&self) -> &str {
        self.deployment
            .server()
            .base_url()
            .expect("bound server has a URL")
    }
}

/// `all` reaches only the open deployment, deployment and exact-model grants
/// reach the granted one, a denied target is refused before provider IO and
/// never rescued by a fallback, a denied fallback is skipped, a model outside
/// the allowlist is unavailable, and `/v1/models` lists only what each
/// caller may invoke.
///
/// # Panics
/// Panics when a status, error code, listing, or upstream call differs.
#[tokio::test(flavor = "multi_thread")]
#[ignore = "requires the repository-managed Postgres journey lifecycle"]
async fn deployment_invocation_access_and_visibility() {
    let journey = AccessJourney::start().await;
    let everyone = journey
        .invoker("all_invoker", serde_json::json!("all"))
        .await;
    let dedicated = journey
        .invoker(
            "dedicated_invoker",
            serde_json::json!({"gateway": {"deployment": {"deployment": "openai-dedicated"}}}),
        )
        .await;
    let exact = journey
        .invoker(
            "exact_invoker",
            serde_json::json!({"gateway": {"model": {"deployment": "openai-dedicated", "model": "gpt-4o"}}}),
        )
        .await;
    let open = format!("Bearer {OPEN_KEY}");
    let granted = format!("Bearer {DEDICATED_KEY}");
    Gateway::connect_with(&journey.deployment.admin())
        .await
        .expect("handle connects")
        .put_fallback_policy(
            &serde_json::from_value(serde_json::json!({"rules": [
                {"scope": {"model": {"model": {"deployment": "openai-primary", "model": "gpt-4o"}}},
                 "candidates": [{"deployment": "openai-dedicated", "model": "gpt-4o"}]},
                {"scope": {"model": {"model": {"deployment": "openai-dedicated", "model": "gpt-4o"}}},
                 "candidates": [{"deployment": "openai-primary", "model": "gpt-4o"}]},
            ]}))
            .expect("fallback policy parses"),
        )
        .await
        .expect("fallback policy puts");

    assert_eq!(
        journey.chat(&everyone, "openai-primary/gpt-4o").await,
        (200, None)
    );
    let denied = Some("WYRD_PERMISSION_403_DENIED_RBAC".to_owned());
    assert_eq!(
        journey.chat(&everyone, "openai-dedicated/gpt-4o").await,
        (403, denied.clone()),
        "`all` never reaches a granted deployment, even with an open fallback"
    );
    assert_eq!(
        journey.chat(&dedicated, "openai-primary/gpt-4o").await,
        (403, denied.clone()),
        "a deployment grant covers only its deployment"
    );
    assert_eq!(
        journey.chat(&dedicated, "openai-dedicated/gpt-4o").await,
        (200, None)
    );
    assert_eq!(
        journey.chat(&exact, "openai-dedicated/gpt-4o").await,
        (200, None)
    );
    assert_eq!(
        journey
            .chat(&dedicated, "openai-dedicated/gpt-4o-mini")
            .await
            .0,
        404,
        "a model outside the deployment's allowlist is unavailable"
    );
    assert_eq!(
        journey.upstream_keys().await,
        [open.clone(), granted.clone(), granted],
        "no refused call reaches a provider"
    );
    // `all` does not cover the granted fallback of a retryably refused
    // call, so the fallback is skipped rather than dispatched.
    assert_eq!(
        journey
            .say(&everyone, "openai-primary/gpt-4o", REFUSED)
            .await
            .0,
        503,
        "with its only fallback denied, the provider's refusal is the answer"
    );
    assert_eq!(
        journey.upstream_keys().await.last(),
        Some(&open),
        "the denied fallback is never dispatched"
    );

    assert_eq!(journey.models(&everyone).await, ["openai-primary/gpt-4o"]);
    assert_eq!(
        journey.models(&dedicated).await,
        ["openai-dedicated/gpt-4o"]
    );
    assert_eq!(journey.models(&exact).await, ["openai-dedicated/gpt-4o"]);
    journey.deployment.shutdown().await;
}
