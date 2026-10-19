# Immutable remediation review subject

- Root: `/home/thorrester/Documents/GitHub/wyrd-worktrees/local-dev-setup`.
- Original base: `1b962bb0052e58c65af8c45be3ee0d1a04d483b6`.
- Previous candidate: `98d8ef46e61ea6aa5dae90d59572467a256a3ef3`.
- Current candidate: `d6b7c75b88db5eb144e6c282263f51692371bff1`.
- Review complete cumulative `git diff 1b962bb00...d6b7c75b8`; use `git diff 98d8ef46e...d6b7c75b8` to locate remediation owners, never as a substitute for cumulative acceptance.
- Approved spec: `changes/active/agent-readiness/spec.md`, revision 2.
- Original tasks: `changes/active/agent-readiness/tasks/TASK-001-declare-get-pull.md`, `TASK-002-machine-readable-local-tooling.md`, `TASK-003-self-contained-agent-onboarding.md`.
- Prior review directory: `changes/active/agent-readiness/review/TASK-001-003-review-20261018-01/`; read its `verdict.md`, `findings-validation.md` and `TASK-001-003-R1-close-validated-readiness-gaps.md` (including appended execution evidence). Prior findings are hypotheses to recheck, not proof of current closure or intended verdict.
- Scope: original TASK-001–003 plus R1 remediation, excluding TASK-004 live acceptance and final integrated gate. Six prior code findings and two evidence-only obligations retain their stable IDs; do not invent replacement IDs for the same causes.
- Verification sources: original task evidence and appended R1 table/commands. Treat reported results separately from fresh review execution.
- No `.codegraph/` exists; use repository-native navigation. No tracked changes at start. Preserve pre-existing untracked local-dev-setup review directory.
- Source remains immutable. Only write assigned reports here. No patch, diff or source archive belongs in this review directory. No commits, staging, branch changes or implementation.

## Shared starting map (expand independently)

| Boundary | Existing owners / remediation entry points | Callers / tests / consumers |
|---|---|---|
| Local check / declaration | CLI card.rs, wyrd-loader, Cards handle and saga | CLI local check/lifecycle; three SDK declaration stories; SQL/server replay |
| Contract / persistent declaration / audit | wyrd-spec registry and audit_detail; server components/cards; SQL declaration migrations/queries | Generated TS/errors, served OpenAPI, SQL replay and tenant tests |
| Read / pull / state | Cards get/pull, cards/pull/{graph,bundle,workspace}; download_progress::hidden; BundleReader | CLI artifact-bearing PTY pull; shared state negative/recovery tests; SDK generic/per-kind get/pull/load |
| CLI diagnostics / foreground lifetime | lib.rs::install_tracing/open_diagnostic_log; main; DevServer::Diagnostics; error boundary | card_lifecycle logged success/failure and terminal_json_pull; pg_tests readiness/recovery; installer and MCP host/proxy |
| Recovery metadata | wyrd-spec/src/error.rs bundle remediation attributes | shared state assert_pull_recovery; SDK error projections; generated errors.md / llms-full |
| MCP | merged Card selectors, start topic, initialize/proxy instructions; onboarding helper rustdoc | MCP discovery/verification; CLI saved-client/proxy proof |
| Docs / snippets / telemetry | Observe guide, quickstart LangTabs, page-text tests; support-desk-app Rust SupportDesk::serve and public otel owner; TS startup/shutdown regions | Real Service YAML table/bindings/slots; Python quickstart; Rust example compilation; TS typecheck; generated full reference |
| Safe deployment | Python and TS deploy callers into existing Cards.pull/BundleWorkspace | New runtime-owned failed/repeat deploy tests; existing shared publication rollback |
| Standards / dependency closure | New rustdoc and CLI test PTY helper; Cargo rustix dev feature/workspace-hack | All touched Rust/Python/TS/docs/UI surfaces, manifests/lockfiles/codegen; prior purely mechanical rename closure |

Sensitive domains: security/RBAC/tenancy (including diagnostic files/secret handling) and persistent declaration/replay/bundle integrity. System reviewer owns lifecycle/failure propagation and evidence recovery. All reviewers read AGENTS.md, agent-rules, spec-driven-development, maintainer-style, current design/doctrine and full applicable routed authority. Compare existing workflow reuse; do not manufacture platform mechanisms, release work or speculative hardening. Coordinate any Cargo execution; no broad/full journey suites. Each fresh discovery reviewer reads the prior round but not another current-round report or the user's implementation summary.
