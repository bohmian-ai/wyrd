# TASK-001 round 2 — Wave 2 findings validation

## Immutable subject and coverage

- Repository: `/Users/stevenforrester/Documents/GitHub/verified-change-contract`
- Base: `5293546f33b3a5fd9de529098e23ea70d472c412`
- Cumulative candidate: `dd0503e7149017d760a987346e2838001e5c3429`
- Current HEAD: `5f14f3c325cc8c45081fe9ad0543e7794b4a4e0f`; its only tracked
  difference from the candidate is the current `AGENTS.md` process authority.
- Approved specification: `changes/active/verified-change-contract/spec.md`,
  revision 32.
- Original task, prior round-1 verdict and ledger, and the round-1 remediation
  task were read with the complete `5293546f3..dd0503e71` diff.
- Wave 1 inputs: `task-review.md`, `standards-review.md`,
  `domain-review-security-tenancy.md`, `domain-review-data-durability.md`, and
  `domain-review-contracts.md` in this directory.

There is no `.codegraph/` index in this worktree. The candidate object and its
base were resolvable at the start and end of validation, and
`git diff --quiet dd0503e71 5f14f3c32 -- . ':(exclude)AGENTS.md'` confirmed that
the reviewed source still matches the immutable candidate.

## Wave 1 proposal validation

| Source proposal | Result | Independent validation and smallest correction boundary |
|---|---|---|
| TR-1 | **CONFIRMED** → residual `FIND-TASK-001-4` | `binding_validation_errors` and `check_on_failure` use `CardRef::to_string()`, whose `Display` includes optional `#uid`, while resolution, authorization, graph ordering, and the repository's own `CardRef::identity_key`/`same_identity` deliberately ignore UID. The HTTP body is reachable through `register_card_http` → `register_card` → `validate_request`; authored UIDs are ignored for lookup and replaced during binding. Equal inline `OperatorSpec` values receive no duplicate check. Reuse `CardRef::identity_key()` and `OperatorSpec`'s existing typed equality; do not add a new digest abstraction for this registration check. |
| TR-2 | **CONFIRMED** → residual `FIND-TASK-001-4` | `referenced_binding_refusals_leave_no_writes` checks only HTTP 403 for the denied caller, while the other cases decode their stable code and round-1 AC-R4 expressly requires each case to assert it. Decode once, assert `WYRD_PERMISSION_403_DENIED_RBAC`, and preserve the no-write check. |
| TR-3 / DC-2 | **CONFIRMED**, deduplicated → residual `FIND-TASK-001-5` | The CLI remediation at `wyrd-cli/src/error.rs:41,116` still instructs users to pass or fix an “Eval card”; `card/eval.rs:1-3` promises deleted `Spec::Eval`; and the public module headers at `vala/eval/spec.rs:1` and `vala/eval/mod.rs:1` call Eval a Card kind. These are live user/API docs, unlike the TASK-002-owned record field. Rewrite only those descriptions around an eval-backed Verifier and preserve the `card::eval::EvalSpec` re-export and stable errors. |
| TR-4 / SR2-1 / DDR2-1 | **CONFIRMED**, deduplicated → new `FIND-TASK-001-11` | Both cluster restart tests save `postgres_pool_identity()`, drop the old server, then compare allocator addresses after replacement allocation. Address reuse is legal and occurred in the recorded required lane. The helper has only these two callers. Delete it and the address assertions; prove the replacement through the existing restart owner and real pool operations, retaining stopped-node, survivor, WAL, resource-plan, and reset-state assertions. |
| SR2-2 | **REVISED** → new `FIND-TASK-001-13` | Current and base `AGENTS.md` §16 expressly includes tests and test helpers and requires `# Panics` whenever a panic remains. The listed tests call `unwrap`/`expect`, assertions, or explicit `panic!` without that section; the materially changed ingest test has no rustdoc. The directly adjacent new/moved fixture owners `seed_dependency` and `seed_card` also panic and need the same contract. This is documentation-only; no Result conversion or new lint/check is warranted. |
| DS-R2-1 | **CONFIRMED** → residual `FIND-TASK-001-2` | The source fix is reachable and fail closed: `register_card_http` calls `register_card`, which invokes `validate_request` before graph planning or DB resolution, and `validate_request` calls the shared `spec_binding_errors`. Only pure tests cover the two nested shapes, however. Round 1's retained closure proof explicitly required an authenticated real-server stable-code/no-write case. Add one case to the existing registration target; keep the two unit cases rather than duplicating both shapes at the server tier. |
| DDR2-2 | **CONFIRMED** → new `FIND-TASK-001-12` | The distributed Oracle journey samples `oracle_inspection()` once immediately after the terminal refusal. The repository's own capacity journey documents that terminal delivery precedes asynchronous graph settlement and already uses bounded polling for settled physical evidence. Preserve the zero-ownership invariant, but poll the existing inspection under one deadline and report the last nonzero snapshot; do not sleep once or weaken/delete the assertion. |
| DC-1 | **CONFIRMED** → new `FIND-TASK-001-14` | `openapi.yaml` contains live refs to `VerificationBinding` (four sites), `VerifierImplementation` (two), and `TriggerActivation` (two), but defines none of those components. `Spec::schemas` registers only direct spec bodies, while `VerifierImplementation::schemas` registers only `DriftSpec`. `codegen:check` proves reproducibility, not reference closure. Extend the existing utoipa registration owner and test reachable ref closure; do not create a second schema model. |

No proposal is rejected. The security review's apparent disagreement with the
task review is resolved by separating source correctness from required closure
proof: the nested-binding source path is now fail closed, but prior
`FIND-TASK-001-2` is not closed until its mandated server-boundary proof exists.
The standards review's generated-artifact PASS does not contradict DC-1's
finding: the generated file is current but internally unresolved.

## Final deduplicated finding ledger

### FIND-TASK-001-2 — nested inline-Agent refusal lacks its required server-boundary proof

- **Sources/status/classification:** DS-R2-1; **REVISED**; **MISSING**.
- **Obligation:** prior `FIND-TASK-001-2` closure proof, REQ-090/092/094/143,
  and rejection before any registration write.
- **Location:** `crates/wyrd-spec/src/graph/composition.rs:238-358`;
  `crates/wyrd/wyrd-server/src/components/cards/service.rs:497-527,1154-1192`;
  absent from `crates/wyrd/wyrd-server/tests/pg_card_registration_route.rs`.
- **Evidence:** both nested forms are covered only by pure
  `spec_binding_errors_*` tests. The production route reaches the correction,
  but no authenticated HTTP test asserts its stable error mapping and durable
  no-write boundary, which the prior independently validated ledger required.
- **Observable consequence:** a future break between HTTP decoding,
  `validate_request`, error mapping, and transaction entry can reopen the
  original trust-boundary defect while every supplied closure test passes.
- **Decision-complete correction:** add one case to the existing
  `pg_card_registration_route` target submitting either supported nested shape
  with non-empty `verified_by`; authenticate with its existing writer fixture,
  assert HTTP 400 and `WYRD_REGISTRY_400_INVALID_CARD_SPEC`, and reuse
  `assert_no_registration_writes`. Preserve both pure tests, all production
  validation code, the canonical visitor, and rejection ordering. No new
  helper hierarchy, harness, or error code.
- **Focused closure proof:** run that exact new server test through the existing
  repository-managed Postgres wrapper, then `mise run test:cards:integration`.

### FIND-TASK-001-4 — duplicate identity enforcement and one stable-code proof remain incomplete

- **Sources/status/classification:** TR-1, TR-2; **REVISED**;
  **INCORRECT** (with a residual proof gap).
- **Obligation:** REQ-102, AC-018, Scenario 2, and prior
  `FIND-TASK-001-4`/AC-R4 closure.
- **Location:** `crates/wyrd-spec/src/graph/composition.rs:110-140,169-215`;
  `crates/wyrd-spec/src/reference.rs:211-229,454-469`;
  `crates/wyrd/wyrd-server/tests/pg_card_registration_route.rs:1091-1107`.
- **Evidence:** `to_string()` makes otherwise identical references distinct
  when only optional UID differs, contrary to the canonical identity used by
  every downstream resolver. Inline Operators are never inserted into the
  duplicate set. Existing tests cover identical UID-less refs only. The denied
  registration case checks status and writes but not its stable code.
- **Observable consequence:** a direct HTTP caller can bind one exact Verifier
  version twice, or list the same referenced/inline Operator twice, and the
  required denial proof accepts any 403 problem code.
- **Decision-complete correction:** use the existing
  `CardRef::identity_key()` for referenced Verifiers and Operators. Track inline
  Operators by existing `OperatorSpec: PartialEq` typed equality and reject an
  equal repeated spec; do not serialize/hydrate them or introduce a digest
  service solely for registration. Preserve mixed inline/reference semantics,
  wrong-kind ordering, existing error codes/details, and Workflow refusal.
  Add focused tests for UID-bearing versus UID-less copies of one Verifier ref,
  the same Operator ref, and two equal inline Operators. In the existing denied
  server case, decode once and assert `WYRD_PERMISSION_403_DENIED_RBAC` before
  the unchanged no-write assertion.
- **Focused closure proof:** run the four exact binding-validation unit tests
  and `referenced_binding_refusals_leave_no_writes`, then
  `mise run test:shared` and `mise run test:cards:integration`.

### FIND-TASK-001-5 — live public docs still advertise the retired Eval Card

- **Sources/status/classification:** TR-3, DC-2; **REVISED**; **DRIFT**.
- **Obligation:** REQ-103/109/114, AC-021, Scenario 4, and prior
  `FIND-TASK-001-5` closure.
- **Location:** `crates/wyrd/wyrd-cli/src/error.rs:35-42,110-117`;
  `crates/wyrd-spec/src/card/eval.rs:1-8`;
  `crates/wyrd-spec/src/vala/eval/spec.rs:1-4`;
  `crates/wyrd-spec/src/vala/eval/mod.rs:1-11`.
- **Evidence:** those live surfaces say “Eval card”, “Eval card kind”, or
  `Spec::Eval`, although the candidate rejects `kind: Eval` and removed
  `Spec::Eval`.
- **Observable consequence:** CLI remediation and Rust API docs direct a user
  to author or call a contract this candidate rejects or no longer compiles.
- **Decision-complete correction:** describe an eval-backed `Verifier` with
  `implementation.kind: eval` in the two CLI remediations and three public
  module headers. Preserve all stable codes/statuses, the
  `card::eval::EvalSpec` re-export, and Eval engine semantics. Do not touch the
  TASK-002-owned `EvalRecordObservation.eval_ref` surface or unrelated UI/mock
  prose.
- **Focused closure proof:** the focused grep for `Spec::Eval`, “Eval card
  kind”, and the two stale remediation strings is empty in these files;
  `mise run docs:check` and `mise run codegen:check` pass.

### FIND-TASK-001-11 — allocator addresses make required restart tests nondeterministic

- **Sources/status/classification:** TR-4, SR2-1, DDR2-1; **CONFIRMED**;
  **VIOLATION**.
- **Obligation:** current `AGENTS.md` §12's encountered-failure rule and the
  remediation's required `mise run test:bifrost` closure.
- **Location:** `crates/wyrd/wyrd-testing/src/bifrost/cluster.rs:2750-2779,2827-2859`;
  `crates/wyrd/wyrd-testing/src/server.rs:2923-2930`.
- **Evidence:** `stop_node` drops the old server before `restart_node` allocates
  its replacement, so the allocator may reuse both `PgPool` wrapper addresses.
  The committed remediation evidence records that exact valid execution as a
  failure. `postgres_pool_identity` has only the two flaky test callers.
- **Observable consequence:** a correct restart fails the required gate, while
  unequal addresses do not prove pool usability or lifecycle isolation.
- **Decision-complete correction:** delete `postgres_pool_identity` and both
  address comparisons. In both existing tests, exercise the replacement's
  application and Vala pools through their existing health/query operations;
  keep stopped-node absence, restart success, survivor usability, unchanged
  survivor ownership, retained WAL root, re-derived resource plan, reset
  resource state, and credential exchange assertions. No generation token,
  retry wrapper, sleep, or new fixture abstraction.
- **Focused closure proof:** run
  `cluster_restart_rederives_same_plan_from_retained_snapshot` and
  `process_pool_lifecycle_isolated_across_restart` repeatedly through exact
  `cargo nextest` expressions under `mise exec --`, then
  `mise run test:bifrost` once without retrying a failure.

### FIND-TASK-001-12 — Oracle cleanup is asserted before its asynchronous settlement boundary

- **Sources/status/classification:** DDR2-2; **CONFIRMED**; **VIOLATION**.
- **Obligation:** current `AGENTS.md` §12, the required Bifrost gate, and the
  analytical reliability invariant that terminal cancellation releases all
  descendant reservations and temporary ownership.
- **Location:** `crates/wyrd/wyrd-testing/tests/bifrost/oracle/distributed.rs:318-327`.
- **Evidence:** the test samples once directly after terminal delivery. The
  same repository's capacity journey states that a terminal reaches the caller
  before graph settlement and uses bounded polling for the settled owner.
  The remediation evidence records this test failing with “Oracle runtime did
  not settle” and accepts a rerun.
- **Observable consequence:** ordinary delayed cleanup under load fails the
  journey despite eventually releasing every resource; a passing rerun is not
  repeatable closure proof.
- **Decision-complete correction:** in this existing journey, poll
  `oracle_inspection()` under one bounded deadline until `active_queries`,
  `queued_queries`, `reserved_memory_bytes`, `reserved_spill_bytes`,
  `peer_pending`, and `peer_running` are all zero. Preserve the exact zero
  invariant and return the last inspection on deadline. Reuse Tokio time and
  the existing inspection owner; do not add a general wait abstraction for one
  caller or weaken the assertion.
- **Focused closure proof:** run
  `pg_bifrost_selective_predicate_and_projection_prune_distributed_reads`
  repeatedly through the repository-managed Oracle journey environment, then
  `mise run test:bifrost:journey:oracle` and `mise run test:bifrost` without
  accepting retries.

### FIND-TASK-001-13 — new and materially changed panicking Rust tests omit their panic contract

- **Sources/status/classification:** SR2-2; **REVISED**; **VIOLATION**.
- **Obligation:** `AGENTS.md` §16 and `architecture/agent-rules.md` Rustdoc hard
  blocker.
- **Location/evidence:** the panicking tests at
  `wyrd-loader/src/parse.rs:420`, `wyrd-spec/src/card/trigger.rs:62,89,100,115`,
  `wyrd-spec/src/card/mod.rs:2670`, and
  `wyrd-spec/src/graph/composition.rs:552,569,582,597,613,630,657,683,718`
  have intent docs but no `# Panics`. The new
  `pg_card_registration_route.rs:906` test and fixture owner
  `pg_card_registration_route.rs:38-88` likewise panic without documenting it.
  The materially modified
  `pg_grpc_ingest_smoke.rs:537` test has neither intent rustdoc nor `# Panics`.
- **Observable consequence:** the candidate violates a repository hard blocker
  that compiler, formatter, and runtime-test success do not enforce.
- **Decision-complete correction:** add accurate `# Panics` sections to the
  listed documented tests and to `seed_dependency`/`seed_card`; add intent
  rustdoc plus `# Panics` to the modified ingest test and the new registration
  integration test. Keep the existing tests, assertions, and fixture behavior;
  do not convert test-only invariant failures into production-style Results or
  add another documentation check. Before closure, apply the same mechanical
  audit to every Rust item added by the round-1 remediation commits so another
  item in that bounded diff is not missed.
- **Focused closure proof:** source inspection of the remediation diff, followed
  by `mise run fmt` and `mise run lints`.

### FIND-TASK-001-14 — the generated OpenAPI leaves new Verifier components unresolved

- **Sources/status/classification:** DC-1; **CONFIRMED**; **INCORRECT**.
- **Obligation:** REQ-045/109/114 and AC-021 require the language-agnostic HTTP
  contract to expose the same Verifier, binding, and Trigger model.
- **Location:** `crates/wyrd-spec/src/envelope.rs:263-324`;
  `crates/wyrd-spec/src/card/verifier.rs:57-136`;
  generated `openapi.yaml:1922,3967,4200,4262,4332,4478,4631,4821`.
- **Evidence:** the document references `VerificationBinding`,
  `VerifierImplementation`, and `TriggerActivation`, but none is defined under
  `components.schemas`. The new types are reachable from Agent, Service,
  Verifier, and Trigger roots used by the Card HTTP contract.
- **Observable consequence:** schema inspection and generated HTTP clients
  cannot resolve the newly required Verifier contract.
- **Decision-complete correction:** extend the existing `Spec::schemas`/utoipa
  registration path to register these reachable types through their existing
  `ToSchema`/`PartialSchema` implementations, including their dependency
  closure. Do not duplicate the contract or hand-edit `openapi.yaml`. Add one
  focused OpenAPI test that starts from the Verifier, binding, and Trigger roots,
  recursively follows each `$ref`, and asserts every referenced component is
  present; it need not clean unrelated pre-existing roots.
- **Focused closure proof:** run the exact new `wyrd-server` OpenAPI unit test,
  regenerate via the repository owner, then `mise run codegen:check` and
  `mise run docs:check`.

## Specification-revision decision and recommendation

No retained correction changes approved product behavior, a public API,
security policy, compatibility rule, concurrency semantic, resource owner, or
persistent-data decision. **No finding requires `SPEC_REVISION_REQUIRED`.**

Recommended verdict: **FIX_REQUIRED**.

Retained finding IDs: `FIND-TASK-001-2`, `FIND-TASK-001-4`,
`FIND-TASK-001-5`, `FIND-TASK-001-11`, `FIND-TASK-001-12`,
`FIND-TASK-001-13`, `FIND-TASK-001-14`.
