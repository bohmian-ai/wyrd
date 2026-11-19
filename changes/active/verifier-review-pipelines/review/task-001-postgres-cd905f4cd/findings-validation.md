# PostgreSQL boundary audit — independent validation

**Verdict: FIX_REQUIRED.** Existing FIND-TASK-001-16 and FIND-TASK-001-17 remain source-proven. USER-PG-001 separately requires removal of the rejected per-execution SQL authority. Existing FIND-TASK-001-19 remains open and belongs to the affected queue/result reconciliation boundary. No new FIND identifier is assigned. The prior specification-decision stop based on finding20 is superseded by current user authority; implementation difficulty in extending the already approved Bifrost reader is not a new approval gate.

## Handoff consolidation — 2026-11-06

The user directed one remediation packet for all remaining findings. The
[consolidated R3 task](TASK-001-R3-complete-execution-state-remediation.md) now
includes USER-PG-001 and16/17/19 plus the original independently validated21–23,
with24/25 addressed through removal. It replaces the narrower PG-only handoff.
References below to keeping21–23 separately open describe this audit's original
scope, not separate active packets; they remain open but belong to the same R3
implementation. Source and independent finding dispositions are unchanged;
consolidation grants no closure. Finding18 remains withdrawn and20 superseded.

## Subject and method

Repository: `/home/thorrester/Documents/GitHub/wyrd-worktrees/code-review-workflow`. Base `0e8a31b14d336fadf62e1d8525a7fa030ab6c4eb`; candidate and inspected HEAD `cd905f4cd929785bda25696326d58b4a4ae88dee`. This is independent validation of the focused audit, not implementation or a fresh full TASK-001 review. Only this file was written. No Cargo, migration or runtime command was run.

Inputs read: this round's subject, all three complete discovery reports and claim comparison; previous `task-001-bifrost-cd905f4cd/verdict.md`, finding ledger and verification; the applicable spec18/supplement2/TASK-001 sections; architecture execution/recovery, canonical records, ordinary query source semantics and tenant binding; SQL AGENTS, maintainer/Rust/testing guidance. CodeGraph exploration preceded source searches. I independently traced the actual head claim/read/write, canonical state shape, runner outcomes, queue statements and completion/renewal stored functions, status/inspection readers, direct fallback, removed SQL event migration, frame/proto/error dependencies, batch-fence diff, shard refusal/replay and published-source/read-cut owners. Discovery agreement was not treated as source proof.

Mutable authorities match the discovery hashes:

| Input | SHA-256 |
| --- | --- |
| spec.md | `830a24787671f190ba50ccecf16d72b4d76d24df561fbb5313fa46d2cc0d7d74` |
| execution-state-and-durability.md | `c03ebbc755caf9ef6fba23134ddc6b3ff1d08eeb2da0a9d7af2a0593ac3a6d5b` |
| tasks/TASK-001-bifrost-execution-state.md | `87fde0bd7a8e8adf49467bd57cebe9a8ecffddfab36085fe96a37538a6d07649` |
| architecture/bifrost-design.md | `f876727f4f0c0a59bd53729cbaaeb87908a5fe7cba180bcd1f281fb361d3f094` |
| architecture/wyrd-design.md | `1887a2fe659a2a652b60998c7fa05c845e26f4976c0ae051eee138a79a896fc8` |
| architecture/wyrd-security-posture.md | `01d3475bdff78814ca33785bd4ecfd94db907e51ba1356e7095274ad405244ea` |
| architecture/operations/reliability-and-recovery.md | `6f138d2130bcff7da437e04b88ef0361091866a6da0ea2e908079d7baef01aca` |

The current instruction rejects `scribe_state_heads` itself, accepts the specified rare delayed-old-worker/replacement interval, and withdraws a direct final input/output/result ACK wait. These instructions outrank contrary older prose. They do not waive required-source failure, normal queue controls, canonical terminal custody, tenant/feature permissions, exact File custody or retained-volume durability.

## Proposal dispositions

| Proposal | Disposition | Stable identity / correction |
| --- | --- | --- |
| PG-AUDIT-001 / ES-01: queue SQL remains sole full terminal/error custody and public selection authority | **REVISED** | FIND-TASK-001-17 for missing canonical terminal producers/full errors. Public selected-result reader work overlaps FIND-TASK-001-16; do not create another ID. Retain legal queue status, retry diagnostics and narrow result references for coordination, idempotent completion and group comparison. Their existence alone is not a violation. |
| PG-AUDIT-002 / ES-02: live Cards or queue membership gate retained inspection | **CONFIRMED**, with consolidated scope | FIND-TASK-001-16. Source-deleted result reconstruction and direct/retained-queued pages/content share missing recorded identity/common-reader authority. Persist missing facts once and adapt existing readers; no per-route archive fallback. |
| All reports: remove new SQL head authority | **CONFIRMED as human-directed work** | USER-PG-001, never a retroactive FIND. Remove claim/head/advance, schema and all dependent authority plumbing; background relocation fails this instruction. |
| Data report: ordinary latest-visible query cannot replace SQL head | **CONFIRMED** | Required safety condition of USER-PG-001 removal, not a new finding or decision request. Extend existing table-scoped Bifrost batch/source/read/replay owners for complete accepted state, authoritative absence or explicit unavailability. |
| Inventory note: redundant tenant predicates in the Vala head query violate the sibling SQL AGENTS | **REJECTED as a separate mandatory finding** | The narrower `wyrd-sql/AGENTS.md` does not automatically govern sibling `vala-sql`. Prior validation already narrowed this claim. No tenant leak is established. Delete these queries with USER-PG-001; prescribe no isolated predicate patch. |

No genuinely new independently sourced cause remains, so IDs26+ are unused. No validator-discovered blocker needs separate validation. No follow-up discovery is required: source resolves the proposals and correction boundary without an unresolved conflict.

## Validated PostgreSQL inventory

The detailed per-operation inventories in [postgres-usage-audit.md](postgres-usage-audit.md), [execution-storage-audit.md](execution-storage-audit.md) and [bifrost-data-audit.md](bifrost-data-audit.md) are incorporated with the qualifications below. They identify caller, phase, read/write, table/fields, provenance, authority and consequence; the following reconciliation records the field-use boundary, rather than allowing a whole module by label. `Q` denotes `crates/wyrd/wyrd-sql/src/queries/verifier_executions.rs`; server paths below are relative to `crates/wyrd/wyrd-server/src`.

| Operation / concrete fields and owner | Validated boundary |
| --- | --- |
| Target/readiness/cache: `VerificationTargets`, Cards identity/spec/status, SYSTEM id; readiness/baselines; registry notifications | Allowed admission/runtime registry and security context. Historical result reconstruction cannot require live Cards (16). The documented single-statement tenant-binding exception belongs to this existing owner only. |
| Submission: Q INSERT/EXISTING/KEYED, request-key advisory locks, inactive-principal share locks; execution/target/run/group/environment, principals, accepted credential-free authority, trace/span, idempotency scope/digest, limits/timestamps | Allowed initial admission, idempotency and security custody. Inputs and route values are staged through canonical inputs before SQL handoff; do not restore them to SQL. The full refusal error has no canonical sibling producer (17). |
| Schedule: resolve binding, due selection, existing occurrence, insert, cursor advance/CAS; binding/target/trigger/Operator identity, cron/tz, original window, due cursor | Allowed existing scheduling and activation. Preserve occurrence dedup and database clock; no selected result/checkpoint authority follows from these fields. |
| Group: source-activity pending rows, generation/debounce, due/binding, consume/generation guard, snapshot digest and insert | Allowed activation and admission dedup. Ordered selected source bodies remain canonical input data. Refused group SQL-only full error is 17; do not fabricate an admitted engine run or accepted inputs for refusal. |
| Claim: worker_format, status, attempts/max_attempts, token/expiry, next_attempt_at, original deadline, wait/age/remaining durations and routing/security context; ready fitted baselines | Allowed queue controls and preparation. Original deadline uses COALESCE/statement_timestamp. Exhaustion SQL-only terminal error is 17, and exhausting before reconciling durable terminal selection remains 19. |
| Renew/confirm, retry, terminate, release, cancel/principal-cancel | Allowed queue fencing, retry budget/backoff, attempt refund and auth revocation ordering. Full historical error/cancel decisions require canonical custody (17). Keep ordinary HeldLease confirmation; no new acceptance lease/handoff protocol. |
| Complete single/group and stored `complete_verifier_executions`; result_id, status, same-result replay predicate, cleared lease/error, settlement time; group binding lock/previous result_id+group_verdict | Allowed idempotent delivery and independent group comparison bookkeeping. SQL result_id cannot select the retained public result in place of canonical selected identity (16/17). Do not delete every result pointer or change group serialization. Stored renewal/completion functions remain reachable after the new migration and must be included in correction verification. |
| Operator dispatch creation/claim/deliver/fail/retry/release/exhaust; bounded frozen failure_context, refs/verdict/drift/prior group result, dispatch status/error/attempt/lease; provider connection credentials | Allowed independent delivery payload/control lifetime. Dispatch errors are not execution errors; this owner must not become canonical execution recovery custody. Existing provider/auth checks remain. |
| Status/dispatch reads; binding targets/activity and last execution; runnable/due tenant discovery, queue depth | Allowed current queue/delivery projections and control telemetry. Q STATUS→execution_status currently exposes SQL error and result pointer as historical truth, and queue row existence gates inspection (16/17). Binding's latest admission pointer is not execution terminal-result selection. |
| Current auth/SYSTEM, tenant admission, role/permission/restriction lookups, exact workload scope, workload activity | Allowed security/control owners. Retained evidence reads still authorize current exact feature grants against recorded identity, never infer grants from historical accepted authority or a SYSTEM read capability. |
| Shared File upload initialization/idempotency/session reads/backend handle/plans/expected digest+length/type/status/failure/expiry/completion | Allowed shared storage lifecycle. Accepted publication descriptor/call receipt association lives in Bifrost state from acceptance, not reconstructed from session SQL. No upload-session ownership transfer or new File store. |
| New head: tenant/table/execution key, writer/generation, accepted revision/batch; claim/head/advance | USER-PG-001 deviation. Added, execution-specific authoritative absence/current position/ownership. No payload is required for this to be rejected authority. |
| Generic `scribe_batch_commits`: tenant/table/batch, version, digest/slice count, ingest instant, exact node/epoch/shard/segment/LSN/request | Allowed pre-existing immutable batch identity, durability, retry/ambiguous-commit and WAL replay metadata. The diff adds an in-memory state field and advance hook, not new persisted generic columns. Remove that hook, not the generic fence transaction. |
| `bifrost_tables`/catalog definitions/layout/version/UID; read-cut maintenance authority/Iceberg pointer; `file_list` physical artifacts and exact stream/range/ordinal/checksum; table/node publication revisions and trigger | Allowed pre-existing table/source publication metadata. It can support the approved complete-recovery owner extension; it may not gain execution current-writer/head columns or an execution-specific authoritative sidecar. |
| Membership heartbeat/ready/node/address/epoch; publication fence; newest WAL-fence function; published-range suppression | Allowed existing process/physical-source coordination and retained-volume recovery. Node fencing is not a verification execution lease. Live membership is not complete unpublished-data inventory. |
| Old invocation/publication/event SQL tables/triggers and accounting columns | Candidate correctly drops these with old-format custody guard. Removing heads does not restore them or weaken the guard. The head-specific migration explanation is obsolete. |

Introduced/retained status matters only for provenance: retained SQL error custody and live-Card gates still violate the refactor's explicit outcome. Conversely, pre-existing catalog/File/auth/Operator payloads do not become execution-state violations merely because reachable from verification. No unrelated Forge/platform SQL review is claimed.

## Source-validated findings and consequence

### FIND-TASK-001-17 — REVISED / MISSING

Obligation: REQ-054/056, supplement complete-state/terminal requirements and TASK-001 terminal inspection; canonical verification data owns selected result/full sanitized error for every outcome.

Producer/consumer trace: `wyrd-spec/src/verification/state.rs:31–103` retains invocations/publications and `TerminalSelection { result_id, started_at, ended_at, outcome }`, with no non-verdict full-error/control terminal representation. Runner `execute` (`verification/runner.rs:397–446`) returns pre-state target failures and `EngineOutcome::Terminal` directly as transitions. `settle` writes TERMINATE/RETRY; admission/group refusal, CLAIM exhaustion and user/principal cancellation likewise write only queue errors. Q `execution_status:3209–3262` maps `row.error` into the API; service `get_execution:243–290` uses that projection and SQL result_id. The new migration removes old event producers, not this dependency.

An ordinary cancellation, target/planning failure or exhausted run consequently cannot be reconstructed from retained canonical execution data without its coordination payload. This is the explicit terminal retention boundary, independent of the accepted rare worker gap. Existing FIND-16 owns the related retained selected-identity reader dependency; do not duplicate it.

Minimum correction: complete the existing versioned canonical terminal/lifecycle representation and wire each existing terminal producer into its existing reserved shared-writer path. Retain exact sanitized status/error, identity and selected result. Engine errors with a state use that state owner; pre-engine/queue decisions use their actual lifecycle owner, not a stale cancelled runner and not a new finalizer. Existing SQL status/retry diagnostics may serve coordination, but cannot be the sole retained outcome or public recovery authority. Preserve existing failure/retry and queue cancellation behavior; do not add atomic queue→generic acceptance ordering or a second lease protocol. Complete canonical custody and adapt its reader in the same task.

Closure proof: engine terminal error plus queue-origin cancellation/refusal/exhaustion, including no prior checkpoint, remain readable with exact sanitized error from canonical data independent of queue payload. Exercise retry of the existing staging/receipt path and preserve repeated cancellation, principal-revocation controls and failed/inconclusive completed verdict semantics. Do not equate a completed failed verdict with an execution error.

### FIND-TASK-001-16 — CONFIRMED / MISSING

Obligation: REQ-052/054/056, AC-048 and `architecture/wyrd-design.md:1925–1932`: retained identity/reads survive source Card deletion and leftover queue-row removal with current feature authorization.

Producer/consumer trace: canonical state lacks exact Verifier/subject kind/name/version reconstruction facts. State row UID dimensions do not replace those facts. `get_execution` obtains live Cards (`service.rs:264–265`) and returns a queued response without result when either is absent. `direct_execution:319–344` returns None for the same condition. Cards SELECT_BY_UID explicitly excludes deleted rows. Inspection `authorized:282–306` requires Q status before reading canonical state; direct work has no queue row. Without a queue row, `get_execution` only tries `InputResolver::direct:476`, whose SQL filters `i.origin = 'direct'`; retained queued work cannot use it. Invocations, publications and content share the queue gate.

These are supported completed-work/deletion and direct-publication flows, not hypothetical retention hardening. Record immutable identity/required feature scope once in existing canonical input/state/lifecycle data, then use the existing accepted-state/exact-reference reader for status, pages and content. Resolve exact selected result from canonical state, not SQL pointer or result-row recency. Current scoped authorization/auditing, SYSTEM serving constraints, exact File selection and stable page cuts remain mandatory. Queue existence may control mutation/coordination, not retained read existence. No direct queue rows, tombstones, archive or per-reader fallback layer.

Closure proof: direct and queued nonempty executions with accepted publications retain identical exact refs/result/body/pages/content after source Card deletion; retained queued data remains readable without queue parent. Current grant denial/cross-tenant IDs still fail. Missing required source returns retryable unavailability rather than not-found; stable cut and exact producer/File descriptor remain.

### FIND-TASK-001-19 — carried forward, affected PostgreSQL reconciliation

The previous independently validated finding remains unchanged. I checked Q CLAIM_SQL:412–440: expired final attempts are terminalized before a claim reaches `Runner::execute`, although that runner correctly redelivers recovered terminal selection before computation. A durable selected result followed by failed queue completion on the final attempt is stranded. This is adjacent to the public SQL selection/error defect, not closed by removing the head.

Include its existing bounded correction in the one PG task: reconcile canonical terminal selection through existing queue→runner/load/deliver/complete before treating final-attempt bookkeeping as unfinished computation. SQL must not query Bifrost. Preserve exhausted unfinished-work behavior, original attempts/deadline and cancellation; no additional computation or delivery-policy extension. Adapt the existing failed-settlement regression to the last permitted attempt, require identical already-durable result and zero engine/provider reruns, and retain the unfinished exhaustion negative flow.

## USER-PG-001 removal and recovery boundary

This is an explicit directed addition, not review credit. `RunState::claim:341` commits a new SQL claim before execution; `fresh` routes direct work through it; `accepted:428` asks SQL for revision before `InputResolver::state` reads that exact Bifrost row. `StateScribe::write:284–293` attaches writer/revision/not_after to a random stable-per-offer batch; `scribe_batch_commits::record:476–505` advances the head in the fence transaction. SQL revision zero authorizes fresh state. Relocation cannot remove that authority.

The complete affected write set is bounded by these dependencies:

1. `vala-sql` head migration/query/export, ScribeBatchCommit state field/advance/StateRefused resolution; remove the owner entirely. Applied migrations remain immutable/checksum-stable; use an ordered retirement migration for installed schema and remove runtime reliance. Do not rewrite an applied migration to erase history. Preserve accepted canonical records and generic fences during upgrade.
2. Server StateWriter/StateScribe/RunState fresh/claim/accepted/read and direct/runner/inspection/hosted consumers; testing reader fixture. Keep live Held/Pending coalescing, exact ticket coverage, shared waiter driving, short memory locks, stable retry batches and ordinary HeldLease controls.
3. Redux contracts/re-exports, ingress/preprocess propagated state, builtin `STATE_FENCED`/definition marker and execution_events enablement; shard refused-state bookkeeping, transaction retry branches and unfenced-state replay suppression. Remove SQL-specific assumptions consistently while retaining generic accepting-version/digest retry, ambiguous commit handling, sibling-batch progress, memory ownership and canonical WAL-attempt/published-range suppression.
4. Outbox ExecutionState/ScribeBatch/Frame/Chunk and IPC/peer routing; capture_peer parsing and `IngestCaptureState`/request field10 in `wyrd.v1.proto`; Scribe/gate/spec error catalog and exhaustive matches. Remove unused authority fields together; reserve retired protobuf names/numbers as appropriate. Keep actual checkpoint payloads, execution lanes and individual frame ACK semantics. Regenerate owned derived surfaces when their inputs change.
5. SQL head tests, lifecycle tests that UPDATE the head, durable-work/state/frame/peer/gRPC/WAL fixtures including `state: None` additions; obsolete head-specific docs/assertions. Retarget useful recovery/tenant/result/publication/replay proof at existing Bifrost owners. Do not keep a rejected owner just to keep tests unchanged, or delete behavioral coverage with the obsolete assertion.
6. Align active authority/task wording below; retain prior reports as historical evidence. The Wyrd state-in-Bifrost migration's explanatory head reference can be superseded in current documentation/new migration explanation without altering an applied checksum; its old-format custody guard and dropped payload stores remain.

Deletion alone is insufficient. `InputResolver::state` reads an exact supplied revision, while generic batch commits contain no execution UUID/revision and current checkpoint batch IDs are random. `cluster_nodes::list_live:219` filters expired/unready owners; `RegistryTailStreamDiscovery::discover` can return an empty discovered tail for an empty roster. Ordinary successful latest-visible `MAX(sequence)` can therefore hide a later acknowledged unpublished checkpoint. The current SQL head prevents that particular older/fresh fallback, so its removal must replace the proof of completeness at the already approved Bifrost read owner.

Use the tenant's logical verification table and existing durable batch/source streams as the completeness scope: existing batch fences identify acknowledged durable batches and exact WAL attempts; file_list exact stream/range/artifact coverage and Oracle table cuts/publication revisions identify published coverage; existing Scribe live/staged reads and WAL replay supply surviving unpublished records. Extend these owners to reconcile required durable sources with published coverage and reachable retained WAL/live data, then resolve execution identity/revision from canonical records. A required uncovered/unreachable source yields explicit retryable unavailability, not older state or authoritative absence. Successful complete coverage permits compatible accepted state or authoritative absence. Preserve publication-handover consistency while doing so; no execution-key SQL index/sidecar/head cache is permitted.

This is an already approved owner extension, not evidence that an off-the-shelf complete reader currently exists. No new persistent source registry, queue, service, writer or lease system is needed or authorized. Permanent unpublished-volume loss remains outside automatic recovery; retained-volume owner absence must still fail visibly. Fresh unique direct IDs initialize in memory without a claim; retained inspection/queued retry require complete recovery. The accepted rare late-worker interval is the precise concurrency exception; do not broaden it into missing-source acceptance or resurrect actually refused progress during replay.

## One remediation task recommendation

Recommend one self-contained task, `TASK-001-R1-postgres-boundary.md`, for **USER-PG-001 + FIND-16 + FIND-17 + affected existing FIND-19**, using the approved spec/supplement/original task and immutable identities recorded above. The diagnosis, source paths, selected owner reuse, non-goals and proof in this ledger are its substance; it must not ask an implementer to choose between removal and relocation.

The outcome is one canonical Bifrost execution authority for recoverable state/identity/terminal facts and exact selected result, complete table-scoped recovery using existing batch/source/read/replay owners, existing queue coordination preserved, and retained direct/queued inspection independent of live Cards/queue parent. A replacement authoritative SQL table/column/cache under generic naming is prohibited. Ordinary catalog/batch/File/auth/binding/group/Operator operations retain the enumerated roles above. Do not restore old payload/event tables, add a finalizer, or impose a direct final-record ACK wait. Changes needed for removal, existing terminal producers and shared readers belong together; optional cleanup and unrelated old findings do not.

Align these exact active mandates in that same task under already supplied user authority:

| Authority text | Required alignment |
| --- | --- |
| spec REQ-057:973–977, INV-018:1012, AC-049:1140; supplement recovery/stale-worker wording:298–325, DUR-009/010:383–384 and decision4:411–417 | Explicitly reject per-execution SQL ownership/checkpoint-head authority and qualify universal replacement/current-writer ordering with the accepted rare delayed-old-worker gap. Keep normal renewal/cancel/deadline checks and accepted-write/rejected-write durability. No new queue→generic-Scribe acceptance protocol. |
| `architecture/bifrost-design.md:481–486`, `architecture/wyrd-design.md:1871–1879`, operations reliability/recovery:259–266 | Reconcile the same universal takeover/expected-revision mandates; keep exact recovery/absence/unavailability and existing WAL ownership. Existing generic batch/catalog/source metadata stays allowed, distinct from rejected execution heads. |
| TASK-001 Revision1:62–65,113–116, What Changes:235–240, scenario3:321–338, acceptance:447, takeover proof:492–495 | Update implementation/proof obligations to the approved removal and accepted exception; task:256 must not excuse heads as generic fence SQL. Preserve historical evidence:588–589 as historical, with supersession recorded. |
| spec REQ-055:960–963; supplement result flow:127–129, terminal boundary table around:179, DUR-004:378; TASK-001 selected correction5:99–103; wyrd-design “final success” acknowledgement wording | Qualify direct final input/output/result delivery as intentionally asynchronous. Preserve consequential checkpoint/publication boundaries and stable selected outcome; do not restore withdrawn18 via an unqualified terminal rule. |

Focused acceptance/proof for the task:

| Gap | Minimum credible closure |
| --- | --- |
| Rejected SQL authority | Installed final schema has no head table or replacement execution authority; production direct/queued/read/embedded/peer/replay paths no longer claim/read/advance it. Preserve applied migration checksums and accepted canonical evidence. |
| Complete SQL-free recovery | Real writer checkpoint read/retry, authoritative fresh absence, and a required owner unavailable **before discovery** while holding a later unpublished accepted revision: fail retryably, never fresh/older. Retained-volume WAL restart returns the exact canonical state. Publication transition preserves source coverage. |
| Durability/replay | Keep generic identical batch retry and ambiguous-commit identity; crash-before-fence recovery follows retained generic rules, acknowledged writes survive restart, actual refused writes do not reappear, unrelated batches in a group still succeed. Adapt existing lifecycle/fence fixtures, no new harness. |
| Canonical terminal/identity | Focused existing negative-flow seams for engine/pre-engine and queue-owned terminal decisions; full sanitized errors/exact refs persist without queue payload; direct/queued pages/result/content survive source deletion and queued-parent removal with current permission/cross-tenant denial. |
| Final-attempt selected result | Existing failed-settlement regression on last attempt: durable selected identity/results recovered and completed without computation, original budget/deadline unchanged; unfinished final-attempt exhaustion remains. |
| Adjacent controls | Normal cancellation/revocation/renewal/deadline, initial input ACK→admission, File descriptors/receipts, group previous-verdict/dedup and Operator delivery behavior retained. Do not prove an unapproved stronger rare-gap exclusion. |

Use existing focused state/outbox units, durable-verifier/verification-runtime integration seams, SQL batch/migration tests and Scribe lifecycle/replay seams. `mise.toml` already owns fmt, SQL/Bifrost integration and Scribe journey lanes; select the narrow tests for the actual write set through repository Postgres, plus applicable format/lints/codegen/tenant-boundary checks. Name and run exact nextest selectors for tests used as closure evidence. Do not require a broad aggregate or full language journey rerun in this remediation task; integrated change review owns those. New/changed-item rustdoc remains mandatory; obsolete tests/docs can be deleted only with their replaced behavioral proof retained.

Existing command recipes for the affected test targets (recommendations, not commands executed during this audit):

```bash
mise exec -- cargo nextest run --locked -p wyrd-server --lib -E 'test(/^verification::state::tests::/) | test(/^scribe_outbox::tests::/)'
mise exec -- scripts/postgres/with-test-postgres.sh -- bash -lc 'mise run db:migrate:all:inner && cargo nextest run --locked -p wyrd-server --features test-support --test integration --test-threads=1 -E "test(/^pg_durable_verifier_work::/) | test(/^pg_verification_runtime::/)"'
mise exec -- scripts/postgres/with-test-postgres.sh -- bash -lc 'mise run db:migrate:all:inner && cargo nextest run --locked -p vala-sql --test integration --test-threads=1 -E "test(/^pg_scribe_batch_commits::/) | test(/^pg_stream_identity::/)"'
mise exec -- scripts/postgres/with-test-postgres.sh -- bash -lc 'mise run db:migrate:all:inner && cargo nextest run --locked -p wyrd-testing --test scribe -P journey --run-ignored=all -E "test(/^lifecycle::/)"'
```

Narrow these module selectors to exact changed/proving tests once their final names exist; add the specific existing migration/replay target only when touched. These selected seams replace running the complete `test:wyrd` or `test:bifrost` aggregates. Existing `mise run fmt:check`, relevant lints and changed generated/error/tenant checks remain required, not claimed passed here.

## Prior outcome, applicability and carry-forward

| Previous item | Current disposition |
| --- | --- |
|18| Withdrawn; no remediation and no direct final ACK requirement. |
|20| Superseded hardening/authority-handoff demand. Remove it as the specification-decision stop; do not reintroduce a new SQL protocol. Historical report remains intact. |
|25| Prior claim that the head itself was allowed and relocation sufficient is superseded. Its request-owned mutation disappears through USER-PG-001 removal; no separate relocation task. Not closed until actual removal is verified. |
|24| Ordinal decoding exists only in the rejected query owner. Subsume by its deletion, not a named-column repair preserving it. Not closed before deletion. |
|16/17/19| Open as above; consolidate PG remediation without duplicate findings. The prior claimed dependency of17 on a new20 decision is superseded. |
|21| Covered publication replay bypasses ordinary current lease confirmation: remains separately open. Normal existing controls are not waived by rare-gap acceptance. No closure inferred from head deletion. |
|22/23| Bounded JSON/Variant preparation and mandatory rustdoc remain separately open. Removal may delete specific affected items, but closure requires later source/proof validation. |

Fresh repo/invariant/data selection is appropriate: the changed authority invalidates prior acceptance of heads and affects persisted recovery/read consumers. Fresh reports cover the SQL permission inventory, execution terminal/identity paths and source/batch/WAL removal boundary. Prior unchanged security/concurrency/system/maintainer evidence carries only exact grants/File custody, short-lock/shared-future/lane behavior, existing control lifetimes and unchanged generic mechanisms. No new implementation or new queue handoff warrants recreating that whole team. A changed implementation with a genuinely new boundary would need targeted review then. The validator finds no missed applicable specialty in this focused audit.

Historical 25 state/outbox units, one engine checkpoint regression, 68 server integrations and fmt/diff checks remain credible for their recorded assertions. They neither prove SQL-free complete recovery nor close retained identity/terminal/final-attempt defects. Hosted dependency observation, encoded-byte/concurrent-run measurements, old-format progressed-custody refusal and other prior proof limits remain where previously recorded. There is no new implementation acceptance or whole-task PASS here.

**Final recommendation: FIX_REQUIRED**, one PG remediation task as above; no new approval decision established. Preserve historical evidence, supersede contrary active rationale explicitly, and keep unrelated findings separately open.

Skipped runtime execution and unrelated platform re-review. Risk: deleting the head without complete existing-owner recovery would turn unavailable acknowledged state into fresh/older state; the separately accepted rare worker gap remains.
