---
id: TASK-002-R1
kind: remediation
status: ready
spec: SPEC-storage-upload-consolidation
spec_revision: 2
findings: [FIND-TASK-002-1, FIND-TASK-002-2, FIND-TASK-002-3]
---

# Preserve independent File uploads and reclaim abandoned sessions

Route directly to `$wyrd-implement`. Correct the three independently validated findings below without reopening the task's accepted surfaces or pulling TASK-003 forward.

## Acceptance context

- Repository: `/Users/stevenforrester/Documents/GitHub/wyrd`.
- Approved specification: `changes/active/storage-upload-consolidation/spec.md`, revision 2.
- Original task: `changes/active/storage-upload-consolidation/tasks/TASK-002-shared-file-uploads.md`.
- Original base: `15f939fc917b22efa5e5e4d12fd1f97be807a0a7`.
- Reviewed candidate / remediation baseline: `5ef2981e66068e8ee5f96d3be15000eea9cbd94f`.
- Review: this directory's `verdict.md`, `findings-validation.md` and `followup-review.md`. Keep them and the original task as acceptance context for remediation review.

Before this correction, two parallel verifications of the same document can cancel one another's upload, and a canceled File upload can remain in the shared expiry queue forever. Afterward, independent File sessions have independent final locations and state, while the existing sweeper reclaims abandoned Files and Cards through the same lifecycle. Existing File authoring, completion acknowledgement, validation and authorization remain intact.

## FIND-TASK-002-1: independent planning aborts a live upload

**Diagnosis.** `wyrd-storage/src/service.rs:202–226` initializes each File at the same tenant/digest object path with no Card. Shared initialization at `:269` reaches `find_and_mark_prior_pending` at `:812–834`, which matches NULL Card, relative path and digest through SQL `multipart_uploads.rs:392–436`, then aborts the earlier pending session. A-plan, B-plan, A-transfer for the same tenant/File is sufficient: B revokes A; A's local transfer or completion returns `WYRD_STORAGE_409_UPLOAD_NOT_PENDING`. Shared-client `RunEvidence::pin` propagates that failure before verification. Parallel verifications of a common reference document are supported use, so this regresses the original task's preserved authoring behavior and REQ-001/016. Existing serial tests do not exercise it.

**Selected correction.** Keep File sessions independent using their existing upload identities: each independently initialized File gets its own final object location distinguished by its existing `UploadId` beneath the tenant's digest evidence namespace. Persist that location with the same session and retain it through transfer, completion, inventory recording and replay. Preserve destructive pending replacement for Card reinitialization only. Extend the existing evidence path construction and strict validation consistently across request continuation, inventory selection and fixtures; verify the tenant, canonical digest and server-generated session segment. `ObjectInventory` remains the only completed-object authority, and hosted reads continue using its returned location. An existing completed File digest/size match still avoids initialization entirely.

The active-path unique index stays intact. Simply omitting abort still hits that index; relaxing it leaves concurrent provider writers at one key, including Azure's fixed block IDs. Replaying a live session merges independent abort and backend commit ownership and would need new semantics. New caller retries/refusals leave the supported operation regressed. Existing per-session locations solve the cause without locks, another ID system, a queue, cache, provider-specific transfer code or new shared-session ownership. The location is final from initialization onward: no promotion, extra hashing or immutability guarantee. REQ-004 permits existing upload identities, REQ-010 resolves recorded locations, and fresh-install scope requires no old-path compatibility. This is private path/session plumbing under the approved behavior.

**Affected paths.** Preserve shared planner/dispatcher/signers and `ValidatedPath` consumers; coordinate strict File validation in normal lifecycle, inventory folding and the sweeper. Update exact-path assertions/synthetic deletion facts in `pg_verification_routes`, path unit coverage, and the hosted tamper test's target: `pg_hosted_files::evidence_refuses_tampered_body` must tamper with the actual completed object's recorded location rather than a digest-derived guess. Completion replay must still identify one logical object for that upload. Multiple independently initialized uploads may have distinct completed locations; no global digest deduplication or new deletion policy is required.

## FIND-TASK-002-2: the shared sweeper rejects File sessions

**Diagnosis.** New File initialization commits ordinary initiating/pending rows with nullable Card ownership and evidence paths. Existing `wyrd-storage/src/sweeper.rs:297–298` still validates every expired row with the Card-only path grammar. It fails before backend abort and before `mark_aborted_if_open`, so the row remains eligible forever. `wyrd-sql/src/queries/storage/admin/multipart_uploads.rs:43–78` orders oldest expiry/grace first with a bounded LIMIT. Enough abandoned Files occupy every batch and starve later expired Cards across tenants; orphan initiating rows retain their active-path claim. Normal cancellation/restart therefore breaks the existing recovery lifecycle and Artifact reclamation (REQ-001/016). Existing Card-only sweeper fixtures miss the new row class.

**Selected correction.** Extend the existing expired-upload projection and query with persisted nullable Card ownership. In the existing sweeper, choose strict Card or session-qualified File validation from that ownership, matching the normal lifecycle's owner validation. Keep verified tenant checks, existing backend abort behavior, per-row errors, tenant transaction ownership, terminal-state race protection and status commit. Reuse the existing worker and coordination owner. Accepting either grammar without checking persisted ownership weakens the boundary; skipping File rows, changing limits or a second worker leaves the cause unresolved. Do not change expiry/grace, completed-payload deletion, provider cleanup semantics, retention, ordering or batch policy.

## FIND-TASK-002-3: mandatory changed-item rustdoc is missing

**Diagnosis.** Root AGENTS and `agent_docs/rust.md#rustdoc` require meaningful documentation for changed items, including fields, private helpers and tests; Errors/Panics where applicable; and cancellation, partial progress, idempotency and retry for durable operations. The candidate omits those contracts at the following bounded inventory. Compiler and runtime success cannot waive this merge gate.

| Owner | Items to correct in place |
|---|---|
| `wyrd-storage/src/service.rs` | `PriorAbort` and `validated`; `PersistUploadInit` and `body_sha`; `upload_init`; `file_upload_init`; `init_upload`; `persist_upload_init`; `claim_upload_init`; `try_replay_idempotent_init`; `find_and_mark_prior_pending`; `abort_prior_best_effort`; `insert_initiating_upload`; `upload_part_url`; `upload_abort`; `upload_local_blob` |
| `wyrd-client/src/verify/mod.rs` | `RunEvidence::pin` partial completion/cancellation/retry effects |
| `wyrd-server/src/components/verification/service.rs` | `VerificationControl::evidence`: earlier session commits can survive a later failure/cancellation before plans return |
| `wyrd-server/src/components/storage/routes.rs` | `part_url` and `abort` error contracts |
| `wyrd-server/src/http/middleware/body_limit.rs` | changed `Service::call` handler-owned upload bound contract |
| `wyrd-sql/src/queries/storage/multipart_uploads.rs` | `MultipartUploadRowDb` and nullable `card_uid` owner/decoding invariant |
| `wyrd-sql/tests/integration/pg_migration.rs` | `storage_queries_round_trip_under_tenant_conn` purpose and Panics |
| `wyrd-storage/src/tenant_path.rs` | `validate_evidence_accepts_only_own_tenant_digest_paths` Panics |

The exact missing contracts and candidate line locations are preserved in `findings-validation.md`. Document final semantics after the lifecycle corrections, including committed initiating rows, best-effort prior abort, backend-versus-SQL partial effects, and completed earlier Files. Use concise item rustdoc in place, not placeholders or new recovery promises. Include newly changed remediation items under the same mandatory rule. No new checker, suppression, helper extraction, broad documentation rewrite or documentation-only runtime test is needed.

## Constraints and preserved behavior

Preserve File/ArtifactRef public declarations, SDK authoring, shared planner/transfer/completion and row-specific ACK, Card replacement/reconciliation, per-file/aggregate limits, encryption/size checks, local digest validation, and hosted length/digest/format checks. Keep operation-level File staging, exact Verifier invocation, persisted-owner permission selection, tenant isolation, MCP write scopes, table-scoped internal inventory reads and nonblocking audit boundaries. Preserve one inventory and completion replay identity. Update owning documentation only where the private File location explanation changes, through the existing generator when needed.

Excluded: TASK-003 metadata reuse/application-SQL elimination/canonical readback, new public contracts, shared-session ownership, physical Card/File sharing, global deduplication, extra hashing, promotion, new queue/worker/cache, payload GC, migration/backfill, retries/wait policies and unrelated refactoring. The review rejected MAINT-001 as a TASK-002 blocker because the original TASK-003 explicitly owns REQ-014; it creates no remediation work here.

## Acceptance and focused proof

| Finding | Required observation |
|---|---|
| FIND-TASK-002-1 | Obtain two same-tenant/same-File plans before completion; neither revokes the other, both complete, and verification reads a completed inventory location. Replaying one completion preserves that upload's location/logical object identity. Separately abort A and finish B. Card re-init still replaces its prior pending session. Distinct live S3/Azure plans use distinct object locations; strict malformed/foreign/session-path refusal and hosted tamper checks remain. |
| FIND-TASK-002-2 | Expired pending and orphan initiating File sessions become aborted through the existing sweeper. With batch size one and an older File before an expired Card, successive bounded ticks reclaim both. Replanning after initiating-row reclamation succeeds. Completed-row race protection and owner/path/tenant refusal survive; File multipart cleanup reaches the existing backend-abort seam. |
| FIND-TASK-002-3 | Every item in the inventory and every item changed by remediation has the applicable purpose/invariant, Errors/Panics and durable partial-effect/retry contracts. |

Use existing fixtures and deterministic request/tick ordering, no sleeps or synthetic load. Add focused overlap/independent-abort cases under server `pg_verification_routes` and File expiry/orphan/starvation cases under storage `pg_sweeper::pg_tests`; new exact selectors are planned and must be confirmed before RED. Run all new tests by exact nonzero selector with `WYRD_LOG=info,wyrd_server=debug`, through `mise exec -- scripts/postgres/with-test-postgres.sh -- bash -lc "mise run db:migrate:all:inner && cargo nextest run --locked ..."`. Cargo runs are sequential.

Rerun the existing exact server selectors `pg_verification_routes::file_upload_uses_shared_completion`, `pg_verification_routes::file_reuse_requires_completed_file_inventory`, and `pg_verification_routes::file_upload_authority_and_validation_are_preserved` using `-p wyrd-server --test integration --test-threads=1`; rerun `pg_hosted_files::evidence_refuses_tampered_body` if adapting its target. Storage's existing selectors are `pg_sweeper::pg_tests::tick_sweeps_expired_uploads_and_reaps_idempotency_rows` and `pg_sweeper::pg_tests::sweeper_skips_reclamation_when_upload_already_completed`, under `-p wyrd-storage --test integration`. Add exact focused path-unit selectors and changed SQL projection tests as needed.

Provider proof uses only the affected `storage_e2e::s3_multipart_e2e` and `storage_e2e::azure_multipart_e2e` cases or focused new cases in that owner, with `-p wyrd-server --features storage-emulator --test integration --test-threads=1`. Reuse `storage:up` and the corresponding existing `test:storage:e2e:inner` environment/setup; inspect the owning mise task rather than inventing credentials/configuration. Do not require the full e2e aggregate. Preserve the disclosed lack of real-GCS multipart proof; no new credential/harness work is required.

Finish with `mise run fmt`, `mise run lints`, `mise run check:tenant-isolation`, `mise run check:storage:drift`, and `git diff --check` for the changed write set; use owning docs checks if prose/generated docs change. No full language/user-journey suite, release aggregate, benchmark or codegen run is required without a corresponding contract/generator change. Record RED cause, GREEN results, exact selectors and remaining proof limits for the bounded remediation review.

## Implementation evidence

Commits on `shared-file-uploads`: `f4406862f`, `e4e9d5621`, `559349cb9`, `9a8733256`.

| Acceptance criterion | Implementation evidence | Verification evidence | Result |
|---|---|---|---|
| FIND-1: two same-File plans before completion both complete at their own location; replay keeps its location; Verifier reads a completed object | `tenant_path::evidence`/`validate_evidence` take/verify the canonical `<digest>/<upload_id>` session segment; `service::file_upload_init` mints the `UploadId` first and `init_upload` takes it with the path | `pg_verification_routes::file_sessions_are_independent` (RED: first session's PUT `409 WYRD_STORAGE_409_UPLOAD_NOT_PENDING`; GREEN) | PASS |
| FIND-1: abort A, finish B | same | same test, second pair | PASS |
| FIND-1: Card re-init still replaces its prior pending session | `find_and_mark_prior_pending` unchanged; a File path is unique per session so only Card rows match | same test, Card pair (first PUT → 409) | PASS |
| FIND-1: distinct live S3/Azure plans complete | journey plans two File sessions before either transfers | `storage_e2e::s3_multipart_e2e`, `azure_multipart_e2e`, `gcs_single_put_e2e`, `local_client_server_round_trip` | PASS |
| FIND-1: strict malformed/foreign/session-path refusal; hosted tamper uses recorded location | `validate_evidence` refuses digest-only, extra segment, non-canonical id, Card path, foreign tenant; tamper test validates the completion's recorded path | `tenant_path::tests::validate_evidence_accepts_only_own_tenant_digest_paths`, `pg_hosted_files::evidence_refuses_tampered_body`, `pg_verification_routes::file_upload_authority_and_validation_are_preserved`, `file_reuse_requires_completed_file_inventory` (tombstone names the completed path), `file_upload_uses_shared_completion` | PASS |
| FIND-2: expired pending and orphan initiating File sessions abort; batch-of-one File before Card reclaims both; replan succeeds; owner/path/tenant refusal survives | `ExpiredUpload.card_uid` projected by `expired_uploads_batch`; `Sweeper::abort_one` validates by persisted owner | `pg_sweeper::pg_tests::tick_reclaims_file_sessions_without_starving_card_uploads` (RED: expired File stays `pending`; GREEN), `pg_migration::pg_tests::storage_admin_queries_find_and_abort_expired_uploads` (owner projected) | PASS |
| FIND-2: completed-row race protection and existing sweep | unchanged | `pg_sweeper::pg_tests::sweeper_skips_reclamation_when_upload_already_completed`, `tick_sweeps_expired_uploads_and_reaps_idempotency_rows` | PASS |
| FIND-3: rustdoc inventory and remediation-changed items | `service.rs` (`PriorAbort`+fields, `PersistUploadInit`+`body_sha`, `upload_init`, `file_upload_init`, `init_upload`, `persist_upload_init`, `claim_upload_init`, `try_replay_idempotent_init`, `find_and_mark_prior_pending`, `abort_prior_best_effort`, `insert_initiating_upload`, `upload_part_url`, `upload_abort`, `upload_local_blob`); `RunEvidence::pin`; `VerificationControl::evidence`; routes `part_url`/`abort`; `WyrdBodyLimitService::call`; `MultipartUploadRowDb`+`card_uid`; `ExpiredUpload.card_uid`; `Sweeper::abort_one`; pg_migration storage tests; tenant_path test Panics; new tests and fixture helpers | `mise run lints` | PASS |

Checks: `mise run fmt`, `mise run lints`, `mise run check:tenant-isolation`, `mise run check:storage:drift`, `mise run docs:check`, `git diff --check 15f939fc9..HEAD`, `wyrd-storage --lib` (53), `wyrd-client` `observe::tests::execute_pins_files_and_completes_shared_uploads`, `wyrd-sql` `pg_migration::pg_tests::storage_queries_round_trip_under_tenant_conn` — all pass. Postgres selectors ran through `mise exec -- scripts/postgres/with-test-postgres.sh` with `db:migrate:all:inner` and `WYRD_LOG=info,wyrd_server=debug`; e2e through `storage:up` with the `test:storage:e2e:inner` environment.

Non-goals confirmed: no new public contract, migration, index change, queue/worker/cache, retry or wait policy, global deduplication, promotion or hashing; expiry/grace/order/batch policy unchanged; TASK-003 work not pulled forward. `architecture/wyrd-design.md` gains one sentence on per-session File objects and sweeper reclamation. Only the untracked review directories remain uncommitted.

Limits: File multipart backend-abort dispatch in the sweeper is proven to pass owner validation and reach `abort_one`'s existing abort branch on the local backend only; no emulator sweeper case drives a provider abort. Real-GCS multipart remains unproven (emulator proves GCS single PUT).
