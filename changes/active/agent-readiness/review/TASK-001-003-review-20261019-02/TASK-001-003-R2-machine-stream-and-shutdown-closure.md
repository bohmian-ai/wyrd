---
id: TASK-001-003-R2
kind: remediation
status: ready
spec: SPEC-agent-readiness
spec_revision: 2
parent_task: [TASK-001, TASK-002, TASK-003, TASK-001-003-R1]
remediates: [FIND-TASK-002-1, FIND-TASK-002-3, FIND-TASK-003-4]
evidence_dependencies: [FIND-TASK-002-4, FIND-TASK-003-3]
---

# Finish machine-stream isolation and Rust shutdown

Execute directly with `$wyrd-implement`. This is a bounded implementation handoff from independently validated findings, not another design cycle. Preserve other contributors' work and keep correction at the existing owners.

## Subject and authority

- Repository: `/home/thorrester/Documents/GitHub/wyrd-worktrees/local-dev-setup`.
- Approved spec: `changes/active/agent-readiness/spec.md`, revision 2.
- Original tasks: `changes/active/agent-readiness/tasks/TASK-001-declare-get-pull.md`, `TASK-002-machine-readable-local-tooling.md`, `TASK-003-self-contained-agent-onboarding.md`.
- Original base: `1b962bb0052e58c65af8c45be3ee0d1a04d483b6`.
- Prior candidate: `98d8ef46e61ea6aa5dae90d59572467a256a3ef3`.
- Reviewed cumulative candidate: `d6b7c75b88db5eb144e6c282263f51692371bff1`.
- Prior R1: `changes/active/agent-readiness/review/TASK-001-003-review-20261018-01/TASK-001-003-R1-close-validated-readiness-gaps.md`.
- Current validated ledger/verdict: `findings-validation.md` and `verdict.md` beside this task; focused uncertainty is resolved in `followup-review.md`.
- Governing requirements: REQ-006/AC-007 exclusive machine streams; REQ-008/AC-008 actual language lifecycle recipe; INV-004 buffered evidence and safe drain; AGENTS.md §16 changed-item rustdoc.

Four prior code findings are closed: FIND-TASK-001-1, FIND-TASK-002-2, FIND-TASK-003-1 and FIND-TASK-003-2. Preserve their recovery/pull/recipe/deploy corrections. New FIND-TASK-003-4 is R1's new serve error path; do not reopen 003-1 for that same cause or count both.

## FIND-TASK-002-1: failed diagnostic writes escape onto JSON stderr

**Diagnosis.** CLI `src/lib.rs:132–136` correctly selects a file for ordinary JSON-mode tracing, but the locked `tracing-subscriber` 0.3.23 builder defaults internal-error reporting to true. Its fmt layer directly calls `eprintln!` when the selected writer fails. A successful append open cannot prevent later full-filesystem/quota/write failure. HTTP/TLS diagnostics during get/install/dev can therefore precede or accompany the operation's one JSON error/result. Current R1 process tests use a healthy file and cannot establish this failure path; the invariant reviewer reproduced it with an isolated failing sink, corroborating pinned source.

The source of invalid output is the shared process subscriber, not the CLI result formatter. All native commands use that owner; embedded SDKs own their own process subscribers. DevServer's direct progress/notice writes already tolerate lost diagnostics by dropping failed writes rather than falling back onto reserved streams.

**Selected correction.** Set the installed builder's `log_internal_errors(false)` policy in the JSON subscriber branch. Continue ordinary requested event writes to the file. Preserve structured pre-command refusal when opening fails, normal-file logs and secret protection, command errors/exits, human stderr diagnostics, and dev child/readiness/wait/cleanup. Do not disable normal requested tracing, create another logger/sink framework/public option, scrub consumer strings or change operation success because a later diagnostic write fails. The dependency already provides the exact correction; only its JSON failure policy is missing.

**Focused proof.** Exercise the production subscriber construction with a portable writer that fails when an actual event is written after setup; establish clean complete machine streams and attempted ordinary event writes. Reuse a local subscriber/captured writer or existing process facilities without adding a public sink API or generic harness. Constructing a separate copy of the fixed builder solely in a test would mirror implementation and is insufficient. Retain normal-file whole-result/whole-error and no-secret assertions. A Linux failing device may corroborate locally but must not become a product/test prerequisite.

## FIND-TASK-002-3: helper docs moved to the wrong item

**Diagnosis.** CLI `tests/card_lifecycle.rs:882–908` inserts new `run_cli_logged -> Output` between the old JSON decoder rustdoc and `run_cli_json -> Result<Value,String>`. The logged helper incorrectly claims decoding/returned string errors; the actual decoder now lacks item docs. This insertion is an R1 regression at changed helpers, not a request to document unchanged tests. New public `src/lib.rs:95–136::install_tracing` also omits the actual `.init()` panic contract for an already installed global subscriber/log tracer. AGENTS.md §16 makes changed-item rustdoc an explicit gate even without a runtime failure caused by prose.

**Selected correction.** Restore accurate decoder purpose/Errors docs immediately on `run_cli_json`; leave the logged helper accurate environment/raw-output/Panics docs without a returned-error claim; document the one-time initializer's actual panic contract. Preserve bodies, assertions and subscriber behavior. Batch these bounded docs with the behavior fixes; do not add try_init behavior, error variants, a checker, a style-only round or an unrelated helper sweep.

**Focused proof.** Inspect those exact attached blocks against full bodies and the pinned dependency. Owning format/lint checks suffice; comments need no new runtime test.

## FIND-TASK-003-4: Rust span flush failure skips the independent row drain

**Diagnosis.** New `examples/support-desk-app/rust/lib.rs:121–135::SupportDesk::serve` awaits serving, then returns immediately on `telemetry.shutdown()?` before `state.shutdown().await`. The public Rust OTEL owner force-flushes its provider and returns the existing ClientConfigInvalid error on real exporter failure/timeout. The distinct WyrdState writer barrier is never attempted. Requests have independently admitted ticket rows; an OTLP-only failure does not establish that their sink cannot drain. Ordinary graceful exit can therefore lose buffered tickets that could have been settled.

This new method has no in-tree executable caller, but it is the explicitly required Rust recipe embedded in quickstart and Observe and expanded into llms-full. The existing canonical support-desk journey executes a different source. Compilation and the current page-text assertion, which literally requires the sequential short-circuit pair, do not prove this negative flow. This is a new R1 coordinator cause; old 003-1's rejected commands/resources/hidden startup are corrected.

Rust already has the right owners: application-held Telemetry and telemetry-free WyrdState. Python/TS shutdown siblings retain the telemetry outcome, always attempt native drain and then settle; the Rust app must compose its existing owners without importing managed-language error machinery or moving global telemetry into state.

**Selected correction.** After serving settles, always attempt both existing shutdown stages, telemetry before native drain, and settle their outcomes only after both attempts. Preserve current first-failure precedence: telemetry error, otherwise native drain error, otherwise the existing mapped serving error. Do not return success when any stage failed. Keep the actual telemetry handle local to serving, multi-thread runtime requirement, graceful HTTP handling and serving-error cleanup. Update lifecycle documentation and embedded source expectation to describe the actual attempts and truthful cancellation/loss behavior, then regenerate source-owned references. No new SDK API, combined error schema, runtime owner, retry loop, background flush or stronger durability contract is needed.

**Focused proof.** Exercise the actual application shutdown coordinator with a telemetry export failure while its independent writer has drainable work; observe the native drain settling before the same telemetry failure returns. Reuse existing Rust SDK runtime/server/mock-export facilities and checked-in-example inclusion pattern where appropriate. No new general harness or public SDK test API is justified. Retain normal success and serving-result paths. A test of a copied cleanup sequence, two source strings or the different canonical app cannot close this case.

## Evidence dependencies remain separate

- **FIND-TASK-002-4:** Linux arm64/macOS arm64 execution is absent. Source portability and Linux x86_64 runs do not supply it. Attach candidate/target/toolchain and exact focused existing CLI machine-output/readiness/recovery and relevant SDK pull/load results when capable hosts are available; otherwise retain BLOCKED. Do not add platform mechanisms/gates or weaken supported targets.
- **FIND-TASK-003-3:** compatible released-artifact acquisition/setup/quickstart execution is absent. The packet's dated observations and partial caveats remain honest; source/local-provider runs do not supply published-install proof. Record artifact identities and focused execution only when independently available, otherwise retain BLOCKED. This task authorizes no release, tag, version bump, package/image publication, provider run or new claim about current registries.

Code completion does not waive either proof dependency. TASK-004/live acceptance and final integrated change gates remain separately owned.

## Acceptance criteria

| Finding | Required outcome |
|---|---|
| FIND-TASK-002-1 | A post-open diagnostic sink failure never writes subscriber prose to JSON stdout/stderr; normal requested logs, structured opening errors, exit behavior and secrets policy remain |
| FIND-TASK-002-3 | Exact changed helpers/initializer have accurate attached operation/Errors/Panics docs; executable logic and assertions remain |
| FIND-TASK-003-4 | Telemetry failure still permits native drain to settle, then returns the correct existing failure; successful drain/serve behavior and public ownership remain |
| Evidence dependencies | Actual missing proof is attached or explicitly BLOCKED, without release/platform scope expansion |

## Verification and constraints

Follow Red–Green–Refactor for the two executable failure cases. The initial failed tests must expose the diagnosed stream/drain defects, not unrelated setup/compile errors. Confirm collected selectors and record/run each new named test's exact mise package/target/test command; do not invent selectors or count zero matches. All runtime tests stay in their owning language/runtime. Use the current owning build prerequisites before compiled-CLI proof.

Retain the narrow real-CLI normal-file proof:

```sh
mise exec -- scripts/postgres/with-test-postgres.sh -- cargo nextest run --locked -p wyrd-cli --test cli -E 'test(=card_lifecycle::pg_tests::json_results_and_errors_are_machine_readable)'
```

If subscriber/diagnostic ownership changes beyond the JSON builder policy, retain focused foreground readiness:

```sh
mise exec -- scripts/postgres/with-test-postgres.sh -- cargo nextest run --locked -p wyrd-cli --test cli -E 'test(=pg_tests::server_dev_first_run_is_client_ready)'
```

Compile the actual Rust example and check its expanded recipe:

```sh
mise exec -- cargo check --locked -p wyrd-rust-examples --lib
mise exec -- pnpm --dir docs exec vitest run src/lib/mdsvex/page-text.test.ts
mise run docs:check
```

These checks supplement the new negative runtime exercise; they do not replace it. If existing Rust SDK `integration` support owns that exercise, use only its new exact case with the repository Postgres wrapper when needed, not the whole journey suite. Do not add a framework or test-only public API to test a small application coordinator.

Run `mise run fmt`, `mise run lints`, `git diff --check`, and inspect tracked/untracked changes. Source-generated full references must be regenerated through their existing owner. No contract/SQL/Python/TS API change is prescribed, so codegen, dependency, tenant and complete language suites are not added speculatively. No `mise run gate` or full SDK/CLI journey aggregates here; final integrated review owns broad proof once.

Preserve all original declare/get/pull, auth/tenant/audit, bundle integrity/rollback, secret handling and foreground lifetime contracts. No compatibility aliases, new durable behavior, logging framework, SDK runtime, process manager, MCP write/check/upload, sibling-repository work, release or deployment. The later review remains cumulative against the original base and includes prior verdicts, ledgers, R1 and this task.
