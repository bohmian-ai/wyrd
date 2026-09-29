---
id: TASK-001-R2
kind: remediation
status: ready
spec: SPEC-verified-change-contract
spec_revision: 32
parent_task: TASK-001
remediates:
  - FIND-TASK-001-2
  - FIND-TASK-001-4
  - FIND-TASK-001-5
  - FIND-TASK-001-11
  - FIND-TASK-001-12
  - FIND-TASK-001-13
  - FIND-TASK-001-14
---

# TASK-001-R2 — Verifier contract closure

## Authority and immutable input

- Approved spec: `changes/active/verified-change-contract/spec.md`, revision 32
- Original task: `changes/active/verified-change-contract/tasks/TASK-001-verifier-contract-and-registration.md`
- Base: `5293546f33b3a5fd9de529098e23ea70d472c412`
- Reviewed cumulative candidate: `dd0503e7149017d760a987346e2838001e5c3429`
- Round-2 verdict and validated evidence: this directory's `verdict.md` and `findings-validation.md`

Use `$wyrd-implement`. Correct only the seven validated gaps below and preserve
the already-closed TASK-001 behavior.

## Outcome

TASK-001 has one deterministic, fully documented proof surface and one
internally resolvable public contract. Registration rejects duplicate and
forbidden bindings by canonical identity, every required denial is proved at
the appropriate server boundary, live documentation teaches only the Verifier
model, and required Bifrost gates do not depend on allocator addresses or an
instantaneous asynchronous cleanup sample.

## Diagnoses and required corrections

### `FIND-TASK-001-2` — server proof for nested inline-Agent refusal

The shared production validation now rejects bindings nested under an inline
Agent, but only pure contract tests prove the two rejected shapes. The prior
finding required proof across authenticated HTTP decoding, validation, stable
error mapping, and the no-write boundary. Without it, those seams may regress
while the existing unit tests remain green.

Add one case to the existing Postgres-backed card-registration test surface
using either already-covered nested shape. Authenticate through the existing
writer setup and prove HTTP 400, `WYRD_REGISTRY_400_INVALID_CARD_SPEC`, and no
registration writes through the existing no-write assertion. Keep both pure
shape tests and the production validation unchanged. Do not add a harness,
error code, validation path, or alternate reference walker.

### `FIND-TASK-001-4` — canonical duplicate identity and denial-code closure

Duplicate referenced Verifiers and Operators are keyed by rendered `CardRef`.
That representation includes the optional server-managed UID, while registry
resolution and the repository's canonical named identity intentionally ignore
it. A direct HTTP caller can therefore repeat one exact version with different
UID presentation. Equal inline Operator specs are not checked at all. The
under-privileged registration case also proves only status and no writes, not
the stable denial code required by the prior remediation.

Use the existing `CardRef::identity_key()` mechanism for referenced Verifier
and Operator identity. Use the existing typed equality of `OperatorSpec` for
repeated inline Operators; do not introduce serialization, hydration, hashes,
or a digest service solely for validation. Preserve mixed inline/reference
semantics, wrong-kind ordering, Workflow refusal, existing error codes, and
error details. Add focused coverage for UID-bearing versus UID-less copies of
one Verifier, the same referenced Operator, and two equal inline Operators.
Extend the existing denied server case to assert
`WYRD_PERMISSION_403_DENIED_RBAC` while retaining its no-write proof.

### `FIND-TASK-001-5` — retired Eval Card documentation

Live CLI remediation and public Rust module documentation still tells users to
author an Eval Card or call the deleted `Spec::Eval` variant. The executable
contract accepts only an eval-backed Verifier, so following that documentation
fails.

Update only the validated live descriptions in `wyrd-cli/src/error.rs`,
`wyrd-spec/src/card/eval.rs`, and `wyrd-spec/src/vala/eval/{mod,spec}.rs` to
describe `kind: Verifier` with `implementation.kind: eval`. Preserve stable
error codes/statuses, the `card::eval::EvalSpec` re-export, and all Eval engine
semantics. Do not modify the TASK-002-owned observation reference or unrelated
UI/mock wording.

### `FIND-TASK-001-11` — allocator-address restart assertions

Two cluster restart tests compare heap addresses after the old server is
dropped. Legal allocator reuse caused the recorded failure, and unequal
addresses would not prove that replacement pools work.

Remove the test-only pool-address identity surface and both comparisons. Prove
replacement through the existing restart owner and real application/Vala pool
operations. Retain stopped-node absence, successful restart, survivor
usability and ownership, WAL-root retention, resource-plan re-derivation,
resource-state reset, and credential-exchange assertions. Do not add a
generation token, retry wrapper, fixed sleep, or new fixture abstraction.

### `FIND-TASK-001-12` — Oracle asynchronous settlement

The distributed Oracle journey samples ownership once immediately after a
terminal response, even though this runtime explicitly settles graph ownership
asynchronously. The recorded failure is therefore a race, while the invariant
that every reservation and peer counter reaches zero remains required.

Reuse Tokio time and the existing Oracle inspection owner to observe settlement
under one bounded deadline. Success still requires every existing ownership
field to equal zero; deadline failure must expose the final nonzero snapshot.
Do not weaken or delete the invariant, add an unbounded wait, use a one-shot
sleep, or create a general wait abstraction for this single caller.

### `FIND-TASK-001-13` — panic contracts on remediation tests and helpers

The round-1 remediation added or materially changed panicking tests and fixture
owners without the rustdoc and `# Panics` sections required by repository
authority. Runtime success and lint success do not establish that hard
documentation rule.

Document intent and the actual panic conditions for every item enumerated in
the validated ledger, including the new registration test, modified ingest
test, and adjacent seed owners. Apply the same mechanical audit to every Rust
item added by the round-1 remediation commits so the bounded correction is
complete. Preserve assertions and fixture behavior; do not convert test
invariants into production-style Results and do not add a new documentation
check.

### `FIND-TASK-001-14` — unresolved Verifier OpenAPI components

The generated OpenAPI references `VerificationBinding`,
`VerifierImplementation`, and `TriggerActivation` from live Card roots but
defines none of them. Artifact regeneration reproduces the broken document; it
does not prove reference closure.

Extend the existing utoipa/schema-registration owner so those reachable types
and their required dependencies are registered through their existing schema
implementations. Do not create a second schema model or hand-edit generated
OpenAPI. Add one focused OpenAPI test rooted at the new Verifier, binding, and
Trigger surfaces that recursively proves their referenced components exist.
The test need not clean unrelated pre-existing OpenAPI roots.

## Constraints and preserved behavior

- Preserve approved spec revision 32 and every closed round-1 finding.
- Preserve the 15-kind Card catalog, closed Drift/Eval implementation union,
  existing payload semantics, canonical visitor, and registration transaction
  ordering.
- Preserve tenant isolation, authorization, stable errors, no-write refusals,
  table/protocol retirement, generated-source ownership, and all first-class
  language projections.
- Do not add compatibility aliases, Card kinds, Verifier implementations,
  permissions, persistent state, dependencies, retry infrastructure, or test
  waivers.
- Do not weaken validation, durability, cleanup, or test assertions to obtain a
  green lane.

## Acceptance criteria

| Finding | Acceptance criterion |
|---|---|
| `FIND-TASK-001-2` | An authenticated real-server nested inline-Agent registration is refused with the existing stable invalid-card-spec code and produces no writes. |
| `FIND-TASK-001-4` | UID presentation cannot bypass referenced duplicate rejection; equal inline Operators are rejected; the under-privileged server case asserts its stable denial code and no writes. |
| `FIND-TASK-001-5` | The validated CLI and public Rust surfaces teach only an eval-backed Verifier and contain no promise of `Spec::Eval` or a registrable Eval Card. |
| `FIND-TASK-001-11` | Both restart scenarios prove usable replacement pools and retained lifecycle invariants without pointer/address identity. |
| `FIND-TASK-001-12` | The Oracle journey waits within a bound for the exact zero-ownership invariant and reports the final state on timeout. |
| `FIND-TASK-001-13` | Every new/materially changed panicking Rust item in the bounded remediation diff documents intent and its panic contract. |
| `FIND-TASK-001-14` | Every component reference reachable from the new Verifier/binding/Trigger OpenAPI roots resolves to a generated component. |

## Focused and broader proof

Run every new or changed test by its exact repository-pinned `mise exec --
cargo nextest run --locked` expression and record the selected test name and
result. In particular, exercise the existing registration refusal target, the
two cluster restart tests repeatedly, and the distributed Oracle journey
repeatedly before running their owners' broader lanes. A passing retry after a
failure is not closure; diagnose and correct any encountered failure.

Then run the narrowest complete owner set:

```bash
mise run fmt
mise run lints
mise run test:shared
mise run test:cards:integration
mise run test:bifrost:journey:oracle
mise run test:bifrost
mise run test:wyrd
mise run codegen:check
mise run docs:check
mise run check:client-tier
mise run check:pyo3-scope
git diff --check
```

Also rerun Python or TypeScript lanes only if implementation changes their
source or generated projections. Record RED, GREEN, and final verification in
this task without treating an unrelated or pre-existing failure as a waiver.

## Non-goals

- Later TASK-002 through TASK-008 runtime, SDK, MCP, Operator, or integrated
  journey work.
- Unrelated OpenAPI reference cleanup.
- New product behavior, schema vocabulary, persistent data, concurrency
  semantics, or compatibility paths.
- Refactoring adjacent test infrastructure beyond the validated root causes.

## Implementation evidence

Commits `6f59da966..b612e263e` on `verified-change-contract`, applied on top of
the reviewed candidate's worktree HEAD `5f14f3c32`.

| Acceptance criterion | Implementation evidence | Verification evidence | Result |
|---|---|---|---|
| `FIND-TASK-001-2` | `crates/wyrd/wyrd-server/tests/pg_card_registration_route.rs` — a fifth case in `referenced_binding_refusals_leave_no_writes` submits a Workflow whose step inlines an Agent carrying `verified_by`, through the existing writer JWT and `request_with_body`, asserting HTTP 400, `WYRD_REGISTRY_400_INVALID_CARD_SPEC`, and the existing `assert_no_registration_writes`. No new harness, helper, error code, or validation path. | `scripts/postgres/with-test-postgres.sh -- bash -lc 'mise run db:migrate:all:inner && WYRD_REG_E2E=1 mise exec -- cargo nextest run --locked -p wyrd-server --test pg_card_registration_route -E "test(=referenced_binding_refusals_leave_no_writes)"'` → 1 passed. `mise run test:cards:integration` → 24 passed, 0 failed. | PASS |
| `FIND-TASK-001-4` | `crates/wyrd-spec/src/graph/composition.rs` — `binding_validation_errors` keys referenced Verifiers on `CardRef::identity_key()`; `check_on_failure` keys referenced Operators the same way and rejects a repeated inline body by existing `OperatorSpec: PartialEq`. Mixed inline/reference semantics, wrong-kind ordering, Workflow refusal, error codes, and details are unchanged. Denial-code proof added to `referenced_binding_refusals_leave_no_writes`. | RED recorded first: with `to_string()` keys and the inline check disabled, the three new tests failed (`2 failed`/`3 failed` run: `binding_validation_rejects_duplicate_verifier_differing_only_by_uid`, `..._on_failure_operator_differing_only_by_uid`, `..._duplicate_inline_on_failure_operator`) while `binding_validation_rejects_duplicate_on_failure_operator` still passed. GREEN: `mise exec -- cargo nextest run --locked -p wyrd-spec --lib -E 'test(/graph::composition::tests::binding_validation/)'` → 8 passed. `mise run test:shared` → 664 passed. `mise run test:cards:integration` → 24 passed (includes the `WYRD_PERMISSION_403_DENIED_RBAC` assertion). | PASS |
| `FIND-TASK-001-5` | `crates/wyrd/wyrd-cli/src/error.rs` (both remediations), `crates/wyrd-spec/src/card/eval.rs`, `crates/wyrd-spec/src/vala/eval/spec.rs`, `crates/wyrd-spec/src/vala/eval/mod.rs` now describe `kind: Verifier` with `implementation.kind: eval`. Stable codes/statuses, the `card::eval::EvalSpec` re-export, and Eval engine semantics are untouched; the TASK-002-owned `EvalRecordObservation.eval_ref` surface was not modified. | `grep -rn -e 'Spec::Eval' -e 'Eval card' -e 'Eval Card'` over those four files is empty. `mise run docs:check` → OK, 60 pages. `mise run codegen:check` → All checks passed. | PASS |
| `FIND-TASK-001-11` | `crates/wyrd/wyrd-testing/src/server.rs` — `PostgresPoolIdentity` and `postgres_pool_identity` deleted (their only callers were the two flaky assertions). `crates/wyrd/wyrd-testing/src/bifrost/cluster.rs` — both tests now exercise the replacement's application and Vala pools with a real query; stopped-node absence, restart success, survivor usability, retained WAL root, re-derived resource plan, reset resource state, and the credential-exchange assertions are retained. No generation token, retry wrapper, sleep, or new fixture. | `mise exec -- cargo nextest run --locked -p wyrd-testing --lib --features wyrd-server/test-support -E "test(=bifrost::cluster::tests::cluster_restart_rederives_same_plan_from_retained_snapshot) \| test(=bifrost::cluster::tests::process_pool_lifecycle_isolated_across_restart)" --retries 0` under `with-test-postgres.sh`, three consecutive passes, no failure and no retry. `mise run test:wyrd` → 1979 passed. | PASS |
| `FIND-TASK-001-12` | `crates/wyrd/wyrd-testing/tests/bifrost/oracle/distributed.rs` — the journey re-reads the existing `cluster.oracle_inspection()` under `SETTLEMENT_POLLS`/`SETTLEMENT_INTERVAL` (300 × 100 ms, the same bounded-poll shape the capacity journey already uses for settled physical evidence) and still requires every one of `active_queries`, `queued_queries`, `reserved_memory_bytes`, `reserved_spill_bytes`, `peer_pending`, `peer_running` to be zero, reporting the final snapshot on deadline. No general wait abstraction, one-shot sleep, or weakened assertion. | `mise exec -- cargo nextest run --locked -p wyrd-testing --test oracle -P journey --run-ignored=all -E "test(=distributed::pg_bifrost_selective_predicate_and_projection_prune_distributed_reads)" --retries 0`, three consecutive passes, no failure and no retry. | PASS |
| `FIND-TASK-001-13` | `# Panics` sections added to every ledger-named item: `wyrd-loader/src/parse.rs`, `wyrd-spec/src/card/trigger.rs` (4 tests), `wyrd-spec/src/card/mod.rs`, `wyrd-spec/src/graph/composition.rs` (fixtures + tests), and `seed_dependency`/`seed_card`; intent rustdoc plus `# Panics` added to `pg_grpc_ingest_smoke.rs::ingest_valid_token_is_not_rejected_as_unauthenticated` and to `referenced_binding_refusals_leave_no_writes`. `VerifierImplementation`'s `schema`, `name`, and `schemas` received intent rustdoc. Assertions and fixture behavior are unchanged; no Result conversion and no new documentation check. | A mechanical audit script over `e22899bae..dd0503e71` (every added `fn`/`struct`/`enum`/`const`/`type`, checked for a doc block and for `# Panics` whenever the body can panic) reports zero remaining items. `mise run fmt`; `mise run lints` → clean at `--workspace --all-features --all-targets -D warnings`. | PASS |
| `FIND-TASK-001-14` | `crates/wyrd/wyrd-server/src/http/openapi.rs` — `TriggerActivation`, `VerificationBinding`, and `VerifierImplementation` registered in the existing `components(schemas(...))` list through their existing derives. `crates/wyrd-spec/src/card/verifier.rs` — `VerifierImplementation::schemas` now also forwards `<DriftSpec as ToSchema>::schemas`, so the Drift branch's dependency closure (e.g. `DriftSignal`) is defined. `openapi.yaml` regenerated through `mise run codegen:openapi`; purely additive. | RED recorded first: the new test failed with `DriftSignal is reachable from the Verifier contract but is not a defined component`. GREEN: `mise exec -- cargo nextest run --locked -p wyrd-server --lib -E 'test(=http::openapi::verifier_reference_closure_tests::verifier_contract_component_references_all_resolve)'` → 1 passed. `mise run codegen:check` → All checks passed. `mise run docs:check` → OK. | PASS |

### Broader verification

| Command | Result |
|---|---|
| `mise run fmt` | clean |
| `mise run lints` | clean |
| `mise run test:shared` | 664 passed, 0 failed |
| `mise run test:cards:integration` | 24 passed, 0 failed |
| `mise run test:bifrost:journey:oracle` (focused, ×3) | passed, no retry |
| `mise run test:wyrd` | 1979 passed, 0 failed |
| `mise run test:bifrost` | 9/9 lanes passed, exit 0 |
| `mise run codegen:check` | All checks passed |
| `mise run docs:check` | 60 pages, contrast AA verified |
| `mise run check:client-tier` | clean |
| `mise run check:pyo3-scope` | clean |
| `git diff --check` | clean |

No encountered failure was accepted by retry: the two restart tests and the
distributed Oracle journey each ran three consecutive times with `--retries 0`
and passed every time.

### Non-goals and scope

No TASK-002…TASK-008 surface was touched. No unrelated OpenAPI root was
cleaned. No Card kind, Verifier implementation, permission, persistent state,
dependency, compatibility alias, retry infrastructure, or test waiver was
added. Python and TypeScript lanes were not rerun because no SDK source or
generated projection changed (`git diff --stat dd0503e71..HEAD -- sdks/ docs/`
is empty and `codegen:check` reports no drift).

### Addendum — named proofs rerun through the pinned toolchain (`FIND-TASK-001-17`)

The three named proof groups above were originally recorded with raw Cargo.
They were rerun unchanged except for the mandatory
`mise exec -- cargo nextest run --locked` form, retaining every package,
target, feature, selector, profile, ignored flag, environment owner, and
`--retries 0`.

| Proof group | Exact command | Result |
|---|---|---|
| Registration route (`FIND-TASK-001-2`, `FIND-TASK-001-4`) | `scripts/postgres/with-test-postgres.sh -- bash -lc 'mise run db:migrate:all:inner && WYRD_REG_E2E=1 mise exec -- cargo nextest run --locked -p wyrd-server --test pg_card_registration_route -E "test(=referenced_binding_refusals_leave_no_writes)" --retries 0'` | 1 passed, 0 failed |
| Restart lifecycle (`FIND-TASK-001-11`), ×3 | `scripts/postgres/with-test-postgres.sh -- bash -lc 'mise exec -- cargo nextest run --locked -p wyrd-testing --lib --features wyrd-server/test-support -E "test(=bifrost::cluster::tests::cluster_restart_rederives_same_plan_from_retained_snapshot) \| test(=bifrost::cluster::tests::process_pool_lifecycle_isolated_across_restart)" --retries 0'` | 2 passed, 0 failed on each of three consecutive runs, no retry |
| Oracle journey (`FIND-TASK-001-12`), ×3 | `scripts/postgres/with-test-postgres.sh -- bash -lc 'mise run db:migrate:inner && mise exec -- cargo nextest run --locked -p wyrd-testing --test oracle -P journey --run-ignored=all -E "test(=distributed::pg_bifrost_selective_predicate_and_projection_prune_distributed_reads)" --retries 0'` | 1 passed, 0 failed on each of three consecutive runs, no retry |

No broad lane was rerun for this correction; it changed evidence only.
