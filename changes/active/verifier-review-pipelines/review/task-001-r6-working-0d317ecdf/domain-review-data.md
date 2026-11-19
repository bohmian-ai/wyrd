# R6 data-domain review

Overall result: **PASS**. No material findings proposed in this boundary.

## Subject, scope and authority

Reviewed the user-submitted pinned working source on HEAD
`0d317ecdf2d28d2e77b6f394ebe0f443d256d580`, as described in this round's
subject.md. `subject-sha256.txt` passed after source inspection. The exact new
writer delta is `git diff HEAD -- crates/wyrd/wyrd-server/src/scribe_outbox.rs`:
that owner matched HEAD at R5. No reconstructible R5 source commit is assumed.
No source, test, task, authority or manifest was modified; this report is the
only authored output. No Cargo was run by this reviewer.

Scope is USER-RESULT-IDEMPOTENCY-001 and the affected persistent-data,
identity/fingerprint, ACK and consumer boundaries, plus FIND27 and the local
FIND23 documentation residual. Read the review skill and subject first, used
CodeGraph before source navigation, and read root/SQL AGENTS, spec-driven and
implementation-execution rules, maintainer guidance, agent-docs index, TESTING,
original Task001 concurrency/recovery obligations, approved terminal supplement
revision3, specification terminal obligations, R6 task, and prior R5 verdict,
validation and data report. Applicable architecture is Bifrost batch identity
and retained-retry comparison (`bifrost-design.md:230–255`), Scribe logical
identity/WAL and process outbox ownership, definition/fence retention roots
(`:1713–1731`), and security posture's Postgres tenant binding (`:752–789`).

R5 source-backed acceptance carries for terminal SQL producers, tenant claims,
latest-delivery fencing, frozen identity, retained migration, common terminal
operation, source-aware recovery, completion effects and generic replay. The
reason is the prior report/validation's traced producers, consumers and tests,
combined with unchanged runner/inspection/group/SQL-integration owner hashes;
unchanged paths alone are not the evidence. The new Result framing exercises
existing generic fences, so those mechanics and affected result consumers are
checked afresh below. This is not a whole Task001 restart or a stronger stale
worker, mixed-version, global-row-uniqueness or retention contract.

## Contract matrix

| Requirement / invariant | Implementation evidence | Verification evidence | Result |
| --- | --- | --- | --- |
| Independently restaged selected results preserve identity per tenant/table | `scribe_outbox.rs:1298–1315` isolates each Result row with its payload result_id key; `Frame::shares:1234` excludes keyed frames; `submit:1670–1680` substitutes `(key,0)` for slice/place. `batch_id:1600–1624` hashes tenant, table FQN, key and positions. No new store, identity or generic API is added. | `result_redelivery_keeps_batch_identity:3194` writes the selected payload in three differently composed/ordered slices, checks one-row frames, exact IPC/ID reuse and six distinct identities for three results/two tables. Five writes cost ten frames. Root independent execution recorded 1/1 PASS. | PASS |
| Stable identities do not depend on frame ceiling/chunk splitting | `ResultPayloadBuilder::build_as` constructs exactly one output and one summary row; `chunks:1363–1394` takes keyed frames one row per chunk; `Chunk::split` leaves a single oversized row alone. Thus chunk/start remain zero and the two table IDs differ. Existing ceiling refusal remains authoritative. | Identity unit test asserts one row per frame. Existing keyed ExecutionState path and splitting tests retain their owner; no new oversized-row bypass exists. | PASS |
| Reconstructed payload matches strict Arrow content identity, including frozen correlation | `runner.rs:1574–1610` uses selected result_id, started_at/ended_at, frozen run/verifier and subject/SYSTEM attribution. `results.rs:318–338,351–384` constructs output and summary from those values. `preprocess.rs:455–541` digests Arrow length/offset, validity, data/child buffers and nonvolatile fields, independently of schema identity; it excludes managed request/time metadata, not durable correlation. | `result_redelivery_is_idempotent_after_restart:2795–2902` reconstructs the selected drifting outcome in fresh runtimes and compares full sent batches per table, then checks retained physical rows. Exact bytes are compared rather than assuming decoded-value equivalence. Root focused integration PASS. | PASS |
| Existing durable fence ACKs matches, suppresses duplicate rows and refuses changed accepted content | `scribe_batch_commits.rs:588–618` classifies matching logical identity as AlreadyCommitted and different same-version content as Conflicted. TenantConn owns lookup scope; table and batch are lookup keys. `shards.rs:3911–4000` marks spent IDs to prevent another insertion; `:4052–4064` returns BatchConflict for changed content. `resolve_replay:230–255` uses immutable fence WAL coordinates to restore the canonical attempt and suppress other attempts. | Restart test checks `(1,[failed])` retained rows after settlement-failure reclaim; output-only partial acceptance becomes `(1,[failed])` after restart; conflicting build_as under that accepted result_id drops the ACK and leaves retained rows unchanged. Lost sealed-ACK test separately covers transport retry. Both root focused tests PASS. | PASS |
| Partial acceptance/rejection never falsely ACKs a whole Result | `scribe_outbox.rs:1897–1927` submits detail phase before summaries, awaits both phases and group activity, and sends Result ACK only when its item is absent from lost. `tally:1750–1785` records terminal rejection against owners and preserves retryable refusal. `runner.rs:1527–1562` maps dropped/timed-out ACK to Defer; it settles success only on receipt. Existing claim confirmation and terminal selection remain unchanged. | P5 `cancelled_selected_delivery_waits_for_all_required_acks:2685–2777` holds output, fences stale settlement, refuses summary, then retires owed delivery with zero fresh engine entries and exactly one retained row per result/table. Restart test independently covers output accepted/summary refused. Root both PASS. | PASS |
| Result consumers retain exact selected result and completeness semantics | `service.rs:261–274` requests selected.result_id through InputResolver. `inputs.rs:409–459` joins summary/output by result_id and inputs by execution_id and returns None until the required records exist. `groups.rs:501–525,548–580,614–668` uses the same result/output completeness test and logical result map, not batch grouping or physical positions. Direct staging `service.rs:806–850` preserves caller attribution and selected identity through the same Result owner. | Prior R5 validation traced reader/group/completion effects; new tests verify exact selected identity and no recomputation. Keyed frames remove duplicate physical result rows without changing row shape or reader joins. Existing direct fast return and completion effects carry. | PASS |
| Existing batching and authority for sibling write kinds remain intact | Only Result acquires a new key in frames; Audit/Capture/Inputs/Object/Spans keep Frame::absorb. ExecutionState's earlier isolated path is untouched. Outbox limits, route, retries, generic fingerprint and fence machinery are unchanged. | Mixed slice tests now assert each Result's isolated output/summary while retaining dense batching of other kinds, per-row attribution, tenant and no-caller transport principal. Implementation records 28/28 related units; root independently runs the new identity test. | PASS |
| FIND27 bounded plan assertion recognizes retained delivery index | `verifier_executions.rs:4372–4419` documents/allows verifier_executions_delivery_due alongside verifier_runs_due/verifier_runs_leased, retaining Index Scan/Index Only Scan constraints. The required owing-delivery read and migration are retained. | Root exact library test recorded 1/1 PASS, resolving R5's source-proven rejected-index assertion. Migration apply/idempotence also PASS. | PASS / FIND27 closed |
| FIND23 local changed-item rustdoc residual | `settle_delivery:3397–3411` now states caller-owned transaction, matching claimed_until fence, StaleLease no-op and database-error condition. `state.rs:410–428` documents saturating no-buffer Counted::write and infallible flush, including Errors sections. Changed Result/frame/chunk docs describe keyed identity. | Static inspection closes the identified local gate; implementation-recorded format/lints are supporting evidence, not a substitute for reading docs. | PASS within assigned residual |
| Existing source/capacity and tenant semantics remain accepted | Existing TenantConn, forced-RLS and delivery claim/fence owners are not replaced by the Result key. `exhausted_reconciliation_survives_source_and_capacity_outage:2284–2376` now holds the real server outbox's free reserved share after the separate source outage, checks actual reserve refusal, unchanged attempt/status and no terminal mutation, then drops it and recovers. | Root exact integration PASS. Data semantics carry R5 acceptance; fresh system review owns complete capacity/hosted workflow coverage. | PASS within data boundary |

## Recovery and retention limits

The application restart proof uses fresh verification runtimes against the
same bound server/Scribe and PostgreSQL. It directly proves reconstructed
application delivery across claim recovery, not a new full Scribe-process or
cluster crash experiment. Durable WAL/fence replay is the existing owner and
carries source-backed R5/R3 acceptance; its current source still suppresses
noncanonical attempts, including already-committed matches and conflicts.

The generic fence binds source schema, ordered logical slices/count and strict
Arrow representation. Semantic JSON equivalence alone does not authorize a
retry ACK. The new frozen one-row producer path and byte-comparison proof are
sufficient for this bounded addition; no permissive digest or per-reader dedup
has been introduced. Accepted-under-another-version resolution remains the
existing mechanism and is not a new mixed-version producer guarantee.

This review accepts idempotency within the existing retained batch-fence
contract. It does not certify indefinite result-row retention, retrospective
cleanup, unique result_id across arbitrary ingestion producers, external effect
exactly-once behavior, or deduplication of pre-R6 unkeyed batches. Existing
retention/definition roots and oversized-row refusal remain unchanged. Those
limits do not contradict the approved bounded Result writer change.

## Findings and disposition

Proposed material finding ledger: **empty**. The smallest complete correction
already present is the local Result key selection and isolated-frame reuse.
The existing Scribe fence, content comparison, two-phase ACK, frozen producer
and reader identities provide the required behavior; no additional persistent
registry or abstraction is earned.

FIND27 closes in this data-owner scope. FIND23's specified SQL and Counted
residuals close statically. No previously accepted persistent-data behavior is
reopened. The root validator must reconcile full specialist coverage and
original hosted proof before issuing the overall task verdict.

Skipped reviewer Cargo execution, broad journeys, benchmark/load work and
unrelated Task001 discovery. Risk: acceptance retains existing fence/retention
and producer-version limits; root execution evidence is attributed separately
in verification.md.
