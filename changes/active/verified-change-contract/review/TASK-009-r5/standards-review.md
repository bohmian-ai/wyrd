# Repository Standards Review — TASK-009 r5

## Immutable subject

- Repository: `/home/thorrester/Documents/GitHub/wyrd-verification-closeout`
- Base: `7d96c30066425e0cde2290842d5801307843283d`
- Candidate: `11eb8ed2b64b70903f171335723dcc549b463352`
- Approved specification: `changes/active/verified-change-contract/spec.md`, revision 46.
- Task inputs: original TASK-009 plus R1, R2, R3, and the revision-46 R4E remediation that supersedes R4.
- Reviewed range: the complete cumulative base-to-candidate diff, including implementation, declarations, generated outputs, tests, dependency metadata, architecture/spec updates, task evidence, and prior review records.
- CodeGraph was skipped because this checkout has no `.codegraph/` directory.

## Material repository-rule findings

### REPO-R5-001 — Superseded remediation remains machine-readable as ready

- **Rule:** `architecture/references/languages/spec-driven-development.md` requires `status: superseded` when an approved specification revision or replacement task invalidates a task.
- **Location:** `changes/active/verified-change-contract/review/TASK-009-r4/TASK-009-R4-stabilize-provider-registration-idempotency.md:4,13`.
- **Evidence:** the frontmatter still declares `status: ready`, while the same file says it is superseded and R4E declares `supersedes: [TASK-009-R4]` under approved specification revision 46.
- **Consequence:** tooling or an agent that reads task frontmatter can select the invalid revision-45 remediation even though the human-approved revision-46 task replaced it. The prose and machine-readable lifecycle state disagree.
- **Correction:** change only TASK-009-R4's frontmatter status from `ready` to `superseded`; retain the historical task body and R4E's `supersedes` link.

### REPO-R5-002 — Named Rust verification was not recorded with exact repository-native commands

- **Rule:** `AGENTS.md` §11 and `architecture/references/languages/spec-driven-development.md` require every specifically named Rust test in a task artifact or implementation report to include and run an exact `mise exec -- cargo nextest run` command with explicit package, target, and exact `test(=...)` expression; environment-owning setup must be present when required.
- **Location:** `changes/active/verified-change-contract/tasks/TASK-009-run-context-and-python-otel-correlation.md:213-214,261,269-272`.
- **Evidence:** the task names `run_for_card_selects_the_initial_view_and_shares_its_invocation` and `run_for_card_refuses_an_unknown_alias_without_network_io`, but records one regex command, `cargo nextest ... -E 'test(/observe::tests::run_for_card/)'`, rather than one exact command per named test and omits `mise exec --`. The named Rust SDK journey is likewise summarized as a bare `cargo nextest` invocation “under the Postgres wrapper” instead of preserving the full wrapper and `mise exec --` command.
- **Consequence:** the active completion record does not meet the repository's zero-ambiguity proof format and can be replayed with a changed or empty regex selection or without the repository toolchain/environment owner, despite broader lanes being recorded green.
- **Correction:** run and record the two shared tests separately with exact `test(=observe::tests::...)` expressions through `mise exec --`; record the complete repository-managed Postgres wrapper plus exact `mise exec -- cargo nextest ... test(=scoped_run_emits_drift_eval_and_generic_rows)` command for the SDK journey. Preserve the existing broader lane results.

## Authority coverage

| Changed surface | Applicable authority read | Source and consumer coverage | Result |
|---|---|---|---|
| Shared Rust `WyrdState` / `Run` initial-Card behavior | `AGENTS.md` §§2–6, 11–12, 16; `architecture/agent-rules.md`; `wyrd-design.md` client model and observation identity; `architecture/patterns.md`; `rust-core.md`; `run_api.md` | `crates/shared/wyrd-client/src/state.rs:489-510`; `observe/mod.rs:42-115`; all construction/selection callers; inline tests; Rust SDK journey | PASS |
| Python PyO3 state and Run projection | `AGENTS.md` §§7–8, 16; `pyo3-boundaries.md`; `python-api-and-stubs.md`; `maintainer-style.md` | `sdks/wyrd-sdk-python/src/state/mod.rs:177-193`; `src/observe/mod.rs:156-258`; registration, public imports, owning stub sources, generated projections | PASS |
| Python OpenTelemetry correlation and provider registration | approved spec revision 46 REQ-151/AC-032; `wyrd-design.md` observation identity; `bifrost-design.md` row identity; `run_api.md`; `telemetry-observations.md`; architecture constraints | Full `python/wyrd/otel.py`; PyO3 entry/exit callers; healthy, nested, await, copied-task, identical-Run concurrency, private/global provider, failure, and mismatch tests; authenticated journey | PASS |
| Python packaging and generated typing | `AGENTS.md` §§3, 8, 11; generated-artifact rule; `python-api-and-stubs.md`; implementation-execution | `pyproject.toml`, `uv.lock`, stub sources and generated `_wyrd.pyi` / public module stubs; dev-only OTLP/HTTP exporter | PASS |
| TypeScript native and ergonomic Run surfaces | `AGENTS.md` ownership/testing rules; `typescript-guide.md`; `maintainer-style.md`; `wyrd-design.md` client model | `native/src/cards.rs:299-311`; generated `index.d.ts` / `index.d.cts`; `wyrd/src/index.ts:1855-1865`; existing server journey | PASS |
| Cross-language journey and runtime-specific tests | `AGENTS.md` §11; `testing-workflows.md`; Python/TypeScript guides; runtime-ownership rules | Rust/TS initial-selection journeys; Python OTel unit coverage in Python; existing Python SDK→server OTLP/HTTP journey with persisted Bifrost queries | PASS |
| Active spec/task/remediation lifecycle and evidence | `AGENTS.md` §14; `spec-driven-development.md`; `implementation-execution.md` | Spec revision 46; original task; R1/R2/R3; R4 and replacement R4E; implementation evidence and prior review records | **FAIL — REPO-R5-001, REPO-R5-002** |

## Applicable-rule audit

| Rule | Evidence | Result |
|---|---|---|
| Shared client behavior belongs in `wyrd-client`; SDKs remain thin projections. | `WyrdState::run_for_card` owns hydrated alias resolution and `Run::new` owns the UUIDv7 invocation. PyO3 and N-API delegate to those methods without duplicating graph or identity logic. | PASS |
| New/materially changed Rust uses cohesive concrete owners and synchronous local computation. | Behavior remains on `WyrdState`, `Run`, `PyRun`, and `NativeWyrdState`; no new trait, factory, utility struct, async state machine, or alternate client path was introduced. | PASS |
| Rust types, borrowing, errors, imports, and documentation follow repository rules. | Shared methods use `CardRef`, `RunId`, `&str`, and `WyrdError`; changed items and tests have substantive rustdoc and required `# Errors` / `# Panics`; no new production unwrap, unjustified allow, or function-local import entered the diff. | PASS |
| PyO3 stays in the Python SDK and preserves the Rust-native owner. | Changed PyO3 uses `Bound`, remains synchronous, stores no Python lifetime across await, and calls the shared `Run`/`WyrdState` APIs. `wyrd-spec` and shared client remain PyO3-free. | PASS |
| Python public runtime, stubs, and package projections agree. | Runtime and stubs agree on keyword-only `run(card=None)`, `__enter__`, and `__exit__(exc_type=None, exc_value=None, traceback=None) -> Literal[False]`; public tests exercise signature parity. | PASS |
| Generated artifacts derive from owners and have drift evidence. | Stub sources changed with generated `.pyi`; N-API Rust source changed with generated `.d.ts` / `.d.cts`; recorded `codegen:check`, `py:typecheck`, and `ts:napi:check` passed. | PASS |
| TypeScript N-API remains thin, synchronous, and explicitly typed. | `NativeWyrdState::run` delegates to shared Rust and the public wrapper exposes `run(card?: string): Run`; no TS-only error, transport, lifecycle, or durable state was added. | PASS |
| OTel values are execution-local and use exact Bifrost attribute names. | `_SCOPE_KEY` stores a tuple stack in OTel context; `_RunCorrelationProcessor` writes only `wyrd.card_ref` and `wyrd.run_id`; no tenant, principal, Card UID, or request identity is client-authored. | PASS |
| Optional OTel remains optional and fail-open without weakening Card or observation errors. | Imports are optional; registration/context/span failures are contained; alias resolution remains strict; the OTLP/HTTP exporter is dev-only. | PASS |
| Revision-46 token-free behavior is reflected in implementation and owning docs. | No detach/token store remains; entry/exit attach updated stack values; provider outcomes are weakly cached before the foreign call; PyO3 passes the exact pair to both hooks; `run_api.md` records the deliberate attach-without-detach decision. | PASS |
| User-facing capability has the required real journeys and runtime-local tests. | Rust and TS journeys cover initial selection; Python unit tests own interpreter/OTel behavior; the Python journey uses stock OTLP/HTTP, real auth/server/Postgres, explicit flush/shutdown/publication, and persisted trace/custom/Eval joins. | PASS |
| Test integrity and placement are preserved. | Shared unit tests remain inline; real-server tests stay in gated integration homes; no ignore/xfail/sleep/assertion weakening or new test harness entered the cumulative diff. | PASS |
| Named Rust proof uses exact repository-native commands. | TASK-009 records a regex selector and bare Cargo summaries instead of the required exact `mise exec --` recipes. | **FAIL — REPO-R5-002** |
| Superseded tasks use the machine-readable `superseded` state. | R4 prose and R4E metadata say R4 is superseded, but R4 frontmatter remains `ready`. | **FAIL — REPO-R5-001** |
| No prohibited architecture entered the diff. | No server Run, new wire/persisted schema, second queue/transport/exporter, wrapper span, log/metric promise, compatibility alias, SQL/server boundary, or client-managed identity was added. | PASS |

## Verification reviewed

The cumulative implementation records report the focused Python OTel tests, the authenticated persisted journey, shared/Rust SDK/Python/TypeScript suites, typing, N-API and code generation checks, client/PyO3 boundary checks, formatting, linting, and `git diff --check` as passing. R4E records each new or changed Python test run by exact node ID, the full focused file (37 passed), and the real journey under the repository Postgres wrapper.

This reviewer did not run Cargo, pytest, or mise commands because the review coordinator prohibited overlapping verification during parallel discovery. I independently inspected the complete diff, owning sources, callers, declarations, tests, manifests, lockfile, architecture/spec/task records, and confirmed the cumulative `git diff --check` is clean. `REPO-R5-002` is a command-precision violation in the durable task evidence, not a claim that the broader recorded suites failed.

## Overall result

**FAIL**

The implementation and generated/public surfaces comply with their applicable repository authorities. The active packet still violates two explicit spec-driven workflow rules: one invalidated remediation remains `ready`, and named Rust proof is not recorded with exact repository-native commands.
