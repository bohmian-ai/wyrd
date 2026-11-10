# Shared verification setup

The tasks specify exact planned selectors and their assertions. This file owns repeated setup/check commands only. No planned test is claimed to exist or pass. After adding a test, confirm its actual selector with the owning runner before RED; update the task if the test is split or moved. Keep focused tests separate when they assert different behavior. Existing negative/regression cases may already pass; record that baseline rather than manufacturing a failure. The new capability's missing assertion must produce RED before its GREEN implementation.

## Existing baseline and TASK-007 extension

The revision-24 GitHub amendment replaces deprecated TASK-004 in full. Its
connection proof starts from the deployment-owned App and both browser entry
points, never tenant-provided App credentials. TASK-005 consumes its verified
connection and scoped provider adapter; TASK-006 starts its combined story from
that onboarding. Keep generic external revision `9fdf6dc` and verify its immutable
descendant; do not restore the rejected Wyrd implementation. GitHub setup/OAuth
protocol controls stay in Rust/browser integration tests. SDKs expose typed
start/status/control and do not grow server fault controls or copied callbacks.

Implemented TASK-001 is historical baseline; R5 executes R4’s retained
foundation correction/proof contract after replacing its rejected implementation. Tasks 002–006 extend that remediated foundation.
TASK-007 adds the approved SubmissionSink-owned resume admission and suspended
workflow lifecycle under the new supplement; it cannot proceed while its
remaining material contract decisions are open. All new selectors are planned unless
explicitly confirmed existing. Planning executes no runtime proof. Use narrow
scenario tests during each task, then combined journeys at change review.

Both direct and queued execution use the common loader/checkpoint lifecycle.
Direct stays queue-free. Count application PostgreSQL invocation/publication
payload reads/writes separately from queue coordination, authorization/registry,
shared upload/session and Bifrost catalog/fence metadata. Do not assert zero
checkpoint writes for direct work. Recovery must never interpret unavailable
accepted state as absent or silently revive rejected WAL progress.

Public closure preserves feature grants, stable pages, replay conflicts, exact
File associations and selected terminal outcome. Independent delivery and
schedule/group/library lifetimes remain intact. No new archive service, GC
protocol, synthetic load or per-task complete gate.

## Revision-22 TASK-003 proof boundaries

Server crash, lease expiry and replica takeover stay in Rust real-server
journeys. Keep SDK public-contract tests on every shipped surface, but do not
add Python/TypeScript harness lifecycle controls to duplicate shared server
fault injection. TASK-003 scenarios 5–6 own exact SDK cancelled-wait errors and
the forced server cancellation-writer race. Static scope corrections do not
recertify historical results. Existing shared process/per-execution admission
and tenant claim rotation remain required; no per-tenant quota/reservation or
acceptance-time limits snapshot is added.

## Existing targets and setup

| Proof | Inspected existing owner |
|---|---|
| Rust SDK journey | Package wyrd-ai, sdks/wyrd-sdk-rust/Cargo.toml, existing tests/integration/main.rs, nextest journey profile; public SDK and in-process CLI against WyrdTestServer. Register new stories in the existing explicit journey lists where required. |
| Server seam | Package wyrd-server, tests/integration/main.rs, existing integration target; test-support enables current fault controls. Tests use real SQL/storage and production owners, not mocked publication/engine behavior. |
| Cluster | Package wyrd-testing, Cargo.toml server target, verification_runtime module and WyrdTestCluster; real Postgres and role-separated/multi-replica workers. |
| Bifrost acceptance/restart | Package wyrd-testing, existing scribe and oracle targets; lifecycle/source-boundary/distributed modules cover retained-volume WAL recovery and partial-source analytical behavior. Recovery-specific assertions extend these owners without changing unrelated analytical query semantics. |
| Engine | Package vala-verify, --lib, blocks/engine/tests.rs scripted ActionExecutor/CheckExecutor; controlled futures expose readiness, call order and limits. |
| Library | bohmian-code-review-pipeline Cargo.toml pipeline_journey target and test:pipeline:journey mise task; real public facade/stages with fake GitHub and injected stores. Run in its checkout/toolchain. Wyrd adapter integration additionally needs real Wyrd SQL/storage. |
| Python | Existing sdks/wyrd-sdk-python integration/pytest setup, top-level tests, public imports and native lifetime proof. |
| TypeScript | Existing sdks/wyrd-sdk-ts/wyrd Vitest integration and @wyrd/testing server harness; public imports and Node lifetime proof. |
| MCP/HTTP | Existing wyrd-mcp mcp target verification::pg_tests drives a real MCP client/server; server route/OpenAPI tests prove shared HTTP payload/error/scopes. |
| UI | Existing ui-pages.integration.test.ts boots production BFF/Playwright against WyrdTestServer; set WYRD_UI_INTEGRATION=1. Reuse current browser install/host prerequisites. |

Run prerequisites for affected surfaces once, sequentially:

```bash
mise run build:server
mise run py:setup
mise run ts:build
mise run ts:build:testing
```

For UI proof also build/check its existing package:

```bash
mise exec -- pnpm --dir crates/wyrd/wyrd-server/wyrd-ui build
mise exec -- pnpm --dir crates/wyrd/wyrd-server/wyrd-ui check
```

Each SQL/server test command in a task includes the repository Postgres lifecycle wrapper and migrations. Fixtures own isolated tenant databases as required by TESTING.md. Do not replace them with globally serialized shared state. Cargo-backed commands sharing target/ run sequentially; later tokens in `mise run task args` are arguments, not other tasks.

## Owner checks

Run only checks relevant to the task's changed owners, each as its own invocation. Do not run gate or complete language journey suites independently for every task.

Rust/contracts/server/SQL:

```bash
mise run fmt
mise run lints
mise run codegen:check
mise run check:deps
mise run check:tenant-isolation
```

Python projection/stubs:

```bash
mise run py:lints
mise run py:typecheck
```

TypeScript/native projection:

```bash
mise run ts:format:check
mise run ts:lints
mise run ts:typecheck
mise run ts:napi:check
```

Regenerate from owners, never hand-edit generated output. Public contract cases assert exact stable errors, typed fields, permissions and discoverability; positive-only smoke tests do not close negative journeys. Native Python/Node lifetime behavior is tested in its language, not emulated in Rust.

## Volume and integrated proof

Use bounded representative datasets, deterministic blocked providers/IO and real scoped SQL plan assertions. Assert maximum active/staged work, page/batch boundaries, acquisition counts, persisted spend and unrelated-tenant/repository progress. Never introduce synthetic host load or unsupported latency/QPS thresholds. Existing opt-in verification benchmarks may measure a final candidate, but are not task gates or evidence until run.

For existing queue geometry, preserve this exact confirmed test and add equivalent focused assertions for new backlog-sensitive queries (record their actual names/commands before RED):

```bash
mise exec -- scripts/postgres/with-test-postgres.sh -- cargo nextest run --locked -p wyrd-sql --lib --test-threads=1 -E 'test(=queries::verifier_executions::pg_tests::runnable_scan_walks_the_claim_indexes_however_large_the_backlog)'
```

At integrated change review, run the affected capability/language journeys, boundary/codegen/docsite/example checks once for the combined write set and required cross-platform/deployment closure. Upstream pipeline/provider delivery evidence and immutable dependency revision are required. CI/scheduling/commits/pushes are caller-owned. Planning runs none of these behavioral tests and makes no throughput claim.
