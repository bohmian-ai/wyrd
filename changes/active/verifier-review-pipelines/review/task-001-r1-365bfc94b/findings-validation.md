# Independent findings validation

Recommendation: **FIX_REQUIRED**. The independently validated ledger contains **15 material correction boundaries**. No new product, public API, architecture, security, concurrency-semantics or persistent-data decision is needed beyond the approved revision-16 outcomes. The CLI naming concern remains human-directed and has no finding ID.

Subject: base `f4e79ea00523af5cd9212fdacde64f16c97ad554`, candidate `365bfc94bfef1a153f213462f7ef104afa823741`; first cumulative TASK-001 review. Authority: frozen revision-16 spec, public-contracts.md, output-publication.md and original task; applicable root/nested AGENTS; workflow, Rust/maintainer guides and owning Verifier, security and Bifrost sections. All eight discovery reports and the focused follow-up were read. No intended verdict was supplied.

Source navigation began with CodeGraph in the original checkout. The live checkout subsequently moved during an approved merge, detected as HEAD `fe7a41071dafdeb7939090151654db2d2fe9de99` with conflicts. Validation paused immediately. At the user's explicit direction the orchestrator supplied `/tmp/wyrd-task-001-review-365bfc94b`, detached at the original exact candidate, tracked-clean, with the four authority hashes matching subject.md. Validation resumed there; remaining source reads and identity checks used that checkout. This report reviews the original candidate only. The merge and its integration are excluded. No BLOCKED report or intermediate ledger was finalized.

## Source-local disposition and deduplication

Each row is independently validated against the source and approved obligation, not confirmed by reviewer agreement. REVISED means the cause remains but the final correction is consolidated or narrowed below. A duplicate creates no additional remediation work.

| Source ID | Disposition | Retained ID / resolution |
| --- | --- | --- |
| BEH-001 | REVISED | FIND-TASK-001-1: receipt replay and concurrent same-call identity are one SQL/backend correction boundary |
| INV-R1 | REVISED | FIND-TASK-001-1 |
| SYS-002 | REVISED | FIND-TASK-001-1 |
| CONC-001 | REVISED | FIND-TASK-001-1 |
| CONC-002 | REVISED | FIND-TASK-001-1; preserve its distinct READ COMMITTED interleaving in closure proof |
| BEH-002 | REVISED | FIND-TASK-001-2; existing backend/effective limits, without implementing future expansion/tooling |
| BEH-003 | CONFIRMED | FIND-TASK-001-3 |
| BEH-004 | REVISED | FIND-TASK-001-4, shared invocation/publication cut cause |
| DATA-001 | REVISED | FIND-TASK-001-4 |
| BEH-005 | REVISED | FIND-TASK-001-5, both internal authorization and unavailable-capability propagation |
| SYS-003 | REVISED | FIND-TASK-001-5; ordinary model/check failure remains a judgment |
| BEH-006 | REVISED | FIND-TASK-001-6; missing required proof, not an asserted runtime conversion defect |
| INV-R2 | CONFIRMED | FIND-TASK-001-7 |
| SECURITY-1 | REVISED | FIND-TASK-001-8, one admission/revocation authority boundary across siblings |
| CONC-003 | REVISED | FIND-TASK-001-8 |
| CONC-004 | REVISED | FIND-TASK-001-9, new work/checkpoint/publication authority, preserving committed replay and release semantics |
| SYS-001 | REVISED | FIND-TASK-001-10; approved final-durability amendment controls the correction |
| FUP-001 | REVISED | FIND-TASK-001-10; authority and missing recovery interval independently checked |
| DATA-002 | CONFIRMED | FIND-TASK-001-11 |
| DATA-003 | REVISED | FIND-TASK-001-12, narrow durable event framing correction |
| MNT-001 | REVISED | FIND-TASK-001-13, required backend and existing blocking strategy only |
| REPO-002 | REVISED | FIND-TASK-001-14, one authored declaration omission |
| MNT-002 | REVISED | FIND-TASK-001-14 |
| REPO-001 | CONFIRMED | FIND-TASK-001-15, explicit mandatory changed-item rustdoc gate |

No complete source-local finding is rejected. The final corrections reject or exclude the broader alternatives identified below: new ingestion/queue services, blanket error promotion, downstream reauthorization loops, future Agent tools, a replacement filesystem stack, SQL analytical payload custody, and unrelated prose/style cleanup. The report does not retain the standards review's non-blocking tuple-mapping observation as remediation.

## Validated ledger

Paths below are repository-relative; line numbers name the immutable candidate. Every correction remains within the existing owner. Focused proofs are required closure evidence, not tests executed during this static review.

### FIND-TASK-001-1 — REVISED / INCORRECT — publication identity does not reliably recover the original receipt

Sources: BEH-001, INV-R1, SYS-002, CONC-001, CONC-002. Obligation: REQ-047, AC-040, Task scenario 5 and original-call replay contract.

Locations: `crates/wyrd/wyrd-server/src/verification/hosted/publications.rs:113` (`path`) and `:201` (`accept`); `crates/wyrd/wyrd-sql/src/queries/verifier_executions.rs:690` (`ACCEPT_PUBLICATION_SQL`) and `:3448` (`accept_publication`). Full backend methods and SQL producer/read consumers inspected.

Evidence/scenario: path publication freezes and completes every File before consulting persisted call identity. After a committed file publication and lost reply, replay from removed scratch/a fresh workspace fails; replay during storage outage also fails before finding the receipt. Replacing the same path with a directory reaches SQL replay but returns `content.kind()` from the new proposal, so the receipt says directory while the stored manifest remains file. JSON shares acceptance but not the scratch dependency. Separately, two identical concurrent calls use compatible execution share locks. B starts its statement snapshot before A commits, waits on the unique index, then `ON CONFLICT DO NOTHING` inserts nothing after A commits; its same-statement SELECT cannot see A's row and returns false conflict. Uniqueness prevents duplicates but does not implement correct replay acknowledgement.

Consequence/materiality: the explicit reply-loss recovery scenario can fail or misdescribe accepted evidence, including an ordinary racing transport retry. SQL already retains call ID, argument digest, kind and manifest; a new receipt store is unnecessary.

Correction: make the existing SQL publication identity owner resolve a matching original call and arguments to its stored receipt before mutable scratch/storage work. Order same-identity acceptance/recheck so concurrent identical calls return the same receipt after a committed winner; a fresh statement after conflict or existing transaction-scoped identity serialization suffices. Keep one identity implementation for JSON/file/tree, reject changed arguments/another call, preserve fresh-attempt separation, and validate the current execution fence for new work. Do not hold a transaction over uploads. Return stored kind on replay. A preflight-only lookup without resolving the insertion race is incomplete.

Burden/proof: a bounded SQL/backend correction, not another queue. Commit a file, replace/remove scratch, replay with storage unavailable/current lease and recover original kind with one manifest and no new upload. In separate real tenant transactions, hold A's identical acceptance uncommitted until B waits, commit A, require B replay; distinct-call conflict and stale/new-attempt cases remain correct.

### FIND-TASK-001-2 — REVISED / MISSING — publication limits do not bound retained execution resources or guarantee mountable accepted files

Source: BEH-002. Obligations: Task execution-record cumulative budgets; output-publication.md:98–101 effective file/tree/JSON, count, cumulative retained-byte, staging and deadline ceilings; persisted budgets across recovery.

Locations: `hosted/publications.rs:80,113,201`, `hosted/mod.rs:128`, `hosted/workspace.rs:565`, `hosted/evidence.rs:265`; publication acceptance/migration at `verifier_executions.rs:690` and `migrations/20261104000100_verifier_execution_publications.sql`. All paths are under `crates/wyrd/wyrd-server/src/verification/` except SQL paths under `crates/wyrd/wyrd-sql/`.

Evidence/scenario: JSON has a per-value bound; directory freezing bounds current tree bytes/entries. Successive distinct names can retain repeatedly rewritten bounded scratch without any execution-level count/byte accounting in acceptance or schema. Reclaim does not restore an accounting record because none exists. Default workspace write permits 50 MiB while `TenantEvidence::file_object` rejects over 20 MiB, so a valid 21 MiB file can be accepted and then never mounted. Ordinary trusted backend calls suffice; Task-003's absent tool is irrelevant to the explicitly required backend.

Consequence/materiality: required retained-resource ceilings are absent, restart/concurrency cannot preserve them, and accepted evidence can be unusable by required consumers. This is not a request for new Task-002 fanout/model-token machinery.

Correction: enforce effective backend retention ceilings at the existing execution/publication SQL authority with atomic count/retained-byte accounting preserved across attempts/reclaim and same-call replay charged once. Reuse deployment/host limit ownership and existing serialized JSON/manifest byte/entry measurements; Verifier limits may only lower ceilings. Enforce effective file/consumer limits before completing new content, and deadline before new acceptance. A workspace-only/per-call limit cannot establish cumulative retention. Keep bytes outside SQL and existing File lifecycle; do not invent another budget service, authoring option or public upload surface. Any local default wiring belongs to existing deployment-limit policy, not an expanded product contract.

Burden/proof: bounded execution metadata/acceptance changes are necessary for the explicit cumulative invariant. With small effective limits, cross total/count using individually valid JSON/files; refuse without a new manifest, retain usage across reclaim, charge replay once, and show an accepted boundary-size file mounts while an over-consumer-limit file refuses. Include concurrent acceptance at the shared accounting authority. Preserve staging bounds and no storage/model IO in a SQL transaction.

### FIND-TASK-001-3 — CONFIRMED / INCORRECT — pending publication inventory spends attempts and permanently fails consumers

Source: BEH-003. Obligations: Task scenario 8; output-publication.md:205–211; INV-015/AC-041 bounded content wait, released permits and no model attempt while visibility is pending.

Locations: server `hosted/evidence.rs:138,265`, `hosted/workspace.rs:110,225`, `hosted/tools.rs:114`; `crates/vala/vala-verify/src/blocks/agent.rs:343` and `blocks/engine.rs:745,808,951`.

Evidence/scenario: accepted manifest outlives inventory projection. Workspace preparation calls `published` → `file_object`; absent inventory is generic Retryable. `AgentExecutor::invoke` opens the tool session before `run_prompt`. The engine has already begun a durable attempt/held action permit. Retryable action error uses ordinary retries; zero retries commits a failed action. A check becomes inconclusive. Neither dispatched a model, and eventual inventory visibility cannot undo that committed settlement. File and directory mounts, named and collection consumers share the reader/session path.

Consequence/materiality: normal supported projection lag changes valid queued work into an irreversible verification failure. Metadata HTTP 503 handling does not repair this producer-to-consumer path.

Correction: represent pending publication content at existing evidence/workspace admission and carry it through pre-dispatch deferral rather than ordinary action/check judgment. Reuse engine forget/defer and runner release behavior so unstarted work consumes no attempt and permits/workspaces are dropped while waiting; retry only within the same execution deadline/cancellation boundary. Preserve terminal size/digest errors and ordinary provider errors. No SDK poller, new freshness mode or scheduler is needed.

Proof/burden: hold inventory invisible after acceptance, run a zero-retry action and Agent requirement, assert no provider dispatch/spent attempt/permanent judgment, restore visibility and obtain exact bytes with one real invocation. Expiry ends boundedly. The smallest proof uses existing hosted/queued seams; no load harness.

### FIND-TASK-001-4 — REVISED / INCORRECT — insertion identities do not freeze committed page membership

Sources: BEH-004, DATA-001. Obligations: Task scenario 9 and public-contract stable first-page cut, no gaps/duplicates while concurrent writes commit.

Locations: `crates/wyrd/wyrd-sql/src/queries/verifier_executions/inspection.rs:19–55,135,206`, `verifier_executions.rs:650,690`; invocation/publication migrations. Invocation begins and publication accepts are sibling writers with independent transactions and compatible execution share locks; hosted step concurrency is supported.

Evidence/scenario: row seq1 is committed; writer A allocates seq2 but holds its transaction; writer B commits seq3. First page limit1 reads seq1 with through=3. A commits between pages, so continuation now includes seq2 which was absent at the first cut. Another ordering skips a lower sequence already behind after. `max(sequence)` measures visible allocation, not commit ordering; reader snapshots in separate HTTP calls cannot recover original membership. Existing page test commits in order on one connection.

Consequence/materiality: normal parallel work violates the public stable-list promise. This is inconsistent inspection, not physical evidence loss.

Correction: at the existing execution SQL owner coordinate sequence assignment/commit across invocation/publication insert siblings and first-cut acquisition so no lower sequence can become visible after its cut is exposed. Reuse execution-row coordination; serialize the bounded insert transaction before its sequence allocation and establish the first cut against that ordering, not filesystem locks or client retries. Keep storage/model IO outside these transactions and preserve nullable producer identity. Indexing alone or per-request repeatable-read alone is insufficient.

Proof/burden: real separate transactions hold a lower writer open while higher work/first-page acquisition are attempted. Continuations equal exactly first-cut committed membership, fresh polls include later work; cover both paged tables. This is a small coordination correction using an existing authority, not a persistent snapshot service.

### FIND-TASK-001-5 — REVISED / MISSING — internal exact-capability resolution has the wrong permission and failure semantics

Sources: BEH-005, SYS-003. Obligations: public-contracts.md:31 internal definitions under verifier:run without cards:read; REQ-014/AC-025 and Task scenario 7 unavailable exact Agent/Prompt terminates errored with retained work.

Locations: server `hosted/blocks.rs:150`, full `hosted/registry.rs`, `runner.rs:338,550,1074`; `vala-verify/src/blocks/agent.rs::resolve/invoke`, `blocks/engine.rs:808,951` and `Attempted::settle`.

Evidence/scenario: HostedBlocks installs TenantRegistry. It calls the public `get_card_spec_by_ref_for` which authorizes Card read. A valid exact verifier-run caller without Cards permission therefore cannot load referenced Agent/Prompt definitions. Deleted exact Cards are excluded by the Card SQL reader; registry returns Terminal, action execution turns that into failed StepResult, and Agent requirement errors become inconclusive. Runner receives Completed rather than execution errored. Runner preflight checks Verifier/subject only. Recovery after a predecessor checkpoint reaches exactly this path without substituting any version. Action and check consumers share resolvers; Artifact/source reads have different authorization.

Consequence/materiality: supported verifier-only execution is refused, and required capability unavailability is recorded as observed verification judgment, possibly still passing a threshold. Both outcomes are explicit contract failures at a shared resolver boundary, not general provider-failure hardening.

Correction: use the existing tenant-bound internal definition lookup under accepted exact-Verifier execution authority, retaining exact kind/name/version/UID, deletion checks and audit. Propagate required-definition unavailability distinctly to the existing queued terminal-error boundary; preserve ordinary provider/model/code/check failure judgments. Do not grant general cards:read, loosen Artifact/public Card reads, substitute versions or preflight completed work unnecessarily. Reconcile stale owner text where it still requires the old public-read permission. One resolver/error-propagation change covers normal and recovered unfinished work.

Proof/burden: verifier-run-only referenced Agent/Prompt succeeds while explicit Card fetch still refuses. After committed predecessor work, delete the required Agent and separately Prompt, resume action/check and assert errored/unavailable, no provider call/version substitution, unchanged checkpoints/publications. Ordinary provider/check failure remains a judgment. This is bounded owner reuse, not a second recovery executor.

### FIND-TASK-001-6 — REVISED / MISSING — required Python/TypeScript success journeys are absent

Source: BEH-006. Obligations: Task scenario 9/Public surface closure and root per-shipping-surface user journeys.

Locations: `sdks/wyrd-sdk-python/tests/integration/test_durable_verifier_work.py::test_execution_pages_are_stable_and_tenant_scoped`; `sdks/wyrd-sdk-ts/wyrd/tests/integration/durable-verifier-work.test.ts`; existing `crates/wyrd/wyrd-testing/src/verification.rs:678::publish`.

Evidence: both tests wait for completed requirement-only work and assert zero publications. Python never calls successful download; TS only calls missing-publication download. Neither crosses the actual binding boundary with JSON/null, file/directory manifest, successful plan or unfinished cancellation. Static thin wrappers and Rust/MCP success journeys do not satisfy the explicit language proof requirement. This finding asserts missing mandated proof, not an unobserved runtime conversion bug.

Correction: extend existing language journeys against real WyrdTestServer using the existing trusted publication fixture through the test-only binding/fixture owner, with minimal exposure needed for seeding. Assert successful JSON/null, exact file/directory manifests, selected producer and usable plan, viewer denied cancel and authorized live/repeated cancel through public SDKs. Retain current malformed/foreign/authorization coverage. No production upload endpoint or Task-003 Agent tools; Task 3 separately proves content creation through tools.

Proof/burden: run the two exact existing journey selectors after extension and narrow language/native checks for the changed test seam. Reuse storage correctness proof from hosted tests rather than duplicating every storage failure in each language. The explicit Task-001 proof cannot be deferred merely because the Rust-only seeding seam needs a small test-only bridge.

### FIND-TASK-001-7 — CONFIRMED / INCORRECT — target loading replenishes the remaining deadline

Source: INV-R2. Obligations: REQ-014, Task scenarios 2/7 remaining deadline and no new work after timeout; PostgreSQL coordination time.

Locations: SQL `verifier_executions.rs:457–474`; server `runner.rs:798–844` claim and `:404` execute.

Evidence/scenario: CLAIM computes remaining_ms at database statement time. Runner awaits each target resolution in the claim batch before execution. Execute then constructs `Instant::now() + run.remaining`. Time spent in target reads, subsequent batch resolution/commit and dispatch scheduling is added back. Near-expiry reclaim can dispatch a provider after persisted deadline. `loaded` timing is used for telemetry, not subtracting budget. Engine timeout is correct relative to its incorrect replenished Instant.

Correction: conserve one monotonic deadline anchored to the database remaining-time observation through target loading/dispatch, using existing claim timing; alternatively refreshing the database interval must be at the same owner before dispatch, without granting elapsed time. Select the anchored claim-to-dispatch deadline approach: target loading and queue handoff consume that budget. No wall-clock comparison or clock abstraction. Preserve direct verification and SQL-owned deadlines.

Proof/burden: delay target loading beyond a small persisted remaining budget; assert timeout and zero provider/action dispatch, then shorter loading leaves only residual time. Existing reclaim SQL proof establishes persistence but not this handoff. A local timing handoff correction suffices; no new scheduler.

### FIND-TASK-001-8 — REVISED / INCORRECT — principal revocation and delayed admissions are not ordered

Sources: SECURITY-1, CONC-003. Obligations: REQ-014/AC-025, Task scenario 7, security owner active-work revocation; frozen grants may be reduced by revocation.

Locations: `wyrd-auth/src/revoke.rs:47–108`; server `components/verification/service.rs:718–790`, `verification/submissions.rs:236–306`; SQL `verifier_executions.rs:112–140,629–636,1955,2264,2815`.

Evidence/scenario: enqueue freezes AcceptedAuthority before Scribe→submission handoff. Revocation suspends principal and cancels visible unfinished rows in its own transaction. A handoff pending/insert uncommitted during that scan can later create pending work. Admissions check Cards/idempotency, not shared locked principal state. Claim and Caller::accepted restore frozen permissions, so pure actions and later recovery can execute after revocation without a token expiry bound. Scheduled/grouped admissions also carry earlier authority and use sibling insert owners; earlier preparation checks cannot order the later commit.

Consequence/materiality: successful administrative revocation can be followed by newly runnable work under retired authority on ordinary asynchronous delay/concurrency. Existing active/pending-row cancellation tests do not exercise it.

Correction: order durable admission for subject/accepting actor against the existing principal suspension/revocation transaction, validating active state while holding the common principal lock through bounded admission commit. Apply consistently to submitted/scheduled/grouped admission owners in stable lock order. If admission wins, revocation sees/cancels its committed row; if revocation wins, no delayed runnable row is created. Preserve frozen accepted grants, completed records, idempotent replay and tenant capability types. Do not hold a lock through Scribe/model IO or add downstream authorization checks/token epochs.

Proof/burden: pause handoff before admission, revoke/commit, release and assert no invocation dispatch; reverse with uncommitted admission and concurrent revoke, then assert cancellation/refusal. Cover sibling admission paths at their existing seams. Common principal/queue transaction coordination is required; an unlocked state check cannot close the race.

### FIND-TASK-001-9 — REVISED / INCORRECT — new work fences accept database-expired leases before reclaim

Source: CONC-004. Obligations: current lease for dispatch/checkpoints/publication; Task stale-write refusal; owner says expired lease is never revived.

Locations: SQL `verifier_executions.rs:650–710`; server `runner.rs:1135–1193`, `hosted/publications.rs::accept`, `verification/leases.rs::renew/renew_tenant`.

Evidence/scenario: BEGIN/SETTLE/publication fenced SELECT checks running/token only. Expiry leaves both unchanged until reclaim. Renewal rejects expired database leases but its asynchronous sequential tenant loop cancels local work only after the database result/error. A pause/delayed renewal can resume a checkpoint or dispatch admission after SQL expiry and before reclaim/cancellation. Trusted backend accepts ExecutionLease without a renewal watcher as well. Status/token tests cover reclaim/cancel, not expiry alone.

Correction: require unexpired database-owned lease time in the existing SQL predicates admitting new invocation/checkpoint/publication work, not a local clock guard. Preserve stored receipt/readability, legitimate terminal completion replay and administrative release; do not mechanically rewrite unrelated lifecycle predicates. Coordinate with findings 1/2/4 at the same SQL authority.

Proof/burden: expire a lease on PostgreSQL time without reclaim; begin/settle/new publication refuse, retained evidence remains readable, current re-claim works. Preserve cancel/replay tests. Predicate correction is smaller and stronger than another timer/watch service.

### FIND-TASK-001-10 — REVISED / MISSING — completed state can outlive the only final-result payload

Sources: SYS-001, FUP-001. Obligations: REQ-013 and AC-007/008, Task durable completion/scenario 2; explicitly approved architecture amendment 2 (`spec.md:926–942`).

Locations: server `verification/completions.rs:198–242`, `runner.rs:670–760,1000–1064`, `scribe_outbox.rs:5–32,1800–1842`; SQL COMPLETE/CLAIM; public `components/verification/service.rs:260–280`, `verification/inputs.rs:395–445`; shared client `verify/execution.rs:298–334`; stale `architecture/wyrd-design.md:1837–1840`.

Evidence/scenario: result builder selects final ID and a process-memory write. Both batched completion and fallback apply queue completion, stage on reserved outbox slot, commit SQL without durable result ACK. Slot reservation is capacity, not custody. Crash before Scribe accepts detail/summary loses payload. Completed rows are not claimed; history projector emits events, never result payloads. Readback joins result/output/input and returns None when absent; SDK polls completed-without-result until timeout. No durable recovery owner supplies lost bytes. Existing journey flushes Bifrost before final restart, outside this interval.

Authority reconciliation: the old owning paragraph permits best-effort result loss, but the frozen spec expressly strengthens that exact boundary and explicitly authorizes the Verifier settlement/Bifrost owner amendment. Workflow recognizes approved outcomes and requires missing owner updates in remediation. The required correction updates that paragraph within approved authority; keeping best-effort completion would revise the approved outcome. Non-blocking audit and actual Scribe ACK semantics remain locked.

Correction: use the existing result/completion/outbox handoff to obtain actual durable Scribe acceptance of the exact selected result before completed SQL acknowledgement/commit. Reuse ACK notification patterns already used for inputs/events, distinguishing accepted result from dropped/rejected writes. Preserve selected identity, detail-before-summary, batched settlement/fallback, lease/cancel fencing, no transactions across external IO, tenant authority and unrelated service availability. Lost ACK/commit must leave replayable work from existing checkpoints; do not store analytical payloads in SQL, reclaim all completed rows, reconstruct results at every read or wait for Iceberg visibility. Update the owning paragraph in the same correction.

Proof/burden: delay result durable ACK after committed action/checkpoints, interrupt without draining/flushing, restart same durable backing and obtain the selected final result without repeating committed provider work. Assert no unrecoverable completed state and correct rejected/lost ACK/commit handling. This extends the existing handoff, requiring a bounded ACK path rather than new ingestion machinery. A generic outbox-drained signal is insufficient because sink success can include terminally lost writes.

### FIND-TASK-001-11 — CONFIRMED / VIOLATION — required sequence page indexes are absent

Source: DATA-002. Obligation: Task line 30 tenant-scoped indexed backlog-sensitive page queries and bounded results.

Locations: SQL `verifier_executions/inspection.rs:21–55`; migrations `20261104000000_verifier_execution_invocations.sql:31`, `20261104000100_verifier_execution_publications.sql:34`, `20261105000000_verifier_execution_inspection.sql:20`.

Evidence: invocation/publication identity indexes order tenant/execution/task/item/iteration/attempt(/name), omit sequence. Inspection computes max(sequence) and continuation range/order by sequence. Existing event-table sequence index cannot serve these tables. LIMIT bounds returned count but not the per-execution scan/filter/sort. This is the exact explicit missing access path, not an unmeasured latency claim.

Correction/proof: add the tenant/execution/sequence index to each paged table in a new immutable migration, retaining producer uniqueness and due/leased geometry. Use representative retained rows to inspect cut/continuation range/order plans and run existing page proof plus finding-4 interleaving. Two narrow indexes suffice; no paging abstraction. Keep this distinct from cut correctness: either fix alone leaves the other obligation unmet.

### FIND-TASK-001-12 — REVISED / INCORRECT — event replay physical identities depend on mutable frame budget

Source: DATA-003. Obligations: REQ-048/Task stable event/sequence/batch replay across worker restart; Scribe same-identity dedup.

Locations: server `scribe_outbox.rs:188–190,1237–1249,1306–1325,1370–1410,1510–1536,1580–1587`; `verification/history.rs:95–161`; config `:331–332`; Scribe `shards.rs:1157` and `vala-sql/src/queries/scribe_batch_commits.rs::classify`.

Evidence/scenario: SQL restores batch identity/membership. New ExecutionEvents frames use that batch key but chunks depend on current configured request ceiling; physical ID hashes key/frame/chunk/start, not immutable event range. At ceiling A physical chunk0 accepts events A,B, ACK is lost. Restart with valid larger ceiling B recomputes chunk0 as A,B,C,D under the same ID. Scribe compares logical digest/count, so same-version identity contradicts accepted rows and cannot ACK. The assigned head history batch retries and blocks progress. Capacity configuration is a supported deployment setting; the new durable key across processes creates this issue although transient chunk machinery is inherited.

Correction: for durable execution events use stable per-event physical identity/membership through the existing outbox, derived from retained event identity under the persisted batch, so configurable grouping cannot reuse an ID for different rows. Keep event identities/counts and Scribe digest checks; do not simply add budget to IDs, which can duplicate already accepted events. Do not change ordinary audit/capture/output delivery or add an ingest service. A single event too large for an explicitly configured valid request remains a separate ordinary size refusal, not a reason to repartition identity.

Proof/burden: partially accept a multi-frame event batch, lose ACK, replay through a fresh sink at another valid ceiling where individual rows fit. All event IDs appear once and SQL position acknowledges. Event-specific stable membership is bounded and cheaper than persisting a second transport plan; no expanded deployment compatibility policy is required.

### FIND-TASK-001-13 — REVISED / VIOLATION — required async snapshot backend blocks its runtime worker

Source: MNT-001. Obligation: `agent_docs/rust.md` prohibits blocking async request paths without explicit blocking strategy; Task-001 requires trusted snapshot backend even before Task-003 Agent tools.

Locations: server `hosted/workspace.rs:565–636` (`freeze`), caller `hosted/publications.rs:113`; nearby existing `Workspaces::open` and hosted tool filesystem blocking strategy.

Evidence: freeze awaits writer mutex then synchronously traverses/read_dir/reads the whole tree in one async poll, with defaults 50 MiB/10,000 entries. A stalled filesystem occupies the Tokio worker and cannot yield to timeout/lease/cancellation work. The backend is explicitly required and callable through trusted hosted seam/tests; absence of Agent tool dispatch does not waive its required execution strategy. No measured service outage is claimed.

Correction: retain strict snapshot/confinement at Workspace and run synchronous freeze IO using its existing blocking-pool strategy. Writer exclusion must remain owned by the blocking snapshot for its full lifetime, including if the awaiting future is cancelled. Release after frozen bytes are owned, before uploads. Reusing best-effort walker would weaken completeness; spawning a new service is unnecessary.

Proof/burden: retain existing directory completeness/refusal and held-writer proof; inspect/test cancellation does not release exclusion while detached blocking work still reads. No synthetic runtime load test. This is a bounded execution-boundary correction, not new filesystem machinery.

### FIND-TASK-001-14 — REVISED / INCORRECT — authored Python download plan omits required expiry

Sources: REPO-002, MNT-002. Obligations: authored Python stubs precisely project shared wire contracts; first-class typed File download plan.

Locations: `sdks/wyrd-sdk-python/python/wyrd/verify/__init__.pyi:463–466`; producer `crates/wyrd-spec/src/storage/download.rs:40–48`; `PyExecution::download` in SDK `src/verify.rs:359`.

Evidence/scenario: storage always returns get_url and ttl_secs (cloud expiry, local zero). Inspection/shared Execution preserve response; PyO3 serializes the whole object. New TypedDict has only get_url, so typed callers cannot access the valid expiry key or construct a literal of the real fixed record without type errors. TS projection includes expiry. Green existing typecheck does not inspect that missing access.

Correction/proof: add required `ttl_secs: int` to the existing authored TypedDict, with owner meaning, and exercise actual-shaped mapping/expiry access in the existing typecheck fixture. Narrow Python typecheck/lints suffice; no runtime change, new export or cloud fixture. This one-field correction fully closes both reports.

### FIND-TASK-001-15 — CONFIRMED / VIOLATION — mandatory changed-item rustdoc gate is incomplete

Source: REPO-001. Obligation: root AGENTS changed Rust items require rustdoc, Errors for fallible items, Panics where applicable; Rust guide includes private/test items and fields. This explicit merge gate is not waived by materiality preferences or existing lints.

Validated locations: SQL `verifier_executions.rs:1708::PublicationRow` eight fields and `verifier_executions/inspection.rs:91–110` thirteen row fields; auth `revoke.rs:99::cancel_executions` missing Errors; runner `:1164,1173,1182` checkpoint implementation methods missing item/error docs; engine tests `:2339,2351,2363` Ledger methods missing Errors (currently always Ok); spec `verification/ids.rs:105–124` new FromStr/Deserialize/JsonSchema items missing item/error docs; runtime permission `:1410::verifier_run_is_uid_scoped` and spec ids `:270` publication-name test missing Panics; server `tests/integration/pg_durable_verifier_work.rs:162,171` call/rule helpers missing Panics; MCP verification test `:2955–2959` five local tool-name constants and Rust durable-work journey `:133–134` REVIEW/JUDGE constants undocumented.

Evidence/materiality: item bodies actually contain fallible SQL/parsing paths, expect/assert/panic or undecorated fields/constants as reported. Type-level/trait docs do not satisfy the explicitly item-wide gate. This is a bounded mandatory correction, not open-ended prose improvement.

Correction/proof: document these touched items in place with meaning/invariants, precise Errors and Panics as applicable, including “never returns Err” where true. Use links rather than duplicate contracts when sufficient. Reinspect the bounded list and run formatting/lints for edited Rust; no new checker or behavioral test. Batch with behavioral remediation; do not create a style-only round or expand to unrelated pre-existing items.

## Reconciled acceptance and scope

| Required outcome / constraint | Reconciled result and source/proof assessment |
| --- | --- |
| Durable admission/idempotent keyed replay | PASS for existing admitted-input/row ordering: service → Scribe ACK → SubmissionSink commit → AdmissionReply; recorded Rust response-loss journey supports it. Revoked authority admission has finding 8. |
| Separate committed action/requirement restoration, ambiguous attempt counting | PASS for existing plain-work checkpoint restore/counting, engine/SQL/Rust journey evidence; not a pass for remaining deadline or expiry, findings 7/9. |
| Durable final completion/readback | FAIL, finding 10. Explicit approved owner amendment resolves the architecture conflict; flush-before-restart proof cannot cover crash-before-ACK. |
| Immutable file/JSON/null and complete directory snapshot | PASS for all-or-nothing accepted content/strict tree grammar/writer exclusion on normal execution; findings 1/2/13 limit replay, budgets and execution strategy, not content immutability already proven. |
| Publication uniqueness/fresh attempts | PASS for tuple uniqueness/fresh attempt separation; original/concurrent same-call receipt FAIL, finding 1. |
| Typed retained mounts, honest collection statuses and producer-only source lineage | PASS for inspected ordinary mounts/provenance, existing mount/engine proofs. Pending visibility FAIL, finding 3; accepted consumer size FAIL, finding 2. Task-002 expansion/speculation remains excluded. |
| Cancel preserves accepted work and terminal results | PASS for visible pending/running rows, repeated terminal cancel, token replacement and worker-format refusal; findings 8/9 narrow the authority windows not covered by those tests. |
| Required exact Agent/Prompt availability and verifier-run internal loading | FAIL, finding 5; injected-engine/card-read-granted scenario does not prove the required resolver path. |
| Canonical event table/Variant/history-only semantics and ordinary replay | PASS for registration, event intent transaction and same-configuration replay; frame-configuration recovery FAIL, finding 12. Preserve execution_outputs semantics. |
| Dedicated metadata-only exact-Verifier read/cancel, tenant and manifest-selected downloads | PASS for shared service/HTTP/Rust/MCP source and recorded journeys, including local verifier-read-only bytes. No Cards/Bifrost fallback or arbitrary path introduced. |
| Stable bounded indexed pages | FAIL findings 4/11. Existing in-order page proof establishes only sequential geometry. |
| Thin HTTP/Rust/Python/TS/CLI/MCP projections | PASS for owner split and method presence. Python expiry parity FAIL finding 14; required Python/TS content/live-cancel proof FAIL finding 6. Task 3 creates content via tools and does not defer Task-001 read-binding proof. |
| Required changed-item rustdoc | FAIL finding 15. Optional prose preferences remain excluded. |
| Existing plain Verifiers/direct synchronous verify; no extra Card kind, queue, graph store, ingest path or tool exposure | PASS on selected cumulative coverage and inspected affected callers; corrections must retain these non-goals. |
| Supported-platform parity | No platform-specific implementation expansion identified. Actual all-target CI evidence absent; record as verification limit for change/CI, not an invented implementation finding or an executed PASS. |

All selected reports exist and cover the assigned task/boundaries. The independent pass filled the specific routed uncertainties and did not restart comprehensive discovery. No new blocker was discovered by this validator requiring another fresh discovery agent. No prior findings/remediation rounds exist; convergence assessment is inapplicable.

## Correction grouping and verification limits

One bounded remediation packet may package this ledger with cohesive sub-boundaries: execution authority/time (7–9), publication SQL/hosted behavior (1–4,11,13), resolver/error handoff (5), final-result and event delivery (10,12), and surface proof/declarations/docs (6,14,15). Shared SQL execution-row ordering must reconcile replay, budgets and page cuts together; avoid separate competing lock/identity mechanisms. Preserve accepted checkpoint recovery and unrelated service availability throughout. Require focused closure tests for each diagnosed interval and only narrow owner lanes, not full journey suites or broad gates during task remediation.

This is static independent validation. No Cargo/Postgres/Python/TypeScript execution, fault injection, EXPLAIN, formatter or codegen ran here; recorded green checks were inspected as recorded evidence, not independently rerun. Concurrency/configuration failures are source-derived interleavings; claims do not depend on measured latency or synthetic host load. The live merge interruption was resolved by an exact detached candidate, not by accepting merged source as the review subject. Final recommendation remains **FIX_REQUIRED**, FIND-TASK-001-1 through FIND-TASK-001-15.

Human CLI naming: help identifies queued Verifier executions and no approved namespace is violated. A rename needs a separate explicit human decision, has no FIND ID and is not part of this remediation.

Skipped runtime reproduction and all-platform CI; material risks are the source-backed durability, authority, replay and acceptance gaps above.
