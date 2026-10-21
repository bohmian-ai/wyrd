# TASK-001 acceptance verdict

**FIX_REQUIRED**

The candidate has five independently validated, bounded findings. Actual performance qualification also fails the approved absolute targets. Closing the remediation task below does not close that separate acceptance obligation.

## Immutable subject

| Input | Identity |
| --- | --- |
| Repository | `/home/thorrester/Documents/GitHub/wyrd-worktrees/bifrost-source-consistency` |
| Base | `1c5edf72958af8f47c63eb8ce39eb316672e170a` |
| Candidate | `0d3223696f091fb16bb2e0ec36040a249d39b61f` |
| Range | Complete `git diff <base>...<candidate>`, 58 changed files |
| Approved specification | `changes/active/bifrost-source-consistency/spec.md`, revision 2 |
| Original task | `changes/active/bifrost-source-consistency/tasks/TASK-001-source-consistency.md` |
| Delivery context | PR #192, `change/bifrost-source-consistency` into `main`, as supplied by the caller |

Revision 2 overrides the task's stale revision-1 header and always-fresh-catalog mechanics. The stronger authority is available and unambiguous. There is no prior task verdict, remediation or finding closure to carry forward. HEAD remained the candidate throughout review; the tracked source is unchanged. Only this review directory was added. Nothing was implemented, committed, merged, pushed or deployed.

## Independent review evidence

| Required role | Report | Discovery result |
| --- | --- | --- |
| Behavior | [task-review-behavior.md](task-review-behavior.md) | FAIL |
| Invariants | [task-review-invariants.md](task-review-invariants.md) | FAIL |
| Repository standards | [standards-review.md](standards-review.md) | FAIL |
| Maintainer | [maintainer-review.md](maintainer-review.md) | FAIL |
| System resilience | [system-review.md](system-review.md) | FAIL |
| Concurrency | [domain-review-concurrency.md](domain-review-concurrency.md) | FAIL |
| Durability/persistent data | [domain-review-durability.md](domain-review-durability.md) | FAIL |
| Security/tenancy | [domain-review-security-tenancy.md](domain-review-security-tenancy.md) | FAIL |
| Fresh structured Ponytail validation | [findings-validation.md](findings-validation.md) | Five retained findings; FIX_REQUIRED recommended |

The orchestrator established the subject and navigation map in [subject.md](subject.md), compared discovery claims in [claims-comparison.md](claims-comparison.md), and wrote this verdict from the independent validation ledger. Reviewer agreement was not treated as source proof. Every proposed discovery ID received a validation disposition, including the unique release-deadline finding.

No focused follow-up was needed: discovery supplied concrete traces for each reachable proposal, with no unresolved material contradiction about code behavior. The fresh validator resolved correction/severity judgments and capacity disposition from authority. The concurrency reviewer disclosed an overlapping expiry message received after writing its independently traced report; this late corroboration supplied no validation proof.

## Validated finding ledger

The full producer-to-consumer evidence, sibling caller/writer tracing, exact locations, rejected correction scope and focused closure proof are preserved in [findings-validation.md](findings-validation.md). These are the only actionable findings in this verdict.

| Stable ID | Status / classification | Source location and consequence | Discovery IDs |
| --- | --- | --- | --- |
| FIND-TASK-001-1 | CONFIRMED / INCORRECT | `scribe/memtable.rs:1678,1886`; `scribe/shards.rs:2405`: captures/readers keep Arrow allocations after retirement returns their governed charge, allowing growth beyond the shared memory budget. | BEH-001, INV-REV-02, CONC-001 |
| FIND-TASK-001-2 | REVISED / INCORRECT | `scribe/tail_rpc.rs:468,805,877`: expiry can remove an absent ID before insertion; an elapsed capture can also authorize a new open until cleanup is polled. Resources can remain pinned indefinitely. | BEH-002, INV-REV-03, CONC-002, DUR-001, SEC-001, SYSTEM-001, REPO-002 |
| FIND-TASK-001-3 | CONFIRMED / INCORRECT | Server `oracle/tail_discovery.rs:244,304,369`: failed acquisition awaits release after the listing timeout, with no deadline on the release RPC; detached route cleanup has the same unbounded wait. | SYSTEM-002 |
| FIND-TASK-001-4 | CONFIRMED / VIOLATION | Server `grpc/scribe_tail.rs:49,83`; Redux `oracle/follower.rs:158`, `oracle/live.rs:138,193`: five changed methods lack the mandatory error/lifecycle rustdoc. | M-001, REPO-001 |
| FIND-TASK-001-5 | REVISED / INCORRECT | Current architecture/product/diagram/generated prose and execution guidance still allow publication duplication or promise degradation for fatal accepted-capture loss. | M-002 |

Redux source locations above are relative to `crates/vala/vala-bifrost-redux/src`; server source locations to `crates/wyrd/wyrd-server/src`. No speculative hardening, placement preference, unrelated pre-existing SQL redesign, draft-feature implementation or performance optimization was added to this ledger.

## Reconciled acceptance matrix

PASS below is scoped source and available-proof acceptance, not a claim that this review reran tests. Sub-obligations are separated where result behavior passes but resource/deadline ownership fails.

| Obligation | Implementation evidence | Verification evidence | Result |
| --- | --- | --- | --- |
| REQ-001 / AC-001: exact rows across publication, final partition disappearance and rollover | Coherent `LiveCapture::take`, registry revision handoff, stale-cut eviction/repin | Recorded exact-row journey and local rollover/staged/opened-reader proofs | PASS for row selection |
| REQ-002: acquisition before execution, discarded attempts never mixed | Oracle `classify_one_build` / `build_physical_root` validates before output | Recorded moved-revision and warm-cut journeys; cumulative source trace | PASS for selection ordering; failed cleanup deadline is FIND-3 |
| REQ-003 / INV-004: uncertain publication, commit order/replay and recovery | Existing publisher/reconciler extended; transaction-local per-node revisions; boot restoration before capture readiness | Recorded SQL replay/rollback/tenant proof and recovery tests | PASS |
| REQ-004 / AC-003: preserve group workflows on the shared reader | Group scheduling unchanged; same Oracle service | Recorded late-evidence group journey; integrated first-class regression checklist remains change-review work | PASS at task scope |
| REQ-005 / AC-004: one current protocol, obsolete allowances removed | Execution-time fresh acquisition deleted; stale current contract descriptions remain | Source/diff inspection and independent documentation validation | FAIL — FIND-5 |
| INV-001: server owners, no caller workaround | Oracle/Scribe/Vala SQL remain owners; no group reread or alternate SDK path | Cumulative range and reuse/caller tracing | PASS |
| INV-002: capture authority includes deadline | Query/table/stream/epoch/digest propagation correct; no capture-local elapsed-open check | Existing authority tests omit cleanup-not-yet-polled expiry case | FAIL — FIND-2 |
| INV-003: append progress, no remote barrier or append SQL | Brief local capture exclusion; publication-only revision SQL | Unchanged append path, recorded write-capacity passes and rollover proofs | PASS |
| INV-005: invalid selection/lost capture cannot produce complete success | Stale acquisition retried; accepted-source loss fails, listing-only degradation preserved | Recorded negative journey and terminal matrix | PASS for result semantics; settlement deadline is FIND-3 |
| INV-006 / AC-002: governed retention and bounded release | Memory lifetime disconnect, expiry installation/open gaps and unbounded release | Existing tests do not exercise the validated gaps | FAIL — FIND-1, FIND-2, FIND-3 |
| INV-007 / AC-005: actual cost and precise benchmark judgments | Inclusive report comparisons and existing error/sample/refusal criteria preserved | Recorded boundary tests plus retained actual reports/samples | PASS for measurement integrity |
| INV-008 / AC-007: warm queries skip catalog, stale revision repins | Notification cache plus in-memory reported-revision validation, generation-safe eviction | Recorded zero-read warm query/join and exactly one repin proof | PASS |
| REQ-006 / AC-006: actual final capacity passes targets | Correct gates report failed qualification | Retained reports: hot p99 2.352 ms; Parquet p99 2.745 ms; direct-64 overhead p99 bucket 0.9 ms | FAIL — separate qualification blocker |
| Repository rustdoc gate | Five changed methods omit required contracts | Full body/document comparison against actual AGENTS §16 | FAIL — FIND-4 |
| Non-goals / unaffected integrity | No new unavailable-writer recovery, WAL/history scan, selector, second scanner or scheduler; existing tenant/schema/footer/reconciliation protections retained | Cumulative source review and recorded focused/static checks | PASS |

## Performance qualification remains open

The validator directly inspected `../bsc-rev2/{commit,status}` and the verification/hot-query/Parquet-query reports. The measured implementation is `05186c1e443096e6c33d6b5f08831b613fa2caa2`; only task evidence Markdown changes between that commit and this candidate. Recorded correctness checks at `051be28a3` therefore exercise equivalent runtime source.

The final status is write exit 0, verification exit 1, hot query exit 1, Parquet query exit 1. Selective one-client p99 exceeds the approved ≤1.000 ms limit in both tiers; direct verification at 64 clients reports the 0.9 ms bucket against ≤0.5 ms, with 435,378 observations. A baseline that also fails cannot waive these limits. The final capacity sequence also predates the later all-checks pass, so the prescribed final ordering is not certified by those artifacts.

BEH-003 and INV-REV-01 are independently confirmed failed acceptance, without an actionable FIND ID: no source-backed diagnosis establishes a sufficient bounded performance correction. The remediation below must not invent one. Performance investigation and a passing comparable final write → verification → Bifrost query sequence remain required under the original task and approved specification, with query capacity last. Runtime changes from remediation invalidate reuse of this candidate's measurements for final qualification.

The Parquet table aggregate's 56.9 versus 66.4 QPS comparison establishes neither a causal regression nor that the difference is noise; its configured criterion passes. No separate finding is assigned.

No evidence establishes that the approved product/architecture must change to meet capacity; the verdict is therefore FIX_REQUIRED rather than a forced specification revision. Any choice to accept baseline-relative performance or relax thresholds requires explicit human-approved specification revision. This review grants no waiver.

## Remediation and verification limits

Route [TASK-001-R1-retained-capture-lifecycle.md](TASK-001-R1-retained-capture-lifecycle.md) directly to `$wyrd-implement` for FIND-TASK-001-1 through FIND-TASK-001-5. It batches the material runtime and documentation corrections under revision 2, with focused closure proof. It does not authorize implementation in this read-only review turn.

No runtime checks, builds, benchmarks, integrated SDK/agent gates or cloud probes were rerun in review. Existing evidence was assessed against its coverage limits; green tests do not cover the newly validated schedules/accounting paths. The orchestrator ran `git diff --check <base>...<candidate>` successfully and confirmed unchanged tracked source. Integrated change review still owns `verify:bifrost` and the remaining language/agent/contract proof specified by the packet.

The stale task revision/status can be reconciled when recording disposition, without rewriting historical evidence. Carried Verifier drafts remain disclosed drafts, not approval or extra implementation scope. There are no prior findings claimed closed and no missing required reviewer/report. Acceptance remains withheld for the five findings and independently failed capacity qualification.
