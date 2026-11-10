---
id: TASK-001-R1
kind: remediation
status: superseded
spec: SPEC-verifier-review-pipelines
spec_revision: 17
parent_task: TASK-001
requirements: [REQ-010, REQ-011, REQ-012, REQ-013, REQ-014, REQ-015, REQ-031, REQ-032, REQ-046, REQ-047, REQ-048, REQ-049, REQ-052, REQ-053, REQ-054, REQ-055, REQ-056, REQ-057, REQ-058, REQ-059]
invariants: [INV-002, INV-004, INV-005, INV-009, INV-010, INV-011, INV-015, INV-017, INV-018, INV-019]
acceptance: [AC-005, AC-006, AC-007, AC-008, AC-009, AC-024, AC-025, AC-039, AC-040, AC-041, AC-046, AC-047, AC-048, AC-049, AC-050, AC-051]
remediates: [FIND-TASK-001-1, FIND-TASK-001-2, FIND-TASK-001-3, FIND-TASK-001-4, FIND-TASK-001-5, FIND-TASK-001-6, FIND-TASK-001-7, FIND-TASK-001-8, FIND-TASK-001-9, FIND-TASK-001-10, FIND-TASK-001-11, FIND-TASK-001-12, FIND-TASK-001-13, FIND-TASK-001-14, FIND-TASK-001-15]
---

# Restore durable verification execution guarantees

> Historical remediation packet. Original fixes and proofs remain evidence, not
> an independent PASS. The approved durability supplement revision 2 supersedes
> this packet's architecture. Do not execute it as a current task. Scenarios
> 12–17 and their lifecycle proof are replaced by
> [Current Task 001](../../tasks/TASK-001-bifrost-execution-state.md).
> The implementor reports the original fixes and scenario 12 implemented;
> scenarios 13–17 were not implemented. Fresh review remains required.

Close the independently validated Task 001 gaps at the existing execution, publication,
capability-resolution and Scribe owners. A committed execution must retain its final
result; a replay must recover its original publication receipt; revocation, time and
page membership must remain authoritative under supported concurrency and recovery. Keep
the accepted plain-step checkpoint behavior and thin client projections.

Approved specification: `changes/active/verifier-review-pipelines/spec.md`, revision 17,
with `public-contracts.md` and `output-publication.md` in the same directory.
Original task (archived): [revision-16 Task 001](TASK-001-original-durable-agent-work.md).
Validated review: `changes/active/verifier-review-pipelines/review/task-001-r1-365bfc94b/findings-validation.md`.
Base: `f4e79ea00523af5cd9212fdacde64f16c97ad554`.
Reviewed candidate: `365bfc94bfef1a153f213462f7ef104afa823741`.
Prior remediation candidate: none. This is R1.

Earlier human-approved correction, now scoped to active/finalizing metadata by revision
17: reuse the standardized File/Artifact completion metadata pattern for publication
readback, so a successful publication does not require Bifrost inventory rediscovery
before use. This replaces the former finding-3 waiting/deferral recommendation and
narrows related finding-1/2 proof. The original discovery and findings-validation
reports remain historical evidence; this is a human-directed correction-selection
update, not a new finding or an additional FIND ID. See
`publication-readback-addendum.md` for focused source validation.

The live checkout is independently integrating main. These diagnoses refer to the
immutable reviewed candidate, available at detached
`/tmp/wyrd-task-001-review-365bfc94b`. Before implementing on the integrated branch,
establish correspondence for each affected owner and retain any already-correct
integrated behavior; do not undo main or reintroduce a closed defect. The integration
itself was not reviewed here. Record the implementation candidate and focused evidence
for the later remediation review.

## Outcome and Value

Complete the original fixes and establish the approved active-to-retained execution
lifecycle. A direct no-publication request uses the current in-memory engine without
queue/checkpoint writes; queued execution is recoverable through logical block
checkpoints; accepted publication File references are usable immediately; complete
terminal evidence is served from Bifrost after handoff and operational Postgres rows can
be removed. Original findings are still 1–15; revision-17 lifecycle work is separately
approved scope, not new FIND IDs or retroactive evidence.

## What Changes and What Must Stay True

The seven lifecycle decisions in spec revision 17 and public-contracts.md are
authoritative. Extend canonical execution_events metadata for the exact terminal
selection/full errors, typed invocation records, complete manifests/verified File
associations, lineage and retained replay identity; preserve existing
input/result/output meanings and public responses. Publish components through existing
Scribe delivery and select one stable terminal fact. The phase-aware reader uses
Postgres while active/finalizing and Bifrost after handoff even while old rows await
cleanup. Page cuts, current feature authorization and terminal cancel/replay survive
purge and source Card deletion.

Use the existing engine/walker/ledger and optional Checkpoints owner. Process
dependencies remain shared; an engine instance is disposable per attempt. Restore
logical block settlements/attempts and frozen decisions from active Postgres, not
serialized engine memory or historical event replay. Preserve conservative
ambiguous-work accounting. Direct publication has a request-owned durable context/fence,
never background eligibility; abandoned direct work can finalize evidence but never
restart Agent calls. Pure direct work adds no execution queue/per-block checkpoint
writes, and returned committed references can remain in the running ledger without
redundant metadata queries.

Active publication descriptors from the shared File completion owner move with
authoritative manifests into Bifrost before operational associations are purged. Keep
File inventory ACK and history intent. A complete handoff is more than one event ACK:
all required records and terminal selection must be acknowledged with stable identities;
cleanup additionally verifies retained source ownership through existing Scribe/Oracle
publication/capture continuity. Do not use an arbitrary TTL/sample query, wait for
Iceberg or add an archive service. Cleanup is bounded/indexed, tenant-scoped and
retryable in the existing execution/projection lifecycle. Through the shared File owner,
also release completed upload-session metadata owned solely by retained publications
once completion/replay no longer needs it; never delete accepted objects, change
ordinary Card/input-File completion semantics or assume an existing general File
collector. Preserve binding last-execution and independent delivery references to
retained identities without a terminal queue parent. Preserve pending Operator/provider
delivery and scheduling/repository/library/principal state; delivery status changes are
projected independently. Existing File/Bifrost evidence retention is unchanged.

## Approach

1. Reconcile owning architecture and versioned contract/event metadata with the approved
   phase ownership and logical checkpoint pattern.
2. Keep direct and queued execution on the shared engine; support direct publication
   without scheduling continuation and preserve the plain direct write boundary.
3. Repair the existing publication/fence/admission/deadline/resolver defects at their
   common owners, carrying committed File associations through the ledger and
   persistence.
4. Make selected final handoff durably replayable, extend the shared historical reader,
   and prove every terminal outcome before enabling purge.
5. Add bounded operational cleanup with retained replay/cursor identity and independent delivery/control lifetimes.
6. Close binding/runtime proof and declaration/rustdoc gaps through focused existing
   targets; leave Agent tool dispatch to Task 003.

## Implementation Reference

| Existing owner and inspected consumers | Missing behavior / selected extension |
| --- | --- |
| HostedBlocks::engine; VerificationControl::run_direct; VerifierRunner::dispatch; VerificationEngine::with_checkpoints; walk/RunLedger/Scope | Already one engine per attempt, shared process resources and optional block checkpoints. Formalize/reuse this pattern; add request-owned direct publication and phase-aware finalization, not an actor runtime or serialized engine. |
| VerifierExecutionQueue and Checkpoints; runner/submissions/completions/leases | Extend common SQL authority for existing findings, stable terminal selection and deletion eligibility. No new queue or recovery executor. |
| File file_upload_init/reconcile_upload/finish_upload; ObjectInventory; Cards::manifest_completion_rows; Publications::complete; TenantEvidence and inspection readers | Shared completion already returns verified StoredObjectRef and persists upload completion. Preserve the descriptor privately with active acceptance, carry it into retained manifest metadata and use common storage readers; do not create Artifact Cards or a duplicate blob catalog. |
| execution_events canonical table, EventProjector, process Scribe outbox, Oracle cuts/captures and Scribe published source revisions | Existing stable event delivery and source continuity lack full terminal data and complete handoff coverage. Extend these owners for typed retained facts, physical retry identity and safe purge proof; do not make event history the active recovery executor. |
| VerificationControl status/inspection reader; shared Execution; existing SDK/MCP/CLI journeys | Current queued inspection depends on PG even after completion. Centralize phase selection and preserve current grants, errors, cursors and wire shapes after purge. |
| Operator queue/delivery status; binding/group/schedule owners; Task 005 webhook/check lifecycle | These records may outlive evaluation. Preserve/detach required operational dependencies and project delivery updates without retaining a terminal execution row solely for status or cascading away live work. |

Source inspection baseline is e36474189c2cf74408af6e9c918a1efd84446b92; immutable
finding locations remain the original candidate recorded below. These are extension
decisions, not a private helper/type/file allowlist.

## Diagnosis and required correction

The following is the independently validated ledger, reproduced here so implementation
does not depend on rediscovering the defects. Each section identifies the obligation,
exact source, reachable consequence, minimum correction boundary and closure proof.
Source-local review IDs are provenance, not additional work. Implement through
`$wyrd-implement`.


Paths below are repository-relative; line numbers name the immutable candidate. Every
correction remains within the existing owner. Focused proofs are required closure
evidence, not tests executed during this static review.

### FIND-TASK-001-1 — REVISED / INCORRECT — publication identity does not reliably recover the original receipt

Sources: BEH-001, INV-R1, SYS-002, CONC-001, CONC-002. Obligation: REQ-047, AC-040, Task
scenario 5 and original-call replay contract.

Locations: `crates/wyrd/wyrd-server/src/verification/hosted/publications.rs:113`
(`path`) and `:201` (`accept`);
`crates/wyrd/wyrd-sql/src/queries/verifier_executions.rs:690` (`ACCEPT_PUBLICATION_SQL`)
and `:3448` (`accept_publication`). Full backend methods and SQL producer/read consumers
inspected.

Evidence/scenario: path publication freezes and completes every File before consulting
persisted call identity. After a committed file publication and lost reply, replay from
removed scratch/a fresh workspace fails; replay during storage outage also fails before
finding the receipt. Replacing the same path with a directory reaches SQL replay but
returns `content.kind()` from the new proposal, so the receipt says directory while the
stored manifest remains file. JSON shares acceptance but not the scratch dependency.
Separately, two identical concurrent calls use compatible execution share locks. B
starts its statement snapshot before A commits, waits on the unique index, then `ON
CONFLICT DO NOTHING` inserts nothing after A commits; its same-statement SELECT cannot
see A's row and returns false conflict. Uniqueness prevents duplicates but does not
implement correct replay acknowledgement.

Consequence/materiality: the explicit reply-loss recovery scenario can fail or
misdescribe accepted evidence, including an ordinary racing transport retry. SQL already
retains call ID, argument digest, kind and manifest; a new receipt store is unnecessary.

Correction: make the existing SQL publication identity owner resolve a matching original
call and arguments to its stored receipt before mutable scratch/storage work. Order
same-identity acceptance/recheck so concurrent identical calls return the same receipt
after a committed winner; a fresh statement after conflict or existing
transaction-scoped identity serialization suffices. Keep one identity implementation for
JSON/file/tree, reject changed arguments/another call, preserve fresh-attempt
separation, and validate the current execution fence for new work. Do not hold a
transaction over uploads. Return stored kind and its original completed File linkage on
replay; reuse the authoritative readback correction in finding 3 rather than
reconstructing the object from scratch or Bifrost. A preflight-only lookup without
resolving the insertion race is incomplete.

Burden/proof: a bounded SQL/backend correction, not another queue. Commit a file,
replace/remove scratch, replay with storage unavailable/current lease and recover
original kind with one manifest and no new upload. In separate real tenant transactions,
hold A's identical acceptance uncommitted until B waits, commit A, require B replay;
distinct-call conflict and stale/new-attempt cases remain correct.

### FIND-TASK-001-2 — REVISED / MISSING — publication limits do not bound retained execution resources or guarantee mountable accepted files

Source: BEH-002. Obligations: Task execution-record cumulative budgets;
output-publication.md:98–101 effective file/tree/JSON, count, cumulative retained-byte,
staging and deadline ceilings; persisted budgets across recovery.

Locations: `hosted/publications.rs:80,113,201`, `hosted/mod.rs:128`,
`hosted/workspace.rs:565`, `hosted/evidence.rs:265`; publication acceptance/migration at
`verifier_executions.rs:690` and
`migrations/20261104000100_verifier_execution_publications.sql`. All paths are under
`crates/wyrd/wyrd-server/src/verification/` except SQL paths under
`crates/wyrd/wyrd-sql/`.

Evidence/scenario: JSON has a per-value bound; directory freezing bounds current tree
bytes/entries. Successive distinct names can retain repeatedly rewritten bounded scratch
without any execution-level count/byte accounting in acceptance or schema. Reclaim does
not restore an accounting record because none exists. Default workspace write permits 50
MiB while `TenantEvidence::file_object` rejects over 20 MiB, so a valid 21 MiB file can
be accepted and then never mounted. Ordinary trusted backend calls suffice; Task-003's
absent tool is irrelevant to the explicitly required backend.

Consequence/materiality: required retained-resource ceilings are absent,
restart/concurrency cannot preserve them, and accepted evidence can be unusable by
required consumers. This is not a request for new Task-002 fanout/model-token machinery.

Correction: enforce effective backend retention ceilings at the existing
execution/publication SQL authority with atomic count/retained-byte accounting preserved
across attempts/reclaim and same-call replay charged once. Reuse deployment/host limit
ownership and existing serialized JSON/manifest byte/entry measurements; Verifier limits
may only lower ceilings. Enforce effective file/consumer limits before completing new
content, and deadline before new acceptance; route accepted-file mountability through
the authoritative File readback in finding 3 rather than a separate inventory resolver.
A workspace-only/per-call limit cannot establish cumulative retention. Keep bytes
outside SQL and existing File lifecycle; do not invent another budget service, authoring
option or public upload surface. Any local default wiring belongs to existing
deployment-limit policy, not an expanded product contract.

Burden/proof: bounded execution metadata/acceptance changes are necessary for the
explicit cumulative invariant. With small effective limits, cross total/count using
individually valid JSON/files; refuse without a new manifest, retain usage across
reclaim, charge replay once, and show an accepted boundary-size file mounts while an
over-consumer-limit file refuses. Include concurrent acceptance at the shared accounting
authority. Preserve staging bounds and no storage/model IO in a SQL transaction.

### FIND-TASK-001-3 — REVISED / INCORRECT — publication readback discards completed File identity and rediscovers it through analytical inventory

Source: BEH-003; human-approved correction revision above. Obligations: usable immutable
retained content, Task scenario 8, INV-015/AC-041, and existing shared File completion
ownership. Successful publication must supply the authoritative metadata needed for
exact readback; analytical projection lag must not consume attempts or turn an accepted
publication into failed evidence.

Locations in reviewed candidate: server
`verification/hosted/publications.rs:127–137,158–192`,
`verification/hosted/evidence.rs:138–140,265–296`,
`components/verification/service/inspection.rs:456–501`; shared
`wyrd-storage/src/service.rs:632–659,842,889,1532–1546`;
`wyrd-spec/src/verification/execution.rs:453–461`. Card reuse reference:
`components/cards/service.rs:131–148,360–374,2382–2421` and
`components/storage/inventory.rs:1–14,291–327`.

Diagnosis: publication already uses the standardized File initializer, reconciliation,
verified completion and inventory acknowledgement; no replacement uploader is needed.
Shared completion returns the actual StoredObjectRef after object verification and
PostgreSQL completion persistence. Publications::complete returns only success,
discarding that descriptor, while its stored file entry keeps relative path/digest/size.
Both downstream publication mounts and authorized inspection download/content therefore
query Bifrost by digest/size to rediscover the object location. Card-backed reads
instead use their completed PostgreSQL manifest. When the analytical inventory lookup is
unavailable or not visible, generic action/check error handling consumes retries or
settles a failure before any model call, despite already completed content.

Correction: preserve the server-validated completed File descriptor as private
authoritative metadata associated with the existing active PostgreSQL publication
acceptance record and carry that same association into the retained Bifrost manifest
before cleanup, using the shared File representation/completion/readback owner and the
existing Card metadata pattern. Commit the descriptor linkage with the fenced immutable
manifest and projection intent. Resolve publication mounts and inspection
download/content from that exact authorized record through shared storage reads; do not
rediscover by digest through Bifrost or fall back to another object. Keep the public
manifest/receipt and producer selectors unchanged; physical storage references remain
internal and cannot be supplied by the Agent/client. Preserve tenant binding, selected
producer/name, digest/size validation, local verifier-read authorization and normal
cloud download plans.

Use existing File upload/completion, inventory acknowledgement and retention/cleanup
ownership, not a second object catalog, object lifecycle or parallel metadata service;
do not rely on an expiring upload-session record as the publication's durable identity.
Accepted records must continue protecting retained objects under existing lifecycle
rules. Bifrost keeps required inventory/history delivery and becomes the authoritative
retained manifest source after complete handoff. Active readback does not query Bifrost;
retained readback selects the exact archived descriptor through the normal Bifrost
contract instead of rediscovering by digest. Genuine historical source unavailability is
explicit, never false absence. Existing generic inventory readers for other evidence
retain their contract. Do not add publication-specific waiting guards, engine deferral
modes, SDK polling or new freshness controls for a dependency this correction removes.
Actual object-storage failure still uses existing bounded failure behavior.

Focused proof: complete and accept file/directory publications through the real shared
lifecycle, then make inventory queries unavailable or invisible while leaving the
completed objects accessible. A zero-retry downstream Agent action and Agent requirement
mount the exact bytes immediately and dispatch once; authorized file/directory
download/content also succeeds, including after worker/server restart. Assert no
inventory lookup is issued for publication readback, no waiting/retry attempt is
introduced, and existing foreign-tenant/wrong-Verifier/producer refusal and digest/size
checks remain. Preserve existing shared completion ACK requirements when arranging the
analytical lookup fault. Replay after scratch removal still uses the original descriptor
and charges once (finding 1); accepted boundary-size files mount under consistent limits
(finding 2). Exercise upload-session expiry and the existing sweeper after acceptance to
verify completed referenced content survives; keep non-Card publication objects
independent of Card deletion, and do not introduce a publication-specific collector or
claim a general File collector already exists.

Burden: carry metadata already produced by shared completion through the existing
publication owner and readers; this removes an unnecessary resolver dependency instead
of adding a waiting state machine. No new public API, Card kind or Artifact Card per
publication is required.

### FIND-TASK-001-4 — REVISED / INCORRECT — insertion identities do not freeze committed page membership

Sources: BEH-004, DATA-001. Obligations: Task scenario 9 and public-contract stable
first-page cut, no gaps/duplicates while concurrent writes commit.

Locations:
`crates/wyrd/wyrd-sql/src/queries/verifier_executions/inspection.rs:19–55,135,206`,
`verifier_executions.rs:650,690`; invocation/publication migrations. Invocation begins
and publication accepts are sibling writers with independent transactions and compatible
execution share locks; hosted step concurrency is supported.

Evidence/scenario: row seq1 is committed; writer A allocates seq2 but holds its
transaction; writer B commits seq3. First page limit1 reads seq1 with through=3. A
commits between pages, so continuation now includes seq2 which was absent at the first
cut. Another ordering skips a lower sequence already behind after. `max(sequence)`
measures visible allocation, not commit ordering; reader snapshots in separate HTTP
calls cannot recover original membership. Existing page test commits in order on one
connection.

Consequence/materiality: normal parallel work violates the public stable-list promise.
This is inconsistent inspection, not physical evidence loss.

Correction: at the existing execution SQL owner coordinate sequence assignment/commit
across invocation/publication insert siblings and first-cut acquisition so no lower
sequence can become visible after its cut is exposed. Reuse execution-row coordination;
serialize the bounded insert transaction before its sequence allocation and establish
the first cut against that ordering, not filesystem locks or client retries. Keep
storage/model IO outside these transactions and preserve nullable producer identity.
Indexing alone or per-request repeatable-read alone is insufficient.

Proof/burden: real separate transactions hold a lower writer open while higher
work/first-page acquisition are attempted. Continuations equal exactly first-cut
committed membership, fresh polls include later work; cover both paged tables. This is a
small coordination correction using an existing authority, not a persistent snapshot
service.

### FIND-TASK-001-5 — REVISED / MISSING — internal exact-capability resolution has the wrong permission and failure semantics

Sources: BEH-005, SYS-003. Obligations: public-contracts.md:31 internal definitions
under verifier:run without cards:read; REQ-014/AC-025 and Task scenario 7 unavailable
exact Agent/Prompt terminates errored with retained work.

Locations: server `hosted/blocks.rs:150`, full `hosted/registry.rs`,
`runner.rs:338,550,1074`; `vala-verify/src/blocks/agent.rs::resolve/invoke`,
`blocks/engine.rs:808,951` and `Attempted::settle`.

Evidence/scenario: HostedBlocks installs TenantRegistry. It calls the public
`get_card_spec_by_ref_for` which authorizes Card read. A valid exact verifier-run caller
without Cards permission therefore cannot load referenced Agent/Prompt definitions.
Deleted exact Cards are excluded by the Card SQL reader; registry returns Terminal,
action execution turns that into failed StepResult, and Agent requirement errors become
inconclusive. Runner receives Completed rather than execution errored. Runner preflight
checks Verifier/subject only. Recovery after a predecessor checkpoint reaches exactly
this path without substituting any version. Action and check consumers share resolvers;
Artifact/source reads have different authorization.

Consequence/materiality: supported verifier-only execution is refused, and required
capability unavailability is recorded as observed verification judgment, possibly still
passing a threshold. Both outcomes are explicit contract failures at a shared resolver
boundary, not general provider-failure hardening.

Correction: use the existing tenant-bound internal definition lookup under accepted
exact-Verifier execution authority, retaining exact kind/name/version/UID, deletion
checks and audit. Propagate required-definition unavailability distinctly to the
existing queued terminal-error boundary; preserve ordinary provider/model/code/check
failure judgments. Do not grant general cards:read, loosen Artifact/public Card reads,
substitute versions or preflight completed work unnecessarily. Reconcile stale owner
text where it still requires the old public-read permission. One
resolver/error-propagation change covers normal and recovered unfinished work.

Proof/burden: verifier-run-only referenced Agent/Prompt succeeds while explicit Card
fetch still refuses. After committed predecessor work, delete the required Agent and
separately Prompt, resume action/check and assert errored/unavailable, no provider
call/version substitution, unchanged checkpoints/publications. Ordinary provider/check
failure remains a judgment. This is bounded owner reuse, not a second recovery executor.

### FIND-TASK-001-6 — REVISED / MISSING — required Python/TypeScript success journeys are absent

Source: BEH-006. Obligations: Task scenario 9/Public surface closure and root per-shipping-surface user journeys.

Locations:
`sdks/wyrd-sdk-python/tests/integration/test_durable_verifier_work.py::test_execution_pages_are_stable_and_tenant_scoped`;
`sdks/wyrd-sdk-ts/wyrd/tests/integration/durable-verifier-work.test.ts`; existing
`crates/wyrd/wyrd-testing/src/verification.rs:678::publish`.

Evidence: both tests wait for completed requirement-only work and assert zero
publications. Python never calls successful download; TS only calls missing-publication
download. Neither crosses the actual binding boundary with JSON/null, file/directory
manifest, successful plan or unfinished cancellation. Static thin wrappers and Rust/MCP
success journeys do not satisfy the explicit language proof requirement. This finding
asserts missing mandated proof, not an unobserved runtime conversion bug.

Correction: extend existing language journeys against real WyrdTestServer using the
existing trusted publication fixture through the test-only binding/fixture owner, with
minimal exposure needed for seeding. Assert successful JSON/null, exact file/directory
manifests, selected producer and usable plan, viewer denied cancel and authorized
live/repeated cancel through public SDKs. Retain current malformed/foreign/authorization
coverage. No production upload endpoint or Task-003 Agent tools; Task 3 separately
proves content creation through tools.

Proof/burden: run the two exact existing journey selectors after extension and narrow
language/native checks for the changed test seam. Reuse storage correctness proof from
hosted tests rather than duplicating every storage failure in each language. The
explicit Task-001 proof cannot be deferred merely because the Rust-only seeding seam
needs a small test-only bridge.

### FIND-TASK-001-7 — CONFIRMED / INCORRECT — target loading replenishes the remaining deadline

Source: INV-R2. Obligations: REQ-014, Task scenarios 2/7 remaining deadline and no new
work after timeout; PostgreSQL coordination time.

Locations: SQL `verifier_executions.rs:457–474`; server `runner.rs:798–844` claim and `:404` execute.

Evidence/scenario: CLAIM computes remaining_ms at database statement time. Runner awaits
each target resolution in the claim batch before execution. Execute then constructs
`Instant::now() + run.remaining`. Time spent in target reads, subsequent batch
resolution/commit and dispatch scheduling is added back. Near-expiry reclaim can
dispatch a provider after persisted deadline. `loaded` timing is used for telemetry, not
subtracting budget. Engine timeout is correct relative to its incorrect replenished
Instant.

Correction: conserve one monotonic deadline anchored to the database remaining-time
observation through target loading/dispatch, using existing claim timing; alternatively
refreshing the database interval must be at the same owner before dispatch, without
granting elapsed time. Select the anchored claim-to-dispatch deadline approach: target
loading and queue handoff consume that budget. No wall-clock comparison or clock
abstraction. Preserve direct verification and SQL-owned deadlines.

Proof/burden: delay target loading beyond a small persisted remaining budget; assert
timeout and zero provider/action dispatch, then shorter loading leaves only residual
time. Existing reclaim SQL proof establishes persistence but not this handoff. A local
timing handoff correction suffices; no new scheduler.

### FIND-TASK-001-8 — REVISED / INCORRECT — principal revocation and delayed admissions are not ordered

Sources: SECURITY-1, CONC-003. Obligations: REQ-014/AC-025, Task scenario 7, security
owner active-work revocation; frozen grants may be reduced by revocation.

Locations: `wyrd-auth/src/revoke.rs:47–108`; server
`components/verification/service.rs:718–790`, `verification/submissions.rs:236–306`; SQL
`verifier_executions.rs:112–140,629–636,1955,2264,2815`.

Evidence/scenario: enqueue freezes AcceptedAuthority before Scribe→submission handoff.
Revocation suspends principal and cancels visible unfinished rows in its own
transaction. A handoff pending/insert uncommitted during that scan can later create
pending work. Admissions check Cards/idempotency, not shared locked principal state.
Claim and Caller::accepted restore frozen permissions, so pure actions and later
recovery can execute after revocation without a token expiry bound. Scheduled/grouped
admissions also carry earlier authority and use sibling insert owners; earlier
preparation checks cannot order the later commit.

Consequence/materiality: successful administrative revocation can be followed by newly
runnable work under retired authority on ordinary asynchronous delay/concurrency.
Existing active/pending-row cancellation tests do not exercise it.

Correction: order durable admission for subject/accepting actor against the existing
principal suspension/revocation transaction, validating active state while holding the
common principal lock through bounded admission commit. Apply consistently to
submitted/scheduled/grouped admission owners in stable lock order. If admission wins,
revocation sees/cancels its committed row; if revocation wins, no delayed runnable row
is created. Preserve frozen accepted grants, completed records, idempotent replay and
tenant capability types. Do not hold a lock through Scribe/model IO or add downstream
authorization checks/token epochs.

Proof/burden: pause handoff before admission, revoke/commit, release and assert no
invocation dispatch; reverse with uncommitted admission and concurrent revoke, then
assert cancellation/refusal. Cover sibling admission paths at their existing seams.
Common principal/queue transaction coordination is required; an unlocked state check
cannot close the race.

### FIND-TASK-001-9 — REVISED / INCORRECT — new work fences accept database-expired leases before reclaim

Source: CONC-004. Obligations: current lease for dispatch/checkpoints/publication; Task
stale-write refusal; owner says expired lease is never revived.

Locations: SQL `verifier_executions.rs:650–710`; server `runner.rs:1135–1193`,
`hosted/publications.rs::accept`, `verification/leases.rs::renew/renew_tenant`.

Evidence/scenario: BEGIN/SETTLE/publication fenced SELECT checks running/token only.
Expiry leaves both unchanged until reclaim. Renewal rejects expired database leases but
its asynchronous sequential tenant loop cancels local work only after the database
result/error. A pause/delayed renewal can resume a checkpoint or dispatch admission
after SQL expiry and before reclaim/cancellation. Trusted backend accepts ExecutionLease
without a renewal watcher as well. Status/token tests cover reclaim/cancel, not expiry
alone.

Correction: require unexpired database-owned lease time in the existing SQL predicates
admitting new invocation/checkpoint/publication work, not a local clock guard. Preserve
stored receipt/readability, legitimate terminal completion replay and administrative
release; do not mechanically rewrite unrelated lifecycle predicates. Coordinate with
findings 1/2/4 at the same SQL authority.

Proof/burden: expire a lease on PostgreSQL time without reclaim; begin/settle/new
publication refuse, retained evidence remains readable, current re-claim works. Preserve
cancel/replay tests. Predicate correction is smaller and stronger than another
timer/watch service.

### FIND-TASK-001-10 — REVISED / MISSING — completed state can outlive the only final-result payload

Sources: SYS-001, FUP-001. Obligations: REQ-013 and AC-007/008, Task durable
completion/scenario 2; explicitly approved architecture amendment 2 (`spec.md:926–942`).

Locations: server `verification/completions.rs:198–242`, `runner.rs:670–760,1000–1064`,
`scribe_outbox.rs:5–32,1800–1842`; SQL COMPLETE/CLAIM; public
`components/verification/service.rs:260–280`, `verification/inputs.rs:395–445`; shared
client `verify/execution.rs:298–334`; stale `architecture/wyrd-design.md:1837–1840`.

Evidence/scenario: result builder selects final ID and a process-memory write. Both
batched completion and fallback apply queue completion, stage on reserved outbox slot,
commit SQL without durable result ACK. Slot reservation is capacity, not custody. Crash
before Scribe accepts detail/summary loses payload. Completed rows are not claimed;
history projector emits events, never result payloads. Readback joins
result/output/input and returns None when absent; SDK polls completed-without-result
until timeout. No durable recovery owner supplies lost bytes. Existing journey flushes
Bifrost before final restart, outside this interval.

Authority reconciliation: the old owning paragraph permits best-effort result loss, but
the frozen spec expressly strengthens that exact boundary and explicitly authorizes the
Verifier settlement/Bifrost owner amendment. Workflow recognizes approved outcomes and
requires missing owner updates in remediation. The required correction updates that
paragraph within approved authority; keeping best-effort completion would revise the
approved outcome. Non-blocking audit and actual Scribe ACK semantics remain locked.

Correction: use the existing result/completion/outbox handoff to obtain actual durable
Scribe acceptance of the exact selected result before completed SQL
acknowledgement/commit. Reuse ACK notification patterns already used for inputs/events,
distinguishing accepted result from dropped/rejected writes. Preserve selected identity,
detail-before-summary, batched settlement/fallback, lease/cancel fencing, no
transactions across external IO, tenant authority and unrelated service availability.
Lost ACK/commit must leave replayable work from existing checkpoints; do not store
analytical payloads in SQL, reclaim all completed rows, reconstruct results at every
read or wait for Iceberg visibility. Update the owning paragraph in the same correction.

Proof/burden: delay result durable ACK after committed action/checkpoints, interrupt
without draining/flushing, restart same durable backing and obtain the selected final
result without repeating committed provider work. Assert no unrecoverable completed
state and correct rejected/lost ACK/commit handling. This extends the existing handoff,
requiring a bounded ACK path rather than new ingestion machinery. A generic
outbox-drained signal is insufficient because sink success can include terminally lost
writes.

### FIND-TASK-001-11 — CONFIRMED / VIOLATION — required sequence page indexes are absent

Source: DATA-002. Obligation: Task line 30 tenant-scoped indexed backlog-sensitive page
queries and bounded results.

Locations: SQL `verifier_executions/inspection.rs:21–55`; migrations
`20261104000000_verifier_execution_invocations.sql:31`,
`20261104000100_verifier_execution_publications.sql:34`,
`20261105000000_verifier_execution_inspection.sql:20`.

Evidence: invocation/publication identity indexes order
tenant/execution/task/item/iteration/attempt(/name), omit sequence. Inspection computes
max(sequence) and continuation range/order by sequence. Existing event-table sequence
index cannot serve these tables. LIMIT bounds returned count but not the per-execution
scan/filter/sort. This is the exact explicit missing access path, not an unmeasured
latency claim.

Correction/proof: add the tenant/execution/sequence index to each paged table in a new
immutable migration, retaining producer uniqueness and due/leased geometry. Use
representative retained rows to inspect cut/continuation range/order plans and run
existing page proof plus finding-4 interleaving. Two narrow indexes suffice; no paging
abstraction. Keep this distinct from cut correctness: either fix alone leaves the other
obligation unmet.

### FIND-TASK-001-12 — REVISED / INCORRECT — event replay physical identities depend on mutable frame budget

Source: DATA-003. Obligations: REQ-048/Task stable event/sequence/batch replay across
worker restart; Scribe same-identity dedup.

Locations: server
`scribe_outbox.rs:188–190,1237–1249,1306–1325,1370–1410,1510–1536,1580–1587`;
`verification/history.rs:95–161`; config `:331–332`; Scribe `shards.rs:1157` and
`vala-sql/src/queries/scribe_batch_commits.rs::classify`.

Evidence/scenario: SQL restores batch identity/membership. New ExecutionEvents frames
use that batch key but chunks depend on current configured request ceiling; physical ID
hashes key/frame/chunk/start, not immutable event range. At ceiling A physical chunk0
accepts events A,B, ACK is lost. Restart with valid larger ceiling B recomputes chunk0
as A,B,C,D under the same ID. Scribe compares logical digest/count, so same-version
identity contradicts accepted rows and cannot ACK. The assigned head history batch
retries and blocks progress. Capacity configuration is a supported deployment setting;
the new durable key across processes creates this issue although transient chunk
machinery is inherited.

Correction: for durable execution events use stable per-event physical
identity/membership through the existing outbox, derived from retained event identity
under the persisted batch, so configurable grouping cannot reuse an ID for different
rows. Keep event identities/counts and Scribe digest checks; do not simply add budget to
IDs, which can duplicate already accepted events. Do not change ordinary
audit/capture/output delivery or add an ingest service. A single event too large for an
explicitly configured valid request remains a separate ordinary size refusal, not a
reason to repartition identity.

Proof/burden: partially accept a multi-frame event batch, lose ACK, replay through a
fresh sink at another valid ceiling where individual rows fit. All event IDs appear once
and SQL position acknowledges. Event-specific stable membership is bounded and cheaper
than persisting a second transport plan; no expanded deployment compatibility policy is
required.

### FIND-TASK-001-13 — REVISED / VIOLATION — required async snapshot backend blocks its runtime worker

Source: MNT-001. Obligation: `agent_docs/rust.md` prohibits blocking async request paths
without explicit blocking strategy; Task-001 requires trusted snapshot backend even
before Task-003 Agent tools.

Locations: server `hosted/workspace.rs:565–636` (`freeze`), caller
`hosted/publications.rs:113`; nearby existing `Workspaces::open` and hosted tool
filesystem blocking strategy.

Evidence: freeze awaits writer mutex then synchronously traverses/read_dir/reads the
whole tree in one async poll, with defaults 50 MiB/10,000 entries. A stalled filesystem
occupies the Tokio worker and cannot yield to timeout/lease/cancellation work. The
backend is explicitly required and callable through trusted hosted seam/tests; absence
of Agent tool dispatch does not waive its required execution strategy. No measured
service outage is claimed.

Correction: retain strict snapshot/confinement at Workspace and run synchronous freeze
IO using its existing blocking-pool strategy. Writer exclusion must remain owned by the
blocking snapshot for its full lifetime, including if the awaiting future is cancelled.
Release after frozen bytes are owned, before uploads. Reusing best-effort walker would
weaken completeness; spawning a new service is unnecessary.

Proof/burden: retain existing directory completeness/refusal and held-writer proof;
inspect/test cancellation does not release exclusion while detached blocking work still
reads. No synthetic runtime load test. This is a bounded execution-boundary correction,
not new filesystem machinery.

### FIND-TASK-001-14 — REVISED / INCORRECT — authored Python download plan omits required expiry

Sources: REPO-002, MNT-002. Obligations: authored Python stubs precisely project shared
wire contracts; first-class typed File download plan.

Locations: `sdks/wyrd-sdk-python/python/wyrd/verify/__init__.pyi:463–466`; producer
`crates/wyrd-spec/src/storage/download.rs:40–48`; `PyExecution::download` in SDK
`src/verify.rs:359`.

Evidence/scenario: storage always returns get_url and ttl_secs (cloud expiry, local
zero). Inspection/shared Execution preserve response; PyO3 serializes the whole object.
New TypedDict has only get_url, so typed callers cannot access the valid expiry key or
construct a literal of the real fixed record without type errors. TS projection includes
expiry. Green existing typecheck does not inspect that missing access.

Correction/proof: add required `ttl_secs: int` to the existing authored TypedDict, with
owner meaning, and exercise actual-shaped mapping/expiry access in the existing
typecheck fixture. Narrow Python typecheck/lints suffice; no runtime change, new export
or cloud fixture. This one-field correction fully closes both reports.

### FIND-TASK-001-15 — CONFIRMED / VIOLATION — mandatory changed-item rustdoc gate is incomplete

Source: REPO-001. Obligation: root AGENTS changed Rust items require rustdoc, Errors for
fallible items, Panics where applicable; Rust guide includes private/test items and
fields. This explicit merge gate is not waived by materiality preferences or existing
lints.

Validated locations: SQL `verifier_executions.rs:1708::PublicationRow` eight fields and
`verifier_executions/inspection.rs:91–110` thirteen row fields; auth
`revoke.rs:99::cancel_executions` missing Errors; runner `:1164,1173,1182` checkpoint
implementation methods missing item/error docs; engine tests `:2339,2351,2363` Ledger
methods missing Errors (currently always Ok); spec `verification/ids.rs:105–124` new
FromStr/Deserialize/JsonSchema items missing item/error docs; runtime permission
`:1410::verifier_run_is_uid_scoped` and spec ids `:270` publication-name test missing
Panics; server `tests/integration/pg_durable_verifier_work.rs:162,171` call/rule helpers
missing Panics; MCP verification test `:2955–2959` five local tool-name constants and
Rust durable-work journey `:133–134` REVIEW/JUDGE constants undocumented.

Evidence/materiality: item bodies actually contain fallible SQL/parsing paths,
expect/assert/panic or undecorated fields/constants as reported. Type-level/trait docs
do not satisfy the explicitly item-wide gate. This is a bounded mandatory correction,
not open-ended prose improvement.

Correction/proof: document these touched items in place with meaning/invariants, precise
Errors and Panics as applicable, including “never returns Err” where true. Use links
rather than duplicate contracts when sufficient. Reinspect the bounded list and run
formatting/lints for edited Rust; no new checker or behavioral test. Batch with
behavioral remediation; do not create a style-only round or expand to unrelated
pre-existing items.

## Coordination, preserved behavior and non-goals

Publication readback has one authoritative metadata owner per phase: active/finalizing
PostgreSQL publication metadata, then the retained canonical Bifrost manifest with the
same verified completed File association; bytes remain in object storage. Reuse the
existing upload/completion and metadata/readback pattern across ordinary publication
consumption, inspection/download/content and retry. Update the narrow owning
publication/File architecture text where it currently makes publication lookup depend on
analytical inventory; preserve the generic non-publication File lookup contract. The
original supplement's no-attempt-spending guarantee is satisfied by removing this
dependency, rather than implementing a new wait path.

Use one coherent execution-row/identity authority for replay, accounting, commit-ordered
page cuts and new-work fences. Select existing execution-row serialization before
identity allocation, with replay lookup on a fresh statement snapshot after any wait.
Release before uploads, then reacquire and recheck for acceptance; this closes the
concurrent replay interval and aligns with finding 4's page ordering. The independent
validator confirmed this selection is within findings 1/2/4/9/11. Do not introduce
separate competing locks or replay implementations for each finding. Keep principal
admission coordination at the existing principal/queue transaction boundary and
storage/model IO outside transactions. Apply the selected anchored claim-to-dispatch
deadline from finding 7. Preserve ordinary direct synchronous verification, exact
declarations, same-call idempotency and conflicting replay refusal, fresh-attempt
separation, producer-only citations, failed/skipped collection status, tenant RLS, fixed
scoped inspection queries, ordinary Bifrost grants and local verifier-read-only
downloads.

Final-result ACK must mean actual durable acceptance, preserve detail-before-summary and
selected identity, and leave interrupted work recoverable without repeating settled
provider work. Keep audit non-blocking and unrelated server capabilities available.
Event-specific physical identity must preserve the existing Scribe digest checks and
ordinary audit/capture/output delivery. The original settlement/internal-definition
amendments and revision-17 lifecycle amendments are already approved; update their
owning paragraphs with the behavior.

Do not implement Task 2 expansion/speculation, Task 3 Agent tools, a public
arbitrary-path publication endpoint, another ingestion/queue/budget/filesystem service,
SQL analytical payload custody, generalized error promotion, a new checker or unrelated
cleanup. Language success proof uses the existing test-only publication seam. Batch the
bounded declaration/rustdoc work with these fixes. The CLI namespace is unchanged by
this task: the human concern is recorded separately, with no finding ID or approved
rename/alias.

## Ordered Implementation Scenarios

Original diagnosis labels below are stable FIND IDs; new lifecycle AC rows are
separately approved scope. Each new executable branch uses TDD; an already-correct
property uses recorded regression/no-op proof rather than manufactured churn. Exact
selectors are in Acceptance and focused proof.

### 1. Recover the original publication receipt

**Behavior.** Original file/tree/JSON receipt survives lost reply, changed scratch and
simultaneous identical calls; conflicting calls refuse. (FIND-1).

**RED.** Current backend reads scratch first and can return false conflict under
concurrent acceptance. Add/assert the focused gap before the minimum correction; confirm
the listed selector selects a test.

**GREEN.** Use the shared publication identity authority and selected bounded
serialization; return the stored kind/File association.

**REFACTOR.** Keep one identity path for JSON/file/tree and no transaction over upload.
Keep this and the earlier affected scenario proofs green.

### 2. Enforce retained resource limits

**Behavior.** Concurrent publications/reclaim preserve effective count/byte limits and
accepted files are mountable. (FIND-2).

**RED.** Current per-call checks lack durable totals and permit files above consumer
limits. Add/assert the focused gap before the minimum correction; confirm the listed
selector selects a test.

**GREEN.** Charge the existing execution/publication authority once per accepted call
and apply consistent effective ceilings.

**REFACTOR.** Reuse current limit/manifest accounting; no independent budget service.
Keep this and the earlier affected scenario proofs green.

### 3. Read accepted File associations without rediscovery

**Behavior.** Active successors and authorized content/download use exact accepted bytes
despite inventory-query outage; retained reads still work after PG purge. (FIND-3).

**RED.** The current completion descriptor is discarded and reader requires inventory
lookup. Add/assert the focused gap before the minimum correction; confirm the listed
selector selects a test.

**GREEN.** Carry the existing descriptor through active acceptance, running ledger and
retained Bifrost metadata; preserve authorization/integrity/retention.

**REFACTOR.** Remove the redundant resolver dependency instead of adding a new
wait/deferral path. Keep this and the earlier affected scenario proofs green.

### 4. Freeze page membership

**Behavior.** Separate concurrent writers and first-page reads preserve committed cuts
and indexed continuation on both tables. (FIND-4/11).

**RED.** Current sequence allocation can commit out of order and lacks the required
sequence indexes. Add/assert the focused gap before the minimum correction; confirm the
listed selector selects a test.

**GREEN.** Use common execution ordering and tenant/execution/sequence indexes in new migrations.

**REFACTOR.** Preserve producer uniqueness and due/leased geometry. Keep this and the
earlier affected scenario proofs green.

### 5. Load exact required capabilities

**Behavior.** Verifier-run-only invocation loads exact definitions; deletion during
unfinished recovery errors without version substitution. (FIND-5).

**RED.** Current public loader requires Cards read and turns unavailable definitions
into ordinary judgments. Add/assert the focused gap before the minimum correction;
confirm the listed selector selects a test.

**GREEN.** Use the existing authorized internal resolver/error boundary while retaining
ordinary provider judgment semantics.

**REFACTOR.** Do not broaden public Card/Artifact permissions or create recovery-only
resolution. Keep this and the earlier affected scenario proofs green.

### 6. Conserve deadline through loading

**Behavior.** Loading consumes the saved database deadline and expired work dispatches nothing. (FIND-7).

**RED.** Current loading reanchors the remaining interval later and grants time back.
Add/assert the focused gap before the minimum correction; confirm the listed selector
selects a test.

**GREEN.** Carry the anchored deadline through claim/load/dispatch at the existing owner.

**REFACTOR.** No wall-clock comparison or new scheduler. Keep this and the earlier affected scenario proofs green.

### 7. Order revocation with admission

**Behavior.** Both transaction orders cancel/refuse delayed admissions across
submitted/group/scheduled writers. (FIND-8).

**RED.** Current revoke scan can miss in-transit admitted authority. Add/assert the
focused gap before the minimum correction; confirm the listed selector selects a test.

**GREEN.** Use current principal/admission transaction coordination and stable lock ordering.

**REFACTOR.** No per-consumer permission loop or transaction across Scribe/model IO.
Keep this and the earlier affected scenario proofs green.

### 8. Reject expired lease writes

**Behavior.** Expiry without reclaim refuses new begin/settle/publication while old
accepted evidence remains readable. (FIND-9).

**RED.** Current token/status guards alone accept the expired owner. Add/assert the
focused gap before the minimum correction; confirm the listed selector selects a test.

**GREEN.** Apply database-owned expiry to the existing new-work fence.

**REFACTOR.** Preserve terminal replay and administrative release semantics. Keep this
and the earlier affected scenario proofs green.

### 9. Deliver the selected final result durably

**Behavior.** No acknowledged completion loses its chosen result on crash before Scribe ACK. (FIND-10).

**RED.** Current completion settles after process-memory staging. Add/assert the focused
gap before the minimum correction; confirm the listed selector selects a test.

**GREEN.** Use actual durable result acceptance and the revision-17 complete terminal handoff.

**REFACTOR.** Keep result/detail meaning, batching, authority and non-blocking audit.
Keep this and the earlier affected scenario proofs green.

### 10. Replay stable event frames

**Behavior.** Ambiguous partial delivery across valid frame-ceiling changes yields each
event once and advances projection. (FIND-12).

**RED.** Current physical frame identity can represent different rows after restart.
Add/assert the focused gap before the minimum correction; confirm the listed selector
selects a test.

**GREEN.** Use stable event membership/identity through the existing outbox for retained execution facts.

**REFACTOR.** Keep Scribe digest checks and unrelated audit/capture delivery. Keep this
and the earlier affected scenario proofs green.

### 11. Freeze snapshots on the proper execution lane

**Behavior.** Strict complete snapshots keep writer exclusion even if the awaiting request is cancelled. (FIND-13).

**RED.** Current synchronous traversal blocks an async worker. Add/assert the focused
gap before the minimum correction; confirm the listed selector selects a test.

**GREEN.** Reuse the existing blocking strategy while the snapshot owns exclusion until copying finishes.

**REFACTOR.** Keep the strict workspace owner and no replacement walker. Keep this and
the earlier affected scenario proofs green.

### Superseded lifecycle scenarios 12–17

The reported scenario-12 implementation enforces a publication-free direct path
without checkpoints; that assertion is superseded by the approved common state
lifecycle. The unimplemented scenarios 13–17 prescribed request-owned PostgreSQL
publication state, phase-aware reads, terminal handoff/purge and transferred replay
identity. Do not implement those mechanisms.

Their valid obligations are carried into
[Current Task 001](../../tasks/TASK-001-bifrost-execution-state.md): direct publication
without background promotion; complete exact reads; no lost acknowledged state;
replay/cursor/cancel continuity; and independent delivery/control lifetimes.
Original FIND identifiers and immutable review evidence remain unchanged.

## Acceptance and focused proof

| Obligations | Foundation proof and observation |
| --- | --- |
| Original FIND-TASK-001-1–15; REQ-010–015/031–032/046–049 | Diagnoses and scenario proofs retain original closure; finding 3's active descriptor is transferred to Bifrost for retained reads. Findings 6/14/15 are coverage/declaration/rustdoc corrections; use verification/static proof where runtime behavior is already correct. |
| REQ-052; INV-017; AC-046 | Existing logical checkpoint recovery plus the focused worker-format SQL proof below show disposed engines reload settlements, conserve attempts/deadline, refuse incompatible claim/resume without dispatch or reinterpretation, and preserve checkpoint/publication custody for a compatible worker; no event-history replay or completed calls. |
| REQ-053; INV-019; AC-047 | Plain direct write-boundary regression and request-owned direct publication/disconnect cases; same-request references avoid rediscovery. |
| REQ-054–056; INV-017/018; AC-048/049 | Complete terminal retained reads, crash/ACK/source-transfer tests across success/error/cancel/timeout and all public surfaces; source errors are typed, not false absence. |
| REQ-057–059; INV-018; AC-050/051 | Bounded cleanup, cursor/replay/cancel after purge and independent live delivery/state preservation and completed publication-session metadata release; File roots survive cleanup and session expiry. |


Each finding's proof paragraph above is an acceptance criterion mapped to that FIND ID.
All 15 must close; a green historical happy-path proof alone does not close a newly
diagnosed interval. For executable corrections, use Red-Green-Refactor one scenario at a
time and record the expected missing-behavior failure, actual nonzero selection and
passing closure. Documentation/type-only corrections need static parity proof, not
invented runtime tests.

The selectors below are the exact planned closure inventory. Existing tests are extended
where named; new names are planned and must be added/listed before RED. Tests stay in
existing owner targets; no new test binary or harness is prescribed. If an existing test
already proves the full scenario, record that correspondence and its exact selector
instead of creating a redundant test. Run Cargo-backed checks sequentially.

Findings 1, 2, 13: publication replay/conflict interleavings and original receipt
without scratch/storage; cumulative count/bytes across concurrent acceptance/reclaim;
boundary-size mountability; strict snapshot and cancelled waiter exclusion. Server
target:

```bash
mise exec -- scripts/postgres/with-test-postgres.sh -- bash -lc 'mise run db:migrate:all:inner && cargo nextest run --locked -p wyrd-server --features test-support --test integration --test-threads=1 -E "test(=pg_durable_verifier_work::publication_replay_preserves_attempt_identity) | test(=pg_durable_verifier_work::publication_limits_survive_reclaim) | test(=pg_durable_verifier_work::directory_snapshot_is_complete_or_rejected) | test(=pg_durable_verifier_work::cancelled_snapshot_keeps_writer_exclusion)"'
```

Findings 4, 9, 11: out-of-order concurrent insertion and first-cut membership for both
tables, expiry without reclaim, retained reads and index access plans. SQL target (the
existing invocation-cut test is extended; publication/expiry/worker-format tests are
planned). Worker-format proof varies old/current/new formats: incompatible workers
cannot claim/resume or dispatch and leave attempts/settlements/publications unchanged; a
compatible replacement restores the same logical work/custody, with no fabricated
checkpoints for older formats:

```bash
mise exec -- scripts/postgres/with-test-postgres.sh -- bash -lc 'mise run db:migrate:all:inner && cargo nextest run --locked -p wyrd-sql --test integration --test-threads=1 -E "test(=pg_verifier_executions::invocation_pages_keep_their_first_cut) | test(=pg_verifier_executions::publication_pages_keep_their_first_cut) | test(=pg_verifier_executions::expired_leases_refuse_invocation_and_publication_writes) | test(=pg_verifier_executions::worker_formats_preserve_checkpoint_custody)"'
```

Pin representative cut/continuation EXPLAIN evidence with the same SQL page tests.
Preserve existing due/leased index geometry; do not edit old migrations. For finding 1
include the simultaneous identical-call SQL interleaving in its existing backend proof
rather than adding another replay owner.

Findings 3, 5, 7, 8, 10: authoritative completed-File readback with inventory queries
unavailable, zero-retry action and Agent requirement plus publication download/content
and restart, exact verifier-run-only capability resolution/deletion recovery, consumed
load deadline, both revocation/admission orders and sibling admissions, final crash
before ACK without drain/flush. Existing Rust SDK target, planned tests:

```bash
mise exec -- scripts/postgres/with-test-postgres.sh -- bash -lc 'mise run db:migrate:all:inner && cargo nextest run --locked -p wyrd-ai --test integration -P journey --run-ignored=all --test-threads=1 -E "test(=durable_verifier_work::publication_readback_uses_completed_file_metadata) | test(=durable_verifier_work::required_capabilities_stay_exact_during_recovery) | test(=durable_verifier_work::target_loading_consumes_remaining_deadline) | test(=durable_verifier_work::revocation_orders_with_delayed_admission) | test(=durable_verifier_work::final_completion_survives_unacknowledged_result_write)"'
```

Finding 12: ambiguous partial ACK and changed valid request ceiling with individual rows
fitting; event IDs exactly once and projected position advances. Existing cluster
target, planned test:

```bash
mise exec -- scripts/postgres/with-test-postgres.sh -- bash -lc 'mise run db:migrate:all:inner && cargo nextest run --locked -p wyrd-testing --test server -P journey --run-ignored=all --test-threads=1 -E "test(=verification_runtime::event_replay_preserves_identity_across_request_ceilings)"'
```

Finding 6: extend the two existing SDK journeys, retaining their negative coverage and
adding actual JSON/null/file/directory/selected-attempt/download/live-cancel success
across the native boundary. Build only the changed production/testing bindings first
through their existing mise prerequisites. Exact tests:

```bash
mise exec -- scripts/postgres/with-test-postgres.sh -- bash -lc 'mise run db:migrate:all:inner && cd sdks/wyrd-sdk-python && uv run python -m pytest -q -m integration tests/integration/test_durable_verifier_work.py::test_execution_pages_are_stable_and_tenant_scoped'
mise exec -- scripts/postgres/with-test-postgres.sh -- bash -lc 'mise run db:migrate:all:inner && cd sdks/wyrd-sdk-ts/wyrd && pnpm exec vitest run tests/integration/durable-verifier-work.test.ts -t "^execution_pages_are_stable_and_tenant_scoped$"'
```

Finding 14: shared wire/stub field parity and typed expiry access in the existing Python
typecheck fixture; `mise run py:typecheck` and `mise run py:lints`. Finding 15: inspect
the bounded listed items plus any Rust items changed by remediation, with required
Errors/Panics and field docs. No new behavior test for prose.

### Revision-17 foundation proof

The following new tests are **planned** in the existing Rust SDK/cluster targets; list
each actual selector after adding it and before RED. Extend the existing Python/TS/MCP
public-closure cases below to cover direct versus queued, finalization and retained
read/cancel/page behavior after cleanup; no whole-surface suite is required here.

```bash
mise exec -- cargo nextest run --locked -p vala-verify --lib -E 'test(=blocks::engine::tests::checkpoints_restore_settled_blocks_and_count_interrupted_attempts)'
mise exec -- scripts/postgres/with-test-postgres.sh -- bash -lc 'mise run db:migrate:all:inner && cargo nextest run --locked -p wyrd-ai --test integration -P journey --run-ignored=all --test-threads=1 -E "test(=durable_verifier_work::direct_verification_avoids_execution_coordination) | test(=durable_verifier_work::direct_publications_never_become_queued_work) | test(=durable_verifier_work::terminal_reads_survive_execution_cleanup) | test(=durable_verifier_work::retained_pages_and_replays_survive_cleanup)"'
mise exec -- scripts/postgres/with-test-postgres.sh -- bash -lc 'mise run db:migrate:all:inner && cargo nextest run --locked -p wyrd-testing --test server -P journey --run-ignored=all --test-threads=1 -E "test(=verification_runtime::terminal_handoff_survives_ack_and_cleanup_loss) | test(=verification_runtime::execution_cleanup_preserves_independent_work)"'
```

Task 001 proves the backend with existing trusted hosted fixtures; actual Agent publish
tools remain Task 003. Measure query/write counts at existing owner seams with
deterministic assertions, not host load or a new telemetry product. Positive zero-write
scope is execution queue/checkpoints, not authorization, registry misses, gateway or
Bifrost catalog SQL. Pin indexed eligible-cleanup/cursor plans with representative
retained rows and assert old-format work is not falsely backfilled or deleted; exact
missing terminal facts block legacy cleanup until honestly delivered, without fabricated
evidence.

Final remediation checks: format/lints for edited Rust; `check:tenant-isolation` for
SQL/admission changes; `check:deps` for affected crate wiring; `codegen:check` only if
contract/generator roots change; Python stub/type lanes and TypeScript
lint/type/declaration parity only for their changed write sets. Use existing mise tasks,
regenerate artifacts from owners, and record actual commands/results. Documentation
validation is scoped to changed owner text; no full language journey suites, broad
`gate`, all-platform matrix or full docs aggregate is imposed by this remediation
packet. Combined change-level verification runs once at change review.

Completion evidence must map each FIND ID to its correction commit/source and focused
proof, retain original candidate/review context, and request a remediation-scoped
`$wyrd-task-review` on an immutable integrated candidate. Do not merge/push/deploy as
part of this task.

## Material Stop Conditions

Do not weaken original findings, shared File/Scribe acknowledgement, current authority,
stable public responses or accepted evidence lifetime to enable cleanup.
Missing/incompatible historical facts remain explicit and ineligible for purge; never
label incomplete legacy work fully retained. A new archive table family/service,
event-replay engine, SDK backend option, evidence TTL, automatic direct resume or policy
widening requires renewed spec authority. Private layout/helpers remain
implementation-owned.

## Authority Links

[Approved spec revision 17](../../spec.md) · [Public
contracts](../../public-contracts.md) · [Publication
lifecycle](../../output-publication.md) · [Task ownership](../../tasks/README.md) ·
[Original validated findings](findings-validation.md). Original verdict/reports continue
to describe their revision-16 immutable subject.
