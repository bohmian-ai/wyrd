//! Card registry MCP delegates.
//!
//! These tools project the existing typed Card registry contract into the
//! Skald tool registry. The server remains responsible for validation,
//! authorization, lifecycle transitions, storage, tenancy, and audit.

use std::sync::Arc;

use async_trait::async_trait;
use schemars::JsonSchema;
use serde::Deserialize;
use serde_json::Value;
use skald_tool::{AgentTool, ToolError, ToolRegistry};
use wyrd_registry::{CardSelector, Cards};
use wyrd_spec::envelope::CardKind;
use wyrd_spec::error::WyrdError;
use wyrd_spec::ids::{CardName, SpaceName};
use wyrd_spec::registry::{CardLocator, GetCardResponse, ListCardsRequest, ListCardsResponse};
use wyrd_spec::storage::{DownloadInitRequest, DownloadInitResponse};

const CARDS_GET: &str = "cards.get";
const CARDS_LIST: &str = "cards.list";
const CARDS_LATEST: &str = "cards.latest";
const CARDS_LOAD: &str = "cards.load";

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
    cards: Cards,
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
        let response = self
            .cards
            .get_response(selector(locator))
            .await
            .map_err(invocation)?;
        serialize(response)
    }
}

struct ListTool {
    cards: Cards,
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
        let response = self.cards.list(request).await.map_err(invocation)?;
        serialize(response)
    }
}

struct LatestTool {
    cards: Cards,
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
        let response = self
            .cards
            .get_response(CardSelector::named(input.kind, input.space, input.name))
            .await
            .map_err(invocation)?;
        serialize(response)
    }
}

struct LoadTool {
    cards: Cards,
}

#[async_trait]
impl AgentTool for LoadTool {
    fn name(&self) -> &str {
        CARDS_LOAD
    }

    fn description(&self) -> &str {
        "Plan one authorized Card artifact download using the shared typed storage contract. Reads require the cards:read permission."
    }

    fn input_schema(&self) -> Value {
        schema::<DownloadInitRequest>()
    }

    fn output_schema(&self) -> Value {
        schema::<DownloadInitResponse>()
    }

    async fn invoke(&self, args: Value) -> Result<Value, ToolError> {
        let request: DownloadInitRequest = decode(args)?;
        let response = self
            .cards
            .download_init(request)
            .await
            .map_err(invocation)?;
        serialize(response)
    }
}

/// Register the read-only Card registry delegates in `registry`.
pub fn register_card_tools(
    registry: &ToolRegistry,
    client: wyrd_client::WyrdClient,
) -> Result<(), ToolError> {
    let cards = Cards::with_client(client);
    registry.register(Arc::new(GetTool {
        cards: cards.clone(),
    }))?;
    registry.register(Arc::new(ListTool {
        cards: cards.clone(),
    }))?;
    registry.register(Arc::new(LatestTool {
        cards: cards.clone(),
    }))?;
    registry.register(Arc::new(LoadTool { cards }))?;
    Ok(())
}

fn selector(locator: CardLocator) -> CardSelector {
    match locator {
        CardLocator::Uid { kind, uid } => CardSelector::uid(kind, uid),
        CardLocator::Ref(card_ref) => CardSelector::exact(card_ref),
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
        let config = ClientConfig {
            api_key: Some("test_key_placeholder".to_owned().into()),
            ..ClientConfig::default()
        };
        WyrdClient::with_config(config).expect("test client builds")
    }

    #[test]
    fn card_tools_register_deterministic_surface() {
        let registry = ToolRegistry::new();
        register_card_tools(&registry, dummy_client()).expect("registration succeeds");

        assert_eq!(
            registry.names(),
            vec!["cards.get", "cards.latest", "cards.list", "cards.load"]
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
    fn exact_ref_without_space_preserves_shared_selector_problem() {
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

        let skald_tool::ToolError::Invocation { detail, .. } = error else {
            panic!("expected shared selector problem, got {error:?}");
        };
        let problem: serde_json::Value =
            serde_json::from_str(&detail).expect("invocation detail is problem JSON");
        assert_eq!(problem["code"], "WYRD_REGISTRY_400_INVALID_CARD_SPEC");
    }
}
