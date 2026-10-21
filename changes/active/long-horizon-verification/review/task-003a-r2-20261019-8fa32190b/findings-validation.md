# Independent structured Ponytail validation

Result: **FIX_REQUIRED**. Final deduplicated ledger: **one finding, FIND-TASK-003a-9**. The three R2 implementation corrections are closed; the retained correction is documentation and generated-reference parity only.

## Immutable subject and independent method

- Repository: `/home/thorrester/Documents/GitHub/wyrd-worktrees/long-horizon-verification-task-001`.
- Cumulative base: `22335c2c40a5ad24c27bfa0d4686f4da49f922c5`.
- Candidate: `8fa32190b26b11588721e8db44b3eb94139e3739`.
- Remediation locator: `8376bb2aa..8fa32190b`; this is not the acceptance scope.
- Authority: approved revision-20 specification, original TASK-003a including A9/current-doc closure, R1 as superseded by revisions 19/20, amended R2, AGENTS, agent rules, spec-driven-development, maintainer style, and applicable grouped/runtime architecture.

This validator is fresh relative to implementation and discovery. Read the subject, all seven discovery reports, claims comparison, fresh follow-up, and verification record. Used CodeGraph first for GroupPoller/poll, Visible/missing_inputs and Scheduler/hand_off; then inspected actual cumulative changes, current governing paragraphs, complete affected function bodies, sibling writers and the consumed generator. HEAD and tracked-source status were checked unchanged. Only this report was written. No build, test or PostgreSQL lane was run by this validator; root's recorded focused proof is identified separately below.

## Independent claim reconciliation

| Proposed claim / uncertainty | Validation | Source-based decision |
|---|---|---|
| INV-R2-01: current incomplete-snapshot failure contract absent from published docs/catalog | **CONFIRMED**, consolidated in FIND-TASK-003a-9 | Current guide promises visible-row judgment, grouped architecture allows missing whole inputs without the revision-20 exception, and generated catalog omits the emitted error. Original A9 and current spec closure require parity. |
| MREV-R2-01: architecture/guide still describe pre-revision-20 behavior | **REVISED**, same FIND-TASK-003a-9 | Qualify the existing paragraphs rather than abolishing all nulls. Input present/result absent and absent selected values remain valid. This is one approved failure-contract correction, not a runtime regression finding. |
| STANDARDS-1: generated error row absent | **CONFIRMED**, same FIND-TASK-003a-9 | Independently imported the existing generator read-only and compared `render_errors()` with the committed page. `catalog_matches: False`; exactly the SnapshotIncomplete row is missing. |
| FU-R2-01: follow-up resolves behavior A9/A10 PASS versus docs failures | **CONFIRMED**, same FIND-TASK-003a-9 | Source proves the current exception and its public error. Behavior report's broad documentation PASS does not cover it; runtime PASS observations remain credible. Final reconciled A9 and dependent functional-documentation closure cannot pass yet. |
| Literal historical date versus actual production-query DST fixture | **REJECTED as a defect** | The test first proves the session's day lasts 23 hours, calls the real `record_group_activity` statement for all bindings, then proves daily/hourly equality and a 23-hour delta from the one-hour binding. This directly covers the defect without a production clock API or copied corrected expression. Root's year/leap boundary probes support its premise. This is a reversible proof-detail substitution, not a product/spec change. |
| Migration embedding requires recipients to touch lib.rs / new build mechanism | **REJECTED for this candidate** | This commit changes SQL crate Rust source as well as adding the migration, causing ordinary recompilation. Root's migrations/P2/P1 passed without source touches. Migration-only development behavior does not establish an acceptance defect here. |
| Security-posture paragraph mentions only Drift internal reads | **REJECTED as a separate blocker** | The active runtime-identity architecture explicitly includes group snapshots at wyrd-design.md:1460–1468. Security review and independent current-source inspection establish the narrow existing read authority. No forbidden widening or consequential contradictory policy is proved by the descriptive omission alone. Do not make this note a new task, mechanism or acceptance gate. |

The discovery union contains no other proposed material finding. Agreement was not used instead of source validation.

## Prior-finding closure against current source

| Stable ID | Current evidence | Decision |
|---|---|---|
| FIND-TASK-003a-1 | `groups.rs:72–77` maps every WyrdError to retryable Read. `scheduler.rs:490–491` retains query failures; owner activity/authority rejection is a separate path. No retired generic-403 Denied branch remains. | Closed; no R2 work required. |
| FIND-TASK-003a-2 | The sole activity writer at `verifier_executions.rs:201–207` converts the accepted interval to epoch seconds then adds seconds-only interval to PostgreSQL statement time. Parser accepts only bounded positive whole s/m/h/d values. ScribeSink continues to call this owner after ACK; claims consume its due_at unchanged. | Closed by R2 and credible P2. No global timezone policy or new duration owner. |
| FIND-TASK-003a-3 | Current Rust support journey invokes actual Python/TypeScript SDK turns against its server/owner/group; no foreign runtime is emulated inside Rust. Both implementation reviewers retain direct source evidence of shared conversation. | Remains closed. Historical journey evidence is not claimed as fresh. |
| FIND-TASK-003a-4 | Current pg_verification_groups test exercises actual ACK scheduling, capacity-before-read, due batches, transactional handoff and separate cron progress; root selected/passed the owning pending test. | Remains closed. |
| FIND-TASK-003a-5 | Cumulative Rust/Python/TS pressure journey changes use the selected binding source and group. No new R2 pressure behavior enters this range. | Remains closed; no fresh full SDK run claimed. |
| FIND-TASK-003a-6 | Slot visitor associated Value and expecting/visit_str/visit_map all have substantive docs; all fallible hooks have Errors sections accurately describing formatter/tagged-map failures and verbatim string acceptance. Actual bodies unchanged. | Closed by R2. |
| FIND-TASK-003a-7 | New 20261019 migration adds all eight evidence-field NULL requirements in authority-free branch. The four production INSERT owners remain submitted/scheduled/pinned-group/refused-group; only the last omits authority. Its full body omits evidence correctly. Status decoding and ordinary lease/claim writers remain compatible. | Closed at the invariant owner; P2 rejects six mutation categories. No broader authority-present hardening needed. |
| FIND-TASK-003a-8 | Revision 19 explicitly removes owner Bifrost-grant dependency for internal snapshots. Poller reuses existing table-scoped SystemReadAuthority and serving attribution; runnable work still stores owner accepted authority. Replacement server scenario removes roles and proves admission. | Superseded, not an outstanding regression and not represented as a successful run of the removed journey. |
| HUMAN-CLEANUP 1–3 | Cumulative removal/rename of unused due projections, retired grouping descriptions and fixtures remains present; neither R2 nor revisions19/20 restore retired machinery. | Remains closed; human requests are not reassigned FIND IDs. |

## Final retained ledger

### FIND-TASK-003a-9 — Published grouped behavior and generated error reference omit the approved incomplete-snapshot outcome

- **Validation:** REVISED consolidated boundary; underlying runtime/document mismatch and missing generated row independently CONFIRMED.
- **Discovery source IDs:** INV-R2-01, MREV-R2-01, STANDARDS-1, FU-R2-01.
- **Classification:** MISSING / public-contract documentation mismatch.
- **Violated obligation:** original TASK-003a A9 at line483 and scenario8 current-surface closure; approved REQ-004 revision20 at spec.md:422–435; spec functional/documentation closure at 1036–1046; repository source-derived generated-public-reference requirement. This is not a cosmetic wording preference.
- **Locations:** `docs/src/content/docs/understand/how-verification-works.md:176–180`; `architecture/wyrd-design.md:1324–1345`; missing row in `docs/src/content/docs/api/errors.md` WYRD_VERIFICATION section around502–520.

#### Producer-to-consumer proof and reachability

1. GroupPoller::poll (`groups.rs:117–165`) reads exact owner/source/group-filtered verification rows using existing narrow tenant internal read authority. Sources::decode produces Visible. `Visible::missing_inputs` (`439–448`) enumerates selected result execution IDs absent from the input map.
2. `poll:156–159` returns Incomplete before Visible::pin or enqueue. This is an actual production caller; it is not a dormant/test-only helper. Normal present-input/missing-result rows still pin aligned null results.
3. Scheduler::hand_off (`scheduler.rs:427–514`) handles an incomplete read while `due.overdue` by constructing VerificationSnapshotIncomplete with IDs in message/details; earlier incomplete reads retain pending. DueGroup's flag comes from PostgreSQL `statement_timestamp() - due_at > INTERVAL '5 minutes'` in DUE_GROUP_SQL.
4. The same method calls the sole `VerifierExecutionQueue::refuse_group` production owner (`verifier_executions.rs:1834–1854`), using VerificationError code/message. Its INSERT_GROUP_ERROR_SQL omits authority and all evidence fields. `hand_off_groups:363–405` consumes pending and commits this error atomically. Owner rejection is the sibling caller outcome of this same writer, not another lifecycle. Runnable submitted/scheduled/group writers remain separate existing accepted-authority paths.
5. The ordinary execution status projection reads the terminal code/message; binding status supplies last_execution_id. The public verification service still performs normal authorization. A terminal error does not claim/run/evaluate or dispatch an Operator; fresh matching source activity can create a new pending row through the ordinary activity owner.
6. Current guide says the due execution judges every visible input/result and describes terminal consumption only for owner rejection; it never describes this five-minute exception. Grouped architecture's unqualified missing-input/null and query-deferral paragraphs similarly omit the exception. A caller following them cannot account for pending then terminal error requiring fresh activity.
7. The producer's derive metadata in `wyrd-spec/src/error.rs:1930–1949` defines the stable code/title/remediation. The existing `docs/scripts/generate_api_docs.py::error_catalog` reads it; `render_errors` renders the consumed page; `main` rewrites existing API pages. `mise.toml:1238–1245,1294–1301` already owns generation and tracked-drift enforcement. This validator read full generator bodies and these callers. Its read-only render independently returns the single missing line:

```text
| `WYRD_VERIFICATION_409_SNAPSHOT_INCOMPLETE` | 409 | Verification snapshot incomplete | Check Bifrost publication for the named source executions, then submit a new turn to the group. |
```

The generated TS error union already includes the code; changing it or runtime contracts again is unnecessary.

#### Minimum decision-complete correction

Correct this once at the existing consumed documentation/projection owners:

- Amend the current grouped paragraphs in how-verification-works and wyrd-design to describe only the approved exception: a visible result without its execution's input pins nothing and retries; if still incomplete more than five minutes past the current due time on PostgreSQL's clock, Wyrd records one terminal `errored` execution with `WYRD_VERIFICATION_409_SNAPSHOT_INCOMPLETE` naming missing execution IDs and consumes pending atomically. No evaluation, snapshot, verdict/result or Operator is produced; new matching activity schedules normally again.
- Qualify the null statement; preserve normal input-without-result nulls and absent selected values. Preserve internal snapshot authority versus owner execution authority, ordinary query-failure retries, immutable snapshots and all accepted crash/visibility/backlog/overlap/postponement limits.
- Describe stored readback truthfully: VerificationError has code/message only, and the IDs are in the message. Do not promise a new persisted details field or alter the wire/storage contract to match incidental prose.
- Run the existing `mise run docs:generate` owner and commit the changed consumed generated output, including the new API error row. Do not manually maintain a second catalog, modify the generator or invent another documentation mechanism. Other existing generated outputs need change only if this owner actually changes them.

No runtime, SQL, migration, scheduling, authorization, SDK/native binding, configuration, recovery or test-harness change is needed. The stale security-posture-only note is not a separate required correction. No material product or architecture decision is reopened, so SPEC_REVISION_REQUIRED is not indicated.

#### Ponytail ladder applied

1. **Delete?** Deleting this requirement or the error would violate approved behavior/current-doc closure. Delete no working runtime code.
2. **Reuse existing repository behavior?** Yes: edit existing explanatory paragraphs and use the existing source-derived docs generator. This is the correction boundary.
3. **Conventional standard?** Existing generated contract documentation and explicit description of the shipped failure outcome are sufficient. No extra status system or publication protocol.
4. **Standard library/native platform?** No runtime work is necessary; not applicable beyond existing tooling.
5. **Installed dependency?** Existing docs tooling already suffices; no dependency needed.
6. **Minimum correction?** Two existing prose locations plus actually changed generated documentation. No consumer guard is retained or proposed.

#### Focused closure proof

1. Read the changed paragraphs against revision20 and the cited producer/scheduler/storage/status source, explicitly distinguishing missing whole input from missing outcome/selected value.
2. Re-run the existing read-only `render_errors()` comparison: committed page must match exactly and contain the named code/status/title/remediation.
3. Use existing `mise run docs:generate`, then `mise run docs:check` under the normal generated-output workflow; AGENTS requires the docs gate for docs-site changes. No new runtime test, full journey suite, broad repository gate or benchmark is warranted for this docs-only correction.

## Verification assessment and limits

Root verification records the candidate's sequential managed-PG P2 1/1 and P1 2/2 passing, migration tests, fmt/lints/tenant-isolation/diff checks passing, and focused DST year/leap probes. These are credible focused closure evidence for R2. They do not establish documentation parity. The read-only render command exiting zero only means the diagnostic ran; its False result is a demonstrated mismatch, not a passing docs gate.

No fresh full Rust/Python/TypeScript/MCP journeys, docs build/a11y aggregate or benchmarks were run by this validator; no performance PASS is implied. Accepted post-ACK scheduling-signal loss and continuous postponement remain accepted limits, not additional findings. Candidate source stayed immutable; no runtime defect survived validation of the discovery union.
