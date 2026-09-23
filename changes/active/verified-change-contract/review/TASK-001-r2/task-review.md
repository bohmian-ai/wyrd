# TASK-001 round 2 — task implementation review

## Immutable subject

- Repository: `/Users/stevenforrester/Documents/GitHub/verified-change-contract`
- Base: `5293546f33b3a5fd9de529098e23ea70d472c412`
- Cumulative candidate: `dd0503e7149017d760a987346e2838001e5c3429`
- Approved specification: `changes/active/verified-change-contract/spec.md`, revision 32
- Original task: `changes/active/verified-change-contract/tasks/TASK-001-verifier-contract-and-registration.md`
- Prior review: `changes/active/verified-change-contract/review/TASK-001-r1/`
- Remediation task: `changes/active/verified-change-contract/review/TASK-001-r1/TASK-001-R1-verifier-binding-closure.md`

The complete `5293546f3..dd0503e71` range was reviewed. The current worktree
HEAD is `5f14f3c32`; its later `AGENTS.md` process-rule change is not part of the
reviewed source range. The candidate object remained resolvable throughout the
review.

## Acceptance matrix

| Requirement, acceptance criterion, constraint, or non-goal | Implementation evidence | Verification evidence | Result |
|---|---|---|---|
| REQ-045, REQ-046, REQ-047, REQ-056, REQ-109, INV-014: one registrable `Verifier` Card with exactly one closed Drift/Eval implementation; no Drift/Eval Card kinds or Verifier DAG | `wyrd-spec/src/{envelope.rs,card/verifier.rs}`; `CardKind::registrable()` has 15 kinds; `VerifierImplementation` is a closed adjacently tagged enum; Drift/Eval envelope variants are removed | `card::verifier_card_tests`; retired-kind parse tests; committed `codegen:check` evidence | PASS |
| REQ-110, REQ-111, INV-012: preserve the approved Drift/Eval payloads and engines while applying the locked Drift removals and validation matrix | `card/drift.rs`, `vala/eval/spec.rs`, and existing `vala-drift`/`vala-eval` consumers remain the payload/engine owners; SPC+Metric and non-statistical combinations are refused | focused Drift validation tests and stated `test:shared`/`lints` results | PASS |
| REQ-090, REQ-091, INV-001, INV-013: `verified_by` exists only on Service, Service component occurrences, and standalone Agent; Trigger/Operator inline and referenced bodies share one shape | `VerificationBinding`; `ServiceSpec`, `ServiceComponent`, `AgentSpec`; `Spec::binding_sites`; nested inline-Agent sites are explicitly refused | nested Workflow-Agent and Eval-judge refusal tests; loader journey covers legal locations/forms | PASS |
| REQ-092, AC-018: registration resolves, tenant-scopes, UID-pins, traverses, derives relationships, and fails closed for unresolved, wrong-kind, unauthorized, and cross-tenant refs | canonical `ReferenceSlotVisitor`; `resolve_card_references`; `EffectiveSpecs`; registration and CLI journey assertions | referenced workflow-Operator, activation mismatch, cross-tenant and permission-denial cases; UID/relationship assertions. The under-privileged case does not assert its stable body code (TR-2) | FAIL |
| REQ-093, REQ-094, REQ-143: closed Trigger activation, closed Operator action, activation/implementation pairing, and Workflow refusal before persistence | `TriggerActivation`, `OperatorAction`, `EffectiveSpecs::validate_binding` | strict-decoding tests and real-server referenced refusal cases | PASS |
| REQ-102 and AC-018 duplicate rejection: one subject occurrence cannot bind the same exact Verifier version twice and one binding rejects duplicate Operators | `binding_validation_errors` and `check_on_failure` attempt duplicate checks | existing tests cover only identical UID-less referenced values. UID-bearing identity variants and duplicate inline Operators are accepted (TR-1) | FAIL |
| REQ-103 and AC-021 retirement portion: `verified_by` replaces verification `publishes_to`; public contract, loader, registry, schema, CLI, SDK, HTTP and MCP describe Verifier rather than an Eval/Drift Card | old runtime fields/kinds are removed and generated Card artifacts expose Verifier | stale public CLI remediation and public Rust contract docs still say “Eval card” / `Spec::Eval` (TR-3) | FAIL |
| REQ-116, REQ-120, REQ-144, AC-022: retire eval runs/assertions tables, drift alerts, Eval pull protocol, and `vala-core::alert_router`, including build references | deleted tables/routes/types/crate; drop migration; built-in removed-table list; test target and change-detection references removed | stated `test:bifrost`, `test:sql`, codegen, docs and grep evidence | PASS |
| REQ-113: reuse the normal Card envelope, canonical reference visitor, composite registration, shared clients, and existing engines; add no alternate broker/engine/registration path | new behavior is integrated into existing `Spec`, visitor, loader, cards service and client state; no second walker or executor was added | visitor completeness test and relevant crate/journey lanes | PASS |
| REQ-114 and Scenario 4: permanent authorities and generated artifacts publish the 15-kind Verifier binding model within TASK-001's scope | `architecture/wyrd-design.md`, `wyrd-doctrine.mdx`, Bifrost/evaluation/drift references, schemas, OpenAPI, stubs and docs changed | stated `codegen:check` and `docs:check` results | PASS |
| Task non-goals: no compatibility alias, binding Card, future implementation, Verifier DAG, speculative TypeScript `CardKind`, or replacement Eval/Drift engine | no such surface appears in the cumulative diff | source inspection | PASS |
| AGENTS §11–§12 completion: required lanes are credible and encountered failures/flakes are corrected rather than waived | remediation evidence records all requested lanes as eventually green | the committed evidence explicitly records a real flaky assertion and leaves it unchanged; its heap-address premise is nondeterministic (TR-4) | FAIL |

## Prior-finding closure

| Prior finding | Closure result | Evidence |
|---|---|---|
| FIND-TASK-001-1 | CLOSED | `pg_eval_v1_protocol` was removed from `mise.toml` and `.github/scripts/detect-changes.sh`; the Bifrost lane is recorded green. |
| FIND-TASK-001-2 | CLOSED | `Spec::binding_sites` is the shared owner; nested inline-Agent bindings are refused; the four consumers use `spec_binding_errors`/`binding_sites`; `owned_bindings` is gone. |
| FIND-TASK-001-3 | CLOSED | `ObservationsReady {}` and `VerifierImplementation` strict decoding reject unknown/secret-shaped siblings, with focused tests. |
| FIND-TASK-001-4 | PARTIAL | Most missing proof was added, but duplicate-Operator proof covers only UID-less CardRefs and the under-privileged server case asserts status rather than the required stable error code. See TR-1 and TR-2. |
| FIND-TASK-001-5 | PARTIAL | The originally named primary prose was improved, but adjacent public CLI remediation and public contract module docs still advertise an Eval Card. See TR-3. |
| FIND-TASK-001-6 | CLOSED | Eval run-open request/response, lease token, generator fixtures, re-exports and unreachable CLI variants are gone. |
| FIND-TASK-001-7 | CLOSED | The task-verification sentence was removed and the recorded test name retained. |
| FIND-TASK-001-8 | CLOSED | The five named fallible items now have intent docs and `# Errors`. |
| FIND-TASK-001-9 | CLOSED | `EffectiveSpecs` owns resolved refs/cache and the IO workflow is an inherent method. |
| FIND-TASK-001-10 | CLOSED | The listed imports are module-scoped and signatures use imported bare names. |

## Proposed findings

### TR-1 — duplicate binding validation uses rendered refs and ignores inline Operator identity

- Classification: **INCORRECT**
- Violated obligation: REQ-102; AC-018; TASK-001 Scenario 2 and acceptance requirement for duplicate Verifier/Operator refusal.
- Exact location: `crates/wyrd-spec/src/graph/composition.rs:110-140,169-215`; `crates/wyrd-spec/src/reference.rs:454-469`.
- Evidence: Verifier and referenced-Operator duplicate keys are
  `CardRef::to_string()`. `Display` appends `#uid`, even though
  `CardRef::same_identity` and `identity_key` define exact named identity while
  intentionally ignoring optional server-managed UID. A direct HTTP request
  can therefore provide the same `(kind, space, name, version)` twice with
  different/absent UIDs and bypass both duplicate checks; the resolver itself
  ignores authored UID for lookup and replaces it with the server UID. The
  `InlineableRef::Inline` branch performs no duplicate check at all, despite
  `OperatorSpec: PartialEq` and the runtime contract identifying inline
  Operators by canonical spec digest. Existing tests use two identical,
  UID-less CardRefs only.
- Observable consequence: registration accepts two bindings to the same exact
  Verifier version, or accepts the same referenced/inline Operator twice in one
  `on_failure` list, contrary to the approved registration contract.
- Required testable correction: reuse `CardRef::identity_key()` for referenced
  Verifier and Operator duplicate keys, and reject repeated inline Operator
  specs by their existing typed/canonical identity without changing mixed
  referenced-vs-inline semantics. Add focused tests for UID-present vs UID-less
  copies of one Verifier and Operator ref and for two equal inline Operators;
  each must assert the existing duplicate code.

### TR-2 — the required under-privileged stable-code proof is absent

- Classification: **MISSING**
- Violated obligation: remediation AC-R4 for FIND-TASK-001-4; AC-018's
  fail-closed unauthorized proof.
- Exact location: `crates/wyrd/wyrd-server/tests/pg_card_registration_route.rs:1091-1107`.
- Evidence: the other three server-only refusal cases decode the response and
  assert their stable code. The under-privileged case asserts only HTTP 403 and
  no writes, although the route contract documents
  `WYRD_PERMISSION_403_DENIED_RBAC` and AC-R4 explicitly requires each case to
  assert its code.
- Observable consequence: a regression to a different 403 body or stable code
  passes the claimed closure test, leaving the agent-facing error contract
  unproved.
- Required testable correction: decode the denied response once, assert
  `WYRD_PERMISSION_403_DENIED_RBAC`, and retain the existing no-write proof.

### TR-3 — public surfaces still describe the retired Eval Card contract

- Classification: **INCORRECT**
- Violated obligation: REQ-109, REQ-103, AC-021, TASK-001 Scenario 4, and
  remediation AC-R5/FIND-TASK-001-5.
- Exact location: `crates/wyrd/wyrd-cli/src/error.rs:35-42,110-117`;
  `crates/wyrd-spec/src/card/eval.rs:1-3`;
  `crates/wyrd-spec/src/vala/eval/spec.rs:1-4`;
  `crates/wyrd-spec/src/vala/eval/mod.rs:1-2`.
- Evidence: CLI stable remediation still directs callers to an “Eval card YAML
  or JSON file” and to “Fix the Eval card spec”; public Rust module docs call
  the payload the `Eval` card kind and claim `Spec::Eval` retains a stable
  import surface. The candidate removed both `CardKind::Eval` and `Spec::Eval`.
  These are adjacent live public surfaces, not the explicitly deferred
  `EvalRecordObservation.eval_ref` field owned by TASK-002.
- Observable consequence: users and agents following the CLI remediation or
  Rust contract documentation author a Card kind that this candidate rejects.
- Required testable correction: describe an eval-backed Verifier and
  `implementation.kind: eval` at these live surfaces, preserving every error
  code/status and the `card::eval::EvalSpec` re-export. Regenerate/check public
  docs and error artifacts as required; do not touch the TASK-002-owned record
  field.

### TR-4 — a known nondeterministic heap-address assertion remains in a required gate

- Classification: **VIOLATION**
- Violated obligation: AGENTS §11–§12 verification/completion standard,
  including the current rule that an encountered flaky assertion is fixed and
  not recorded as acceptable; TASK-001's required green Bifrost verification.
- Exact location: `crates/wyrd/wyrd-testing/src/bifrost/cluster.rs:2764-2779`;
  remediation evidence at
  `changes/active/verified-change-contract/review/TASK-001-r1/TASK-001-R1-verifier-binding-closure.md:367-373`.
- Evidence: the test saves `postgres_pool_identity()` as a heap address, drops
  the old server, constructs a replacement, then asserts the replacement
  address differs. An allocator may legally reuse the freed address, and the
  remediation session observed exactly that failure. The implementation report
  explicitly left it untouched and called the final rerun green.
- Observable consequence: `test:bifrost` remains nondeterministic and can fail
  without any product defect; the candidate's required verification is not a
  reliable completion gate.
- Required testable correction: remove the heap-address inequality as proof of
  restart and assert a stable observable restart property already owned by the
  test (stopped-node absence, successful restart, retained WAL root, re-derived
  resource plan and reset resource state). Run the exact test repeatedly and
  the owning Bifrost lane.

## Explicit assessment of the two reported concerns

1. The cluster pool-identity concern is reachable in the required Bifrost gate
   and is retained as TR-4. Heap allocation addresses do not provide a stable
   generation identity.
2. The tenant-admission concern does not produce another finding. The changed
   ingest test now seeds its newly minted tenant before minting/using the token,
   and every other `mint_user_jwt` use in that test file uses a seeded tenant.
   Other server route tests mint for `WyrdTestServer::data_tenant_id()`, which
   the fixture owns. A hypothetical unseeded sibling without a reachable
   failing path would be speculative and outside TASK-001.

## Verification limits

- The committed remediation artifact records all required lanes as exit 0, but
  raw command logs are not preserved in the immutable candidate. This review
  inspected the source and test assertions and did not rerun the broad lanes.
- The recorded `test:bifrost` history includes one failure from TR-4 before a
  passing rerun; a passing rerun does not validate a nondeterministic assertion.
- The other recorded Oracle-settle flake has no demonstrated TASK-001 causal
  path or incorrect assertion in the available evidence and is not proposed as
  a finding.

## Overall result

**FAIL**

TR-1 violates duplicate-refusal behavior, TR-2 leaves a required stable error
contract unproved, TR-3 retains public retired-model instructions, and TR-4
leaves a known flaky assertion in a required capability gate. All four are
bounded corrections within approved specification revision 32; none requires a
new product, API, architecture, security, compatibility, concurrency, or
persistent-data decision.
