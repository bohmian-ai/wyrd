# TASK-008 round 2 verdict

**FIX_REQUIRED**

## Immutable subject and review scope

- Review base / prior candidate: `798b6b0e83595356c0d2328fa541119267e84fb2`.
- Immutable candidate: `cac4a4ce3ba4b6c41d23e81e927f6afbe1de284a`.
- Actual remediation diff: `git diff 798b6b0e8 cac4a4ce3`; original cumulative base `cd0b8e18bd5737267f13c8e72110a06d379d7a0b` remains acceptance context.
- Original task: [008-approval-steps.md](../../tasks/008-approval-steps.md). Approved packet spec revision 8 and Human approval contracts govern alongside repository/architecture authority.
- Prior verdict, validation and required observable closure: [round 1](../008-approval-steps-r1-798b6b0e8/verdict.md), [validation](../008-approval-steps-r1-798b6b0e8/findings-validation.md), [remediation](../008-approval-steps-r1-798b6b0e8/008-R1-close-approval-acceptance-gaps.md).

This is the first remediation review, not a second comprehensive task audit. [scope.md](scope.md) records the navigation map, role selection and unchanged acceptance evidence carried forward. Prior source-backed acceptance remains for closed action/schema, queue_only registration, ordinary permission/audit/tenant checks, first-wins Scribe custody and ACK, stable occurrence identity, supersession, restoration and thin SDK/CLI/MCP projections. Changed producer/consumer paths reopen pending visibility, withdrawn disposition, deadline resolution, identity proof, redaction and pool acquisition only. Historical R1 reports added in this diff are evidence, not additional production scope.

## Independent coverage and reconciliation

| Role | Scope / selection reason | Report / result |
|---|---|---|
| Fresh remediation ponytail discovery | All seven closures, changed code/tests/docs, affected callers, existing-owner reuse and mandatory Rust docs | [task-review-behavior.md](task-review-behavior.md), FAIL |
| Persistent-data/lifecycle domain | Changed state recording, accepted disposition, early-decision continuation, original expiry, cold target load and recovery | [domain-review-data.md](domain-review-data.md), FAIL |
| Security domain | Authenticated actor versus effective subject, real delegation reachability, replay and public redaction | [domain-review-security.md](domain-review-security.md), FAIL |
| Fresh independent ponytail validation | Source, reachability, materiality, burden, closure and specialist coverage | [findings-validation.md](findings-validation.md), FIX_REQUIRED |

All selected discovery reports completed before the fresh validator started. Separate invariant/system roles were omitted because the data specialist traced the changed lifecycle/recovery seam and prior reports cover unchanged custody/queue behavior. No substantial new ownership abstraction or broad cross-tier changes require separate maintainer/repository specialists; discovery covers the scoped changed-item docs and reuse, security covers errors. The validator independently checks that applicability and carry-forward coverage.

[claim-comparison.md](claim-comparison.md) records grouping by cause. Ready-target deadline success and refused-target failure concern different inputs, not a material disagreement. Nested acquisition proposals collapse into one finding. No followup-rev was needed, and the validator discovered no additional blocker requiring independent follow-up.

## Reconciled acceptance and prior-finding closure

| Finding / obligation | Evidence and resulting behavior | Closure |
|---|---|---|
| FIND-008-1, REQ-010/016 reached pending identity while independent work runs | Engine calls Checkpoints::reached; RunState records through sole memory/background writer. Held-extraction journey inspects and decides while running, then same execution passes with one compare read. Runner rereads accepted decisions at park. Stable-ID and existing custody evidence preserved. | CLOSED |
| FIND-008-2, current occurrence only | Retained uses accepted Discarded/Blocked disposition, removes withdrawn occurrences from pending and refuses fresh decision after replay lookup. Journey retains expired history while ci stays waiting and asserts typed 409. | CLOSED |
| FIND-008-3, REQ-010/011 original authored/runtime expiry | Ready-target eight-second stopped-worker test passes. However target resolution refusal is erased by `.ok()`, substituting runtime-only ceiling; ordinary deleted/unavailable target can therefore receive a fresh decision after authored expiry. | OPEN, REVISED |
| FIND-008-4, actor provenance and identical actor retry | Both production actor uses now follow authenticated attribution. Synthetic unit proof exercises only replay with a manufactured receipt. Required ordinary real exchange, persisted receipt/inspection and actor conflicts remain untested; generic Service-subject queued approval is supported. | OPEN, REVISED |
| FIND-008-5, mandatory Panics docs | MCP Opened::start now documents actual assertion panic conditions; assertions unchanged. | CLOSED |
| FIND-008-6, accurate owner contract | Architecture uses approved/rejected, matching closed enum and approved contract; public behavior unchanged. | CLOSED |
| FIND-008-7, public diagnostic redaction | Approval custody traces internal cause and returns fixed safe ServiceUnavailable message/table. Local proof and unchanged derive/HTTP projection establish redaction; held-ACK retry still passes. | CLOSED |
| FIND-008-8, supported cold approval availability | A held tenant transaction precedes Db::Pool resolve; cache miss requests another connection from the same app pool. Cold decisions can exhaust available slots and wait until acquisition timeout. Existing Db::Conn already supports the required lookup. | NEW, CONFIRMED |

Unchanged original-task acceptance rows carry the R1 verdict's evidence. Current rows preserve independent work, async persistence, all-blocked capacity handoff, same ExecutionId, no renewed budget/deadline, no approval-specific clock, ordinary auth, ACK/replay and typed thin projections except the specific gaps above. No new UI, token restriction, workflow owner, storage schema or dependency was introduced.

## Validated ledger and correction

Only the independently retained findings enter this verdict. Full source paths, source-local IDs, producer-to-consumer trace, consequences, materiality, correction burden and proof are in [findings-validation.md](findings-validation.md).

| ID | Classification | Exact correction boundary |
|---|---|---|
| FIND-008-3 | INCORRECT, REVISED | `service/approvals.rs:132–150`: distinguish resolver refusal from a resolved Verifier with no authored timeout; refuse fresh acceptance rather than widening the original ceiling. Preserve already accepted identical replay. |
| FIND-008-4 | MISSING, REVISED | `service/approvals.rs:261–326` and approval/identity integration proof: retain correct production attribution; exercise real delegated acceptance, persisted inspection, A retry and B/direct-S conflicts using existing Service-subject enqueue/exchange owners. |
| FIND-008-8 | REGRESSION, CONFIRMED | `service/approvals.rs:122–145`: use existing `Db::Conn(conn.transaction())` target resolver path instead of acquiring a second connection from the same pool while holding the first. |

Implement the single bounded [008-R2-close-remaining-approval-gaps.md](008-R2-close-remaining-approval-gaps.md) with `$wyrd-implement`. No specification revision is required. This is the first unsuccessful remediation review; the skill's two-unsuccessful-remediation convergence threshold has not been reached. Next review stays confined to these three corrections and affected paths.

## Verification, waits and limits

[verification.md](verification.md) records observed commands: 8/8 Postgres journeys and 5/5 engine/server unit tests passed; fmt and diff checks passed. Green tests do not exercise refused-target fallback, real delegated acceptance or constrained cold-cache pool acquisition. Those failing branches are source-proven, not reproduced in this review.

The wait inventory is in the data report: reached performs memory mutation/background notification only; fresh acceptance adds a sanctioned cold exact-target SQL lookup, whose nested connection acquisition is FIND-008-8; runner adds existing accepted-decision fence/query reread at background park handoff. No new pending-step SQL/ACK, direct object-store or Vault wait was found. Existing durable decision ACK remains authorized. Full clean L1 comparison and broad integrated journeys remain change-review qualification under the R1 task's explicit bounded-proof instruction; this verdict claims no performance qualification.

No full SDK/CLI/MCP suites, Clippy, codegen/docs build, SQL-wide aggregate or benchmark was rerun. A fmt:check launch overlapped the lifetime of the Postgres wrapper; exact overlap with its cargo test was not established. This sequencing deviation is disclosed in verification.md; it does not establish strict Cargo serialization. No further overlap was introduced.

HEAD remained the immutable candidate. Production and prior review files remain unchanged; only the new R2 review reports were written. Nothing was committed or pushed.
