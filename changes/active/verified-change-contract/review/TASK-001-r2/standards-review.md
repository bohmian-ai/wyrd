# TASK-001 r2 — Repository-standards review (`repo-rev`)

Subject: base `5293546f33b3a5fd9de529098e23ea70d472c412` → cumulative
candidate `dd0503e7149017d760a987346e2838001e5c3429`.

The worktree is at `5f14f3c325cc8c45081fe9ad0543e7794b4a4e0f`, whose only
change is the out-of-subject process rule in `AGENTS.md` that pre-existing test
failures and flaky assertions are not waivers. The reviewed source remained
immutable. This report audits repository-rule compliance only; it does not
reassess TASK-001 product acceptance.

## Authority coverage

| Changed surface | Applicable authority read and applied |
|---|---|
| Card vocabulary, Verifier envelope/spec, `CardRef`, graph/reference traversal, schemas | `AGENTS.md` §§2–5, 9, 12, 15–16; `architecture/agent-rules.md`; `architecture/wyrd-design.md`; `architecture/wyrd-doctrine.mdx`; `architecture/references/doctrine/{positioning-and-vocabulary,architecture-constraints}.md`; `architecture/references/architecture/patterns.md`; `architecture/references/languages/{rust-core,errors}.md` |
| Loader, shared client state, Rust/Python state projection | `AGENTS.md` §§2–8, 11–12, 16; `agent-rules.md`; `rust-core.md`; `pyo3-boundaries.md`; `python-api-and-stubs.md`; client-tier and PyO3 boundary rules |
| Server registration, reference resolution, auth smoke tests, Postgres migration | `AGENTS.md` §§3–6, 9, 11–12, 15–16; `agent-rules.md` connection/RLS, audit, struct-owner, import, rustdoc, and test-placement rules; `architecture/patterns.md`; `languages/errors.md`; `languages/testing-workflows.md` |
| Vala drift/eval engines, Bifrost table catalog, table-removal migration | `AGENTS.md` §§2–6, 10–12, 15–16; `architecture/bifrost-design.md`; `domain/{vala-architecture,evaluation,drift-monitoring,olap-serving,analytical-operations-reliability}.md` |
| CLI behavior, fixtures, and user journeys | `AGENTS.md` §§2, 4, 9, 11–12, 16; `languages/{errors,testing-workflows}.md`; `agent-rules.md` journey and test-integrity rules |
| Python stubs/tests and TypeScript generated error catalog | `AGENTS.md` §§7–8, 11–12; `languages/{pyo3-boundaries,python-api-and-stubs,typescript-guide}.md`; generated-artifact rule |
| UI types, component, mock, and test | `AGENTS.md` §§2–3, 11–12; `wyrd-design.md`; `wyrd-doctrine.mdx`; UI remains a projection rather than authority |
| Architecture and product docs, OpenAPI, JSON schemas, `.pyi`, generated docs | authority hierarchy in `architecture/references/README.md`; `AGENTS.md` §§2, 8–12; `agent-rules.md` generated-artifact rule; `docs:check` and `codegen:check` |
| Root manifests, lockfile, `mise.toml`, CI change detection, test-family retirement | `AGENTS.md` §§1, 4, 11–12; `agent-rules.md`; `languages/{implementation-execution,testing-workflows,spec-driven-development}.md` |
| Active task/review packet and remediation evidence | `AGENTS.md` §14; `languages/{spec-driven-development,implementation-execution}.md`; `wyrd-task-review` Wave 1 repository-review contract |

## Rule results

| Repository rule | Result | Exact evidence |
|---|---|---|
| Wyrd vocabulary and authority alignment | PASS | `CardKind::Verifier` and `Spec::Verifier` replace registrable Drift/Eval kinds; the design, doctrine, focused references, docs, schemas, CLI, UI, and SDK projections consistently describe 15 native kinds. Drift and Eval remain typed Verifier implementations. |
| Ownership and client/server boundaries | PASS | Durable registration remains in `wyrd-server`; pure contracts and validation remain in `wyrd-spec`; the Python change is a thin state projection; the candidate removes, rather than adds, client-side eval transport. Reported `check:client-tier` and `check:pyo3-scope` both passed. |
| Struct-centered Rust and sync/async boundaries | PASS | Remediation replaces dependency-threading `validate_effective_bindings` with state-owning `EffectiveSpecs::{new,validate_bindings,validate_binding,load}` in `cards/resolve.rs:114-246`; async methods directly await registry IO. Pure binding enumeration and validation remain synchronous on `Spec`. |
| Imports and bare signature types | PASS | Remediation commit `f00cf963d` hoists `utoipa`, `Cow`, reference, and verifier types to module imports and removes the round-1 fully qualified signature paths. Reported `fmt` and `lints` passed. |
| Public stable errors and removed compatibility surface | PASS | The seven unreachable CLI variants and retired eval-run wire types/schemas are deleted; remaining public errors use the derive-backed catalog. `git grep` evidence in the remediation record is empty for `EvalRunOpen`, `LeaseToken`, `ServerRequiresAgentUrl`, and stale publisher vocabulary; reported codegen/docs checks passed. |
| `wyrd-spec` foundational boundary | PASS | No IO, async, SQL, network, or PyO3 dependency enters `wyrd-spec`; all new contract checks are pure. |
| SQL tenancy and migration rules | PASS | The cumulative diff adds only append-only migrations; server reference reads stay behind `TenantConn`; no callee commit/rollback or manual tenant predicate was added. The test-only direct inserts use the existing superuser fixture pattern. Reported `test:sql` passed. |
| Generated artifacts | PASS | Source generators change with schemas/OpenAPI/stubs/error catalog; reported `codegen:check` and `docs:check` passed. No evidence of a hand-edited generated contract was found. |
| Retiring unreachable tests/checks and keeping lanes valid | PASS | Remediation commit `d52918f8e` removes the deleted `pg_eval_v1_protocol` target from both `mise.toml:256` and `.github/scripts/detect-changes.sh:64`; reported `test:bifrost:integration:server` and the nine-lane `test:bifrost` aggregate passed. |
| No plan/task references in production code | PASS | The round-1 task-reference sentence was removed; `git grep -n -i 'task verification' -- crates` is empty. Domain uses of “plan” remain ordinary planner vocabulary. |
| Test tier and journey placement | PASS | Pure strict-decoding and binding checks are inline unit tests; registry/RLS refusals remain in `pg_card_registration_route`; CLI and Python journeys exercise public surfaces. |
| Gate integrity and completion treatment of encountered failures | **FAIL** | **SR2-1.** The remediation record explicitly accepts a logically invalid flaky assertion after rerun instead of correcting it. |
| Rustdoc for every new/materially modified Rust item, including tests, and `# Panics` where applicable | **FAIL** | **SR2-2.** Multiple new remediation tests document intent but omit their panic contract, and one materially modified test has no rustdoc at all. This is a hard blocker under `AGENTS.md` §16 and `agent-rules.md`. |
| Python test form | PASS | Changed Python tests remain top-level `def test_*`; no class-based tests were added. |
| Git identity for subject commits | PASS | Every commit in `5293546f3..dd0503e71` is authored by `Thorrester <sjforrester32@gmail.com>` and contains no AI co-author trailer. The out-of-subject `5f14f3c32` commit is not part of the reviewed candidate. |

## Material findings

### SR2-1 — VIOLATION: a known allocator-address flake is accepted as green after rerun

- **Rule:** `AGENTS.md` §12, including the `5f14f3c32` process clarification:
  a pre-existing failing lane or flaky assertion is not a waiver and must be
  fixed in the same change; `architecture/references/languages/implementation-execution.md`
  forbids weakening assertions and requires credible proof.
- **Location:**
  `crates/wyrd/wyrd-testing/src/bifrost/cluster.rs:2750-2778`, especially
  `assert_ne!(server.postgres_pool_identity(), original_identity)`; the value is
  produced by `crates/wyrd/wyrd-testing/src/server.rs:2923-2930`, which casts the
  addresses of two `PgPool` wrapper fields to `usize`.
- **Evidence:** TASK-001-R1's committed implementation record says
  `cluster_restart_rederives_same_plan_from_retained_snapshot` failed because
  “allocator reused the addresses,” calls the behavior pre-existing, leaves it
  untouched, and treats the aggregate's passing rerun as closure. Once the old
  server is dropped, the allocator may legally reuse both addresses; pointer
  inequality therefore cannot prove that restart constructed fresh pools.
- **Consequence:** `mise run test:bifrost` remains nondeterministic, and a green
  rerun does not establish the lifecycle invariant the assertion claims.
- **Testable correction:** replace the heap-address observation with a stable,
  lifecycle-owned proof of the intended restart invariant (or remove the
  identity assertion only if fresh pool identity is not an actual required
  behavior), then run the exact test repeatedly and the complete
  `mise run test:bifrost` lane. Do not add sleeps or merely retry the assertion.

### SR2-2 — VIOLATION: remediation tests do not satisfy the mandatory rustdoc contract

- **Rule:** `AGENTS.md` §16 and `architecture/agent-rules.md`: every new or
  materially modified Rust item, including test helpers and test functions,
  requires rustdoc describing intent/workflow/invariants; a possible panic
  requires `# Panics`. Missing documentation is `BLOCK_BEFORE_MERGE`.
- **Locations and evidence:**
  - `crates/wyrd/wyrd-server/tests/pg_grpc_ingest_smoke.rs:537` is materially
    modified by the tenant seed but has no rustdoc or `# Panics`, despite its
    `expect` calls and assertions.
  - New remediation tests at
    `crates/shared/wyrd-loader/src/parse.rs:420`,
    `crates/wyrd-spec/src/card/trigger.rs:62,89,100,115`,
    `crates/wyrd-spec/src/card/mod.rs:2670`, and
    `crates/wyrd-spec/src/graph/composition.rs:569,582,597,613,630,657,683,718`
    have intent rustdoc but no `# Panics`, while each can panic through
    `unwrap`/`expect`, assertions, or both.
  - `crates/wyrd/wyrd-server/tests/pg_card_registration_route.rs:906` is a new
    integration test with many `expect` calls, explicit `panic!`, and
    assertions, but its rustdoc has no `# Panics` section.
- **Consequence:** the cumulative candidate violates a repository hard blocker
  even though formatting, linting, and runtime tests pass; those tools do not
  enforce this repository-specific documentation rule.
- **Testable correction:** add accurate `# Panics` sections to every new or
  materially modified panicking Rust test/helper in the cumulative diff, and
  add full intent rustdoc plus `# Panics` to the modified ingest test. Re-audit
  all touched Rust items, then run `mise run fmt` and `mise run lints`.

## Tenant-seeding decision

The `pg_grpc_ingest_smoke.rs:543-545` change is a correct fixture repair, not a
weakened assertion: `tenant_admits_credentials` makes an unseeded tenant's
token invalid, and the existing `seed_tenant` owner supplies the production
precondition before the test asserts that a valid token is not rejected as
unauthenticated. The other custom tenants in this test target are already
seeded at `:602` and `:866-867`; the other auth route tests mint against
`server.data_tenant_id()`, which the harness bootstraps. No additional latent
unseeded-tenant finding was found in the inspected server tests.

## Prior standards-finding closure

| Round-1 source finding | Closure |
|---|---|
| SR-1 stale Bifrost target | CLOSED by `d52918f8e`; lane and path regex no longer name the deleted target. |
| SR-2 task reference in code | CLOSED; sentence removed and grep clean. |
| SR-3 rustdoc on listed production items | CLOSED for the listed production items, but remediation introduced the distinct test-documentation defect SR2-2. |
| SR-4 dependency-threading free workflow | CLOSED by the state-owning `EffectiveSpecs` implementation. |
| SR-5 imports / qualified signatures | CLOSED by `f00cf963d`. |
| SR-6 unreachable CLI error variants | CLOSED by deletion and regeneration. |
| SR-7 inaccurate Trigger invariant doc | CLOSED; documentation now points to registration binding validation. |
| SR-8 residual vocabulary | CLOSED for the retained validated scope: binding-owner names and generated/docs vocabulary are updated. |

## Verification limits

The committed remediation record reports all requested focused tests and the
following lanes exit 0: `fmt`, `lints`, `codegen:check`, `docs:check`,
`check:client-tier`, `check:pyo3-scope`, `test:shared`, `test:cards:unit`,
`test:cards:integration`, `test:cli:journey`, `test:wyrdstate:journey`,
`test:sql`, `test:bifrost`, `test:wyrd`, Python unit/type checks, TypeScript
unit/type checks, and the focused Vala table test. This reviewer did not rerun
the expensive lanes. The same record discloses the first-run allocator-address
failure, so the final green aggregate cannot cure SR2-1. `git diff --check`
passes. Generated-artifact provenance is supported by the changed generators
and green drift checks, not by reviewer regeneration.

## Overall result

**FAIL.** The remediation closes the eight prior repository-standards findings,
but SR2-1 leaves a known nondeterministic assertion in a required lane and SR2-2
violates the hard Rust documentation rule. Both are bounded implementation
corrections; neither requires a specification revision.
