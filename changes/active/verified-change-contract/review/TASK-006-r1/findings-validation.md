# TASK-006 Wave 2 Structured Ponytail Validation

## Subject and result

- Base: `f8811ac5035c3aa165d34c38992f9889b3c9081f`
- Candidate: `55c5bff84fbfcc8f43967a8ad3aeac6057f10c3f`
- Approved authority: `changes/active/verified-change-contract/spec.md`, revision 35
- Original task: `changes/active/verified-change-contract/tasks/TASK-006-continuous-eval-verifier.md`
- Wave 1 inputs: `task-review.md`, `standards-review.md`, `domain-review-eval.md`, `domain-review-data.md`, and `domain-review-security.md`
- **Result: ten validated implementation findings.** The requested commit objects
  and reviewed product source remained unchanged. The repository owner accepted
  the intervening skill-documentation commit `01146cf87d22147b87d0c9224aa2bdf67decad92`
  and explicitly resolved the System-principal decision after Wave 2. These
  human decisions supersede the prior blocked disposition; they do not change
  any Wave 1 source findings or the named candidate.

No reviewed source was modified.

## Validation of every proposed finding

| Wave 1 source ID | Status | Resolution |
|---|---|---|
| `TASKREV-006-001`, `DATA-001` | REVISED | Retained as `FIND-TASK-006-1`. The defect is confirmed, but the minimum correction is to reuse the existing batch-fence `Committed`/`AlreadyCommitted` disposition and invoke activation only for the commit that inserted the rows. Persisting or re-querying an authoritative replay timestamp is unnecessary. The original fail-open callback may still fail without recovery, as the approved non-outbox behavior permits. |
| `TASKREV-006-002`, `RS-001` | CONFIRMED | Retained as `FIND-TASK-006-2`. |
| `TASKREV-006-003`, `EVAL-DOM-004`, `RS-004`, `SEC-T006-01` | REVISED | Deduplicated and retained as `FIND-TASK-006-3`; the effective body read, not the preceding metadata read, must enforce the ceiling. |
| `EVAL-DOM-001` | REJECTED | The reported two-Prompt path is not reachable through this implementation. `SkaldJudgeInvoker::agent_for` permits one constrained judge Agent per Eval run and all executable LLM judge tasks therefore use the same cached Prompt. Applying the record binding set to that one Prompt is exactly REQ-131's required behavior. Supporting multiple judge Agents/Prompts would broaden the task and cannot justify remediation here. |
| `EVAL-DOM-002` | CONFIRMED | Retained as `FIND-TASK-006-4`. |
| `EVAL-DOM-003` | CONFIRMED | Retained as `FIND-TASK-006-5`. |
| `EVAL-DOM-005` | CONFIRMED | Retained as `FIND-TASK-006-6`. |
| `RS-002` | REVISED | Retained as `FIND-TASK-006-7`. Tenant confinement succeeds, while principal authenticity and audit attribution fail. The owner approved reuse of the existing stable per-tenant System principal with a narrow server-minted read scope, making this bounded remediation. |
| `RS-003` | REVISED | Retained as `FIND-TASK-006-8`. `ScheduledQueryCaller` is intentionally a generic full-result collector; the smallest correction belongs to the new Eval trace query, which must close its time interval and impose a fixed result ceiling before that collector. |
| `RS-005`, `SEC-T006-02` | REVISED | Deduplicated and retained as `FIND-TASK-006-9`. The defect is broader than media locators: SQL, Bifrost, registry, storage, and provider strings all enter the public `VerificationError.message`. |
| `RS-006` | CONFIRMED | Retained as `FIND-TASK-006-10`. |

No optional suggestions were promoted. Proposed extra frameworks, replay stores, provider upload lifecycle, generalized streaming collectors, multiple-judge support, and speculative configuration knobs are rejected.

## Final deduplicated finding ledger

### FIND-TASK-006-1

- **Source IDs:** `TASKREV-006-001`, `DATA-001`
- **Status / classification:** REVISED / INCORRECT
- **Obligation:** REQ-077, REQ-079, INV-004, AC-014, and Task Scenario 4 require an observation-created run to freeze the committed row's exact server-managed `wyrd_event_time`, with sealed-batch replay converging idempotently.
- **Location and evidence:** `crates/vala/vala-bifrost-redux/src/scribe/ingress.rs:346-357,492-582` creates a fresh `receipt_micros` on every ingress attempt and returns it after durable completion. `crates/vala/vala-bifrost-redux/src/scribe/shards.rs:3623-3710` already learns whether the batch-control fence was `AlreadyCommitted`, but that disposition is discarded before `FrameAdmission`. `crates/vala/vala-bifrost-redux/src/gate/mod.rs:979-993` invokes the observation hook for every acknowledged attempt, and `tables/eval/observations.rs:92-120` substitutes the attempt-local receipt when the client frame has no event time. The hook's only production caller then persists that value through `verification/observations.rs:59-117`.
- **Consequence:** an exact replay can win the asynchronous unique-key race with a new receipt instant. The run then records false provenance and, across a UTC boundary, prunes a different day from the one containing the committed row and exhausts retries as `errored`.
- **Minimum decision-complete correction:** propagate the existing Scribe batch-fence disposition through durable acknowledgement and call the existing observation activation hook only when that acknowledgement corresponds to the commit that inserted the batch. Do not add an outbox, replay timestamp store, Bifrost poller, or post-ACK row query. Preserve best-effort asynchronous enqueue; an activation failure remains a recorded fail-open loss as approved.
- **Focused closure proof:** a real Gate/Scribe/Postgres test submits one unstamped sealed Eval batch, waits for its ACK, replays identical batch ID and bytes with a forced different receipt day, and proves the hook is called once, exactly one run exists, its frozen event time equals the stored row, and the runner reads the original-day row.

### FIND-TASK-006-2

- **Source IDs:** `TASKREV-006-002`, `RS-001`
- **Status / classification:** CONFIRMED / INCORRECT
- **Obligation:** REQ-083, INV-010, and AC-016 require sampling before execution and a restart/retry-stable decision for one durable run.
- **Location and evidence:** `crates/wyrd/wyrd-sql/src/queries/verifier_runs.rs:269-279,1295-1312` recomputes an ordinal as the count of currently visible prior rows. Its sole production consumer, `crates/wyrd/wyrd-server/src/verification/eval.rs:169-198`, repeats that read on every attempt. An earlier-created uncommitted row can become visible between two attempts and change the ordinal.
- **Consequence:** the same run can execute once and later become sampled out, or the reverse, changing durable result and dispatch behavior after retry/reclaim/restart.
- **Minimum decision-complete correction:** in the existing `VerifierRunQueue` enqueue owner, serialize observation-run creation per binding, assign the next ordinal once in the insert transaction, persist it on the run, and have `observation_ordinal` read that immutable value. Do not introduce a sampling service or recompute historical rank.
- **Focused closure proof:** a Postgres concurrency test holds an earlier enqueue transaction open, completes the later enqueue/ordinal read, commits the earlier transaction, and proves the later run's stored ordinal and `every_nth` selection are unchanged on a second attempt.

### FIND-TASK-006-3

- **Source IDs:** `TASKREV-006-003`, `EVAL-DOM-004`, `RS-004`, `SEC-T006-01`
- **Status / classification:** REVISED / INCORRECT
- **Obligation:** REQ-131 and AC-027 require the bytes actually resolved and sent to be bounded and oversized media to fail before provider invocation.
- **Location and evidence:** `crates/wyrd/wyrd-server/src/verification/eval.rs:694-752` stats the object, then calls unrestricted `StorageHandle::get_object`; `crates/wyrd/wyrd-storage/src/handle.rs:272-303` performs a separate full-body `Operator::read` and materializes all bytes. `TenantMedia::resolve` does not validate the returned body's length before base64 expansion.
- **Consequence:** replacement or stale metadata can bypass the 20 MiB ceiling, causing unbounded allocation/encoding and oversized provider input.
- **Minimum decision-complete correction:** add one bounded read operation to the existing `StorageHandle` owner using the already-installed OpenDAL range/read capability, retaining at most `limit + 1` bytes and returning overflow when the sentinel byte exists. `TenantMedia` uses that result; metadata may remain only as a fast rejection. Add no storage abstraction or tunable knob.
- **Focused closure proof:** a storage-backed resolver test reports an allowed metadata length but returns more than `MEDIA_LIMIT_BYTES`; it proves overflow is refused before encoding/provider invocation and that the read retains no more than `limit + 1` bytes.

### FIND-TASK-006-4

- **Source IDs:** `EVAL-DOM-002`
- **Status / classification:** CONFIRMED / INCORRECT
- **Obligation:** REQ-083, REQ-111, and INV-012 require continuous Eval to preserve the existing trace assertion semantics rather than fabricate different evidence.
- **Location and evidence:** `crates/wyrd/wyrd-server/src/verification/eval.rs:394-496` selects no event/link columns and hard-codes `events`, `links`, and their dropped counts to empty/zero. The sole scoring path passes those `SpanRecord`s into `vala-eval`; `crates/vala/vala-eval/src/tasks/trace.rs:124-146` exposes events and links to authored JSONPath selectors. The canonical trace projection already persists them in `crates/vala/vala-bifrost-redux/src/tables/traces/projection.rs`.
- **Consequence:** valid event/link assertions evaluate invented empty evidence and can produce false assertions and verdicts.
- **Minimum decision-complete correction:** extend the existing `BifrostReader::spans` projection and `span` decoder to reconstruct the canonical persisted `SpanEvent`, `SpanLink`, and dropped-count fields into the existing `SpanRecord`; add no second trace model or execution path.
- **Focused closure proof:** a real-server continuous Eval journey exports a span with one event and one link, selects both through the existing trace task, and asserts the expected item and common verdict.

### FIND-TASK-006-5

- **Source IDs:** `EVAL-DOM-003`
- **Status / classification:** CONFIRMED / INCORRECT
- **Obligation:** REQ-083's retry/restart behavior and the evaluation reproducibility authority require stable evidence for positional trace selectors.
- **Location and evidence:** `crates/wyrd/wyrd-server/src/verification/eval.rs:394-423` queries multiple spans without `ORDER BY` and preserves physical batch/row order. `crates/vala/vala-eval/src/tasks/trace.rs:124-146` exposes the resulting array to positional selectors such as `$.spans[0]`. No other production caller reorders the rows.
- **Consequence:** compaction, fused-source composition, or retry can change the assertion result for the same committed trace.
- **Minimum decision-complete correction:** order the existing trace query by persisted `start_time_unix_nano, span_id`, using span ID as the total-order tiebreaker. Do not add an in-memory sorting layer or ordering configuration.
- **Focused closure proof:** a multi-span real-server case inserts spans in the reverse of logical order and proves a positional assertion has the same result on repeated reads across runtime restart.

### FIND-TASK-006-6

- **Source IDs:** `EVAL-DOM-005`
- **Status / classification:** CONFIRMED / MISSING
- **Obligation:** AC-014 and Task Scenario 5 explicitly require the real continuous journey to demonstrate managed-event-day partition pruning when client `created_at` is on another UTC day.
- **Location and evidence:** `crates/wyrd/wyrd-testing/tests/bifrost/server/eval_verification.rs:215-226,762-780` offers no authored `created_at` control and proves only equality between the row event time and frozen run event time. The implementation query at `verification/eval.rs:330-345` uses managed event time, but the required adversarial journey is absent.
- **Consequence:** the acceptance test remains green if lookup regresses to client `created_at` whenever both happen to share a day.
- **Minimum decision-complete correction:** extend the existing Eval journey helper, using the existing SDK observation options/record construction surface, to author `created_at` on a different UTC day; do not create a new harness.
- **Focused closure proof:** that journey completes the run and reads the exact row/result through the frozen managed-event day while asserting the authored creation day differs.

### FIND-TASK-006-7

- **Source IDs:** `RS-002`
- **Status / classification:** REVISED / VIOLATION
- **Obligation:** `architecture/wyrd-security-posture.md` requires every internal request to be authenticated and authorized at the receiving boundary, and Oracle read audit to identify authoritative principals.
- **Location and evidence:** `crates/wyrd/wyrd-server/src/verification/eval.rs:270-301` creates a fresh UUIDv7 `PrincipalKind::User`, grants it `bifrost_query:read`, and constructs an internal authorized context without loading any persisted identity or evaluating canonical authority. Both record and span reads use this sole reader. Existing SYSTEM token issuance grants exact-Verifier record-write authority, not query-read authority, so it is not a reusable read principal.
- **Consequence:** Oracle audit attributes each read to a nonexistent, changing user whose authority was fabricated by the caller; the decision cannot be revoked or reconciled to stored identity.
- **Owner-approved decision:** continuous Eval's internal Bifrost reads use the existing per-tenant `PrincipalKind::System` principal: stable persisted ID, no credential or public lifecycle, and a narrow server-minted scope for Eval input reads. No fabricated `User` principal is permitted.
- **Minimum decision-complete correction:** in the existing tenant System-principal and Oracle query-authority path, load the persisted tenant System principal and authorize Eval observation/trace reads with a narrow server-minted read scope. Retain the separate exact-Verifier write scope for result publication; neither capability grants general query or public access. Route Oracle's decision and audit through the existing path so the stored System principal is attributable. Update `changes/active/verified-change-contract/spec.md` and `architecture/wyrd-security-posture.md`, whose current text says this principal is write-only, to record the approved dual-purpose, separately scoped internal capabilities. Do not create another principal, credential, user grant, token format, or public lifecycle.
- **Focused closure proof:** integration proof that the Eval Oracle read's audit row names the same stable persisted System principal on repeated reads and restart; missing, wrong-tenant, or under-scoped authority fails closed before rows are returned. Existing result-writer scope and public System-principal exclusions remain green.

### FIND-TASK-006-8

- **Source IDs:** `RS-003`
- **Status / classification:** REVISED / VIOLATION
- **Obligation:** AGENTS.md §10 and Bifrost resource rules require analytical reads to state a bounded time range and prevent unbounded result accumulation.
- **Location and evidence:** `crates/wyrd/wyrd-server/src/verification/eval.rs:389-423` has only a lower event-time predicate and no row ceiling. Its sole caller for this SQL passes through `crates/wyrd/wyrd-server/src/query/scheduled.rs:142-226`, which intentionally collects every decoded batch. A trace ID can therefore match indefinitely many future rows.
- **Consequence:** one run can grow query work and server heap until deadline/resource failure, with the same retry repeating the open-ended scan.
- **Minimum decision-complete correction:** keep the generic scheduled collector unchanged. In `BifrostReader::spans`, close the event-time interval around the record/trace deadline and apply one fixed server-owned maximum span count with one extra sentinel row; reject overflow as the existing trace-source execution error. Reuse the current SQL query and retry path; add no streaming framework or runtime setting.
- **Focused closure proof:** a real query/journey proves both partition predicates appear in the plan, a trace at the ceiling decodes, and ceiling-plus-one returns the stable trace error without task/provider execution.

### FIND-TASK-006-9

- **Source IDs:** `RS-005`, `SEC-T006-02`
- **Status / classification:** REVISED / VIOLATION
- **Obligation:** the repository error authority and `VerificationError` contract require secret-free stable public errors; private storage locators and raw infrastructure/provider diagnostics must not be persisted, logged as the public message, or returned by run status.
- **Location and evidence:** `crates/wyrd/wyrd-server/src/verification/eval.rs:120-156,169-198,242-267,633-664,694-752` converts Bifrost, SQL/registry, judge/provider, tenant-path, and storage errors with `to_string()` into `VerificationError.message`. `verification/runner.rs:565-570` persists/logs it, and `components/verification/service.rs:183-220` exposes it. Storage errors include the object key, and malformed locator/provider/SQL text can carry dependency diagnostics.
- **Consequence:** callers with run-read access and log readers can receive private paths, URL material, backend/provider detail, or database diagnostics; public text also drifts with dependencies.
- **Minimum decision-complete correction:** at the existing `EvalEngine` boundary, log each raw cause once in structured server diagnostics and construct `VerificationError` only from the existing stable code plus operation-specific safe text. `TenantMedia` may include the non-sensitive binding ID and safe category, never locator/path/backend `Display`. Do not add a second error catalog or expose new public detail fields.
- **Focused closure proof:** inject unique sentinels through representative Bifrost/SQL, storage-locator, and provider failures, settle the runs, and assert HTTP/MCP status and persisted `VerificationError` retain stable codes but contain no sentinel. Capture logs separately to prove raw diagnostics are available only in the protected server diagnostic event and private locator strings are redacted there too.

### FIND-TASK-006-10

- **Source IDs:** `RS-006`
- **Status / classification:** CONFIRMED / VIOLATION
- **Obligation:** AGENTS.md §16 and `architecture/agent-rules.md` require rustdoc, `# Errors`, and cancellation/partial-progress behavior for every materially modified fallible async Rust item; absence is `BLOCK_BEFORE_MERGE`.
- **Location and evidence:** `crates/vala/vala-eval/src/executor.rs:385-418` materially changes `fan_out_bucket` to propagate the first executor/join error. Its complete body has no rustdoc, error contract, or explanation that returning drops the `JoinSet` and aborts remaining tasks. Its only callers are the four branches in `fan_out_all_buckets`; therefore this is the exact shared owner to document.
- **Consequence:** the candidate violates a hard repository merge rule at the concurrency/error boundary changed by the task.
- **Minimum decision-complete correction:** document this existing helper's bucket fan-out role, first-error propagation, and sibling-task cancellation on early return. Add no wrapper or refactor.
- **Focused closure proof:** `mise run lints` and the repository documentation/touched-item check pass; the existing executor-error propagation test remains green.

## Recommendations

1. Package all ten findings in one remediation task under the owner's two explicit decisions.
2. For `FIND-TASK-006-7`, align the spec and security posture with the approved System-principal read capability as part of the same remediation.
3. Keep shared fixes singular: Scribe commit-disposition activation, immutable queue ordinal, bounded storage read, and sanitized Eval error conversion.

## Verification limits

- Validation inspected the named candidate objects with `git show`, the complete base-to-candidate diff, all Wave 1 reports, and the full bodies and production callers named above. The checkout's later skill-doc commit did not alter these object-level observations and the owner accepted it.
- This was a static Wave 2 pass. No broad `mise` lane was rerun and no reviewed source was changed.
- Caller tracing is complete for every retained correction boundary. The owner has now selected the existing persisted per-tenant System principal for Eval input reads. The current write-only authority text remains to be updated during remediation.
