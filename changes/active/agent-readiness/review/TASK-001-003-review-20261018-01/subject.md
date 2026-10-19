# Immutable review subject

- Repository: `/home/thorrester/Documents/GitHub/wyrd-worktrees/local-dev-setup`
- Base: `1b962bb00` (approved specification and plan).
- Candidate: `98d8ef46e61ea6aa5dae90d59572467a256a3ef3`.
- Scope: cumulative TASK-001, TASK-002, TASK-003; TASK-004 live acceptance is excluded.
- Approved authority: `changes/active/agent-readiness/spec.md`, revision 2.
- Original tasks: `changes/active/agent-readiness/tasks/TASK-001-declare-get-pull.md`, `TASK-002-machine-readable-local-tooling.md`, `TASK-003-self-contained-agent-onboarding.md`.
- Verification results: each task's Implementation Evidence section; treat results as reported evidence, not independent source proof.
- Source must remain unchanged. Reports only in this directory. Read the complete `git diff 1b962bb00...98d8ef46e`; expand this navigation map independently.
- No `.codegraph/` directory exists in this checkout. Use repository-native navigation.
- Pre-existing untracked reports under `changes/active/local-dev-setup/review/TASK-001-review-20261017-03/` belong to another change and are outside this review.

## Navigation map (starting points, not coverage conclusions)

| Surface | Owners / changed symbols | Callers and tests |
|---|---|---|
| Declaration and local checking | `wyrd-client/src/cards/{handle,progress,saga}`, `wyrd-loader/src/lib.rs`, CLI `card.rs`, `declaration.rs` | CLI `tests/card_lifecycle.rs`; three SDK declare-and-pull journeys; loader tests |
| Wire, persistent declaration and audit | `wyrd-spec/src/registry/{submission,enums,bundle}.rs`, `vala/audit_detail.rs`; server `components/cards/{routes,service,mapping}`; SQL migrations and `queries/cards/declare.rs` | Server `pg_card_declaration_route`, served OpenAPI; SQL `pg_cards_declare`; SDK identity/replay journeys |
| Get, pull and runtime loading | `Cards::get`, `cards/pull/{mod,graph,bundle,workspace}`; `WyrdState`; Python state registry/per-kind APIs; TS native/public Cards | CLI complete/metadata bundle journey; SDK declare/get/pull and state tests; bundle old-header refusal |
| CLI machine streams and process lifetime | CLI `lib.rs`, `main.rs`, `error.rs`, `cli.rs`, `server/{dev,install,mod}.rs`, `mcp/mod.rs` | CLI `server_install`, `pg_tests`, local lifecycle parser and JSON errors; language CLI testing wrappers |
| MCP discovery and reads | Server `mcp/{cards,guide,mod}.rs`, spec `mcp.rs`; CLI proxy | MCP discovery and verification journeys; CLI selected-host/proxy tests; hosted tool references |
| SDK and generated closure | Python native wrappers, authored stubs and exports; TS native types, public Cards/testing APIs, wire generation; Rust SDK | Public CRUD, graph, local-development and support-desk journeys; type checks and codegen |
| Docs and embedded examples | `generate_llms_txt.py`, `mdsvex/{highlight,page-text}.js`, setup/quickstart and related guides; support-desk-app Python/Rust/TS | Docs content/highlight/page-text/index tests; `docs:check`; Python published-quickstart journey; Rust/TS example compilation |
| Static authority and UI | `AGENTS.md`, architecture vocabulary and Rust style references; existing UI Card wording | Compare approved target correction and naming scope; UI build/check evidence |

Read `AGENTS.md`, `architecture/agent-rules.md`, the spec-driven-development and maintainer-style references, current design/doctrine and applicable routed language/domain authority. Select additional authority from `architecture/references/README.md`. Assess source and callers, not the implementation summary. Do not run broad or competing Cargo suites; coordinate any necessary focused execution with the orchestrator.

Sensitive domain discovery is split into security/RBAC/tenant boundaries and persistent declaration/replay/bundle integrity. System reviewer owns process cancellation, readiness, failure and recovery paths. All reviewers cover their entire applicable cumulative surface, recording limits explicitly.
