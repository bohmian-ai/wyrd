# Direct verification copy audit supporting TASK-005

Audited 2026-11-09 to correct the earlier Gateway-only investigation. The direct
`POST /v1/verification/execute` route has redundant input copies too. This is
source-backed research, not a measurement or implementation claim. The maintainer
subsequently required both Gateway and verification removal in TASK-005; approved
spec revision 5 and that task now own implementation and proof. This audit is
supporting evidence, not a separate design-only deliverable or deferred task.

## Findings

The normal direct route retains the original request for staging, copies its
inputs into the evaluation engine, then copies that evidence again into the
hosted evidence reader. Inline JSON is deep-copied at both boundaries.

| Owner | Copy and reachability | Decision |
|---|---|---|
| `crates/wyrd/wyrd-server/src/components/verification/service.rs::VerificationControl::run_direct`, line 573 | `request.inputs.clone()` duplicates the full inline input map before `engine.run`; original request inputs are retained for direct staging at line 815. Reached by default Verification L1. | Remove the duplicate input owner. Move decoded evidence into execution ownership and preserve the original declarations for staging through that same backing. |
| `crates/wyrd/wyrd-server/src/verification/hosted/blocks.rs::HostedBlocks::engine`, line 144; `hosted/evidence.rs::TenantEvidence` | `execution.evidence.clone()` creates another owned evidence map. Reached by ordinary direct calls, including default L1. | Share the immutable execution-owned declarations with the hosted reader. Keep mutable attachment/resolution bookkeeping separate. |
| `crates/vala/vala-verify/src/blocks/scope.rs::select`, lines 291–316; `Scope::resolve`, line 198 | Clones the selected subtree into an owned executor input. Root selection copies the whole value; leaf selection copies the leaf. | Retain the executor's required owned selected value for this bounded change. Prepared executor futures require owned data; removing that boundary would widen the executor API/lifetime work. Do not describe every selection as a full-context copy. |
| `crates/vala/vala-verify/src/blocks/engine.rs::json`, lines 1295–1302, called by `judge` | Clones the selected JSON input again solely for read-only comparison. L1 reaches this with small scalar values; a root/subtree assertion can duplicate a large value here. | Borrow JSON for comparison. Only evidence-declaration comparison needs the existing owned JSON conversion. Preserve its representation and failure behavior. |
| `engine.rs`, lines 668–669 | Retries clone resolved action inputs/workspace. Action-bearing direct executions reach this; default assertion-only L1 does not. | Retain the ownership required by the current executor contract. Do not claim this disappears with shared original evidence. Changing executor input ownership needs a separate bounded investigation. |
| `engine.rs::Attempted::settle`, line 1118 | Clones successful step output into both public step-result and dependency-ledger state. Action-bearing direct executions reach this; default L1 does not. | Required in TASK-005: one internal output owner serves dependency resolution and final result assembly; focused proof covers the path absent from default L1. |
| `crates/vala/vala-verify/src/assertion.rs::hash_context_value`, lines 134–143 | Allocates compact JSON bytes only to hash them. Reached by hashed context capture, absent from default full-capture L1. | Required in TASK-005: stream the same Serde byte sequence into SHA-256, preserving byte ordering, escaping and digests; focused proof covers the path absent from default L1. |
| `crates/wyrd/wyrd-server/src/verification/hosted/evidence.rs::located`, lines 103–111, and `cited`, lines 580–591 | Citation/extraction handling clones declarations and JSON into an envelope. Relevant to cited Agent/code paths, absent from default L1. | Preserve independently owned extracted/cited results where required. Further elimination needs a citation-lifetime-specific proof; no claim of removing all citation copies. |

Required verification outcomes in TASK-005 are removal of the two full-input map
copies, second read-only comparison copy, redundant successful-output ownership
and hash-only serialization buffer. Remaining required executor/retry and
independently owned citation boundaries are not claimed eliminated.

## Already efficient paths

- HTTP `components/verification/routes.rs::decode_body` decodes bounded bytes
  directly into the request contract. It does not repeat Gateway's disposable
  typed-validation round trip.
- SDK `Target::project` and `RunEvidence::pin` move inline JSON into declarations;
  no inline-payload clone was found at that boundary.
- `verification/results.rs::ResultPayloadBuilder::inputs/build` borrow original
  declarations and outcomes and use `EncodedVariant::from_serialize`. Preserve
  direct Variant construction; do not add an intermediate JSON tree or redo the
  shared Scribe optimization. Its optional trigger-metadata canonicalization is
  not a direct input-copy finding.
- ID, caller and `Arc` handle clones are not deep payload copies.

## Selected implementation outline

1. Transfer ownership of the decoded direct request into the existing direct
   control path. Keep small routing/identity metadata separately and give the
   original evidence one immutable execution-owned backing.
2. Reuse that backing in the existing evaluation engine and `TenantEvidence`
   reader. Keep synthetic attachments distinct from caller-declared slots, and
   keep per-execution resolution state mutable only under its existing owner.
   HTTP and MCP direct callers must reach the same control path. Existing owned
   public request/result and executor APIs remain compatible; no new dependency,
   general shared-data framework or runtime option is needed.
3. Stage the same original declarations and exact outcome through the existing
   result builder. Release internal readers before reclaiming evidence into any
   required owned outcome. Merely moving a deep clone from admission to result
   assembly does not satisfy the outcome. Do not stage synthetic attachments as
   if they were caller inputs, stage early to avoid ownership, or retain a second
   original-input snapshot.
4. Borrow the existing selected JSON during read-only assertions. Preserve owned
   declaration conversion for the non-JSON comparison branch. Leave the executor's
   one required owned input intact; this is not a public executor lifetime change.
5. Retain successful step output once through dependent evaluation and public
   result assembly, and stream context hashing without disposable JSON text.
   TASK-005's direct scenarios include the focused proofs absent from default L1.
6. Run the small correctness proof below, then paired profiled and unprofiled
   Verification L1. Report which clone stacks disappeared and which ownership
   boundaries remain. Do not promise a throughput gain before measurement.

Required invariants: exact Verifier/subject/Run/tenant attribution; original input
content and captured actual values; passed/failed/inconclusive results; required
slot and schema handling; file/digest limits; pinned citations; permission/audit
counts; stage ordering and pressure; deadlines and cancellation; independent-step
concurrency; unchanged `queue_execution=false`. Direct persistence replay never
queues execution. Do not redesign queued/group execution, scheduling, admission,
or post-response ownership to obtain the saving. Shared callsites receive only
the mechanical adaptations necessary to preserve their existing behavior.

## Minimum proof and L1 acceptance

Extend existing direct-route and engine tests, not a new suite. Add at most one
focused ownership/allocation observation at the existing server test owner for
large inline JSON and multiple requirements. Construct input before observing,
separate one required selected-value/result encoding from duplicate whole-input
ownership, and verify sharing does not end in an equivalent result-time clone.
L1 profiles establish the production-path allocation change.

For implementation TDD, the first RED observation is duplicate original-input
ownership on the current direct path; GREEN shares/moves that input through
execution/reader/staging; REFACTOR removes superseded duplicate owners while
the same route assertions stay green. The read-only comparison change has a
separate RED observation for an existing selected JSON value being copied a
second time; GREEN borrows it, and REFACTOR keeps the non-JSON representation
conversion confined to its existing comparison branch. Do not manufacture a new
journey for an unchanged public capability.

Existing exact commands (confirmed packages, targets and selectors):

```sh
mise exec -- scripts/postgres/with-test-postgres.sh -- bash -lc 'mise run db:migrate:all:inner && cargo nextest run --locked -p wyrd-server --test integration --test-threads=1 -E "test(=pg_verification_routes::direct_execution_judges_inline_without_enqueueing)"'
mise exec -- scripts/postgres/with-test-postgres.sh -- bash -lc 'mise run db:migrate:all:inner && cargo nextest run --locked -p wyrd-server --test integration --test-threads=1 -E "test(=pg_verification_routes::direct_execution_refuses_nonconforming_evidence)"'
mise exec -- cargo nextest run --locked -p vala-verify --lib -E 'test(=blocks::engine::tests::no_step_verifier_judges_inputs_and_traces)'
mise exec -- cargo nextest run --locked -p vala-verify --lib -E 'test(=blocks::engine::tests::context_capture_hashes_or_redacts_actual_values)'
mise exec -- cargo nextest run --locked -p vala-verify --lib -E 'test(=blocks::engine::tests::independent_actions_overlap_within_the_concurrency_bound)'
mise exec -- cargo nextest run --locked -p vala-verify --lib -E 'test(=blocks::engine::tests::execution_deadline_cancels_running_work)'
mise exec -- cargo nextest run --locked -p wyrd-server --lib -E 'test(=verification::results::tests::outcome_writes_output_then_summary)'
mise run bench:l1:verification -- --allow-dirty --profile
mise run bench:l1:verification -- --allow-dirty
```

The route regression already covers passed/failed results, exact identities,
one audit per call and no enqueue activity. Extend it only where needed to pin
exact original-input/output readback after drain. Existing concurrency, timeout,
capture and staging assertions protect ownership/lifetime changes. Reuse the
repository's format, lint and dependency-boundary checks from TASK-005 for the
combined write set rather than list another aggregate suite here.

Default Verification L1 is `direct-verify-c1` and `direct-verify-c64`: four scalar
assertions over a 2 KiB `context`, alternating passed/failed calls. It exercises
both full-input map clones and comparison, but its comparisons select only
`a/b/c/d`, not the entire padded context. The benchmark cannot prove removal of
optional action, hash-mode or citation copies.

Compare immutable immediate-baseline and candidate builds on the same quiet
host, identical geometry/settings, all default steps and three repetitions.
Use paired profiles for allocation per reconciled call and clone attribution;
use unprofiled runs for throughput, CPU, overhead p99 and peak RSS. Compare
candidate medians with the baseline repetition range; rerun a worse-outside-range
step as a fresh back-to-back pair. Zero refusal, lost writes, reconciliation
errors and undrained work are mandatory; both verdict populations must be exact.
The driver's 500 microsecond c64 overhead target remains reported as met/missed/
unsettled, and a valid record/zero exit alone does not establish improvement.
Retain Gateway L1 and original whole-change baselines; Verification L1 does not
substitute for them. There is no new large-payload benchmark matrix or numeric
allocation floor.

## Scope assigned in the packet

Revision 4's Gateway-specific scope omitted direct verification. The maintainer
explicitly required TASK-005 to implement both paths on 2026-11-09; revision 5
records that instruction as REQ-013/AC-011. TASK-005 includes the direct scenarios,
source owners, exact focused commands, paired profiles and full Gateway AND
Verification L1. No further scope-correction task or separate direct-verification
task is required. TASK-005 is the authoritative executable plan; the audit's
outline/commands are research context rather than a competing proof inventory.

## Evidence and limits

Read-only source audit; no code, tests, prototypes, profiles or benchmarks ran.
Installed-source ownership/caller inspection supports the selected outline;
internal sharing/reclamation must still be implemented and verified. The current
scope is recorded by spec revision 5 and TASK-005, not inferred from this audit.

Local document links, whitespace and named-test source references passed. An
independent document reader found no material comprehension gap across this
audit, the approved spec and TASK-005's scope correction; that pass did not
validate source findings or approve implementation.

Authority: [direct execution and staging](../../../architecture/wyrd-design.md),
[approved resource-scaling scope](spec.md),
[testing](../../../TESTING.md), and
[benchmark method](../../../benchmarks/README.md).
