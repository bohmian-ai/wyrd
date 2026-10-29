# TASK-002 behavior review

Result: **FAIL**. Two proposed material lifecycle regressions require independent validation.

## Immutable subject and authority

- Repository: `/Users/stevenforrester/Documents/GitHub/wyrd`.
- Base: `15f939fc917b22efa5e5e4d12fd1f97be807a0a7`.
- Candidate: `5ef2981e66068e8ee5f96d3be15000eea9cbd94f`; HEAD independently checked at that commit.
- First review of the complete three-dot range. No TASK-002 acceptance carried forward. No implementation source was edited.
- Original obligations were read from approved specification revision 2 and `tasks/TASK-002-shared-file-uploads.md` before using its separate Implementation Evidence as verification claims.
- Authority inspected: supplied root AGENTS; SQL/Python/TypeScript/docs nested AGENTS; `changes/spec-driven-development.md`; `agent_docs/{index,rust,errors,maintainer-style,agent-surfaces}.md`; Wyrd design Verifier upload/authorization section; Bifrost design server-internal inventory/outbox section; security posture internal storage query/peer boundaries. Scope and test command ownership follow the common `scope.md` and task's TESTING links.
- No `.codegraph/` exists, so ordinary source navigation applies. No other reviewer report or conclusion was read.

TASK-003 explicitly owns once-per-execution metadata reuse, canonical readback, File application-SQL elimination, and expanded direct/queued readiness proof. These are not treated as already completed by TASK-002. In particular, `ObjectInventory::rows` still looks up SYSTEM in SQL, and `TenantEvidence::file_object` maps known-deleted File records to readiness, but TASK-003 explicitly names both relevant outcomes. Neither is raised here as a newly added TASK-002 obligation. No production path currently deletes File objects; the new deletion test constructs that fact directly.

## Coverage and expanded navigation

The supplied map was expanded across these caller/consumer seams:

1. `File::pin` → `RunEvidence::pin` → evidence HTTP request → `VerificationControl::evidence` → `ObjectInventory::file_object` or `service::file_upload_init` → existing planner/backend session initialization.
2. Shared client `upload_artifact{,_with_progress}` → dispatcher/source streaming → part URL/local upload → persisted-owner authorization → completion → existing object inventory recording/ACK. Existing Artifact declaration and CLI pull/list callers remain on the same workflow.
3. `TenantEvidence::{read,file_object,stored,media,cited}` → inventory-recorded location → bounded byte/length/digest/format checks. Inspected moved real-server hosted File tests and unchanged Artifact sibling behavior.
4. New nullable ownership through SQL insert/find/decode, tenant path validation, pending replacement, completion replay, abort, failed completion, and **existing sweeper expiry/recovery**. This last sibling consumer was absent from the starting map and establishes BEH-002.
5. HTTP route removal/router/body-limit exemption, served OpenAPI assertion, MCP evidence descriptor/guide/journey and existing queued frame producer; Rust/Python/TypeScript journey additions and unchanged thin bindings; CLI Artifact regression.
6. Inventory row origin/digest normalization and query folding, unchanged table-scoped SYSTEM query authority and unchanged ACK delivery owner. No new append worker, backend-specific SDK loop, queue, cache, checksum pass, or second inventory was introduced.

The new `UploadSubject` and generalized initialization reuse the existing lifecycle rather than creating a parallel transfer implementation. `ObjectInventory::rows` extracts the existing query path; `file_fold` supplies File-specific selection. Neither abstraction is unearned machinery. The behavioral problems below concern how existing Card assumptions reach the new File owner, not a preference for different organization.

## Acceptance matrix

`PASS` below is bounded to this task's implementation and exercised evidence, not final change acceptance of TASK-003 work.

| Requirement, criterion, constraint, or non-goal | Implementation evidence | Verification evidence | Result |
|---|---|---|---|
| REQ-001 / AC-001: File uses existing initialization, planner, dispatcher and completion | `service.rs::file_upload_init` builds a Card-less subject and calls `init_upload`; `RunEvidence::pin` calls existing `upload_artifact`; protocols remain selected by `plan_upload` | V1; shared-client unit mock now asserts single-PUT and multipart completion; backend journey adds File completion and reuse | PASS |
| Preserve supported File authoring behavior when independent calls overlap; no unrelated regression from consolidation | New File initialization applies Card's destructive pending-session replacement to every same-tenant digest (`service.rs:269,812–834`) | Existing journeys are serial; deterministic overlap trace in BEH-001 is uncovered | FAIL |
| Shared lifecycle includes existing interruption/expiry recovery without regressing Card reclamation | File sessions have evidence paths, but `Sweeper::abort_one` still accepts only Card paths (`sweeper.rs:297`) | Existing `pg_sweeper` tests seed Card paths only; source-proven failure and bounded-queue starvation in BEH-002 | FAIL |
| REQ-003 / INV-004: preserve existing file/aggregate limits | Evidence admission checks per-file and aggregate Deployment limits before initializing any session; local route bounds body by admitted session size | V3 asserts both planning 413 cases, oversized body, wrong size and wrong digest; V1 transfers local body | PASS |
| REQ-003: preserve encryption, backend completion size and local digest validation; no new remote hashing | `upload_local_blob`, `verified_object_head` and `verify_object_head` remain shared checks; remote completion uses declared SHA metadata | V3 plus backend completion regressions; hosted tampering test retains digest refusal | PASS |
| REQ-010: completed, undeleted same-tenant File digest/size alone qualifies for reuse | `ObjectInventory::file_object` filters `origin = file_upload`, digest or deletion; `file_fold` matches size and excludes deleted paths | V2 covers raw uncompleted bytes, Card file/envelope origin, foreign tenant, size mismatch and deletion; V1 completed reuse | PASS |
| REQ-010: read recorded inventory location, never infer completion from HEAD/path | `TenantEvidence::read` calls new `file_object`; no direct digest-path construction remains in File read | V2 reads existing raw bytes without inventory as inconclusive; V1 consumes completed record; hosted Files and V4–V6 | PASS |
| REQ-010 / INV-004: bounded byte reads and length/digest/format checks remain | `TenantEvidence::stored` remains owner of exact size and digest checks; `read` retains format sniff | Moved `pg_hosted_files` preserves validation/media/extraction/citation assertions; V1 and SDK output assertions | PASS |
| REQ-013 / AC-006 task portion: absent inventory is bounded retryable readiness, no fallback | `file_object` returns `BlockError::Retryable` for no completed object/query errors; inventory query has deadline; direct execution uses existing engine mapping | V2 directly proves missing visible File fact cannot judge existing bytes; full controlled outage/queued proof explicitly belongs to TASK-003 | PASS |
| REQ-015 / INV-002 / AC-007: operation-level run admission; exact invocation remains exact | `authorize_evidence` admits any `Verifier/Run` grant; existing `authorize_run` unchanged | V3 exact-grant caller uploads/completes/verifies, different Verifier refused; V7 mirrors MCP | PASS |
| REQ-015: persisted owner, verified tenant, no arbitrary storage path authority | `upload_owner` performs tenant-scoped row lookup; `authorize_upload` chooses Card/File permission; `row_path` validates owner namespace | V3 Card upload refusal, malformed ID, foreign upload 404; tenant-path unit test | PASS |
| REQ-015: caller needs neither Card-write nor general Bifrost query grant | Thin adapter authorizes File operation then inventory reads under existing table-scoped SYSTEM authority | V3/V7 exact-Verifier role contains Card-read and exact run, no Card-write/query permission; Card init denied | PASS |
| REQ-015: permission checks retain their audit boundary | Shared route delegates File decisions to `authorize_evidence`; Card decisions retain `authorize_card_write`; outbox stage remains nonblocking | V3 asserts allowed and denied decisions for planning/local/part/completion/abort | PASS |
| REQ-007 / INV-003 / AC-003 File portion: same upload-only ACK guarantee | Completion calls existing `ObjectInventory::record`; its row-specific notification has bounded wait; ordinary verification still uses unchanged outbox staging | V1 completion/replay and inventory reuse; structural reuse of dependency's ACK owner, with no new waits in verification settlement | PASS |
| REQ-016: path and memory File ergonomics remain thin shared-client projections | Only `RunEvidence::pin` transfer call changes; File/ArtifactRef declaration shapes unchanged; Python/NAPI wrappers continue through shared Rust owner | V4/V5/V6 each verify path and bytes then confirm reuse; shared-client mock covers pinning/protocol completion | PASS |
| REQ-016: Artifact interfaces and output survive | Existing Card initialization carries `Some(card_uid)`; shared dispatcher/completion unchanged; Card inventory reader keeps existing fold | V8 declares/pulls artifact bytes and refuses Card-file-as-File reuse; backend journeys retain Artifact download | PASS |
| REQ-016: HTTP/MCP/CLI contracts agree; MCP scope supplements domain admission | `EvidenceUpload` adds upload ID; MCP descriptor/guide tells clients to complete shared route; same HTTP implementation behind MCP | V7 plans via MCP then transfers/completes shared HTTP and executes MCP; served OpenAPI assertion; CLI regression | PASS |
| REQ-016–017 / AC-008 File portion: obsolete local evidence path/contracts removed without alias | Evidence-only route, `upload_local_evidence`, `evidence_upload_plan`, and client `upload_evidence` removed; verification router retains five operations | Updated served OpenAPI path set; generator/check claims; source search confirms no replacement alias | PASS |
| REQ-017: fresh install coordinates Card-less File sessions and uses sole inventory | New migration drops session Card UID NOT NULL; SQL row/options and writes updated; no second completed metadata table | V1/V2/V3 use fresh migrated Postgres and Bifrost records; SQL Card regression adjusted to `Some` | PASS |
| Non-goals: no Card creation for File, physical Card/File sharing, retention/GC policy, migration/backfill, extra completion mode, checksum protocol, or queue redesign | Evidence namespace and `file_upload` origin stay separate from Card objects; owner changes are only coordination; existing inventory ACK mode reused | V2 and CLI reject cross-origin reuse; source/diff inspection | PASS |

## Failure and recovery assessment

- **Inventory outage/delayed visibility:** File planning propagates the inventory query failure instead of using HEAD; File execution returns retryable block failure instead of guessing the object path. Existing deadlines/engine behavior bound work. The controlled outage and full queued outcome matrix remain TASK-003 proof.
- **Inventory ACK lost/refused after transfer:** storage completion may already have committed its coordination state, then inventory recording returns the existing retryable writer-unavailable error. Replaying the same upload re-verifies and re-records through the same owner; there is no new durable repair promise. No new shared writer dependency on the HTTP caller was found.
- **Transfer/completion validation failure:** existing shared storage errors propagate; wrong local bytes are not written, terminal backend/head failures mark the upload failed, and execution's read checks still refuse corrupt content.
- **Request cancellation/process death after File initialization:** SQL coordination survives, but normal expiry recovery rejects every File path before abort/terminal update. This is BEH-002, including its effect on later Card reclamation.
- **Concurrent File calls:** shared identity is tenant+digest, broader than a single invocation; new initialization invalidates an earlier live upload merely because a second invocation requests the same uncompleted File. This is BEH-001. The server stays available but the first supported SDK operation fails before evaluation.

## Proposed findings

### BEH-001 — REGRESSION: planning an independent File call aborts another live call's upload

**Violated obligation:** original task's same SDK operation/preserved authoring behavior, REQ-001 shared upload consolidation and REQ-016 preserved File authoring surfaces; this is a supported-use regression, not a new concurrency service requirement.

**Exact changed location:** `crates/wyrd/wyrd-storage/src/service.rs:269` (`init_upload` calls `find_and_mark_prior_pending`) and `:812–834` (File-inclusive lookup then unconditional `mark_aborted`). New producer: `:202–226` (`file_upload_init`, no invocation/session identity or replay key). SQL lookup: `crates/wyrd/wyrd-sql/src/queries/storage/multipart_uploads.rs:396–446` matches `card_uid IS NOT DISTINCT FROM NULL`, relative path and digest, with no distinct-caller identity.

**Reachable producer-to-consumer trace:**

1. Two independent Rust/Python/TypeScript `verify(File(same bytes))` calls in one tenant begin before the File has completed. `RunEvidence::pin` deduplicates within each call, but not between calls; each sends its own evidence planning request.
2. Call A receives pending upload A. Call B's inventory query also finds no completed record, so it enters `file_upload_init` for the same digest-derived relative path.
3. B's locked initialization finds A pending, marks A aborted, commits, and aborts A's backend multipart handle when present. B receives its new upload B. This requires only A-plan, B-plan, A-transfer ordering, not a data race inside one function.
4. A's authenticated local PUT or part URL request loads A through `load_pending_upload` and returns `WYRD_STORAGE_409_UPLOAD_NOT_PENDING`; if its remote bytes already transferred, `upload_complete` returns the same error for `Aborted`. `upload_artifact` propagates this and `RunEvidence::pin` fails, so A never evaluates its File.

**Why material:** a dataset or common reference document used in parallel verification calls is ordinary supported use; the old deterministic digest PUT path did not revoke another invocation's transfer. The new behavior also wastes/invalidates live multipart transfers. No authority requires tenant-wide single active verification call per digest. Existing Card replacement is tied to explicit reinitialization of one Card/path; applying it to independent File invocations without distinct ownership is the cause. The current V2 intentionally replans unfinished bytes but never attempts to finish the previously returned plan, so its green result cannot refute this path.

**Sibling review:** inspected both Card and File initialization, SQL dedupe, part URL/local upload/completion/abort, shared-client completion, and per-call source deduplication. Card reinitialization semantics must remain intact; a client retry loop would hide the invalid server state and amplify cancellation, not fix it.

**Smallest testable correction boundary:** prevent a new independent File initialization from revoking an already returned live File upload, within the existing initialization/session owner. Keep Card pending-replacement and shared backend/planner/limits unchanged. Independent File sessions can coexist using the already existing session identity; no second inventory, digest cache, queue, or new lifetime/physical sharing model is needed. Do not merely add downstream retries. If choosing to reuse a live session instead, prove that existing multipart completion/abort ownership remains correct for independent callers; handing out one mutable session without that proof is not an equivalent small fix.

**Focused closure proof:** deterministically obtain A and B plans for identical same-tenant File bytes before either completes, then finish A and B through the existing local lifecycle and verify the File. Include the existing multipart backend lane if the correction changes its initialization/abort path. Assert Card re-init still uses its prior semantics. No synthetic load or new harness is required.

### BEH-002 — REGRESSION: expired File sessions never pass the existing sweeper's Card-only path validation

**Violated obligation:** shared existing lifecycle/recovery and preservation of Artifact behavior (REQ-001, REQ-016). This is a newly reachable regression in an existing consumer, not a request for new retention, garbage collection, or crash-safe inventory repair.

**Exact changed producer location:** `crates/wyrd/wyrd-storage/src/service.rs:202–226` produces persisted File sessions at `<tenant>/evidence/sha256/<digest>` and `:269–310` inserts/persists them in the existing upload table. Affected unchanged consumer: `crates/wyrd/wyrd-storage/src/sweeper.rs:297–298`, `Sweeper::abort_one`, calls Card-only `tenant_path::validate`. New `validate_evidence` at `tenant_path.rs:143` establishes the distinct supported File shape; the normal service was updated through `row_path`, while this sibling was missed.

**Reachable producer-to-consumer trace:**

1. A File planning request opens a persisted initiating/pending session. The client cancels, process dies, or a transfer is abandoned before completion/abort. No later replan of that digest is required.
2. Existing `admin::multipart_uploads::expired_uploads_batch` selects all expired pending/orphan initiating rows, including these File sessions. Its query orders earliest deadline first and applies the configured `LIMIT`.
3. `Sweeper::abort_one` validates the row with the Card-only path grammar and returns `TenantPathMismatch` before backend abort and before `mark_aborted_if_open`.
4. The caller logs the row error but leaves its persisted status and expiry unchanged. Every future tick selects the same File row and fails again, including after restart. Multipart uploads retain their backend resource handle without a successful abort.
5. Once enough older File rows fill one configured batch, all later expired Card sessions are excluded from discovery forever: the same oldest failing File rows occupy every limited batch. Thus abandoned File work can stop existing cross-tenant Card upload reclamation too.

**Why material:** cancellation and process interruption are normal upload recovery paths. The default sweeper is enabled and exists precisely to reclaim such coordination/backend resources. Indefinitely unabortable multipart sessions and deterministic starvation of unrelated existing reclamation are meaningful resource/lifecycle regressions. This does not require an unusual forged row, a new deletion policy, or a new background worker.

**Sibling review:** inspected shared service `row_path`, explicit upload abort, re-init abort, completed/failed transitions, sweeper discovery and per-row terminal update, and existing `pg_sweeper` tests. The existing tests exercise Card-shaped upload rows, so their success does not establish File recovery.

**Smallest testable correction boundary:** extend the existing sweeper to admit the persisted File owner/path through the same strict tenant-qualified File validation already added for ordinary service operations. Preserve Card validation, backend abort/error handling, tenant transaction ownership, and existing terminal-state race protection. If owner projection is needed, add it to the existing expired-row projection rather than inferring a caller-supplied owner or adding another sweeper. No change to expiry duration, retention policy, completed payload deletion, discovery ordering, or batch sizing is necessary. Raising batch limits or skipping File rows does not repair the orphan and cannot preserve the complete lifecycle.

**Focused closure proof:** extend the existing real-Postgres sweeper proof with expired File pending and orphan initiating sessions; after a tick they become aborted under their tenant. With a batch size of one, seed an older File and a later Card session, run bounded ticks, and show both are reclaimed. Exercise real multipart abort through the established backend fixture where relevant, retaining the completed-session race assertion. No clock sleeps, new worker, or generalized fault harness is needed.

## Verification and limits

The implementation evidence claims V1–V8, local/S3 multipart/GCS single-put/Azure block-blob journeys, changed hosted/OpenAPI modules, and format/lint/language/codegen/boundary checks. Source inspection confirms those tests exercise the described normal/negative paths and route removal; it does not convert those claims into independently rerun results. The e2e fixture correction registers a real Card before inventory completion, matching Scribe's registered-subject requirement rather than weakening a production check.

The orchestrator's separate `verification.md` records fresh review-time exact V1/V2/V3 runs: three passed, with successful repository migrations, and range `git diff --check` passed. Those tests remain serial and do not exercise either finding. This reviewer ran no heavyweight Cargo/cloud suites and changed no source. Credentialed GCS multipart proof remains unavailable as disclosed by implementation evidence. No additional speculative findings or optional improvements are proposed.
