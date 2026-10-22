# TASK-001 maintainer review

Result: **FAIL** — one explicit changed-item rustdoc obligation remains. No duplicate workflow owner or material abstraction/placement defect was found.

Subject: base `ebd75dc224854413d17cb06b70da8181b78cde6b`, candidate `32fbc264adccaf1160894769d5501fbe441259a5`. HEAD was checked and remains the candidate; only the review directory is untracked. This report reviews the cumulative range, independently of other reports.

## Authority and method

Read the approved revision-5 spec, original TASK-001 and appended evidence, root instructions, nested Python/TypeScript/docs instructions, `changes/spec-driven-development.md`, `agent_docs/index.md`, `rust.md`, `agent-surfaces.md`, and `maintainer-style.md`. Applicable architecture was the Wyrd client model/framework adapter boundary, local bundle operations, local credential persistence, and Bifrost system boundary. Used CodeGraph before source navigation, then read the actual cumulative diff and surrounding owners, callers and tests. Evidence claims were treated as claims, not as independent runtime proof.

## Changed-surface coverage

| Surface | Symbols, callers, tests and maintenance assessment |
|---|---|
| Local bootstrap and command exposure | `cli.rs::Command/dispatch`, `lib.rs`, moved `init.rs::InitArgs::run`, `DevServer::run`, `DevReady`, `setup_prompt`, and server install compatibility export. The rename retains the dependency-owning bootstrap, child shutdown, input resolution, saved configuration and gateway operations rather than adding a setup engine. `global_config.rs`, `gateway.rs`, and `server/{mod,install}.rs` follow the move. Read `guided_init_print_prompt_is_offline`, new lifecycle/prompt journey, and the cumulative local-development regression test edits. |
| Host handoff | `McpHost::launch/label` extends existing host metadata; `McpInstallReport::print_handoff` uses existing connected outcomes. Literal quoting is covered by the shell argument test and lifecycle journey; no agent execution path is introduced. |
| Prompt contract and transport | `mcp.rs::{SETUP_PROMPT_NAME,SETUP_PROMPT,setup_prompt}` and complete prompt body; server `guide::{prompt_descriptor,prompt}`, handler capability/list/get operations, and CLI `McpProxy` list/get methods alongside existing list/call tool methods. Shared text has one owner; installed proxy reuses `WyrdMcpHttpClient`/shared client authentication rather than creating a transport. Rust and Python proxy journeys cover byte parity and rejected inputs. Read the MCP discovery test update. |
| Schema projection | `envelope.rs::JsonSchema for Spec` and adjacent `PartialSchema`/`ToSchema` implementations; MCP `cards.get` journey consumer. The change modifies the existing schema projection, without a parallel validator/parser. OpenAPI remains its existing separate projection; this report does not claim new parity proof for that unchanged projection. |
| ASGI public adapter | Complete `_middleware.py`, package exports and authored re-export stub. `WyrdMiddleware` retains app/state/configuration, uses the public state startup/shutdown methods and `Run` context, and contains only ASGI protocol adaptation. No registration, writer/provider implementation, queue, or runtime is duplicated. Read the real-server lifespan, isolation, telemetry, example and worker tests plus worker app. The open mapping types are appropriate to the extensible ASGI boundary. |
| Existing lifecycle and correlation owners | Traced native Python `state/mod.rs::{start_bifrost,start_telemetry,shutdown}` and `observe/mod.rs::{__enter__,__exit__}` to `otel.py::{_RunCorrelationProcessor.on_start,_scope_stack,_enter_run,_exit_run,install_run_correlation}`. Inspected unit tests for nested scopes, task isolation, captured contexts into threads, mismatched exits and failing enrichment, and sibling observation/framework consumers. The mirrored ContextVar is local to the existing correlation owner and supports instrumentation that attaches an extracted OTel context. Retaining the existing OTel carrier still serves explicit captured-context thread callers. |
| Docs and examples | Setup, Choose your path, complete FastAPI integration page and embedded example; quickstart next link; stock-client, MCP, gateway, permission and troubleshooting rename references; example README/gateway README/TypeScript support-app comment. New copyable FastAPI code comes from the tested example and uses existing state/run methods. `llms.txt`/`llms-full.txt` project the same content. Architecture edits and CHANGELOG describe these boundaries and rename. |
| Docs build | `+page.server.ts` loader and its consumers of Quickstart YAML/programs; `highlight.js` shared highlighter/export change; publication fixture symlink in `shell-links.test.ts`. Raw imports use Vite's existing build facility while highlighting remains with its existing owner. No source-loading framework or second highlighter is introduced. Read command-checker change and docs contributor rules. |
| Installed clients and test wiring | TypeScript CLI signal wrapper remains a dispatcher into Rust, with renamed foreground command recognition. Python installed CLI bootstrap/prompt/manual helpers extend the existing local-deployment harness. Read agent-readiness runner/test comment edits, docsite/readme regression edits, server UI identity fixture comment changes, mise journey selectors, Python dev dependency additions and lockfile delta. New Gunicorn and OTel-ASGI dependencies are confined to test/dev needs. |

## Proposed finding

### MAINT-001 — missing panic contract on new filesystem test helper

- Classification: **VIOLATION**.
- Changed location: `crates/wyrd/wyrd-cli/tests/cli.rs:110–119`, `walk`.
- Governing rule: root `AGENTS.md`, Code: every new or changed Rust item has rustdoc, including `# Panics` when it can panic; missing rustdoc blocks merge. The review skill explicitly retains that blocking exception for documentation.
- Evidence: the new helper's only rustdoc sentence describes a file snapshot. Its `read_dir`, directory entry and file reads all use `expect`, so filesystem enumeration/read failures panic. The only nonrecursive caller is the new offline-purity test's `listing` closure (`cli.rs:55–59`); the full helper and that test were read. Nearby `guided_init_print_prompt_is_offline` and the new lifecycle journey already document their panic conditions.
- Concrete maintenance cost: the helper presents a reusable snapshot contract without documenting that a failure to inspect the tree aborts the test, leaving the required failure contract incomplete at the owning helper. This is an explicit documentation gate, not a claim that the implementation needs a different filesystem abstraction.
- Smallest testable correction: add a concise `# Panics` section naming directory enumeration/entry and file-read failures. Keep the current helper and offline assertions. Source inspection confirms closure; no runtime test or new helper is needed for this documentation correction.

## Verification limits

This was a source review. No Cargo commands, tests, formatters, dependency resolution, network lookup or source mutations were run. Existing C1–C8/check evidence is recorded in the task, but was not independently rerun here. The recorded runtime evidence is Linux x86_64 only; macOS arm64 and Linux arm64 remain unexercised. Combined-candidate verification explicitly belongs to change review. No other review report was read.

No speculative abstractions, naming preferences, optional refactors or additional features are requested.
