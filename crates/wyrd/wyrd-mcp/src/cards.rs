//! Card registry MCP delegates.
//!
//! These tools project the existing Card HTTP contract into the Skald tool
//! registry. The server remains responsible for validation, authorization,
//! lifecycle transitions, storage, tenancy, and audit.

use std::sync::Arc;

use async_trait::async_trait;
use reqwest::Method;
use schemars::JsonSchema;
use serde::Deserialize;
use serde_json::{Value, json};
use skald_tool::{AgentTool, ToolError, ToolRegistry};
use wyrd_client::WyrdClient;
use wyrd_spec::envelope::CardKind;
use wyrd_spec::error::WyrdError;
use wyrd_spec::ids::{CardName, CardUid, IdempotencyKey, SpaceName};
use wyrd_spec::registry::{
    CardLocator, CreateCardRequest, CreateCardResponse, GetCardResponse, ListCardsRequest,
    ListCardsResponse,
};
use wyrd_spec::storage::{DownloadInitRequest, DownloadInitResponse};

const CARDS_GET: &str = "cards.get";
const CARDS_LIST: &str = "cards.list";
const CARDS_LATEST: &str = "cards.latest";
const CARDS_LOAD: &str = "cards.load";
const CARDS_REGISTER: &str = "cards.register";
const CARDS_FINALIZE: &str = "cards.finalize";
const CARDS_DELETE: &str = "cards.delete";

#[derive(Debug, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
struct RegisterInput {
    /// Existing composite registration request.
    pub request: CreateCardRequest,
    /// Stable key reused when finalizing the same registration.
    pub idempotency_key: IdempotencyKey,
}

#[derive(Debug, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
struct FinalizeInput {
    /// Server-minted Card UID from the registration outcome.
    pub card_uid: CardUid,
    /// The idempotency key supplied to `cards.register`.
    pub idempotency_key: IdempotencyKey,
}

#[derive(Debug, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
struct LatestInput {
    /// Card kind.
    pub kind: CardKind,
    /// Exact Card workspace.
    pub space: SpaceName,
    /// Card name.
    pub name: CardName,
}

struct GetTool {
    client: WyrdClient,
}

#[async_trait]
impl AgentTool for GetTool {
    fn name(&self) -> &str {
        CARDS_GET
    }

    fn description(&self) -> &str {
        "Get one exact Card by its kind-qualified UID or exact CardRef. Reads require the cards:read permission."
    }

    fn input_schema(&self) -> Value {
        schema::<CardLocator>()
    }

    fn output_schema(&self) -> Value {
        schema::<GetCardResponse>()
    }

    async fn invoke(&self, args: Value) -> Result<Value, ToolError> {
        let locator = decode(args)?;
        let path = card_locator_path(&locator)?;
        let response: GetCardResponse = self
            .client
            .request_json(Method::GET, &path, None::<&()>)
            .await
            .map_err(invocation)?;
        serialize(response)
    }
}

struct ListTool {
    client: WyrdClient,
}

#[async_trait]
impl AgentTool for ListTool {
    fn name(&self) -> &str {
        CARDS_LIST
    }

    fn description(&self) -> &str {
        "List tenant-visible Card summaries with the shared typed filters and cursor. Reads require the cards:read permission."
    }

    fn input_schema(&self) -> Value {
        schema::<ListCardsRequest>()
    }

    fn output_schema(&self) -> Value {
        schema::<ListCardsResponse>()
    }

    async fn invoke(&self, args: Value) -> Result<Value, ToolError> {
        let request: ListCardsRequest = decode(args)?;
        let query = serde_urlencoded::to_string(&request)
            .map_err(|error| ToolError::InvalidInput(error.to_string()))?;
        let path = if query.is_empty() {
            "/v1/cards".to_owned()
        } else {
            format!("/v1/cards?{query}")
        };
        let response: ListCardsResponse = self
            .client
            .request_json(Method::GET, &path, None::<&()>)
            .await
            .map_err(invocation)?;
        serialize(response)
    }
}

struct LatestTool {
    client: WyrdClient,
}

#[async_trait]
impl AgentTool for LatestTool {
    fn name(&self) -> &str {
        CARDS_LATEST
    }

    fn description(&self) -> &str {
        "Resolve the newest stable Active Card in an exact kind, space, and name identity line. Reads require the cards:read permission."
    }

    fn input_schema(&self) -> Value {
        schema::<LatestInput>()
    }

    fn output_schema(&self) -> Value {
        schema::<GetCardResponse>()
    }

    async fn invoke(&self, args: Value) -> Result<Value, ToolError> {
        let input: LatestInput = decode(args)?;
        let path = format!(
            "/v1/cards/{}/{}/{}/latest",
            input.kind.wire_name(),
            input.space,
            input.name
        );
        let response: GetCardResponse = self
            .client
            .request_json(Method::GET, &path, None::<&()>)
            .await
            .map_err(invocation)?;
        serialize(response)
    }
}

struct LoadTool {
    client: WyrdClient,
}

#[async_trait]
impl AgentTool for LoadTool {
    fn name(&self) -> &str {
        CARDS_LOAD
    }

    fn description(&self) -> &str {
        "Plan one authorized Card artifact download using the shared storage contract. Reads require the cards:read permission."
    }

    fn input_schema(&self) -> Value {
        schema::<DownloadInitRequest>()
    }

    fn output_schema(&self) -> Value {
        schema::<DownloadInitResponse>()
    }

    async fn invoke(&self, args: Value) -> Result<Value, ToolError> {
        let request: DownloadInitRequest = decode(args)?;
        let response: DownloadInitResponse = self
            .client
            .request_json(Method::POST, "/v1/cards/download/init", Some(&request))
            .await
            .map_err(invocation)?;
        serialize(response)
    }
}

struct RegisterTool {
    client: WyrdClient,
}

#[async_trait]
impl AgentTool for RegisterTool {
    fn name(&self) -> &str {
        CARDS_REGISTER
    }

    fn description(&self) -> &str {
        "Register Cards through the composite registry service. Registration requires the cards:write permission; artifact upload plans remain an internal transfer seam."
    }

    fn input_schema(&self) -> Value {
        schema::<RegisterInput>()
    }

    fn output_schema(&self) -> Value {
        schema::<CreateCardResponse>()
    }

    async fn invoke(&self, args: Value) -> Result<Value, ToolError> {
        let input: RegisterInput = decode(args)?;
        let response: CreateCardResponse = self
            .client
            .submit_with_idempotency_key(
                Method::POST,
                "/v1/cards",
                &input.request,
                input.idempotency_key.as_str(),
            )
            .await
            .map_err(invocation)?;
        serialize(response)
    }
}

struct FinalizeTool {
    client: WyrdClient,
}

#[async_trait]
impl AgentTool for FinalizeTool {
    fn name(&self) -> &str {
        CARDS_FINALIZE
    }

    fn description(&self) -> &str {
        "Finalize a pending Card after its storage transfer using the registration service. Finalization requires the cards:write permission."
    }

    fn input_schema(&self) -> Value {
        schema::<FinalizeInput>()
    }

    fn output_schema(&self) -> Value {
        schema::<CreateCardResponse>()
    }

    async fn invoke(&self, args: Value) -> Result<Value, ToolError> {
        let input: FinalizeInput = decode(args)?;
        let path = format!("/v1/cards/{}/complete", input.card_uid);
        let response: CreateCardResponse = self
            .client
            .submit_with_idempotency_key(
                Method::POST,
                &path,
                &json!({}),
                input.idempotency_key.as_str(),
            )
            .await
            .map_err(invocation)?;
        serialize(response)
    }
}

struct DeleteTool {
    client: WyrdClient,
}

#[async_trait]
impl AgentTool for DeleteTool {
    fn name(&self) -> &str {
        CARDS_DELETE
    }

    fn description(&self) -> &str {
        "Delete one exact Card by UID or CardRef through the registry service. Deletion requires the explicit cards:write permission used by the registry route."
    }

    fn input_schema(&self) -> Value {
        schema::<CardLocator>()
    }

    fn output_schema(&self) -> Value {
        schema::<wyrd_spec::registry::DeleteCardResponse>()
    }

    async fn invoke(&self, args: Value) -> Result<Value, ToolError> {
        let locator = decode(args)?;
        let path = card_locator_path(&locator)?;
        let response: wyrd_spec::registry::DeleteCardResponse = self
            .client
            .request_json(Method::DELETE, &path, None::<&()>)
            .await
            .map_err(invocation)?;
        serialize(response)
    }
}

/// Register all Card registry delegates in `registry`.
pub fn register_card_tools(registry: &ToolRegistry, client: WyrdClient) -> Result<(), ToolError> {
    registry.register(Arc::new(GetTool {
        client: client.clone(),
    }))?;
    registry.register(Arc::new(ListTool {
        client: client.clone(),
    }))?;
    registry.register(Arc::new(LatestTool {
        client: client.clone(),
    }))?;
    registry.register(Arc::new(LoadTool {
        client: client.clone(),
    }))?;
    registry.register(Arc::new(RegisterTool {
        client: client.clone(),
    }))?;
    registry.register(Arc::new(FinalizeTool {
        client: client.clone(),
    }))?;
    registry.register(Arc::new(DeleteTool { client }))?;
    Ok(())
}

fn card_locator_path(locator: &CardLocator) -> Result<String, ToolError> {
    match locator {
        CardLocator::Uid { kind, uid } => {
            Ok(format!("/v1/cards/by-uid/{}/{}", kind.wire_name(), uid))
        }
        CardLocator::Ref(card_ref) => {
            let Some(space) = &card_ref.space else {
                return Err(ToolError::InvalidInput(
                    "exact CardRef must include space".to_owned(),
                ));
            };
            let query = serde_urlencoded::to_string([
                ("kind", card_ref.kind.wire_name().to_owned()),
                ("space", space.to_string()),
                ("name", card_ref.name.to_string()),
                ("version", card_ref.version.to_string()),
            ])
            .map_err(|error| ToolError::InvalidInput(error.to_string()))?;
            Ok(format!("/v1/cards/by-ref?{query}"))
        }
    }
}

fn decode<T>(args: Value) -> Result<T, ToolError>
where
    T: for<'de> Deserialize<'de>,
{
    serde_json::from_value(args).map_err(|error| ToolError::InvalidInput(error.to_string()))
}

fn serialize<T: serde::Serialize>(value: T) -> Result<Value, ToolError> {
    serde_json::to_value(value).map_err(|error| ToolError::OutputSerialization(error.to_string()))
}

fn schema<T: JsonSchema>() -> Value {
    serde_json::to_value(schemars::schema_for!(T)).expect("MCP tool schemas must serialize to JSON")
}

fn invocation(error: WyrdError) -> ToolError {
    ToolError::Invocation {
        detail: error.as_problem_json().to_string(),
        cause: None,
    }
}

#[cfg(test)]
mod tests {
    use serde_json::json;
    use skald_tool::ToolRegistry;
    use wyrd_client::{WyrdClient, config::ClientConfig};

    use super::register_card_tools;

    fn dummy_client() -> WyrdClient {
        WyrdClient::with_config(ClientConfig {
            api_key: Some("test_key_placeholder".to_owned().into()),
            ..ClientConfig::default()
        })
        .expect("test client builds")
    }

    #[test]
    fn card_tools_register_deterministic_surface() {
        let registry = ToolRegistry::new();
        register_card_tools(&registry, dummy_client()).expect("registration succeeds");

        assert_eq!(
            registry.names(),
            vec![
                "cards.delete",
                "cards.finalize",
                "cards.get",
                "cards.latest",
                "cards.list",
                "cards.load",
                "cards.register",
            ]
        );
    }

    #[test]
    fn card_tool_schemas_are_machine_readable_and_closed() {
        let registry = ToolRegistry::new();
        register_card_tools(&registry, dummy_client()).expect("registration succeeds");

        for name in registry.names() {
            let tool = registry.resolve(&name).expect("tool resolves");
            let schema = tool.input_schema();
            assert!(
                schema.get("type").is_some()
                    || schema.get("oneOf").is_some()
                    || schema.get("anyOf").is_some()
                    || schema.get("$ref").is_some(),
                "schema for {name} is not a JSON Schema root: {schema}"
            );
            assert!(!tool.description().is_empty());
        }
    }

    #[test]
    fn exact_ref_without_space_is_rejected_before_transport() {
        let registry = ToolRegistry::new();
        register_card_tools(&registry, dummy_client()).expect("registration succeeds");
        let tool = registry.resolve("cards.get").expect("get resolves");
        let error = tokio::runtime::Runtime::new()
            .expect("runtime builds")
            .block_on(tool.invoke(json!({
                "ref": {
                    "kind": "Mcp",
                    "name": "catalog",
                    "version": "1.0.0"
                }
            })))
            .expect_err("space is required for exact API refs");

        assert_eq!(error.code(), "SKALD_TOOL_422_INPUT");
    }
}
