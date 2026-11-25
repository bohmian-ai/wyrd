# TASK-004B-R2 re-review — FIX_REQUIRED

FIND-TASK-004B-1 through FIND-TASK-004B-7 are closed within the recorded
functional evidence and confirmed validation ruling. R2 implements explicit
loading and honest stale totals. One mandatory documentation gate remains:
FIND-TASK-004B-8, two missing Rust item comments. The three submitted visual
decisions remain unapproved. The bounded follow-on is
[TASK-004B-R3](TASK-004B-R3-document-cancellation-and-record-visual-rulings.md).

## Immutable subject and authority

- Repository: `/home/thorrester/Documents/GitHub/wyrd`.
- Candidate: `7cf5bc4abb1462e5783df67ff6d0a2a5f2a06b14`.
- Prior review: HEAD `79a4ef66190e572c47aa93351e7362329f7bc73f` plus its
  fingerprinted three uncommitted UI test files. Baseline `41f2419c8` commits
  those test files before R2; its diff from that HEAD touches only those owners.
- Focused range: `git diff 41f2419c8 7cf5bc4ab`; cumulative prior range
  `79a4ef661..7cf5bc4ab`, original candidate `7e2b3f6fa` available. No ancestry
  rewrite prevents carrying forward unchanged prior evidence.
- Approved SPEC-verification-workflow-ui revision 6, supporting UI revision 28,
  original TASK-004B, prior reviews and R1 Revision 1/R2 acceptance apply.
- R2 task and evidence:
  `../TASK-004B-2026-11-19-01/TASK-004B-R2-complete-support-lifecycle.md`.

The current caller confirms the recorded per-host-request validation ruling;
that explicit decision replaces FIND-7's original per-client-refresh criterion.
Root/SDK AGENTS, Rust item documentation rules, applicable architecture,
spec-driven development, maintainer style and TESTING otherwise remain binding.
No architecture, approved contract, implementation or prior review was edited.

HEAD stayed at the candidate throughout review. Relevant implementation files
were clean; unrelated working-tree skill/design changes were preserved. The
twenty lifecycle captures are uncommitted supplied evidence, not part of the
candidate's Git tree; their inventory digest is in `verification.md`.

## Independent coverage and finding validation

Fresh `r2_ponytail_rev` checked prior finding closure, current requirements,
loading/pending/count behavior, actual assertions, supplied images, proof and
TDD. Its complete matrix and validated ledger are
[findings-validation.md](findings-validation.md).

Fresh `r2_domain_rev` separately reviewed the material public TS/native
cancellation boundary and affected UI consumer. Its report is
[task-review-cancellation.md](task-review-cancellation.md). That specialist
discovered C-01; the primary independently checked source and the mandatory
documentation authority before retaining it as FIND-TASK-004B-8. No reviewer
validated its own new finding. All selected reports are present. The
orchestrator packaged the result without substituting its own source review.

A separate SDK specialist was necessary because R2 adds native cancellation
ownership and public signal options. Additional repo/system/server reviewers
were omitted: prior closed behavior is unchanged, and the fresh reviewer and
specialist cover the affected consumers and failure paths. No storage,
migration, deployment or server hot-path behavior changes in this range.

## Reconciled coverage

PASS applies within the inspected implementation-record executions; review
did not independently rerun runtime commands. Detailed source locations and
assertions are in the independent reports.

| Existing obligation | Implementation / proof | Status |
|---|---|---|
| FIND-1/5, REQ-014, original scenario 2 | Matched stat bucket projection, sparse-trend and populated cutoff/token/currency proof remain unchanged from accepted R1 evidence | PASS carried forward |
| FIND-2/3, original scenario 5 | Population-specific support, list-only isolation and complete automatic refresh settlement preserved; same-population Live test confirms no updating flicker | PASS |
| FIND-4, R2 pre-header SQL and Card rows | Executor forwards signal; TS listener precedes native operation; NativeAbort drops the pending shared-client future. Real browser header holds observe SQL/Card upstream closure, supported by public SDK silent-server socket-close assertions | PASS for client-owned cancellation |
| FIND-6, REQ-018/AC-012 | Accepted explained-query negative-boundary assertions and production source unchanged | PASS carried forward |
| FIND-7 under current explicit ruling | Each host request validates once before dependent reads; invalid/denied input issues no dependent query; standalone/list-only guarantees retained | PASS under ruling |
| R1 Revision 1 functional loading | Retained population marked Updating/aria-busy; initial Observe loading/error panels; stale summary totals marked; Verify old-total counting/paging suppressed; browser/unit assertions | PASS |
| R1 Revision 1 visual acceptance | Representative captures inspected; overlay/hint overlap, boxed loading and existing dark background cutoff still await maintainer rulings | UNRESOLVED |
| Mandatory new Rust-item documentation | `NativeAbort.aborted` and `Default::default` lack required item rustdoc | FAIL: FIND-TASK-004B-8 |

## Remaining correction and visual decisions

The retained finding's full record appears once in `findings-validation.md`.
It is a VIOLATION of the explicit merge gate, not a newly alleged runtime bug.
The minimum correction is two concise rustdoc additions at
`sdks/wyrd-sdk-ts/native/src/lib.rs:324,346`; no logic, signature or test rewrite.

Visual rulings are already-required human scope, not new finding IDs. The
reviewer inspected six representative supplied images and confirmed the
explicit loading behavior. The maintainer still must accept or request changes
to the Updating overlay (which overlaps the facet hint), boxed initial loading,
and existing full-page dark background cutoff. This review cannot grant those
rulings. The approved R1 criterion says: “Any intentional visible difference
requires the maintainer's ruling before acceptance.” Evidence preparation is
not that ruling. R3 records the pending decisions without prescribing a new
design or background repair.

## Verification and hot-path limits

See [verification.md](verification.md) for candidate attribution and limits.
The implementer records six browser journeys, 25 UI tests, ui:check zero type
errors/95 tests, SDK checks/91 unit tests, and Rust SDK fmt/clippy. Review
inspected assertions/source and supplied captures; no build/test/codegen/lint
was independently run. Root confirmed unchanged HEAD and clean diff whitespace.

Cancellation proof is actual client-owned HTTP closure, including startup and
Card enumeration. It does not prove server CPU/planning/downstream termination.
R2 now reaches the existing shared Rust client's request/response abandonment
contract; no new server cancellation guarantee or API is inferred.

Every R2 change adds or moves zero waits on PostgreSQL, Scribe, object storage
or Vault. Signal setup/listener cleanup and native watch notification are local;
they cancel existing client futures. UI loading markers add presentation state.
No server production hot path, coordination write, audit acknowledgement wait
or new auth/transport implementation is introduced. The prior hosted benchmark
override remains a performance limit, with no new qualification claimed.

## TDD and convergence

Preserve the original task's nonbehavioral RED/test-order violations, R1's
malformed-JSON test-after-fix and R2's SDK test-after-native-signal violation.
Browser RED and later mutation strengthen proof but do not restore test-first
history. No retroactive RED or production churn is required.

After two unsuccessful remediation acceptance rounds, the functional work has
converged: original cancellation/loading gaps are corrected and duplicate
validation follows the explicit maintainer ruling. The remaining code gate is
R2-introduced documentation, plus the already-known visual decisions. Neither
justifies reopening analytical work, adding guarantees, or repeating the whole
test matrix. This is not an automatic PASS or a round limit.

Verdict: **FIX_REQUIRED**, FIND-TASK-004B-8; visual acceptance separately
unresolved. No implementation, commit, merge, push or deployment follows this
review. R3 and future review remain caller-owned.
