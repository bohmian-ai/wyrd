# Persistent data review — FAIL

Subject: base `f4e79ea00523af5cd9212fdacde64f16c97ad554` to candidate `365bfc94bfef1a153f213462f7ef104afa823741`, first cumulative review of TASK-001. HEAD remained the candidate when checked. No prior acceptance carried forward. Independent scope: persistent schema, RLS/capabilities, atomicity, indexed bounded inspection, stable cuts, and execution-event projection integrity. No other reviewer reports were read.

## Authority and source coverage

| Boundary | Authority | Sources inspected and conclusion |
|---|---|---|
| Tenant SQL | Root AGENTS.md; SQL AGENTS.md; security posture §Postgres tenant binding | Five new migrations; queue SQL and method signatures; events.rs; inspection.rs. New tables force RLS, carry tenant isolation/operator policies and composite execution foreign keys. Tenant operations use TenantConn, pending-tenant enumeration uses OperatorPool. No new RLS bypass or transaction control in tenant query methods found. |
| Durable execution/publication writes | Approved spec revision 16 REQ-012–015, INV-002/015; TASK-001 outcomes and line 30 constraints | verifier_executions.rs begin/forget/settle/accept/cancel SQL and methods; runner.rs ExecutionCheckpoints and restore; hosted/publications.rs acceptance/File completion; hosted/blocks.rs concurrency. Caller-owned transactions commit before dispatch/receipt; event triggers share transition transactions. File IO is outside the manifest transaction. |
| Inspection | REQ-049; public-contracts.md pagination; output-publication.md stable-cut requirement; TASK-001 scenario 9/index constraint | inspection.rs SQL in SQL crate, service/inspection.rs authorization/limits/cursors/list/select; all new migration indexes. Response count is bounded, but the sequence access path and stable cut have the findings below. |
| Canonical history | REQ-048, INV-009/015; Bifrost design execution_events and Variant ownership | Events migration/triggers; events.rs batch assignment/ack/defer/position; server history.rs projector; results.rs event row builder; scribe_outbox.rs frames/chunks/identity/ACK; canonical table registration and execution_events.rs; staged.rs nullable Int; Gate reserved-write allowlist; existing Scribe committed duplicate and durable-fence consumers. Canonical registration and sensitive Variant projection use existing owners. Cross-process frame identity has DATA-003 below. |
| Proof | TASK-001 recorded verification; pg_verifier_executions tests; verification_runtime::events_replay_without_hiding_live_progress | Inspected test source, including invocation checkpoint reclaim, first-cut page test and role-separated history replay test. Recorded green runs were not independently rerun. The cut test is entirely one connection/transaction; history replay preserves the same frame configuration. Neither exercises the identified interleavings/configuration change. |

CodeGraph was used first. Also read the workflow, architecture map, applicable security/Verifier/Bifrost sections and maintainer guide. This review makes no finding about neighboring Agent tools, SDK parity or unrelated outbox writes.

## Acceptance assessment

| Obligation | Implementation evidence | Verification evidence | Result |
|---|---|---|---|
| Tenant separation and caller-owned atomic transactions | Forced RLS/composite parent FKs; TenantConn queries; separate OperatorPool enumeration; runner/hosted callers commit | Existing focused checkpoint and public tenant journey source plus recorded results | PASS within scoped static review |
| Transition and projection intent commit together | AFTER triggers insert events in the write transaction; manifest accept fenced on execution row | Checkpoint/reclaim and history journey source | PASS |
| Immutable publication identity and JSON null | Unique NULLS NOT DISTINCT producer/name key; call/arguments identity; SQL JSONB null represented as non-SQL-null value; storage completes before accept | pg_durable_verifier_work recorded proofs, acceptance source | PASS for this boundary |
| Stable pages while concurrent work commits | Identity sequence and max cut; compatible writer share locks | Single-transaction page proof does not cover inverted commits | FAIL — DATA-001 |
| Indexed bounded checkpoint/page queries | LIMIT bounds returned pages; existing identity indexes omit sequence | No corresponding sequence index or query-plan proof found | FAIL — DATA-002 |
| Stable event/batch replay across worker/process recovery | SQL batch membership persists, but physical frame membership is recomputed from runtime frame budget | Same-configuration replay proof | FAIL — DATA-003 |
| Canonical table and payload semantics | ExecutionEventsTable registration; sensitive `value` Variant; existing result/output tables retained; reserved Gate write handling | Table contract, nullable Int and role-separated history test source | PASS apart from replay finding |

## Material findings

### DATA-001 — INCORRECT: identity allocation does not establish a stable committed listing cut

**Obligation:** TASK-001 scenario 9 requires stable pages while more entries commit, without gaps or duplicates; public-contracts.md says the cursor fixes the cut; output-publication.md requires stable cuts that do not silently mix changing data.

**Location:** `crates/wyrd/wyrd-sql/src/queries/verifier_executions/inspection.rs:21–55` (both MAX/cursor queries), with producer SQL `verifier_executions.rs:650–656` and `:690–711`. The new invocation/publication `sequence` fields are PostgreSQL identities, allocated before commit. Both insert paths hold only `FOR SHARE` on the execution; these locks are compatible among writers.

**Reachable trace:** one running Verifier has independent same-stage tasks. `VerificationEngine::drive` prepares them concurrently; hosted `STEP_CONCURRENCY` is four. `runner.rs:1135–1158` opens a separate TenantConn for every checkpoint write, with no serialization mutex. Writer A inserts sequence 1 and pauses before COMMIT. Writer B inserts sequence 2 and commits; a later writer commits sequence 3. Public listing with limit 1 reads MAX=3 and returns sequence 2, with continuation after=2/through=3. A now commits. The continuation returns sequence 3, skipping sequence 1 despite its being below the declared cut. Alternatively, if an earlier committed row was first returned, sequence 1 can appear on a continuation despite not being committed when the first page fixed the cut. The same cut therefore does not identify a stable committed membership. Public service reads acquire no coordinating lock. JSON publication acceptance similarly opens its own connection and uses compatible share locks; it has the same cause. Workspace path freezing does not serialize JSON acceptance or invocation starts.

**Consequence/materiality:** concurrent execution is ordinary supported behavior. A reader can finish a supposedly stable listing with a missing invocation/publication, or receive membership added after the first page; progress/content metadata is unreliable during exactly the running-work scenario required by the task. A fresh complete listing may find the row; this is not a claim of physical data loss.

**Smallest correction boundary:** fix cut membership once at the execution SQL owner, coordinating sequence assignment/commit and first-cut acquisition so a lower sequence cannot become visible after the cut is exposed. Reuse the existing execution-row coordination rather than adding client dedup/retries or a second history store. Merely adding an ORDER BY/index, or using repeatable-read in each separate HTTP request, cannot repair the membership guarantee. Keep all invocation/publication writer siblings on that invariant.

**Focused proof:** use separate TenantConn transactions to hold the lower-sequence writer open while higher writes and the first page complete, then commit it between pages. Prove both lists have a consistent cut, no missing member below it, and fresh listings include later committed work. Existing `invocation_pages_keep_their_first_cut` at `pg_verifier_executions.rs:4027` performs writes and reads on one transaction and cannot falsify this path.

### DATA-002 — VIOLATION: sequence pagination has no index matching its range and order

**Obligation:** TASK-001 line 30 explicitly requires new backlog-sensitive checkpoint/page queries to have tenant-scoped indexes and bounded results, and its approach requires proving indexed access.

**Location:** the two cut/page query pairs in `inspection.rs:21–55`; migrations `20261104000000_verifier_execution_invocations.sql:31–34`, `20261104000100_verifier_execution_publications.sql:34–37`, and `20261105000000_verifier_execution_inspection.sql:20–21`.

**Evidence:** the only invocation index orders `(data_tenant_id, execution_id, task_id, item_index, iteration_index, attempt)`; the only publication index adds `name` to that identity. Neither contains `sequence`. The inspection migration adds the identity sequence but no index. The event table's separate `(tenant, execution, sequence)` index cannot serve these tables. For a fixed execution, PostgreSQL must inspect its rows to compute MAX and inspect/filter/sort candidate rows for sequence order; a small LIMIT bounds the response but does not give the required range access path. This is an actual missing access path, not a report that no EXPLAIN was recorded or a measured latency claim.

**Consequence/materiality:** every page redoes work over retained execution history rather than advancing by its indexed sequence. This is the precise backlog-sensitive query behavior the task prohibits; no synthetic-load threshold is needed to establish the explicit constraint violation.

**Correction/proof:** add the smallest tenant/execution/sequence index on each paged table, in a new migration, preserving the identity uniqueness indexes and claim geometry. Confirm cut and continuation plans use that range/order path against representative retained rows; run the existing page proof. No new query service or paging abstraction is needed.

### DATA-003 — INCORRECT: persisted event batch IDs are repartitioned into different durable Scribe identities after a frame-budget change

**Obligation:** REQ-048 and TASK-001 history outcomes require stable event/sequence/batch replay identities; Bifrost design execution_events promises an ambiguous outcome resubmits the same batch, with Scribe dedup absorbing it. Recovery must survive process restart/another worker.

**Changed location:** `scribe_outbox.rs:1237–1249` gives ExecutionEvents a persisted frame key, and `:1580–1587` uses `(key, 0, chunk)` for that frame. Inherited machinery at `:1306–1325`, `:1370–1410` and `:1510–1536` partitions rows using the current `frame_budget` and hashes `(tenant, table, key, frame, chunk, start)` without the immutable row range. `frame_budget` is current `scribe.ingest_request_bytes / 4` (`:188–190`, `:1066`). Boot supplies the configured value (`boot/mod.rs:893–905`); config.rs:331–332 exposes it as a deployment capacity setting. No setting/range is persisted with the SQL batch.

**Reachable trace:** a persisted history batch contains enough same-sized events to produce physical chunks. At one supported request ceiling, chunk 0 contains events [A,B]; Scribe durably accepts it but the response/SQL batch acknowledgement is lost. Restart the projector with a larger supported request ceiling, still below which the individual rows fit. The SQL batch ID and rows are restored unchanged, but chunk 0 now contains [A,B,C,D]. Both frames hash the same `(batch,0,0,start=0)` identity. The logical payload is different. Scribe does not blindly hide the contradiction: `shards.rs:1157–1170` requires matching logical digest/slice count for fast duplicate acceptance; `vala-sql/src/queries/scribe_batch_commits.rs:553–578` rejects a same-version durable identity with different rows. Thus replay cannot obtain the promised ACK, and the head assigned SQL batch continues blocking subsequent history for that tenant. A smaller budget has the inverse collision. Depending on subsequently accepted chunks, extra events can also be retained under different IDs, so the correct proof is exact event identity/count, not just eventual nonempty history.

**Introduced vs inherited:** existing transient outbox slices kept a fixed sink budget during their in-process retries and did not claim to recover the same slice key after a fresh process configuration. This candidate newly persists the logical key and rebuilds that batch through current sink configuration, so the cross-process mismatch is part of the new replay path. This finding does not request changing ordinary audit/capture/output delivery semantics.

**Consequence/materiality:** changing a supported deployment request capacity during restart/rollout can make durable history permanently fail projection; live state remains, but the explicitly durable queryable history guarantee fails. It is not an arbitrary malformed caller input.

**Smallest correction boundary:** keep ExecutionEvents physical frame identity and membership immutable across processes, within the existing Scribe outbox. A narrow event-specific use of stable per-event identities avoids configuration-dependent grouping; alternatively preserve the exact partition membership needed for replay. Reusing the batch key with recomputed ordinal chunks is insufficient, and changing IDs solely when the budget changes would duplicate previously accepted events. Preserve the existing stronger Scribe identity checks and ordinary outbox semantics. No parallel ingestion service is justified.

**Focused proof:** accept a physical frame from a multi-frame event batch, lose its ACK, replay through a fresh sink with a different valid request ceiling, and assert all committed event IDs appear exactly once and SQL projection reaches acknowledged. Existing role-separated replay proof uses unchanged configuration and a small batch; it does not exercise this boundary.

## Proof limits

Read-only source/Git review only: no Cargo jobs, Postgres interleaving reproduction, EXPLAIN or configuration-change replay was run. `git diff --check` passed. Recorded checks are evidence supplied with the task, not independent execution. No source was edited; only this report was written. The three source-backed proposals require the independent validation stage before becoming remediation findings.

Overall result: **FAIL** for DATA-001, DATA-002 and DATA-003.
