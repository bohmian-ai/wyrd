//! Every built-in call is authorized as `tool:invoke` on its exact name
//! before it runs, and its domain owner still decides data access.

use std::collections::{BTreeMap, BTreeSet};
use std::path::PathBuf;

use skald_spec::MediaFormat;
use vala_verify::blocks::Position;
use wyrd_spec::ids::SlotName;
use wyrd_spec::verification::TaskId;

use super::super::workspace::Staged;
use super::*;

use crate::test_support::{test_caller as caller, test_state as state};

/// A sandbox that never spawns a worker; no test here runs Python.
///
/// # Panics
/// Panics when the empty pool cannot start.
async fn idle_sandbox(limits: HostedLimits) -> Arc<Sandbox> {
    Arc::new(
        Sandbox::start(
            PathBuf::from("/nonexistent/wyrd-server"),
            HostedLimits {
                monty_min_workers: 0,
                ..limits
            },
        )
        .await
        .expect("an empty pool starts"),
    )
}

/// An invocation as `caller` over a workspace holding `/inputs/notes.txt`.
async fn invocation(caller: Caller, limits: HostedLimits) -> Invocation {
    let workspace = Workspace::materialize(
        vec![Staged {
            path: "/inputs/notes.txt".to_owned(),
            bytes: b"alpha\nbeta\n".to_vec(),
            format: Some(MediaFormat::Text),
            sources: BTreeSet::from([SlotName::new("notes").expect("slot")]),
        }],
        limits,
        None,
    )
    .expect("workspace materializes");
    Invocation {
        state: state().await,
        caller,
        workspace: Some(Arc::new(workspace)),
        sandbox: idle_sandbox(limits).await,
        deadline: Instant::now() + Duration::from_secs(30),
        code_budget: Arc::new(Mutex::new(limits.agent_code_budget)),
        limits,
    }
}

/// The stable code of a structured failure.
///
/// # Panics
/// Panics on success or an unstructured failure.
fn code(result: Result<Value, ToolError>) -> String {
    match result {
        Err(ToolError::StructuredInvocation(problem)) => problem.code,
        other => panic!("expected a structured failure, got {other:?}"),
    }
}

/// Only an exact or global tool grant runs a tool; the denial comes first,
/// and interleaved callers never share authority.
///
/// # Panics
/// Panics when a tool runs without its grant or a grant leaks across calls.
#[test]
fn tools_require_their_exact_invoke_grant() {
    wyrd_runtime::runtime().block_on(async {
        let limits = HostedLimits::default();
        let exact = invocation(
            caller([Permission::tool_invoke(BuiltinTool::FsRead)]),
            limits,
        )
        .await;
        let global = invocation(caller([Permission::tool_invoke_any()]), limits).await;
        let none = invocation(caller([]), limits).await;
        let read = json!({"path": "/inputs/notes.txt"});

        assert_eq!(
            exact
                .call(BuiltinTool::FsRead, read.clone())
                .await
                .expect("granted")["units"][1]["text"],
            "beta"
        );
        assert_eq!(
            code(none.call(BuiltinTool::FsRead, read.clone()).await),
            "WYRD_PERMISSION_403_DENIED_RBAC"
        );
        assert_eq!(
            code(
                exact
                    .call(BuiltinTool::FsList, json!({"path": "/inputs"}))
                    .await
            ),
            "WYRD_PERMISSION_403_DENIED_RBAC"
        );
        assert!(
            global
                .call(BuiltinTool::FsSearch, json!({"query": "ALPHA"}))
                .await
                .is_ok()
        );
        assert_eq!(
            code(
                none.call(BuiltinTool::CodeRun, json!({"source": "1"}))
                    .await
            ),
            "WYRD_PERMISSION_403_DENIED_RBAC"
        );
    });
}

/// A tool grant gives no data access and data access gives no tool grant.
///
/// The query's own table decision is Oracle's and needs a live Oracle;
/// `pg_hosted_artifacts::recovered_authority_bounds_every_agent_tool`
/// proves it. Here the query proves only that data access without a tool
/// grant is refused.
///
/// # Panics
/// Panics when either grant alone reaches Card or Bifrost data.
#[test]
fn tool_and_domain_grants_are_both_required() {
    wyrd_runtime::runtime().block_on(async {
        let limits = HostedLimits::default();
        let tools_only = invocation(caller([Permission::tool_invoke_any()]), limits).await;
        let data_only = invocation(
            caller([Permission::card_read(), Permission::bifrost_query_read()]),
            limits,
        )
        .await;
        let denied = |result: &Result<Value, ToolError>| {
            matches!(result, Err(ToolError::StructuredInvocation(problem)) if problem.status == 403)
        };
        for (tool, args) in [
            (BuiltinTool::CardsList, json!({})),
            (BuiltinTool::BifrostListTables, json!({})),
        ] {
            let result = tools_only.call(tool, args).await;
            assert!(denied(&result), "{tool:?}: {result:?}");
        }
        for (tool, args) in [
            (BuiltinTool::CardsList, json!({})),
            (BuiltinTool::BifrostQuery, json!({"sql": "SELECT 1"})),
        ] {
            let result = data_only.call(tool, args).await;
            assert_eq!(code(result), "WYRD_PERMISSION_403_DENIED_RBAC", "{tool:?}");
        }
    });
}

/// Arguments cannot carry authentication context, and `code.run` stops at
/// the call's spent Python budget.
///
/// # Panics
/// Panics when a forged field or a spent budget is accepted.
#[test]
fn tools_refuse_forged_arguments_and_spent_budgets() {
    wyrd_runtime::runtime().block_on(async {
        let invocation = invocation(
            caller([Permission::tool_invoke_any()]),
            HostedLimits {
                agent_code_budget: Duration::ZERO,
                ..HostedLimits::default()
            },
        )
        .await;
        for (tool, args) in [
            (BuiltinTool::FsRead, json!({"path": "/inputs/notes.txt", "principal": "admin"})),
            (BuiltinTool::BifrostQuery, json!({"sql": "SELECT 1", "tenant": "other"})),
            (BuiltinTool::CardsList, json!({"delegation_chain": []})),
        ] {
            let result = invocation.call(tool, args).await;
            assert!(matches!(result, Err(ToolError::InvalidInput(_))), "{tool:?}: {result:?}");
        }
        let spent = invocation.call(BuiltinTool::CodeRun, json!({"source": "1"})).await;
        assert!(
            matches!(&spent, Err(ToolError::Invocation { detail, .. }) if detail.contains("budget")),
            "{spent:?}"
        );
    });
}

/// A filesystem or code tool without a declared workspace fails before any
/// tool is bound; Card and Bifrost tools need none.
///
/// # Panics
/// Panics when a workspace tool opens without a workspace.
#[test]
fn workspace_tools_need_a_declared_workspace() {
    wyrd_runtime::runtime().block_on(async {
        let limits = HostedLimits::default();
        let tools = HostedTools::new(
            state().await,
            caller([Permission::tool_invoke_any()]),
            Arc::new(TenantEvidence::new(
                state().await,
                caller([]),
                BTreeMap::new(),
                limits,
            )),
            Arc::new(Workspaces::new(limits)),
            idle_sandbox(limits).await,
            limits,
        );
        let id = TaskId::new("review").expect("task id");
        let action: Action =
            serde_json::from_value(json!({"kind": "extract", "from": "inputs.deck"}))
                .expect("action parses");
        let call = || BlockCall {
            id: &id,
            position: Position::Step,
            block: &action,
            inputs: BTreeMap::new(),
            workspace: BTreeMap::new(),
            deadline: Instant::now() + Duration::from_secs(30),
        };
        for tool in [BuiltinTool::FsRead, BuiltinTool::CodeRun] {
            let refused = tools.open(&call(), &[tool]).await.err().expect("refused");
            assert!(matches!(refused, BlockError::Terminal { .. }), "{refused}");
        }
        assert!(
            tools
                .open(&call(), &[BuiltinTool::CardsGet, BuiltinTool::BifrostQuery])
                .await
                .is_ok()
        );
    });
}

/// An action's Python budget survives its attempts; other actions start
/// full.
///
/// # Panics
/// Panics when a retry restores spent time or actions share a budget.
#[test]
fn retries_keep_their_actions_spent_time() {
    wyrd_runtime::runtime().block_on(async {
        let limits = HostedLimits::default();
        let tools = HostedTools::new(
            state().await,
            caller([]),
            Arc::new(TenantEvidence::new(
                state().await,
                caller([]),
                BTreeMap::new(),
                limits,
            )),
            Arc::new(Workspaces::new(limits)),
            idle_sandbox(limits).await,
            limits,
        );
        let review = TaskId::new("review").expect("task id");
        *tools.code_budget(&review).lock().expect("budget") = Duration::from_secs(1);
        assert_eq!(
            *tools.code_budget(&review).lock().expect("budget"),
            Duration::from_secs(1)
        );
        assert_eq!(
            *tools
                .code_budget(&TaskId::new("other").expect("task id"))
                .lock()
                .expect("budget"),
            limits.agent_code_budget
        );
    });
}
