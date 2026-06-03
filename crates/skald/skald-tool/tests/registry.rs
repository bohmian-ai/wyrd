use std::sync::Arc;
use std::thread;

use serde_json::json;
use skald_tool::{ToolError, ToolRegistry, ToolResolver, default_registry};

mod fixtures;
use fixtures::echo_tool;

#[test]
fn test_registry_register_and_resolve_round_trip() {
    let reg = ToolRegistry::new();
    reg.register(echo_tool("echo")).expect("register");
    let arc = reg.resolve("echo").expect("resolve");

    let rt = tokio::runtime::Runtime::new().expect("runtime");
    let out = rt
        .block_on(arc.invoke(json!({"text": "hi"})))
        .expect("invoke");

    assert_eq!(arc.name(), "echo");
    assert_eq!(out, json!({"echoed": "hi", "version": "v1"}));
}

#[test]
fn test_registry_register_duplicate_returns_name_taken() {
    let reg = ToolRegistry::new();

    reg.register(echo_tool("echo")).expect("first register");
    let err = match reg.register(echo_tool("echo")) {
        Ok(()) => panic!("expected duplicate registration error"),
        Err(err) => err,
    };

    assert!(matches!(err, ToolError::NameTaken { ref name } if name == "echo"));
    assert_eq!(err.code(), "SKALD_TOOL_409_NAME_TAKEN");
}

#[test]
fn test_registry_resolve_unknown_includes_available_list() {
    let reg = ToolRegistry::new();
    reg.register(echo_tool("b")).expect("register b");
    reg.register(echo_tool("a")).expect("register a");

    let err = match reg.resolve("c") {
        Ok(_) => panic!("expected unknown tool error"),
        Err(err) => err,
    };
    let code = err.code();

    assert!(matches!(err, ToolError::NotRegistered { .. }));
    if let ToolError::NotRegistered { name, available } = err {
        assert_eq!(name, "c");
        assert_eq!(available, vec!["a".to_string(), "b".to_string()]);
        assert_eq!(code, "SKALD_TOOL_404_NOT_REGISTERED");
    } else {
        unreachable!("wrong error variant");
    }
}

#[test]
fn test_registry_names_returns_sorted_list() {
    let reg = ToolRegistry::new();
    reg.register(echo_tool("zeta")).expect("register zeta");
    reg.register(echo_tool("alpha")).expect("register alpha");
    reg.register(echo_tool("mu")).expect("register mu");

    assert_eq!(
        reg.names(),
        vec!["alpha".to_string(), "mu".to_string(), "zeta".to_string()]
    );
}

#[test]
fn test_default_registry_is_process_global_singleton() {
    let r1 = default_registry();
    let r2 = default_registry();

    assert!(std::ptr::eq(r1, r2));
    r1.register(echo_tool("c02-singleton-probe"))
        .expect("global register");

    let arc = r2.resolve("c02-singleton-probe").expect("global resolve");
    assert_eq!(arc.name(), "c02-singleton-probe");
}

#[test]
fn test_default_registry_persists_across_test_threads() {
    let probe = "c02-thread-probe".to_string();
    let handle = thread::spawn(move || {
        default_registry()
            .register(echo_tool(&probe))
            .expect("thread register");
    });
    handle.join().expect("thread join");

    let arc = default_registry()
        .resolve("c02-thread-probe")
        .expect("global thread probe resolve");
    assert_eq!(arc.name(), "c02-thread-probe");
}

#[test]
fn test_tool_resolver_blanket_impl_on_reference_compiles_and_resolves() {
    fn helper(r: &dyn ToolResolver) -> Result<Arc<dyn skald_tool::AgentTool>, ToolError> {
        r.resolve("echo")
    }

    let reg = ToolRegistry::new();
    reg.register(echo_tool("echo")).expect("register");
    let arc = helper(&reg).expect("resolve by resolver");
    assert_eq!(arc.name(), "echo");
}

#[test]
fn test_tool_resolver_trait_is_object_safe() {
    let _boxed: Box<dyn ToolResolver> = Box::new(ToolRegistry::new());

    fn accepts(_: &dyn ToolResolver) {}
    let reg = ToolRegistry::new();
    accepts(&reg);
}

#[test]
fn test_registry_clear_removes_all_entries_but_preserves_pre_resolved_arcs() {
    let reg = ToolRegistry::new();
    reg.register(echo_tool("a")).expect("register a");
    reg.register(echo_tool("b")).expect("register b");

    let pre_a = reg.resolve("a").expect("pre resolve");
    reg.clear();

    assert!(reg.names().is_empty());
    let err = match reg.resolve("a") {
        Ok(_) => panic!("expected missing tool error"),
        Err(err) => err,
    };
    match err {
        ToolError::NotRegistered { available, .. } => assert!(available.is_empty()),
        _ => unreachable!("wrong error variant"),
    }

    assert_eq!(pre_a.name(), "a");
}
