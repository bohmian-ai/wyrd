# TASK-003 system resilience review

Overall: **PASS** within the assigned failure-propagation and recovery scope. No material source-backed finding is proposed.

## Immutable subject and coverage

Repository: `/Users/stevenforrester/Documents/GitHub/wyrd`.
Base: `3a99eb05fd5ea78a21fd732212bf2c5d3672f27f`.
Candidate: `f1f6a9c833061ec6f0a781247b762ec3c0de4bea` (HEAD verified).
Reviewed the complete base-to-candidate diff, approved revision 2 of `changes/active/storage-upload-consolidation/spec.md`, and original `tasks/TASK-003-canonical-evidence.md`. Appended execution evidence was treated as reported verification, not acceptance authority. No prior acceptance was carried forward. No `.codegraph/` directory is present. Source was kept immutable; only this report was written.

Assigned scope: execution-local metadata lifetime; File/Artifact inventory admission; direct HTTP failure boundaries; queued attempts, exhaustion, cancellation and recovery; preservation of byte/workspace validation. Security specialist and behavior reviewer own their broader independent scopes. No subagent or Cargo/benchmark process was started by this reviewer.

Applicable authorities read: root AGENTS instructions; `changes/spec-driven-development.md`; `agent_docs/maintainer-style.md`; `architecture/README.md`; owning Verifier evidence section of `architecture/wyrd-design.md`; Bifrost durability/visibility section; operations readiness/topology section. No nested AGENTS file exists in the changed server/architecture directories.

## Deployed paths and failure boundaries

| Path | Source and deployed boundary | Consequence/recovery |
|---|---|---|
| Direct invocation | `components/verification/service.rs:550–633` builds the hosted engine and maps `EngineError::EvidenceUnavailable` to `WyrdError::ServiceUnavailable`. `verification/hosted/blocks.rs:137–177` builds one `TenantEvidence` and shares it with blocks, tools, media and agent evidence. | Inventory admission failure rejects this request with the existing typed 503. It does not terminate HostedBlocks, the runtime supervisor, or the shared server. No successful outcome is staged on this error path. A later request creates fresh evidence state and retries resolution. |
| Queued invocation | `verification/runner.rs:439–513` resolves canonical submitted/grouped evidence, creates the accepted caller and a fresh engine per attempt; `engine_failure:1030–1061` maps unavailable evidence to `Retry(evidence_unavailable)`. `settle:672–680` delegates to the existing fenced queue owner. | The attempt fails before blocks run. `wyrd-sql/src/queries/verifier_executions.rs:2496–2546` schedules ordinary bounded backoff, then terminates `errored`, with no verdict or Operator dispatch, at the attempt limit. There is no publication wait loop or new readiness worker. |
| Inventory query | `components/storage/inventory.rs::rows` uses table-scoped `SystemReadAuthority`, `ScheduledQueryCaller` and a query deadline derived from half the configured request timeout. Missing tables yield no rows; actual query failure is propagated. `TenantEvidence::file_object:208–245` maps absence/error to retryable readiness and memoizes only success. | No failed lookup poisons a subsequent attempt. No object HEAD, inferred digest location, SQL completed-object projection, or second inventory supplies substitute evidence. Normal Oracle/Scribe source availability remains the dependency. |
| Artifact resolution | `TenantEvidence::inventory:322–349` calls the authorized Card owner once and retains exact UID/files. `cards/service.rs::card_files:335–359` refuses completed manifests absent from the inventory; `hydrate_card` verifies envelope/registry/inventory integrity. Later metadata use reauthorizes through `authorize_card_ref`. | Unavailable inventory/registry maps through `hosted/registry.rs::refusal` to retryable admission; invalid references and permission failures remain terminal. Successful metadata reuse does not skip byte validation or broaden its lifetime beyond the attempt. |
| Workspace and byte consumption | `TenantEvidence::{read,artifact,stored}` retain bounded object-length/body reads, declared length and digest verification, format sniffing and terminal object-not-found mapping. `hosted/workspace.rs:69–126` acquires bounded workspace admission and obtains bytes through that same reader. | Inventory outage after successful admission does not force duplicate metadata reads. Storage outage, corruption, or absence still prevents usable bytes. Failed/cancelled staging drops its permit; existing temporary workspace ownership is preserved. |

Actual topology is established by `architecture/operations/README.md:35–75`, not inferred from the local test setup: public serving may coexist with local Scribe/Oracle or use their peer planes according to `WYRD_TARGET`; a logical server may have multiple replicas. `architecture/wyrd-design.md:670–685` retains the process outbox and local/peer writer choice. `architecture/bifrost-design.md:485–555` makes ACK a durable Scribe append, not an unconditional inventory visibility promise; Oracle query source failures remain failures under the same deadline. This candidate adds no writer, health dependency, readiness policy, deployment role, or cross-process cache.

`verification/mod.rs:1–10,346–393,486–595` is the existing process lifecycle owner: runtime capabilities are supervised separately, the runner is composed only with hosted blocks and a Scribe-reachable outbox, and durable queue leases allow recovery by another process. Candidate changes do not replace this owner.

## Failure and recovery assessment

| Scenario | Source evidence | Proof assessment |
|---|---|---|
| Missing/delayed File inventory | `TenantEvidence::admit:484–552` resolves every File before execution; `file_object` has no negative memo. A subsequent queued attempt uses new `TenantEvidence`. | Added `pg_verification_runtime::inventory_readiness_preserves_execution_bounds:1341–1428` explicitly observes two retries followed by three-attempt exhaustion; a separate execution retries, upload completes, and the next attempt completes. Reported GREEN evidence is credible for this changed path. |
| Inventory query outage and restoration | Query errors become `BlockError::Retryable`, then `admission:688–695` preserves `EvidenceUnavailable`; direct service maps it to typed 503. | Same test injects `fail_next_query_after_schema`, checks 503 with inventory-unavailable reason, then observes a passing new direct request. This proves request-local restoration, not every multi-pod network failure. |
| Query stalls or many serial input lookups | `vala-verify/src/blocks/engine.rs:353–376` wraps attachments, admission and execution in `timeout_at(deadline, ...)`; inventory also supplies its query deadline. | Source proves new admission IO is inside the existing total engine deadline, including before any block runs. This is not an independent latency measurement. Deadline expiry follows existing direct timeout / queued timed-out behavior rather than indefinite waiting. |
| Cancellation, lease loss, rolling stop or process crash during admission | `runner.rs:886–918` selects shutdown abandonment or lease loss against the execution future; retries observed after stop become fenced Release. Graceful release refunds attempts. A hard crash leaves the durable lease to expire and be reclaimed. | No new uninterruptible wait, persisted memo or extra transaction is introduced. The memo contains only complete successful metadata and holds no mutex guard across IO. Lifecycle behavior is source-supported and unchanged, not newly chaos-tested in this review. |
| Corrupt/deleted payload after metadata admission | `stored:363–405` keeps length/digest checks and terminal object-not-found; `read:160–194` retains format validation. | Existing `pg_hosted_files::{evidence_reads_only_matching_files,evidence_refuses_tampered_body}` and `pg_hosted_artifacts::artifact_reads_refuse_before_use` assert terminal mismatches. Added metadata-reuse test reads File/Artifact bytes through repeated reads, citations, media and workspace while asserting only two inventory queries. |
| Shared dependency outage | Inventory read failure stays within request/attempt; standard operations readiness independently reflects unhealthy selected Oracle/Scribe dependencies. | No change creates a process crash, new ACK dependency, retry barrier for sibling producers, or new outbox recovery promise. Existing source governs process/Scribe restart behavior. |

A missing inventory row is deliberately not treated as known deletion. Existing File inventory folding excludes deleted locations; this candidate does not add a production File deletion path or change its prior readiness classification. Known missing bytes after a valid location retain the existing terminal read behavior. No new deletion, retention or repair promise is inferred from this task.

## Scoped acceptance matrix

| Obligation | Implementation evidence | Verification evidence | Result |
|---|---|---|---|
| REQ-011: reuse required metadata across admission, reads and workspace | Execution-owned maps in `TenantEvidence`; serial admission; shared reader wired by HostedBlocks | Added `pg_hosted_artifacts::evidence_metadata_is_resolved_once`, repeated bytes/media/citation/workspace use and inventory count | PASS |
| REQ-013: typed retryable missing/unreadable inventory, bounded direct and queue behavior, no guess/fallback | File lookup/admission, direct service error mapping, engine deadline, runner and queue retry owner | Added runtime readiness/exhaustion/recovery test plus source trace | PASS |
| INV-004 / REQ-003: preserve byte/format/resource validation | Existing `stored`, `read`, and Workspaces retained; memo stores metadata only | Existing hostile Artifact/File tests; added workspace reuse path | PASS |
| REQ-008: preserve result/audit/other producer settlement and ACK semantics | Diff changes inventory reads/admission and Card hydration return plumbing; queue/result/outbox writers unchanged | Runtime transitions inspected; no new writer/waiting barrier | PASS |
| No queue redesign/global cache/durable repair | Fresh evidence owner for each engine; durable queue remains existing authority | Full diff and relevant lifecycle owner inspected | PASS |

## Proposed findings and limits

Validated-at-discovery proposal set: empty. No correction burden is imposed; no material failing scenario was established in this scope.

This is a static specialist review with inspection of existing and added test source and recorded results. I did not rerun tests, invoke Cargo, run benchmarks, or perform a multi-pod failure experiment. Reported benchmark runs admitted no work and supply no performance evidence; performance acceptance remains with the comprehensive task review. Passing this scope does not establish upload or inventory latency costs.

Independent verification received before finalization: `verification.md` records an immutable-candidate rerun of exact V1, V2 and V3 selectors against repository Postgres with tracing, three selected tests passed (nextest run `2e8390bb-6504-4c00-903d-139d10a739d5`), plus `git diff --check`. I inspected that record. It strengthens the metadata-reuse and direct/queued readiness/recovery rows above; it does not establish measured performance or multi-pod crash recovery.
