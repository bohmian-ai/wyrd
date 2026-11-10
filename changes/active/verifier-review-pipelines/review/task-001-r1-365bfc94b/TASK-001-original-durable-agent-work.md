---
id: TASK-001
spec: SPEC-verifier-review-pipelines
spec_revision: 16
status: superseded
depends_on: []
req: [REQ-010, REQ-011, REQ-012, REQ-013, REQ-014, REQ-015, REQ-031, REQ-032, REQ-046, REQ-047, REQ-048, REQ-049]
inv: [INV-002, INV-004, INV-005, INV-009, INV-010, INV-011, INV-015]
ac: [AC-005, AC-006, AC-007, AC-008, AC-009, AC-024, AC-025, AC-039, AC-040, AC-041]
---

# Durable execution, publications and inspection

> Historical Task 001 scope (revision 16), reported implemented. Do not re-execute
> its PostgreSQL state architecture. [Current Task 001](../../tasks/TASK-001-bifrost-execution-state.md)
> refactors the implemented foundation under approved revision 18; immutable
> review evidence and valid original capability contracts are preserved.

## Outcome and Value

Implement the durable backend that remembers accepted work, stores immutable publications and restores interrupted executions. Task 3 alone exposes fs.write/publish as Agent tools; extraction uses internal engine retention, and standalone code actions have no direct publication interface; this task does not implement fs.write or the publish tool.

## What Changes and What Must Stay True

| Implement | Required implementation logic |
|---|---|
| **Execution records** | Extend the existing execution SQL/runner to retain pinned declarations and authority, logical invocation identity (execution, step, nullable item/iteration), one-based attempts, separate action/requirement settlements, cumulative budgets and deadline. Commit identities before dispatch; restore only unfinished work. Task 2 adds expansion/iteration/guard transitions to this same owner. |
| **Durable admission and completion** | Extend the existing batched submission/completion handoffs so acknowledgement follows recoverable admission or final commit. Matching idempotency replay returns the original execution; conflicting replay refuses. Keep direct verify bounded and synchronous, with no background continuation. |
| **Publication backend** | Accept invocation-bound file/tree/JSON sources from trusted hosted callers. Freeze a workspace source under its writer lock, release writers after freezing, complete immutable File content, then accept its manifest and projection intent under the current execution fence before returning a receipt. Upload/checkpoint failure exposes no partial publication. Large bytes stay outside SQL. |
| **Publication identity and mounts** | Enforce uniqueness by execution/step/item/iteration/attempt/name. Original tool-call identity plus identical arguments recovers its receipt; conflicting replay or another call reusing that attempt's name refuses. A fresh attempt may reuse the name without merging earlier work. Resolve output, strict named publications and settled collection manifests using the supplement's exact status/path/provenance contract. |
| **Read/cancel authorization** | Implement [public contracts](../../public-contracts.md): verifier:read scoped to exact Verifier/all for inspection and verifier:run for cancel. Repository reads/cancel additionally need repository read/run. No cards:read fallback or inspector role; discarded evidence has normal read authorization but cannot enter downstream mounts. Dedicated reads use fixed scoped internal queries; public Bifrost queries keep table grants. Update owning authority, built-in grants, scope validation and every projection. |
| **Execution inspection APIs** | Implement the method/path/request/response table in [public contracts](../../public-contracts.md): existing execution-state read, new invocation/publication lists, producer-selected publication read/download and durable cancel. Reuse the producer tuple plus logical name, immutable manifest, File download plan and items/next_cursor envelope; do not invent output IDs, report schemas or a second manifest. |
| **History and public reads** | Add execution_events to canonical Bifrost tables through existing Scribe capture/outbox; commit replayable intent with each transition and reuse stable event/sequence/batch identities. Preserve execution_outputs. Extend VerificationControl/shared client and all surfaces with paginated invocation/publication inspection, authorized reads/downloads and cancellation; Bifrost queries stay on existing Bifrost interfaces. |

Backend proofs use trusted hosted calls with real storage/SQL; they do not introduce a public arbitrary-path publication endpoint. Agent-to-tool-to-backend proof belongs to Task 3. New backlog-sensitive checkpoint/page queries must have tenant-scoped indexes and bounded results; preserve the existing due/leased claim-index geometry. No database transaction spans storage or model IO.

## Approach

1. Extend execution/checkpoint contracts and their owning architecture sections.
2. Implement durable admission, fenced settlements, restoration and cancellation in existing SQL/runner owners.
3. Implement immutable snapshot/JSON acceptance, receipt replay and typed handoff using current workspace/File owners.
4. Add canonical event projection and paginated authorized execution APIs; project thinly into SDKs/CLI/MCP.
5. Prove commit boundaries, recovery, lag and indexed access before Task 3 adds Agent tool dispatch.

## Ordered Implementation Scenarios

### 1. Acknowledge only durable admission

**Behavior.** Submit an ordinary existing Verifier; lose the response after acceptance, restart and replay the same key. Return the same execution. Fail admission persistence: no success acknowledgement; conflicting payload under the key refuses.

**RED.** Assert one recoverable execution and one accepted identity after restart; current staged acceptance can acknowledge before that durable boundary. Expected RED: queued acceptance can be acknowledged before recoverable admission. Run [proof 1](#proof-1); names are planned until added/listed.

**GREEN.** Make the existing submission handoff wait for recoverable acceptance, preserving batched writes and source acknowledgement ordering.

**REFACTOR.** Keep one idempotency/admission implementation for HTTP and MCP. Keep this and earlier scenario tests green.

### 2. Restore action and requirements separately

**Behavior.** Crash after an action commits but before its requirements, and after final commit before response. Recovery uses exact pinned declarations and remaining attempts/deadline, does not repeat committed calls, and returns the committed final result.

**RED.** Count provider/action calls and inspect retained outputs across recovery; rerunning a committed action or replenishing budget fails. Expected RED: action and requirement progress is not durably restored independently. Run [proof 2](#proof-2); names are planned until added/listed.

**GREEN.** Persist action and requirement settlements separately and restore them through the existing leased runner.

**REFACTOR.** Share plain-step checkpoint logic with the engine rather than a recovery executor. Keep this and earlier scenario tests green.

### 3. Accept file and JSON publications

**Behavior.** Call the hosted backend with a file, bounded JSON object and explicit JSON null. Read stored content/digests, mutate scratch and read again; accepted content remains identical. Oversized content refuses without an accepted entry.

**RED.** Use real File storage and Postgres; assert exact bytes/JSON, producer identity and no manifest on refusal. No backend acceptance exists yet. Expected RED: there is no invocation-bound named publication acceptance/manifest. Run [proof 3](#proof-3); names are planned until added/listed.

**GREEN.** Implement typed file/JSON acceptance, integrity/size checks and fenced manifest commit through File completion.

**REFACTOR.** Reuse existing File lifecycle and JSON size accounting. Keep this and earlier scenario tests green.

### 4. Snapshot a complete directory

**Behavior.** Snapshot nested Unicode/space paths and empty directories. Reject traversal, absolute manifest paths, backslashes, controls, symlinks, special/non-UTF8 paths, file/directory or NFC+casefold collisions, unreadable entries and file/count/depth/byte limits. No partial tree is accepted.

**RED.** Table-test each rejected tree and compare every accepted relative path/byte. Hold a writer while snapshot starts; assert a consistent frozen result. Expected RED: the existing walker is best-effort rather than an atomic complete snapshot. Run [proof 4](#proof-4); names are planned until added/listed.

**GREEN.** Add strict writer-excluded snapshotting at the current workspace owner, then upload frozen content; do not publish the truncating best-effort walker.

**REFACTOR.** Reuse confinement and writer locking without changing Artifact path grammar. Keep this and earlier scenario tests green.

### 5. Recover publication acceptance

**Behavior.** Lose the backend reply after commit; retry identical original call/attempt and recover one receipt. Another call with that name conflicts; a new attempt can publish the same name with separate attribution. Interrupt upload/checkpoint or replay a stale lease: nothing partial or late becomes readable.

**RED.** Inject faults at storage completion and fenced manifest commit; assert receipt identity, entry counts, both attempt records and absence of stale entries. Expected RED: accepted publication calls have no durable receipt replay identity. Run [proof 5](#proof-5); names are planned until added/listed.

**GREEN.** Persist publication reservation/call identity, arguments identity and receipt; reconcile File completion and check the execution fence at acceptance.

**REFACTOR.** Reuse upload reconciliation and SQL uniqueness instead of another upload queue. Keep this and earlier scenario tests green.

### 6. Resolve retained evidence

**Behavior.** Resolve a strict name and a complete collection through the hosted evidence boundary. Missing names block consumers; skipped and failed producers yield honest collection status and committed partial work. Manifest paths determine file placement; generated opinions cannot forge original-source citations.

**RED.** Assert mounted bytes/paths and source slots, failed/skipped reasons and missing-name refusal before consumer execution. Expected RED: publication references and settled retained manifests do not exist. Run [proof 6](#proof-6); names are planned until added/listed.

**GREEN.** Add typed publication references and immutable collection assembly to existing Scope/evidence/materialization owners. Task 2 adds expanded/speculative filtering.

**REFACTOR.** Keep reference authorization at the evidence owner, not in model-returned JSON. Keep this and earlier scenario tests green.

### 7. Fence cancelled or unavailable work

**Behavior.** Cancel/time out after a checkpoint: no new call or late commit, accepted work survives. Delete a required exact Card: unfinished work errors without version substitution. Revoke accepted capability: active work cancels. Incompatible workers cannot claim the record.

**RED.** Assert terminal reason, unchanged checkpoint and rejected stale writes for each termination case; direct verify never secretly resumes. Expected RED: checkpoint fencing/availability cannot preserve the new publication contract. Run [proof 7](#proof-7); names are planned until added/listed.

**GREEN.** Extend current cancellation, availability, worker-version and lease checks without widening accepted authority.

**REFACTOR.** Reuse the same capability checks during normal execution and recovery. Keep this and earlier scenario tests green.

### 8. Project history without blocking inspection

**Behavior.** Pause Scribe projection/File inventory, commit work and inspect live progress. Metadata remains available; content lag is typed/bounded and spends no model attempt. Resume projection: query exactly the same event identities and bounded authored JSON fields; audit failure alone does not block.

**RED.** Use a role-separated cluster and real Bifrost queries; assert live state, replay dedup, native Variant values and unchanged completed output-row meaning. Expected RED: execution_events and its replayable projection are absent. Run [proof 8](#proof-8); names are planned until added/listed.

**GREEN.** Commit transition plus projection intent; replay through existing Scribe outbox with stable event/batch identities and explicit projection position.

**REFACTOR.** Reuse canonical registration, capture allowlists and Oracle cuts. Keep this and earlier scenario tests green.

### 9. Inspect and cancel through public surfaces

**Behavior.** List invocations/publications at a stable page cut while more entries commit, then poll a new cut. No gaps, duplicates, foreign entries or unauthorized content. Test an exact verifier:read grant without cards/Bifrost query grants: dedicated progress/publication/download reads succeed, including discarded inspection; another Verifier scope and run-only callers cannot read. Read-only callers cannot cancel. Cancellation uses exact verifier:run (plus repository run), is idempotent and does not rewrite terminal results. Explicit Card fetching still requires cards:read; arbitrary Bifrost SQL still requires table grants. Denied cancellation leaves work unchanged.

**RED.** Drive every execution-inspection method/path in the public contract through SDK/server APIs and assert typed bodies/status codes: invocation/status pages during projection lag; publication list without body reads; exact step/item/iteration/attempt/name selection; JSON including null; file/directory manifests and authorized File download plans; invalid selectors, absent/foreign resources and denied access; durable cancel and terminal/repeated cancel. Assert page identities/order and exact permission/visibility errors and 200 cancellation responses. Repeat this journey on Python/TS/CLI/MCP. Expected RED: the new invocation/publication pages and cancel surface are absent. Run [proof 9](#proof-9); names are planned until added/listed.

**GREEN.** Expose typed pages/read/download/cancel through VerificationControl and shared wyrd-client, then thin projections.

**REFACTOR.** Keep pagination and permission logic server-owned. Keep this and earlier scenario tests green.

## Acceptance Criteria

| Obligation | Required proof |
|---|---|
| REQ-010–012/031/046–047; INV-004/005 | File/JSON, directory, replay and mount scenarios prove immutable all-or-nothing acceptance, attempt scope, missing names and original-source lineage; Task 3 proves Agent-facing calls. |
| REQ-013–015; INV-003/009/011; AC-006–009/024–025 | Admission, checkpoint and cancellation scenarios prove acknowledgement, pinning, recovery, conservative ambiguous-call accounting, version checks and fencing. |
| REQ-048–049/032; INV-002/015; AC-005/041 | History and public-page scenarios prove live/historical identities, typed lag, source citations and equivalent read/cancel permissions. |
| INV-010; AC-039–040 contribution | Snapshot/replay cases cover referenced content and portable paths. Run changed capabilities on all supported CI targets. |

## Implementation Reference

| Inspected owner / consumers | Reuse and precise gap |
|---|---|
| Workspace::writer/materialize; workspace.rs::walkdir; hosted workspace tests | Reuse locking/confinement/materialization. Existing walker truncates/skips: add atomic snapshot validation at this owner, not a second filesystem stack. |
| wyrd-storage::file_upload_init/upload_complete/reconcile_upload/download_init; pg_hosted_files | Reuse non-Card File lifecycle. Add publication acceptance and stored-content linkage; no Artifact Card intermediary or blob-store duplicate. |
| VerifierExecutionQueue::claim/renew/complete/complete_many/release; runner/leases/submissions/completions | Reuse indexed claims, fences and batched handoffs; add durable invocation/action/requirement/publication records. Existing in-memory RunLedger cannot recover these. |
| VerificationControl; wyrd-client::verify::Execution; MCP verification; SDK verify_in_real_time | Extend the shared execution surface. Public checkpoint/page/cancel journeys use the existing SDK integration target; hosted publication seams use server integration tests before tools exist. |
| tables/verification; scribe_outbox; verification_runtime cluster journeys | Add execution_events and restricted capture registration to existing Scribe/Oracle/Forge. No direct Iceberg writes or stronger audit acknowledgement. |

## Verification and Evidence

All new names below are **planned**. Existing packages, targets, runners and Postgres setup are listed in [shared verification](../../tasks/verification.md). After adding a test, list/confirm its actual selector before RED; zero selected tests is not proof. The RED paragraph states the missing assertion, not a claim that a test has run. Use focused owner tests plus real public journeys; no credentialed provider or synthetic host load.

### Proof 1

```bash
mise exec -- scripts/postgres/with-test-postgres.sh -- bash -lc 'mise run db:migrate:all:inner && cargo nextest run --locked -p wyrd-ai --test integration -P journey --run-ignored=all --test-threads=1 -E "test(=durable_verifier_work::queued_acceptance_survives_response_loss)"'
```

### Proof 2

```bash
mise exec -- scripts/postgres/with-test-postgres.sh -- bash -lc 'mise run db:migrate:all:inner && cargo nextest run --locked -p wyrd-testing --test server -P journey --run-ignored=all --test-threads=1 -E "test(=verification_runtime::action_checkpoint_survives_unfinished_requirements)"'
```

### Proof 3

```bash
mise exec -- scripts/postgres/with-test-postgres.sh -- bash -lc 'mise run db:migrate:all:inner && cargo nextest run --locked -p wyrd-server --features test-support --test integration --test-threads=1 -E "test(=pg_durable_verifier_work::file_and_json_publications_are_immutable)"'
```

### Proof 4

```bash
mise exec -- scripts/postgres/with-test-postgres.sh -- bash -lc 'mise run db:migrate:all:inner && cargo nextest run --locked -p wyrd-server --features test-support --test integration --test-threads=1 -E "test(=pg_durable_verifier_work::directory_snapshot_is_complete_or_rejected)"'
```

### Proof 5

```bash
mise exec -- scripts/postgres/with-test-postgres.sh -- bash -lc 'mise run db:migrate:all:inner && cargo nextest run --locked -p wyrd-server --features test-support --test integration --test-threads=1 -E "test(=pg_durable_verifier_work::publication_replay_preserves_attempt_identity)"'
```

### Proof 6

```bash
mise exec -- scripts/postgres/with-test-postgres.sh -- bash -lc 'mise run db:migrate:all:inner && cargo nextest run --locked -p wyrd-server --features test-support --test integration --test-threads=1 -E "test(=pg_durable_verifier_work::publication_mounts_preserve_status_and_lineage)"'
```

### Proof 7

```bash
mise exec -- scripts/postgres/with-test-postgres.sh -- bash -lc 'mise run db:migrate:all:inner && cargo nextest run --locked -p wyrd-testing --test server -P journey --run-ignored=all --test-threads=1 -E "test(=verification_runtime::cancel_and_revocation_fence_unfinished_work)"'
```

### Proof 8

```bash
mise exec -- scripts/postgres/with-test-postgres.sh -- bash -lc 'mise run db:migrate:all:inner && cargo nextest run --locked -p wyrd-testing --test server -P journey --run-ignored=all --test-threads=1 -E "test(=verification_runtime::events_replay_without_hiding_live_progress)"'
```

### Proof 9

```bash
mise exec -- scripts/postgres/with-test-postgres.sh -- bash -lc 'mise run db:migrate:all:inner && cargo nextest run --locked -p wyrd-ai --test integration -P journey --run-ignored=all --test-threads=1 -E "test(=durable_verifier_work::execution_pages_are_stable_and_tenant_scoped)"'
```

### Public surface closure

Mirror all execution-inspection operations in the public-contract API table, including producer selectors, JSON/file/directory response shapes, progress/page/read/cancel and denied/cross-tenant/visibility cases. Task 3 additionally proves the publication content is created through real Agent tools. The Rust journey above covers the same server contract; add HTTP/OpenAPI assertions at existing route tests and CLI assertions through its in-process command surface. Planned language/MCP selectors:

```bash
mise exec -- scripts/postgres/with-test-postgres.sh -- bash -lc 'mise run db:migrate:all:inner && cd sdks/wyrd-sdk-python && uv run python -m pytest -q -m integration tests/integration/test_durable_verifier_work.py::test_execution_pages_are_stable_and_tenant_scoped'
mise exec -- scripts/postgres/with-test-postgres.sh -- bash -lc 'mise run db:migrate:all:inner && cd sdks/wyrd-sdk-ts/wyrd && pnpm exec vitest run tests/integration/durable-verifier-work.test.ts -t "^execution_pages_are_stable_and_tenant_scoped$"'
mise exec -- scripts/postgres/with-test-postgres.sh -- bash -lc 'mise run db:migrate:all:inner && cargo nextest run --locked -p wyrd-mcp --test mcp -P journey --run-ignored=all --test-threads=1 -E "test(=verification::pg_tests::durable_verifier_work_execution_pages_are_stable_and_tenant_scoped)"'
```

Apply only the [shared owner checks](../../tasks/verification.md#owner-checks) for this write set; record real command/results during implementation.

## Material Stop Conditions

Changing acknowledgement, retention, accepted authority or the existing Card-subject behavior requires authority review. Do not add a queue, graph store, analytical writer or report schema.

## Authority Links

[Shared authorization and wire contracts](../../public-contracts.md).

[Approved spec, revision 16](../../spec.md) · [Task ownership/dependencies](../../tasks/README.md) · [Publication contract](../../output-publication.md) · [PR extraction](../../pull-request-extraction.md) · [Orchestration](../../example-orchestration.md).

[Wyrd design](../../../../../architecture/wyrd-design.md) · [Bifrost design](../../../../../architecture/bifrost-design.md) · [Security](../../../../../architecture/wyrd-security-posture.md) · [Implementation execution](../../../../implementation-execution.md) · [Testing](../../../../../TESTING.md). Apply nested AGENTS.md at each implementation owner.

## Implementation Evidence

Commits on `code-review-workflow`: f36150646, dda7949d5, cae9abf63, 2f1a1a636, 3be739955, 186c5a525, d8574f480, 949271ad5, 214630739, 40052c2fd, ae1cb87cf, a2e6f552e, a171c3a61, 2cb4aa57e, 365bfc94b. Proofs 1–8 were re-run on a171c3a61's tree and proof 9 and the MCP journey on 365bfc94b's, with repository-managed Postgres.

| Acceptance criterion | Implementation evidence | Verification evidence | Result |
|---|---|---|---|
| Scenario 1: acknowledged only after durable admission; replay survives response loss and restart | f36150646 | Proof 1 `durable_verifier_work::queued_acceptance_survives_response_loss` | PASS |
| Scenario 2: action/requirement checkpoints restore settled work and count interrupted attempts | dda7949d5 | Proof 2 (corrected: `-p wyrd-ai --test integration`, test `durable_verifier_work::action_checkpoint_survives_unfinished_requirements`; the task text names `wyrd-testing --test server`, which selects zero tests); `pg_verifier_executions::invocation_checkpoints_are_fenced_and_survive_reclaim` | PASS |
| Scenario 3: file/JSON publications are immutable | cae9abf63 | Proof 3 `pg_durable_verifier_work::file_and_json_publications_are_immutable` | PASS |
| Scenario 4: a directory snapshot is complete or rejected | 2f1a1a636 | Proof 4 | PASS |
| Scenarios 5–6: replay identity pinned per attempt; mounts with honest status and producer-only lineage | 3be739955, 186c5a525 | Proofs 5, 6 | PASS |
| Scenario 7: durable cancel, revocation, newer-format refusal | d8574f480 | Proof 7 | PASS |
| Scenario 8: replayable history projected to Bifrost | 949271ad5 | Proof 8 | PASS |
| Scenario 9: pages, exact-attempt reads, download plans, cancel under `verifier:read`/`verifier:run` (HTTP, Rust) | 214630739, 40052c2fd: `wyrd-server/src/components/verification/service/inspection.rs`, `wyrd-sql/src/queries/verifier_executions/inspection.rs`, migration `20261105000000` | Proof 9 `durable_verifier_work::execution_pages_are_stable_and_tenant_scoped`; `pg_verifier_executions::invocation_pages_keep_their_first_cut`; `pg_openapi_contract::verification_contract_publishes_eleven_typed_operations_and_the_local_upload`; `pg_verification_routes::*` | PASS |
| Scenario 9: local-backend downloads redeem under `verifier:read` alone | 365bfc94b: `GET .../publications/{step_id}/{name}/content` (`routes.rs::publication_content`, `inspection.rs::{download,content,file}`); local plans point there; `wyrd_storage::service::download_local_object`; client local dispatch keyed on root-relative plans (`storage/download/mod.rs::authenticated`) | `pg_verification_routes::local_publication_downloads_redeem_under_verifier_read` (exact-Verifier grant fetches the exact file and directory bytes; another Verifier's grant and `cards:read` alone get 403); `wyrd-client` full suite (326); `declare_and_pull::*` (15) | PASS |
| Scenario 9 on MCP | ae1cb87cf | `verification::pg_tests::durable_verifier_work_execution_pages_are_stable_and_tenant_scoped`; `mcp::tests::every_tool_schema_is_accepted_by_common_tool_callers` | PASS |
| Scenario 9 on Python | a2e6f552e | `test_durable_verifier_work.py::test_execution_pages_are_stable_and_tenant_scoped` | PASS (limited, see below) |
| Scenario 9 on TypeScript | a171c3a61 | `durable-verifier-work.test.ts -t execution_pages_are_stable_and_tenant_scoped` | PASS (limited, see below) |
| Scenario 9 on CLI | 2cb4aa57e: `wyrd-cli/src/execution.rs` (`wyrd execution status/invocations/publications/publication/download/cancel`; in-process `wyrd_ai::cli::execution`) | Proof 9 drives the in-process command: listing, directory read, download plan, limit refusal, viewer cancel refusal, repeated cancel, status; `wyrd-cli --lib` (command tree consistency, no secret arguments) | PASS |
| Shared checks | — | `mise run fmt`, `lints`, `codegen:check`, `check:tenant-isolation`, `check:deps`, `docs:generate` (no drift); `py:lints`, `py:typecheck`; `ts:typecheck`, `ts:lints` | PASS |

Limits:

- **Python and TypeScript journeys cannot seed publications.** Only the Rust `VerificationFixture::publish` seam can. Those journeys cover pages, validation, not-found, access and cancel against a settled execution. JSON (including null), file and directory content, download plans and live cancel are proven through the Rust and MCP journeys. Task 3's Agent tools will make content reachable from every SDK.
- **Non-goals.** No `fs.write` or publish tool was added, and no queue, graph store or report schema was introduced.

Decision recorded: the local backend serves publication bytes through a byte route scoped to the Verifier (chosen over signed local URLs and over keeping `cards:read`).

Status: **IMPLEMENTED**.
