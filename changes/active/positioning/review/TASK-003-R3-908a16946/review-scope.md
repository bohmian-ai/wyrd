# R3 closure review

Repository: `/Users/stevenforrester/Documents/GitHub/wyrd`.
R3 base: `7bb44f2df` (resolved by Git; prerequisite NOT_STARTED removal).
Candidate: `908a169460550bba10eecfd2758fa9deddd9d4cc`.
Last reviewed candidate: `d7685d129ffee870ab1c497cdcb1206a89bc80c4`.
R3 delta: `git diff 7bb44f2df 908a16946` (three commits, 13 paths).
Also inspect `git diff d7685d129 7bb44f2df` only for dependency seams invalidating
carried-forward acceptance; it is not mislabeled as R3 work. Full original
context remains `5775c3795...908a16946`.

Approved authority is committed SPEC-positioning revision 7 and
SPEC-service-identity-and-telemetry revision 17, current architecture and root/
nested rules. No `.codegraph/` exists. Main HEAD is candidate; implementation
files in the reviewed range are clean. Dirty mockups/planning/TASK004 and
verification-model material are excluded. Review writes reports only; no
source, staging, commit or authority edits.

## Original and prior context

Original TASK-003 working content:
`changes/active/positioning/tasks/TASK-003-runnable-verification-tutorials.md`,
hash `6846032793910170e76c0cc0ec9de2f50cb6ad6f`.
Prior complete review: `../TASK-003-full-d7685d129/` (all six discovery reports,
findings-validation.md, verdict.md and verification.md). Earlier review
history remains at the R1/first-review directories referenced there.
R3 task plus appended execution evidence:
`../TASK-003-full-d7685d129/TASK-003-R3-preserve-first-sample-evidence.md`,
content hash `a0e6da2f93e223a7276607dd0e8985b4291c6b76`.
Hashes pin read-only working documents, not necessarily stored Git objects.
Recheck HEAD/source cleanliness and pins before verdict; candidate changes
invalidate the review.

Retain stable findings FIND-TASK-003-2, -5 and -6. FIND-1/-4 are previously
closed; FIND-3 is superseded by revision 7 and its replacement passes. New IDs
begin at 7 only for independently validated new material failures.

The maintainer explicitly narrowed FIND-6 after review: preserve WAL durability
and use two plain sentences about background writing and possible pre-WAL
loss. Remove the fixed deadline/unconditional promises; do not demand queue
mechanics, failure inventories or shutdown-budget detail in user flows.
That human direction and the amended R3 task override more elaborate earlier
remediation wording. Do not reopen that accepted communication choice.

## Bounded review and navigation

| Obligation / contract | Changed owner and affected consumers | Review coverage |
|---|---|---|
| FIND-5 cold/warm enqueue causal evidence | `observe/mod.rs::{record_or_stage,finish,Evidence::Window}`, stored Option<TraceContext>, `transport/trace.rs` existing producer scope; Node NativeRun stage/spawn and Python blocking/Rust submit siblings | Targeted evidence-domain discovery then fresh independent validator; follow trace producer/caller, successful admission, startup failure/cancel, native errors and real timeline assertions. No whole tutorial restart |
| FIND-6 simple before/after WAL durability wording | Three concept/FAQ/investigation paragraphs and generated llms-full.txt | Validator checks narrowed user instruction, actual outbox contract and generated coupling against prior source-backed proof |
| FIND-2 cancellation/progress rustdoc and link repairs | start_gated, BifrostLifecycle::flush, WyrdState::flush; existing Trace and benchmark-label doc links | Validator checks exact missing contracts and ordinary documentation corrections, preserving all prior closed sites |
| Proof for cold and warm paths | TS action-timeline existing scenario identifies each producer parent and consumer link, distinct record count; supplied concurrent-row and Python timeline regression runs | Domain checks source/assertions, validator checks adequacy; orchestrator owns exact focused checks |
| Prerequisite catalog deletion seam | d7685d129→7bb44f2df removes unreachable NOT_STARTED methods/error, uses current/Option and invariant expects, adjusts test-support metrics | Targeted validator source correspondence; confirm start/shutdown and warm samples retain prior accepted semantics, public first-write offline error remains transport failure |

Carry forward source-backed full-review acceptance for four tutorials, native
judgments/Trace lowering, configuration/provider ownership, startup lifecycle,
shared pool/admission and tenant/storage authority. Reopen only changed trace
handoff, dependency seams and exact docs corrections. No new whole-task
discovery or repeated search for rejected provider-race/stub-layout findings.

## Specialist selection before discovery

| Role | Applicability decision and evidence |
|---|---|
| domain-rev (trace evidence) | Required targeted review: correction changes when a caller-runtime producer is created and carried into deferred row admission, across shared Rust/Node/Python paths; includes the reported startup-failure attempted span |
| fresh ponytail-rev | Always required after discovery, including empty proposal union; checks prior closure, actual source/materiality and complete carried-forward coverage/applicability |
| behavior-rev | Omitted: remediation scope with complete prior behavior matrix; fresh validator checks closure and affected paths |
| invariant-rev / system-rev | Omitted provisionally: phase/gate/provider/outbox implementations are unchanged in R3. Prior full reports cover lifecycle/concurrency/recovery; domain+validator check whether new immutable producer handoff invalidates those guarantees. Add targeted review only on material unresolved evidence |
| maintainer-rev | Omitted: one optional immutable context field and existing trace-scope reuse, not a substantial ownership/abstraction change. Prior workflow ownership evidence carries; validator independently compares existing mechanism and correction burden |
| repo-rev | Omitted: bounded comments/prose/generated projections and existing trace handoff; no broad new tier or public signature. Prior standards coverage plus validator's direct changed-item rustdoc/declaration checks covers this delta |
| other security/tenancy/persistence domains | Omitted: no changed auth, tenant, wire/storage or server durable owner; prior coverage checked against unchanged dependencies |

All selected reviewers get this map as a starting point, not a verdict. Only
assigned reports may be written. No reviewer runs Cargo/tests or spawns agents;
orchestrator owns verification. No patches/source archives in review directory.

Reported risk to assess under existing obligation, not automatically retain:
producer creation now records a submission attempt before writer startup; a
failed start can leave a producer span without a stored row. Determine whether
that violates an actual supported trace/row contract or is a truthful attempt,
preserving typed error/refusal and no fabricated row/ack. Reachability alone
does not make it a blocker or justify lifecycle redesign.

## Verification / hot paths

R3 task evidence supplies original RED (missing cold producer), GREEN exact
one-test Node timeline selection; observation-surface7, Python timeline1,
client lib290; Rust/TS checks and docs checks/generated sync. Those are reported
runs, distinct from new orchestrator checks recorded in verification.md.
Do not demand broad journeys/gate at task remediation.

Prior full review independently classified designated verification/gateway/
Oracle/Scribe request owners unchanged and inventoried cold startup waits.
R3 does not change those owners or add external IO to shared trace capture;
validate that assessment, not merely filenames. If an actual defined hot path
is materially touched, the mandatory wait inventory and full clean comparative
L1 engine VALID evidence still apply; no benchmark waiver is granted.
