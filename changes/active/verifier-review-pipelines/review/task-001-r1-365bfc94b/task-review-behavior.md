# TASK-001 behavior acceptance review

Overall result: **FAIL**.

Subject: repository `/home/thorrester/Documents/GitHub/wyrd-worktrees/code-review-workflow`, base `f4e79ea00523af5cd9212fdacde64f16c97ad554`, candidate `365bfc94bfef1a153f213462f7ef104afa823741`. Reviewed the entire three-dot range. HEAD remained the candidate at the end of inspection. This is the first review; no carried-forward acceptance evidence or other reviewers' reports were used.

Authority: approved revision 16 `spec.md`, `public-contracts.md`, `output-publication.md`, original `tasks/TASK-001-durable-agent-work.md`, root and applicable SQL/Python/TypeScript/docs AGENTS, `changes/spec-driven-development.md`, architecture map and changed owning Verifier/Bifrost/security sections, and maintainer/Rust/errors/agent-surfaces guides. CodeGraph was consulted before source navigation. Task 2 expansion/repeat/speculation behavior and Task 3 Agent tool dispatch are excluded; backend retention, recovery, handoff and inspection already belong to this task.

## Independently expanded navigation and coverage

* Submission: `VerificationControl::enqueue` → Scribe input acknowledgement → `SubmissionSink::write` → queue admission/commit → `AdmissionReply`. Matching replay and refusal flow through the same SQL owner.
* Recovery: runner claim/target resolution → persisted input record → `HostedBlocks::engine` → `with_checkpoints` → action/requirement `begin`/`commit` → result completion. Expanded into the shared `AgentExecutor::invoke`, `TenantRegistry`, and error-to-settlement paths, rather than treating saved rows as proof of recovery semantics.
* Retention: trusted `Publications::{json,path}` → `Workspace::freeze` → File init/reconciliation/inventory → SQL acceptance → event trigger. Followed receipt replay and mutable workspace changes independently of the happy-path publication tests.
* Consumption: reference parser/validator → `Scope::resolve` → `Workspaces::open`/`Mount::publication` → `TenantEvidence::published/file_object` → hosted tool-session setup → engine retry/settlement. This expanded the starting map to the pre-model inventory-lag path.
* Inspection: SQL sequence allocation/writes → page-cut queries → cursor validation → `VerificationControl` → shared `Execution` → Python/N-API/TS/CLI/MCP wrappers. Reviewed public tests against the exact required operations, not only their names.
* History/cancellation: transition triggers → durable event batch identity → `EventProjector` → Scribe outbox; cancel/revoke → SQL fence → renewal/checkpoint/publication/final-write consumers. Reviewed table registration/capture changes and result-table preservation in the diff.

Specialists were selected by the orchestrator for concurrency, data, security, resilience, repository standards and maintainability; this report does not rely on their conclusions. No replacement queue, Agent loop, graph store, storage service or report schema was found in this range. The new publication and checkpoint owners extend the mapped existing execution/workspace/File paths. The findings below concern missing behavior at those owners, not an alternative design preference.

## Acceptance matrix

Verification named here is recorded implementation execution, cross-checked against test source; it was not independently rerun in this review. A PASS row covers that stated bounded property, not all neighboring obligations.

| Requirement, acceptance criterion, constraint or non-goal | Implementation evidence | Verification evidence | Result |
|---|---|---|---|
| Scenario 1 / REQ-013: 202 only after durable admission; response-loss replay retains execution; conflict refuses | `service.rs::enqueue`, `submissions.rs::AdmissionReply` and `SubmissionSink::write`; reply follows SQL commit | Rust `queued_acceptance_survives_response_loss` drives restart and identical/conflicting keys | PASS |
| REQ-014 / AC-006: independently restore committed plain action and requirement work | `engine.rs::{with_checkpoints,restored,step,requirement}`, runner checkpoint loading and `ExecutionCheckpoints` | Rust `action_checkpoint_survives_unfinished_requirements`; SQL `invocation_checkpoints_are_fenced_and_survive_reclaim` | PASS |
| REQ-015: conservatively count ambiguous unfinished action attempts | `engine.rs::recorded/attempt`, committed begin rows and interrupted-attempt exhaustion | Engine `checkpoints_restore_settled_blocks_and_count_interrupted_attempts` | PASS |
| REQ-014: recovery retains deadline and accepts only compatible worker format | queue `CLAIM_SQL`, `WORKER_FORMAT`, persisted `deadline_at`, runner `run.remaining` | SQL checkpoint test and runtime cancellation/version proofs | PASS |
| Task scenario 7 / REQ-014 / AC-025: missing Agent/Prompt capability is an execution error; internal definition reads require verifier:run rather than cards:read | `HostedBlocks::engine` still installs `TenantRegistry`, which invokes public Card reads; terminal resolution failures settle an ordinary failed action/check | Existing scenario 7 uses an injected engine and grants `card_read`; it does not exercise this missing path; BEH-005 | FAIL |
| Scenarios 3–4 / INV-004: File/JSON/null bytes and immutable tree manifests commit only after storage completion | `publications.rs::{json,path,complete,accept}`, `workspace.rs::freeze`, SQL acceptance | Server file/JSON and directory snapshot tests check exact bytes, JSON null, mutation and rejected tree absence | PASS |
| REQ-046 / AC-039: complete writer-excluded nested tree snapshot, Unicode/spaces/empty directories and unsafe-entry refusal | `Workspace::freeze`, `Tree::admit`, writer lock and strict read errors | `directory_snapshot_is_complete_or_rejected` covers nested names, empty dirs, lock exclusion and rejected cases | PASS |
| Task execution-record budgets and publication supplement: effective per-file and cumulative retention/count ceilings survive replay/recovery | `Publications` checks only each JSON serialization and workspace snapshot; SQL has no cumulative accounting, and accepted file size can exceed downstream reader limit | Existing size proof lowers work limit only; BEH-002 | FAIL |
| REQ-047 / AC-040: original same-call receipt survives mutable/replaced/deleted scratch | Path replay freezes/uploads first and returns the proposed kind, not the stored kind | Replay proof repeats unchanged scratch only; BEH-001 | FAIL |
| REQ-047: separate call/name conflicts and fresh-attempt identity do not merge work | `ACCEPT_PUBLICATION_SQL` unique producer/name key plus call/arguments comparison | `publication_replay_preserves_attempt_identity` | PASS |
| REQ-012 / INV-004: interrupted upload or stale manifest commit never exposes a partial publication | Upload precedes manifest acceptance; lease/status predicate gates acceptance; SQL row is all-or-nothing | Replay test injects upload failure/reclaim; cancellation test rejects later writes | PASS |
| REQ-010–011 / scenario 6: typed named/collection handoff, missing-name refusal, honest failed/skipped status and producer provenance | Reference validation, `Scope::resolve/collection`, workspace mount layout, exact producer selection | Engine `publication_references_resolve_to_attempts_and_statuses`; server `publication_mounts_preserve_status_and_lineage` | PASS |
| INV-005 / AC-005: generated descriptors/scratch cannot grant original-source citations | `CallInput.sources`, retained scope sources, `TenantEvidence::cited`; manifest has no citation sources | Server mount/lineage test checks source sets and forged citations | PASS |
| REQ-049 / INV-015 / scenario 8: inventory lag waits within deadline without spending model attempts or permanently failing consumers | New publication mount errors pass through `BlockError::Retryable` into step retry exhaustion | History proof checks inspection metadata, not this handoff; BEH-003 | FAIL |
| REQ-015 / scenario 7: cancel/revoke fence checkpoint/publication/final writes and retain accepted work; repeated terminal cancel is harmless | queue `CANCEL_SQL`, `cancel_principal`, revoke transaction, lease-fenced writers and completion | Runtime `cancel_and_revocation_fence_unfinished_work`, server cancellation test, Rust/MCP repeated cancellation | PASS |
| REQ-013–015: direct verify remains synchronous and creates no durable continuation | Direct `run_direct` builds engine with `execution: None`; enqueue alone uses durable handoff | Existing real-time verify journeys retained/updated | PASS |
| REQ-048 / INV-009 / AC-041: canonical events, stable event/batch identities, Variant JSON, projection position, unchanged execution_outputs meaning | SQL event triggers/batch assignment, `EventProjector`, Scribe event write, canonical schema and capture allowlist | `events_replay_without_hiding_live_progress` checks IDs/dedup/Variant and one completed output row | PASS |
| REQ-049 / public contracts: progress lists are metadata-only, typed exact-attempt read/download and typed invalid/not-found responses | inspection service, SQL summary page, producer selector, manifest-selected File download/local content route | Rust public journey, OpenAPI/routes, MCP journey | PASS |
| Public contracts: a listing's first committed cut does not mix later commits | max(identity sequence) cut plus concurrently share-locked writers does not establish commit ordering | Existing page test commits entries sequentially; BEH-004 | FAIL |
| REQ-049 / INV-002: dedicated exact-Verifier read/cancel permissions and tenant isolation; no cards/Bifrost fallback on inspection | `inspection.rs::{require_verifier,authorized,select,file,cancel}`, status feature grants, RLS, internal fixed record queries | Rust/MCP/routes test exact grants, foreign tenant, viewer denial and local content redemption | PASS |
| REQ-032/049: thin shared-client projections exist on HTTP/Rust/Python/TS/CLI/MCP | Shared `verify::Execution`, Python `PyExecution`, N-API/TS Execution, CLI verbs, MCP typed handlers and schema generation | Static cross-surface method/signature tracing; codegen/type/lint checks recorded green | PASS |
| Scenario 9 public-surface closure: every first-class SDK proves real JSON/null/file/directory read/download and live cancel | Python/TS journeys explicitly use a terminal execution with no publications | Python/TS tests and task evidence admit limitation; BEH-006 | FAIL |
| INV-011: old unmapped Verifiers and old records remain usable without invented settlements | New checkpoint tables have no fabricated backfill; new references optional; direct and plain engine behavior kept | Existing verify-in-real-time journeys and engine tests retained | PASS |
| Non-goals: no fs.write/publish Agent tools, public arbitrary-path upload endpoint, parallel queue/ingest, report schema or SDK durable behavior | Complete range and production publication caller search | Static range/caller inspection | PASS |
| INV-010: portable implementation and required all-target verification | No new platform-specific production mechanism seen; Unix-specific fixtures remain tests | Recorded evidence is local; no macOS arm64/Linux arm64 execution evidence supplied. This remains a change/CI verification limit, not a claimed cross-platform pass | FAIL |

## Material findings

### BEH-001 — INCORRECT: path receipt replay depends on current scratch and can return the wrong kind

Obligation: REQ-047, task scenario 5, AC-040, and the supplement's original-call receipt replay contract.

Locations: `crates/wyrd/wyrd-server/src/verification/hosted/publications.rs:122–149,207,241`; queue replay comparison at `verifier_executions.rs:690–712`.

Reachable path: trusted hosted caller publishes file `/work/report`, commits it, and loses the response. The invocation later edits/removes that scratch path, or recovery replays the original call in a fresh workspace. `path` freezes and uploads the current path before asking SQL whether this call already committed. Missing scratch or a storage outage therefore refuses a call whose immutable publication is already durable. If the path now names a directory, SQL recognizes the same name/path arguments and returns Replayed, but `accept` returns `content.kind()` from the newly proposed directory: the receipt says directory while the durable publication remains file. JSON callers share `accept`, but do not depend on workspace reads; file and directory paths share the defect. The current replay test never mutates scratch between calls.

Consequence/materiality: recovery cannot recover an acknowledged identity independently of transient scratch/storage, violating a principal purpose of this backend; even a successful replay may misdescribe the retained object.

Smallest correction: use the existing durable publication/call identity before reading or uploading scratch; matching arguments return the stored receipt kind, conflicting identity refuses. Preserve current fencing for new acceptance and do not introduce another upload queue or mutable manifest. A source-independent committed-receipt lookup is sufficient for this finding; a new filesystem abstraction is not.

Focused proof: commit a file; replace it with a directory and replay the same call, then remove it and replay from a fresh workspace/current lease. Each replay returns the original file receipt and one stored manifest without new File uploads; conflicting arguments still refuse.

### BEH-002 — MISSING: publication acceptance has no effective retained-resource accounting

Obligation: task Execution records explicitly retain cumulative budgets; publication supplement explicitly enforces effective per-file/tree/JSON, publication count, cumulative retained bytes, staging and deadline limits. These are Task 1 backend obligations; Task 3 exposing tools cannot make unbounded backend acceptance correct.

Locations: `publications.rs:80–106,122–149,201–241`; `hosted/mod.rs::HostedLimits`; `workspace.rs:577–620`; `verifier_executions.rs:690–712`; publication migration.

Producer-to-consumer evidence: each `json` call checks only its individual serialization, each `path` checks current scratch, and the SQL writer inserts a new producer/name row without count/retained-byte/deadline accounting. Publishing successive distinct logical names or rewriting and republishing one bounded workspace file can retain content repeatedly without any cumulative ceiling. The per-file ceiling is also inconsistent: default `work_write_max_bytes` is 50 MiB, so freeze accepts a 21 MiB file, while `TenantEvidence::published` → `file_object` rejects it against default `file_max_bytes` of 20 MiB. A successfully accepted named publication can therefore never mount for its consumer. Neither `accept_publication` nor the migration has a durable budget owner; a per-workspace write limit does not bound cumulative retained content. All backend producers use this owner, and inspection/collection consumers read these rows.

Consequence/materiality: backend callers can accumulate retained content beyond effective execution limits, and ordinary large accepted files are unusable for downstream evidence. Limits are an explicit execution/storage boundary, not speculative additional hardening.

Smallest correction: extend the existing execution/publication SQL owner with atomic effective-count/retained-byte accounting and the existing host limits, preserving it across attempts/reclaim and excluding same-call receipt replay from duplicate charging. Validate per-file size against the effective publication/consumer ceiling before completing storage, and enforce the execution deadline at new acceptance. No new budget service, queue or public authoring mechanism is needed. Per-call guards alone cannot preserve cumulative accounting across restart/concurrent calls.

Focused proof: small configured ceilings; repeated individually valid JSON/files cross the total/count limit and refuse without a new manifest; reclaim does not reset usage; receipt replay charges once; a file just above the consumer ceiling refuses before acceptance, and an accepted boundary-size file mounts successfully.

### BEH-003 — INCORRECT: publication inventory lag permanently spends consumer retries

Obligation: task scenario 8 and supplement Storage/history/inspection require bounded content-visibility waiting, released permits and no model attempt spent while waiting; INV-015 and AC-008/041.

Locations: `workspace.rs::Workspaces::open` (publication branches around 110–151), `Mount::publication:225–250`; `evidence.rs::published:138–144` and `file_object:265–302`; `engine.rs::attempt:830–871`.

Reachable path: an accepted file/tree publication has a durable manifest while its inventory projection is pending. A declared downstream Agent mounts it. Scope resolves the exact producer; workspace preparation asks `published`, which maps absent inventory to generic `BlockError::Retryable`. `HostedTools::open` propagates that error before `Agent::run_prompt`. The engine has already durably begun an attempt and holds its action permit. Generic retryable failures consume the step's retries; with default zero retries, the first visibility miss settles a permanently failed action. With retries, the immediate loop can exhaust them before projection catches up. This also affects requirement workspaces through the shared session setup. Metadata inspection's correct 503 behavior does not fix this consumer path.

Consequence/materiality: a normal accepted-content projection delay changes a valid queued workflow into a final failure despite no model call taking place. Later inventory visibility cannot revive the committed failed settlement.

Smallest correction: distinguish pending publication inventory at the existing evidence/workspace admission boundary and defer/retry that admission within the execution deadline without charging a dispatched model/action attempt. Reuse the engine's existing pre-dispatch deferral/forget behavior and release workspace/action permits while waiting; do not add an SDK polling loop, freshness mode or new scheduler. Preserve terminal digest/size validation failures.

Focused proof: accept a file while inventory visibility is held, start a zero-retry consumer, assert no provider call or spent attempt/final failure; restore visibility within the same deadline and obtain the expected mounted bytes and one real invocation. Deadline expiry must terminate boundedly.

### BEH-004 — INCORRECT: identity-sequence cursors do not freeze a committed listing cut

Obligation: task scenario 9 and public-contract stable pagination cut; `ExecutionPageQuery` explicitly promises only entries committed before the first page, in commit order.

Locations: `wyrd-sql/src/queries/verifier_executions/inspection.rs:19–48`; `verifier_executions.rs::BEGIN_INVOCATION_SQL` and `ACCEPT_PUBLICATION_SQL:690–703`; identity `sequence` columns in both migrations.

Reachable interleaving: publication seq1 is committed. Concurrent trusted publication calls allocate seq2 (transaction held open) and seq3 (committed). Distinct names do not conflict, and their execution `FOR SHARE` locks are compatible. First page limit1 sees seq1 and seq3, fixes through=3 and after=1. Commit seq2 after that first page. The next page selects seq2 because `1 < 2 <= 3`; it silently includes an entry that was not committed at the first cut. Invocation writers use the same identity/cut geometry. The workspace writer lock does not serialize this: freeze releases it before upload/acceptance, and callers can use different workspaces. Existing SQL page proof inserts and commits in order.

Consequence/materiality: supported concurrent publication/step production violates the explicit stable snapshot contract; consumers cannot distinguish a fixed listing from later changes. An identity sequence orders allocation, not commit.

Smallest correction: establish a monotonic execution-owned committed ordering at the existing SQL write boundary, coordinated with the first-page cut, rather than assuming global identity allocation is commit order. Reuse execution row coordination; no cursor service or long-lived read transaction is needed. Apply the same invariant to both lists, preserving indexed bounded reads.

Focused proof: hold the middle insertion transaction open, commit a later insertion, obtain the first page, then commit the middle one. Continuations must equal the first committed cut exactly; a fresh poll then discovers the new entry. Exercise invocation and publication paths or prove their shared ordering owner once with both callers covered.

### BEH-005 — MISSING: required-capability failure still follows the ordinary judged-action path

Obligation: task scenario 7 / REQ-014 / AC-025 require deletion of an exact Agent/Prompt before unfinished work to end the execution errored with retained checkpoints. Public-contract Execution authorization additionally requires internal engine definition loading under verifier:run without cards:read.

Locations: `hosted/blocks.rs:150–155` installs `TenantRegistry`; `hosted/registry.rs::spec/resolve/refusal`; `vala-verify/src/blocks/agent.rs::resolve/invoke`; `engine.rs::attempt:858–872`, `Attempted::settle:1289–1336`; runner `engine_failure` versus ordinary `EngineOutcome::Completed`.

Reachable path: checkpoint an action, delete the exact Agent or Prompt needed by an unfinished later action, and reclaim. The registry's public Card read returns a terminal BlockError. The action engine converts it to an ordinary failed StepResult; the engine returns an outcome and the runner completes the execution with a result, rather than terminating errored. A requirement using that capability becomes inconclusive instead. The same registry still authorizes cards:read, so a legitimate exact verifier:run caller without cards grants cannot load referenced definitions. Existing scenario 7 uses an injected EngineScript and includes `Permission::card_read()`, so it does not prove either obligation. Both actions and Agent checks use these resolvers; Artifact reads retain their distinct original-source authorization and must not be broadened as collateral correction.

Consequence/materiality: recovery confuses unavailable required execution capability with an observed verification result, and accepted verifier-only authority cannot execute supported referenced-Agent Verifiers. This is explicitly required task behavior, even though the untouched resolver was suitable for the earlier contract.

Smallest correction: extend the existing hosted internal definition resolution path under the accepted exact-Verifier execution authority, preserving tenant binding, exact identities, deletion checks and audit. Carry unavailable required-capability failures to the existing execution terminal-error boundary rather than ordinary action/check judgments. Do not grant general Card reads, substitute versions, or weaken public Cards/Artifact APIs. Amend the owning architecture description to reflect the approved internal-loader rule where it remains stale.

Focused proof: verifier:run-only caller executes a referenced Agent/Prompt successfully without explicit Card-fetch permission; delete each exact required definition after a predecessor checkpoint and recover, observing errored/unavailable, no newer version/provider dispatch, and unchanged retained checkpoint. Cover action and requirement consumers of the shared resolver.

### BEH-006 — MISSING: Python and TypeScript publication journeys do not meet explicit public-surface closure

Obligation: original task scenario 9 and Public surface closure require JSON/null/file/directory responses, authorized download plans and live cancellation through each SDK; root AGENTS requires the user journey on every shipping surface. Task 3 owns creating content through Agent tools, not proving these Task 1 read bindings.

Locations: `sdks/wyrd-sdk-python/tests/integration/test_durable_verifier_work.py::test_execution_pages_are_stable_and_tenant_scoped`; `sdks/wyrd-sdk-ts/wyrd/tests/integration/durable-verifier-work.test.ts`.

Evidence/consequence: both journeys wait for a terminal requirement-only execution and assert an empty publication list. Python never invokes download; TS invokes it only on a nonexistent publication. Neither decodes a real JSON/null or manifest, obtains/redeems a real plan, or cancels unfinished work. Rust and MCP content proof does not cross Python-object/N-API/JS conversion boundaries. The implementation evidence explicitly records the omission; no source proof establishes equivalent binding behavior on these required success paths.

Smallest correction: reuse the existing trusted `VerificationFixture::publish` seam through test-only fixtures/bindings or an existing shared server fixture, then run the ordinary public Python/TS reads/download/cancel against that server. Do not add a production arbitrary-path endpoint or wait for Agent tools to satisfy Task 1. The fixture should remain test-only and minimal; duplicating the publication implementation in either SDK would be wrong.

Focused proof: the two existing journeys exercise real JSON including null, file/directory manifests, exact producer selection, a usable download plan, viewer read/denied cancel and authorized unfinished cancellation, with public SDK exceptions/typed shapes asserted. Reuse Rust backend proof for storage correctness rather than copying every storage failure case into both languages.

## Limits and non-blocking note

No Cargo, Python, TS, format or codegen jobs were run by this reviewer; no source was changed. Recorded green checks support the covered paths but do not disprove the source-traced failures above. All-target CI evidence was not supplied; the matrix records that limit instead of claiming portable execution was proven. The review should not create a new implementation requirement solely from this missing CI artifact if the normal change-level matrix will supply it.

The CLI `execution` noun is generic, but help text explicitly identifies queued Verifier execution and its verbs route through the shared handle. A verification-specific parent may improve discoverability; naming alone is not a material failure and no rename is prescribed.

Skipped independent runtime reproduction and all-target CI; the retained risks are the six source-backed acceptance gaps above.
