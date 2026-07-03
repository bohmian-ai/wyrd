use skald_tool::default_registry;
use wyrd_client::WyrdClient;
use wyrd_mcp::bifrost::register_bifrost_tools;

fn main() {
    let client = WyrdClient::from_env().expect("wyrd-mcp: client initialization failed");
    register_bifrost_tools(default_registry(), client)
        .expect("wyrd-mcp: Bifrost tool registration failed");
}
