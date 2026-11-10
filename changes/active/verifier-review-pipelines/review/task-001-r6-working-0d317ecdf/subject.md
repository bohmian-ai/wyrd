# R6 focused review subject

## Identity and authority

Repository: `/home/thorrester/Documents/GitHub/wyrd-worktrees/code-review-workflow`.
HEAD: `0d317ecdf2d28d2e77b6f394ebe0f443d256d580`.
R3 restart baseline: `d89356985566ab3492699682942438c25c8322fd`.
Original base: `0e8a31b14d336fadf62e1d8525a7fa030ab6c4eb`.

The user explicitly submits uncommitted R6, as for R5. Candidate is HEAD plus
the submitted working source/authority in subject-sha256.txt and packet content
in packet-sha256.txt. This records the user-requested working-tree exception to
the skill's committed-subject convention; it creates no commit or staging.
Any pinned subject mutation during review blocks it. Review reports are excluded
from the packet pin; no diff, patch or source archive belongs in this directory.

Prior acceptance subject is the R5 working source pinned in
../task-001-r5-working-0d317ecdf/subject-sha256.txt, with its verdict, independent
validation and four reports. R5's sources were not committed; no prior-source
archive is invented. Compare its hashes to current source to identify unaffected
owners. R5 runner, inspection, group support, SQL integration and Bifrost journey
consumer hashes remain identical; architecture and governing rules also match.
R6 changes outbox Result framing, state/SQL documentation, SQL plan assertion
and runtime/durable-work tests. scribe_outbox.rs was identical to HEAD at R5, so
`git diff HEAD -- <that file>` is the exact new writer delta. HEAD/R3 cumulative
diffs and prior reports provide context for the other affected owners; do not
pretend a reconstructible R5 commit exists.

Read AGENTS.md, SQL AGENTS.md, changes/spec-driven-development.md,
changes/implementation-execution.md, agent_docs/maintainer-style.md, TESTING.md
and applicable architecture Scribe identity/fingerprint/retention, verification
terminal recovery and security boundaries. Packet authority relative to
changes/active/verifier-review-pipelines/:

- spec.md approved terminal amendment and unchanged revision-18 obligations;
- execution-state-and-durability.md approved terminal supplement revision 3;
- tasks/TASK-001-bifrost-execution-state.md and R4 original obligations;
- review/task-001-r5-working-0d317ecdf/TASK-001-R6-proof-closure.md, including
  subsequent explicitly user-approved USER-RESULT-IDEMPOTENCY-001;
- prior R5 verdict/findings-validation, preserving human-directed IDs separately
  from numbered implementation findings.

## Scope, navigation and carry-forward

Check closure of FIND19 capacity proof, FIND23 rustdoc, FIND27 plan assertion,
original hosted A/B→C observation, and user-approved verification-result
idempotency. No Task 001 restart, provider/platform/SDK review, new performance
threshold, stronger previously rejected stale-worker fence or TASK-007 work.

Navigation map (starting point, not a coverage conclusion):

- scribe_outbox.rs: ScribeWrite::Result, rows, Frame::shares/absorb,
  ScribeSink::frames/chunks/submit/write, batch_id, result ACK/rejection;
  existing ExecutionState keyed path, group activity and slice batching tests;
- verification/results.rs ResultPayload::result_id and build_as; runner.rs
  common conclude/result/deliver/settle (accepted R5 unchanged production owner);
- Bifrost preprocess logical digest, Scribe warm/durable batch acceptance,
  AlreadyCommitted/Conflicted/replay and existing fence retention boundary;
- pg_verification_runtime: independent staging/restart/partial ACK/conflicting
  result proof, P5 uniqueness, lost sealed ACK, exhausted source/capacity proof;
- pg_durable_verifier_work: actual hosted concurrent A/B→C, held ACK and reclaim;
- SQL verifier_executions library plan test and settle_delivery docs;
  verification/state.rs Counted methods and unchanged state owner.

Carry source-backed R5 verdict/validation and current specialist reports for
common terminal operation, no-second-pass failure retirement, actual control/
refusal producers, exact identity and tenant/claim fences, existing completion
effects, short memory locks, source-aware recovery, checkpoint charge/protobuf,
publication/File security, original bounds and direct fast return. Recheck
dependencies touched by new result framing; unaffected files alone are not
acceptance evidence. R5 verdict remains FIX_REQUIRED until this review closes
its residuals and the approved addition. R6 summary is not an acceptance verdict.

## Specialist applicability and assignments

| Boundary | Selection and source-backed reason | Report |
|---|---|---|
| Persisted result identities, content fingerprints and generic Scribe fences | Fresh data domain reviewer: new Result keyed frames exercise existing dedup across application reclaims, tables, tenants and restart; inspect retained generic mechanics, not a new persistence design | domain-review-data.md |
| Restart/partial delivery/conflict ACK, result consumers and capacity/hosted closure | Fresh system reviewer: new physical Result delivery semantics and actual failure/recovery tests; verify required scenarios and affected consumer/completion effects, plus residual local gates | system-review.md |
| Final closure, rules, evidence and materiality | Fresh ponytail validator after selected discovery: independently checks all residual obligations, addition, reuse and coverage | findings-validation.md |
| Shared-state invariant specialist | Omit a separate fresh invariant role: no RunState/runner state-machine or queue-fence production changes; R5 independent invariant/data/system evidence carries, while new frame identity/ACK invariants receive current data/system review |
| Ownership/abstraction maintainer specialist | Omit: R6 extends the existing frames operation and adds no owner/type/framework; R5 independent maintainer acceptance carries; changed-item rustdoc and test/support quality are checked in closure validation |
| Broad repo/security/platform/SDK specialists | Omit: no new cross-tier interface/token/grant/platform/native-lifetime change; prior source-backed coverage carries, new tenant/table write identity checked by data reviewer |

Reviewers are fresh relative to R6 implementation, each assigned one role and
exact report path. Read actual source/test assertions and evidence, not the chat
implementation summary or other discovery conclusions. No Cargo by reviewers,
no source/test/task/authority edits, no commits. Orchestrator alone runs any
justified focused verification serially; background logs require attribution
before being treated as this candidate's failure. A fresh validator must check
selection/carry-forward and separately validate any blocker it alone discovers.

Convergence scope: finish the existing closure and explicitly approved result
write behavior. No quota of findings and no new hardening requirements.
