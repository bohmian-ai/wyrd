# TASK-001 R5 focused review verdict

**FIX_REQUIRED.** The single terminal workflow is corrected. Remaining work is
two explicit proof observations, one regressed SQL library assertion and a few
mandatory rustdoc edits. No further production terminal redesign or specification
decision is established by this review.

Remediation: [TASK-001-R6 — proof closure](TASK-001-R6-proof-closure.md).
Validated finding IDs: **FIND-TASK-001-19, FIND-TASK-001-23,
FIND-TASK-001-27**. The original hosted-dependent observation remains an
unnumbered acceptance obligation, consolidated across reports rather than
misrepresented as another new runtime defect.

## Subject and review scope

[subject.md](subject.md) records repository, authority, navigation, selection
and scope before discovery. The subject is the explicitly submitted uncommitted
working source on HEAD `0d317ecdf2d28d2e77b6f394ebe0f443d256d580`, pinned by
[source/authority hashes](subject-sha256.txt) and [packet hashes](packet-sha256.txt).
This user-requested working-tree review uses content identity rather than
creating or claiming a new commit. R3 `d89356985566ab3492699682942438c25c8322fd`
and original base `0e8a31b14d336fadf62e1d8525a7fa030ab6c4eb` remain cumulative
context. Hashes remained unchanged throughout review; only reports/remediation
were written. No source, tests, task evidence or authority was edited.

This is focused remediation closure under the approved terminal amendment and
unchanged revision-18 obligations, not another Task 001 comprehensive review.
Source-backed unaffected R3 concurrency, data, standards/security, publication,
File, provider and direct-return evidence carries within its original limits.
Affected terminal owners/producers/consumers and prior open obligations were
checked anew. Rejected R4 completion claims do not carry as acceptance evidence.
Unrelated draft TASK-007 choices remain excluded.

## Independent coverage and validation

| Fresh role | Report | Result and scope |
|---|---|---|
| Invariants | [task-review-invariants.md](task-review-invariants.md) | FAIL proposals: terminal ownership/ACK paths pass; capacity and hosted-consumer proof missing |
| Maintainer | [maintainer-review.md](maintainer-review.md) | FAIL proposals: consolidation/reuse passes; changed-item rustdoc and hosted proof incomplete |
| Persistent data/concurrency | [domain-review-data.md](domain-review-data.md) | FAIL proposal: SQL plan test regressed; actual queue fence/tenancy/atomicity/identity boundary passes |
| System recovery | [system-review.md](system-review.md) | FAIL proposals: no additional runtime recovery defect; same two proof gaps |
| Final independent Ponytail validator | [findings-validation.md](findings-validation.md) | FIX_REQUIRED; validates coverage, carry-forward, source, materiality and correction burden |

The validator independently confirms these four specialists cover materially
changed lifecycle, ownership, persistence and failure paths. Separate broad
repo/security/provider/SDK/UI reviewers are omitted with source-backed reasons:
unchanged contracts retain prior independent evidence, while actual modified
revocation/tenant/retained-reader seams have current data/system coverage.
The orchestrator routed evidence and grouped claims; it did not validate its
own findings. There is no unresolved proposal conflict requiring a follow-up,
and no validator-discovered blocker needing separate source validation.

## Reconciled findings and smallest corrections

| Finding / obligation | Validated status and consequence | Selected correction |
|---|---|---|
| FIND19, sources INVAR-R5-19/SYS-R5-01 | REVISED / MISSING. Source recovery passes; required reserved-capacity refusal is never exercised. This is incomplete prior proof, not demonstrated current work loss. | Extend P3 with slot-only reserve/drop on the actual recovery outbox using existing runtime composition. No GiB payloads, new hook, configuration or architecture. |
| FIND23, source MNT-R5-001 | REVISED / VIOLATION. Original three docs close, but new SQL settle_delivery and Counted methods leave the mandatory cumulative documentation gate open. | Local rustdoc at those three items, with real error/infallible contracts; batch with proof work. |
| FIND27, source DATA-R5-01 | CONFIRMED / REGRESSION. Existing plan test rejects the retained delivery index and fails independently. | Restore that index alternative and rustdoc, retaining the index-scan-only property. |
| Original hosted-dependent proof, sources INVAR-R5-PROOF/MNT-R5-002/SYS-R5-02 | CONFIRMED MISSING observation. T6 manually settles state and begins C; no hosted C consumes the values. | Drive existing hosted engine/checkpoint adapter with parallel A/B and real C, held ACK and recovery; no new scheduler or production rewrite. |

The complete producer-to-consumer traces, exact locations, authority, consequences
and correction burden are in the [validated ledger](findings-validation.md),
incorporated here. A claimed need for unauthorized capacity machinery is
rejected: existing atomic reservations and runtime outbox selection suffice.
No optional refactor or stronger distributed-fencing requirement is retained.

## Acceptance and prior-finding closure

| Obligation | Reconciled result |
|---|---|
| USER-TERMINAL-001 / FIND17 common decision→records→ACK→settlement | CLOSED: execute and record converge on conclude and existing result delivery; P1/P2 observe transactional no-second-pass and accepted-failure reclaim; P4/P5 exercise producer custody and all required ACKs. |
| USER-TERMINAL-002 rollback/migration preservation | PASS: recorded rollback and actual rebuild/migration bytes match; historical action is evidenced by the execution record, not solely the final diff. |
| FIND16 retained group/owner/exact identity and permission negatives | CLOSED: frozen custody and T3/P4 assertions. |
| FIND19 final-attempt reconciliation | Source correction and source-outage proof pass; capacity proof remains open. |
| FIND22 complete checkpoint charge and immutable admission/release | CLOSED at source and T5 assertions with recorded GREEN. |
| FIND23 changed-item rustdoc | Original locations close; residual new items above remain. |
| FIND26 protobuf slot/name retirement | CLOSED: tag 10/name state reserved; recorded codegen GREEN. |
| Original hosted A/B→C observation | Still open; current T6 proves state-owner behavior only. |
| Matching claim fences and immediate atomic revocation | PASS: current data/system tracing, actual producer interruption assertions, stale computation and latest-delivery SQL proof. |
| Bounded tenant discovery verification | Production query retains approved owner/index; modified library assertion fails, FIND27. |
| Accepted-prefix loading, shared waiters/short locks, publication/File custody, original bounds, direct fast return, provider/platform boundaries | Carry source-backed prior acceptance and recorded limits; changed terminal dependencies rechecked. |

## Verification, result duplication and convergence

[verification.md](verification.md) separates recorded R5 GREEN (83 server,
34 SQL integration, T5 and gates) from independent review execution. The one
independently run affected SQL library test fails on
`Index Only Scan via verifier_executions_delivery_due`; migrations each pass
apply/idempotence. Source/packet pins and diff check pass. No broad language
journeys, aggregate gate or load measurement was repeated.

At review time, the user's concurrent redelivery question was advisory, not a
silently added acceptance requirement. Current results preserve logical result_id but
new application delivery can create a new physical batch and append equivalent
rows. Existing result readers/group logical dedup/completion idempotency were
traced; no changed-consumer defect was established. This is not a physical-row
uniqueness certificate. Bifrost supports stable keyed writes elsewhere, so
idempotent selected-result writes are a separate bounded design option, not an
unavoidable property of Bifrost or a change implemented by this review.

Convergence: R4's failure was duplicated production ownership; R5 genuinely
removes it and closes terminal ACK/custody/recovery consequences. Remaining
capacity and hosted observations are incomplete original proof, FIND27 is a
fix-induced test regression, and FIND23 is an explicit local static gate.
They are not expanding expectations and do not justify another redesign.
Next review scope is the four closure rows and affected test/support paths,
carrying accepted R5 production evidence forward. Review PASS has not yet been
earned; Task 002's prerequisite completion cannot be claimed from this verdict.

No implementation, staging, commit, push or completion cleanup occurred.

Handoff check: a separate read-only document reader reviewed the approved
terminal context, verdict and R6 without code or author conversation and found
no material comprehension gap. Local report/remediation links, whitespace,
source/packet hashes and final `git diff --check` pass.

## Subsequent user-approved addition

After this verdict, the user explicitly approved idempotent verification-result
writes and requested inclusion in the remediation. R6 now records that addition
as **USER-RESULT-IDEMPOTENCY-001**, separate from independently validated FIND
IDs. Reuse selected result_id as the existing keyed-frame identity for output
and summary in the shared server Scribe outbox; derive repeatable batch_ids via
existing tenant/table namespacing and retain existing Scribe content conflicts.
The approved tradeoff is two one-row frames per result instead of aggregating
unrelated results in a frame. No additional persisted key, writer/store, generic
Bifrost API or retrospective duplicate cleanup is approved.

R6 includes ordered RED/GREEN/REFACTOR proof for cross-slice identity and real
restart/partial-delivery idempotency, exactly one retained row per result/table,
conflicting-content refusal and existing ACK/claim semantics. This amendment
does not retroactively change the R5 review verdict or claim implementation.
Next review includes the explicitly added writer behavior and its affected
consumers, carrying unaffected R5 acceptance forward.
