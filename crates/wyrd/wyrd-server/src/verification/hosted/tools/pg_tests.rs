//! Every built-in call is authorized as `tool:invoke` on its exact name
//! before it runs, and its domain owner still decides data access.

use std::collections::{BTreeMap, BTreeSet};
use std::path::PathBuf;

use skald_spec::MediaFormat;
use wyrd_runtime::{DelegationStep, PrincipalRef};
use wyrd_spec::auth::PrincipalId;
use wyrd_spec::ids::{CardUid, SlotName};
use wyrd_spec::verification::TaskId;

use super::super::workspace::Staged;
use super::*;

use crate::components::gateway::recording::RecordingScribe;
use crate::test_support::{test_caller as caller, test_state as state, test_tenant};
use crate::verification::state::{StateRow, stand_in_identity};

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
///
/// # Panics
/// Panics when the workspace cannot materialize or the shared fixture or
/// empty pool cannot start.
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
        publisher: None,
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
                .call(BuiltinTool::FsRead, None, read.clone())
                .await
                .expect("granted")["units"][1]["text"],
            "beta"
        );
        assert_eq!(
            code(none.call(BuiltinTool::FsRead, None, read.clone()).await),
            "WYRD_PERMISSION_403_DENIED_RBAC"
        );
        assert_eq!(
            code(
                exact
                    .call(BuiltinTool::FsList, None, json!({"path": "/inputs"}))
                    .await
            ),
            "WYRD_PERMISSION_403_DENIED_RBAC"
        );
        assert!(
            global
                .call(BuiltinTool::FsSearch, None, json!({"query": "ALPHA"}))
                .await
                .is_ok()
        );
        assert_eq!(
            code(
                none.call(BuiltinTool::CodeRun, None, json!({"source": "1"}))
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
            let result = tools_only.call(tool, None, args).await;
            assert!(denied(&result), "{tool:?}: {result:?}");
        }
        for (tool, args) in [
            (BuiltinTool::CardsList, json!({})),
            (BuiltinTool::BifrostQuery, json!({"sql": "SELECT 1"})),
        ] {
            let result = data_only.call(tool, None, args).await;
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
            let result = invocation.call(tool, None, args).await;
            assert!(matches!(result, Err(ToolError::InvalidInput(_))), "{tool:?}: {result:?}");
        }
        let spent = invocation.call(BuiltinTool::CodeRun, None, json!({"source": "1"})).await;
        assert!(
            matches!(&spent, Err(ToolError::Invocation { detail, .. }) if detail.contains("budget")),
            "{spent:?}"
        );
    });
}

/// A filesystem reading tool without a declared workspace or a scratch
/// writer fails before any tool is bound, and so does `publish` outside a
/// step attempt of an execution; `fs.write` and `code.run` get private
/// scratch the reading tools may read, and Card and Bifrost tools need none.
///
/// # Panics
/// Panics when a reading tool or an unbound `publish` opens, or a
/// scratch-only tool is refused.
#[test]
fn reading_tools_need_a_declared_workspace() {
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
        let deadline = Instant::now() + Duration::from_secs(30);
        let none = BTreeMap::new();
        let producer = Producer {
            step_id: id.clone(),
            item_index: None,
            iteration_index: None,
            attempt: 1,
        };
        for (tool, producer) in [
            (BuiltinTool::FsRead, None),
            (BuiltinTool::Publish, None),
            (BuiltinTool::Publish, Some(&producer)),
        ] {
            let refused = tools
                .open(&id, &none, deadline, &[tool], producer)
                .await
                .err()
                .expect("refused");
            assert!(matches!(refused, BlockError::Terminal { .. }), "{refused}");
        }
        for declared in [
            &[BuiltinTool::CardsGet, BuiltinTool::BifrostQuery][..],
            &[BuiltinTool::FsWrite, BuiltinTool::CodeRun],
            &[
                BuiltinTool::FsWrite,
                BuiltinTool::FsRead,
                BuiltinTool::FsList,
            ],
        ] {
            assert!(
                tools
                    .open(&id, &none, deadline, declared, None)
                    .await
                    .is_ok(),
                "{declared:?}"
            );
        }
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

/// A fresh direct execution state of Verifier `verifier` for `caller`,
/// staging its revisions on `state`'s outbox.
///
/// # Panics
/// Panics when a `UUIDv7` is not a Card UID.
fn execution(state: &AppState, caller: &Caller, verifier: &CardUid) -> Arc<RunState> {
    let subject = CardUid::from_uuid(uuid::Uuid::now_v7()).expect("uid");
    Arc::new(RunState::fresh(
        Arc::clone(state.bifrost.scribe_outbox()),
        test_tenant(),
        StateRow {
            execution_id: wyrd_spec::ids::ExecutionId::new_v7(),
            identity: stand_in_identity(verifier, &subject),
            verifier_uid: verifier.clone(),
            card_uid: subject,
            owner_card_uid: None,
            run_id: None,
            deployment_environment: None,
            attribution: wyrd_spec::auth::Attribution::new(
                caller.principal.id,
                caller.principal.id,
            ),
        },
        Instant::now() + Duration::from_secs(30),
    ))
}

/// A `/work` file publication is a File operation of the receiving
/// boundary: a delegated caller, executed by another actor, holding
/// `verifier:run` on exactly its Verifier stages one `tool:invoke` and one
/// File decision, both allowed and both with the caller's request, subject,
/// actor, and delegation, and a replay with the scratch gone returns
/// the original receipt. A JSON value creates no File and stages only its
/// tool decision. A caller holding only the `publish` grant is denied at the
/// File decision before any File work, so a missing path answers the denial.
///
/// # Panics
/// Panics when a decision is missing, extra, or misattributed, a receipt
/// differs, or the tool-only caller reaches File work.
#[test]
fn file_publications_decide_their_file_permission() {
    wyrd_runtime::runtime().block_on(async {
        let (recorded, scribe) = RecordingScribe::attach(state().await);
        let verifier = CardUid::from_uuid(uuid::Uuid::now_v7()).expect("uid");
        let limits = HostedLimits::default();
        let producer = Producer {
            step_id: TaskId::new("draft").expect("task id"),
            item_index: None,
            iteration_index: None,
            attempt: 1,
        };
        let publishing = async |caller: Caller| {
            let run = execution(&recorded, &caller, &verifier);
            Invocation {
                state: recorded.clone(),
                publisher: Some((run, producer.clone())),
                ..invocation(caller, limits).await
            }
        };
        let decisions = async |caller: &Caller| {
            let deadline = std::time::Instant::now() + Duration::from_secs(30);
            assert_eq!(recorded.bifrost.scribe_outbox().settle(deadline).await, 0);
            scribe
                .audit_decisions()
                .into_iter()
                .filter(|row| row.request_id == caller.request_id.as_str())
                .map(|row| {
                    let attribution = caller.attribution();
                    assert_eq!(
                        (row.sub_principal_id, row.act_principal_id),
                        (
                            attribution.sub_principal_id.as_uuid(),
                            attribution.act_principal_id.as_uuid()
                        ),
                        "{} keeps the caller's subject and actor",
                        row.operation
                    );
                    assert_eq!(
                        row.detail.is_some(),
                        !caller.delegation_chain.is_empty(),
                        "{} keeps the caller's delegation",
                        row.operation
                    );
                    (row.operation, row.resource, row.permission, row.outcome)
                })
                .collect::<Vec<_>>()
        };
        let row = |operation: &str, resource: &str, permission: Permission, outcome: &str| {
            (
                operation.to_owned(),
                resource.to_owned(),
                permission.to_string(),
                outcome.to_owned(),
            )
        };
        let invoke = |tool: BuiltinTool| {
            row(
                OPERATION,
                tool.as_str(),
                Permission::tool_invoke(tool),
                "allowed",
            )
        };
        let files = |outcome| {
            row(
                "verification.publication.files",
                "publication:tree",
                Permission::verifier_run_any(),
                outcome,
            )
        };

        let delegator = caller([]).principal;
        let granted = Caller {
            delegation_chain: vec![DelegationStep {
                principal: PrincipalRef::from_principal(&delegator),
            }],
            executed_by: Some(PrincipalId::new(uuid::Uuid::now_v7())),
            ..caller([
                Permission::tool_invoke_any(),
                Permission::verifier_run(verifier.clone()),
            ])
        };
        let publisher = publishing(granted.clone()).await;
        publisher
            .call(
                BuiltinTool::FsWrite,
                None,
                json!({"path": "/work/tree/a.md", "content": "alpha"}),
            )
            .await
            .expect("the scratch file writes");
        let tree = json!({"name": "tree", "path": "/work/tree"});
        let receipt = json!({"name": "tree", "kind": "directory"});
        assert_eq!(
            publisher
                .call(BuiltinTool::Publish, Some("p-tree"), tree.clone())
                .await
                .expect("the tree publishes"),
            receipt
        );
        assert_eq!(
            publisher
                .call(
                    BuiltinTool::Publish,
                    Some("p-json"),
                    json!({"name": "j", "value": 1})
                )
                .await
                .expect("the value publishes"),
            json!({"name": "j", "kind": "json"})
        );
        std::fs::remove_dir_all(
            publisher
                .workspace()
                .expect("scratch")
                .work_dir()
                .join("tree"),
        )
        .expect("scratch removed");
        assert_eq!(
            publisher
                .call(BuiltinTool::Publish, Some("p-tree"), tree)
                .await
                .expect("the replay answers its receipt"),
            receipt,
            "a replay needs no scratch and uploads nothing"
        );
        assert_eq!(
            decisions(&granted).await,
            [
                invoke(BuiltinTool::FsWrite),
                invoke(BuiltinTool::Publish),
                files("allowed"),
                invoke(BuiltinTool::Publish),
                invoke(BuiltinTool::Publish),
                files("allowed"),
            ],
            "the tool and File decisions are separate; JSON makes no File decision"
        );

        let tool_only = caller([Permission::tool_invoke(BuiltinTool::Publish)]);
        let denied = publishing(tool_only.clone())
            .await
            .call(
                BuiltinTool::Publish,
                Some("p-ghost"),
                json!({"name": "tree", "path": "/work/missing"}),
            )
            .await;
        assert_eq!(code(denied), "WYRD_PERMISSION_403_DENIED_RBAC");
        assert_eq!(
            decisions(&tool_only).await,
            [invoke(BuiltinTool::Publish), files("denied"),],
            "the File decision precedes any File work"
        );
    });
}
