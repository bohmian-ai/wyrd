use skald_tool::default_registry;
use wyrd_client::WyrdClient;
use wyrd_mcp::bifrost::register_bifrost_tools;
use wyrd_mcp::cards::register_card_tools;

fn main() {
    let client = WyrdClient::from_env().expect("wyrd-mcp: client initialization failed");
    let registry = default_registry();
    register_bifrost_tools(registry, client.clone())
        .expect("wyrd-mcp: Bifrost tool registration failed");
    register_card_tools(registry, client).expect("wyrd-mcp: Card tool registration failed");
}
