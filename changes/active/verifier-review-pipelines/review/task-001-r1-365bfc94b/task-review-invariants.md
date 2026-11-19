# TASK-001 invariant review

Result: **FAIL**.

Subject: base `f4e79ea00523af5cd9212fdacde64f16c97ad554`, candidate `365bfc94bfef1a153f213462f7ef104afa823741`; first review of the full cumulative range. HEAD remained the candidate during inspection. No prior evidence was carried forward. This report covers action/requirement checkpoints, attempt and deadline flow, publication replay identity, collection state/lineage, and execution fences. Other reviewer conclusions were not consulted.

Authority read: root instructions; SQL AGENTS.md; task-review skill; `changes/spec-driven-development.md`; maintainer style; architecture authority map and the Verifier/workspace and coordination-clock sections of `architecture/wyrd-design.md`; approved revision-16 specification, original TASK-001, and `output-publication.md`. The review used CodeGraph before source navigation, then actual source and the recorded Git range. No source edits or Cargo jobs were performed.

## Expanded navigation and invariant ownership

- SQL `CLAIM_SQL` produces `remaining_ms` and preserves `deadline_at`; `ClaimedRow::into_claimed` places it in `ClaimedExecution.remaining`. `VerifierRunner::claim` then resolves every target in the batch before the claim transaction completes and `attempt`/`execute` begin engine work.
- `VerificationEngine::with_checkpoints` restores task-keyed action and requirement settlements and the maximum recorded attempt. `step`/`requirement` consult these before dispatch. `attempt` records each action attempt before executing it, counts interrupted attempts against retries, and forgets capacity refusals. `ExecutionCheckpoints` commits each write in its own tenant transaction.
- `Scope::resolve` constructs strict successful-attempt selectors and settled collection state; `Workspaces::open` reads persisted publications through `TenantEvidence`, mounts immutable content and serializes the collection manifest. The `Scope` sources, not a publication JSON value, determine original-source reachability.
- `Publications::{json,path}` produce arguments and content; `accept` hashes canonical arguments; SQL `ACCEPT_PUBLICATION_SQL` owns tuple/name uniqueness and same-call replay. `path` completes ordinary Files and inventory before reaching that SQL owner.
- `CANCEL_SQL`/principal cancellation move the execution out of running state; invocation/publication writes share-lock and require current token plus running status. Runner lease loss cancels execution futures. Token/state fences cover reclaim and cancellation; expired-but-not-reclaimed timing is separately identified below for concurrency review.

## Acceptance matrix

| Obligation | Implementation evidence | Verification evidence | Result |
|---|---|---|---|
| Action and requirement settlements recover independently; committed actions are not rerun | engine `with_checkpoints`, `restored`, `step`, `requirement`; runner `dispatch` loads SQL invocations and installs the shared engine checkpoint owner | Inspected `checkpoints_restore_settled_blocks_and_count_interrupted_attempts`; task records real SDK `action_checkpoint_survives_unfinished_requirements` and SQL checkpoint test passing | PASS |
| Attempts commit before dispatch; ambiguous unfinished action attempts consume existing retries; Busy does not consume an attempt | engine `attempt`, `begin`, `forget`; SQL BEGIN/FORGET statements; runner `ExecutionCheckpoints::fenced` | Engine recovery test checks only `action:late` executes after recovery, attempt 2 is retained, exhausted `stuck` fails without dispatch | PASS |
| Recovery keeps the database-owned remaining execution deadline, with no new action after timeout | SQL CLAIM preserves deadline, but runner reanchors the returned interval after target resolution | SQL test checks remaining interval after reclaim; it does not exercise time between SQL claim and runner engine start | **FAIL — INV-R2** |
| Publication names are unique by execution/producer indices/attempt/name; same-name new attempts remain separate | SQL publication unique index and ACCEPT statement; nullable indices bound from Producer | Inspected `publication_replay_preserves_attempt_identity` and its separate attempts/conflict/stale-token assertions; recorded passing | PASS |
| Identical original call/attempt recovers its original receipt after an ambiguous response, independently of later scratch changes | `Publications::path` freezes/uploads anew before replay lookup; `accept` returns newly computed kind even for Replayed | Replay test repeats unchanged source only | **FAIL — INV-R1** |
| File/tree acceptance does not expose a partial publication; accepted snapshots remain immutable | freeze produces owned bytes; `path` completes every File before atomic SQL manifest insertion; conflicts do not replace persisted manifest | Inspected file/JSON immutability and replay test failure injection; recorded directory snapshot proof | PASS |
| Strict name selects successful attempt; missing name prevents consumer execution | Scope selector uses `result.attempts` only for Succeeded; workspace lookup requires exact producer/name before materialization | Engine publication-reference tests; hosted mount test verifies missing-name refusal and successful-attempt placement | PASS |
| Collection manifests preserve failed/skipped status, attempt attribution and partial content; generated data cannot supply source grants | Scope `collection`; workspace `CallValue::Publications` builds content paths and an unprivileged manifest; TenantEvidence citation filtering checks reachable pinned slots | Inspected `publication_mounts_preserve_status_and_lineage` assertions for failed/skipped status, partial bytes and source slots | PASS |
| Cancel/reclaim fences later checkpoints/publications and preserves accepted records | SQL running/token predicates and row locks; runner lost-lease branch; cancel does not delete checkpoint/publication tables | Inspected stale-reclaim publication and checkpoint assertions; recorded `cancelled_work_keeps_checkpoints_and_fences_stale_writes` | PASS for cancel/reclaim; expiry timing delegated |
| Standalone code has no publication tool; Task 3 owns Agent tool exposure | Backend is trusted hosted `Publications`; hosted Blocks routes existing action kinds without adding publish tools | Static source inspection | PASS |

Verification evidence above is source inspection plus explicitly identified recorded results, not independently rerun tests. Full model/token/resource-limit expansion belongs to later task scheduling work; this report does not claim that cumulative accounting beyond the implemented plain-step attempts and deadline has been proven.

## Material proposed findings

### INV-R1 — INCORRECT: committed path publication replay depends on mutable scratch and returns a newly computed receipt

**Obligation:** TASK-001 scenario 5 and `output-publication.md` require an identical original tool call with its original attempt identity to recover its original receipt. Accepted snapshots survive later workspace changes.

**Location:** `crates/wyrd/wyrd-server/src/verification/hosted/publications.rs:122`–149 and `:241`; SQL replay authority is `crates/wyrd/wyrd-sql/src/queries/verifier_executions.rs:690`–712.

**Producer-to-consumer proof:** first publish `/work/report` as a file under call `c1`, committing kind=file and immutable manifest. Its response is lost. Later scratch is removed or replaced; overwriting scratch is explicitly supported. Replaying `c1` with the identical name/path and attempt first invokes `workspace.freeze`, and either fails because the path is gone or uploads its replacement. Replacing the file with a directory reaches SQL with unchanged canonical arguments; SQL recognizes the original call and leaves the original file manifest intact, while `accept` returns the new local kind=directory on `Replayed`. Thus the retry either cannot recover or reports a receipt inconsistent with authorized inspection. A storage/inventory outage after the original commit similarly prevents a replay that should only recover its receipt. JSON replay shares `accept`, although identical JSON arguments keep its kind stable.

**Materiality:** response loss and later scratch mutation are explicit supported scenarios, and the durable receipt is the caller's recovery boundary. This is not a speculative invalid input. Existing tests only replay an unchanged file, so they miss the failing boundary.

**Smallest correction:** reuse the existing SQL publication identity owner to recover an already committed same-call/same-arguments publication under the execution fence before new freezing/uploading, returning its persisted kind. Preserve conflict and stale-lease refusals and atomic first acceptance. Do not add a second upload service or mutate the original manifest. Recovery cannot be implemented merely by changing the returned kind, because missing scratch and dependency failure must also stop affecting an already committed receipt.

**Focused closure proof:** publish a file, lose/ignore its reply, remove or replace scratch with a directory, replay identical call/name/path/attempt, and assert the original file receipt and original bytes/manifest with one accepted entry. Repeating under stale lease must still refuse; another call with the same name must still conflict.

### INV-R2 — INCORRECT: target loading replenishes the durable deadline interval

**Obligation:** TASK-001 scenario 2 requires recovery using the remaining deadline; scenario 7 requires no new call after timeout. REQ-014 and the architecture coordination-clock section require the persisted database deadline to govern recovery.

**Location:** `crates/wyrd/wyrd-server/src/verification/runner.rs:404`, producer path `:798`–844; `crates/wyrd/wyrd-sql/src/queries/verifier_executions.rs:457`–474.

**Producer-to-consumer proof:** CLAIM computes `remaining_ms = deadline_at - statement_timestamp()` and Rust decodes that interval without an anchor. Runner claim then sequentially awaits target resolution for every claimed execution before the batch is processed. `execute` finally constructs `Instant::now() + run.remaining`. Consequently all target-resolution, commit and scheduling time after CLAIM is added back. On recovery with a short remaining interval, target lookup taking longer than that interval still permits unfinished action dispatch and successful checkpoints after the persisted deadline has passed. Normal slow database reads or a batch with several resolutions suffice; no clock skew is needed. The engine correctly observes its supplied Instant, but that Instant represents the replenished interval.

**Materiality:** conserving deadlines across interruption is an explicit task outcome. The near-expiry recovery case is routine for retries with backoff and can start costly provider work that was already out of budget. The SQL proof only checks that reclaim returns zero/unchanged remaining time and cannot prove the later handoff.

**Smallest correction:** preserve one in-process deadline anchored to the database remaining-time observation through load and dispatch, or obtain a fresh database remaining interval before dispatch. Reuse the existing claim timing/Instant mechanism and preserve PostgreSQL authority; do not compare wall clocks or introduce a clock abstraction. Target loading must consume the same remaining budget rather than grant it again.

**Focused closure proof:** create/reclaim an execution with a small remaining database budget, delay target loading beyond it, and assert timeout with no action/provider dispatch; verify a shorter load allows only the residual budget. Keep existing claim/reclaim and synchronous-direct-verification behavior unchanged.

## Verification limits and separately routed boundary

No Cargo, Postgres or platform jobs were rerun, as instructed. Source and tests establish the two failures without an external service. Snapshot path hardening, SQL concurrency, sensitive authorization and all public surface parity remain with their selected reviewers.

The orchestrator was notified that BEGIN/SETTLE/ACCEPT require matching token and running state but do not test lease expiration itself. Expired-but-not-yet-reclaimed writes need reconciliation with the concurrency review; this report neither treats token checks as proof of expiry fencing nor retains an unvalidated duplicate finding.
