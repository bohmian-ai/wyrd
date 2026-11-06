# TASK-001 remediation review scope

## Immutable subject

- Repository: `/home/thorrester/Documents/GitHub/wyrd-worktrees/variant-file-reads`.
- Original base: `5234961d51798581a026834cfffd5efaadc226b4`.
- Prior candidate: `9e8f1d9d0d38fa4f33bbcdffc9d656fa08bb3994`.
- New candidate: `96fea41e147c799354f797469260391d2fc237ff`.
- Primary review: `git diff <prior-candidate> <new-candidate>`, five descendant
  commits, no rebase. Original-base cumulative range remains acceptance context.
- Approved spec: `../../spec.md`, revision 2.
- Original task: `../../tasks/TASK-001-memory-variant-execution.md`.
- Prior verdict and independent ledger: `../TASK-001-r1-2026-11-06/verdict.md` and
  `../TASK-001-r1-2026-11-06/findings-validation.md`.
- Remediation/evidence: `../TASK-001-r1-2026-11-06/TASK-001-R1-memory-execution-corrections.md`.
- SHA-256: spec `6709bdcd455c6b669ebd335af83412d5065622825795ac229dd2e2b6a01dfa9f`;
  original task `dc933d52d5111145011fd431ecc2a88c2b29c54d0006f69e20987eb8103bdf9b`;
  remediation/evidence `7c17c5efa6cbbe199ccc7995c164e4d569007503d8554ea9fdabd6c655221c0a`.
- Source worktree has no tracked implementation modifications; existing
  benchmark-scaling deletions and untracked packet remain outside the source
  subject. No `.codegraph/` exists. Write reports only; do not store patches,
  diffs or source archives here or edit reviewed implementation/authority.

## Round and carried-forward acceptance

This is the first remediation review, following one initial FIX_REQUIRED result.
Review FIND-TASK-001-1 through -5 closure and changed/affected paths. No convergence
assessment is yet required. Carry forward unaffected first-review acceptance:
learned layout adaptation, SQL Variant/null/residual/fallback semantics, fixed
source cohort, original tenant/permission/fence proof, cache/lease behavior,
measurement meanings, existing workflow reuse and excluded writer/persistence/SDK
changes, citing the prior matrix/reports. Reopen only an affected property or a
specific evidence gap; do not restart whole-task discovery.

Human direction remains authoritative: core Bifrost budget/tracker is the one
accounting source per process. Query limits and role/tenant/table attribution are
core-backed handles/views, never a second tracker or independent resource root.

New evidence potentially invalidates the prior finding-2 supported-use claim:
the remediation records that real server SELECT-list scalar subqueries become
joins, so its new SDK journey stays green when output snapshotting is removed.
Recheck that precise reachability/materiality claim and the small in-owner fix;
do not require a fake RED or new SQL/planner capability to defend the prior review.

## Starting navigation map

Reviewers must independently expand/verify this map against actual source.

- `scribe/memtable.rs`: `LiveCapture::read`, `ReadableSource::project`,
  `LiveReadOwners`, fallback workspace, retained output `MemoryReservation`,
  `unshared_arrow_bytes` callers and bounded-pool owner test. Trace registration,
  accounting timing/attribution, original-buffer sharing and output lifetime.
- `scribe/tail_rpc.rs`: new request memory handle, result owners and capture read;
  `oracle/follower.rs::ScribeTailResolver`: session runtime pool forwarded to that
  request, returned owners passed into `MemtableSource`.
- `oracle/memtable_source.rs`: owners retained through source cloning/rewrite and
  open streams; unchanged original charge/retirement mechanism is a dependency.
- `resources.rs`: unchanged `ScribeResources::follower_execution`, governed view,
  core `GovernedMemoryRoot`/governor/tracker and allocation ownership contracts.
  Verify this shared root is actually used, not inferred from a pool type alone.
- `oracle/live.rs::resolved`: outputs snapshotted before predicate early return;
  scalar pending/two-execution owner test and unchanged source/route semantics.
  Follow real server optimizer/planner to reconcile scalar-output reachability.
- `oracle/follower.rs::signed_requirements`: recursive column collection,
  ordinal/name closure checks and propagated Boolean filter typing before tail
  access; recording-tail cases add nested/filter invalid shapes.
- `oracle/codec.rs`: payload fields/output type/builder/parser narrowed; new
  `test-support` filter observation; external peer journey consumes that helper.
- `distributed.rs`: new successful scalar-output journey and panic-free answer
  observations; changed runtime helpers have rustdoc corrections.
- `shards.rs` and persistence tests: mechanical memory-pool parameter updates;
  verify new unbounded handles stay test-only and production uses core resources.

## Specialist applicability and report assignment

| Material contract or question | Selection and source-backed justification |
|---|---|
| FIND-1; new shared query reservation plus original generation lifetime; root attribution; refusal/drop/cancellation | Fresh `invariant-rev`, report `task-review-invariants.md`: new `LiveReadOwners` and reservation/workspace path materially change shared allocation/lifetime behavior not covered by prior PASS rows. Trace core-backed producer-to-stream ownership, including sibling consumers and existing byte owner. |
| FIND-2 reachability and prior supported SQL regression claim versus new optimizer evidence | Fresh focused `followup-rev`, report `followup-review.md`: one material evidence conflict, scoped to actual server scalar-output planning, snapshot path, unit/journey proof and public reachability. No complete system/team restart. |
| All five closure decisions, changed paths, mandatory rustdoc, visibility and complete coverage/materiality | Fresh `ponytail-rev` after selected discovery, report `findings-validation.md`: independently checks actual source and reports, retains stable IDs and rejects unsupported prior claims. It cannot validate a new blocker it discovers itself. |

No fresh broad behavior/repository team: prior reports/validated matrix cover the
original source contract and authorities; fresh validator checks all remediation
changes/closure rather than accepting them by unchanged paths alone. The same
Rust/runtime/test surfaces change, with no generated, SDK, SQL or dependency diff.

No new security-domain review selected: FIND-3 is a bounded correction inside the
already-audited shared closure validator; authentication, authority model and IO
ordering owners are unchanged. Prior security report supplies boundary coverage;
fresh validator must check every new guard and both consumers. A materially new
trust boundary or unresolved sensitive path requires additional domain review.

No new maintainer specialist: `LiveReadOwners` groups existing generation charges
and one existing reservation as required by the prior correction, without another
workflow or resource authority. Prior whole-workflow reuse evidence carries
forward; invariant/validator check that assumption from source, not type names.

No new full system specialist: prior topology/failure report carries forward;
resource refusal/drop effects are assigned to invariant review and scalar query
failure/reachability to focused follow-up. No new deployment, peer version,
supervisor, retry or persistent recovery protocol appears in the range.

No separate concurrency/persistence/SDK domain: no new locks, writer protocol,
SQL/WAL/format or binding contract. Validate omissions independently; obtain
missing specialists if actual behavior contradicts this scoped record.

## Verification and immutable checks

Use appended remediation evidence as claims to source-check, not the user summary
or an intended verdict. Reported P1–P3 RED/GREEN and P4–P6 GREEN, owner/shard tests
and static gates are available; full journey/gate/L1 runs are explicitly excluded
from this remediation proof. Record the P4 limitation honestly. No reviewer runs
Cargo concurrently or edits source. Coordinate any necessary focused runtime proof
with the orchestrator. Recheck candidate and authority/evidence hashes at closeout.

## Discovery complete; validation inputs

Both selected scoped reports are present. Invariant review proposes incomplete
FIND-TASK-001-1 closure in adapted partial-filter/zero-output workspace, while
accepting core linkage and retained-output/lifetime portions. Follow-up resolves
the scalar evidence conflict: configured server planning rewrites the cited
projection scalar examples into joins, while the internal snapshot correction
and P2 proof are task-bounded. These are proposals/evidence for independent
validation, not an intended verdict. The remaining four closure decisions and
all changed-path/mandatory-rule coverage are independently checked by the fresh
validator with prior source-backed coverage available. No additional discovery
follow-up is presently justified; no new finding ID is assigned to the remaining
accounting branch because it concerns the original FIND-1 obligation.
