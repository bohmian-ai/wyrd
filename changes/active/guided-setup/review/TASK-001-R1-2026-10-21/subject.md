# TASK-001 cumulative remediation review subject

Repository: `/home/thorrester/Documents/GitHub/wyrd-worktrees/guided-setup`.
Original base: `ebd75dc224854413d17cb06b70da8181b78cde6b`.
Prior implementation candidate: `32fbc264adccaf1160894769d5501fbe441259a5`.
Remediation start (approved packet): `00b6148919b9ccd5f958f3b621ab6a89c6372001`.
Candidate: `796377a1d02f5a0a80fd905cc18fbbbb2d49bb4a`.
Full acceptance range: `git diff ebd75dc224854413d17cb06b70da8181b78cde6b...796377a1d02f5a0a80fd905cc18fbbbb2d49bb4a`.
Latest fix range for locating owners and closure: `git diff 00b6148919b9ccd5f958f3b621ab6a89c6372001...796377a1d02f5a0a80fd905cc18fbbbb2d49bb4a` (five commits starting `57759179b`).
Initial tree clean and HEAD equals candidate. Reviewed source must remain immutable; only this new review directory may be written. Return BLOCKED if candidate changes.

## Authority and inputs

- Approved `changes/active/guided-setup/spec.md`, revision 6. Human explicitly approved `wyrd init guided`; it is a scope addition, not a retroactive finding. Revision 6 supersedes the original task's passive-only restriction solely for this explicit mode. Ordinary init remains passive. TASK-002 and combined checks remain outside task gate.
- Original `changes/active/guided-setup/tasks/TASK-001-guided-setup-and-asgi.md`, original obligations and implementation evidence.
- Remediation `changes/active/guided-setup/review/TASK-001-2026-10-21/TASK-001-R1-close-guided-setup-gaps.md`, full correction diagnoses, guided addition and appended implementation evidence/diagnosis/limits. Evidence is a claim to falsify, not acceptance proof.
- Prior verdict and final ledger in `changes/active/guided-setup/review/TASK-001-2026-10-21/{verdict.md,findings-validation.md}`. Preserve IDs FIND-TASK-001-1 (abnormal established lifespan drain), -2 (safe Wyrd-generated diagnostics), -3 (published Manual readback), -4 (host launch syntax), -5 (changed-item rustdoc). New IDs start at 6. Prior rejected pre-receive lifespan proposal and application-owned diagnostic scope must be reassessed only with actual source/authority, not automatically reopened.
- Root and nested AGENTS for docs, Python SDK, TypeScript SDK; `changes/spec-driven-development.md`; `agent_docs/index.md` and applicable rust/errors/agent-surfaces/maintainer-style guides; `TESTING.md`; docs CONTRIBUTING/PRODUCT when applicable. Architecture owner map `architecture/README.md`: Wyrd Client model, runtime/observation identity, public contracts; operations Local server bundles (now explicit agent-child ownership), security credential persistence/privacy; Bifrost System boundary lifecycle.

## Shared navigation map

Starting points, not coverage conclusions. Every reviewer independently expands against actual full diff, owners, sibling consumers/writers and tests.

- CLI: `cli.rs::Command::Init` → `init.rs::InitArgs::run`, `InitMode::Guided`, `DevServer::{run,guide,stop,exited}`, `Agent`, existing `settle`, diagnostics and bootstrap input resolution. `terminal.rs::{Terminal,SavedTerminal}`; process/TTY ownership, signal cleanup, readiness/connection ordering and ordinary-init compatibility. Consumers include installed Python/TS CLI wrappers and Setup docs. Tests C1 `guided_init_print_prompt_is_offline`, C2 `pg_tests::guided_init_lifecycle_and_prompt` and controlled host executable/PTY fixtures.
- Host/prompt boundary: `mcp/hosts.rs::McpHost::{launcher,launch,command}` and installer/detection; `mcp/mod.rs::{offer_local,McpInstallReport::print_handoff}`. Shared release prompt `wyrd-spec/src/{mcp.rs,setup-prompt.md}`; readiness, server `mcp/{guide,mod}.rs`, CLI proxy, mdsvex docs. All seven host branches; literal argv vs display, executable vs config detection, selection/refusal and existing host connection. Host literal unit selector in remediation.
- ASGI: `python/wyrd/integrations/asgi/_middleware.py::{WyrdMiddleware,_diagnostic}`; `_lifespan` local drain and abnormal-exit handling; original lifecycle owners `sdks/wyrd-sdk-python/src/state/mod.rs` and shared client `observe/lifecycle.rs`; Run native enter/exit and `python/wyrd/otel.py` context correlation. `tests/integration/test_asgi_integration.py` lifespan, cancellation, provider, isolation, workers; distinguish a raised CancelledError from actual task cancellation without inferring a defect solely from disclosure.
- Manual: Setup Manual link → Quickstart new step3 → `examples/quickstart/stored-verdicts.sql`; existing `wyrd get` and `wyrd query --server --file` owners/consumers; `test_local_development.py::test_guided_manual_setup` and helper. Follow exact published commands/output to durable rows/Run identity across Python/Rust/TypeScript and rerun/version behavior.
- Cumulative contract/build changes retained: Spec schemars anyOf in `envelope.rs` and unchanged utoipa sibling; shared prompt/MCP/proxy; docs landing raw imports and highlight owner, shell-links publication fixture; generated llms, command checker, pyproject/lockfile worker dependencies and mise test lanes. Review latest fixes for closure but do not omit these full-task boundaries.
- Rustdoc closure sites: changed Spec::json_schema, mcp tests module, CLI walk helper; also audit newly added guided/terminal/test items under same gate.
- Failure diagnosis in remediation evidence: C2 fake agent raw-terminal race. Trace actual test change, trace/diagnosis claim and available independent diagnostician evidence; do not accept or reject a test-only fix just from its summary. Inspect ordinary real-agent child ownership against approved process boundary, without expanding it into a process-tree sandbox or hard-kill promise.

## Independent review discipline

Read `.agents/skills/wyrd-task-review/SKILL.md` and fulfill assigned role. Use CodeGraph before locating/reading code (`.codegraph` exists): `mise exec -- codegraph explore "<symbols/question>"`. Read actual cumulative diff plus latest fix diff and every applicable nested AGENTS. Do not read other current discovery reports; prior verdict/final findings and remediation packet are shared required input. Do not receive the implementation conversation or intended verdict.

Write only your assigned report. No source/packet edits, formatters, commits, diff archives or new navigation scripts. Run commands through mise. No Cargo commands without coordination: shared builds must be sequential. Read-only probes and appropriate existing Python/static checks are allowed; clearly distinguish reruns/probes from inspected claims. Runtime submitted evidence is Linux x86_64 only; macOS arm64/Linux arm64 and paid host runtime behavior are unexercised. Whole journey/docs/SDK aggregates belong to integrated change review, not this task remediation. No optional improvements or style-only blockers; missing new/materially changed Rust-item rustdoc remains blocking. Trace defects to producer and compare existing complete workflows, not leaf reuse only.
