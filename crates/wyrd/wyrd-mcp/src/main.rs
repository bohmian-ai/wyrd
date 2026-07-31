use skald_tool::default_registry;
use std::sync::Arc;
use wyrd_client::WyrdClient;

use wyrd_mcp::bifrost::register_bifrost_tools;

fn main() {
    let client = WyrdClient::from_env().expect("wyrd-mcp: client initialization failed");
    register_bifrost_tools(default_registry(), Arc::new(client))
        .expect("wyrd-mcp: Bifrost tool registration failed");
}
