# TASK-001-R4 — Correct source authority and canonical replay activity

Status: READY FOR IMPLEMENTATION through `$wyrd-implement`.

## Authority and immutable review subject

- Approved spec: `changes/active/long-horizon-verification/spec.md`, revision **15**, approval `4a4ea57b9`.
- Original task: `changes/active/long-horizon-verification/tasks/TASK-001-grouped-verification.md`.
- Base: `ef3a57b0776dd7bc451273b9176498fcca700550`.
- Reviewed candidate: `9f9f7743311aec0f2191f775e7bf054aca00ff91`.
- Worktree/branch: `/home/thorrester/Documents/GitHub/wyrd-worktrees/long-horizon-verification-task-001`, `change/long-horizon-verification-task-001`.
- Independent diagnosis: [findings-validation.md](findings-validation.md), [verdict.md](verdict.md), prior R1–R3 packets and approved owner decisions.

Preserve uncapped identities, the payload-only ceiling, one Bifrost statement for membership/count/value, PostgreSQL-managed group times, and newest-64 discovery without a cursor. Prefer preventing invalid source evidence at its owner. TASK-002, TASK-003, REQ-015, retained upgrades and optional refactors are excluded. Findings 3–8 are closed and impose no new remediation work.

## Diagnosis and selected correction

FIND-1 and FIND-2 share one cause: absence from a queue observation is treated as positive direct origin, although Scribe visibility and queue acceptance are independent. Correct this once at **GroupPoller/Sources with VerifierExecutionQueue's existing classification**. FIND-9 is a separate physical-to-logical activity normalization defect in that same source owner. Do not repair these in SDKs, Vala, runner settlement or Operator consumers.

### FIND-TASK-001-1 — INCORRECT: a newly queued result crosses the classification cut

REQ-002/003/004/005 require selected authoritative queued outcomes and immutable correct snapshots. In `crates/wyrd/wyrd-server/src/verification/groups.rs:141–155,458–480,512–547`, group_submissions is read before the independent Bifrost query. A result's execution absent from that earlier map satisfies NOT queued and is treated as direct.

Reachable sequence: poll observes no X; asynchronous acceptance and claim commit X; runner stages R while completion remains uncommitted or eventually fails (`verification/runner.rs:474–565,806–855`); the later Bifrost read returns R. An already max_age-due group can pin this unselected/abandoned outcome and issue a wrong verdict-change reaction. The current visible-result proof creates X before polling, so it closes the known-row case only. Later judgments cannot retract the bad immutable history or external action.

**Outcome and selected approach:** after the complete Bifrost group read, before deciding end/coverage/enqueue, validate relevant captured result authority using the existing queue group classification through the **same caller-owned TenantConn**. If a captured result relied on an initially unknown execution which is now queued/refused, defer that entire affected group read to the existing next-poll path. Reuse `group_submissions`, whose committed acceptance is durable before any queued result can be claimed/produced. The later statement observes that acceptance under the repository's plain-BEGIN READ COMMITTED transaction. Acceptance after the later observation cannot have produced a result already captured by the completed analytical read.

Do not postfilter an already-computed count/value result and call the remainder complete: its earlier overflow gate may have omitted payloads. The next accepted read must again compute membership/count/values together. Unrelated queue arrivals need not invalidate safe captured evidence; preserve other groups and existing scheduler cursor/error/transaction ownership. Known queued selected results retain existing authority; later selection is discovered on the next tick. Direct result-only evidence remains eligible. The validation belongs at this real cross-store source-authority boundary; it is not a restored separate-payload completeness guard.

No extra connection, isolation setting, lock, result-origin schema contract or companion-input requirement is selected. Those add resource/contract decisions or remove approved result-only evidence without being necessary. Do not move result staging after completion commit, change lease/settlement durability or add producer IO.

### FIND-TASK-001-2 — INCORRECT: visible submitted input precedes acceptance/refusal

REQ-001/002/005/009 and INV-004 require conflict/refusal exclusion before source activity/count/values. Input builder records `queue_execution=true` for submissions (`verification/results.rs:293–354`), while direct staging records false. Scribe ACK precedes the independent SubmissionOutbox acceptance transaction (`scribe_outbox.rs:881–933`, `verification/submissions.rs:84–95`, queue acceptance `wyrd-sql/src/queries/verifier_executions.rs:1233–1388`). Delays or key-lock retries leave a submitted input visible without a queue row.

Sources drops this marker and excludes only known refused IDs (`groups.rs:430–480`). A due group can pin conflicting content or falsely exceed ceiling N with N accepted sources plus the unresolved attempt. Later refusal cannot retract immutable inputs, and shrinking time/member coverage can suppress a corrected judgment. Existing refusal tests wait for queue status before polling and do not exercise this interval.

**Outcome and selected approach:** retain the existing input discriminator in source eligibility. Direct inputs remain eligible immediately. Marked submitted inputs contribute to the one statement's activity, count and payloads only when an existing non-refused accepted queue identity is known. Reuse GroupSubmissions' accepted keys and source predicates before capacity/value production. Accepted pending/running/retrying sources must join without waiting for results; genuine missing outcomes remain null. Acceptance not yet observed can safely join a later tick. FIND-1's later authority validation remains necessary for result-only provenance; this input predicate alone does not close that sibling writer.

This prevents invalid evidence at its source rather than compensating after judgment. Add no pending-turn state, terminal readiness wait, producer lookup/write, new outbox/store or staging reorder.

### FIND-TASK-001-9 — INCORRECT: a physical replay refreshes logical activity

REQ-001 and REQ-003/004/005 require identity-deduplicated evidence; AC-001/J-001 explicitly forbids an idle refresh on replay. Submission::new reuses the execution ID for identical keyed requests (`verifier_executions.rs:798–825`), but every enqueue stages a fresh recorded_at. The encoded batch hash differs (`scribe_outbox.rs:811–845`), so its later managed ingest timestamp remains visible even though queue acceptance correctly replays one execution.

`groups.rs:512–547,553–617,620–650` deduplicates payload choice/list membership but retains every physical time for Ended. A later copy advances through and postpones idle; after settlement it can fail covers and invoke the same group judge again despite unchanged evidence. Existing replay-before-review and same-time duplicate tests do not cover this. This is a new normalization cause, not another ID for the queue-acceptance gap.

**Outcome and selected approach:** normalize eligible logical activity once in Sources/Ended so first/newest/end/coverage use canonical identity activity consistently with payload/list membership. Reuse the earliest visible copy of each logical input and authoritative result. After selected-result authority chooses at most one result per execution, existing surface/execution identity distinguishes its physical copies; a new selected result remains activity distinct from its input. A deliberately new execution remains new activity.

Keep all times PostgreSQL-managed. Preserve visible/retained-only evidence limits and one-statement membership/value selection. Add no historical first-seen inventory, timestamp rewrite, producer guard or change to Scribe batch identity. Apply normalization after eligibility, rather than teaching queue covers or each caller to ignore replay symptoms separately.

## Constraints and preserved behavior

- Keep exact tenant/owner/subject/Verifier/group pins, accepted owner grants, ordinary query/tool admission and audit.
- Preserve direct result-only evidence, direct inputs, accepted nonterminal sources and genuine null outcomes.
- Keep the uncapped identity set, single membership/count/value statement, payload-only ceiling and no overflowing-tuple payload collection. Never accept a stale read's postfiltered overflow count/payload gate as a fresh snapshot.
- Keep PostgreSQL-managed row/tick times, checked duration domain, finality, immutable history/retry inputs, serial outstanding fence, first baseline and verdict-change-only dispatch.
- Keep existing caller-owned TenantConn lifecycle, read-error deferral and earlier whole-group progress; do not affect unrelated groups/services or alter source acceptance/claim/settlement/Scribe durability.
- No new connection, lock, setting, dependency, persisted origin, receipt, queue, lifecycle store, public fault/control API, readiness barrier, producer IO, cursor or broad refactor.
- Preserve the deleted independent payload-cut/completeness machinery as deleted. The previous successful selected-payload-omission proof is superseded by the one-read invariant; do not recreate it.
- No TASK-002 schedule inputs, TASK-003 staging/pressure/progress, capacity benchmark qualification or retained-table upgrade.

## Acceptance and focused proof

| Finding | Focused closure |
|---|---|
| FIND-1 | First classification lacks X; then existing acceptance/claim commits X and an unselected result becomes visible before the analytical read completes in an old max_age-due group. No affected snapshot/reaction enqueues. A fresh poll excludes it until selected; selecting that retained result without republication yields one correct immutable snapshot, then no duplicate. Preserve direct result-only membership. |
| FIND-2 | With N accepted sources at ceiling N, expose a marked input before acceptance. It contributes neither identity/value/activity nor overflow. Commit its conflict/refusal and retain clean history/no contaminated dispatch. Separately accept an initially absent source pending/running and show later membership with null outcome, without terminal wait. Direct input remains immediate. |
| FIND-9 | Replay the exact keyed request after a first review settles under real managed ingestion time: one source execution and unchanged values/activity/history, with no second judgment after idle. Replay while open must not refresh idle. A distinct execution still advances activity. A focused duplicate-identity/different-time unit may pin the normalization; the real queue/group seam must prove production replay reaches it. |

Reuse existing group/query/queue/Scribe and local provider fixtures. Exercise allowed states directly; no network commit failure, public fault endpoint, synthetic load, new general harness or broad language suite is needed. Private helpers, local control flow and fixture structure remain implementation-owned.

Run every specifically named new/changed test with an exact module-qualified `mise exec -- cargo nextest run --locked -p ... --lib/--test ... -E 'test(=...)'` command and record selected counts. Database commands use:

```sh
mise exec -- scripts/postgres/with-test-postgres.sh -- bash -lc 'mise run db:migrate:all:inner && <focused-command>'
```

Server seams use `-p wyrd-server --features test-support --test integration --run-ignored=all`; Rust SDK journeys, if touched, use `-p wyrd-ai --test integration -P journey --run-ignored=all`. All tests/builds/checks through mise, Cargo lanes sequential. Re-run any failure with WYRD_LOG before diagnosis and record the trace-supported cause. Do not mutate due.clock or producer timings to simulate coordination; use real PostgreSQL time/binding durations. No synthetic host load.

Run only focused regressions for the actual write set and applicable format/lints/tenant checks; dependency or generated-contract lanes only if those boundaries change. Build monty-worker as needed. Do not require complete SDK journey suites or broad aggregates in this remediation; change review owns those once. Append per-finding source closure and exact focused execution evidence. A later task review reassesses the full cumulative immutable candidate against revision 15, the original task and all retained findings.
