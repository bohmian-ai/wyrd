# Integration evidence — approved revision 2

This file preserves the complete research/command inputs so the packet does not
depend on temporary files. **spec.md revision 2 is the sole behavioral authority;
TASK-001 executes it.** Research quotations below are historical evidence, not
alternative instructions. All 27 obligations were approved in seven batches.

## Canonical corrections to earlier research

- Source Bifrost retention and deadline clamp win; no reader pins, expiry query
  barrier, or alternative over-limit rejection policy.
- Ten built-ins: no vala.dev.agent_traces or its sole-owner CodeAxis.
- Task payload is assertion_result Variant, not generic details. Drift/Eval
  summaries are typed Structs; only open child values are Variant.
- Keep source benchmark workloads/options/assertions/metrics/entrypoints, but
  compile into main's existing capacity target. Preserve main mixed defaults
  and reference source worktree. No separate-source-bin exception survives.
- Preserve main historical SQL bytes; old workload/grant/Workflow endpoint
  differences are not source post-base deletions/additions. Source registration
  outcome is decided atomically under its lock; Python error_code is catalog code.
- Main memory owner shutdown and late-child accounting survive; per-query slot
  release does not wait for children or revive old exchange/storage owners.
- The compaction artifact prerequisite is fulfilled by published fork commit
  `22cd178d14009468bc951a14d7acf63237af808a` on `wyrd/main`, pinned in
  integration commit `914495674`. Reachability was independently checked with
  `git ls-remote`; fork check/test results are reported in the updated
  [resolution map](../bifrost-variant/merge-research/RESOLUTION-MAP.md). Combined
  Wyrd graph verification remains required. No Ring waiver. Historical
  prerequisite statements below predate this publication and are superseded.
- Main-only raw-pool prohibition, identity/credential controls, shared outbox,
  writer-owned observation queue, governed judges and app lineage remain.

## Immutable provenance

M production: 8a8813a7458b7c98707c7b4f9b9d403f604ee0e6.
S production: cfe21dc9007d5d22d9ae4411be07fb5504b0cc05.
Common ancestor: 5ab92b0039404565e51f4a246863ce90cbd8d11f.
Remote integration research: 40113bc83ba49792d24b17c7552709c4566f2af0.
Initial spec/task commit: 28b6f3b6da61874bd345464f13b735cdde41833e.
527 source changed records; 228 overlapping paths; 148 direct conflicts;
76 automatically merged overlapping modifications needing semantic review.
Source-only facts must be inspected with git show at S, not assumed present in
this unimplemented main-based worktree. Reports/recipes are source-inspected;
no builds, migration execution, benchmarks or tests were run for planning.

## Durable cross-review evidence

[reconciliation.md](reconciliation.md) compares all 27 points with every other
reviewer report and records the latest approval resolutions. The other review
is preserved in [RESOLUTION-MAP.md](../bifrost-variant/merge-research/RESOLUTION-MAP.md)
and its redux/server-sql/shared-spec/sdks/testing/infra-docs/silent companions.
[BENCHMARK-HANDOFF.md](../bifrost-variant/BENCHMARK-HANDOFF.md) and its pre-merge
reports remain performance context; their earlier standalone-bin exception is
superseded by the explicit consolidation decision. The source baseline gaps,
matched-host requirements, fresh-only deployment and fork reachability checks
are requirements to prove, not claims of current success.

## Archived original research and focused proof inputs

The following complete snapshots retain citations, counterevidence, full
changed-path inventory and historical L01–L64/S1–S4 mapping. When an older
recommendation conflicts with the canonical corrections above, it is
superseded. No quoted stale recommendation grants implementation authority.

---

## Archived snapshot: wyrd-merge-audit-identity.md

# Audit, identity, credentials, and server composition merge research

Pinned trees: main `8a8813a7458b7c98707c7b4f9b9d403f604ee0e6`; source `cfe21dc9007d5d22d9ae4411be07fb5504b0cc05`; merge base `5ab92b0039404565e51f4a246863ce90cbd8d11f`. No `.codegraph/` exists in this checkout, so navigation used Git/rg. Static research only; no implementation or test execution.

## 1. Three-way classification matters

`git diff --stat BASE SOURCE -- crates/wyrd/wyrd-auth crates/shared/wyrd-runtime` is empty. The source branch has no post-base changes to those auth/runtime crates. Similarly its post-base diff for server auth/component-auth/middleware-authentication, grpc/otlp, wyrd-spec/auth is empty. Preserve main wholesale here. They do not need semantic cherry-picking from source; a normal three-way merge ordinarily retains the main edits. A whole-tree checkout from source would regress these surfaces, but that is not evidence Git itself will do so.

The real auth/audit conflicts are Bifrost-owned adapters/contracts and composition files changed by source while main replaced their audit collaborators, correlation rules, and runtime wiring. These can compile only after adaptation and can silently regress behavior if resolved by wholesale source selection.

## 2. Preserve the one Scribe outbox, adapt its Bifrost seam

Main authority and implementation: `crates/shared/wyrd-runtime/src/outbox.rs`, `src/audit.rs::AuditStage`; `crates/wyrd/wyrd-server/src/scribe_outbox.rs::{ScribeOutbox,ScribeSink,ScribeWrite,ScribeRoute,ScribeBatch}`; `state.rs::AppState::scribe_outbox`, `state.rs::ServerGate = Gate<ScribeOutbox>`; `boot/mod.rs::compose_bifrost` around lines 1057–1191. Main stages authorization decisions, gateway captures, and verification results on ONE process-memory outbox. `ScribeWrite::rows` projects fixed destinations; the sink groups Arrow rows by destination, uses content-derived batch IDs, retries identical slices, and submits directly to local Scribe or a ready Scribe peer. Permission decisions complete synchronously before the operation proceeds, audit staging cannot fail or delay the request, and audit delivery failures never become public request errors. Abrupt death/deadline can lose unstored memory, with loss metrics.

Source changed `boot/mod.rs::compose_bifrost`, `state.rs::BifrostComposition`, and adds `bifrost/gate_audit.rs::OutboxGateAudit` so Gate and Oracle share `OracleQueryAudit`. That is source's OLD queue-to-SQL-staging audit backend, not main's backend. Do not take its `audit_outbox` field/type/constructor or resurrect `oracle/query_audit.rs` and `audit/publication.rs`. Main's blanket `impl<T: AuditStage> GateAudit for T` in `vala-bifrost-redux/src/gate/mod.rs` and `OracleAudit for T` in `oracle/mod.rs` already do the direct composition without a server adapter. Apply source Gate/Oracle engine changes while retaining those blanket adapters and `Arc<ScribeOutbox>` wiring. Keep lower service/auth crates holding `AuditStage`, not server-specific sinks.

Evidence commits main: `f7b962d9e` one outbox for capture/results; `70a6f4a4e` audit decisions on shared Scribe outbox; `ce56636ba` retry the identical failed slice; `6aaccebf9` outbox routing repair; `dd14c0436` bounded frames and peer timeout on boot-selected routing.

## 3. SQL audit staging is an explicit modify/delete conflict, not useful source infrastructure

Source modifies `crates/vala/vala-sql/src/queries/audit_staging.rs` and `tests/pg_audit_staging.rs` after base; main deleted those runtime modules and tests. Source's `tables/audit/projection.rs` still imports `AuditStagingRow`, validates contiguous sequences/hash chains, and derives IDs from a frozen SQL range. Main's migration `20261008000000_drop_audit_staging.sql` removes staging/chain/publication state. Resolve the source edits to those deleted runtime surfaces by preserving deletion. Keep historical migrations and main drop migration in chronological history; do not edit applied migrations just to mimic the new model. Source new Bifrost migrations are independent and must coexist with main identity/credential/audit migrations.

Catalog source adds/reworks registration but still describes/appends audit in `register_dataset` transactions. Retain its registration lock/type/fingerprint semantics and remove its obsolete audit parameter/write dependency, following main `bifrost/service.rs` permission stage-before-operation pattern. Keep ALL main catalog/route error paths nonblocking for audit. Remove source `IngestError::AuditUnavailable`/catalog audit-unavailable plumbing instead of recreating public codes main retired (`f451d52be`). A missing composed sink is an internal composition defect; failed audit IO is never an operation error.

## 4. Audit table needs a deliberately combined schema

Main `tables/audit/audit_log.rs` has 11 content fields, no `seq`, `entry_hash`, `prev_hash`. Main `project_audit_event(event, decided_at)` projects those 11 plus managed `wyrd_event_time`. Source AuditLogTable has 15 content fields: it retains the three SQL-chain fields, changes `detail` to Variant, and adds typed `created_at` as its time column, with `daily_layout(CREATED_AT, [created_at desc, seq asc], ...)`.

The merged schema must retain main authorization-decision fields and no chain columns, take source Variant encoding for `detail`, and use explicit typed decision timestamp `created_at` (Timestamp microsecond UTC) as declared content/time column. This is 12 content fields before source's remaining physical envelope. Project main `AuditEvent` directly with its already-captured `decided_at`; do not use ingestion clock as decision clock, SQL stamps, `AuditStagingRow`, or synthetic sequence/hash fields. Sort by `created_at` without source `seq` tie-break. Preserve `CorrelationPolicy::None`, event's audit principal/Card/credential content, SYSTEM_OWNER exception only for audit table, and source time/schema contracts. Audit Card reference is identity content (`audit_card_ref`), not the retired observation correlation `card_ref`; do not blindly erase it via a name-ban.

Retained-history consumers and tests need updates: SQL and UI/time predicates must select `created_at` rather than old managed time; detail consumers must extract Variant canonical JSON instead of assuming StringArray. Source SQL-chain tests cannot replace main retention journeys. Main `wyrd-testing/tests/bifrost/server/audit_retention.rs::retained_history_carries_both_credential_shapes` explicitly proves `seq` is absent; keep that invariant. Main source schema test `projects_canonical_content_with_the_decision_time` must assert new declared timestamp/Variant detail and all identity columns.

## 5. Keep six-destination internal peer writes and their trust rules

Main `grpc/capture_peer.rs::ScribeCapturePeerGrpc::batch` accepts audit, two capture, three result destinations; validates Verifier attribution on result tables and forbids it on audit/capture. SYSTEM_OWNER is permitted ONLY for audit. Main `scribe_outbox.rs::ScribeBatch::into_frame` chooses reserved PLATFORM_AUDIT_PRINCIPAL, GATEWAY_CAPTURE_PRINCIPAL, or tenant System principal; result Card scope is exactly UID-bearing Verifier. Public Gate refuses result-table writes and gateway.calls, independent of wildcard permission.

Source `grpc/capture_peer.rs` remains base's two-capture-table service and source `verification/publisher.rs` remains token-based result publication. Normal merge should retain main peer service; taking source composition/proto wholesale will sever the six-table outbox path. Preserve main proto `VerifierResultAttribution`/optional verifier field and six-table validator/test, adapt only Scribe frame construction to source engine. `ScribeIngressFrame` in source lacks main `attributed_cards`; retain/port that necessary validated attribution capability in source ingress instead of weakening scope checks or fabricating scopes from untrusted public input. Keep internal RPC only on peer mTLS listener; never mount on tenant public listener.

Main `ScribeRoute::select` chooses peer routing using membership + TLS even on pods without Oracle. `ScribePeers::channel` rotates sorted ready Scribes per attempt, invalidates obsolete endpoint channels, and applies a 30-second RPC timeout. Main sink splits frames by rows within one-quarter Scribe request ceiling, retries same frame IDs on a lost ACK, treats unsplittable-row terminal rejection as counted loss, and writes later summaries. Retain these protections with source request/accounting/partition limits. The source engine's new partition fanout and time-window validation must be audited against audit rows delayed by backlog; audit retention must not accidentally become a terminal time-window drop merely because decision time predates retry.

## 6. Source Card scope implementation would regress main unbound writers

Source Gate removes main `gate/attribution.rs`, `with_card_registry`, validated `attributed_cards` on ingress, and UID-only rules. Source `record_write_verdict` allows System principals writing result tables using `require_frame_card_scope(card_ref)`; main rejects EVERY public result-table write and uses tokenless private writer. Preserve main decisions and unbound attribution while adapting source LogicalIdentity/Variant processing.

Main `CardRegistry::attributed_cards` resolves at most 32 distinct `card_uid`s in caller tenant via RLS and includes only observation-target kinds; Card-bound scope checks remain signature-bound/local. `native_card_uids`/OTLP signal collectors/`Principal::card_attribution` must work on source's schema and Variant projection without restoring card_ref or treating missing registry as allow-all. Tests must cover User and unbound Service, valid own-tenant registered UID, cross-tenant/missing/non-observation UID, bound out-of-scope UID, and gateway/evaluation attribution. Commits `b5ab2dd5d`, `8f65cb716`, `dadaa3ab9`, `c8febbd49` establish these main rules.

## 7. OTLP static API-key authentication is easy to lose at the Gate seam

Main `71fe71c35` replaced Argon2 credential verification with SHA-256 for high-entropy generated secrets and admits stock OTLP exporters via `x-wyrd-api-key` on HTTP/gRPC. Preserve main `wyrd-auth-issue` secret functions; `wyrd-auth::ExchangeApiKey::authenticate`, `TenantTokenIssuer::verify`, `components/auth/routes.rs::TokenGrants::api_key_principal`, `components/auth/token_extract.rs::WYRD_API_KEY_HEADER`, HTTP OTLP caller, and gRPC `OtlpGrpcAuthentication` adapter. Source has no post-base changes to these server auth files; keep them while merging its HTTP docs/schema/transport edits.

But source Gate/auth changes lack main `AuthContext::from_verified` and Gate `admit_otlp_verified`, which those surviving server adapters invoke. Restore those methods on the source Gate engine. Authentication still runs before payload decode, derives tenant from verified principal/credential, retains permission/Card/credential attribution, and uses access-token path when that header is present. API-key entrance remains OTLP-only, never a global acceptance route. Keep trace/log/metric service constructors accepting AppState so credential auth still has its owners.

Migration `wyrd-sql/migrations/20261006000000_sha256_credential_verifiers.sql` revokes old Argon2-shaped tenant and platform credential rows; do not reintroduce source/base Argon2 verifier or omit migration. Existing credential holders must reissue by the already-adopted main behavior. No new credential migration is warranted merely by merging Bifrost.

## 8. Main identity and grant lifecycle survives but must be proven through changed integration

Retain main OAuth code/device grants, tenant/platform plane separation, human connection lifecycle/sealing changes, token revocation behavior, direct role assignment/source semantics, built-in viewer/workload/editor/admin roles, reserved internal identity rejection, principal/credential attribution, RLS abstractions, and operator bootstrap. Source's unchanged pre-base auth must not replace these files. Source diffs touch only `wyrd-sql/src/queries/verifier_runs.rs` within wyrd-sql, so auth query/migration replacement has no merge justification.

Keep main boot composition supplying audit stage to auth issuers, principal/card/gateway/platform owners. Source boot wholesale picks can break constructor wiring or begin_platform_audited dependencies even when lower auth files merged correctly. `6c23a6f45` externally supplied Postgres credentials/default local storage and latest CLI/server bootstrap belong to main and are not source Bifrost geometry decisions.

## 9. Research uncovered a current-main attribution gap to close during integration

Main `gate/mod.rs::impl<T: AuditStage> GateAudit` builds `AuditEvent::new` but never assigns `auth.principal.credential_id`; `AuditEvent::new` in `wyrd-spec/src/vala/api.rs` defaults `credential_id: None`. Main Oracle `oracle/mod.rs::query_audit_event` has same omission for `context.principal.credential_id`. Source server `audit/mod.rs::attributed_event` explicitly carries credential_id, but restoring its SQL-backed infrastructure is unnecessary. Adapt main event builders to keep credential_id, with a focused recording sink test and real credential→Gate/Oracle→retained-history proof. Existing retention journey only stages already-populated AuditEvent, so it does NOT prove these event builders preserve identity. This gap exists in current main; merging source is an opportunity to preserve required semantics, not proof source fixes every path (source Oracle query builder also uses AuditEvent::new).

## 10. Acceptance evidence

Existing main unit tests in `scribe_outbox.rs`: `a_peer_route_needs_no_oracle`, `an_oversized_detail_reaches_scribe_in_bounded_frames_that_replay_identically`, `an_unsplittable_row_is_submitted_whole_and_terminal_refusal_consumes_it`, `a_silent_peer_times_out_and_the_slice_retries_on_the_next_scribe`, `scribe_refusals_and_peer_codes_classify_alike`. Keep these with source geometry; use exact nextest expressions through `mise exec --`, not positional filters.

Main retention journeys `retained_history_carries_both_credential_shapes`, `system_owner_security_rejections_retain_once`; peer destination validator `requests_are_confined_to_tenant_capture_destinations`; existing auth identity/principal/card scope journeys remain primary contract. Add/adjust proof for explicit created_at/Variant detail, nonblocking audit refusal behavior, actual Gate/Oracle credential attribution, source Scribe retry dedup + outbox partial ACK, API-key OTLP all signals, and unbound UID attribution.

Repository lanes inspected: `mise run test:principals:unit`, `test:principals:integration` (includes served OpenAPI, auth issuance/exchange/refresh/revoke, principal roles, SQL principals), `test:platform:journey`, `test:bifrost:journey:otlp`, `test:bifrost:journey:server`, `test:bifrost`, `codegen:check`, `check:deps`, `check:tenant-isolation`. The overall cross-boundary integration likely needs broad gate as determined by root; avoid rerunning its included lanes merely as duplicate final evidence. No lane was executed during read-only research.

## 11. Remaining uncertainty

The resulting merged source engine does not yet exist, so memory accounting of grouped outbox Variant frames, source 32-partition fanout against arbitrarily delayed audit batches, Variant detail query decoding, and private result stamping require implementation-stage tracing/journey verification. This report establishes source invariants and incompatible seams; it does not claim compile/behavior proof.

## 12. Operational caller checklist and historical-ledger correction

`boot/auth.rs` and `boot/issuer.rs` are unchanged by source from base, so main trusted-issuer/human identity provisioning/resolver changes should survive mechanically. Source `boot/mod.rs` changes extensively; merge its Bifrost build/resource runtime with main `ServerAuth`, `ServerAuthz`, ScribeOutbox/AuditStage constructor dependencies, role routing and membership owner. Main `state.rs::AppState::new` must share the boot-composed outbox, never create a separate production queue; ownerless test shell is its sanctioned special case.

Source `config.rs` edits are Bifrost time/deadline/resource geometry (removes ingest event-time window fields; adds max query deadline, scan bytes and retention validation), not auth provider configuration. Keep those Bifrost changes with main external Postgres credentials/local storage/operator bootstrap configuration. Ensure deployment examples and tests satisfy source retention >= maximum query deadline without changing tenant token expiry or credential policy.

Source `wyrd-testing/src/release_server.rs` adds Host/Placement/Meter and optional systemd-scoped process launch; its setup path still runs `wyrd-server setup --tenant`, captures the first platform credential, supplies it on later tenants, records tenant admin API keys, and launches the release server. Preserve useful source host benchmarking but reconcile main release setup's external database login/env handling. Do not restore old auto-created database credentials just to run a benchmark. This source harness conflict belongs to the infra specialist for full caller/geometry analysis; identity smoke should prove setup first/subsequent tenant provisioning and keyed OTLP using captured tenant credentials.

No audit chain is reconstructed at read-time on current main. `server/audit/mod.rs` has no hash function, only event construction/staging; stale prose says historical rows hash identically but does not implement a chain. `audit_detail_canonical_json` continues deterministic detail encoding, and ScribeOutbox has content hashing for replay identity; neither implies seq/entry_hash/prev_hash in retained audit. Historical ledger L01's suggestion 'audit hash owner now server audit/mod.rs' is factually stale; L51 SQL entry_hash helper is obsolete, not an item to relocate. Keep canonical JSON round-trip/attribution proof while discarding retired chain integrity tests.

## 13. Workload migration collision: preserve main deletion, do not renumber

Source `wyrd-sql/migrations/20261002000000_workload_role.sql` is already present and identical at merge base; it is not source-added relative to base. Main deleted/renumbered it during merges, then `035be3105` intentionally deleted `20261002000100_workload_role.sql` when replacing built-in Roles. Main now uses numeric 20261002000000 for device-authorizations migration. Keep main identity migration and workload migration deletion; no redundant SQL port/renumber.

Exact source SQL inserts workload for admin-bearing existing tenants, using gen_random_uuid and ONLY bifrost_table:read, bifrost_record:write, bifrost_query:read. Main `builtin_roles.rs::WORKLOAD` includes viewer read grants plus bifrost_table write, eval run, verifier run, workflow run, trigger write, operator invoke, gateway invoke; `DEFAULT_CARD_ROLE` is workload. Main `seed_builtin_roles_for_tenant` serializes that authority and uses `builtin_role_uuid(tenant, role.name)`, ON CONFLICT DO NOTHING. Source's old SQL would poison a missing workload row with older narrow permissions and random ID; rerunning main seed would leave it unchanged. Therefore retain current main role authority/provision/repair behavior. Existing deployment migration-history reconciliation remains a separate operational obligation and does not justify reintroducing obsolete source role DDL.


---

## Archived snapshot: wyrd-merge-engine.md

# Bifrost engine merge evidence

Pinned refs: main `8a8813a7458b7c98707c7b4f9b9d403f604ee0e6`; source `cfe21dc9007d5d22d9ae4411be07fb5504b0cc05`; merge base `5ab92b0039404565e51f4a246863ce90cbd8d11f`. No .codegraph directory exists in supplied checkout. Read-only Git comparisons and temporary extraction; no repository edits or tests. This is evidence for root synthesis, not independent authority for final merge decisions.

## 1. Source is a coherent engine replacement, not a small Variant patch
Source's changed engine files include catalog cut_cache/layout/physical types, Oracle native scans/session/memtable_source/nested_pushdown/scan_sizing, Scribe conformance/preprocess/material_plan/WAL admission, Parquet variant_residual/object_uploader, ObjectStore storage, Forge native rewrite path and v3 row lineage. Source deletes gate/attribution.rs, oracle/analytical_scan.rs, oracle/pruning.rs, scribe/fixed_ipc.rs, storage/cache.rs, storage/telemetry.rs. Source commits `740975234`, `029da306f`, `4559cff37`, `509f972c6`, `cfe21dc90` show these are intentional replacement owners. Restoring deleted modules to resolve imports reconstructs old owners and defeats source architecture. However gate attribution deletion collides with main's newer identity corrections; rehome those semantics in the source decoder rather than revive a parallel stamping pipeline (other scout owns full identity details).

## 2. Oracle reader protection deliberately changes from per-query SQL claims to retention
Main `catalog/bifrost_catalog.rs`, `oracle/planner.rs::ActiveReadClaim`, `oracle/mod.rs::ClaimedSqlCut`, query-stream/drop owners pin active SQL reader rows. Main Forge table-authority exclusion prevents expiration or cleanup while live readers hold claims. Source removes that workflow. Its migration `crates/vala/vala-sql/migrations/20261009000000_bifrost_snapshot_retention.sql` drops `oracle_acquire_table_cut` and `oracle_active_table_reads`, introduces `oracle_read_table_cut(uuid,jsonb)` (one STABLE read statement), and tenant-scoped `forge_snapshot_supersessions`. Source `forge/expire.rs::gather_expiry_protections` records first observation of each replaced snapshot and passes `retained_snapshot_ids` into expiry policy. Source `forge/table_authority.rs` and SQL `BifrostTableMaintenanceAuthority` now serialize destructive Forge protocols only. Therefore do not restore main's active-reader checks, reader-release assertions, or associated old schema-contract requirements verbatim. Port safety semantics into source retention architecture and rewrite affected journeys accordingly. Query drop/revocation must still end actual work, despite having no SQL claim to release.

Safety invariant from source: retention >= maximum query lifetime; first supersession observation is never earlier than replacement, so expiration is conservative. `ForgeConfig::snapshot_retention` defaults 6h, `OracleConfig` defaults 2h/default and 6h/max, `crates/wyrd/wyrd-server/src/config.rs:3168` rejects retention below Oracle max on every deployment target (including separated Oracle/Forge). Source `OraclePlanner::request_deadline` silently clamps positive requests to max; main accepts valid positive requested budgets uncapped and supplies stable QueryInvalidDeadline. These behaviors cannot simply coexist: finite retention does not protect unbounded queries. Explicit reject over max preserves source safety with a clear caller failure, while retaining truly uncapped budgets requires restoring independent per-query protection or equivalent dynamic horizon. Root must make this observable contract decision, propagate generated docs/clients/HTTP/MCP tests, and prove no silent truncation.

## 3. Source cut cache adds a new correctness boundary
`catalog/cut_cache.rs::CutCache` caches `(tenant, namespace, table) -> pointer + unresolved hot candidates`, only while LISTEN is live; disconnect/malformed notification clear entries; generations suppress fills raced by invalidation. `20261010000000_bifrost_cut_notify.sql` installs file_list invalidation. Forge terminal rewrites explicitly notify; source `app/server.rs:609` starts catalog.watch_cuts. Source `OraclePlanner::pin_cut` supplies `deadline.checked_sub(config.max_deadline)` as not_before. This binds freshness to snapshot lifetime, not a TTL. Retain tenant-qualified cache keys, same cut tuple, publication-commit notifications and listener lifecycle; never cache just Iceberg pointer independently of hot candidates.

Source `BifrostCatalog::read_cut_cached` timestamps cache fill at response completion (`Instant::now()` after `read_cut`). Module premise says pointer was current at read_at. Potential source-only latent risk: DB statement snapshot is taken before response completion, so a delayed statement can return a pointer replaced before recorded read_at. Source NotFound retry compensates missing metadata during materialization but does not prove already-pinned data cannot expire later. Inspect/prove statement-start timestamp bound (or use conservative start instant) and invalidation racing delayed responses before merging. This is an inferred safety gap, not a reproduced failure, and should be labeled separately from proven merge regressions.

## 4. Retain main's lease-bounded snapshot-expiration outcome handling without reintroducing readers
Main `0e1968570` adds `forge/expire.rs::PreparedExpiry`, `ExpiryProgress{prepared,submitted}`, `expire_under_authority`, `reload_running`, `release_unsubmitted`, `ExpiryResolution`; expiry is bounded by Forge lease TTL and exclusive table authority remains held through reset/settlement. Before submission, corroboration/load/cancellation failure resets prepared claims. After submission, timeout/retryable/unknown outcome leaves operation/task/claims Prepared for exact reconciliation. Source lacks this fix: prepares, checks shutdown, loads, calls complete_expiry, releases authority, then settles/reset; it has no lease timeout/progress flags. Source still needs lease-bound durability behavior because destructive effects and uncertain catalog acceptance remain, even when Oracle readers rely on retention. Main SQL `ForgeOperations::settle_snapshot_expiration/reset_snapshot_expiration` optional held exclusive capability avoids re-locking the caller's authority row; source signatures lack it. Port Rust+SQL API together and update source retention tests; do not retain main active-reader-exists checks that refer to dropped table. Tests: main tests/integration/forge/snapshot_expiration.rs contains timeout-before/after-submission and protection scenarios; preserve useful ones, replace SQL-reader-specific expectations.

## 5. Retain main leader-term linearization and scheduler-exit revocation
Main `a26fda2e3`/`3f591937e` changed `forge/leadership.rs::with_term` to hold slot read lock through notify_commit/pull_compaction_work/report_compaction, so set_held cannot revoke term between check and schedule effect. Source reverts to `term()->Arc<ForgeHeldTerm>` then operates after lock release. Source therefore has same check/use race as ancestor. Main `ForgeLeadership::relinquish` and `forge/scheduler.rs::ForgeRunGuard` revoke local authority on every loop exit/drop/panic before restart; source run guard only clears running flag. Preserve main methods and guard semantics, including SchedulerStopped telemetry enum, tests `schedule_use_is_linearized_with_term_replacement` and `relinquished_term_refuses_every_handler`, plus real handler races in production_routes.rs. These changes are independent of Variant/ObjectStore/retention.

## 6. Retain main same-owner Forge recovery only after proven quiescence
Main `3bf7b0d97`/`3af5bdb5b` added `forge/worker.rs::ForgeLoopQuiescence`, sharing invocation state across clones. `run` establishes reclaim_owner only if prior invocation joined all plans/heartbeats; panicked invocation does not mark joined. Startup drains claimed/running and Prepared rows before readiness. SQL `ForgeTasks::reclaim_expired_attempts(cap, previous_owner)` can immediately reclaim previous_owner's attempts, while `claim_prepared_for_reconciliation(owner, lease_seconds, previous_owner)` permits same-owner Prepared renewal only on proven quiescence. Source lacks quiescence, reclaims only expired claimed/running, but permits renewal of same-owner Prepared without the new explicit quiescence capability. Risks: clean supervised restart waits needless lease age; panic restart renews same-owner Prepared while older work may still live. Preserve full worker+SQL predicate/signature+row type+tests change, especially `only_a_joined_loop_enables_same_owner_reclaim`; adapt it to source native rewrite owner instead of swapping worker.rs wholesale.

## 7. Keep main Forge publication simplification
Main `5ff25deb6` replaces absolute publication wall deadline with initial + three definite-conflict retries (1s,2s,4s), each catalog call bounded by request timeout. Source retains `forge/publication.rs::RewritePublicationDeadline`, RewriteAttemptAuthority.deadline_passed, Deadline/DeadlineTruncated, and worker plan captures deadline *before* rewrite. Slow legitimate rewrite can consume the whole publication budget before first submit. This conflicts with user instruction to retain main simplifications. Carry main removal through source worker and `ForgeRewriteCommit`; preserve source v3 row-lineage/Variant output behavior and exact ambiguous-acceptance reconciliation. Sibling plans still commit independently and no unknown commit is resubmitted.

## 8. Main Oracle memory shutdown fixes are absent source and must be adapted to new executor
Main `71258cc8c` and `51dbfc1fd`: `oracle/admission.rs::OracleAdmission::shutdown` waits both active-query settlement and Oracle-only reserved bytes; source waits only active_queries and reports `resources.shared_memory_reserved()` including Forge. Main resources tracks `oracle_infallible_headroom_bytes` separately from governed Oracle bytes, adds shared-root Notify for release, excludes Forge from Oracle drain. Source has lock-free governed-memory changes (`2f9e2889f`) and LiveConsumers names (`ed9f3bf9a`) but no Oracle headroom subtotal. Port counters/waits semantically to source governor, not old pod-wide operation lock. Preserve exact release/accounting and wake ordering. Main unit tests `oracle_shutdown_drains_only_oracle_memory` and `oracle_shutdown_drains_oracle_infallible_headroom` directly cover independent Forge live memory and late Oracle children.

Main `6aaccebf9`/`c63e2ff46` made `OracleQueryResources::release` return slots while child-owned memory view remains valid; source still poisons on any surviving reserved child and waits graph nested idle. Source pins a newer datafusion-distributed fork with partition-stream release (`63b6df251`), so it may eliminate some former leak paths, but source shutdown-wide accounting remains wrong independently. Need prove new executor cleanup under early error, stream abandon, cancel, late child, zero-byte registered child and infallible headroom before choosing retention of nested_idle bookkeeping. Main count removal is a simplification; source diagnostic names are useful only if source graph still must explicitly wait. Distinguish fork lifecycle proof from copying old graph code.

## 9. Main flush snapshot race fix is absent source
Main `6aaccebf9`: `scribe/assembly.rs::OwnedMembers/owned_members/owns_any`, `scribe/staging_runtime.rs` mirrors methods; `PersistenceWorker::publish_residue` snapshots every unpublished ready OR claimed member before ready sweep, `ClaimSlotWait::Await(&snapshot)` waits if a concurrent lifecycle publish_due owns any snapshot member even with no exhausted slots. Source retains old Await with no snapshot and can return flush while other publisher still owns rows flush promised. Preserve fix across assembler/staging/persistence with source's raw-byte assembly and ObjectStore publisher; source 959e3431f no-reupload-on-recovery fix is separate and also must survive. Main table-scoped PersistenceFaults, staging_backlog_for_test(table) and probe scoping prevent newly continuous audit rows stealing test injections; port harness consumers too. Useful test main staging_runtime::tests::owns_any_tracks_only_snapshot_members and persistence concurrent flush held claim tests.

## 10. Source Scribe durable pipeline changes form one group
Source commits `cfe21dc90`, `88f6e0657`, `84d4f3559`, `255a6e0ec`, `959e3431f` alter batching/fsync, compressed WAL payloads, raw staging trigger and recovery immutability. Last source commit uses one fsync for group SLICE+COMMIT before recording batch fences/ACK. New `scribe/wal.rs::NewestFence`, `FenceLookup`, `ShardRecordCursor::end_at_bad_record` permit truncating only final segment's unacknowledged bad record; if newest durable fence is at/beyond bad record, fail-stop. New SQL `newest_wal_fence` and `20261011000000_scribe_wal_fenced_lsn.sql` expose stream/shard position through scoped definer. Keep source WAL+replay+ingress+SQL+crash tests inseparable; staging/group buffer cannot ACK any member before fsync and fence commit. main shared outbox expects idempotent exact batch fence and typed ingest receipts, which remain load-bearing.

WAL record/header remains v6; compression is Arrow IPC built-in metadata, not separate undocumented WAL framing. Historical uncompressed IPC should still decode through Arrow's stream reader; prove with a main-produced WAL fixture rather than presume dependency compatibility. Source main schema fingerprint/time-column/card_uid changes alter logical rows independently; full migration must drain old Scribe and outbox before schema switch or explicitly reconcile old schema buckets. Source runtime recovery no longer reuploads published objects: publication_state must be checked before upload; Committed verifies bytes only, absent uploads, partial/conflicting fails closed retaining manifest. Do not regress this with main's newer flush patch.

## 11. Main query bind parameters and stable errors disappear in source
Main `82972f9f9`/`b620b069f`: public BifrostQueryRequest.params and Oracle physical planning `.with_param_values(bind_values(params))`, function mapping typed QueryParam to ScalarValue. Source oracle tree contains no with_param_values, bind_values, QueryParam or request.params. Reinsert typed binding before optimize/classify on source native plan, carry through forwarding/client wire, reuse server validation and add each SDK journey. Bound values must not leak into audit logs or plan SQL interpolation.
Main `aa0aeb52b`: public QueryInvalidDeadline error shared across entrypoints. Source OraclePlanner::validate_query maps all request.validate failures to QueryInvalidSql; stable code preservation must be ported alongside retention deadline policy. Source supports tagged late terminal problems (`70142ba09`, `929ce7771`) and peer envelopes; keep those instead of flattening to legacy execution errors.

## 12. SQL identity and migration ownership cannot take source whole files
Main rewrites historical Vala migrations to single externally provisioned login plus tenant/operator RLS policies. Source retains hardcoded wyrd_app/wyrd_platform_admin grants, narrower old login checker, and removed audit staging queries/migrations. Source `ValaPostgres::validate_schema` requires exact wyrd_app; main uses verify_tenant_pool with shared_login from ResolvedDsns. Retain main construction/login policy; add source new migrations using main role/owner conventions. Do not restore source audit_staging.rs/row_types/tests, or remove main drop-audit-staging migration.

New source `ValaPostgres::conn()` exposes naked PoolConnection and source `OracleTableCuts::read`/`newest_wal_fence` accept raw PgConnection, conflicting with current AGENTS raw-connection prohibition. Need earned tenant-bound autocommit owner rather than export unrestricted pool borrow, or use existing TenantConn initially (additional BEGIN/COMMIT correctness cost beats boundary bypass). Source `CutCache::watch` accepts raw PgPool; retention migration definer/function privilege and new supersession RLS must fit main externally provisioned roles. Check mixed-role session state restoration and operator grant behavior with real tenant-isolation tests.

Historical migrations changed by both branches cannot be blindly overwritten once applied: sqlx checksums fail. Distinguish fresh-dev rebuild from supported upgrade. Preserve selected historical main bytes and append new DDL for source runtime changes wherever schemas actually change. Source 20260619000001 only changes layout comment, not JSON rewrite: existing stored physical_layout has three keys while new owner expects time_column; reconcile existing table metadata/registration explicitly, avoiding fingerprints silently matching incompatible built-ins. Database rollback cannot restore dropped active-read rows after source serving starts; old and new engine nodes should not overlap without explicit cut-over protocol.

## 13. Storage engine replacement and narrow shared-storage seams
Source Bifrost `storage/mod.rs` shrinks to one Arc<dyn object_store::ObjectStore>, LimitStore, native DataFusion FileMetadataCache charged up front once to Oracle metadata owner; old Wyrd payload cache/governed I/O wrappers deleted. `catalog/iceberg_storage.rs::BifrostIcebergStorage` adapts validated warehouse-relative keys to ObjectStore and rejects serialization. `ForgeObjectStore` now exposes ObjectStore read/list/delete/metadata and objects for native Iceberg rewrite; cloud builder adopts object_store timeout/retry configs and existing BackendConfig. Retain source ownership, credentials through main configuration, local default from main 6c23a6f45, bounded transfer buffers and cache charge. Native ObjectStore NotFound classification must remain typed through catalog/planner retry, not string match or any-backend fallback.

Outside Bifrost source introduces only narrow public helpers needed by new builders: `crates/wyrd/wyrd-storage/src/factory/gcs.rs::service_account_json` decodes existing credential env representation; `factory/mod.rs::pool_idle_timeout` made public. Carry helpers onto main credential/config implementation. Source also changes LocalSigner methods to sync, introduces boxed SignerFuture across BackendSigner, and adapts telemetry test futures; these changes are NOT required by Bifrost's ObjectStore adapter (it calls factory helpers only). Keep main storage workflow/signing unless root chooses these independent simplifications with their own checks. Shared-storage service/settings/sweeper other differences largely main advancements. `crates/shared/wyrd-bench/src/lane.rs` source only changes `.ok().is_some_and` to `.is_ok_and`, no behavior/benchmark-contract need; main can stay.

## Verification evidence requirements
No tests executed by this research. Integration gate must cover source retained Variant nested list/struct, all built-ins and dynamic types, ingest schema conformance by name, group-fsync crash/restart and replayed batch fence, flush waiting concurrent publisher, object-store existing publication read-only recovery, v3 lineage after repeated Forge rewrites/GC, expiry before/after submission with retention authority, term replacement and scheduler failure/restart, same-owner quiescence, Oracle cancellation/abandon/late child shutdown, SQL bind parameters and deadline code on every first-class SDK/MCP/HTTP surface. Run current main gate for shared CI/dependency/migration infra and scoped source-compatible test:bifrost; static evidence does not establish deployment upgrade success.

## Follow-up: peer configuration and watcher cancellation
Source server config validates retention >= max query deadline in each process only. `max_query_deadline_ms` is operator/environment configurable (`config.rs:2869`); there is no matching deadline/retention comparison in Oracle peer/codec/follower/analytical modules. Thus a cluster deployed with different configuration files needs a deployment invariant `min(Forge retention across all table-maintaining nodes) >= max(admitted Oracle query lifetime across all entrypoint nodes)`. Separate-role nodes each validating their own unrelated Oracle default does not prove this. Make the cluster operator setting explicit and prove mismatched configurations are refused or cannot be deployed; avoid claiming local boot tests provide cross-node proof. Raising max before raising retention everywhere risks readers; lowering retention before draining previously admitted longer queries likewise risks readers. Rollout should raise retention before raising accepted deadline, and lower max/drain before lowering retention.

Membership scout checked normal watcher shutdown: `CutCache::watch` calls reset(false) when cancellation breaks listener loop and on final exit. reset clears cuts, increments generation and disables hits; stale in-flight fill is suppressed. Cancelling watcher before query drain therefore falls back to SQL rather than freezing cache. Preserve this behavior in main supervision wiring.

Main deadline code retained on explicitly oversized requests is a new bounded-deadline behavior compared with uncapped main; update public contracts/docs/defaults/examples and exact HTTP/MCP/SDK parity. Do not present it as only an internal engine fix.

## Superseding authority correction from user clarification
The user explicitly confirmed source Bifrost behavior wins, including source snapshot retention, maximum query lifetime, and oversized-positive-deadline clamp. This supersedes any earlier option suggesting explicit oversized-deadline rejection or restoration of main uncapped budgets. Preserve source OraclePlanner::request_deadline clamp/default/max behavior and source retention model. Keep main stable error improvements only where they match source request semantics; do not invent a new rejection for a source-accepted clamped request. Document and test the resulting source behavior across SDK/HTTP/MCP surfaces.

Compose main compatible memory accounting/shutdown, Forge term/lifecycle/quiescence/durable-outcome and flush safeguards onto source owners. Source remains owner of native scans/executor, WAL durability, storage, Variant representations, and retention; main reader pins and older functional owners do not return. Where the earlier report describes a main simplification (e.g. retry-count rather than absolute rewrite deadline), root must apply clarified authority: source behavior wins unless specifically accepted as a compatible main safeguard; do not silently override source semantics from a generalized main-fluff preference.

The inferred source-only cut-cache timing and cross-node max/retention consistency concerns are separate follow-up research observations, not authorized merge architecture edits. They must not expand the integration into an unsolicited Bifrost redesign.

The user also explicitly removes `vala.dev.agent_traces`; source reintroduction must be dropped across definition, eager provisioning, fixtures/tests, generated contracts and docs as applicable.


---

## Archived snapshot: wyrd-merge-inventory.md

# Pinned inventory

Main `8a8813a7458b7c98707c7b4f9b9d403f604ee0e6`; source `cfe21dc9007d5d22d9ae4411be07fb5504b0cc05`; merge base `5ab92b0039404565e51f4a246863ce90cbd8d11f`. Remote ls-remote matches as of 2026-10-11.
Source 527 changed records (531 paths including rename origins); main 2170 changed records (2250 paths); overlap 228; direct conflict paths 148.

## Every source changed record

|Status|Paths|Main also changed any path|Direct conflict on any path|
|---|---|---|---|
|M|.agents/skills/wyrd-task-review/SKILL.md|True|False|
|M|.claude/skills/wyrd-task-review/SKILL.md|True|True|
|M|Cargo.lock|True|True|
|M|Cargo.toml|True|True|
|M|architecture/bifrost-design.md|True|True|
|M|architecture/references/domain/datafusion.md|False|False|
|M|architecture/wyrd-design.md|True|False|
|A|changes/active/bifrost-variant/architecture_ref.md|False|False|
|A|changes/active/bifrost-variant/diagnostics/README.md|False|False|
|A|changes/active/bifrost-variant/diagnostics/fixture-fixes-findings.md|False|False|
|A|changes/active/bifrost-variant/diagnostics/fixture-fixes-prompt.md|False|False|
|A|changes/active/bifrost-variant/diagnostics/observe-run-findings.md|False|False|
|A|changes/active/bifrost-variant/diagnostics/observe-run-prompt.md|False|False|
|A|changes/active/bifrost-variant/merge-integration-into-task-003.md|False|False|
|A|changes/active/bifrost-variant/review/TASK-001-r1/TASK-001-R1-close-variant-contract-gaps.md|False|False|
|A|changes/active/bifrost-variant/review/TASK-001-r1/domain-review-iceberg-durability.md|False|False|
|A|changes/active/bifrost-variant/review/TASK-001-r1/domain-review-oracle-datafusion.md|False|False|
|A|changes/active/bifrost-variant/review/TASK-001-r1/domain-review-sdk-parity.md|False|False|
|A|changes/active/bifrost-variant/review/TASK-001-r1/domain-review-security-tenancy.md|False|False|
|A|changes/active/bifrost-variant/review/TASK-001-r1/domain-review-variant-arrow.md|False|False|
|A|changes/active/bifrost-variant/review/TASK-001-r1/findings-validation.md|False|False|
|A|changes/active/bifrost-variant/review/TASK-001-r1/followup-review.md|False|False|
|A|changes/active/bifrost-variant/review/TASK-001-r1/maintainer-review.md|False|False|
|A|changes/active/bifrost-variant/review/TASK-001-r1/standards-review.md|False|False|
|A|changes/active/bifrost-variant/review/TASK-001-r1/system-review.md|False|False|
|A|changes/active/bifrost-variant/review/TASK-001-r1/task-review-behavior.md|False|False|
|A|changes/active/bifrost-variant/review/TASK-001-r1/task-review-invariants.md|False|False|
|A|changes/active/bifrost-variant/review/TASK-001-r1/verdict.md|False|False|
|A|changes/active/bifrost-variant/review/TASK-001-r2/TASK-001-R2-exact-integers-and-late-errors.md|False|False|
|A|changes/active/bifrost-variant/review/TASK-001-r2/domain-review-iceberg-durability.md|False|False|
|A|changes/active/bifrost-variant/review/TASK-001-r2/domain-review-oracle-datafusion.md|False|False|
|A|changes/active/bifrost-variant/review/TASK-001-r2/domain-review-sdk-parity.md|False|False|
|A|changes/active/bifrost-variant/review/TASK-001-r2/domain-review-security-tenancy.md|False|False|
|A|changes/active/bifrost-variant/review/TASK-001-r2/domain-review-variant-arrow.md|False|False|
|A|changes/active/bifrost-variant/review/TASK-001-r2/findings-validation.md|False|False|
|A|changes/active/bifrost-variant/review/TASK-001-r2/followup-review.md|False|False|
|A|changes/active/bifrost-variant/review/TASK-001-r2/maintainer-review.md|False|False|
|A|changes/active/bifrost-variant/review/TASK-001-r2/standards-review.md|False|False|
|A|changes/active/bifrost-variant/review/TASK-001-r2/system-review.md|False|False|
|A|changes/active/bifrost-variant/review/TASK-001-r2/task-review-behavior.md|False|False|
|A|changes/active/bifrost-variant/review/TASK-001-r2/task-review-invariants.md|False|False|
|A|changes/active/bifrost-variant/review/TASK-001-r2/verdict.md|False|False|
|A|changes/active/bifrost-variant/review/TASK-001-r3/TASK-001-R3-close-authority-style-and-distributed-errors.md|False|False|
|A|changes/active/bifrost-variant/review/TASK-001-r3/domain-review-iceberg-durability.md|False|False|
|A|changes/active/bifrost-variant/review/TASK-001-r3/domain-review-oracle-datafusion.md|False|False|
|A|changes/active/bifrost-variant/review/TASK-001-r3/domain-review-sdk-parity.md|False|False|
|A|changes/active/bifrost-variant/review/TASK-001-r3/domain-review-security-tenancy.md|False|False|
|A|changes/active/bifrost-variant/review/TASK-001-r3/domain-review-variant-arrow.md|False|False|
|A|changes/active/bifrost-variant/review/TASK-001-r3/findings-validation.md|False|False|
|A|changes/active/bifrost-variant/review/TASK-001-r3/followup-review.md|False|False|
|A|changes/active/bifrost-variant/review/TASK-001-r3/maintainer-review.md|False|False|
|A|changes/active/bifrost-variant/review/TASK-001-r3/standards-review.md|False|False|
|A|changes/active/bifrost-variant/review/TASK-001-r3/system-review.md|False|False|
|A|changes/active/bifrost-variant/review/TASK-001-r3/task-review-behavior.md|False|False|
|A|changes/active/bifrost-variant/review/TASK-001-r3/task-review-invariants.md|False|False|
|A|changes/active/bifrost-variant/review/TASK-001-r3/verdict.md|False|False|
|A|changes/active/bifrost-variant/review/TASK-001-r4/TASK-001-R4-close-final-variant-contract-gaps.md|False|False|
|A|changes/active/bifrost-variant/review/TASK-001-r4/domain-review-iceberg-durability.md|False|False|
|A|changes/active/bifrost-variant/review/TASK-001-r4/domain-review-oracle-datafusion.md|False|False|
|A|changes/active/bifrost-variant/review/TASK-001-r4/domain-review-sdk-parity.md|False|False|
|A|changes/active/bifrost-variant/review/TASK-001-r4/domain-review-security-tenancy.md|False|False|
|A|changes/active/bifrost-variant/review/TASK-001-r4/domain-review-variant-arrow.md|False|False|
|A|changes/active/bifrost-variant/review/TASK-001-r4/findings-validation.md|False|False|
|A|changes/active/bifrost-variant/review/TASK-001-r4/followup-review.md|False|False|
|A|changes/active/bifrost-variant/review/TASK-001-r4/maintainer-review.md|False|False|
|A|changes/active/bifrost-variant/review/TASK-001-r4/reuse-review.md|False|False|
|A|changes/active/bifrost-variant/review/TASK-001-r4/root-cause.md|False|False|
|A|changes/active/bifrost-variant/review/TASK-001-r4/standards-review.md|False|False|
|A|changes/active/bifrost-variant/review/TASK-001-r4/system-review.md|False|False|
|A|changes/active/bifrost-variant/review/TASK-001-r4/task-review-behavior.md|False|False|
|A|changes/active/bifrost-variant/review/TASK-001-r4/task-review-invariants.md|False|False|
|A|changes/active/bifrost-variant/review/TASK-001-r4/verdict.md|False|False|
|A|changes/active/bifrost-variant/review/TASK-001-r5/TASK-001-R5-close-remaining-variant-contract-gaps.md|False|False|
|A|changes/active/bifrost-variant/review/TASK-001-r5/domain-review-otlp-tables.md|False|False|
|A|changes/active/bifrost-variant/review/TASK-001-r5/domain-review-persistent-schema.md|False|False|
|A|changes/active/bifrost-variant/review/TASK-001-r5/domain-review-scribe-ingest.md|False|False|
|A|changes/active/bifrost-variant/review/TASK-001-r5/domain-review-sdk-cli.md|False|False|
|A|changes/active/bifrost-variant/review/TASK-001-r5/domain-review-security-tenancy.md|False|False|
|A|changes/active/bifrost-variant/review/TASK-001-r5/domain-review-variant-arrow.md|False|False|
|A|changes/active/bifrost-variant/review/TASK-001-r5/findings-validation.md|False|False|
|A|changes/active/bifrost-variant/review/TASK-001-r5/followup-review.md|False|False|
|A|changes/active/bifrost-variant/review/TASK-001-r5/maintainer-review.md|False|False|
|A|changes/active/bifrost-variant/review/TASK-001-r5/reuse-review.md|False|False|
|A|changes/active/bifrost-variant/review/TASK-001-r5/root-cause.md|False|False|
|A|changes/active/bifrost-variant/review/TASK-001-r5/standards-review.md|False|False|
|A|changes/active/bifrost-variant/review/TASK-001-r5/system-review.md|False|False|
|A|changes/active/bifrost-variant/review/TASK-001-r5/task-review-behavior.md|False|False|
|A|changes/active/bifrost-variant/review/TASK-001-r5/task-review-invariants.md|False|False|
|A|changes/active/bifrost-variant/review/TASK-001-r5/verdict.md|False|False|
|A|changes/active/bifrost-variant/review/TASK-001-r6/TASK-001-R6-close-variant-canonicality-and-proof-gaps.md|False|False|
|A|changes/active/bifrost-variant/review/TASK-001-r6/domain-review-iceberg-durability.md|False|False|
|A|changes/active/bifrost-variant/review/TASK-001-r6/domain-review-oracle-datafusion.md|False|False|
|A|changes/active/bifrost-variant/review/TASK-001-r6/domain-review-persisted-schema.md|False|False|
|A|changes/active/bifrost-variant/review/TASK-001-r6/domain-review-scribe-ingest.md|False|False|
|A|changes/active/bifrost-variant/review/TASK-001-r6/domain-review-sdk-parity.md|False|False|
|A|changes/active/bifrost-variant/review/TASK-001-r6/domain-review-security-tenancy.md|False|False|
|A|changes/active/bifrost-variant/review/TASK-001-r6/domain-review-variant-arrow.md|False|False|
|A|changes/active/bifrost-variant/review/TASK-001-r6/findings-validation.md|False|False|
|A|changes/active/bifrost-variant/review/TASK-001-r6/followup-review.md|False|False|
|A|changes/active/bifrost-variant/review/TASK-001-r6/maintainer-review.md|False|False|
|A|changes/active/bifrost-variant/review/TASK-001-r6/reuse-review.md|False|False|
|A|changes/active/bifrost-variant/review/TASK-001-r6/root-cause.md|False|False|
|A|changes/active/bifrost-variant/review/TASK-001-r6/standards-review.md|False|False|
|A|changes/active/bifrost-variant/review/TASK-001-r6/system-review.md|False|False|
|A|changes/active/bifrost-variant/review/TASK-001-r6/task-review-behavior.md|False|False|
|A|changes/active/bifrost-variant/review/TASK-001-r6/task-review-invariants.md|False|False|
|A|changes/active/bifrost-variant/review/TASK-001-r6/verdict.md|False|False|
|A|changes/active/bifrost-variant/review/TASK-001-r7/TASK-001-R7-close-json-identity-and-proof-gaps.md|False|False|
|A|changes/active/bifrost-variant/review/TASK-001-r7/domain-review-oracle-datafusion.md|False|False|
|A|changes/active/bifrost-variant/review/TASK-001-r7/domain-review-persistent-schema.md|False|False|
|A|changes/active/bifrost-variant/review/TASK-001-r7/domain-review-scribe-ingest.md|False|False|
|A|changes/active/bifrost-variant/review/TASK-001-r7/domain-review-sdk-parity.md|False|False|
|A|changes/active/bifrost-variant/review/TASK-001-r7/domain-review-security-tenancy.md|False|False|
|A|changes/active/bifrost-variant/review/TASK-001-r7/domain-review-variant-arrow.md|False|False|
|A|changes/active/bifrost-variant/review/TASK-001-r7/findings-validation.md|False|False|
|A|changes/active/bifrost-variant/review/TASK-001-r7/followup-review.md|False|False|
|A|changes/active/bifrost-variant/review/TASK-001-r7/maintainer-review.md|False|False|
|A|changes/active/bifrost-variant/review/TASK-001-r7/reuse-review.md|False|False|
|A|changes/active/bifrost-variant/review/TASK-001-r7/root-cause.md|False|False|
|A|changes/active/bifrost-variant/review/TASK-001-r7/standards-review.md|False|False|
|A|changes/active/bifrost-variant/review/TASK-001-r7/system-review.md|False|False|
|A|changes/active/bifrost-variant/review/TASK-001-r7/task-review-behavior.md|False|False|
|A|changes/active/bifrost-variant/review/TASK-001-r7/task-review-invariants.md|False|False|
|A|changes/active/bifrost-variant/review/TASK-001-r7/verdict.md|False|False|
|A|changes/active/bifrost-variant/review/TASK-003-r1/TASK-003-R1-review-fixes.md|False|False|
|A|changes/active/bifrost-variant/review/TASK-003-r1/domain-review-resource-lifecycle.md|False|False|
|A|changes/active/bifrost-variant/review/TASK-003-r1/domain-review-security-tenancy.md|False|False|
|A|changes/active/bifrost-variant/review/TASK-003-r1/domain-review-storage-durability.md|False|False|
|A|changes/active/bifrost-variant/review/TASK-003-r1/findings-validation.md|False|False|
|A|changes/active/bifrost-variant/review/TASK-003-r1/followup-review.md|False|False|
|A|changes/active/bifrost-variant/review/TASK-003-r1/maintainer-review.md|False|False|
|A|changes/active/bifrost-variant/review/TASK-003-r1/navigation-map.md|False|False|
|A|changes/active/bifrost-variant/review/TASK-003-r1/readiness-review-codex-2.md|False|False|
|A|changes/active/bifrost-variant/review/TASK-003-r1/readiness-review-codex.md|False|False|
|A|changes/active/bifrost-variant/review/TASK-003-r1/reuse-review.md|False|False|
|A|changes/active/bifrost-variant/review/TASK-003-r1/standards-review.md|False|False|
|A|changes/active/bifrost-variant/review/TASK-003-r1/system-review.md|False|False|
|A|changes/active/bifrost-variant/review/TASK-003-r1/task-review-behavior.md|False|False|
|A|changes/active/bifrost-variant/review/TASK-003-r1/task-review-invariants.md|False|False|
|A|changes/active/bifrost-variant/review/TASK-003-r1/verdict.md|False|False|
|A|changes/active/bifrost-variant/spec-revision-20-read-core.md|False|False|
|A|changes/active/bifrost-variant/spec-revision-21-sampling.md|False|False|
|A|changes/active/bifrost-variant/spec-revision-22-output-leaf-projection.md|False|False|
|A|changes/active/bifrost-variant/spec-revision-23-nested-shredding.md|False|False|
|A|changes/active/bifrost-variant/spec-revision-24-benchmark-shape.md|False|False|
|A|changes/active/bifrost-variant/spec-revision-25-hot-parquet-source.md|False|False|
|A|changes/active/bifrost-variant/spec-revision-26-datafusion-variant.md|False|False|
|M|changes/active/bifrost-variant/spec.md|False|False|
|A|changes/active/bifrost-variant/tasks/TASK-001-variant-storage-and-query.md|False|False|
|A|changes/active/bifrost-variant/tasks/TASK-002-user-defined-variant-tables.md|False|False|
|A|changes/active/bifrost-variant/tasks/TASK-003-decisions.md|False|False|
|A|changes/active/bifrost-variant/tasks/TASK-003-native-datafusion-read-path-plan.md|False|False|
|A|changes/active/bifrost-variant/tasks/TASK-003-revision-query-latency.md|False|False|
|A|changes/active/bifrost-variant/tasks/TASK-003-shredding-and-leaf-pruning.md|False|False|
|A|changes/active/bifrost-variant/tasks/TASK-005-table-time-column.md|False|False|
|M|crates/shared/workspace-hack/Cargo.toml|True|True|
|M|crates/shared/wyrd-bench/src/lane.rs|False|False|
|M|crates/shared/wyrd-client/Cargo.toml|True|False|
|M|crates/shared/wyrd-client/src/bifrost/facade.rs|True|True|
|M|crates/shared/wyrd-client/src/bifrost/grpc.rs|True|True|
|M|crates/shared/wyrd-client/src/bifrost/handle.rs|True|False|
|M|crates/shared/wyrd-client/src/bifrost/mod.rs|True|True|
|M|crates/shared/wyrd-client/src/bifrost/query.rs|True|True|
|M|crates/shared/wyrd-client/src/bifrost/table.rs|True|True|
|M|crates/shared/wyrd-client/src/error.rs|True|True|
|M|crates/shared/wyrd-client/src/observe/drift.rs|True|False|
|M|crates/shared/wyrd-client/src/observe/eval.rs|True|False|
|M|crates/shared/wyrd-client/src/observe/tests.rs|True|False|
|A|crates/shared/wyrd-client/tests/bifrost/every_iceberg_column_type.rs|False|False|
|A|crates/shared/wyrd-client/tests/bifrost/register_a_table_from_a_model.rs|False|False|
|A|crates/shared/wyrd-client/tests/bifrost/table_time_column.rs|False|False|
|A|crates/shared/wyrd-client/tests/bifrost/three_timestamp_types.rs|False|False|
|M|crates/shared/wyrd-client/tests/pg_bifrost_e2e.rs|True|True|
|A|crates/shared/wyrd-client/tests/pg_bifrost_e2e/variant_tables.rs|False|False|
|M|crates/shared/wyrd-queue/Cargo.toml|True|False|
|M|crates/shared/wyrd-queue/src/batch_builder.rs|True|True|
|M|crates/shared/wyrd-queue/src/config.rs|True|True|
|M|crates/shared/wyrd-queue/src/error.rs|True|True|
|M|crates/shared/wyrd-queue/src/lib.rs|False|False|
|M|crates/shared/wyrd-queue/src/producer.rs|True|True|
|M|crates/shared/wyrd-queue/src/queue.rs|True|True|
|D|crates/shared/wyrd-queue/src/schema.rs|True|True|
|A|crates/shared/wyrd-queue/src/variant.rs|False|False|
|A|crates/shared/wyrd-types/Cargo.toml|False|False|
|A|crates/shared/wyrd-types/src/lib.rs|False|False|
|A|crates/shared/wyrd-types/src/schema.rs|False|False|
|A|crates/shared/wyrd-types/src/timestamp.rs|False|False|
|A|crates/shared/wyrd-types/src/variant.rs|False|False|
|M|crates/vala/vala-bifrost-redux/Cargo.toml|True|False|
|M|crates/vala/vala-bifrost-redux/src/catalog/bifrost_catalog.rs|True|True|
|A|crates/vala/vala-bifrost-redux/src/catalog/cut_cache.rs|False|False|
|M|crates/vala/vala-bifrost-redux/src/catalog/error.rs|True|False|
|M|crates/vala/vala-bifrost-redux/src/catalog/event_time.rs|False|False|
|M|crates/vala/vala-bifrost-redux/src/catalog/iceberg_storage.rs|False|False|
|M|crates/vala/vala-bifrost-redux/src/catalog/layout.rs|False|False|
|M|crates/vala/vala-bifrost-redux/src/catalog/mod.rs|False|False|
|M|crates/vala/vala-bifrost-redux/src/catalog/table_ref.rs|False|False|
|M|crates/vala/vala-bifrost-redux/src/catalog/wire.rs|True|True|
|M|crates/vala/vala-bifrost-redux/src/contracts.rs|True|False|
|M|crates/vala/vala-bifrost-redux/src/forge/compact.rs|True|False|
|M|crates/vala/vala-bifrost-redux/src/forge/error.rs|False|False|
|M|crates/vala/vala-bifrost-redux/src/forge/expire.rs|True|True|
|M|crates/vala/vala-bifrost-redux/src/forge/expiry_policy.rs|False|False|
|M|crates/vala/vala-bifrost-redux/src/forge/gc.rs|False|False|
|M|crates/vala/vala-bifrost-redux/src/forge/live_reconcile.rs|False|False|
|M|crates/vala/vala-bifrost-redux/src/forge/live_replace.rs|False|False|
|M|crates/vala/vala-bifrost-redux/src/forge/managed/executor.rs|False|False|
|M|crates/vala/vala-bifrost-redux/src/forge/managed/identity.rs|False|False|
|M|crates/vala/vala-bifrost-redux/src/forge/managed/policy.rs|False|False|
|M|crates/vala/vala-bifrost-redux/src/forge/metrics.rs|True|False|
|M|crates/vala/vala-bifrost-redux/src/forge/mod.rs|False|False|
|M|crates/vala/vala-bifrost-redux/src/forge/orphan_gc.rs|False|False|
|M|crates/vala/vala-bifrost-redux/src/forge/path.rs|False|False|
|M|crates/vala/vala-bifrost-redux/src/forge/scribe_promotion.rs|False|False|
|M|crates/vala/vala-bifrost-redux/src/forge/table_authority.rs|False|False|
|M|crates/vala/vala-bifrost-redux/src/forge/worker.rs|True|False|
|M|crates/vala/vala-bifrost-redux/src/gate/error.rs|True|True|
|M|crates/vala/vala-bifrost-redux/src/gate/limits.rs|False|False|
|M|crates/vala/vala-bifrost-redux/src/gate/mod.rs|True|True|
|M|crates/vala/vala-bifrost-redux/src/lib.rs|False|False|
|M|crates/vala/vala-bifrost-redux/src/oracle/admission.rs|True|True|
|M|crates/vala/vala-bifrost-redux/src/oracle/analytical.rs|True|True|
|D|crates/vala/vala-bifrost-redux/src/oracle/analytical_scan.rs|False|False|
|M|crates/vala/vala-bifrost-redux/src/oracle/analytical_supervisor.rs|True|True|
|M|crates/vala/vala-bifrost-redux/src/oracle/analytical_transport.rs|True|True|
|M|crates/vala/vala-bifrost-redux/src/oracle/bindings.rs|False|False|
|M|crates/vala/vala-bifrost-redux/src/oracle/codec.rs|True|False|
|M|crates/vala/vala-bifrost-redux/src/oracle/dispatcher.rs|False|False|
|M|crates/vala/vala-bifrost-redux/src/oracle/exec.rs|False|False|
|M|crates/vala/vala-bifrost-redux/src/oracle/follower.rs|True|True|
|M|crates/vala/vala-bifrost-redux/src/oracle/live.rs|False|False|
|A|crates/vala/vala-bifrost-redux/src/oracle/memtable_source.rs|False|False|
|M|crates/vala/vala-bifrost-redux/src/oracle/mod.rs|True|True|
|A|crates/vala/vala-bifrost-redux/src/oracle/nested_pushdown.rs|False|False|
|M|crates/vala/vala-bifrost-redux/src/oracle/peer.rs|True|False|
|M|crates/vala/vala-bifrost-redux/src/oracle/planner.rs|True|False|
|A|crates/vala/vala-bifrost-redux/src/oracle/profile.rs|False|False|
|D|crates/vala/vala-bifrost-redux/src/oracle/pruning.rs|False|False|
|M|crates/vala/vala-bifrost-redux/src/oracle/query_stream.rs|True|True|
|A|crates/vala/vala-bifrost-redux/src/oracle/session.rs|False|False|
|A|crates/vala/vala-bifrost-redux/src/oracle/sizing.rs|False|False|
|M|crates/vala/vala-bifrost-redux/src/oracle/tail_discovery.rs|False|False|
|M|crates/vala/vala-bifrost-redux/src/parquet/mod.rs|False|False|
|M|crates/vala/vala-bifrost-redux/src/parquet/object_uploader.rs|False|False|
|M|crates/vala/vala-bifrost-redux/src/parquet/promoted_object.rs|False|False|
|A|crates/vala/vala-bifrost-redux/src/parquet/variant_residual.rs|False|False|
|M|crates/vala/vala-bifrost-redux/src/parquet/writer_properties.rs|False|False|
|A|crates/vala/vala-bifrost-redux/src/profile.rs|False|False|
|M|crates/vala/vala-bifrost-redux/src/resources.rs|True|True|
|A|crates/vala/vala-bifrost-redux/src/scan_sizing.rs|False|False|
|M|crates/vala/vala-bifrost-redux/src/schema/fingerprint.rs|False|False|
|M|crates/vala/vala-bifrost-redux/src/scribe/admission.rs|False|False|
|M|crates/vala/vala-bifrost-redux/src/scribe/assembly.rs|True|False|
|M|crates/vala/vala-bifrost-redux/src/scribe/claim_assembly.rs|False|False|
|M|crates/vala/vala-bifrost-redux/src/scribe/claim_merge.rs|False|False|
|A|crates/vala/vala-bifrost-redux/src/scribe/claim_sample.rs|False|False|
|M|crates/vala/vala-bifrost-redux/src/scribe/execution_lanes.rs|True|True|
|M|crates/vala/vala-bifrost-redux/src/scribe/file_list_writer.rs|False|False|
|D|crates/vala/vala-bifrost-redux/src/scribe/fixed_ipc.rs|False|False|
|M|crates/vala/vala-bifrost-redux/src/scribe/hot_source.rs|False|False|
|M|crates/vala/vala-bifrost-redux/src/scribe/hot_stage.rs|False|False|
|M|crates/vala/vala-bifrost-redux/src/scribe/ingress.rs|True|True|
|M|crates/vala/vala-bifrost-redux/src/scribe/material_plan.rs|False|False|
|M|crates/vala/vala-bifrost-redux/src/scribe/member_stager.rs|False|False|
|M|crates/vala/vala-bifrost-redux/src/scribe/memory.rs|False|False|
|M|crates/vala/vala-bifrost-redux/src/scribe/memtable.rs|False|False|
|M|crates/vala/vala-bifrost-redux/src/scribe/mod.rs|True|False|
|M|crates/vala/vala-bifrost-redux/src/scribe/parquet_writer.rs|False|False|
|M|crates/vala/vala-bifrost-redux/src/scribe/persistence.rs|True|False|
|M|crates/vala/vala-bifrost-redux/src/scribe/preprocess.rs|True|False|
|A|crates/vala/vala-bifrost-redux/src/scribe/profile.rs|False|False|
|M|crates/vala/vala-bifrost-redux/src/scribe/replay.rs|True|False|
|M|crates/vala/vala-bifrost-redux/src/scribe/seal_key.rs|False|False|
|M|crates/vala/vala-bifrost-redux/src/scribe/shards.rs|True|False|
|M|crates/vala/vala-bifrost-redux/src/scribe/staging.rs|False|False|
|M|crates/vala/vala-bifrost-redux/src/scribe/staging_runtime.rs|True|False|
|M|crates/vala/vala-bifrost-redux/src/scribe/tail_rpc.rs|False|False|
|M|crates/vala/vala-bifrost-redux/src/scribe/tests/pg_scribe_crash_injection.rs|False|False|
|M|crates/vala/vala-bifrost-redux/src/scribe/tests/pg_scribe_restart.rs|False|False|
|M|crates/vala/vala-bifrost-redux/src/scribe/tests/scribe_persistence_path.rs|True|True|
|M|crates/vala/vala-bifrost-redux/src/scribe/tests/time_partition.rs|False|False|
|M|crates/vala/vala-bifrost-redux/src/scribe/tests/wal_closeout.rs|True|False|
|M|crates/vala/vala-bifrost-redux/src/scribe/wal.rs|False|False|
|D|crates/vala/vala-bifrost-redux/src/storage/cache.rs|False|False|
|M|crates/vala/vala-bifrost-redux/src/storage/error.rs|False|False|
|M|crates/vala/vala-bifrost-redux/src/storage/mod.rs|True|True|
|D|crates/vala/vala-bifrost-redux/src/storage/telemetry.rs|False|False|
|M|crates/vala/vala-bifrost-redux/src/tables/audit/audit_log.rs|True|True|
|M|crates/vala/vala-bifrost-redux/src/tables/audit/mod.rs|False|False|
|M|crates/vala/vala-bifrost-redux/src/tables/audit/projection.rs|True|True|
|M|crates/vala/vala-bifrost-redux/src/tables/dev/agent_traces.rs|True|True|
|M|crates/vala/vala-bifrost-redux/src/tables/drift/observations.rs|False|False|
|M|crates/vala/vala-bifrost-redux/src/tables/drift/result_features.rs|False|False|
|M|crates/vala/vala-bifrost-redux/src/tables/eval/observations.rs|True|True|
|M|crates/vala/vala-bifrost-redux/src/tables/eval/result_items.rs|False|False|
|M|crates/vala/vala-bifrost-redux/src/tables/fields.rs|False|False|
|M|crates/vala/vala-bifrost-redux/src/tables/gateway/calls.rs|False|False|
|M|crates/vala/vala-bifrost-redux/src/tables/gateway/mod.rs|False|False|
|M|crates/vala/vala-bifrost-redux/src/tables/logs/mod.rs|True|False|
|M|crates/vala/vala-bifrost-redux/src/tables/logs/projection.rs|True|True|
|M|crates/vala/vala-bifrost-redux/src/tables/logs/records.rs|False|False|
|M|crates/vala/vala-bifrost-redux/src/tables/managed_columns.rs|True|True|
|M|crates/vala/vala-bifrost-redux/src/tables/metrics/mod.rs|True|False|
|M|crates/vala/vala-bifrost-redux/src/tables/metrics/points.rs|False|False|
|M|crates/vala/vala-bifrost-redux/src/tables/metrics/projection.rs|True|True|
|M|crates/vala/vala-bifrost-redux/src/tables/mod.rs|True|True|
|M|crates/vala/vala-bifrost-redux/src/tables/signal.rs|True|True|
|M|crates/vala/vala-bifrost-redux/src/tables/traces/mod.rs|True|False|
|M|crates/vala/vala-bifrost-redux/src/tables/traces/projection.rs|True|True|
|M|crates/vala/vala-bifrost-redux/src/tables/traces/spans.rs|False|False|
|M|crates/vala/vala-bifrost-redux/src/tables/verification/mod.rs|False|False|
|M|crates/vala/vala-bifrost-redux/src/tables/verification/results.rs|True|True|
|M|crates/vala/vala-bifrost-redux/tests/integration/distributed_compat.rs|False|False|
|M|crates/vala/vala-bifrost-redux/tests/integration/forge/compaction_admission.rs|True|True|
|M|crates/vala/vala-bifrost-redux/tests/integration/forge/expired_cleanup.rs|False|False|
|M|crates/vala/vala-bifrost-redux/tests/integration/forge/managed_rewrite.rs|False|False|
|M|crates/vala/vala-bifrost-redux/tests/integration/forge/orphan_cleanup.rs|True|False|
|M|crates/vala/vala-bifrost-redux/tests/integration/forge/production_routes.rs|True|False|
|M|crates/vala/vala-bifrost-redux/tests/integration/forge/promotion.rs|True|True|
|M|crates/vala/vala-bifrost-redux/tests/integration/forge/publication.rs|True|True|
|M|crates/vala/vala-bifrost-redux/tests/integration/forge/reader_expiry_ordering.rs|False|False|
|M|crates/vala/vala-bifrost-redux/tests/integration/forge/rewrite_support.rs|False|False|
|M|crates/vala/vala-bifrost-redux/tests/integration/forge/snapshot_expiration.rs|True|False|
|M|crates/vala/vala-bifrost-redux/tests/integration/forge/support.rs|True|False|
|M|crates/vala/vala-sql/migrations/20260619000001_olap_minimal.sql|True|False|
|A|crates/vala/vala-sql/migrations/20261009000000_bifrost_snapshot_retention.sql|False|False|
|A|crates/vala/vala-sql/migrations/20261010000000_bifrost_cut_notify.sql|False|False|
|A|crates/vala/vala-sql/migrations/20261011000000_scribe_wal_fenced_lsn.sql|False|False|
|M|crates/vala/vala-sql/src/lib.rs|True|True|
|M|crates/vala/vala-sql/src/postgres.rs|True|False|
|M|crates/vala/vala-sql/src/queries/audit_staging.rs|True|True|
|M|crates/vala/vala-sql/src/queries/forge_operations.rs|True|False|
|M|crates/vala/vala-sql/src/queries/olap_catalog.rs|True|False|
|M|crates/vala/vala-sql/src/queries/oracle_reader_authority.rs|True|True|
|M|crates/vala/vala-sql/src/queries/scribe_batch_commits.rs|False|False|
|M|crates/vala/vala-sql/src/row_types/file_list.rs|False|False|
|M|crates/vala/vala-sql/src/row_types/oracle_reader_authority.rs|False|False|
|M|crates/vala/vala-sql/tests/pg_audit_staging.rs|True|True|
|M|crates/vala/vala-sql/tests/pg_forge_operations.rs|True|False|
|M|crates/vala/vala-sql/tests/pg_forge_tasks.rs|True|False|
|M|crates/vala/vala-sql/tests/pg_olap_catalog.rs|True|False|
|M|crates/vala/vala-sql/tests/pg_oracle_membership.rs|True|False|
|M|crates/vala/vala-sql/tests/pg_schema_usage.rs|True|False|
|M|crates/wyrd-spec/schemas/bifrost_audit_event.json|True|False|
|M|crates/wyrd-spec/schemas/bifrost_data_type_spec.json|True|False|
|M|crates/wyrd-spec/schemas/bifrost_field_spec.json|True|False|
|M|crates/wyrd-spec/schemas/bifrost_physical_layout.json|False|False|
|M|crates/wyrd-spec/schemas/bifrost_register_table_request.json|True|False|
|M|crates/wyrd-spec/schemas/bifrost_table_description.json|True|False|
|M|crates/wyrd-spec/schemas/bifrost_time_granularity.json|False|False|
|M|crates/wyrd-spec/src/error.rs|True|False|
|M|crates/wyrd-spec/src/vala/api.rs|True|True|
|M|crates/wyrd-spec/src/vala/assignment_authority.rs|True|True|
|M|crates/wyrd-spec/src/vala/error.rs|True|False|
|M|crates/wyrd-spec/src/vala/logs/record.rs|False|False|
|M|crates/wyrd-spec/src/vala/managed_columns.rs|True|False|
|M|crates/wyrd-spec/src/vala/mod.rs|True|True|
|M|crates/wyrd-spec/src/vala/trace/trace_summary.rs|False|False|
|M|crates/wyrd-spec/tests/fixtures/trace/schemas/trace_summary_record.schema.json|False|False|
|M|crates/wyrd-spec/tests/schemas/bifrost_audit_event.json|True|True|
|M|crates/wyrd-spec/tests/schemas/bifrost_data_type_spec.json|True|True|
|M|crates/wyrd-spec/tests/schemas/bifrost_field_spec.json|True|True|
|M|crates/wyrd-spec/tests/schemas/bifrost_physical_layout.json|True|True|
|M|crates/wyrd-spec/tests/schemas/bifrost_register_table_request.json|True|True|
|M|crates/wyrd-spec/tests/schemas/bifrost_table_description.json|True|True|
|M|crates/wyrd-spec/tests/schemas/bifrost_time_granularity.json|True|True|
|M|crates/wyrd/wyrd-cli/Cargo.toml|True|True|
|M|crates/wyrd/wyrd-cli/src/query/mod.rs|True|False|
|M|crates/wyrd/wyrd-cli/tests/query_server_journey.rs|True|False|
|M|crates/wyrd/wyrd-mcp/tests/bifrost/mcp/query.rs|True|True|
|M|crates/wyrd/wyrd-server/Cargo.toml|True|True|
|M|crates/wyrd/wyrd-server/src/app/metrics.rs|False|False|
|M|crates/wyrd/wyrd-server/src/app/serve.rs|False|False|
|M|crates/wyrd/wyrd-server/src/app/server.rs|True|True|
|M|crates/wyrd/wyrd-server/src/audit/mod.rs|True|True|
|M|crates/wyrd/wyrd-server/src/bifrost/convert.rs|False|False|
|M|crates/wyrd/wyrd-server/src/bifrost/gate_audit.rs|True|True|
|M|crates/wyrd/wyrd-server/src/bifrost/routes.rs|True|False|
|M|crates/wyrd/wyrd-server/src/bifrost/service.rs|True|True|
|M|crates/wyrd/wyrd-server/src/boot/mod.rs|True|True|
|M|crates/wyrd/wyrd-server/src/components/gateway/capture.rs|True|True|
|M|crates/wyrd/wyrd-server/src/components/gateway/pg_invocation_tests.rs|True|False|
|M|crates/wyrd/wyrd-server/src/config.rs|True|False|
|M|crates/wyrd/wyrd-server/src/grpc/query.rs|False|False|
|M|crates/wyrd/wyrd-server/src/grpc/scribe_tail.rs|False|False|
|M|crates/wyrd/wyrd-server/src/http/otlp.rs|True|True|
|M|crates/wyrd/wyrd-server/src/mcp/bifrost.rs|True|True|
|M|crates/wyrd/wyrd-server/src/oracle/peer_service.rs|True|False|
|M|crates/wyrd/wyrd-server/src/oracle/tail_discovery.rs|False|False|
|M|crates/wyrd/wyrd-server/src/otlp_json.rs|True|False|
|M|crates/wyrd/wyrd-server/src/otlp_metrics_decode.rs|True|False|
|M|crates/wyrd/wyrd-server/src/query/routes.rs|True|False|
|M|crates/wyrd/wyrd-server/src/query/scheduled.rs|True|False|
|M|crates/wyrd/wyrd-server/src/query/service.rs|True|True|
|M|crates/wyrd/wyrd-server/src/state.rs|True|True|
|M|crates/wyrd/wyrd-server/src/test_support.rs|True|False|
|M|crates/wyrd/wyrd-server/src/verification/drift.rs|True|True|
|M|crates/wyrd/wyrd-server/src/verification/eval.rs|True|True|
|M|crates/wyrd/wyrd-server/src/verification/observations.rs|True|True|
|M|crates/wyrd/wyrd-server/src/verification/publisher.rs|True|True|
|M|crates/wyrd/wyrd-server/src/verification/results.rs|True|True|
|M|crates/wyrd/wyrd-server/src/verification/runner.rs|True|True|
|M|crates/wyrd/wyrd-server/tests/pg_grpc_ingest_smoke.rs|True|False|
|M|crates/wyrd/wyrd-server/tests/pg_verification_routes.rs|True|False|
|M|crates/wyrd/wyrd-server/tests/pg_verification_runtime.rs|True|False|
|M|crates/wyrd/wyrd-sql/src/queries/verifier_runs.rs|True|False|
|M|crates/wyrd/wyrd-storage/src/factory/gcs.rs|False|False|
|M|crates/wyrd/wyrd-storage/src/factory/mod.rs|False|False|
|M|crates/wyrd/wyrd-storage/src/local.rs|True|True|
|M|crates/wyrd/wyrd-storage/src/signer.rs|True|False|
|M|crates/wyrd/wyrd-storage/src/telemetry.rs|False|False|
|M|crates/wyrd/wyrd-testing/Cargo.toml|True|True|
|M|crates/wyrd/wyrd-testing/src/bifrost/canonical_signals.rs|False|False|
|M|crates/wyrd/wyrd-testing/src/bifrost/cluster.rs|True|False|
|M|crates/wyrd/wyrd-testing/src/bifrost/forge_harness.rs|True|False|
|M|crates/wyrd/wyrd-testing/src/bifrost/mod.rs|False|False|
|A|crates/wyrd/wyrd-testing/src/bifrost/object_reads.rs|False|False|
|M|crates/wyrd/wyrd-testing/src/bifrost/scribe_workload.rs|True|True|
|R098|crates/wyrd/wyrd-testing/src/bin/bifrost_ingest_capacity/fixture.rs → crates/wyrd/wyrd-testing/src/bin/bifrost_client_queue/fixture.rs|True|True|
|R089|crates/wyrd/wyrd-testing/src/bin/bifrost_ingest_capacity/main.rs → crates/wyrd/wyrd-testing/src/bin/bifrost_client_queue/main.rs|True|True|
|R100|crates/wyrd/wyrd-testing/src/bin/bifrost_ingest_capacity/proxy.rs → crates/wyrd/wyrd-testing/src/bin/bifrost_client_queue/proxy.rs|True|True|
|R093|crates/wyrd/wyrd-testing/src/bin/bifrost_ingest_capacity/step.rs → crates/wyrd/wyrd-testing/src/bin/bifrost_client_queue/step.rs|True|True|
|M|crates/wyrd/wyrd-testing/src/bin/bifrost_forge_capacity/deployment.rs|False|False|
|M|crates/wyrd/wyrd-testing/src/bin/bifrost_forge_capacity/main.rs|False|False|
|A|crates/wyrd/wyrd-testing/src/bin/bifrost_query_capacity/logs.rs|False|False|
|M|crates/wyrd/wyrd-testing/src/bin/bifrost_query_capacity/main.rs|True|True|
|M|crates/wyrd/wyrd-testing/src/bin/bifrost_query_capacity/report.rs|True|True|
|M|crates/wyrd/wyrd-testing/src/bin/bifrost_query_capacity/run.rs|True|True|
|A|crates/wyrd/wyrd-testing/src/bin/bifrost_query_capacity/telemetry.rs|False|False|
|M|crates/wyrd/wyrd-testing/src/bin/verification_capacity/main.rs|True|True|
|D|crates/wyrd/wyrd-testing/src/bin/verification_capacity/profile.rs|True|False|
|M|crates/wyrd/wyrd-testing/src/bin/verification_capacity/step.rs|True|True|
|M|crates/wyrd/wyrd-testing/src/capacity.rs|True|False|
|A|crates/wyrd/wyrd-testing/src/gateway_capture.rs|False|False|
|M|crates/wyrd/wyrd-testing/src/lib.rs|True|True|
|M|crates/wyrd/wyrd-testing/src/load/harness.rs|True|False|
|A|crates/wyrd/wyrd-testing/src/profile.rs|False|False|
|M|crates/wyrd/wyrd-testing/src/release_server.rs|True|True|
|A|crates/wyrd/wyrd-testing/src/release_server/os.rs|False|False|
|M|crates/wyrd/wyrd-testing/src/server.rs|True|True|
|M|crates/wyrd/wyrd-testing/src/verification.rs|True|False|
|M|crates/wyrd/wyrd-testing/tests/README.md|True|True|
|M|crates/wyrd/wyrd-testing/tests/bifrost/forge/live_rewrite.rs|True|False|
|M|crates/wyrd/wyrd-testing/tests/bifrost/forge/production_closeout.rs|True|True|
|M|crates/wyrd/wyrd-testing/tests/bifrost/forge/scribe_promotion.rs|True|False|
|M|crates/wyrd/wyrd-testing/tests/bifrost/oracle/analytical_activation.rs|True|False|
|M|crates/wyrd/wyrd-testing/tests/bifrost/oracle/analytical_lifecycle.rs|True|False|
|M|crates/wyrd/wyrd-testing/tests/bifrost/oracle/capacity.rs|True|True|
|M|crates/wyrd/wyrd-testing/tests/bifrost/oracle/distributed.rs|True|True|
|M|crates/wyrd/wyrd-testing/tests/bifrost/oracle/peer_cluster.rs|True|True|
|M|crates/wyrd/wyrd-testing/tests/bifrost/oracle/peer_network/analytical.rs|True|False|
|M|crates/wyrd/wyrd-testing/tests/bifrost/oracle/peer_network/security.rs|True|False|
|M|crates/wyrd/wyrd-testing/tests/bifrost/oracle/published.rs|True|True|
|M|crates/wyrd/wyrd-testing/tests/bifrost/oracle/support.rs|True|True|
|M|crates/wyrd/wyrd-testing/tests/bifrost/otlp/logs_export.rs|False|False|
|M|crates/wyrd/wyrd-testing/tests/bifrost/otlp/metrics_export.rs|False|False|
|M|crates/wyrd/wyrd-testing/tests/bifrost/otlp/support.rs|True|False|
|M|crates/wyrd/wyrd-testing/tests/bifrost/otlp/trace_export.rs|True|True|
|M|crates/wyrd/wyrd-testing/tests/bifrost/otlp/trace_export_http.rs|False|False|
|M|crates/wyrd/wyrd-testing/tests/bifrost/scribe/fencing.rs|False|False|
|M|crates/wyrd/wyrd-testing/tests/bifrost/scribe/main.rs|True|False|
|M|crates/wyrd/wyrd-testing/tests/bifrost/scribe/qualification.rs|True|True|
|A|crates/wyrd/wyrd-testing/tests/bifrost/scribe/recovery.rs|False|False|
|M|crates/wyrd/wyrd-testing/tests/bifrost/scribe/support.rs|True|True|
|M|crates/wyrd/wyrd-testing/tests/bifrost/scribe/sustained.rs|True|False|
|M|crates/wyrd/wyrd-testing/tests/bifrost/scribe/write_read.rs|True|True|
|M|crates/wyrd/wyrd-testing/tests/bifrost/server/eval_verification.rs|True|True|
|M|crates/wyrd/wyrd-testing/tests/bifrost/server/main.rs|True|True|
|M|crates/wyrd/wyrd-testing/tests/bifrost/server/owner_inspection.rs|True|False|
|M|crates/wyrd/wyrd-testing/tests/bifrost/server/query.rs|True|True|
|M|crates/wyrd/wyrd-testing/tests/bifrost/server/verification_runtime.rs|True|True|
|M|crates/wyrd/wyrd-testing/tests/gateway/peer.rs|True|True|
|M|crates/wyrd/wyrd-tonic/proto/wyrd.v1.bin|True|True|
|M|crates/wyrd/wyrd-tonic/proto/wyrd.v1.proto|True|True|
|M|crates/wyrd/wyrd-tonic/src/private_conversion.rs|False|False|
|M|crates/wyrd/wyrd-tonic/src/query_conversion.rs|True|True|
|M|docs/src/content/docs/bifrost/architecture.svx|True|True|
|M|docs/src/content/docs/bifrost/data-plane-internals.svx|True|False|
|M|docs/src/content/docs/bifrost/forge.svx|True|True|
|M|docs/src/content/docs/bifrost/reading-data.svx|True|True|
|M|docs/src/content/docs/bifrost/schema.svx|True|True|
|M|docs/src/content/docs/bifrost/writing-data.svx|False|False|
|A|fixtures/bifrost/every_iceberg_type.arrow|False|False|
|A|fixtures/bifrost/every_iceberg_type.py|False|False|
|A|fixtures/bifrost/narrow_types.arrow|False|False|
|M|mise.lock|False|False|
|M|mise.toml|True|True|
|M|scripts/checks/object-store-pin.sh|True|True|
|M|scripts/test-families.sh|True|True|
|M|sdks/wyrd-sdk-python/Cargo.toml|True|False|
|M|sdks/wyrd-sdk-python/python/wyrd/bifrost/__init__.py|True|True|
|M|sdks/wyrd-sdk-python/python/wyrd/bifrost/__init__.pyi|True|True|
|A|sdks/wyrd-sdk-python/python/wyrd/bifrost/_pydantic.py|False|False|
|M|sdks/wyrd-sdk-python/python/wyrd/stubs/bifrost.pyi|True|True|
|A|sdks/wyrd-sdk-python/python/wyrd/types.py|False|False|
|M|sdks/wyrd-sdk-python/src/bifrost/mod.rs|True|True|
|M|sdks/wyrd-sdk-python/tests/bifrost/test_public_typing.py|True|False|
|M|sdks/wyrd-sdk-python/tests/integration/bifrost/test_bifrost_e2e.py|True|True|
|A|sdks/wyrd-sdk-python/tests/integration/bifrost/test_every_iceberg_column_type.py|False|False|
|A|sdks/wyrd-sdk-python/tests/integration/bifrost/test_register_a_table_from_a_model.py|False|False|
|A|sdks/wyrd-sdk-python/tests/integration/bifrost/test_table_time_column.py|False|False|
|A|sdks/wyrd-sdk-python/tests/integration/bifrost/test_three_timestamp_types.py|False|False|
|A|sdks/wyrd-sdk-python/tests/integration/bifrost/test_variant_tables.py|False|False|
|M|sdks/wyrd-sdk-python/tests/integration/state/test_observe_journey.py|True|True|
|M|sdks/wyrd-sdk-python/tests/integration/test_bifrost_query.py|True|True|
|M|sdks/wyrd-sdk-python/tests/integration/test_drift_journey.py|True|True|
|A|sdks/wyrd-sdk-python/tests/unit/bifrost/test_timestamp_types.py|False|False|
|M|sdks/wyrd-sdk-rust/tests/drift_verification.rs|True|True|
|M|sdks/wyrd-sdk-rust/tests/observe_run.rs|True|True|
|M|sdks/wyrd-sdk-ts/native/Cargo.toml|True|False|
|M|sdks/wyrd-sdk-ts/native/src/lib.rs|True|True|
|M|sdks/wyrd-sdk-ts/wyrd/index.d.cts|True|True|
|M|sdks/wyrd-sdk-ts/wyrd/index.d.ts|True|True|
|M|sdks/wyrd-sdk-ts/wyrd/src/error-codes.ts|True|False|
|M|sdks/wyrd-sdk-ts/wyrd/src/index.ts|True|True|
|M|sdks/wyrd-sdk-ts/wyrd/tests/integration/drift-verification.test.ts|True|True|
|A|sdks/wyrd-sdk-ts/wyrd/tests/integration/every-iceberg-column-type.test.ts|False|False|
|M|sdks/wyrd-sdk-ts/wyrd/tests/integration/observe-run.test.ts|True|True|
|M|sdks/wyrd-sdk-ts/wyrd/tests/integration/oracle-query.test.ts|True|True|
|M|sdks/wyrd-sdk-ts/wyrd/tests/integration/otel-export.test.ts|True|True|
|A|sdks/wyrd-sdk-ts/wyrd/tests/integration/register-a-table-from-a-model.test.ts|False|False|
|A|sdks/wyrd-sdk-ts/wyrd/tests/integration/table-time-column.test.ts|False|False|
|A|sdks/wyrd-sdk-ts/wyrd/tests/integration/three-timestamp-types.test.ts|False|False|
|A|sdks/wyrd-sdk-ts/wyrd/tests/integration/variant-tables.test.ts|False|False|
|A|sdks/wyrd-sdk-ts/wyrd/tests/support/test-server.ts|False|False|
|M|sdks/wyrd-sdk-ts/wyrd/tests/unit/bifrost-query.test.ts|True|True|
|A|sdks/wyrd-sdk-ts/wyrd/tests/unit/timestamp-types.test.ts|False|False|

## All direct conflict paths

- `.claude/skills/wyrd-task-review/SKILL.md`
- `.claude/skills~main`
- `Cargo.lock`
- `Cargo.toml`
- `architecture/bifrost-design.md`
- `crates/shared/workspace-hack/Cargo.toml`
- `crates/shared/wyrd-client/src/bifrost/facade.rs`
- `crates/shared/wyrd-client/src/bifrost/grpc.rs`
- `crates/shared/wyrd-client/src/bifrost/mod.rs`
- `crates/shared/wyrd-client/src/bifrost/query.rs`
- `crates/shared/wyrd-client/src/bifrost/table.rs`
- `crates/shared/wyrd-client/src/error.rs`
- `crates/shared/wyrd-client/tests/pg_bifrost_e2e.rs`
- `crates/shared/wyrd-queue/src/batch_builder.rs`
- `crates/shared/wyrd-queue/src/config.rs`
- `crates/shared/wyrd-queue/src/error.rs`
- `crates/shared/wyrd-queue/src/producer.rs`
- `crates/shared/wyrd-queue/src/queue.rs`
- `crates/shared/wyrd-queue/src/schema.rs`
- `crates/vala/vala-bifrost-redux/src/catalog/bifrost_catalog.rs`
- `crates/vala/vala-bifrost-redux/src/catalog/wire.rs`
- `crates/vala/vala-bifrost-redux/src/forge/expire.rs`
- `crates/vala/vala-bifrost-redux/src/gate/error.rs`
- `crates/vala/vala-bifrost-redux/src/gate/mod.rs`
- `crates/vala/vala-bifrost-redux/src/oracle/admission.rs`
- `crates/vala/vala-bifrost-redux/src/oracle/analytical.rs`
- `crates/vala/vala-bifrost-redux/src/oracle/analytical_supervisor.rs`
- `crates/vala/vala-bifrost-redux/src/oracle/analytical_transport.rs`
- `crates/vala/vala-bifrost-redux/src/oracle/follower.rs`
- `crates/vala/vala-bifrost-redux/src/oracle/mod.rs`
- `crates/vala/vala-bifrost-redux/src/oracle/query_stream.rs`
- `crates/vala/vala-bifrost-redux/src/resources.rs`
- `crates/vala/vala-bifrost-redux/src/scribe/execution_lanes.rs`
- `crates/vala/vala-bifrost-redux/src/scribe/ingress.rs`
- `crates/vala/vala-bifrost-redux/src/scribe/tests/scribe_persistence_path.rs`
- `crates/vala/vala-bifrost-redux/src/storage/mod.rs`
- `crates/vala/vala-bifrost-redux/src/tables/audit/audit_log.rs`
- `crates/vala/vala-bifrost-redux/src/tables/audit/projection.rs`
- `crates/vala/vala-bifrost-redux/src/tables/dev/agent_traces.rs`
- `crates/vala/vala-bifrost-redux/src/tables/eval/observations.rs`
- `crates/vala/vala-bifrost-redux/src/tables/logs/projection.rs`
- `crates/vala/vala-bifrost-redux/src/tables/managed_columns.rs`
- `crates/vala/vala-bifrost-redux/src/tables/metrics/projection.rs`
- `crates/vala/vala-bifrost-redux/src/tables/mod.rs`
- `crates/vala/vala-bifrost-redux/src/tables/signal.rs`
- `crates/vala/vala-bifrost-redux/src/tables/traces/projection.rs`
- `crates/vala/vala-bifrost-redux/src/tables/verification/results.rs`
- `crates/vala/vala-bifrost-redux/tests/integration/forge/compaction_admission.rs`
- `crates/vala/vala-bifrost-redux/tests/integration/forge/promotion.rs`
- `crates/vala/vala-bifrost-redux/tests/integration/forge/publication.rs`
- `crates/vala/vala-sql/src/lib.rs`
- `crates/vala/vala-sql/src/queries/audit_staging.rs`
- `crates/vala/vala-sql/src/queries/oracle_reader_authority.rs`
- `crates/vala/vala-sql/tests/integration/pg_oracle_membership.rs`
- `crates/vala/vala-sql/tests/pg_audit_staging.rs`
- `crates/wyrd-spec/src/vala/api.rs`
- `crates/wyrd-spec/src/vala/assignment_authority.rs`
- `crates/wyrd-spec/src/vala/mod.rs`
- `crates/wyrd-spec/tests/schemas/bifrost_audit_event.json`
- `crates/wyrd-spec/tests/schemas/bifrost_data_type_spec.json`
- `crates/wyrd-spec/tests/schemas/bifrost_field_spec.json`
- `crates/wyrd-spec/tests/schemas/bifrost_physical_layout.json`
- `crates/wyrd-spec/tests/schemas/bifrost_register_table_request.json`
- `crates/wyrd-spec/tests/schemas/bifrost_table_description.json`
- `crates/wyrd-spec/tests/schemas/bifrost_time_granularity.json`
- `crates/wyrd/wyrd-cli/Cargo.toml`
- `crates/wyrd/wyrd-mcp/tests/bifrost/mcp/query.rs`
- `crates/wyrd/wyrd-server/Cargo.toml`
- `crates/wyrd/wyrd-server/src/app/server.rs`
- `crates/wyrd/wyrd-server/src/audit/mod.rs`
- `crates/wyrd/wyrd-server/src/bifrost/gate_audit.rs`
- `crates/wyrd/wyrd-server/src/bifrost/service.rs`
- `crates/wyrd/wyrd-server/src/boot/mod.rs`
- `crates/wyrd/wyrd-server/src/components/gateway/capture.rs`
- `crates/wyrd/wyrd-server/src/http/otlp.rs`
- `crates/wyrd/wyrd-server/src/mcp/bifrost.rs`
- `crates/wyrd/wyrd-server/src/query/service.rs`
- `crates/wyrd/wyrd-server/src/state.rs`
- `crates/wyrd/wyrd-server/src/verification/drift.rs`
- `crates/wyrd/wyrd-server/src/verification/eval.rs`
- `crates/wyrd/wyrd-server/src/verification/observations.rs`
- `crates/wyrd/wyrd-server/src/verification/publisher.rs`
- `crates/wyrd/wyrd-server/src/verification/results.rs`
- `crates/wyrd/wyrd-server/src/verification/runner.rs`
- `crates/wyrd/wyrd-server/tests/integration/pg_grpc_ingest_smoke.rs`
- `crates/wyrd/wyrd-server/tests/integration/pg_verification_routes.rs`
- `crates/wyrd/wyrd-server/tests/integration/pg_verification_runtime.rs`
- `crates/wyrd/wyrd-storage/src/local.rs`
- `crates/wyrd/wyrd-testing/Cargo.toml`
- `crates/wyrd/wyrd-testing/src/bifrost/scribe_workload.rs`
- `crates/wyrd/wyrd-testing/src/bin/bifrost_client_queue/fixture.rs`
- `crates/wyrd/wyrd-testing/src/bin/bifrost_client_queue/main.rs`
- `crates/wyrd/wyrd-testing/src/bin/bifrost_client_queue/proxy.rs`
- `crates/wyrd/wyrd-testing/src/bin/bifrost_client_queue/step.rs`
- `crates/wyrd/wyrd-testing/src/bin/bifrost_query_capacity/main.rs`
- `crates/wyrd/wyrd-testing/src/bin/bifrost_query_capacity/report.rs`
- `crates/wyrd/wyrd-testing/src/bin/bifrost_query_capacity/run.rs`
- `crates/wyrd/wyrd-testing/src/bin/capacity/profile.rs`
- `crates/wyrd/wyrd-testing/src/bin/verification_capacity/main.rs`
- `crates/wyrd/wyrd-testing/src/bin/verification_capacity/step.rs`
- `crates/wyrd/wyrd-testing/src/lib.rs`
- `crates/wyrd/wyrd-testing/src/release_server.rs`
- `crates/wyrd/wyrd-testing/src/server.rs`
- `crates/wyrd/wyrd-testing/tests/README.md`
- `crates/wyrd/wyrd-testing/tests/bifrost/forge/production_closeout.rs`
- `crates/wyrd/wyrd-testing/tests/bifrost/oracle/capacity.rs`
- `crates/wyrd/wyrd-testing/tests/bifrost/oracle/distributed.rs`
- `crates/wyrd/wyrd-testing/tests/bifrost/oracle/peer_cluster.rs`
- `crates/wyrd/wyrd-testing/tests/bifrost/oracle/published.rs`
- `crates/wyrd/wyrd-testing/tests/bifrost/oracle/support.rs`
- `crates/wyrd/wyrd-testing/tests/bifrost/otlp/trace_export.rs`
- `crates/wyrd/wyrd-testing/tests/bifrost/scribe/qualification.rs`
- `crates/wyrd/wyrd-testing/tests/bifrost/scribe/support.rs`
- `crates/wyrd/wyrd-testing/tests/bifrost/scribe/write_read.rs`
- `crates/wyrd/wyrd-testing/tests/bifrost/server/eval_verification.rs`
- `crates/wyrd/wyrd-testing/tests/bifrost/server/main.rs`
- `crates/wyrd/wyrd-testing/tests/bifrost/server/query.rs`
- `crates/wyrd/wyrd-testing/tests/bifrost/server/verification_runtime.rs`
- `crates/wyrd/wyrd-testing/tests/gateway/peer.rs`
- `crates/wyrd/wyrd-tonic/proto/wyrd.v1.bin`
- `crates/wyrd/wyrd-tonic/proto/wyrd.v1.proto`
- `crates/wyrd/wyrd-tonic/src/query_conversion.rs`
- `docs/src/content/docs/bifrost/architecture.svx`
- `docs/src/content/docs/bifrost/forge.svx`
- `docs/src/content/docs/bifrost/reading-data.svx`
- `docs/src/content/docs/bifrost/schema.svx`
- `mise.toml`
- `scripts/checks/object-store-pin.sh`
- `scripts/test-families.sh`
- `sdks/wyrd-sdk-python/python/wyrd/bifrost/__init__.py`
- `sdks/wyrd-sdk-python/python/wyrd/bifrost/__init__.pyi`
- `sdks/wyrd-sdk-python/python/wyrd/stubs/bifrost.pyi`
- `sdks/wyrd-sdk-python/src/bifrost/mod.rs`
- `sdks/wyrd-sdk-python/tests/integration/bifrost/test_bifrost_e2e.py`
- `sdks/wyrd-sdk-python/tests/integration/state/test_observe_journey.py`
- `sdks/wyrd-sdk-python/tests/integration/test_bifrost_query.py`
- `sdks/wyrd-sdk-python/tests/integration/test_drift_journey.py`
- `sdks/wyrd-sdk-rust/tests/drift_verification.rs`
- `sdks/wyrd-sdk-rust/tests/observe_run.rs`
- `sdks/wyrd-sdk-ts/native/src/lib.rs`
- `sdks/wyrd-sdk-ts/wyrd/index.d.cts`
- `sdks/wyrd-sdk-ts/wyrd/index.d.ts`
- `sdks/wyrd-sdk-ts/wyrd/src/index.ts`
- `sdks/wyrd-sdk-ts/wyrd/tests/integration/drift-verification.test.ts`
- `sdks/wyrd-sdk-ts/wyrd/tests/integration/observe-run.test.ts`
- `sdks/wyrd-sdk-ts/wyrd/tests/integration/oracle-query.test.ts`
- `sdks/wyrd-sdk-ts/wyrd/tests/integration/otel-export.test.ts`
- `sdks/wyrd-sdk-ts/wyrd/tests/unit/bifrost-query.test.ts`

## Historical merge-ledger reconciliation

Original retained main ledger: changes/active/verified-change-contract/integration/forge-merge-ledger.md, introduced 31d84f07920d3fc29bbd92fff66e9b625d3b33b8. It analyzes source 509f972c6, not current cfe21dc9. User-mentioned verification-closeout path is completed compact record; no historical merge-ledger match found there. This mapping is a retention constraint summary, not evidence all invariant implementations are equivalent; domain agents validate detailed source semantics.

|ID|Historical topic|Current status|Current handling / evidence|
|---|---|---|---|
|L01|Renumber the audit-progress migration and read the owed-tenant scan from `vala.audit_publication`|OBSOLETE/ADAPT MAIN|Audit staging/publication query no longer exists in main. Keep current migration graph and ScribeOutbox; do not restore entry_hash there. No audit hash/chain owner remains: audit scout confirms main has no seq/entry_hash/prev_hash. Content-derived outbox batch dedup is separate.|
|L02|Pin the compaction core to the ring-free revision and regenerate the lockfile and workspace-hack|ADAPT SOURCE + MAIN|Old c41c/c04c/4e9 pins stale. Current source Iceberg 0991fef, compaction 6965606, DF fd02579, arrow e0f604e, distributed 8fac2a. Keep source compatibility graph + main dependency ownership/no-ring constraint; regenerate artifacts.|
|L03|Fold vala-sql tests into one `tests/integration` binary, including the new Forge leader test|RETAIN MAIN|One vala-sql integration target and sync fixture API remain mandatory; source now touches pg_oracle_membership and adds schema validation tests, so old no-touch assertion stale.|
|L04|Take Forge's active-table-read model over the reader-epoch and protection model|ADAPT BOTH|Active-table read model shared, but source replaces private_conversion ownership and distributed graph APIs. Preserve main expiry/lifecycle safeguards while taking source execution model.|
|L05|Keep Forge query logic in verification's pedantic-lint shape|ADAPT SOURCE TO MAIN|Keep source SQL logic in main pedantic shape; do not choose main purely for style.|
|L06|Combine verification's merged match arm with Forge's `ScanLiteral::Bytes`|ADAPT SOURCE TO MAIN|Source ScanLeaf/In predicate and typed timestamp model wins; reapply main exhaustive lint shape.|
|L07|Keep verification's deletion of retired check scripts and Forge's owner rule in tenant isolation|RETAIN MAIN|Retired script deletion wins; check_tenant_isolation ownership stays main.|
|L08|Keep the checked-in proto descriptor deleted|RETAIN MAIN|Checked-in wyrd.v1.bin remains deleted; source descriptor modifications are a modify/delete conflict.|
|L09|Keep typed `PyDim` docs and add Forge's Python help rustdoc|RETAIN MAIN|Interfaces PyDim not changed in source inventory, main implementation remains.|
|L10|Document correlation columns without `CodeAxis`|RETAIN MAIN|No CodeAxis resurrection in architecture/canonical tables.|
|L11|Type-check both new Python test files in the stub lane|ADAPT MAIN|Keep main story names and lanes, add source new Variant/timestamp journeys; source benches need mapping to main consolidated infrastructure.|
|L12|Keep audit publication continuous (no 5 s tick)|OBSOLETE|No publication.rs exists in main: shared ScribeOutbox replaced periodic and continuous audit publishers.|
|L13|Remove the catalog-transaction audit from Bifrost table registration|RETAIN MAIN/ADAPT SOURCE|Current catalog register_dataset has CompactionRegistration and no audit arg. Keep nonblocking permission audit at surface; adapt source registration types.|
|L14|Remove the standalone audit-denial path from verification control|RETAIN MAIN|Nonblocking AuditStage decision path wins; do not resurrect awaited audit or deny_scope.|
|L15|Drop both the Oracle continuity monitor and the Oracle audit drain from shutdown|ADAPT MAIN|No Oracle audit shutdown; source Oracle shutdown must also retain newer main query-memory drain fixes 71258cc/51dbfc1 and membership drain fixes.|
|L16|Use Forge's managed-column table without CodeAxis or batch id|SUPERSEDED IN PART|No CodeAxis/agent_traces/batch id remains valid; main now card_uid only, source eliminates managed event timestamp. Old managed-column shape cannot be reused wholesale.|
|L17|Resolve the Forge config at boot with the hourly maintenance timer and Forge's reduced field set|ADAPT MAIN|Hourly maintenance/reduced Forge config retained, but current boot also owns selected Scribe routing/frame budget/peer timeout; source schema bootstrap additions layer on.|
|L18|Call Drift verify without the Verifier ref|ADAPT MAIN|Keep current Drift runner surface and principal-aware gateway judging; old no-Verifier-ref arity advice must be checked against current signatures.|
|L19|Call `superuser_pool()` synchronously in Forge-side tests|RETAIN MAIN|Sync superuser_pool API remains; source persistence/shards/tests may still await old fixture API.|
|L20|Box large futures that pedantic clippy flags|RETAIN MAIN|Keep required Box::pin at large futures; source crate additions inherit current lint rules.|
|L21|Drop the tenant argument from `client_from_options` and expect an empty foreign result table|RETAIN MAIN|Three-arg client_from_options + eager built-in tenant table behavior remain; add Variant expectations to modern journey.|
|L22|Delete the reader-authority integration test|RETAIN MAIN|Do not restore obsolete reader-authority tests; port live invariants to current active-table read API.|
|L23|Put Forge's test content into the relocated `tests/integration/` binaries|RETAIN MAIN|Relocated integration module layout wins; port source test content there, preserve main Forge tests.|
|L24|Take the more precise wording in doc-only and literal conflicts|ADAPT BOTH|Main shutdown/text now changed; source full new Analytical architecture means precise wording must describe integrated behavior, not historical chosen side.|
|L25|Keep one `capacity` benchmark and the Forge benchmark, and delete the other three|ADAPT MAIN|Keep capacity and bifrost_forge_capacity. Source adds bifrost_reads/Variant benches and client-queue rename; retain only necessary new Variant proof, adapt to existing bins, do not revive deleted capacity bins wholesale.|
|L26|Measure the Forge backlog from `vala.forge_tasks`, not planning demands|RETAIN MAIN|Current durable ForgeTasks replaces planning-demands; source does not justify resurrecting removed table.|
|L27|Have `LocalServer::start` take both the owner URL and the envelope|OBSOLETE|Current main LocalServer::start has FOUR args (binary, tenants, env, envelope), release_server.rs:200. Owner URL removed with externally supplied Postgres credentials/local bootstrap; do not replay old five-arg advice.|
|L28|Merge release_server process plumbing: OperatorRun, role-aware readiness, joined peer ports|ADAPT MAIN|Keep current OperatorRun, role readiness, joined peer ports, plus newer bootstrap external credential/default local storage changes.|
|L29|Have the release_server peer-port unit test expect the joined-replica range|RETAIN MAIN|Current release_server.rs:898 derives joined peer port expected value via replica_port(JOINED_PEER_PORT,2).|
|L30|Drop `has_demand` from `ForgeWorkflowInspection` and keep the synchronous `superuser_pool()`|RETAIN MAIN|Keep sync fixture helper and no planning demand inspection.|
|L31|Disable the audit publisher in single-table Forge and Oracle journeys|OBSOLETE/ADAPT MAIN|without_audit_publication_for_test no longer exists. Main scopes journeys to shared audit table rather than disabling removed publisher; adapt per-table counters/faults.|
|L32|Remove the `audit` field from `CreateTableRequest` literals|RETAIN MAIN|CreateTableRequest must not carry transactional audit; keep surface audit staged outbox.|
|L33|Call `client_from_options` with three arguments|RETAIN MAIN|Three-arg client builder stays; credential semantics must use current exchange.|
|L34|Add `params: Vec::new()` to Forge-side `BifrostQueryRequest` literals|ADAPT BOTH|Keep main BifrostQueryRequest params and all binding propagation alongside source Variant planner changes.|
|L35|Drop `.await` on `superuser_pool()` in Forge-side journeys|RETAIN MAIN|Sync superuser_pool requirement remains in source journey migration.|
|L36|Keep the verification side's client-owned event-time fix in observation journeys|SUPERSEDED/ADAPT SOURCE|Source replaces wyrd_event_time with explicit table time columns (Eval created_at). Preserve causal client timestamp once, use new type/partition/query window consistently; do not restore managed timestamp.|
|L37|Merge production_closeout: Forge geometry assertion and held-reader constant, verification Duration style|ADAPT BOTH|Keep balanced geometry/held-reader invariant and main Duration style, adapt source Iceberg v3 lineage assertion and source rewrite geometry.|
|L38|Fix pedantic clippy findings in Forge-origin code now that wyrd-testing inherits workspace lints|RETAIN MAIN|Pedantic source-origin fixes mandatory; no blanket lint suppression.|
|L39|Drop the per-constructor `tenant` argument and select the tenant only through `WYRD_TENANT`|RETAIN MAIN|WYRD_TENANT-only selection remains; no public per-constructor tenant added.|
|L40|Keep the injected `Environment` on the client credential and Cards config path|RETAIN MAIN|Injected Environment credentials/config retained; source does not touch credential.rs/card config, automatic merge preserves unless wholesale checkout.|
|L41|Carry both the query `params`/typed terminal and the Forge `compaction_type` through Rust and Python Bifrost|ADAPT BOTH|Keep query params/typed terminal + compaction option, add source richer timestamp/types/Variant model; failure envelope propagation requires source variant errors.|
|L42|Thread the TypeScript `compactionType` through the `TableConfigOptions` object and catalog-result native call|ADAPT MAIN|TS TableConfigOptions/native structured errors wins; source positional calls must be converted. Main credential/native API additions retained.|
|L43|Keep the Python `wyrd.verification` module and the TypeScript `Verification` client deleted|RETAIN MAIN|Deleted Verification client remains deleted; source untouched paths normally delete cleanly but index.ts conflict risks manual resurrection.|
|L44|Remove WyrdTestServer hooks and the duplicate method the merge resurrected in the testing stub|ADAPT MAIN|Generated testing surfaces from current source owners, retain current sanctioned controls; do not resurrect removed hooks. Current main retained audit inspectors intentionally expanded.|
|L45|Resolve Python stub docstring conflicts to the verification text and keep Forge's non-conflicting doc expansions|ADAPT MAIN|Keep modern source signatures/docs, regenerate instead of editing generated stub. Existing manual authoring stub owner guidance now bounded by current generation policy.|
|L46|Regenerate the Python public `__init__.pyi` stubs and the napi `.d.ts`/`.d.cts` from source|RETAIN MAIN|Regenerate Python/TS declared artifacts from resolved source; no manual generated merges.|
|L47|Merge the gRPC transport test conflict: keep the verification wiremock exchange and take the Forge refresh tests|ADAPT MAIN|Keep refreshed-auth gRPC tests/wiremock RFC8693 exchange; new source writer/send semantics layered on current transport.|
|L48|Keep both Python compaction-type and query-param Bifrost journeys|ADAPT BOTH|Keep current compaction/query-params Python journeys and source Variant/timestamp tests under main modern fixtures.|
|L49|Keep the relocated Rust observe-run journey and drop the tenant argument|ADAPT MAIN|Old Rust observe_run modify/delete resolved deleted; transfer source Variant readback to current owner/public story, preserving coverage.|
|L50|Take the verification Python CLI module wholesale|RETAIN MAIN|Python CLI module unchanged in source inventory, stays main.|
|L51|Settle audit staging before the progress row so a freeze never waits|OBSOLETE|Audit staging/settle/freeze API gone; old source test entry_hash/seq/prev_hash assertions are obsolete and must be rewritten for current retained audit fields; do not relocate the removed helper.|
|L52|Keep the tenant audit table out of journeys that count pod-wide or single-table work|OBSOLETE/ADAPT MAIN|Publisher opt-out removed; scope counters/faults to intended tables under shared audit outbox behavior.|
|L53|Reconcile Forge-branch tests and stubs with the TASK-008/TASK-016 public surfaces|ADAPT MAIN|Keep current modern public story/native config/error shape and generated controls; source added Variant tests port to it.|
|L54|Wait for the Oracle envelope release as well as the graph drop before asserting revocation|ADAPT MAIN TO SOURCE|Original Analytical reclaim architecture materially rewritten on source. Preserve invariant drain completes only after graph descendants and owner memory release; also newer main own query-memory drain/headroom fixes.|
|L55|Bring SDK journeys in line with the TASK-016 query, drift, and binding surfaces|ADAPT MAIN|Main current observe/verify/binding story API authoritative; source Variant readback transferred, no deleted journey resurrection.|
|L56|Drop merge-hygiene leftovers|RETAIN MAIN|No blanket type_complexity allow or plan/agent markers in permanent code.|
|L57|Publish audit continuously instead of on a fixed 5 s tick|OBSOLETE|Entire audit publication replay superseded by main shared ScribeOutbox, retained audit history, no staging/publisher.|
|L58|Carry main's OIDC and workflow-runtime merge and its repairs|RETAIN/ADAPT MAIN|OIDC/workflow/runtime/renewable credential deltas absent source remain main. Main subsequently rewrote roles/principals, scope, judges, local bootstrap; these additions also retained.|
|L59|Select the tenant only through WYRD_TENANT|RETAIN MAIN|Tenant selector is WYRD_TENANT/config, credential bounds tenant; no fourth builder arg.|
|L60|Bound snapshot-expiry authority by the lease and refuse cuts on an unknown expiry|ADAPT MAIN|Preserve unresolved expiry fail-closed/lease-bounded authority against source new catalog/time schema. Main catalog/error.rs has UnresolvedExpiry mapping; source catalog conflicts risk silently dropping safeguard. Migration checksum needs explicit current audit, no automatic reset.|
|L61|Linearize Forge leader handlers with the term|RETAIN MAIN|Keep leader term linearization and quiescence; source also touches metrics now, old no-touch statement false. New main 3bf7b0d/3af5bdb recovery fixes essential.|
|L62|Pin the compaction fork without the unconsumed planning surface (superseded)|OBSOLETE/ADAPT SOURCE|Old combined fork pins superseded by current source compaction 6965606; actual ancestry/features must prove removed REST/ring/planning surface stay removed.|
|L63|Forge packet review records and authority docs|ADAPT BOTH|Current main global authority/standards win; source Bifrost doctrine/type/query rewrite sections replace old Bifrost only. Preserve main shutdown/expiry/roles details.|
|L64|Wait for a routable Oracle at test-server start and keep audit publication out of the stage-pressure journey|ADAPT MAIN|Current test server readiness checks oracle.is_ready server.rs:5460; source distributed model must keep true routability. Old audit-publisher opt-out is obsolete; adapt scoped shared-audit fixtures.|

### S1–S4

- S1 RETAIN MAIN: current pedantic lint table, per-crate workspace lint inheritance, reasoned narrow expectations, injected Environment, pure synchronous helpers. Source is not automatically compliant.
- S2 RETAIN MAIN: retired checks/scripts/tasks remain deleted; do not restore source object-store-pin.sh/test-families.sh or their callers; check:deps owns narrow dependency checks.
- S3 RETAIN MAIN: one integration test binary, module exact filters, sync superuser_pool, modern nextest groups; source old files remapped to owning integration modules.
- S4 ADAPT MAIN: consolidated capacity binaries/current dev profile/proto descriptor deletion retained; new source Variant read benchmark may be earned but old capacity copies are not.

## Main chronological preservation after historical ledger

The ledger was written 2026-10-07 at 31d84f079. Main now includes:

|Merge commit|PR / date|Retention seams|
|---|---|---|
|c46afdcac8e86bf27334773a4a85df9404d46202|#139 verified-change-contract, 2026-10-08|One shared client, principal-aware permissions, public journey renames, MCP investigation surfaces, pedantic cleanup/test consolidation/schema fixture relocation|
|9b648fe7965e4236784315540912ec2c10a6f4c5|#141 verification-closeout, 2026-10-09|Shared ScribeOutbox; four roles/principal role assignment; unbound/bound UID scope; Service eager tables; application Run correlation; principal-owned gateway judges; direct result evidence; telemetry; role/membership shutdown; Forge recovery quiescence; Oracle scoped drain|
|da4620c0a177b496ed647217e8167f36002c6767|#146 add-server-mcp-cli-commands, 2026-10-10|Server/MCP CLI commands and investigation guides; state/boot/router merges must not remove their setup/routes|
|b231adb3384e601102991070e530dd54b287bc4b|#147 local-server-bootstrap-task-002, 2026-10-10|Verified install/signed release checksum; packaging/release scripts and toolchain metadata stay main|
|cb5656b2b913dbd88a8f6736bf4a0baaf998960c|#148 local-server-bootstrap-task-003, 2026-10-10|Signed platform bundles/release build contract; source dependencies must work in that build/profile/features|
|8a8813a7458b7c98707c7b4f9b9d403f604ee0e6|#149 local-server-bootstrap-task-004, 2026-10-11|Installed local dev startup, externally supplied Postgres credentials, default local storage, loopback gRPC selection, child-exit watching, initial host offer only|

Closeout refinements absent historical ledger: 21d02b174 keeps polling/heartbeats through drain; 740129647 reserves half shutdown budget for Bifrost; 6aaccebf9 repairs routing/self-reclaim/Oracle settlement; 71258cc8c drains Oracle query memory only; 51dbfc1fd attributes Oracle infallible headroom; 3bf7b0d97 fast reclaim only after old loop joins; 3af5bdb5b owns quiescence over recovery and Prepared renewal; dd14c0436 merges frame budget/peer timeout with boot-selected route; 37156ea9c stages verification result before completion commits. Exact semantics belong domain reports.

## Clean-merge and unrelated-change boundaries

76 changed paths merge automatically despite both sides changing them; see /tmp/wyrd-merge-auto-overlap.txt. A clean merge is no semantic proof. Especially inspect initial olap migration, catalog errors, Forge lifecycle SQL, distributed codec/peer/planner, Scribe recovery/sharding, server config/routes/scheduled queries, shared-client handle/observe, error catalog and generated schema paths.

Main-only Cards registration, Operator/Trigger dispatch, Workflow gateway/runtime, policy and auth paths ordinarily remain unchanged in Git merge. Wholesale source copies of state.rs, boot/mod.rs, app/server.rs, SDK index.ts/native lib.rs, or manifests can sever these newer callers without text conflict. Preserve current constructors/fields/routes and alter only Bifrost-owned dependency wiring.

Source incidental non-Bifrost implementation changes:
- wyrd-bench/src/lane.rs changes ok().is_some_and to is_ok_and: trivial simplification, no new behavior required.
- wyrd-storage/src/factory/gcs.rs exposes service_account_json and factory/mod.rs exposes pool_idle_timeout for Bifrost reuse: source storage dependency needs these owner APIs.
- wyrd-storage/src/local.rs changes pure signing methods from async to sync; signer.rs introduces common boxed SignerFuture, consolidating cfg cloud/non-cloud operations. This is outside core Bifrost rewrite and needs explicit retention decision against main minimal helper structure; existing public awaitable BackendSigner remains shape-compatible, direct LocalSigner calls change.
- wyrd-storage/src/telemetry.rs changes mock writer/deleter APIs to ready futures/MaybeSend for dependency API compatibility.
- wyrd-sql/src/queries/verifier_runs.rs changes EvalRecord docs from managed event time to created_at: semantically part of schema-time attribution, not cosmetic if associated runner queries remain old.
- .agents/skills/wyrd-task-review/SKILL.md and .claude mirror source workflow edits are not Bifrost product behavior. Keep main .claude/skills symlink and current canonical skill authority; no source mirror resurrection.
- Source contains 148 changes/active/bifrost-variant records; these are source evidence/history, not required runtime imports. Keep historical evidence reachable by pinned source ref and avoid silently claiming obsolete active packets are current main work.
- Fixtures every_iceberg_type.arrow/every_iceberg_type.py/narrow_types.arrow require mapping to main consolidated fixture owner and tracked-size cap, not blindly copying legacy fixture paths.

## Research limits

This inventory is static/read-only. No integration worktree, build, tests, runtime migration, benchmarks, or fork rebuild executed. git merge-tree wrote only temporary object-store merge data, not an index/worktree merge. Main working tree remains clean. Direct conflict inventory is exact for pinned SHAs; semantic behavior and current dependency feature equivalence require domain reports plus integrated verification.


---

## Archived snapshot: wyrd-merge-membership.md

# Membership, peer roles, and shutdown merge research

Pinned evidence: main/origin/main `8a8813a7458b7c98707c7b4f9b9d403f604ee0e6`; source `origin/wyrd/bifrost-variant/TASK-003` `cfe21dc9007d5d22d9ae4411be07fb5504b0cc05`. Read-only source comparison; no runtime checks run. No `.codegraph` root exists, so ordinary git source/search used. Agent rules, Wyrd design/doctrine, Bifrost design inspected.

## 1. Preserve main's final membership lifetime repair, not merely its first repair

**Definite direct/silent regression:** source `crates/vala/vala-bifrost-redux/src/cluster/mod.rs` has `start_readiness_heartbeat(registered, ready, shutdown) -> JoinHandle<()>` and `start_snapshot_poller(shutdown) -> JoinHandle<()>`. Source `crates/wyrd/wyrd-server/src/state.rs` `Oracle::new` lines ~527-532 and `Scribe::new` ~865-870 give both tasks the process `role_shutdown` token. Those tokens are boot-supplied process cancellation and stop at supervised drain, leaving the shared registry frozen. Source starts one poller per selected role. Router-only composition has neither role's poller.

Main first corrected this in `21d02b1748269fd04bd56807dfe1f5104c3efb82` with role-owned membership tokens. The later `6aaccebf9cc9e54a25995a86286e09cec3da4e7c` replaces that intermediate mechanism with `ClusterTask` and exactly one process poller; this is the state to retain. `ClusterTask` owns its cancellation token, join handle, abort handle, idempotent deadline-bounded stop, and abort-on-drop for failed boot.

Main boot `compose_bifrost` ~1058 starts one `cluster_registry.start_snapshot_poller()` and passes it in `BifrostComposition.membership_poller`. Roles each own their independent `ClusterTask` heartbeat. `process_drain` cancels request/engine work but does not cancel the heartbeat or poller. `Oracle::shutdown_owner` stops heartbeat **after** engine drain; Scribe stops its heartbeat **after** ingest drain. `Bifrost::drain_selected_owners` stops the process poller after Oracle/Scribe teardown, and before storage close. Abort explicitly aborts poller; construction Drop protects failed boot.

**Port guidance:** retain main cluster task APIs, main Bifrost composition/poller fields and ownership, main `process_drain` distinction, and main teardown order while connecting the source engine changes. Do not retain source per-role snapshot pollers, process-token task ownership, or source's synchronous Bifrost abort. Keep the older `await_role_task` helper only for genuine remaining Tokio-owned tasks (WAL fault monitor), as main does. Keep readiness withdrawal separate from heartbeat cessation: `begin_shutdown` clears readiness while fencing heartbeat continues without advertising ready again.

**Why source authority over Bifrost does not justify copying this:** this is a main bug repair to process lifetime/route availability, not an alternative analytical engine design. Source contains no equivalent fix.

**Must retain main tests:**
- `crates/wyrd/wyrd-testing/tests/bifrost/server/owner_inspection.rs::unready_roles_heartbeat_until_teardown_ends_membership_tasks` checks durable heartbeat timestamp advances after readiness withdrawal, stays unready, and shutdown/abort end every poller/heartbeat. Source removes it entirely.
- `.../server/query.rs::oracle_only_pod_retains_audit_staged_before_its_drain`: delay final Scribe boot, stage audit on Oracle-only pod with no ready Scribe, boot Scribe and immediately drain Oracle; another Oracle reads retained audit. Source lacks this main repair. It proves roster stays current during the outbox drain.
- Main `query.rs::prove_scheduled_analytical_peer_loss` waits until the abruptly killed follower leaves membership before its audit-count Analytical query. Preserve that synchronization: counting after process death while its lease is still live wrongly diagnoses expected stale membership as analytical regression.
- Main ClusterTask unit tests `stop_joins_the_task_once`, `stop_aborts_a_task_past_its_deadline`, `abort_and_drop_end_the_task` are absent source; preserve.

## 2. Resolve peer-role terminology against actual tips, preserve independent behavior

There is **no exact `ScribePeer` Rust type or protobuf service at pinned main**, nor a pair of independent `ScribePeer`/`OraclePeer` protocol services. `git grep ScribePeer main` finds only main `ScribePeers` shared-outbox route. Both trees mount one `OraclePeerGrpc` implementing protobuf `OraclePeerService` on the private mTLS listener (`crates/wyrd/wyrd-server/src/grpc/mod.rs::build_peer_grpc`, main ~409). Its Scribe operations are separated behaviorally into `ScribeFragmentExecutor`; absent capability fails closed. Do not manufacture a new protocol split just to satisfy the remembered names.

**Already correct in both:**
- `ScribeFragmentExecutor::execute` takes only `Arc<Scribe>`, validates target role = Scribe, exact node/fence, nil Oracle reservation, then uses Scribe's fragment verifier/resources/follower. It does not require a co-located Oracle, does not reserve an Oracle slot, and does not look up a local Oracle readiness bit.
- `OraclePeerGrpc::execute_fragment` selects Scribe owner for Scribe target; refuses Oracle-target physical fragments because Oracles run Analytical stages, not Scribe fragments.
- `oracle/dispatcher.rs::resolve_snapshot_candidate` matches role: Oracle looks up live_oracle; Scribe looks up live_scribes. It checks the selected role's readiness, age, fence, HTTPS, not the sibling role.
- `ClusterSnapshot` composite key `(NodeId, ClusterRole)` and `ClusterNodes` independently fenced durable registration are equivalent in both. `cluster_nodes.rs` diff is just raw-string formatting.
- Private listener mounts tail/capture only for local Scribe and Analytical worker only for Oracle. Preserve mTLS-only listener, role-neutral cert identity, exact target fence, and no health/reflection there.

**Source-compatible adaptation:** source removes persisted-file/predicate fields from Scribe assignments and changes the physical follower/extension codec. Keep that new assignment shape, projection/provider-cut authority validation, and body fingerprint revision binding; do not copy main's removed assignment fields into source engine just to preserve the role separation. Keep Scribe-only execution/negative tests against the source shape.

**Existing evidence:** `.../oracle/peer_network/security.rs::peer_context_refusals` boots `[Oracle, Oracle, Scribe]`; helper `a_foreign_tenant_context_reads_no_scribe_rows` proves a correct fragment runs on Scribe-only process and a foreign-tenant context runs none. Both tips contain this behavior, adjusted assignment shape differs. Existing SQL integration tests `pg_oracle_membership::same_node_roles_have_independent_fences`, `stale_role_mutation_is_rejected`, `live_discovery_excludes_expired_heartbeat` must stay in main consolidated `tests/integration` binary: source's standalone `tests/pg_oracle_membership.rs` is old layout.

## 3. A different peer coupling really does regress: internal Scribe write route

Source `crates/wyrd/wyrd-server/src/components/gateway/capture.rs::CaptureWriter::for_bifrost` (~698) matches `(bifrost.scribe(), bifrost.oracle())`: local Scribe wins; otherwise it requires an Oracle and retrieves TLS using `oracle.lifecycle_transport().peer_tls()`. No local Scribe and no local Oracle => permanently unavailable, even if that process has peer TLS and the fleet has a ready Scribe. Source `oracle/lifecycle_transport.rs` reintroduces the `peer_tls` accessor just for that dependency.

Main `scribe_outbox.rs::ScribeRoute::select` instead takes boot-resolved local Scribe, registry, and peer TLS directly; main `ScribePeers::channel` reads `live_scribes`, filters ready, sorts node ids, rotates across Scribes per retry, evicts disappeared endpoint cache entries, and uses a bounded timeout. Main `6aaccebf9` performs routing repair; `dd14c0436` merges configured frame budget/peer timeout. This route is independent of Oracle existence/readiness.

**Port guidance:** retain boot-selected main route and shared process outbox for audit, captures, results. Adapt its Arrow rows/attribution to source schemas. Do not restore `CaptureWriter`, `CapturePeers`, `OracleLifecycleTransport::peer_tls`, separate audit publisher/outbox/query audit owners, or `verification.ingest_endpoint`. This is the concrete source regression related to the user's role-separation concern.

**Additional direct regression:** source grpc capture peer server drops main `.max_decoding_message_size(transport.message_limit_bytes())`. Keep main configured decode ceiling: tonic default 4 MiB otherwise rejects an outbox frame already legitimately admitted under larger configured Scribe ceiling. Source capture channels lack main per-call timeout, so a silent peer may hold tenant slice. Retain main timeout and classification/retry tests while adapting source envelope.

## 4. Peer audit changes must be spliced, not wholesale accepted

Source `oracle/peer.rs::PeerSecurityAudit` is async append-returning-Result; adds `PeerSecurityError::AuditUnavailable`. Source `peer_service.rs::audit_denial` waits durable append and returns `Unavailable` if it cannot commit. Main synchronously stages denial on process Scribe outbox and preserves denial regardless of audit storage availability. Main `stage_unverified_ticket_rejection` routes unverified claims to trusted SYSTEM_OWNER; `stage_verified_ticket_violation` uses receiver-verified tenant state. Retain main signatures and behavior across source follower/worker authorization; remove reintroduced audit failure cases. Adapt source's DataFusion-revision stage body digest and new assignment authority shape without weakening its validation.

This must reach peer authority, peer service, source analytical stage authority, conversion/status mappings, unit recording audits, receiver rejection tests, and boot security collaborator composition. Merely selecting main `peer.rs` loses source new digest binding/assignment fields; merely selecting source restores blocking audit.

## 5. Other process seams and targeted evidence

Source `app/server.rs` removes main accepted-Workflow drain before supervised teardown, process Scribe outbox drain before Bifrost teardown with half the remaining budget, and observation-run outbox drain after Gate/Scribe closes. Retain these main ordering hooks. Preserve main composed error that retains both original terminal and later Bifrost teardown error. Main `Bifrost::abort` awaits storage abort; source `abort` is sync and lacks storage settlement. Engine agent covers storage details but process-callers must keep `.await`.

Source adds `TaskId::Worker("oracle_cut_cache")` running `catalog.watch_cuts(shutdown)` under process cancellation. This is a new source engine requirement; retain it, but engine owner must confirm behavior after watcher cancellation: source cached-cut reuse cannot remain valid during drain without notifications. Membership task lifetimes must not be conflated with this independent cache watcher requirement. No runtime proof run in this research.

Recommended integration evidence commands, using main lane/test layout and adjusting only genuine renamed tests after source port:

```
mise run test:bifrost:journey:server
mise run test:bifrost:journey:oracle
mise run test:bifrost:integration:sql
mise run test:server:peer
```

Focused named tests (repository Postgres setup/migrations included; actual source names inspected, not guessed):

```
mise exec -- scripts/postgres/with-test-postgres.sh -- bash -lc 'mise run db:migrate:inner && cargo nextest run --locked -p wyrd-testing --test server -P journey --run-ignored=all -E "test(=owner_inspection::unready_roles_heartbeat_until_teardown_ends_membership_tasks) | test(=query::oracle_only_pod_retains_audit_staged_before_its_drain)"'
mise exec -- scripts/postgres/with-test-postgres.sh -- bash -lc 'mise run db:migrate:inner && cargo nextest run --locked -p wyrd-testing --test oracle -P journey --run-ignored=all -E "test(=peer_network::security::peer_context_refusals)"'
mise exec -- scripts/postgres/with-test-postgres.sh -- bash -lc 'mise run db:migrate:all:inner && cargo nextest run --locked -p vala-sql --test integration -E "test(=pg_oracle_membership::same_node_roles_have_independent_fences) | test(=pg_oracle_membership::stale_role_mutation_is_rejected) | test(=pg_oracle_membership::live_discovery_excludes_expired_heartbeat)"'
```

No new runtime/network behavior proposed beyond preserving these proven main repairs and wiring source new engine contracts. No tests or benchmarks run; this document identifies proof to run on integration tree, not claims of passing merged behavior.

## Follow-up: cache watcher cancellation counterevidence and accepted Workflows

### Normal cut-listener shutdown is safe, so do not extend its lifetime solely by analogy with membership

Source `catalog/cut_cache.rs::CutCache::watch` uses the process shutdown token in its receive `select!`. Cancellation breaks that listener loop, then executes `self.reset(false)` after the loop, and again at method exit. `reset` increments the fill generation, clears every entry, and sets `live=false`. `get` returns no cut when `live=false`; `fill_generation` returns None; a database read already in flight under the old generation cannot refill afterward because `insert` requires both matching generation and live listener. `BifrostCatalog::read_cut_cached` then falls through to `read_cut` against Postgres. This directly disproves the proposed frozen-cache regression during ordinary supervised process drain. Keep the source watcher on its normal process token; unlike membership, stopping it safely removes the optimization instead of freezing authoritative route state.

The cache is not protected by a fixed cache TTL. Each lookup requires cached `read_at >= not_before`; Oracle supplies deadline minus maximum query deadline. Source module docs rely on retained replaced snapshots for at least that maximum query deadline. Snapshot retention correctness and max-deadline propagation are engine review subjects independent of orderly listener cancellation. Source `catalog/cut_cache.rs::tests::cut_cache_serves_only_fresh_listened_cuts` already checks final `reset(false)` causes misses, bound rejects old reads, named-table invalidation, duplicate-table miss, malformed invalidation clears all, and invalidation-raced fill rejection. It does not drive the actual watcher cancellation branch against a real LISTEN connection. If asserting listener lifecycle in integration acceptance, one focused Postgres test should warm a cut, cancel/join watcher, change the table pointer/hot set, and prove next `read_cut_cached` gets the new database cut. No such runtime test was run here.

A forcibly aborted task does not execute post-loop cleanup automatically; that is a distinct hard-stop concern, not evidence of ordinary cancellation failure. The normal supervisor first requests cooperative cancellation. No recommendation to add another cache lifecycle abstraction from this hypothetical alone.

### Workflow preservation reaches state ownership, boot, and tools, not merely one drain call

Source-to-main endpoint diff deletes all five server accepted-Workflow files `components/workflow/{host,mod,routes,runs,tools}.rs`. It removes `AppState.workflows: Arc<WorkflowRuns>`, `AppState::new` composition using `WorkflowRuns::new(config, &shutdown_token)`, `AppState::with_workflow_config`, `components/mod.rs` export, and boot `.with_workflow_config(config.workflow.clone())`. Consequently copying source `state.rs` then reinserting `BoundServer::run`'s drain hook is incomplete; the owner and its cancellable admission/tracker must remain main-native as a connected unit.

Main `WorkflowRuns::drain` closes admission under the same lock as reservation installation, cancels the parent's admission token, closes tracker, and waits until the existing absolute process deadline. Preparations are tracked before their task exists; CPU-bound hydration uses tracked `spawn_blocking`, so aborting the waiting future does not make it vanish from shutdown accounting. Main `Preparation::run` commits accepted run's terminal snapshot only after `RunTools::drain` joins every query owner. `RunTools::QueryTool::invoke` runs a `BoundedQuery` on tracked work; aborting an Agent step signals but does not detach query settlement. Source Bifrost must continue implementing the current server `BoundedQuery`/query-control seam these preserved tools use; source engine changes must not bypass owner-terminal settlement with client-disconnect-only cancellation.

Keep main ordering: accepted Workflows drain first while their Bifrost/gateway dependency services remain available; then supervised process teardown, MCP/gateway tracker accounting, Scribe outbox half-budget drain, Bifrost role/storage drain, observation-run outbox. This ordering is stronger than simply retaining all fields and compiling.

Existing focused proof, no new test demanded if the preserved cases still prove the final engine seam: main server integration `pg_workflow_runs::lifecycle_races_retention_and_shutdown`; main Oracle journey `workflow::workflow_forwarded_query_settles_before_the_run_ends`; WorkflowRuns units `components::workflow::runs::tests::drain_waits_for_a_reservation_before_its_task_exists` and `drain_waits_for_blocking_work_its_caller_abandoned` (confirm exact test module name with nextest list before final named command, tests source location inspected). These should survive source engine port and prove terminal/cleanup rather than only HTTP response completion.

### Three-way merge wording correction

The earlier wording "source deletes Workflow files" describes the endpoint diff only. The pinned common base is `5ab92b0039404565e51f4a246863ce90cbd8d11f`: base→source has no `components/workflow` changes, while base→main **adds** all five files. Source never received these main additions; it did not deliberately delete them. An ordinary three-way merge preserves main-only files automatically. The regression risk is selecting complete source `state.rs`, boot, components exports, config, or server shutdown chunks and removing the main-added owner's wiring. Keep the connected main Workflow capability in each conflict resolution, while accepting source analytical changes at the specific engine seam.


---

## Archived snapshot: wyrd-merge-migrations.md

# Migration history and retained Bifrost data: merge decisions

Compared immutable main 8a8813a7458b7c98707c7b4f9b9d403f604ee0e6 and source cfe21dc9007d5d22d9ae4411be07fb5504b0cc05. Static research only.

## 1. Fresh Bifrost is the source contract; do not invent compatibility

Source `changes/active/bifrost-variant/spec.md:32` says: “Bifrost has not shipped. No existing table, file, or data is migrated.” Source `architecture_ref.md` TASK-001 explicitly says fresh Iceberg-v3-only, no migration or mixed-version rollout. Source spec around 613 reinforces that non-v3 input is unsupported. Honor this in the integration branch. Do not promise that preserving PostgreSQL migrations upgrades old Bifrost data.

Existing deployments with identity/cards/credentials but **no retained Bifrost data** must remain eligible: preserve main's platform/auth SQL and historical checksums, then apply additive Bifrost SQL changes. A populated old Bifrost installation is not a supported in-place upgrade of this source rewrite. Require an operator decision to retain the old deployment separately or explicitly discard/reinitialize only Bifrost state before deployment. Do not reset the whole database and destroy main-owned identities/cards/credentials. This research does not authorize any reset.

If retention is required, stop deployment; a separately specified data export/import or migration project is required. It is outside this source-authoritative merge and cannot be replaced by relaxing format/schema checks.

## 2. Preserve main SQL migration history byte-for-byte

Between tips, Vala migration directory has 20 modifications, 5 additions, 2 deletions; Wyrd SQL has 21 modifications, 1 addition, 8 deletions. Much of this is branch-old RLS/grant policy, not Variant functionality. Do not accept a directory-level source checkout.

Example `crates/vala/vala-sql/migrations/20260619000001_olap_minimal.sql`:
- main SHA256 e7c5ec6049618065d6e38bd9835839b68fc7ea3b479aecd86322ed0882aeca8e
- source SHA256 a6e5983db0e1fa44db70a55d4e741de85def918300537c0a96374f35d520e4f3
- main SQLx SHA384 cc969014192614cc17b29737987673ba499fb78f5cea7b61ff0462abf03c1602043ed54d1257e2aa3be862c4677612a4
- source SQLx SHA384 ff1e222fa762d3861d6c273086d683a593c0a807ac5bd7bcb980bd96459df7f0bb65627771a9409d2008d097d3f0e415

Source modifies layout comments (three keys -> four, explicit time_column), removes main's operator_access policy, adds wyrd_app grant. Even comment-only changes invalidate applied checksums. Keep main historical bytes and place any genuinely required new DDL in a later forward migration. The policy removal is contrary to current main's OperatorPool model; retain main RLS/operator semantics, reconcile new tables/functions to those semantics. Do not restore BYPASSRLS ownership/grant assumptions from source historical SQL.

Both owners embed sqlx migrators. Vala `src/lib.rs::MIGRATOR`, `migrate`, `verify_schema`; Wyrd `schema_check.rs::verify_migrations` checks each embedded version against ledger checksum and rejects mismatches as MigrateChecksum. An edited history is a concrete startup blocker, not theoretical.

Source Vala `src/lib.rs::tests::forge_and_oracle_migrations_have_unique_forward_owners` hardcodes source's a6e... SHA256 for that historical file. Preserve main's checksum assertion with main's file; do not bring this source assertion across unchanged. This is a silent test seam accompanying the SQL decision.

## 3. Main retirement migrations remain authoritative

Keep Vala `20261003000200_audit_publication_progress.sql` and `20261008000000_drop_audit_staging.sql` in history; source deletes both. The latter drops audit_publication, audit_staging, audit_chain_head, audit_staging_immutable. Do not restore source staging writers or assume deleted tables exist. Main's in-memory outbox is authoritative; source retained audit SQL is not a reason to revive staging.

Keep main Wyrd auth/credential/device/audience/result-retirement migrations deleted in source. Source retains base `20261002000000_workload_role.sql`; main intentionally deleted it and replaced the workload role with its builtin-role rewrite (035be3105). Normal three-way merge preserves that deletion. Only a wholesale source directory transplant resurrects the old file and collides with main `20261002000000_auth_device_authorizations.sql`. Preserve the deletion and current main workload implementation. No renumbering or forward workload DDL is needed.

## 4. New Bifrost migrations port forward, with main tenancy semantics

Source delta from common base contains **three new migrations** and one historical modification (20260619000001). The following old grant files are unchanged from base in source and intentionally deleted in main:
- `20260801000002_vala_app_schema_usage.sql`, `20260801000003_grant_vala_schema_usage.sql`: retain main deletions; normal three-way merge does so. Do not resurrect these through source directory transplant.

Actual new source migrations:
- `20261009000000_bifrost_snapshot_retention.sql`: removes oracle_acquire_table_cut and oracle_active_table_reads, introduces oracle_read_table_cut and forge_snapshot_supersessions. Retain source Bifrost behavior, confirm migration applies after untouched main history and that new table has current main tenant/operator policies. This DDL changes reader protection; do not mix older reader-lease methods with source query code.
- `20261010000000_bifrost_cut_notify.sql`: transactional file_list trigger NOTIFY invalidates read-cut cache. Preserve source notification protocol and listener. Ensure unique version/order relative to latest integration head; install trigger once.
- `20261011000000_scribe_wal_fenced_lsn.sql`: index and narrow SECURITY DEFINER newest acknowledged WAL fence. Retain source recovery authority; apply privilege/search_path review under current main operator role model.

Write future forward migration only for DDL difference needed by chosen integrated code; do not mechanically replay all historical source edits. Fresh database replay and existing main database migration must both produce the same target schema. New migration cannot rewrite Parquet schemas, protobuf attributes, Iceberg table version/lineage, WAL records, or old schema identities.

## 5. Old schemas fail closed before useful access; no exhaustive compatibility layer

Source `catalog/bifrost_catalog.rs::create_table_locked` (~1012) compares existing control-row fingerprint before layout, physical schema, or returning AlreadyExists. `validate_physical_table` (~1168) requires FormatVersion::V3 and exact expected physical schema. New tables are constructed as V3 (~1122). `ensure_builtin` (~904) calls this path; built-in `describe` (~1419) calls ensure_builtin and ingress also calls ensure_builtin (`scribe/ingress.rs:206`). Thus old persisted built-ins can fail on first describe/ingest rather than necessarily at process startup. Do not describe successful startup as evidence old tables are compatible.

Changing main system table card_ref -> card_uid into source's explicit timestamp/Variant schemas yields final new fingerprint; this must be used consistently by producer, describe, queue, Scribe, footer and Forge. Do not overwrite stored fingerprint to silence conflict: it certifies exact schema and can expose incompatible files as apparently current.

Existing physical_layout JSON has old three-key layout and wyrd_event_time partitioning; source expects four keys including time_column. SQL JSON backfill alone does not rewrite Iceberg partition spec or files. Payload text/binary -> Variant, removed managed event time -> explicitly typed table columns, and card_ref -> uid are separate data changes. No evidence supports a generic read conversion for every old object. Preserve source rejection and document the fresh-only transition.

## Required validation, not executed here

- Main database containing applied main SQL migrations and identity/card/credential state, but no old Bifrost objects: forward migration succeeds, checksums unchanged, identities/cards/credentials survive, new Bifrost tables initialize as v3.
- Fresh database replay: equivalent final table/function/policy/grant schema.
- Duplicate version scan across each migration owner; confirm deleted base workload/grant files remain deleted and were not resurrected by directory transplantation.
- Schema verification through serving role + operator role, proving main tenant isolation and new SECURITY DEFINER grants.
- Fresh integrated built-in describe/write/read and source wrong-format rejection, final uid/time/Variant schemas exercised.

Operator knowledge not available from git: whether any deployment has retained old Bifrost tables/objects/WAL that cannot be discarded; existing applied migration ledger tips/checksums; external fork pin availability; acceptable separate retention/decommission procedure. These affect deployment eligibility, not the source rewrite's architecture.

Skipped live migration/data/runtime tests. No production compatibility or deletion authorization established.

## Superseding classification correction

Endpoint diff status is not branch ancestry. The counts above describe main-to-source differences, **not newly introduced source edits**. Verified base 5ab92b0039404565e51f4a246863ce90cbd8d11f -> source: only historical OLAP migration 20260619000001 is modified; only three new Bifrost migrations 20261009/10/11 are added; no Wyrd SQL migrations change. Base -> main confirms deletion of both old Vala grant files and workload_role.sql. Therefore the workload numeric collision is a **wholesale source transplant hazard, not a normal three-way merge conflict**. Retain main deletion and builtin-role implementation; do not renumber or add workload DDL. Likewise retain main deletion of old schema grants. The other historical endpoint differences identify main evolution that must survive, not source-requested modifications.


---

## Archived snapshot: wyrd-merge-surfaces-gates.md

# Public surfaces, SDKs, dependency and verification merge research

Pinned trees: main `8a8813a7458b7c98707c7b4f9b9d403f604ee0e6`; source `cfe21dc9007d5d22d9ae4411be07fb5504b0cc05`; merge-base `5ab92b0039404565e51f4a246863ce90cbd8d11f`. Read-only source/diff research; no build, tests, package resolution or runtime claims. Sources below are relative paths at these immutable trees. Source owns Bifrost engine and its new storage semantics; main owns auth, surrounding product behavior, simplifications and current testing/tooling.

## 1. Bind parameters are a major silent contract conflict

Main commit `82972f9f9` adds ordered typed bind values to the public query request. Main `crates/shared/wyrd-client/src/bifrost/facade.rs` exposes `sql(query, &[QueryParam])`, `sql_as(query, &[QueryParam])`, `stream(query, params, deadline)` and re-exports `QueryParam` from `bifrost/mod.rs`. Main `sdks/wyrd-sdk-ts/wyrd/src/index.ts` has `sql(query, params?, rows?)`, and main Python has corresponding ordered params. Source retains old Rust `sql(query)` and old TypeScript `sql(query, rows?)`; its stream/native request construction lacks params. Replacing a whole SDK file or taking source facade reinstates interpolation temptation and loses a released contract even if source code compiles.

Combine source engine planning/Variant support with main typed `QueryParam`, JSON request `params`, protobuf query request params and all conversions. Preserve source result conversion on main APIs rather than accepting source API signatures. Rebase each new source journey call to main signatures (Rust supplies `&[]`; Python/TS retain model projection in its correct parameter slot). Preserve main existing fixture query stories: Rust `sdks/wyrd-sdk-rust/tests/integration/query_bifrost.rs`, Python `tests/integration/test_query_bifrost.py`, TS `tests/integration/query-bifrost.test.ts`. These prove bound data stays separate from SQL. Main exact source `bifrost::query::tests::sql_forwards_bind_values_without_interpolation` in `wyrd-client` is existing focused proof, unlike a new proposed scenario. Main commit `aa0aeb52b` also gives out-of-range deadline one stable Bifrost code everywhere; retain this catalog/validation behavior when importing the new planner.

## 2. Terminal errors are an atomic protocol replacement, not a client-only patch

Source `crates/wyrd-spec/src/vala/api.rs` replaces `QueryTerminalError{code, detail}` with `Option<Box<WyrdProblem>>`. Source `crates/wyrd/wyrd-tonic/proto/wyrd.v1.proto` deletes the closed terminal-error enum and message, reserves old field number 6 and old field name `error`, and introduces `bytes error_problem_json = 9`. It also extends `ListActiveStreamsRequest` with projection and filters and `ActiveTailStream` with byte estimates, while removing persisted-file/predicate assignment messages. Source descriptor `wyrd.v1.bin`, private conversion and query conversion must match the combined proto. Retain main query params and peer identity/security updates in this rewrite.

Import source complete error serialization/reconstruction: `wyrd-spec` Bifrost catalog variants/details, `wyrd-client/src/error.rs::from_problem`, `code_to_wyrd_error`, `bifrost/query.rs` failed-terminal mapping, server query/scheduled consumers, MCP collector, gRPC query conversion and stream validation. Source replaces ~185 lines of per-code Bifrost parsing in client error.rs with deserialize `BifrostError` from problem details and require matching code. Main has newer generic client/auth/workflow errors in the same file; do not replace it wholesale. Preserve main fieldless `CardScopeDenied` (`c8febbd49`) and main stable error mappings. Source's gRPC test server uses real `wyrd_error_to_status` rather than manually made ErrorInfo: useful because reconstruction now requires correctly serialized details. Main already removed audit-unavailable errors (`f451d52be`); source Bifrost serialization must not reintroduce those variants or codes.

Mixed client/server versions cannot be assumed compatible: new terminal uses a new protobuf field and new JSON shape. Validation should deploy matching integrated server and SDKs; do not add a compatibility shim to make old terminals work. Source tests `failed_terminal_problem_rebuilds_its_catalog_error` and MCP/CLI Variant query scenarios provide relevant proof after porting. Proposed additional scenarios: failure after partial rows must still return failure; Variant parse failures retain row/path/details across HTTP, gRPC, Python, TS, MCP and CLI, and cancellation/deadline retain main behavior.

## 3. Shared types/queue split must reach every caller

Source adds `crates/shared/wyrd-types/{Cargo.toml,src/lib.rs,src/schema.rs,src/timestamp.rs,src/variant.rs}` as single owner of model JSON Schema <-> FieldSpec <-> Arrow and timestamp forms; `wyrd-queue/src/schema.rs` is deleted. Source exports shared types and mapping helpers through `wyrd-client::bifrost`, adds dependencies in client/SDK/CLI/server owners, and adds queue Variant JSON encoder factory. Keep these source changes but preserve main deliberately narrowed `internal` feature and API gates. Source `bifrost/mod.rs` again publicly exposes `ClientScope`, `BifrostIngestSink`, `IngestTransport`, client-option and register-outcome plumbing. Main gates these behind `internal` and main Rust SDK must never enable that feature in production (`scripts/checks/deps.sh`). Add timestamp type exports to main narrow surface; do not restore raw transport/plumbing exports.

Main `ClientScope::from_client` uses its auth owner directly and fingerprinting handles `ResolvedCredential::Renewable(source)` through stable source identity. Keep that so credential refresh does not accidentally produce separate pools or regress auth. Main `grpc.rs` retains bounded credential-mint/held-batch behavior and new renewal tests; source edits here are test catalog producer updates, not a reason to overwrite new main auth lifetime behavior.

Source queue `RowPreflight` replaces builders with strict per-row preparation: undeclared/reserved keys, unsupported Arrow/JSON types, Variant limits/type validity fail before admission. Remove event-time stamping and migrate clients to shared type schema API, but retain main queue pooling/lifetime and Card UID scope. Source writable_schema takes one argument and excludes managed columns; main takes event-time option. All surviving callers (capture, observation writers, testing/bench, SDK wrappers, docs) need source API adaptation. An auto-merged untouched main helper can silently retain event-time assumptions.

## 4. Card UID and schema/timestamp combinations

Main consistently changed Bifrost correlation from `card_ref` to `card_uid`. Source source `Correlation.card_ref`, TypeScript `correlation.cardRef`, Python `correlation['card_ref']`, describe inputs and reserve rules still use old vocabulary. Merge source Variant/timestamp behavior with main UID correlation, parsing and server scope checks. CardRef stays valid elsewhere (Cards principals auth declarations): do not apply global text replacement.

Source TableDescription removes `managed_candidates`; only declared user columns and correlation fields are writable. Python TableDescription TypedDict and TS TableDescription interface on main still require managed_candidates and old comments describe event time. Remove the field across public types/generated artifacts/projection tests while retaining `card_uid`/`run_id` correlation. Source layout adds `time_column`: default `wyrd_ingested_at`; user supplied time column must be non-null NTZ or LTZ. Python `_layout_json` and TableConfig constructor/from_arrow, TS layoutJson/TableConfig plus native wrappers, Rust layouts, server validation, describe contracts and doc examples must all accept/report it.

Source shared TimestampNtz/Ltz/Tz types and Python `python/wyrd/types.py` / `_pydantic.py`, TS timestamp branded types/schema helpers and native conversion preserve explicit timestamp semantics. Source UTC canonical form is `+00:00`, not `UTC`. Source model schemas translate nested typed structs/lists to physical nested columns, free-form/union/Any to Variant and maps to wire Map. Capture/Eval context/media become Variant values rather than canonical JSON strings. Retain main exact Card UID association and observation/evaluation control APIs while porting these projections. New source scenarios for early/late own time-column reads, three timestamp types and nested/Variant round trips should cover source behavior against integrated scope checks.

## 5. Rendering and bindings

Source Rust QueryResult.deserialize uses WriterBuilder with `wyrd_queue::variant::WyrdJsonEncoderFactory`; CLI query JSONL uses the same factory; MCP batch collector EncoderOptions also uses it. All three are required for logical JSON rather than exposing Variant binary storage. Source Python _validated_rows recursively converts Variant cells in Struct/List and reads timestamp metadata/texts; TypeScript nativeValue similarly decodes nested Variant values and schema-specific timestamps, and native module adds Variant decoder and timestamp helpers. Import these foreign runtime boundary pieces into current wrappers, retaining main registry/observe.verify/Run/workflow/auth semantics. Source N-API `serde-json` feature is required for serde JSON returning helpers, and source TS fromArrow constructor returns NativeTableConfigResult to preserve structured catalog failures. Regenerate native declarations rather than manually reconcile them.

No source UI implementation paths changed relative to base; preserve main UI auth rewrite. Still inspect generated frontend error/type consumers because terminal/catalog changes are protocol-wide, and source docs changed Bifrost internals/schema/writing/reading while main product doctrine changed independently.

## 6. Workspace native graph is indivisible

Source Cargo changes: Arrow/Parquet 60.0; object_store 0.14.2 (fs); DataFusion/datafusion-proto/datafusion-datasource =55.1.0; parquet-variant/compute/json 60; libc 0.2.186; Iceberg/cat-sql/datafusion rev `0991fef0af3f86336b52d7c56b7f35cf4b2da222`; compaction core rev `696560602183f9eb12e42d7296a1a932e3dc8bed`; distributed planner fork `8fac2a978bdcf8e06457238ebfe9c7f35023c123`. Source replaces Iceberg storage-opendal with object_store for Bifrost but retains opendal for general card/artifact storage. Source patches DataFusion family at `fd02579cc7b4253d477613757b8d2e651f0057e5` and Arrow/Parquet family at `e0f604e20f76a1f55da0970c9269ecb9b2dc1337`. This is functional (Variant, pruning/read planning, unshred fix), not optional dependency churn.

Preserve main Cargo workspace members/non-Bifrost versions/lint/profile policy; graft source analytical graph then resolve Cargo.lock from combined manifests. Do not take either lock wholesale. Regenerate Hakari with `mise exec -- cargo hakari generate` and `mise exec -- cargo hakari manage-deps`, then main `mise run check:workspace-hack`. Main prior compaction core `4e9f325d993d900bcdec2fa74239c6a6173f49cd` explicitly avoids REST catalog/Ring (`161488d9c`). Source new compaction graph must retain no Ring/provider and no unwanted REST feature; verify with main deps.sh, not trust source's lock. New wyrd-types manifest has workspace-hack dev dependency; production graph must stay PyO3-free and client-tier free of SQL/DataFusion/cloud despite parquet Variant type dependencies. New crate auto-joins main shared family via metadata; no hand-maintained list needed.

## 7. Deleted tooling and rules must stay deleted/main-owned

Only source changed scripts vs base are `scripts/checks/object-store-pin.sh` (0.13->0.14) and `scripts/test-families.sh` (adds wyrd-types). Main deleted both in `515f16c97` and replaced ownership with consolidated `scripts/checks/deps.sh` and metadata-derived `scripts/run-family-tests.sh`. Resolve delete/modify in favor of main deletions; source semantic graph requirements are covered by consolidated deps.sh's single-version checks. Do not resurrect old version-pin checker or manual family lists. Main deletes check_clippy_allow.py/check_unwrap_audit.py/check_test_contracts.py/add_justification.py and many name-ban/boundary scripts; preserve all these simplifications and current check lanes. Source old #[allow(clippy::...)] without reason will fail main allow_attributes_without_reason + pedantic; correct owning code or narrow justified expect as main instructions prescribe, not loosen lint table. Keep main workspace unsafe_code deny, pedantic, exception list, dev line-tables-only, release/dist profiles and existing CI selector behavior.

Source `.agents/skills/wyrd-task-review/SKILL.md` edits add reuse reviewer, unconditional repeat-round root-cause agent, expanded spec-revision approval/remediation workflow. These are unrelated to runtime Bifrost and conflict with newer main skill/standards. Keep main entire skill under the user's main-everything-else rule; do not accidentally import a changed review process as part of an engine port. Main commits include 753916bd0/515f16c97 and rule updates narrowing verification and rejecting unearned complexity. If desired later, review skill changes independently; they are not required for interoperability.

## 8. Test migration is a semantic port, not delete/modify resurrection

Main combines test targets into one per crate and rewrote SDK journeys into fixture stories (`753916bd0`, `c9a2ced55`, `d58579412`, `d9f0b2c3`, `5b71c8d82`, `4aa3f0923`, `ef6b5d6c5`). Main client `tests/integration/main.rs` owns pg_bifrost_e2e module behind test-support. Source adds separate-path modules at `tests/bifrost/{every_iceberg_column_type,register_a_table_from_a_model,table_time_column,three_timestamp_types}.rs`, `tests/pg_bifrost_e2e/variant_tables.rs`, included from old deleted `tests/pg_bifrost_e2e.rs`. Port those modules into the existing main integration target and fix path/module helper references; do not revive standalone deleted target. Move Rust-SDK drift/observe edits into main `tests/integration` stories; preserve main new negative/scope journeys rather than restoring deleted old filenames.

Python source integration tests `test_variant_tables`, `test_every_iceberg_column_type`, `test_register_a_table_from_a_model`, `test_table_time_column`, `test_three_timestamp_types` are useful new coverage. Merge on main fixtures/public imports and bind params. Source updates deleted Python test_bifrost_query/observe_journey/drift_journey variants: move relevant new column/value assertions into current main query/observe/drift stories. Preserve main typing fixtures under tests/typecheck; do not restore Python tests/bifrost/test_public_typing.py as a runtime pytest test (main `68281096d`).

TypeScript source creates `tests/support/test-server.ts` with useTestServer that exports URL/GRPC/key only. Main already has stronger `tests/support/server.ts` fixture which isolates WYRD_CONFIG_HOME and clears WYRD_ACCESS_TOKEN, WYRD_WORKLOAD_TOKEN and WYRD_TENANT so developer credentials cannot win resolution. Port source tests to main fixture. Keep column comparison helper only if needed in current owner; do not add competing server harness. Source old oracle-query/drift/observe test modifications must become main story additions.

## 9. Shared binary fixtures

Import `fixtures/bifrost/every_iceberg_type.arrow` (6738 bytes), `narrow_types.arrow` (1746) and generator `every_iceberg_type.py` (105 lines). All three SDK source every-Iceberg-type journeys read those same artifacts. Generator includes nullable parents/children, empty collections, max signed integers, decimals, maps, timestamp us/ns and UTC canonical `+00:00`, plus narrow Arrow types mapping to wider Iceberg forms; none contains card_ref/card_uid or managed event time. Therefore Card UID conversion does not require altering fixture binaries, while main contract fixtures elsewhere do require timestamp/schema updates. Run generator in Python SDK toolchain only if inputs change; add no standalone verification script. Existing source round-trip comparisons are the self-check. Fixture provenance/regeneration command in its docstring: `cd sdks/wyrd-sdk-python && uv run python ../../fixtures/bifrost/every_iceberg_type.py` (invoke owning toolchain with mise exec in actual report). No proof here that installed PyArrow supports source physical type formats; integrated tests settle that.

## 10. Lane and generated-artifact gaps

Keep main mise.toml and CI selection semantics; source changes just new source paths to obsolete explicit lists and benchmark rename. Main Python focused Bifrost lane names one file, so extend it to new Bifrost directory/files; main TS focused Bifrost list must include all five new integration files. Source explicit TS list already forgets `table-time-column.test.ts`; fix in combined lane. Both source focused Bifrost unit lanes omit new timestamp unit files; final all-unit aggregate reaches them but focused capability gate must too if retained as complete gate. Main `.github/scripts/select-ci.py` BIFROST_ONLY recognizes only main story filenames, not new timestamp/Variant fixture and SDK types paths. Update classification coherently (or intentionally allow full gate fallback); never silent omission. New crate is handled by metadata family selector. Preserve main benchmark `bench:capacity` (source wants old verification_capacity and bifrost_client_queue entrypoints); engine owner handles any unavoidable data representation adaptation in current benchmark rather than restoring deleted benchmark commands.

Main codegen regenerates wyrd-spec schemas, typed Card envelopes, Python stubs, TS error codes and theme tokens. Source adds/changes seven Bifrost schemas: audit_event, data_type_spec, field_spec, physical_layout, register_table_request, table_description, time_granularity. Regenerate from combined source so card_uid, new errors, map/timestamp/Variant, terminal problem and no managed_candidates agree. Native TypeScript index.d.ts/index.d.cts need `ts:napi:check` regeneration/check. Python native submodule stubs and public assembled stubs must match timestamp accessors and layout; do not replace broad stubs that now include main principals, Run, observe.verify/workflow and CLI additions. Protobuf descriptor/build generation is owned by wyrd-tonic build/source and must be updated under its current pattern; no restored proto-drift checker. Served OpenAPI proof belongs to main `test:principals:integration`, not generated schema check.

## 11. Verification evidence and commands

These are existing commands inspected in pinned main mise.toml, NOT run in this research: `mise run test:bifrost:journey:sdk`, `mise run test:bifrost:journey:observe`, `mise run test:bifrost:journey:drift`, `mise run test:bifrost:journey:forge`, `mise run test:bifrost:journey:scribe`, `mise run test:bifrost:journey:oracle`, `mise run test:bifrost:journey:otlp`, `mise run test:bifrost:journey:server`, `mise run test:bifrost:journey:mcp`, `mise run test:bifrost:journey:python`, `mise run test:bifrost:journey:typescript`; `mise run codegen:check`, `mise run check:deps`, `mise run check:tenant-isolation`, `mise run check:workspace-hack`, `mise run ts:napi:check`, `mise run test:principals:integration`.

Focused existing main bind-param proof: `mise exec -- cargo nextest run --locked -p wyrd-client --lib -E 'test(=bifrost::query::tests::sql_forwards_bind_values_without_interpolation)'`. Proposed source test module names will change during port; list exact nextest names after incorporation and run focused exact expressions then, not guessed source standalone targets. Existing source every-Iceberg type test exact fn is `every_iceberg_type_round_trips_with_nested_nulls`, but its future module prefix is determined by rehoming.

Final integration necessarily touches global Cargo pins, generated contracts, shared CI/test wiring and several ownership boundaries. Main `mise run gate` is justified here, and already includes `check`, `check:deps`, `check:tenant-isolation`, storage drift, CI selection, docs/examples checks, codegen, docs check, Rust broad matrix, Bifrost/gateway ignored suites and three SDK verify tasks. Do not rerun its component lanes as final checks. Gate intentionally excludes production Scribe/Forge geometry, live storage cloud, opt-in capacity benchmarks and live provider/Operator smoke. Existing optional focused geometry lanes are `test:bifrost:journey:forge:production-geometry` and `test:bifrost:journey:scribe:production-geometry`; if the merge claims those production invariants, run separately and state aggregate gap. No static comparison proves final green lanes, mixed versions, installed fork features, or retained historical data compatibility.

## 12. Post-ledger server/local-development seams confirmed

Main-only CLI owners must remain intact: `crates/wyrd/wyrd-cli/src/server/dev.rs::DevArgs::run`, `DevArgs::validate_local_config`, `DevServer::{new,command,run,start,detect_memory,migrate,provision,ready,stop}`, `local_binds`, `save_local_tenant`, `verify_saved_client`; `GlobalConfig::{local_api_key,save_local}` in client. DevServer resolves selected HTTP and gRPC loopback addresses once, passes WYRD_GRPC_BIND to child and saves selected grpc endpoint with safe credentials before fresh SDK/MCP use. Source boot/state edits cannot revert to old hard-coded gRPC/local credentials because fresh callers then fail despite direct fixture users succeeding. Keep main server install, CLI MCP setup/proxy/host ownership and HTTP router.

Current main `AppState` constructor retains `workflows: Arc<WorkflowRuns>`, `mcp_tasks`, `gateway_tasks`, `scribe_outbox`, `gateway_secret_keys`, `gateway_engine`, `operator_keys`, `verification`, `peer_plane` and authenticated handles. Keep `AppState::new` retrieving the already-composed outbox from `Bifrost::scribe_outbox()` (unit-shell fallback only), `with_workflow_config`, `with_gateway_engine`, `with_gateway_secret_keys`, `with_operator_keys`, `with_auth`, `with_authz`, grpc health/readiness. Source old state has OracleBuildInputs.audit and role_shutdown whereas main roles use process_drain and authorization audit is staged on shared outbox; compose new storage behind main retained lifecycle shape, not old source state replacement. Main `boot::compose_bifrost` starts one ClusterRegistry snapshot poller and one ScribeRoute/ScribeSink outbox used by role security, Gate, Oracle, request paths, capture and Verifiers; retain this wiring. Boot also keeps `attach_human_logins`, `verify_operator_keys`, identity-sealing/issuer owners.

Main app `BoundServer` verification runtime creation/spawn and shutdown must remain: accepted workflows drain while gateway/query remain available; tracked gateway calls finish before ScribeOutbox flush; outbox receives half remaining budget while Bifrost accepts; roles/runtime owners close afterward. Source clean-looking app serve modifications are only new error/storage plumbing, not authority to lose these owners. Main HTTP build_router mounts workflow_runs_router, cards, principals, verification, operator_connections, admin, identity, gateway, authenticated MCP and separate Bifrost-audience query/ingest and OTLP auth groups; source full-file selection must not revert mount/auth order.

Concrete silent main-only compile/behavior seam: `crates/wyrd/wyrd-server/src/components/cards/service.rs::declared_tables` imports `wyrd_queue::json_schema_to_fieldspec`; source deletes queue schema and moves this mapper to wyrd-types. Preserve main Service table declaration preflight/ensure-before-Card-transaction with new mapper import. Main `register_card` extracts Service tables, validates all schema shapes before table/Card write, then calls shared register_table for each; do not drop it because source did not touch the file. New Variant-capable Service schemas must reach same source mapper as ordinary TableConfig so registration and later describe/write agree.

Existing exact main local-dev lane `mise run test:local-dev:journey` builds installed CLI/server, drives Rust/Python/TS and selected MCP host clients from saved config. It selects existing CLI tests `pg_tests::server_dev_first_run_is_client_ready`, `pg_tests::server_dev_repeat_and_partial_failure_preserve_state`, `pg_tests::server_dev_selected_mcp_host_reads` in target cli; Python test_local_development.py parametrized installed-cli bootstrap exercises rust/interrupt/exit/python/typescript. This lane is included by three SDK verification tasks and therefore full gate; no separate final rerun required. No implementation or these tests were run during research.

## 13. Authority clarification: intentional source changes vs inherited old APIs

After the user clarified that source Bifrost behavior wins except retained main fixes, rechecked alleged API conflicts against the merge-base. **Source did not intentionally remove bind parameters.** Base facade already has `sql(query)` at line608, `sql_as(query)` at645 and `stream(query)` at665; source keeps the same signatures. Its post-base diff changes Variant result encoding but no SQL/stream signature or request params. Source `changes/active/bifrost-variant/spec.md` Scope (lines133-162) specifies Variant/storage/read-path work, not parameter removal. REQ-018 (lines882-890) says “Typed row terminals decode it into the language's native open value: Python `sql(query, model)` into `dict`, `list`, and scalars; TypeScript `sql(query, rows)` into plain objects, arrays, and scalars, with 64-bit integers as `bigint`; Rust `sql_as` into `serde_json::Value`.” Those are inherited API examples specifying decoded VALUE behavior, not explicit intent to forbid ordered params. No QueryParam/params/bind-values/bind-parameters occurrence in source spec/tasks/Bifrost design found. Therefore combine main newer typed params with source canonical Variant result semantics; do not describe this as overruling an intentional source deletion.

**Source first-use built-in provisioning is also inherited baseline, not a source removal of eager provisioning.** Base architecture/bifrost-design.md lines112-120 and source lines128-137 carry the identical paragraph “A built-in's per-tenant row is materialized on first use from its owning definition rather than seeded at tenant provisioning, and describing one materializes it exactly as ingesting into it does.” Source post-base diff does not touch this paragraph or platform/builtins.rs / platform/tenant.rs (no changes). Main subsequently introduced eager provisioning in 1090580f2. Preserve main eager workflow under retained main updates, updating combined architecture paragraph to describe actual integration. This does NOT require keeping removed source-authoritative table definitions: retain eager lifecycle for the final source table set, and let source explicit system-table/schema removals win. If another agent finds a task revision expressly changing provisioning, use that authority rather than this inherited paragraph.

The preserved-main-public-API rule is thus a combination rule for additions absent from source's old baseline, not a blanket preference for main inside Bifrost. Actual source changes—new terminal problem framing, no managed_candidates, explicit timestamp semantics/time_column, strict schema validation, new read/storage path and removed tables—remain source-authoritative, with Card UID, identity, outbox, heartbeat/peer and simplifications adapted as requested.


---

## Archived snapshot: wyrd-merge-tables-evaluation.md

# System tables, observation attribution, and verification merge research

Pins: main `8a8813a7458b7c98707c7b4f9b9d403f604ee0e6`; source `cfe21dc9007d5d22d9ae4411be07fb5504b0cc05`; common base `5ab92b0039404565e51f4a246863ce90cbd8d11f`. Read-only source/history research; no merge, build, runtime probe, or tests executed. All path references below are relative to repository root; M means main and S means source. Main architecture/agent-rules.md was read together with design/doctrine/Bifrost design.

## Findings and recommended merge treatment

### 1. Preserve UID-only observation attribution through the new Bifrost engine (critical)

S reintroduces `CARD_REF`, `Correlation.card_ref`, native `card_ref` IPC input, and `wyrd.card_ref` OTLP attributes; it makes `card_uid` server-only and resolves references exclusively against signed CardRefScope. M intentionally changed the public correlation to `CardUid`, native `card_uid`, and `wyrd.card_uid` and additionally permits unbound principals (human/API-key/card-free service) to attribute registered observation-target cards in their tenant. This is a cross-layer contract conflict, not a rename in one table.

Evidence: M `dadaa3ab9` ( Correlate observations by Card UID confirmed against the principal scope or tenant registry), `b5ab2dd5d` (Attribute unbound writers' evidence to registered observation-target Cards); M `crates/vala/vala-bifrost-redux/src/gate/attribution.rs:75` `CardAttributionRegistry::attributed_cards`, `:102` observation_targets; M Gate mod.rs `:1051` native lookup, `:827/:866/:905` OTLP lookups; M execution_lanes.rs `:745` authorized_card_uids, `:832` stamp_correlation_columns, ingress.rs `:99` card_scope. S execution_lanes.rs `:1238` resolve_card_uids and tables/signal.rs RecordCorrelation::extract instead resolve CARD_REF solely from signed scope. S shared client bifrost/table.rs Correlation replaces main UID with CardRef. Shared client bifrost/observe.rs similarly replaces card_uid argument with card_ref.

Recommendation: take S bounded ingest/conform/Variant implementation and bring M UID authorization into it. Keep M Gate registry attribution lookup, peer-carried attributed UID scope, and CPU ingress card_scope field. Accept at most one Utf8 `card_uid` input; authorize the whole frame before admission; preserve the caller's authorized UID exactly; missing/null UID remains null. `card_uid` and `run_id` remain correlation-only (excluded from source fingerprint), while new `wyrd_ingested_at` is server-owned. Preserve bound principal signed-UID checks; unbound principals resolve supplied UID in tenant and require is_observation_target; SYSTEM result writer may assert only its exact Verifier attribution through the internal path. No registry query belongs in Rayon decode.

Reach: wyrd-spec managed column constants/reexports; client facade/handle/sink/observe/table; wyrd-queue encode/conform and JSON input; telemetry OTLP exporters; Gate attribution/OTLP/native projection; ingress FrameEnvelope and peer protocol; execution_lanes decode context/source fingerprint/managed stamp; eval acknowledged decoder; Scribe outbox capture builder; Python/TS binding inputs; fixtures and all observation docs/journeys. Do not eliminate protocol CardRef as a concept: only observation correlation wire is UID-only.

Negative proof required: foreign tenant UID, missing UID, wrong-kind registered UID, out-of-bound signed UID, duplicate correlation fields, malformed/non-Utf8 UID, mixed permitted/forbidden rows, unbound writer permitted registered subject, cross-tenant rejection, and internal result Verifier attribution all retain M behavior.

### 2. Adopt explicit table time axes, updating semantic readers rather than preserving retired wyrd_event_time (critical)

S removes managed WYRD_EVENT_TIME; adds PhysicalLayoutWire.time_column and TimeAxis, with timestamp precision aware bucket/bounds/pruning. M reads and writes retired event time at many seams. Preserve S complete time mechanism, but change M observations/runtime readers to table-declared columns. No compatibility alias or synthetic event-time column should be reintroduced.

Evidence: S tables/managed_columns.rs MANAGED_COLUMNS contains run_id, card_uid, principal_id, wyrd_request_id, wyrd_ingested_at (five physical columns instead of M six). S catalog/layout.rs TimeAxis::of_wire defaults to WYRD_INGESTED_AT and stores explicit axis; catalog/event_time.rs EventTimeStatistics::from_manifest_bounds converts nanosecond lower bounds with floor and upper bounds with ceil, retaining files on missing/invalid bounds. S tables/signal.rs `:807` OTLP_INSTANT is Timestamp(Nanosecond,+00:00), not Int64. S tables/fields.rs ts_us_utc changes timezone UTC -> Iceberg +00:00. Native retired wyrd_* columns remain reserved by is_reserved_column.

Merged time-axis matrix:

| Logical FQN | Granularity / axis | Significant type change | Decision |
|---|---|---|---|
| vala.traces.spans | hour / start_time_unix_nano | OTLP start/end/event instants Int64 -> Timestamp(ns,+00:00); duration stays Int64 | S schema/time axis, M UID attribution |
| vala.metrics.points | hour / time_unix_nano | point/start/exemplar instants -> Timestamp(ns,+00:00) | S schema/time axis, M UID attribution |
| vala.logs.records | hour / time_unix_nano | event/observed instants -> Timestamp(ns,+00:00) | S schema/time axis, M UID attribution |
| vala.drift.observations | day / created_at | created_at Timestamp(us,+00:00), fields otherwise stable | S time axis; M complete runtime |
| vala.eval.observations | day / created_at | context/media Utf8 JSON -> Variant; IDs stay fixed binary 16/8 | S schema and decode; M writer ownership/run linkage |
| vala.verification.results | day / ended_at | details Utf8 -> drift_report Struct + eval_summary Struct | S typed summaries plus M details as Task-only Variant; M lifecycle/run linkage |
| vala.drift.result_features | day / wyrd_ingested_at | window bounds Timestamp(us,+00:00) | S layout; joins by result_id must remain sufficient |
| vala.eval.result_items | day / wyrd_ingested_at | actual/expected Utf8 JSON -> Variant | S projection/schema; M writer/outbox |
| vala.gateway.calls | hour / started_at | request_payload_json/response_payload_json Utf8 -> request_payload/response_payload Variant; optional model children nullable | S schema/projection; M capture/outbox/auth |
| vala.system.audit_log | day / created_at | detail Utf8 JSON -> Variant; S brings SQL hash/sequence fields that M retired | Keep M event content, add explicit decision-created_at+Variant, do not resurrect SQL chain |
| vala.dev.agent_traces | hour / started_at | S adds old removed table and CodeAxis | Keep M deletion; no active producer earns revival |
| dynamic caller table | hour / wyrd_ingested_at default, or declared non-null Timestamp axis | time_column now a supported physical-layout property | S authoritative behavior plus M UID-only correlation |

Observation ACK invariant: freeze the exact admitted row's created_at as run input_event_time, even if receipt/replay happens next day. Eval input lookup uses that UTC day on created_at. Main Gate ACK receipt_micros callback parameter can disappear only after all decoder callsites use created_at. Reads still operate on exact subject UID and record_id; record row also retains application run_id and authenticated writer principal_id. Trace read predicates move to start_time_unix_nano using timestamp SQL literals; decode timestamp as nanoseconds, not integer seconds/microseconds. Keep main ordering start_time then span_id and ceiling +1 behavior.

Drift invariant: change main ObservationWindow SQL predicates from wyrd_event_time to created_at, retain typed frozen schedule window and aggregation method, no arbitrary SQL. Result detail tables partition by ingestion time whereas parent partitions by ended_at; current same-day parent/detail claim is invalid. Never prune detail joins solely to parent's ended_at day; ingestion crossing UTC midnight is legal, result_id is authoritative join. Preserve row window_start/window_end as measured-input window, not ingest time. Catalog S nanosecond bounds conversion and TimeAxis must be used consistently by WAL, hot files, sealed objects, Oracle, Forge, and replay. Equivalent UTC spellings and ns/us boundaries need schema conformance, not hardcoded Arrow downcasts to UTC.

Counterevidence: S changes are already propagated inside its own Drift/Eval SQL and signal codecs. They are useful implementation source; wholesale importing these engine files loses M unrelated durable behavior (items 3-7).

### 3. Preserve main Eval enqueue ownership, retries, and ACK replay fencing (critical)

S observations.rs replaces M generic ObservationRunOutbox/ObservationRunSink with Semaphore(256)+TaskTracker background tasks, no retry queue. It also drops bound writer ownership and activates every subject binding. S verifier_runs.rs reverts main bulk enqueue to per-row per-binding calls, requires stale owner activity, and omits the writer parameter. This silently runs another service owner's verifier on the same subject, or discards runs on transient SQL failures while observations remain acknowledged.

Evidence: M observations.rs is ~223 lines, ObservationRunSink::write -> enqueue_observation_batch, bound_card extracts principal root signed UID, staged ObservationRecord contains subject/writer/record_id/event_time. S observations.rs new enqueue transforms keys into (card_uid,record_id,created_at) only, queue.enqueue_observations, guard records loss. M verifier_runs.rs LOCK_OBSERVATION_SUBJECTS_SQL uses paired subject/writer arrays with `(u.writer IS NULL OR u.writer=b.owner_card_uid)`, INSERT_OBSERVATION_RUNS_SQL does one deduplicated insert with ordinal per binding. S removes both writer and bulk insert, reintroduces binding_activity. M commits `990f922a9`, `ea5752354`, `bcf4b9248` record intentional writer/unbound changes.

Recommendation: keep M observations.rs and SQL run queue behavior, adapt AcknowledgedObservation to created_at and UID-only input; do not use S resolve_card_uids for ACK (it reads CardRef and cannot authorize an unbound writer). ACK source frame carries the already-authorized UID, so decoder reads UID directly. M outbox failure/terminal/shutdown semantics remain. Preserve durable unique tenant/binding/record replay fence, ordinal numbering without gaps from duplicates, and binding lock order. Add/retain next-day replay test where created_at differs from receipt.

### 4. Preserve main verification runner, cache/claim hydration, lease renewal and delayed schedule clock (critical silent loss)

S deletes main verification/{authority,cache,fault,leases}.rs, adds publisher.rs, removes VerifierRunQueue::renew, claim-sourced system_principal/verifier_present/fitted_baseline, changes runnable tenant listing back to capped limit, and removes 30-second SCHEDULE_CLAIM_DELAY. These are non-Bifrost runtime changes and should remain main-owned.

Evidence: verifier_runs.rs diff removes RENEW_LEASES_SQL and renew; DueBindingRow schedule_clock becomes now; DUE_BINDING_SQL and tenant discovery stop applying delay. M commits `0f48fa209` (claim schedule occurrence 30 seconds after window ends), `a6c20a221` (judge missed occurrence on delayed schedule clock), `93f709d08` (runtime correct under load). Main Claims/Lease/Cache ownership retains resource/race fixes that compile might not detect if old S callers accompany old APIs.

Recommendation: keep main queue/schema/runtime/authority/cache/lease/fault modules and tests. Apply only typed schema readers/time-predicate changes to Drift/Eval/result authoring and integration constructors. Main owns claimed baseline hydration and system principal resolution; do not reintroduce per-run Registry/SQL lookups from S. Keep main direct verification authorization, Task kind and selection, typed public error handling. Parent auth research can cover adjacent credential differences.

### 5. Preserve gateway-mediated judge authority and application Run lineage (critical)

S EvalEngine retains providers: Arc<ProviderRegistry>, invokes providers directly, and reads only seven content columns. M deliberately reads `run_id` and `principal_id` from observation, resolves writer_caller and passes ServerWyrdGatewayCaller to its judge, including gateway permission/budget/provider governance. Importing S entire eval.rs undoes both security and attribution while tests for simple assertions may still pass.

Evidence M `ecdd12850` Route Verifier LLM judges through gateway as judging principal; `ad27e71f3` Record application Run on continuous and direct Verifier results. M eval.rs `execute` reads `(record, observed_run, writer)`, calls writer_caller only when needs_judge; main score completion with_run(observed_run); VerifierReport::Eval carries run_id. Source drops all three. M engines.rs `:39/:148` documents application run and runner.rs `:761` passes report.run_id to ResultRun::claimed.

Recommendation: retain main EvalEngine owning state and gateway judge constructor/authority resolution, direct judge parity, SYSTEM tokenless readers, and public verifier:run authorization. Port S Variant context/media/attributes decoding and Timestamp(ns) span decoding into main BifrostReader; retain run_id/principal_id query selections and return tuple. Keep main robust Arrow string/binary representations where compatible rather than reintroducing whole-column casts per cell (S helper column casts entire column each call). Source DriftEngine reintroduces TenantTokenIssuer SYSTEM token mint and extra baseline IO; keep main tokenless SystemReadAuthority and hydrated baseline, only change created_at SQL/time schema.

Important nuance: M result managed run_id means application Run judged (nullable for scheduled Drift), not verifier execution run. S tables/results documentation still says Verifier run; update it to main meaning. ResultRun/result_id/SQL verifier_runs linkage remains separate evidence. Do not accidentally stamp verifier UUID into application Run column.

### 6. Typed verification summaries require explicit Task evidence adaptation (direct and silent schema conflict)

S results table moves details text to typed drift_report and eval_summary Struct. It supports Drift/Eval only. M added Task Verifier and uses details to retain its exact AssertionResult, no per-task detail table. Keep S typed representation and preserve M Task evidence instead of silently dropping Task payload or converting all summaries back to text.

Evidence M results.rs `:235` match includes VerifierReport::Task, `:1065` direct_task_result authoring test; S tables/verification/results.rs `:26-70` only Drift/Eval children and WHOLE_STRUCTS validator. M `284e4bc92` Task kind. Summary names change affects literal SQL and any details JSON parsing; S eval/result_items actual/expected changes affect clients expecting JSON strings.

Recommendation: retain main existing details column as nullable Variant for Task results only, alongside source typed drift_report/eval_summary; no new task_result Struct or new protocol type is needed. Main results.rs build at :252 serializes canonical_json(AssertionResult), summary at :303 emits details. AssertionResult owns task_id, passed, actual Option<Value> (redact/hash/full), expected Value, operator ComparisonOperator, message Option<String>, stage u32, started_at DateTime<Utc>, duration_ms u64 (wyrd-spec/src/vala/eval/result.rs:36). Feed this same serde projection to source Variant builder. Populate details only for Task, drift_report only for Drift, eval_summary only for Eval, and avoid redundant duplicated summaries. Add details to sensitivity metadata; WHOLE_STRUCTS governs source typed summaries, not the Variant details column. Empty sampled-out Eval is an eval_summary with zero task counts; invalid/unscorable Drift has neither summary populated; preserve source nullable children to avoid Parquet padded leaves leaking through get_field. Runtime emits only its implementation's summary. Use source schema/projection builders as baseline, but keep main ScribeOutbox result publication and direct/continuous application-run attribution. No source ResultPublisher SQL staging resurrection.

### 7. Audit schema source is tied to retired staging publisher (critical)

S audit_log 15 fields contain seq,entry_hash,prev_hash; audit projection reads vala_sql::row_types::audit_staging::AuditStagingRow and computes batch ID from SQL frozen sequence ranges. M audit table has 11 decision content columns and projection takes AuditEvent + decided_at, while generic process outbox builds content-derived batch ID. Source audit projection is not reusable wholesale.

Recommendation: keep main event projection signature/fields, in-memory shared outbox, batch identity, loss semantics and no hash chain/staging/publisher dependencies; change detail encoding to Variant and add created_at timestamp from staged decision time, then partition/sort on created_at. Preserve AuditLogTable::admits_system_owner for platform refusal sentinel only; public writes to audit remain refused. Content fields naming audited subject vs managed writer remains intentional; audit_card_ref is audit event evidence, not old generic observation correlation, and should not be mechanically renamed to card_uid without examining main audit contract. Parent audit scout owns exact API semantics.

### 8. Retain eager provision and removed unused dev table (direct conflict)

S tables/mod.rs BUILTIN_TABLES has 11 including vala.dev.agent_traces and source CorrelationPolicy::CodeAxis. M `1090580f2` explicitly provisions all built-ins eagerly, drops lazy creation and unused agent_traces table and narrows SDK controls. Preserve main canonical table count (10) and eager startup/tenant provisioning; use S schema internals for each retained table. Source catalog/bifrost_catalog.rs pin_cut around 1405 still calls ensure_builtin during read for built-ins. Reconcile with main eager provisioning; remove that lazy creation rather than reviving old workflow as part of import. Source dev table and CodeAxis are residue; do not add them solely because they appear in source registry. Check any other source live uses before eliminating enum branch; no active use besides removed dev table identified in this scout.

### 9. Existing retained data is not automatically schema compatible (blocking deployment decision)

S changes built-in user fingerprints (Binary->Variant; Utf8->Variant; Int64->TimestampNs; details split; gateway rename) and every physical layout time axis. Even stable user fields drop old managed event time and canonical physical shape. S BifrostCatalog::register_inner `:1012/:1017` rejects changed fingerprint/layout of an existing registration. Replacing definitions in code does not transform stored catalog rows, Iceberg schema IDs/types, old Parquet, hot file bounds, WAL, memtable spill objects, or frozen verifier_runs.input_event_time.

Decide before executing whether this integration targets fresh non-production state or must preserve existing data. For fresh state: use separate test DB/object root explicitly, with no implicit deletion. For retained state: this research finds no automatic migration covering the changes, so deployment is blocked until a bounded data conversion/cutover is designed. Do not rewrite registered metadata in-place claiming old files satisfy new schema. Existing queued Eval runs froze old event time, which could differ from observation created_at; after changing read pruning they can miss record. Quiesce and settle/drain them under old reader or explicitly migrate input_event_time using exact subject+record evidence before new reader activation. Side-by-side shadow conversion can be recommended only with defined preservation/backout requirements; cannot infer user consent to destroy data.

### 10. Generated contracts, SDKs, docs and fixtures must reflect combined schema, not either branch wholesale

Source types module move wyrd-queue/schema -> wyrd-types/schema introduces canonical Variant types and JSON-schema Value/open map handling. Reuse it. Keep main UID-only SDK correlation, renewable credential scoping, bind-parameter SQL API and SDK ownership restrictions while moving schema mapper imports. Regenerate .pyi/JSON schemas from merged source; don't use source generated artifacts unchanged (they include reverted auth/verifier surfaces and duplicate generated test schemas). Keep main observation-surface Python and TS journeys deleted by source, and scheduled-drift alerts journey/fixtures removed by source; adapt expected context/media/actual/expected to objects, times to timestamp projections, summaries to typed columns, and payload column names. Source new Drift Rust/Python/TS journeys supplement, not replace these main workflows.

Sensitivity annotations remain descriptive under main doctrine: ordinary table query permission governs eval/trace/result columns; only gateway request_payload/response_payload projections require gateway_payload_read. Source schema docs claiming every sensitive column needs elevated payload authority must be corrected, not promoted into new auth checks. Retain Oracle gateway protection while changing its column names.

Docs include source table matrix/types but must preserve main Task verifier, UID correlation, application Run semantics, gateway judge permissions, audit outbox guarantee and absence of CodeAxis dev table. Main bifrost/schema.svx lines 19/29/43/46 assert managed event time and same-day details; replace these with table-axis matrix. All doc/example SQL WHERE wyrd_event_time or SELECT details, request_payload_json need reviewed replacement. Architecture telemetry-observations/evaluation references need combined update.

## Verification strategy (commands to execute later; not executed in research)

Main mise maps server journey target `wyrd-testing --test server` with module eval_verification; source relocates some other integration tests. Keep main layout and confirm names with nextest list on integration tree before execution. Use repository wrapper and migration tasks. Focused example retaining exact named test expression:

```
scripts/postgres/with-test-postgres.sh -- bash -lc 'mise run db:migrate:all:inner && mise exec -- cargo nextest run --locked -p wyrd-testing --test server --run-ignored only -E "test(=eval_verification::eval_runs_follow_the_writing_owner)"'
```

Mandatory existing main cases from crates/wyrd/wyrd-testing/tests/bifrost/server/eval_verification.rs: `continuous_eval_runs_the_terminal_matrix`, `sealed_replay_on_a_later_day_activates_once`, `integrated_enqueue_outage_preserves_ack_and_recovers`, `continuous_eval_reads_ordered_bounded_trace_evidence`, `continuous_eval_refuses_a_trace_over_the_span_ceiling`, `continuous_eval_read_authority_fails_closed`, `continuous_eval_failures_publish_only_stable_errors`, `eval_runs_follow_the_writing_owner`, `direct_task_judgment_records_one_correlated_result`. They must each get exact focused command when named in eventual task/report, with list-confirmed module name.

Mandatory SQL tests main pg_verifier_runs.rs: observation_runs_follow_the_writer, frames_naming_subjects_in_opposite_orders_serialize, observation numbering/replay tests, lease renew/reclaim, database_clock_owns_verifier_queue_deadlines and delayed schedule missed-window behavior. Current main external home is tests/integration/pg_verifier_runs.rs, keep main combined integration target rather than blindly adopting source flattened tests.

Final capability lanes: `mise run test:bifrost` proves integrated Bifrost Rust + language journeys per main task; main `test:bifrost:journey:observe`, `:drift`, `:otlp`, `:server`, `:python`, `:typescript` are useful iteration lanes; `mise run codegen:check`, `check:deps`, `check:tenant-isolation`, docs:check when docs changed, and narrow verification/principals lanes for related non-Bifrost main behavior. Repository-wide integration eventually merits main broad gate because integration crosses many ownership boundaries. Parent testing scout owns final aggregate prescription.

Add small necessary coverage for combined seams: exact created_at vs different ACK receipt across days, result_id detail joins across ingestion midnight, Timestamp ns last instant below/above microsecond pruning boundary, UTC/+00:00 conformance and fixed point, typed null Variant vs SQL null, nested whole Struct nulls after Parquet and hot/compact parity, Task summary preservation, unbound API-key subject under source ingest and capture writer on Scribe-free pod, gateway judge denied principal preserving observed application run.

Risks/uncertainty: no runtime verification; no catalog data inventory; additional code may rely on event-time literal SQL outside paths researched. Preserve exact evidence and use integration tree compilation/journeys to resolve symbols, not optimistic file-level ours/theirs.

## Appendix: pinned source field declarations for adaptation

These are source user fields before the five managed columns; append the merged M UID correlation contract. `false` means required, `true` means nullable. Widths: utf8 string, float64 64-bit IEEE, int32 signed 32, int64 signed 64, ts_us_utc Timestamp(us,+00:00), fixed_binary width in bytes, Variant canonical parquet extension. Source dev table listed only to explain why it must stay removed. Audit source extra sequence/hash fields must not survive.

### crates/vala/vala-bifrost-redux/src/tables/audit/audit_log.rs

```rust
-> Vec<Field> {
        vec![
            Field::new("seq", DataType::Int64, false),
            utf8("entry_hash", false),
            utf8("prev_hash", false),
            utf8("request_id", false),
            utf8("trace_id", true),
            utf8("operation", false),
            utf8("resource", false),
            utf8("audit_card_ref", true),
            utf8("audit_principal_id", false),
            utf8("principal_kind", false),
            utf8("permission", false),
            utf8("outcome", false),
            variant_field(DETAIL, true),
            utf8(CREDENTIAL_ID, true),
            ts_us_utc(CREATED_AT, false),
        ]
    }
```

### crates/vala/vala-bifrost-redux/src/tables/drift/observations.rs

```rust
-> Vec<Field> {
        vec![
            utf8("record_id", false),
            utf8("series", false),
            float64("num_value", true),
            utf8("str_value", true),
            utf8("session_id", true),
            ts_us_utc("created_at", false),
        ]
    }

    /// Daily partitions sorted newest first then by series, Bloom-pruned on `series` for one-feature scans.
```

### crates/vala/vala-bifrost-redux/src/tables/drift/result_features.rs

```rust
-> Vec<Field> {
        vec![
            utf8("result_id", false),
            utf8("owner_card_uid", true),
            utf8("subject_card_uid", false),
            utf8("binding_id", true),
            ts_us_utc("window_start", false),
            ts_us_utc("window_end", false),
            utf8("method", false),
            utf8("feature", false),
            float64("score", true),
            float64("threshold", true),
            utf8("verdict", false),
        ]
    }

    /// Daily partitions sorted newest first, Bloom-pruned on `result_id` for the parent-result join.
```

### crates/vala/vala-bifrost-redux/src/tables/eval/observations.rs

```rust
-> Vec<Field> {
        vec![
            utf8("record_id", false),
            utf8("session_id", true),
            variant_field("context", false),
            fixed_binary("trace_id", 16, true),
            fixed_binary("span_id", 8, true),
            ts_us_utc("created_at", false),
            variant_field("media", true),
        ]
    }

    /// Daily partitions sorted newest first; rows are read by record, not by a pruned key.
```

### crates/vala/vala-bifrost-redux/src/tables/eval/result_items.rs

```rust
-> Vec<Field> {
        vec![
            utf8("result_id", false),
            utf8("owner_card_uid", true),
            utf8("subject_card_uid", false),
            utf8("binding_id", true),
            utf8("source_record_id", false),
            utf8("task_id", false),
            utf8("outcome_kind", false),
            boolean("passed", true),
            variant_field("actual", true),
            variant_field("expected", true),
            utf8("operator", true),
            utf8("message", true),
            int32("stage", true),
            ts_us_utc("started_at", true),
            int64("duration_ms", true),
            utf8("skip_reason", true),
            utf8("upstream_task_id", true),
        ]
    }

    /// Daily partitions sorted newest first, Bloom-pruned on `result_id` for the parent-result join.
```

### crates/vala/vala-bifrost-redux/src/tables/verification/results.rs

```rust
-> Vec<Field> {
        vec![
            utf8("result_id", false),
            utf8("implementation", false),
            utf8("execution_status", false),
            utf8("verdict", false),
            utf8("verifier_version", false),
            utf8("owner_card_uid", true),
            utf8("subject_card_uid", false),
            utf8("binding_id", true),
            utf8("trigger_identity", true),
            utf8("source_record_id", true),
            ts_us_utc("window_start", true),
            ts_us_utc("window_end", true),
            ts_us_utc("started_at", false),
            ts_us_utc("ended_at", false),
            Field::new(
                DRIFT_REPORT,
                DataType::Struct(Self::drift_report_fields()),
                true,
            ),
            Field::new(
                EVAL_SUMMARY,
                DataType::Struct(Self::eval_summary_fields()),
                true,
            ),
        ]
    }

    /// Daily partitions sorted newest first, Bloom-pruned on result id, subject, and binding lookups.
```

### crates/vala/vala-bifrost-redux/src/tables/gateway/calls.rs

```rust
-> Vec<Field> {
        vec![
            int32("schema_version", false),
            utf8("call_id", false),
            utf8("caller_principal_id", false),
            utf8("operation", false),
            utf8("ingress_dialect", false),
            // A `ModelRef`: its provider and provider-native model.
            Field::new(
                "requested_model",
                DataType::Struct(Fields::from(vec![
                    utf8("provider", false),
                    utf8("model", false),
                ])),
                false,
            ),
            // Optional, so both children are nullable: a required Parquet leaf
            // under a null Struct reads back padded values that a field query
            // would expose. The table validator still refuses a partial model.
            Field::new(
                RESOLVED_MODEL,
                DataType::Struct(Fields::from(vec![
                    utf8("provider", true),
                    utf8("model", true),
                ])),
                true,
            ),
            utf8("resolved_deployment", true),
            boolean("streaming", false),
            ts_us_utc("started_at", false),
            ts_us_utc("terminal_at", false),
            utf8("outcome", false),
            utf8("error_code", true),
            int64("attempt_count", false),
            Field::new("usage", Self::usage_type(), true),
            utf8("cost", true),
            utf8("currency", true),
            Field::new("pricing_versions", Self::pricing_versions_type(), false),
            int64("capture_policy_version", false),
            variant_field(REQUEST_PAYLOAD, true),
            variant_field(RESPONSE_PAYLOAD, true),
            Field::new(
                "payload_object_refs",
                Self::payload_object_refs_type(),
                false,
            ),
        ]
    }

    /// Partitions hourly on `started_at` and clusters by call identity.
```

### crates/vala/vala-bifrost-redux/src/tables/traces/spans.rs

```rust
: &[F] = &[
    F::meta("trace_id", T::FixedSizeBinary(16), false),
    F::meta("span_id", T::FixedSizeBinary(8), false),
    F::meta("parent_span_id", T::FixedSizeBinary(8), true),
    F::meta("trace_state", T::Utf8, false),
    F::meta("flags", T::Int64, false),
    F::meta("name", T::Utf8, false),
    F::meta("kind", T::Int32, false),
    F::meta("start_time_unix_nano", signal::OTLP_INSTANT, false),
    F::meta("end_time_unix_nano", signal::OTLP_INSTANT, false),
    F::meta("duration_nano", T::Int64, false),
    F::meta("status_present", T::Bool, false),
    F::meta("status_code", T::Int32, true),
    F::payload("status_message", T::Utf8, true),
    F::sensitive("attributes", T::Variant, false),
    F::meta("dropped_attributes_count", T::Int64, false),
    F::sensitive("events", T::List(&SPAN_EVENT_ELEMENT), false),
    F::meta("dropped_events_count", T::Int64, false),
    F::sensitive("links", T::List(&SPAN_LINK_ELEMENT), false),
    F::meta("dropped_links_count", T::Int64, false),
    F::meta("resource_present", T::Bool, false),
    F::sensitive("resource_attributes", T::Variant, false),
    F::meta("resource_dropped_attributes_count", T::Int64, false),
    F::meta("resource_schema_url", T::Utf8, false),
    F::sensitive(
        "resource_entity_refs",
        T::List(&signal::ENTITY_REF_ELEMENT),
        false,
    ),
    F::meta("scope_present", T::Bool, false),
    F::meta("scope_name", T::Utf8, false),
    F::meta("scope_version", T::Utf8, false),
    F::sensitive("scope_attributes", T::Variant, false),
    F::meta("scope_dropped_attributes_count", T::Int64, false),
    F::meta("scope_schema_url", T::Utf8, false),
    F::meta("service_name", T::Utf8, true),
    F::meta("gen_ai_operation_name", T::Utf8, true),
    F::meta("gen_ai_provider_name", T::Utf8, true),
    F::meta("gen_ai_request_model", T::Utf8, true),
    F::meta("gen_ai_conversation_id", T::Utf8, true),
    F::meta("gen_ai_usage_input_tokens", T::Int64, true),
    F::meta("gen_ai_usage_output_tokens", T::Int64, true),
    F::meta("service_version", T::Utf8, true),
    F::meta("deployment_environment", T::Utf8, true),
    F::meta("http_request_method", T::Utf8, true),
    F::meta("http_route", T::Utf8, true),
    F::meta("http_response_status_code", T::Int64, true),
    F::meta("url_full", T::Utf8, true),
    F::meta("exception_type", T::Utf8, true),
    F::meta("exception_message", T::Utf8, true),
    F::meta("exception_stacktrace", T::Utf8, true),
];
```

### crates/vala/vala-bifrost-redux/src/tables/logs/records.rs

```rust
: &[CanonicalField] = &[
    F::meta("time_unix_nano", signal::OTLP_INSTANT, false),
    F::meta("observed_time_unix_nano", signal::OTLP_INSTANT, false),
    F::meta("severity_number", T::Int32, false),
    F::meta("severity_text", T::Utf8, false),
    F::meta("event_name", T::Utf8, true),
    F::sensitive("body", T::Variant, true),
    F::meta("trace_id", T::FixedSizeBinary(16), true),
    F::meta("span_id", T::FixedSizeBinary(8), true),
    F::meta("flags", T::Int64, false),
    F::sensitive("attributes", T::Variant, false),
    F::meta("dropped_attributes_count", T::Int64, false),
    F::meta("resource_present", T::Bool, false),
    F::sensitive("resource_attributes", T::Variant, false),
    F::meta("resource_dropped_attributes_count", T::Int64, false),
    F::meta("resource_schema_url", T::Utf8, false),
    F::sensitive(
        "resource_entity_refs",
        T::List(&signal::ENTITY_REF_ELEMENT),
        false,
    ),
    F::meta("scope_present", T::Bool, false),
    F::meta("scope_name", T::Utf8, false),
    F::meta("scope_version", T::Utf8, false),
    F::sensitive("scope_attributes", T::Variant, false),
    F::meta("scope_dropped_attributes_count", T::Int64, false),
    F::meta("scope_schema_url", T::Utf8, false),
    F::meta("service_name", T::Utf8, true),
    F::meta("service_version", T::Utf8, true),
    F::meta("deployment_environment", T::Utf8, true),
    F::meta("exception_type", T::Utf8, true),
    F::meta("exception_message", T::Utf8, true),
    F::meta("exception_stacktrace", T::Utf8, true),
    F::meta("body_text", T::Utf8, true),
];
```

### crates/vala/vala-bifrost-redux/src/tables/metrics/points.rs

```rust
: &[CanonicalField] = &[
    F::meta("metric_name", T::Utf8, false),
    F::meta("description", T::Utf8, false),
    F::meta("unit", T::Utf8, false),
    F::sensitive("metadata", T::Variant, false),
    F::meta("metric_type", T::Utf8, false),
    F::meta("time_unix_nano", signal::OTLP_INSTANT, false),
    F::meta("start_time_unix_nano", signal::OTLP_INSTANT, false),
    F::meta("flags", T::Int64, false),
    F::sensitive("attributes", T::Variant, false),
    F::meta("int_value", T::Int64, true),
    F::meta("double_value", T::Float64, true),
    F::meta("aggregation_temporality", T::Int32, true),
    F::meta("is_monotonic", T::Bool, true),
    F::meta("histogram_count", T::Int64, true),
    F::meta("histogram_sum", T::Float64, true),
    F::meta("histogram_min", T::Float64, true),
    F::meta("histogram_max", T::Float64, true),
    F::payload("bucket_counts", T::List(&BUCKET_COUNT_ELEMENT), true),
    F::payload("explicit_bounds", T::List(&EXPLICIT_BOUND_ELEMENT), true),
    F::meta("exponential_scale", T::Int32, true),
    F::meta("exponential_zero_count", T::Int64, true),
    F::meta("exponential_zero_threshold", T::Float64, true),
    F::payload("positive_buckets", T::Struct(&POSITIVE_BUCKET_FIELDS), true),
    F::payload("negative_buckets", T::Struct(&NEGATIVE_BUCKET_FIELDS), true),
    F::meta("summary_count", T::Int64, true),
    F::meta("summary_sum", T::Float64, true),
    F::payload("quantile_values", T::List(&QUANTILE_VALUE_ELEMENT), true),
    F::sensitive("exemplars", T::List(&EXEMPLAR_ELEMENT), false),
    F::meta("resource_present", T::Bool, false),
    F::sensitive("resource_attributes", T::Variant, false),
    F::meta("resource_dropped_attributes_count", T::Int64, false),
    F::meta("resource_schema_url", T::Utf8, false),
    F::sensitive(
        "resource_entity_refs",
        T::List(&signal::ENTITY_REF_ELEMENT),
        false,
    ),
    F::meta("scope_present", T::Bool, false),
    F::meta("scope_name", T::Utf8, false),
    F::meta("scope_version", T::Utf8, false),
    F::sensitive("scope_attributes", T::Variant, false),
    F::meta("scope_dropped_attributes_count", T::Int64, false),
    F::meta("scope_schema_url", T::Utf8, false),
    F::meta("service_name", T::Utf8, true),
    F::meta("service_version", T::Utf8, true),
    F::meta("deployment_environment", T::Utf8, true),
];
```

## Authority correction after user clarification (supersedes conflicting recommendations above)

The user clarified that source wins **all Bifrost behavior**, subject to the explicitly discussed main fixes. Therefore item 8's recommendation to impose main eager provisioning and remove source lazy `ensure_builtin` during `pin_cut` is withdrawn: retain the source Bifrost provisioning mechanism. Preserve main integration ownership where it supplies a distinct supported user workflow, but do not use main implementation shape to override source Bifrost lifecycle.

The removed `vala.dev.agent_traces` table and `CorrelationPolicy::CodeAxis` need narrower treatment: retain main removal as an unearned-fluff simplification only if source has no active approved capability requiring them. This scout observed the source registry/dev declaration and no live producer in the examined paths; it did not establish exhaustive absence. Source requirements take precedence over deletion based only on main's earlier cleanup.

Source explicit time columns, Variant/typed layout choices, mixed-version treatment, and fresh-state authority are authoritative. Item 9 is an operational inventory warning, not a requirement to introduce retained-data migration or compatibility behavior. Follow the source approved fresh-only/no-migration contract as confirmed by the migration researcher; do not broaden this merge with an invented migration system. The identified old registered-schema/queued-run incompatibilities explain why old state cannot be silently reused under that contract. They do not override the source's deployment decision.

All main security/identity/outbox/evaluation ownership fixes explicitly required by the user remain preservation requirements, including UID-only correlation, correct writer-bound activation, gateway judgment authority, and application Run lineage. The table schema adaptations described above must compose those fixes with source Bifrost behavior.

## Bounded provisioning/dev-table follow-up (supersedes item 8 and earlier correction where more specific)

Provisioning composes without replacing source catalog behavior: preserve main server boot/tenant provisioning integration where it calls the source catalog to ensure built-ins, and keep the source catalog's `ensure_builtin` mechanism unchanged. A source-first engine does not require deleting main application-level eager setup, nor does preserving that setup justify rewriting source `pin_cut`/ensure semantics. The surface scout found no source post-base `platform/builtins.rs` requirement replacing main eager integration.

The dev-table residue claim is **not supported** by approved-source evidence. Source `changes/active/bifrost-variant/spec.md:737-744` REQ-010 explicitly requires `vala.dev.agent_traces.messages` and `tool_io` to become Variant. Source TASK-003-decisions.md:111 also names it in the common sampling stratum. Source TASK-001-r4/task-review-behavior.md:31 marks REQ-010 implemented with agent-trace Variant payloads. Its post-base diff changes two Utf8 fields to Variant **and** adds the `started_at` layout axis; it is not a time-axis-only incidental edit. Source `wyrd-testing/tests/bifrost/server/verification_runtime.rs` exercises real generic Bifrost write/query and Variant-contract refusals for it (`:1263`, `:1312`, `:2086`, `:2700-2733`). An exhaustive source `git grep` over crates/SDKs/architecture/source packet found no production application/SDK-specific dev-capture producer beyond the generic Bifrost path; there are protocol types in wyrd-spec/src/vala/dev/record.rs and correlation.rs, registry definitions, engine tests, and generic journey fixtures.

Decision consequence: main ten-table cleanup and source approved dev-table requirement conflict explicitly. Under the user's later source-all-Bifrost authority, retain the source table/CodeAxis unless the user's named main-fluff preservation is interpreted to supersede this exact capability. If choosing main deletion, say it deliberately supersedes REQ-010's dev-table clause, omit the table/CodeAxis and dependent source fixture cases, and amend authoritative architecture/spec/docs accordingly; do not report it as no source requirement or a harmless unchanged-base residue. This scout supplies the evidence and does not invent that priority choice.

## Definitive merged provisioning and CodeAxis decision

Resolved source authority: **retain all eleven source built-in tables**, including `vala.dev.agent_traces`, and retain `CorrelationPolicy::CodeAxis`. REQ-010 explicitly earns this source schema capability. Do not classify it as stale fluff or ask for another priority decision. Preserve main server/tenant bootstrap eager integration, adapted to enumerate source's eleven definitions; retain source catalog ensure behavior. This supersedes the earlier ten-table deletion recommendation and the conditional dev-table discussion.

CodeAxis is a schema/envelope rule, **not a separate Card-kind authorization policy**. Source AgentTracesTable declares required Utf8 `run_id` as caller-authored content. `CorrelationPolicy::owns_run_id`, `appends`, execution_lanes::is_server_owned/user_fields, and source_schema_fingerprint must preserve that content in its authored slot and fingerprint; the managed envelope appends only `card_uid`, `principal_id`, `wyrd_request_id`, `wyrd_ingested_at`. Never remove it as ordinary correlation or append a second `run_id`. Source test `execution_lanes::tests::code_axis_decode_keeps_declared_run_id_content` pins exact one-column behavior.

No additional CodeAxis CardKind allowlist was found: source `validate_card_scope`/`resolve_card_uids` apply the same signed-scope resolution independent of policy. Compose main UID-only attribution directly: accept optional input `card_uid`, authorize bound principals against signed UID scope and unbound principals against registered tenant observation-target kind, and preserve authenticated writer identity. Do not recreate native card_ref, derive a second identity, invent Agent-only restrictions, or attach clients to declarative records. Repo/commit_sha/branch/dev_session_id are caller-authored code context; they do not replace authenticated identity. Update source dev generic-ingest fixtures to carry authorized card_uid rather than card_ref, with required content run_id retained. The source messages/tool_io Variant types and explicit started_at timestamp axis remain.

## FINAL USER OVERRIDE — dev table removed (supersedes every dev-table retention decision above)

The user explicitly instructed: **“no get rid of vala.dev.agent_traces for now”**. Preserve main's ten-table registry and omit source `vala.dev.agent_traces`. This overrides source REQ-010's dev-table clause only; Variant, time axes, and all remaining source Bifrost contracts stay authoritative.

Source exhaustive search found `CodeAxis` has exactly one real table owner, `AgentTracesTable`. Remove the variant and `owns_run_id` branch when omitting that table, together with dev table exports/definitions/protocol types/docs and caller-facing controls that are exclusively for this omitted capability. Confirm no newly integrated owner before deletion. Preserve ordinary Observation `run_id` correlation and None audit envelope.

Preserve source Variant proof properties while removing dev-specific fixtures: adapt generic native-ingest Variant/extension-refusal/roundtrip journey cases onto retained supported table shapes such as Eval observations (`context`/`media`) or gateway capture through its authorized internal path; do not preserve a test-only dev registration or weaken refusal assertions. The specific CodeAxis duplicate-run/fingerprint test protects an unreachable removed rule and should be retired with its sole owner, not migrated into fabricated behavior. Remove dev-specific schema docs, architecture examples, scripts/SDK exports if exclusively owned, and amend merged source specification requirements to omit the dev payload clause. Keep generic namespace/historical migration identifiers unless removing them is necessary and safe; historical SQL checks may still reference vala.dev and are not evidence of a live table.

Provisioning outcome: main server/tenant bootstrap eager integration enumerates the remaining ten source-updated definitions; source catalog ensure behavior remains unchanged. Retained-data and fresh-only authority remain as stated in earlier correction. Main UID-only attribution, writer-bound activation, outbox/identity/security, and application Run fixes remain composed with source engine.


---

## Archived snapshot: wyrd-plan-proof-agent-surfaces.md

# Focused MCP, CLI, OpenAPI and Service table proof

Read-only source confirmation against pinned main 8a8813a and source cfe21dc. Commands are focused existing names, not broad surface lanes. None run in this scout.

## Runtime MCP query catalog and invocation

Manifest wyrd-mcp Cargo.toml names target `mcp` at tests/bifrost/mcp/main.rs. Module `query::pg_tests` contains ignored real-server cases; dev dependencies activate server/test-support and Vala test-support. No explicit extra features needed for current owning command.

```bash
mise exec -- scripts/postgres/with-test-postgres.sh -- bash -lc 'mise run db:migrate:all:inner && cargo nextest run --locked -p wyrd-mcp --test mcp -P journey --run-ignored=all -E "test(=query::pg_tests::agent_investigates_from_multiple_evidence_pivots) | test(=query::pg_tests::bifrost_query_errors_are_actionable_before_row_execution) | test(=query::pg_tests::delegated_agent_query_is_authorized_as_the_subject)"'
```

Evidence: query.rs lines38,84,209; real rmcp client -> /mcp discovers and invokes `bifrost.query`, verifies result/repairable refusal/delegation. Source adds existing ignored names `query::pg_tests::builtin_variant_and_struct_payloads_are_queryable` and `query::pg_tests::sensitive_variant_leaf_is_denied_before_io`; after importing them into main module, exact same target/profile/environment with those exact expressions covers Variant rendering and leaf security. Tests must first be adapted to user's removed dev.agent_traces requirement (main ten builtins + dynamic datasets); these source names currently are absent on main.

Main server tool catalog shape unit exists at `mcp::bifrost::tests::query_schema_is_closed_bounded_and_has_no_path_selector` (bifrost.rs line271; inspect module name is tests before invocation). This server-only unit confirms advertised tool schema; runtime MCP cases above prove mounted catalog/execution.

## Compiled CLI JSONL and problem projection

Main manifest explicitly target `cli` tests/cli.rs mounts `mod query_server_journey`. Both named tests are real server tests, not ignored, and dev dependency supplies harness.

```bash
mise exec -- scripts/postgres/with-test-postgres.sh -- bash -lc 'mise run db:migrate:all:inner && cargo nextest run --locked -p wyrd-cli --test cli -P journey -E "test(=query_server_journey::query_command_reads_seeded_table) | test(=query_server_journey::query_command_denial_preserves_problem_without_read_decision)"'
```

Source augments the EXISTING `query_command_reads_seeded_table` with `parse_json` query returning Variant object including u64::MAX, asserting exact emitted JSONL number/value and successful terminal. Port those assertions into main same test; command remains exact. Denial test pins canonical problem/absence of query work. Do not claim CLI late failure is covered by these: neither main nor source defines a distinct CLI late-failure case here. If requirement includes compiled CLI late Variant failure, extend existing query journey or add meaningful test; list exact final name after implementation.

## HTTP late failure terminal

Existing server unit (no Postgres, no extra features):

```bash
mise exec -- cargo nextest run --locked -p wyrd-server --lib -E 'test(=query::routes::tests::http_query_stream_preserves_late_failed_terminal)'
```

Main source query/routes.rs line804; source updates its synthetic late terminal to VariantInvalidJson complete WyrdProblem. Covers post-byte framing, not a full client CLI journey. Main library consumers otherwise require source late-problem adaptation already specified. Source true late runtime failures are in wyrd-testing/oracle published tests; engine scout supplies their exact commands.

## Served OpenAPI

Main server target integration tests/integration/main.rs mounts pg_openapi_contract unconditionally. No test-support needed to select these cases (current principals owning lane uses none).

```bash
mise exec -- scripts/postgres/with-test-postgres.sh -- bash -lc 'mise run db:migrate:all:inner && cargo nextest run --locked -p wyrd-server --test integration -P journey -E "test(=pg_openapi_contract::bifrost_operations_publish_typed_problem_refusals) | test(=pg_openapi_contract::every_problem_response_declares_its_media_type_and_stable_code) | test(=pg_openapi_contract::no_operation_documents_an_audit_caused_refusal)"'
```

Existing sources lines706,494,671. Checks actual served document, Bifrost typed problem refs/stable codes and no reintroduced audit-caused refusals. Add `pg_openapi_contract::the_served_document_describes_the_composed_surface` if route composition itself changes.

## Main Service declared-table preflight and activation seam

Main exact test at server/tests/integration/pg_card_registration_route.rs line3935, integration module pg_card_registration_route. No explicit extra feature.

```bash
mise exec -- scripts/postgres/with-test-postgres.sh -- bash -lc 'mise run db:migrate:all:inner && cargo nextest run --locked -p wyrd-server --test integration -P journey -E "test(=pg_card_registration_route::service_declared_tables_are_ensured_before_activation)"'
```

Existing behavior: ensure declared `tickets` table before active Card; equivalent declaration reuses table; conflicting schema refuses with fingerprint code and leaves stored table intact; malformed/freeform table schema fails before Card/registration writes. The current main test expects freeform root `{type:object}` schema parse refusal: source Variant maps are allowed as COLUMNS, but top-level model must still be column object; retain this distinction with new shared mapper. Port imported `wyrd_queue::json_schema_to_fieldspec` in service.rs to shared wyrd-types owner while preserving entire workflow/no-write assertions. Extend same test with source-authoritative typed timestamp/Variant declared columns if needed rather than invent a second registration owner.

## Limits

No runtime builds/tests performed. Source-specific two MCP case commands become runnable only after explicit port. CLI late failure remains a concrete coverage gap to close if task acceptance claims it; HTTP unit does not substitute for compiled CLI behavior.


---

## Archived snapshot: wyrd-plan-proof-engine.md

# Engine proof reuse inventory

Read-only source-confirmed symbols at main 8a8813a7458b7c98707c7b4f9b9d403f604ee0e6 and source cfe21dc9007d5d22d9ae4411be07fb5504b0cc05. No builds, nextest list, or test execution performed. Exact paths below follow Rust module declarations, not guessed standalone file targets. Confirm final names with nextest list during implementation after moves; never accept zero selected tests.

Current main and source vala-bifrost-redux manifests both have `autotests=false`, sole integration target `tests/integration/main.rs`, required test-support, self dev-dependency enabling test-support/bench-support. Therefore commands do NOT add all-features or recreate source standalone binaries. Source new cloud feature is needed only cloud-backend proof, not these local tests. Current vala-sql main consolidates `pg_forge_tasks` into `integration`, unlike source standalone tests; command below explicitly retains main target. Main wyrd-testing already has oracle and scribe targets; source recovery module should be added to scribe/main.rs, not a new target.

DB command wrapper is current `scripts/postgres/with-test-postgres.sh`; exports admin fixture DSN plus separately provisioned tenant/platform DSNs and owns Docker cleanup. For shared server journeys run db:migrate:inner first, matching their mise lanes. These tests use local object storage; no cloud credentials. WYRD_LOG=info,vala_bifrost_redux=debug supplies tracing request; verify existing harness actually installs subscriber on a failure before diagnosis. Pure unit commands need no Postgres.

Existing family gates: `mise run test:bifrost:integration:redux` (all Redux tests; PostgreSQL wrapper); `mise run test:bifrost:integration:sql` (SQL); `mise run test:bifrost:journey:scribe`, `mise run test:bifrost:journey:oracle`, `mise run test:bifrost:journey:forge`; complete standalone capability `mise run test:bifrost`. Global gate selected by parent is final aggregate; do not re-run these family lanes after a green final aggregate unless parent establishes coverage gap. Exact focused commands below are iteration evidence, not duplicate final aggregates.

## Reuse symbols and exact commands

### main: `forge::leadership::tests::schedule_use_is_linearized_with_term_replacement`

Owner `vala-bifrost-redux` / `lib`. Term checked and used under same slot lock; preserve on source owner.

```bash
mise exec -- cargo nextest run --locked -p vala-bifrost-redux --lib -E 'test(=forge::leadership::tests::schedule_use_is_linearized_with_term_replacement)'
```

### main: `forge::leadership::tests::relinquished_term_refuses_every_handler`

Owner `vala-bifrost-redux` / `lib`. Scheduler exit revokes notify/pull/report.

```bash
mise exec -- cargo nextest run --locked -p vala-bifrost-redux --lib -E 'test(=forge::leadership::tests::relinquished_term_refuses_every_handler)'
```

### main: `forge::worker::tests::only_a_joined_loop_enables_same_owner_reclaim`

Owner `vala-bifrost-redux` / `lib`. Panic cannot grant quiescence; clean join can.

```bash
mise exec -- cargo nextest run --locked -p vala-bifrost-redux --lib -E 'test(=forge::worker::tests::only_a_joined_loop_enables_same_owner_reclaim)'
```

### main: `forge::production_routes::failed_scheduler_ends_its_term_before_restart`

Owner `vala-bifrost-redux` / `integration`. Real scheduler supervisor restart; keep existing integration owner.

```bash
mise exec -- scripts/postgres/with-test-postgres.sh -- cargo nextest run --locked -p vala-bifrost-redux --test integration -P journey --run-ignored=all -E 'test(=forge::production_routes::failed_scheduler_ends_its_term_before_restart)'
```

### main: `forge::production_routes::interrupted_startup_waits_for_its_prepared_lease`

Owner `vala-bifrost-redux` / `integration`. Interrupted startup cannot reclaim unexpired Prepared; preserve despite source same-owner SQL.

```bash
mise exec -- scripts/postgres/with-test-postgres.sh -- cargo nextest run --locked -p vala-bifrost-redux --test integration -P journey --run-ignored=all -E 'test(=forge::production_routes::interrupted_startup_waits_for_its_prepared_lease)'
```

### main: `forge::snapshot_expiration::accepted_expiry_past_the_lease_bound_bars_cuts_until_reconciled`

Owner `vala-bifrost-redux` / `integration`. Adapt reader-bar expectation to source retention, retain lease-bound unknown accepted effect proof.

```bash
mise exec -- scripts/postgres/with-test-postgres.sh -- cargo nextest run --locked -p vala-bifrost-redux --test integration -P journey --run-ignored=all -E 'test(=forge::snapshot_expiration::accepted_expiry_past_the_lease_bound_bars_cuts_until_reconciled)'
```

### main: `forge::snapshot_expiration::rejected_expiry_of_unknown_outcome_bars_cuts_until_reconciled`

Owner `vala-bifrost-redux` / `integration`. Adapt old reader pins; retain uncertain rejection and exact reconciliation proof.

```bash
mise exec -- scripts/postgres/with-test-postgres.sh -- cargo nextest run --locked -p vala-bifrost-redux --test integration -P journey --run-ignored=all -E 'test(=forge::snapshot_expiration::rejected_expiry_of_unknown_outcome_bars_cuts_until_reconciled)'
```

### main: `forge::snapshot_expiration::retryable_catalog_failure_retains_prepared_authority`

Owner `vala-bifrost-redux` / `integration`. Retryable catalog failure cannot reset uncertain Prepared.

```bash
mise exec -- scripts/postgres/with-test-postgres.sh -- cargo nextest run --locked -p vala-bifrost-redux --test integration -P journey --run-ignored=all -E 'test(=forge::snapshot_expiration::retryable_catalog_failure_retains_prepared_authority)'
```

### main: `pg_forge_tasks::pg_tests::previous_owner_reclaims_its_unexpired_attempt`

Owner `vala-sql` / `integration`. Actual SQL immediate reclaim of quiescent owner.

```bash
mise exec -- scripts/postgres/with-test-postgres.sh -- cargo nextest run --locked -p vala-sql --test integration -P journey --run-ignored=all -E 'test(=pg_forge_tasks::pg_tests::previous_owner_reclaims_its_unexpired_attempt)'
```

### main: `scribe::staging_runtime::tests::owns_any_tracks_only_snapshot_members`

Owner `vala-bifrost-redux` / `lib`. Ready+claimed members remain owned until settle; later members excluded.

```bash
mise exec -- cargo nextest run --locked -p vala-bifrost-redux --lib -E 'test(=scribe::staging_runtime::tests::owns_any_tracks_only_snapshot_members)'
```

### main: `scribe::persistence::tests::flush_waits_for_an_outside_claim_over_members_durable_at_drain`

Owner `vala-bifrost-redux` / `lib`. Concurrent publication cannot allow flush early return; fixtures use PgFixture.

```bash
mise exec -- scripts/postgres/with-test-postgres.sh -- cargo nextest run --locked -p vala-bifrost-redux --lib -E 'test(=scribe::persistence::tests::flush_waits_for_an_outside_claim_over_members_durable_at_drain)'
```

### main: `scribe::persistence::tests::flush_ignores_an_outside_claim_taken_after_its_snapshot`

Owner `vala-bifrost-redux` / `lib`. Later claim cannot indefinitely hold older flush.

```bash
mise exec -- scripts/postgres/with-test-postgres.sh -- cargo nextest run --locked -p vala-bifrost-redux --lib -E 'test(=scribe::persistence::tests::flush_ignores_an_outside_claim_taken_after_its_snapshot)'
```

### main: `oracle::admission::tests::oracle_shutdown_drains_only_oracle_memory`

Owner `vala-bifrost-redux` / `lib`. Live Forge bytes excluded; late Oracle child waited.

```bash
mise exec -- cargo nextest run --locked -p vala-bifrost-redux --lib -E 'test(=oracle::admission::tests::oracle_shutdown_drains_only_oracle_memory)'
```

### main: `oracle::admission::tests::oracle_shutdown_drains_oracle_infallible_headroom`

Owner `vala-bifrost-redux` / `lib`. Oracle headroom belongs to Oracle drain.

```bash
mise exec -- cargo nextest run --locked -p vala-bifrost-redux --lib -E 'test(=oracle::admission::tests::oracle_shutdown_drains_oracle_infallible_headroom)'
```

### source: `scribe::shards::tests::group_commit_syncs_once`

Owner `vala-bifrost-redux` / `lib`. One fsync submission per complete batch group.

```bash
mise exec -- cargo nextest run --locked -p vala-bifrost-redux --lib -E 'test(=scribe::shards::tests::group_commit_syncs_once)'
```

### source: `scribe::shards::tests::same_batch_resent_before_commit_lands_in_a_later_group_and_commits_once`

Owner `vala-bifrost-redux` / `lib`. Replay/resent idempotence under group commit.

```bash
mise exec -- cargo nextest run --locked -p vala-bifrost-redux --lib -E 'test(=scribe::shards::tests::same_batch_resent_before_commit_lands_in_a_later_group_and_commits_once)'
```

### source: `scribe::shards::tests::group_commit_crash_between_sync_and_fence_replays`

Owner `vala-bifrost-redux` / `lib`. Source fixture starts PgFixture; source lacks pg_tests name; keep gated classification in destination.

```bash
mise exec -- scripts/postgres/with-test-postgres.sh -- cargo nextest run --locked -p vala-bifrost-redux --lib -E 'test(=scribe::shards::tests::group_commit_crash_between_sync_and_fence_replays)'
```

### source: `scribe::pg_scribe_crash_injection::zeroed_block_after_the_newest_fence_ends_the_chain`

Owner `vala-bifrost-redux` / `lib`. Unacknowledged final corrupt tail truncation.

```bash
mise exec -- cargo nextest run --locked -p vala-bifrost-redux --lib -E 'test(=scribe::pg_scribe_crash_injection::zeroed_block_after_the_newest_fence_ends_the_chain)'
```

### source: `scribe::pg_scribe_crash_injection::bad_record_before_an_acknowledged_commit_fails_closed`

Owner `vala-bifrost-redux` / `lib`. Acknowledged WAL corruption cannot be discarded; file pg name does not imply actual DB in this pure test.

```bash
mise exec -- cargo nextest run --locked -p vala-bifrost-redux --lib -E 'test(=scribe::pg_scribe_crash_injection::bad_record_before_an_acknowledged_commit_fails_closed)'
```

### source: `scribe::pg_scribe_crash_injection::bad_record_without_a_fence_authority_fails_closed`

Owner `vala-bifrost-redux` / `lib`. No authority cannot turn corruption into tail discard.

```bash
mise exec -- cargo nextest run --locked -p vala-bifrost-redux --lib -E 'test(=scribe::pg_scribe_crash_injection::bad_record_without_a_fence_authority_fails_closed)'
```

### source: `catalog::cut_cache::tests::cut_cache_serves_only_fresh_listened_cuts`

Owner `vala-bifrost-redux` / `lib`. Listener live flag, invalidation generation, bound and reconnect reset.

```bash
mise exec -- cargo nextest run --locked -p vala-bifrost-redux --lib -E 'test(=catalog::cut_cache::tests::cut_cache_serves_only_fresh_listened_cuts)'
```

### source: `catalog::layout::tests::time_column_drives_partition_and_sort`

Owner `vala-bifrost-redux` / `lib`. Explicit partition time owner drives sort and transform.

```bash
mise exec -- cargo nextest run --locked -p vala-bifrost-redux --lib -E 'test(=catalog::layout::tests::time_column_drives_partition_and_sort)'
```

### source: `catalog::layout::tests::invalid_time_column_is_refused_naming_the_field`

Owner `vala-bifrost-redux` / `lib`. Trust-boundary type/name refusal.

```bash
mise exec -- cargo nextest run --locked -p vala-bifrost-redux --lib -E 'test(=catalog::layout::tests::invalid_time_column_is_refused_naming_the_field)'
```

### source: `scribe::execution_lanes::tests::same_iceberg_type_spellings_cast_to_the_registered_type`

Owner `vala-bifrost-redux` / `lib`. By-name conformance using registered canonical schema.

```bash
mise exec -- cargo nextest run --locked -p vala-bifrost-redux --lib -E 'test(=scribe::execution_lanes::tests::same_iceberg_type_spellings_cast_to_the_registered_type)'
```

### source: `scribe::execution_lanes::tests::a_naive_timestamp_is_refused_for_a_zoned_column`

Owner `vala-bifrost-redux` / `lib`. No invented timezone during conformance.

```bash
mise exec -- cargo nextest run --locked -p vala-bifrost-redux --lib -E 'test(=scribe::execution_lanes::tests::a_naive_timestamp_is_refused_for_a_zoned_column)'
```

### source: `scribe::parquet_writer::tests::row_group_bounds_read_the_time_column`

Owner `vala-bifrost-redux` / `lib`. Footer event bounds follow explicit schema time column.

```bash
mise exec -- cargo nextest run --locked -p vala-bifrost-redux --lib -E 'test(=scribe::parquet_writer::tests::row_group_bounds_read_the_time_column)'
```

### source: `scribe::parquet_writer::tests::recovery_runs_are_unshredded_and_the_claim_sample_shreds`

Owner `vala-bifrost-redux` / `lib`. Recovery format and stratified sampled final format differ intentionally.

```bash
mise exec -- cargo nextest run --locked -p vala-bifrost-redux --lib -E 'test(=scribe::parquet_writer::tests::recovery_runs_are_unshredded_and_the_claim_sample_shreds)'
```

### source: `forge::reader_expiry_ordering::replaced_snapshots_expire_only_after_the_retention_window`

Owner `vala-bifrost-redux` / `integration`. Source retention replaces main active reader pins; same current target/module.

```bash
mise exec -- scripts/postgres/with-test-postgres.sh -- cargo nextest run --locked -p vala-bifrost-redux --test integration -P journey --run-ignored=all -E 'test(=forge::reader_expiry_ordering::replaced_snapshots_expire_only_after_the_retention_window)'
```

### source: `forge::reader_expiry_ordering::cut_read_completes_while_snapshot_expiry_holds_the_authority`

Owner `vala-bifrost-redux` / `integration`. Source Oracle never blocks on Forge destructive authority.

```bash
mise exec -- scripts/postgres/with-test-postgres.sh -- cargo nextest run --locked -p vala-bifrost-redux --test integration -P journey --run-ignored=all -E 'test(=forge::reader_expiry_ordering::cut_read_completes_while_snapshot_expiry_holds_the_authority)'
```

### source: `forge::managed_rewrite::v3_row_lineage_survives_repeated_rewrite`

Owner `vala-bifrost-redux` / `integration`. Exact v3 logical row lineage after repeated rewrite.

```bash
mise exec -- scripts/postgres/with-test-postgres.sh -- cargo nextest run --locked -p vala-bifrost-redux --test integration -P journey --run-ignored=all -E 'test(=forge::managed_rewrite::v3_row_lineage_survives_repeated_rewrite)'
```

### source: `forge::managed_rewrite::standard_variant_layouts_round_trip_per_file`

Owner `vala-bifrost-redux` / `integration`. Mixed shredded layouts preserve logical Variant by per-file reads.

```bash
mise exec -- scripts/postgres/with-test-postgres.sh -- cargo nextest run --locked -p vala-bifrost-redux --test integration -P journey --run-ignored=all -E 'test(=forge::managed_rewrite::standard_variant_layouts_round_trip_per_file)'
```

### source: `published::variant_sql_registry_covers_every_session`

Owner `wyrd-testing` / `oracle`. Every production session installs Variant SQL; source/main same published module.

```bash
mise exec -- scripts/postgres/with-test-postgres.sh -- bash -lc "mise run db:migrate:inner && cargo nextest run --locked -p wyrd-testing --test oracle -P journey --run-ignored=all -E 'test(=published::variant_sql_registry_covers_every_session)'"
```

### source: `published::struct_and_variant_share_physical_pushdown`

Owner `wyrd-testing` / `oracle`. Struct/Variant typed leaf pushes through hot/promoted/rewritten reads.

```bash
mise exec -- scripts/postgres/with-test-postgres.sh -- bash -lc "mise run db:migrate:inner && cargo nextest run --locked -p wyrd-testing --test oracle -P journey --run-ignored=all -E 'test(=published::struct_and_variant_share_physical_pushdown)'"
```

### source: `distributed::pg_hot_variant_filter_on_an_unshredded_key_reads_null`

Owner `wyrd-testing` / `oracle`. Distributed absent leaf remains SQL null.

```bash
mise exec -- scripts/postgres/with-test-postgres.sh -- bash -lc "mise run db:migrate:inner && cargo nextest run --locked -p wyrd-testing --test oracle -P journey --run-ignored=all -E 'test(=distributed::pg_hot_variant_filter_on_an_unshredded_key_reads_null)'"
```

### source: `recovery::variant_staging_restores_and_publishes_once`

Owner `wyrd-testing` / `scribe`. Source-only recovery module; add source recovery.rs and mod recovery to current existing scribe binary.

```bash
mise exec -- scripts/postgres/with-test-postgres.sh -- bash -lc "mise run db:migrate:inner && cargo nextest run --locked -p wyrd-testing --test scribe -P journey --run-ignored=all -E 'test(=recovery::variant_staging_restores_and_publishes_once)'"
```

## Adaptation boundaries

Main accepted/rejected expiry test names express old SQL-reader barring; keep names only if final retained behavior still fits. Reuse orchestration/barriers and exact pre/post-submit durability assertions while rewriting active-read table expectations for source retention. If renamed, update focused command after confirming final Rust symbol. No source active-reader tests may become invisible archived files.

Source group crash test currently sits in `scribe::shards::tests` despite real PostgreSQL; main AGENTS requires pg_tests classification for DB work. If moved under `scribe::shards::pg_tests`, source current exact command above ceases to be final; use nextest list and replace it with actual destination rather than declaring invented final name now. Same caveat applies main persistence flush DB tests currently nested tests. Preserve normal lane isolation as part of integration.

The tables source scout covers all SDK Variant/OTLP/time journeys. This inventory supplies engine-local supporting proof; it does not substitute for Rust/Python/TypeScript end-to-end client-to-server journeys. No live cloud, synthetic load, or opt-in benchmark is necessary for these ordinary engine proofs.


---

## Archived snapshot: wyrd-plan-proof-evaluation.md

# Focused proof map: evaluation, attribution, audit schema

Read-only scout of pinned main `8a8813a7458b7c98707c7b4f9b9d403f604ee0e6`; source `cfe21dc9007d5d22d9ae4411be07fb5504b0cc05`. No tests, builds, or code edits performed. Commands below are proposed implementation checks whose test names and binary/module owners are confirmed from source and manifests; not claims of execution. Retain main test layout. Source core Bifrost authoritative; ten built-ins, no dev.agent_traces, no sole-owner CodeAxis.

## Proof owners and exact adaptation

| Property | Owning code | Existing exact proof; extend same body for merged seams |
|---|---|---|
| Bound Eval writer activates only own binding; unbound writer activates subject's matching bindings | main verification/observations.rs ObservationEnqueue/bound_card/ObservationRunSink; wyrd-sql queries/verifier_runs.rs enqueue_observation_batch + paired subject/writer lock/insert SQL | server eval_verification::eval_runs_follow_the_writing_owner; SQL pg_verifier_runs::observation_runs_follow_the_writer |
| ACK survives enqueue SQL outage and recovers, orderly shutdown drains | same outbox and SQL queue | server eval_verification::integrated_enqueue_outage_preserves_ack_and_recovers; integration pg_verification_runtime::observation_outbox_retains_through_an_outage_and_flushes_at_shutdown |
| Replay next day activates once using admitted created_at, not new receipt | Gate first-ACK hook; tables/eval/observations.rs acknowledged; source created_at axis composed into main decoder | server eval_verification::sealed_replay_on_a_later_day_activates_once; unit tables::eval::observations::tests::acknowledged_keys_mirror_scribe_stamping (retain test name, revise invariant to UID + created_at) |
| Delayed schedule database clock/cursor/missed occurrence and one scheduler claim | main VerifierRunQueue::tick_schedule; SCHEDULE_CLAIM_DELAY + DueBindingRow.schedule_clock | SQL pg_verifier_runs::concurrent_schedulers_create_one_run_per_occurrence and scheduler_skips_inactive_unready_and_missed_occurrences; integration pg_verification_runtime::schedulers_create_one_run_per_occurrence_across_ticks_and_restart |
| Lease renewal one-third threshold, never revive, stale-token fencing, long-running cancellation | main VerifierRunQueue::renew + verification/leases.rs | SQL pg_verifier_runs::leases_renew_once_a_third_has_passed_and_never_revive + reclaimed_lease_fences_the_stale_token; integration pg_verification_runtime::renewal_keeps_a_long_run_and_a_taken_token_cancels_it |
| Gateway-mediated judge, writer authority, no direct provider bypass; stable failure | main EvalEngine::execute/judge_invoker, authority::writer_caller; ServerWyrdGatewayCaller | server eval_verification::continuous_eval_runs_the_terminal_matrix (already binds gpt-test model to JUDGE_ROLE and drives mock upstream through gateway); integration pg_verification_routes::direct_execution_calls_the_judge_provider (existing Eval+Task judge provider matrix); extend these bodies to assert denied judge authority makes no upstream request and permitted caller retains gateway audit/attribution |
| Task result evidence + application Run linkage retained under source typed summaries/Variant details | main ResultPayloadBuilder::build/summary, ResultRun, VerifierReport::run_id/with_run, main direct/runtime flow | server eval_verification::direct_task_judgment_records_one_correlated_result; unit verification::results::tests::direct_task_writes_one_summary_row. Change existing details decode from Utf8 JSON text to Variant, retain exact AssertionResult fields and application run_id/source_record_id assertions |
| Structured Drift/Eval summaries, null/unscorable, ran vs skipped detail items, mapping by column name | ResultPayloadBuilder and source table structs | unit verification::results::tests::eval_result_writes_ran_and_skipped_items_then_summary, sampled_out_eval_writes_zero_count_summary_only, reordered_table_fields_keep_values_bound_to_names; server verification_runtime::result_layout_partitions_blooms_and_prunes_by_result (extend to parent ended_at day differing from detail ingest day, preserve result_id join) |
| Eval trace ns timestamp and Variant attribute decode, order/ceilings | main BifrostReader::record/spans with source typed decode/time predicates | server eval_verification::continuous_eval_reads_ordered_bounded_trace_evidence + continuous_eval_refuses_a_trace_over_the_span_ceiling |
| Audit event projection preserves decision time, credential and main content fields; Variant detail | tables/audit/projection.rs project_audit_event + AuditLogTable; staged AuditEvent decided_at | unit tables::audit::projection::tests::projects_canonical_content_with_the_decision_time (existing name says correct invariant; revise field type/null and timestamp name to created_at, no SQL sequence/hash staging) |
| UID-only native authorization before ACK; signed scope/null/malformed/duplicate behavior | Gate attribution registry; ingress card_scope; authorized_card_uids and source conform/stamp | unit scribe::execution_lanes::tests::card_scope_rejects_before_writer_admission + optional_and_scoped_card_correlations_stamp_signed_uids; OTLP negative::pg_tests::unbound_writer_attributes_spans_to_registered_cards_only + identical_exports_from_two_principals_each_keep_their_own_attribution |

Source-confirmed modules/targets:
- wyrd-testing Cargo.toml `[[test]] name="server" path="tests/bifrost/server/main.rs"`; main root declares `mod eval_verification; mod verification_runtime;`. No feature argument needed; manifest dependency enables wyrd-server test-support.
- wyrd-testing `[[test]] name="otlp" path="tests/bifrost/otlp/main.rs"`; negative.rs nests tests inside `mod pg_tests`, so exact expression includes negative::pg_tests.
- wyrd-server `--test integration`, root tests/integration/main.rs includes pg_verification_routes and feature-gated pg_verification_runtime; use `--features test-support` for combined proof command.
- wyrd-sql integration root tests/integration/main.rs includes pg_verifier_runs; no additional feature needed. Its dev-dependency enables wyrd-dev-fixtures/pg.
- vala-bifrost-redux inline tests `--lib`; no additional feature required for listed pure tests.
- wyrd-server inline result tests `--lib`; `--features test-support` aligns with already selected runtime integration artifact cone.

## Exact focused commands for every named proof above

These use the repository Postgres wrapper, migration setup, mise toolchain, the owning package/target, and **exact** nextest expressions. Each OR operand selects an existing full test name; no positional filter or whole-suite lane. `-P journey --run-ignored=all` follows main mise server/otlp lane convention; SQL/integration tests are not ignored. Prefix tracing so failures have production diagnostics immediately. The migration command provisions required dependencies and is setup, not a broad verification lane.

### Real SDK/server evaluation journeys

```bash
scripts/postgres/with-test-postgres.sh -- bash -lc 'mise run db:migrate:all:inner && WYRD_LOG=info,vala_bifrost_redux=debug mise exec -- cargo nextest run --locked -p wyrd-testing --test server -P journey --run-ignored=all -E "test(=eval_verification::eval_runs_follow_the_writing_owner) | test(=eval_verification::integrated_enqueue_outage_preserves_ack_and_recovers) | test(=eval_verification::sealed_replay_on_a_later_day_activates_once) | test(=eval_verification::continuous_eval_runs_the_terminal_matrix) | test(=eval_verification::direct_task_judgment_records_one_correlated_result) | test(=eval_verification::continuous_eval_reads_ordered_bounded_trace_evidence) | test(=eval_verification::continuous_eval_refuses_a_trace_over_the_span_ceiling) | test(=verification_runtime::result_layout_partitions_blooms_and_prunes_by_result)"'
```

### Runtime dependency integration (queue, schedule, lease, direct judge)

```bash
scripts/postgres/with-test-postgres.sh -- bash -lc 'mise run db:migrate:all:inner && WYRD_LOG=info,vala_bifrost_redux=debug mise exec -- cargo nextest run --locked -p wyrd-server --features test-support --test integration --test-threads=1 -E "test(=pg_verification_runtime::observation_outbox_retains_through_an_outage_and_flushes_at_shutdown) | test(=pg_verification_runtime::schedulers_create_one_run_per_occurrence_across_ticks_and_restart) | test(=pg_verification_runtime::renewal_keeps_a_long_run_and_a_taken_token_cancels_it) | test(=pg_verification_routes::direct_execution_calls_the_judge_provider)"'
```

### SQL dependency proofs

```bash
scripts/postgres/with-test-postgres.sh -- bash -lc 'WYRD_LOG=info mise exec -- cargo nextest run --locked -p wyrd-sql --test integration --test-threads=1 -E "test(=pg_verifier_runs::observation_runs_follow_the_writer) | test(=pg_verifier_runs::concurrent_schedulers_create_one_run_per_occurrence) | test(=pg_verifier_runs::scheduler_skips_inactive_unready_and_missed_occurrences) | test(=pg_verifier_runs::leases_renew_once_a_third_has_passed_and_never_revive) | test(=pg_verifier_runs::reclaimed_lease_fences_the_stale_token)"'
```

SQL PgFixture::start owns fresh per-test database/migrations; main mise test:sql:inner likewise wraps Postgres but does not invoke db:migrate:all:inner before SQL crate tests. This source-confirmed ownership avoids unnecessary extra setup here.

### Native decoder + audit projection + Eval ACK pure proof

```bash
WYRD_LOG=info mise exec -- cargo nextest run --locked -p vala-bifrost-redux --lib -E 'test(=tables::audit::projection::tests::projects_canonical_content_with_the_decision_time) | test(=tables::eval::observations::tests::acknowledged_keys_mirror_scribe_stamping) | test(=scribe::execution_lanes::tests::card_scope_rejects_before_writer_admission) | test(=scribe::execution_lanes::tests::optional_and_scoped_card_correlations_stamp_signed_uids)'
```

### Result authoring pure proof

```bash
WYRD_LOG=info mise exec -- cargo nextest run --locked -p wyrd-server --features test-support --lib -E 'test(=verification::results::tests::direct_task_writes_one_summary_row) | test(=verification::results::tests::eval_result_writes_ran_and_skipped_items_then_summary) | test(=verification::results::tests::sampled_out_eval_writes_zero_count_summary_only) | test(=verification::results::tests::reordered_table_fields_keep_values_bound_to_names)'
```

### Real OTLP UID journeys

```bash
scripts/postgres/with-test-postgres.sh -- bash -lc 'mise run db:migrate:all:inner && WYRD_LOG=info,vala_bifrost_redux=debug mise exec -- cargo nextest run --locked -p wyrd-testing --test otlp -P journey --run-ignored=all -E "test(=negative::pg_tests::unbound_writer_attributes_spans_to_registered_cards_only) | test(=negative::pg_tests::identical_exports_from_two_principals_each_keep_their_own_attribution)"'
```

## Acceptance limits

Keep these same proof bodies and source-owned Variant/time/schema assertions under final combined contracts; never preserve old wyrd_event_time just to satisfy a test. No deleted dev table/CodeAxis test enters this proof list. These focused tests cover this scout's merge seams, not all 27 conflicts or all first-class language projections. Other owners supply SDK, identity, audit-outbox delivery, query protocol, Bifrost engine and peer-command proofs. This report does not prescribe fullsuite/gate inside the single task. Exact names are from pinned source; if integration intentionally relocates a module, update its canonical command using nextest list before reporting it run, and never accept zero selection.


---

## Archived snapshot: wyrd-plan-proof-lifecycle.md

# Focused integration proof: membership, internal peers, lifecycle, and local bootstrap

Read-only inspection anchored to main `8a8813a7458b7c98707c7b4f9b9d403f604ee0e6`. Commands below are recipes for the integration tree, not executions or claims of passing tests. Inspected current `mise.toml`, manifests, `.config/nextest.toml`, `scripts/postgres/with-test-postgres.sh`, module roots, test source and bootstrap fixture requirements. No compilation or nextest discovery run.

## Environment and feature facts

- `mise exec --` supplies pinned Rust/nextest/tools and `[env] RUST_BACKTRACE=1`. There is no checked-in dotenv-loading requirement in these selected mise lanes; their Postgres environment comes from the owning wrapper.
- `scripts/postgres/with-test-postgres.sh` owns a fresh Docker Compose project, DBA setup, serving logins and database ownership, exports `DATABASE_URL`, `WYRD_TEST_DATABASE_ADMIN_URL`, `WYRD_DATABASE_URL`, `WYRD_PLATFORM_DATABASE_URL`, and `WYRD_DB_MAX_CONNECTIONS` default 8. It destroys only its ephemeral project at exit. Do not substitute hand-exported production DSNs.
- Normal Bifrost server/Oracle journey lane migrates via `db:migrate:inner`. This task itself runs the **exact** Wyrd migration-idempotency test; journey fixtures create/migrate their own databases as needed. SQL supporting lane and Workflow proof may use `db:migrate:all:inner`, which additionally verifies Vala migrations; this is a focused setup lane, not suite/gate.
- `wyrd-testing` has default features `[]`, but its manifest dependency declarations enable `wyrd-server/test-support`, `vala-bifrost-redux/test-support,bench-support`, `wyrd-client/internal,test-support`, and `wyrd-tonic/server,client`. Thus its `--test server` and `--test oracle` recipes require **no extra CLI feature flag**; this matches owning mise leaves.
- Direct `wyrd-server --lib` internal-outbox proof needs `--features test-support` for `ClusterRegistry::publish_snapshot_for_test`. It uses local fake Scribe and TLS peers and lazy nonconnecting pools; no Postgres wrapper needed.
- Direct `wyrd-server --test integration` Workflow proof needs `--features test-support` because `tests/integration/main.rs` only declares `pg_workflow_runs` behind that feature. Omitting it selects no such test.
- Journey profile is required for process-per-test telemetry isolation and bounded server concurrency. `.config/nextest.toml` serializes Oracle `peer_network::*` and `workflow::*` through the peer-clusters test group because those clusters use fixed pod-like loopback ports. Retain `-P journey`.
- `WYRD_LOG=info,vala_bifrost_redux=debug` can precede a rerun of any failed recipe to satisfy trace-first diagnosis. Harness production telemetry installs tracing; no credentials needed for local upstream/provider mocks.

## P1: task ownership stop, abort, Drop

Source: `crates/vala/vala-bifrost-redux/src/cluster/mod.rs`, module `tests`.

```
mise exec -- cargo nextest run --locked -p vala-bifrost-redux --lib -E 'test(=cluster::tests::stop_joins_the_task_once) | test(=cluster::tests::stop_aborts_a_task_past_its_deadline) | test(=cluster::tests::abort_and_drop_end_the_task)'
```

Protects main `ClusterTask` lifetime: cancellation joins once, repeated stop is safe, deadline aborts stuck task, explicit abort and failed-boot Drop stop tasks. Database-free supporting proof, does not replace P2.

## P2: readiness withdrawal still heartbeats, poller survives outbox drain

Sources: `crates/wyrd/wyrd-testing/tests/bifrost/server/{owner_inspection,query}.rs`; `main.rs` declares those modules directly.

```
mise exec -- scripts/postgres/with-test-postgres.sh -- bash -lc 'mise run db:migrate:inner && cargo nextest run --locked -p wyrd-testing --test server -P journey --run-ignored=all -E "test(=owner_inspection::unready_roles_heartbeat_until_teardown_ends_membership_tasks) | test(=query::oracle_only_pod_retains_audit_staged_before_its_drain)"'
```

Exact existing behaviors:
1. Withdraw Oracle readiness and prove durable heartbeat timestamp advances while ready remains false; successful shutdown and abort end every membership task.
2. Oracle stages audit with Scribe not yet booted, Scribe then boots, Oracle is immediately drained, another Oracle reads the retained audit. This catches source process-token poller freeze.

## P3: independently fenced durable role rows

Source: `crates/vala/vala-sql/tests/integration/pg_oracle_membership.rs`, consolidated `--test integration` target.

```
mise exec -- scripts/postgres/with-test-postgres.sh -- bash -lc 'mise run db:migrate:all:inner && cargo nextest run --locked -p vala-sql --test integration -E "test(=pg_oracle_membership::same_node_roles_have_independent_fences) | test(=pg_oracle_membership::stale_role_mutation_is_rejected) | test(=pg_oracle_membership::live_discovery_excludes_expired_heartbeat)"'
```

Scribe/Oracle on one node register and advance fences independently; stale mutations refuse; expired heartbeat is excluded. Keep main consolidated target layout; source prebase-style standalone target is unnecessary.

## P4: role-complete private listener and Scribe-only contexts

Sources: `crates/wyrd/wyrd-testing/tests/bifrost/oracle/peer_network/{listener,security,transport}.rs`; all actual tests (not helper functions) have `#[tokio::test]` and ignored journey marker.

```
mise exec -- scripts/postgres/with-test-postgres.sh -- bash -lc 'mise run db:migrate:inner && cargo nextest run --locked -p wyrd-testing --test oracle -P journey --run-ignored=all -E "test(=peer_network::listener::peer_listener_is_isolated_mtls_and_role_complete) | test(=peer_network::security::peer_context_refusals) | test(=peer_network::transport::peer_transport_uses_immutable_fenced_destinations)"'
```

Preserves private/public separation, expected mounts per target, mTLS membership requirement, capability context requirement, missing TLS refusal, Scribe restart fence, volume identity coupling, exact Oracle endpoints. Security journey explicitly boots `[Oracle, Oracle, Scribe]` and proves correct fragment runs on Scribe-only pod while foreign-tenant fragment executes no rows. Transport proof prevents replacing a frozen peer incarnation with its successor. Adapt fixture assignment fields to source physical codec; retain test assertions.

Note: `peer_cluster.rs` is an in-process bound multi-listener harness, not an external prebuilt production binary prerequisite for these selected tests; no `cargo build -p wyrd-server` prerequisite in owning `test:server:peer:inner`.

## P5: internal Scribe routing with no Oracle, identical bounded retries, silent peer timeout

Source: `crates/wyrd/wyrd-server/src/scribe_outbox.rs::tests`. Explicit `test-support` is necessary as noted above.

```
mise exec -- cargo nextest run --locked -p wyrd-server --lib --features test-support -E 'test(=scribe_outbox::tests::a_peer_route_needs_no_oracle) | test(=scribe_outbox::tests::a_retry_resubmits_identical_frames) | test(=scribe_outbox::tests::a_terminal_rejection_never_blocks_later_writes) | test(=scribe_outbox::tests::an_oversized_detail_reaches_scribe_in_bounded_frames_that_replay_identically) | test(=scribe_outbox::tests::an_unsplittable_row_is_submitted_whole_and_terminal_refusal_consumes_it) | test(=scribe_outbox::tests::a_silent_peer_times_out_and_the_slice_retries_on_the_next_scribe)'
```

Peer-route proof composes only registry+TLS, no Oracle; staged work waits for ready Scribe. Other proof forces bounded large-detail frames and exact replay identity, terminal refusal settles without blocking later writes, unsplittable row loss classification, silent first TLS Scribe timeout then retry on next Scribe plus another tenant progress. Local runtime/network proof, no database.

Main restores configured grpc decoding ceiling separately; the outbox bounded-frame proof uses a **small** request ceiling. It does not independently exercise a valid >4 MiB capture frame against production mount. If testing that retained limit is required, add one focused peer server journey with configured Scribe ceiling >4 MiB and a legitimately encoded frame >tonic default, prove ACK+readback. Do not claim P5 alone proves the production mount maximum.

## P6: accepted Workflow reservation and abandoned blocking preparation drain

Source `crates/wyrd/wyrd-server/src/components/workflow/runs.rs::tests`.

```
mise exec -- cargo nextest run --locked -p wyrd-server --lib --features test-support -E 'test(=components::workflow::runs::tests::drain_waits_for_a_reservation_before_its_task_exists) | test(=components::workflow::runs::tests::drain_waits_for_blocking_work_its_caller_abandoned)'
```

IO-free/no-DB runtime tests. Reservation ownership precedes spawning, closes admission atomically, wakes waiter on release; detached blocking hydration remains counted until it actually returns.

## P7: accepted Workflow shutdown and forwarded Bifrost owner settlement

Sources: `crates/wyrd/wyrd-server/tests/integration/pg_workflow_runs.rs` and `crates/wyrd/wyrd-testing/tests/bifrost/oracle/workflow.rs`.

```
mise exec -- scripts/postgres/with-test-postgres.sh -- bash -lc 'mise run db:migrate:all:inner && cargo nextest run --locked -p wyrd-server --features test-support --test integration -P journey -E "test(=pg_workflow_runs::lifecycle_races_retention_and_shutdown)" && cargo nextest run --locked -p wyrd-testing --test oracle -P journey --run-ignored=all -E "test(=workflow::workflow_forwarded_query_settles_before_the_run_ends)"'
```

First test is **not ignored**; do not use `--run-ignored=only` on it. It cancels running/queued/mid-preparation runs, checks whole stable snapshots, retention eviction, restart non-resumption, and shutdown settlement. Second is ignored and loops cancel, run deadline, longer requested query deadline, shorter query deadline, pod loss; proves remote Analytical query owns terminal+cleanup before local Workflow ends. Restoring just BoundServer drain call while dropping AppState Workflow owner/config/tool tracking is not sufficient.

## P8: installed CLI local bootstrap preservation, minimal Rust proof

Source: `crates/wyrd/wyrd-cli/tests/pg_tests.rs`; target root `tests/cli.rs` declares `mod pg_tests`. None of these three tests is ignored.

```
mise exec -- cargo build --locked -p wyrd-cli -p wyrd-server
mise exec -- scripts/postgres/with-test-postgres.sh -- cargo nextest run --locked -p wyrd-cli --test cli -E 'test(=pg_tests::server_dev_first_run_is_client_ready) | test(=pg_tests::server_dev_repeat_and_partial_failure_preserve_state) | test(=pg_tests::server_dev_selected_mcp_host_reads)'
```

Build is mandatory: LocalDev::new expects production `wyrd-server` sibling of compiled CLI executable, fixtures its installed version via symlink, starts empty `UnmigratedDatabase`, and CLI itself migrates/provisions. **Do not pre-run db:migrate for this proof:** pristine unmigrated setup is what it tests. Wrapper provides only DBA-owned lifecycle.

Fixture itself chooses isolated home, HTTP/gRPC/peer binds, `APP_ENV=development`; removes inherited platform DSN, storage URL and config path for child. Test first run proves fresh saved-config client usability and gRPC; repeat/failure proves retained state, credential and config preservation/refusals; MCP host proof uses saved installed host integration.

## P9: explicit cross-language local bootstrap requirement (if mapped into integration acceptance)

Existing Python-parametrized installed startup journey is `sdks/wyrd-sdk-python/tests/integration/test_local_development.py::test_installed_cli_bootstrap[rust|python|typescript|interrupt|exit]`, marked integration. The source uses a temporary installed layout copied from `target/debug/wyrd-server`, reads `target/debug/wyrd --version`, and creates its own **unmigrated** managed DB. Thus no db:migrate setup step.

Prerequisites from existing `test:local-dev:journey`: run `mise run py:setup`, `mise run ts:build`, `mise run ts:build:testing` once before needed runtimes, plus P8 cargo build. `py:setup` owns `uv sync` and `maturin develop --features testing` with `codegen:stubs` dependency. `ts:build` and testing build own package install/native addon builds; no full test suite/gate needed.

Focused runtime recipes:

```
mise exec -- scripts/postgres/with-test-postgres.sh -- uv run --directory sdks/wyrd-sdk-python python -m pytest -q -m integration 'tests/integration/test_local_development.py::test_installed_cli_bootstrap[rust]'
mise exec -- scripts/postgres/with-test-postgres.sh -- uv run --directory sdks/wyrd-sdk-python python -m pytest -q -m integration 'tests/integration/test_local_development.py::test_installed_cli_bootstrap[python]'
mise exec -- scripts/postgres/with-test-postgres.sh -- node scripts/check_ts_package.mjs mise exec -- uv run --directory sdks/wyrd-sdk-python python -m pytest -q -m integration 'tests/integration/test_local_development.py::test_installed_cli_bootstrap[typescript]'
mise exec -- scripts/postgres/with-test-postgres.sh -- uv run --directory sdks/wyrd-sdk-python python -m pytest -q -m integration 'tests/integration/test_local_development.py::test_installed_cli_bootstrap[interrupt]' 'tests/integration/test_local_development.py::test_installed_cli_bootstrap[exit]'
```

TypeScript packaging wrapper is required by existing lane; do not invoke pytest TypeScript parameter bare. Interrupt/exit prove first-setup-only host offer and restart readiness, relevant to main latest local bootstrap corrections. Choose mapped runtime requirements explicitly; P8 Rust-only proof cannot claim Python/TypeScript parity.

## Merge wording correction

Base `5ab92b0039`→source has no changes to Workflow module files; base→main adds all five. Endpoint main→source shows deletion because source never acquired main additions. Ordinary three-way merge keeps main-only modules. Whole-source chunk acceptance can remove AppState/boot wiring even while those files remain. Integration must preserve that wiring and P6/P7 behavior.


---

## Archived snapshot: wyrd-plan-proof-migrations.md

# Existing migration proof anchors and exact focused commands

Inspected main 8a8813a. Preserve main consolidated `--test integration` layout (source reverted tests to per-file targets). Commands below are proposed verification, not run.

## Historical SQL bytes

`crates/vala/vala-sql/src/lib.rs::tests::forge_and_oracle_migrations_have_unique_forward_owners` pins main OLAP SQL digest e7c5ec...; preserve this test and main file.

```sh
mise exec -- cargo nextest run --locked -p vala-sql --lib -E 'test(=tests::forge_and_oracle_migrations_have_unique_forward_owners)'
```

`crates/vala/vala-sql/tests/integration/pg_migration.rs::pg_tests::shipped_audit_staging_migration_is_immutable` pins creating audit staging migration bytes (despite current retirement).

```sh
mise exec -- cargo nextest run --locked -p vala-sql --test integration -E 'test(=pg_migration::pg_tests::shipped_audit_staging_migration_is_immutable)'
```

## Replay, idempotence, retired audit tables, current tenancy readiness

Existing main Vala `pg_migration::pg_tests::vala_migrations_apply_and_are_idempotent` applies Vala twice, verifies owner schemas and absent retired audit staging/history/function. Wyrd `pg_migration::pg_tests::migrations_apply_and_are_idempotent` applies Wyrd twice and proves current principal/credential/grants/auth/device schema with retired pre-principal tables absent. `single_url_boot_and_separate_url_regression` covers runtime boot connection roles; `tenant_policy_on_another_column_fails_readiness` proves readiness rejects malformed tenant policy.

```sh
mise exec -- scripts/postgres/with-test-postgres.sh -- cargo nextest run --locked -p vala-sql --test integration --test-threads=1 -E 'test(=pg_migration::pg_tests::vala_migrations_apply_and_are_idempotent)'
mise exec -- scripts/postgres/with-test-postgres.sh -- cargo nextest run --locked -p wyrd-sql --test integration --test-threads=1 -E 'test(=pg_migration::pg_tests::migrations_apply_and_are_idempotent) | test(=pg_migration::pg_tests::single_url_boot_and_separate_url_regression) | test(=pg_migration::pg_tests::tenant_policy_on_another_column_fails_readiness)'
```

## Bifrost SQL tenant scope

Main `tests/integration/pg_schema_usage.rs` tests `tenant_login_registers_bifrost_table` and `fresh_schema_is_tenant_only_and_keys_are_tenant_qualified` exercise registration through tenant connection, absent old scope/tenant_bucket fields, canonical system tenant, same table uid independently scoped to tenants, and physical file_list index shape.

```sh
mise exec -- scripts/postgres/with-test-postgres.sh -- cargo nextest run --locked -p vala-sql --test integration --test-threads=1 -E 'test(=pg_schema_usage::pg_tests::tenant_login_registers_bifrost_table) | test(=pg_schema_usage::pg_tests::fresh_schema_is_tenant_only_and_keys_are_tenant_qualified)'
```

## Identity upgrade and tenant isolation anchors

Main `pg_migration::pg_tests::human_connection_upgrade_preflight` builds pre-human-connection schema and tests preflight around persisted issuer state. Main `pg_admin_principals::pg_tests::system_principal_upgrade_backfills_existing_tenants_idempotently` proves tenant existing-state backfill idempotence. Main `pg_admin_principals::pg_tests::tenant_principal_queries_are_confined_by_row_level_security` proves principal tenant isolation. These are relevant preserved main behaviors, **not exhaustive proof that every credential/card survives the Bifrost port**.

```sh
mise exec -- scripts/postgres/with-test-postgres.sh -- cargo nextest run --locked -p wyrd-sql --test integration --test-threads=1 -E 'test(=pg_migration::pg_tests::human_connection_upgrade_preflight) | test(=pg_admin_principals::pg_tests::system_principal_upgrade_backfills_existing_tenants_idempotently) | test(=pg_admin_principals::pg_tests::tenant_principal_queries_are_confined_by_row_level_security)'
```

## Real proof gap to extend existing tests

Existing named tests above do not prove the exact integration transition: database with **all pinned main migrations already applied**, persisted main identities/credentials/cards, and **zero old Bifrost tables**, then new forward Bifrost migrations, main historical ledger checksums unchanged, persisted main rows unchanged, new tables/reader retention/notification/WAL-fence SQL installed with main RLS/operator policies. Extend an existing migration integration test with this scenario rather than claim the current idempotence assertions already prove it. No invented new test name or focused command supplied. Main readiness `OperatorPool::verify_migrations` enforces ledger checksum equality; full transition should exercise that on the upgraded database.

Use `mise run test:sql` as owning SQL aggregate; mise provides Postgres bootstrap. Fresh v3 data plane and wrong-format rejection anchors are owned by engine scout and are intentionally not duplicated here.

No commands executed. Exact names derived from source modules; confirm `cargo nextest list` after integration retains main consolidated targets before executing.


---

## Archived snapshot: wyrd-plan-proof-security.md

# Focused security/identity integration proof (source-inspected, not executed)

Pinned main 8a8813a7458b7c98707c7b4f9b9d403f604ee0e6. Test names below inspected in source, target/features from Cargo.toml and mise.toml. `cargo nextest list` has not run, so implementation must confirm selection after merge. No builds or tests run in this research. No full suite/gate is prescribed.

## Credential-preserving audit retention + private destination trust

Existing journey names in `wyrd-testing/tests/bifrost/server/audit_retention.rs`, server target declared in wyrd-testing/Cargo.toml with no required feature; dependency already enables wyrd-server test-support. Extend the retention journey to read typed created_at and Variant detail; it currently proves credential/null and no seq fields but stages fabricated AuditEvent, so does not cover actual Gate/Oracle event construction.

```bash
mise exec -- scripts/postgres/with-test-postgres.sh -- bash -lc 'mise run db:migrate:inner && mise exec -- cargo nextest run --locked -p wyrd-testing --test server -P journey --run-ignored=all -E "test(=audit_retention::retained_history_carries_both_credential_shapes) | test(=audit_retention::system_owner_security_rejections_retain_once)"'
mise exec -- cargo nextest run --locked -p wyrd-server --lib -E 'test(=grpc::capture_peer::tests::requests_are_confined_to_tenant_capture_destinations)'
```

Validator covers six destinations, SYSTEM_OWNER only audit, result Verifier attribution required and forbidden elsewhere, malformed ids, wrong Verifier kind. Keep it on the new Scribe frame contract.

Actual Gate builder support test to extend: `vala-bifrost-redux/src/gate/mod.rs::tests::gate_enforces_bifrost_record_write`. Its RecordingAudit implements GateAudit directly, so change/add test plumbing to use an AuditStage recording sink and exercise the production blanket GateAudit impl. Assert credential_id plus principal, tenant, allowed/denied, request and delegation attribution. Existing command:

```bash
mise exec -- cargo nextest run --locked -p vala-bifrost-redux --lib -E 'test(=gate::tests::gate_enforces_bifrost_record_write)'
```

No existing focused test was found exercising production Oracle `query_audit_event` credential assignment: add one next to `oracle/mod.rs::tests` using the existing `payload_context` helper or an AuthorizedQueryContext; distinguish planned new test name from existing tests. Also extend existing retained_history journey with actual keyed ingress/read decisions so builder→outbox→Scribe retention is proven. This cannot be substituted by its existing directly-staged events.

## OTLP API-key entrance

Existing `trace_export::pg_tests::stock_exporter_authenticates_with_api_key_after_token_lifetime` in wyrd-testing otlp target: authenticates unmodified upstream gRPC exporter with only API key, outlives short access TTL, also exports HTTP, refuses expired bearer/unknown key/key outside OTLP, reads evidence through Bifrost. Extend log/metric signals if needed; existing trace proof does not claim all signals.

```bash
mise exec -- scripts/postgres/with-test-postgres.sh -- bash -lc 'mise run db:migrate:inner && mise exec -- cargo nextest run --locked -p wyrd-testing --test otlp -P journey --run-ignored=all -E "test(=trace_export::pg_tests::stock_exporter_authenticates_with_api_key_after_token_lifetime)"'
```

Preserve Gate AuthContext::from_verified/admit_otlp_verified used by main server API-key entrance, even though source engine lacks them.

## Scoped gateway payload reads

Existing IO-free planning test exercises SQL projection, wildcard, predicate and subquery attempts reaching payload; metadata reads remain allowed. Extend sensitive column set for Variant replacements rather than falling back to Utf8 names only.

```bash
mise exec -- cargo nextest run --locked -p vala-bifrost-redux --lib -E 'test(=oracle::tests::gateway_payload_columns_require_payload_read_authority)'
```

Existing server gateway payload-object test proves authorized retrieval/refusal, convergence and expired-object behavior plus decision audit; module declarations in components/gateway/mod.rs confirm this path. Repository test:gateway:native does not itself select this lib test, so run focused command directly with Postgres lifecycle:

```bash
mise exec -- scripts/postgres/with-test-postgres.sh -- bash -lc 'mise run db:migrate:all:inner && mise exec -- cargo nextest run --locked -p wyrd-server --lib -E "test(=components::gateway::pg_invocation_tests::gateway_payload_objects_are_authorized_convergent_and_stable_when_expired)"'
```

## Changed boot retains identity, credential and DSN behavior

Unit construction test in boot/auth.rs builds production auth handles, mints and verifies a token. Extend to assert credential claim survives when boot-composed audit collaborators are adapted; existing fixture user_principal uses credential_id None, so that needs an explicit credential-bearing case.

```bash
mise exec -- cargo nextest run --locked -p wyrd-server --lib -E 'test(=boot::auth::tests::build_auth_handles_mints_tokens_the_request_verifier_accepts) | test(=boot::auth::tests::build_auth_handles_rejects_an_invalid_signing_key)'
mise exec -- cargo nextest run --locked -p wyrd-sql --lib -E 'test(=dsn::tests::explicit_serving_dsns_stay_separate_and_derive_the_catalog) | test(=dsn::tests::platform_url_falls_back_to_database_url)'
mise exec -- cargo nextest run --locked -p wyrd-server --lib -E 'test(=config::tests::the_signing_key_file_requires_a_restrictive_regular_bounded_file)'
```

DSN tests inspect external tenant/platform login handling, operator/catalog options and redaction, but do not prove boot against actual distinct DB roles. Operator journey proves shipped initialization and usable provisioned tenant. These integration names belong to integration target and normal platform journey lane adds no special feature, so use its exact target/feature pattern:

```bash
mise exec -- scripts/postgres/with-test-postgres.sh -- bash -lc 'mise run db:migrate:all:inner && mise exec -- cargo nextest run --locked -p wyrd-server --test integration -E "test(=platform_admin_e2e::an_operator_initializes_the_deployment_through_the_shipped_binary) | test(=platform_admin_e2e::operator_provisions_a_usable_tenant_without_touching_the_database) | test(=platform_admin_e2e::revoking_a_platform_credential_ends_its_live_sessions)"'
```

These proof points complement the boot diff review: shared ScribeOutbox passed to all AuditStage collaborators, no old SYSTEM result tokens, API-key entrance only OTLP, main identity migrations preserved, and external DSNs fed into source catalog/resource construction. Exact names must be confirmed on final integrated tree before claiming selected/passed.


---

## Archived snapshot: wyrd-plan-proof-surfaces.md

# Source-confirmed public-surface proof commands for integration planning

Read-only scout. Main inspected at `8a8813a7458b7c98707c7b4f9b9d403f604ee0e6`; source `cfe21dc9007d5d22d9ae4411be07fb5504b0cc05`. No tests/builds/environment setup run. These are existing test names and file paths verified from source, with focused invocations formed using normative AGENTS.md exact-test syntax and current mise environment-owning wrappers. No newly invented scenario identifiers are presented as existing tests.

## Focused main Rust tests suitable for the implementation TASK

The SDK package `wyrd-sdk-rust` has one target `integration` at tests/integration/main.rs. Test sources define module names shown below. The manifest dev-dep on itself activates `otel` and `testing` for tests, so the current SDK journey commands explicitly add no features. Use locked nextest, journey profile, ignored tests enabled, and the repository Postgres setup plus both migrations. `mise run db:migrate:all:inner` is setup, not a broad proof suite.

```bash
mise exec -- scripts/postgres/with-test-postgres.sh -- bash -lc 'mise run db:migrate:all:inner && cargo nextest run --locked -p wyrd-sdk-rust --test integration -P journey --run-ignored=all -E "test(=query_bifrost::parameterized_sql_returns_the_callers_rows) | test(=query_bifrost::bound_sql_text_is_treated_as_data) | test(=query_bifrost::stream_yields_arrow_batches_and_a_terminal) | test(=query_bifrost::caller_without_bifrost_read_is_refused)"'

mise exec -- scripts/postgres/with-test-postgres.sh -- bash -lc 'mise run db:migrate:all:inner && cargo nextest run --locked -p wyrd-sdk-rust --test integration -P journey --run-ignored=all -E "test(=observe_a_run::run_observations_read_back_by_run_id) | test(=observe_a_run::card_scoped_key_cannot_write_another_cards_observations) | test(=observe_a_run::unsealable_byte_budget_is_refused_at_connect)"'

mise exec -- scripts/postgres/with-test-postgres.sh -- bash -lc 'mise run db:migrate:all:inner && cargo nextest run --locked -p wyrd-sdk-rust --test integration -P journey --run-ignored=all -E "test(=scheduled_drift_alerts_operator::failed_schedule_alerts_its_operator_on_the_connection_origin) | test(=verify_in_real_time::exact_verifier_grant_verifies_only_that_verifier) | test(=verify_in_real_time::another_tenant_cannot_verify_the_assistant)"'

mise exec -- cargo nextest run --locked -p wyrd-client --lib -E 'test(=bifrost::query::tests::sql_forwards_bind_values_without_interpolation)'
```

Coverage: retained typed params/injection separation; Arrow terminal completion and authorization refusal; Run/Card UID correlated observation readback; Card-scope denial and queue admission; scheduled drift -> Operator and exact verifier/tenant scope. These tests are main behavior checks. Source Variant-native observation assertions must be merged into these tests or separately ported, then exact final names confirmed with `mise exec -- cargo nextest list --locked -p <crate> --test integration` before a task report claims a named test ran.

Existing main `query_bifrost::unwritten_builtin_table_reads_as_empty` also covers main eager registration read behavior; same command pattern selecting only that exact name is valid if needed. None of these names refers to vala.dev.agent_traces; user has removed that table from integration scope.

## Focused main Python files/tests

Setup once: `mise run py:setup` (native development wheel includes testing). Runtime needs repository Postgres wrapper. The current owning Bifrost lane does db:migrate:all:inner before invoking Python SDK uv toolchain.

```bash
mise exec -- scripts/postgres/with-test-postgres.sh -- bash -lc 'mise run db:migrate:all:inner && cd sdks/wyrd-sdk-python && uv run python -m pytest -q -m integration tests/integration/test_query_bifrost.py::test_bound_sql_text_is_treated_as_data tests/integration/test_query_bifrost.py::test_stream_yields_arrow_batches_and_a_terminal tests/integration/test_observe_a_run.py::test_run_observations_read_back_by_run_id tests/integration/test_observe_a_run.py::test_card_scoped_key_cannot_write_another_cards_observations tests/integration/test_scheduled_drift_alerts_operator.py::test_failed_schedule_alerts_its_operator_on_the_connection_origin tests/integration/test_verify_in_real_time.py::test_exact_verifier_grant_verifies_only_that_verifier'
```

Verified individual existing source files to import and run with the same wrapper/migration/cwd/uv/-m integration pattern (these paths do NOT exist in pinned main before integration):

- tests/integration/bifrost/test_variant_tables.py: existing `test_rows_come_back_as_native_values`, `test_query_results_copy_into_another_table`, `test_struct_fields_stay_typed_and_variant_paths_stay_variant`, `test_undeclared_field_is_refused`.
- tests/integration/bifrost/test_every_iceberg_column_type.py: existing `test_every_iceberg_type_round_trips_with_nested_nulls`.
- tests/integration/bifrost/test_register_a_table_from_a_model.py.
- tests/integration/bifrost/test_three_timestamp_types.py.
- tests/integration/bifrost/test_table_time_column.py: existing `test_user_table_time_column_prunes_a_time_range`, `test_an_invalid_time_column_is_refused_naming_the_field`.

The five source files are focused capability files, not whole journey suites. Port their setup/schema/correlation/params to main without restoring old deleted query/drift story files. Use explicit file paths/node IDs in TASK and report the actual final names. Source Variant admission/round-trip fixtures use dynamic datasets; dev-table-specific observation/query tests must be reassigned to supported/dynamic tables under the user's explicit no-agent_traces override.

## Focused main TypeScript files/tests

Setup once: `mise run ts:build`, `mise run ts:build:testing` (public and private harness addons). Current test task uses SDK directory, pnpm exec vitest, and Postgres wrapper.

```bash
mise exec -- scripts/postgres/with-test-postgres.sh -- bash -lc 'mise run db:migrate:all:inner && cd sdks/wyrd-sdk-ts/wyrd && pnpm exec vitest run tests/integration/query-bifrost.test.ts tests/integration/observe-a-run.test.ts tests/integration/scheduled-drift-alerts-operator.test.ts'
```

These are bounded story files. Source-confirmed new individual files for same wrapper/cwd/Vitest run once ported:

- tests/integration/variant-tables.test.ts: existing cases “returns inserted rows as native values”, “copies query results into another table”, “keeps Struct fields typed and Variant paths Variant”, “refuses an undeclared field and writes nothing”.
- tests/integration/every-iceberg-column-type.test.ts.
- tests/integration/register-a-table-from-a-model.test.ts.
- tests/integration/three-timestamp-types.test.ts.
- tests/integration/table-time-column.test.ts: existing cases “a user table's time column prunes a time range”, “an invalid time column is refused naming the field”.

Do not omit table-time-column: source's focused aggregate explicit list did omit it. Reuse main tests/support/server.ts (config-home and credential isolation), not source competing tests/support/test-server.ts.

## Source Rust Variant tests and relocation constraint

Source currently mounts five new modules under deleted old target tests/pg_bifrost_e2e.rs: every_iceberg_column_type, register_a_table_from_a_model, table_time_column, three_timestamp_types, variant_tables. Source exact functions include `every_iceberg_type_round_trips_with_nested_nulls`, `api_requests_read_back_as_the_same_models`, `an_open_dict_field_is_queried_by_key`, `user_table_time_column_prunes_a_time_range`, `invalid_time_column_is_refused_naming_the_field`, `wyrd_timestamps_read_back_as_wyrd_values`. Integrated main target is `wyrd-client --test integration` with `--features test-support`; source target `--test pg_bifrost_e2e` is obsolete in integration. Final module prefixes depend on rehoming and must be listed from integrated source/nextest; do not prescribe an invented future prefix as an existing command. The current main `test:bifrost:gate` enables `wyrd-client/test-support` for this target. TASK should require focused exact integrated commands after listing; no aggregate `test:bifrost:journey:sdk` needed.

## Existing format/lint/codegen/boundary commands

These are verification commands, not full journey suites. Task may require only those relevant to its touched source; aggregate membership is separately documented below.

- `mise run fmt:check`: cargo fmt --all -- --check.
- `mise run lints`: workspace all-features/all-targets Clippy -D warnings plus standalone production default wyrd-server binary Clippy -D warnings.
- `mise run check:workspace-hack`: cargo hakari generate --diff; manage-deps --dry-run; verify.
- `mise run codegen:check`: generated schema/Card/Python stubs/TS codes/theme drift. Regeneration command `mise run codegen:regen` must be performed before declaring drift green.
- `mise run check:deps`: current consolidated production graph script (one native stack, AWS-LC, clients/PyO3/test seams).
- `mise run check:tenant-isolation`: Python tenant/static boundary check.
- `mise run py:format:check`, `mise run py:lints`, `mise run py:typecheck`.
- `mise run ts:napi:check` (depends on ts:build), `mise run ts:format:check`, `mise run ts:lints`, `mise run ts:typecheck` (public/testing builds).

Main served OpenAPI exact suite lives in `wyrd-server --test integration` under pg_openapi_contract; confirm actual exact fn before naming focused case. Owning lane `test:principals:integration` is a full surface suite and belongs in spec closeout coverage, not task focused iteration.

## Whole-candidate proof belongs only in SPEC

`mise run gate` is justified because integration alters global Cargo pins, multiple owners, SDK wire contracts, CI/test ownership. Existing pinned main gate depends on check, check:deps, check:tenant-isolation, check:storage:drift, check:ci-selection, check:docs, check:examples, codegen:check, docs:check, test:rust, test:bifrost:gate, test:gateway:gate, verify:python-sdk, verify:rust-sdk, verify:typescript-sdk, examples:python:datacard. Three SDK verification tasks bring current full SDK journeys and local-development/identity coverage. Do not prescribe gate plus redundant final component gates. This is spec aggregate evidence only, not integration TASK iteration.

Geometry facts: gate's Bifrost share explicitly excludes exact `qualification::scribe_512_mib_physical_object_qualifies`. Separate existing Scribe geometry lane is `mise run test:bifrost:journey:scribe:production-geometry` (repository wrapper + migrations, `wyrd-testing --test scribe -P journey --run-ignored=only -E test(=qualification::scribe_512_mib_physical_object_qualifies)`). Forge geometry lane `mise run test:bifrost:journey:forge:production-geometry` sets WYRD_FORGE_PRODUCTION_GEOMETRY=1 for existing exact `production_closeout::compaction_geometry_exact_rows_and_non_destructive_second_pass` in `wyrd-testing --test forge`. Normal Forge suite can run reduced geometry; full production sizes require separate env/lane. Include any required geometry only in spec change-closeout, not broad task proof. Gate does not include opt-in capacity benchmarks, live provider/Operator smoke or real cloud storage tests.


## P-028 — AC-028 hunk ledger (TASK-002-CLOSE-03)

Scope: `git diff cfe21dc9007d5d22d9ae4411be07fb5504b0cc05 -- <P-028 paths>`. The per-part ledgers below were built at `622299986` in seven read-only passes (each hunk compared with S, main and the post-S log). Every item they flagged is resolved in the disposition table, against the candidate named in the TASK-002 evidence.

Classification legend: **X1–X10** spec exceptions; **main-lint** conformance to main's workspace `[lints]` (REQ-026), with any behaviour change flagged; **main base** a main surface outside Bifrost's engine that the user rule keeps ("Main is the base and the starting point"); **restored** reverted to S.

### Mechanical checks

| P-028 step | Command | Result |
| --- | --- | --- |
| 2 struck symbols | `rg -n 'OwnedMembers\|owns_any\|…\|chain_hash' crates/vala crates/wyrd/wyrd-server/src crates/wyrd/wyrd-testing architecture/bifrost-design.md` | no output |
| 3 migrations | `git diff --stat cfe21dc9… -- crates/vala/vala-sql/migrations` | 27 files. **X2**: `20260802000000_vala_audit_staging`, `…26_audit_tenant_admin_principal_kind`, `…27_audit_staging_credential_id`, `…29_audit_system_principal_kind` absent. **X10**: `20260906000001_maintenance_leases` and `20260910000007_forge_lease_fence_assertion` absent; `forge_snapshot_expiration_claims` drops `lease_key`/`lease_fencing_token`; `forge_tasks` strategy CHECK adds `manifest_rewrite`; `cluster_nodes` comment. **X7**: `20260801000002/3` grant migrations absent; every other `GRANT`/`REVOKE`/role-existence block replaced by main's `operator_access` policy (`wyrd.operator_session()`), `_sqlx_migrations`/`forge_scheduler_state`/`forge_worker_claim_state` get RLS; `assert_scribe_publication_fence` and `scribe_wal_newest_fence` follow main's operator raise/restore. **main**: one trailing blank line in `20260910000018` (main's `diff --check`). No forward, drop or main-only Bifrost migration added. No X5 edit exists (`vala.dev.agent_traces` was never a Postgres object) |
| 4 table schema | `git diff --stat cfe21dc9… -- crates/vala/vala-bifrost-redux/src/tables`; `tables::tests::` | only X1 (`card_uid`), X2 (audit content), X5 (`dev/agent_traces.rs` removed), X9 (`assertion_result`, also in `SENSITIVE_PAYLOAD_COLUMNS`) |
| 5 `agent_traces` wording | `rg -n agent_traces crates architecture docs sdks` | no output |

Privilege note (X7): main's two-login model grants `wyrd_tenant` CRUD on every `wyrd`/`vala` table and execution of every Wyrd function, with RLS confining rows (`scripts/postgres/test-database-setup.sql`). S's `REVOKE`s (claim-row `UPDATE` refusal, no tenant `INSERT` on `forge_tasks`, definer functions executable only by `wyrd_app`) presupposed S's role set and have no main equivalent; migrations name no role under main. The two Scribe definer functions return only fence booleans and WAL positions, never tenant rows.

### Disposition of every flagged item

| # | Item | Disposition | Evidence |
| --- | --- | --- | --- |
| 1 | `server/query.rs` expects `WYRD_VALA_500_QUERY_EXECUTION_FAILED`, not S's 502 | X8 | main `5dfee4d3f` is the test half of `c63e2ff46`: a cancel before the first batch settles through `settle_distributed_failure(QueryExecutionFailed)` |
| 2 | `verification_runtime.rs` `run_id: None` | false positive | REQ-016: task Verifier results carry the judged application Run, absent for a direct run; rustdoc corrected `6f370a205` |
| 3 | Scribe test seams from main `6aaccebf9` | X2 | the shared outbox makes `audit_log` a Scribe table, so lifecycle tests target their own table |
| 4 | `analytical_cleanup_pause_for_test` hook | main base | used by main's `oracle/workflow.rs` journey and S's `oracle/mcp.rs`; `cfg(feature = "test-support")` |
| 5 | first-batch cancel sets `cancellation_marker` | X8 | X8 text: "a cancel observed before the first batch is recorded as `cancelled`". Removing it fails `oracle mcp::pg_tests::agent_runs_three_table_analytical_query_through_mcp` ("did not record Analytical cancellation") |
| 6 | lapsed claim mid-commit closes `cancelled`, not `failed` | X10 | the claim is the table authority; losing it is a cancellation of that attempt, not a strategy failure |
| 7 | two fence checks removed from snapshot expiry | X10 | they preceded only reads; the catalog commit stays behind `require_fence` + `commit_window_fits` and every write opens through `begin_fenced` |
| 8 | expiry `AlreadyApplied` returns before a fence check | X10 | the replay writes nothing |
| 9 | `ForgeTasks::reclaim_expired` deleted | X10 | dead after X10; `reclaim_expired_attempts` is the one path; doc fixed `6f370a205` |
| 10 | dropped privilege assertions; `scribe_wal_newest_fence` raise/restore | X7; fixed | privilege note above; function converted `82c89ff94` (vala-sql 77/77, `check:tenant-isolation`) |
| 11 | `#[cfg(feature = "internal")]` gates on the Bifrost client API | main base | main's client API (`wyrd-client`) |
| 12 | client renames `describe_table`, `stream(.., deadline)`, `to_bytes` | main base | same |
| 13 | `whole_count` errors where S saturated | main-lint | main's `whole_count` is identical; main denies truncating `as` casts. A negative or non-finite count is a broken binding |
| 14 | Oracle and Gate audit rows had a null credential | fixed (REQ-011) | `f23eb1716`: both builders call `with_credential_id(principal.credential_id)`; `server query::service_b_acts_for_service_a_with_only_a_table_authority` asserts B's direct write names B's API key and the delegated refusal names none |
| 15 | main `capacity.rs` FIFO, `owner_inspection` shutdown tests, `peer_cluster` histogram helper | main base | main journeys over S behaviour; lanes pass |
| 16 | main product journeys (`workflow`, `observe_ingest`, `query_lifecycle`, `trace_export`, `ambiguous_retry`) | main base | they assert S's query/ingest contract and pass |
| 17 | four main client tests (`grpc.rs`, `mod.rs`) | main base | main `6f25f42b4` moved S's queue/stall/register-race mechanics into these unit tests |
| 18 | `production_closeout` filters `ForgeError::Shutdown`; schedule sizes include `audit_log` | X2 | under X2 the stopping node can be promoting `audit_log`; the filter matches only the exact clean-stop error the worker itself releases (`worker.rs` clean-cancel match), so `ShutdownRetained` and real failures still fail |
| 19 | `fail_next_failure_settlement`; claim aged in SQL | X10 | replaces S's `fail_next_lease_release` at the runtime fatal path X10 leaves (`ca978e169`); ageing the claim is X10's form of ageing the lease |
| 20 | sibling settlement proof lost | restored | `ab56a47c9` |
| 21 | `follower_retains_a_graph_it_cannot_release` asserts released | X8 | no-wait release: a follower releases its graph inline |
| 22 | Forge claim/ownership assertions dropped | restored | `3b91313c2`, `52c7cd529` |
| 23 | S Gate SYSTEM-writer tests deleted | unrepresentable | `AccessTokenClaims::into_verified` refuses every system claim set, so a SYSTEM principal cannot reach Gate |
| 24 | journey assertions loosened | restored / X2 | `describe.rs`, `ambiguous_retry.rs` durable rows `6bf7dee46`; `scribe/telemetry.rs` counters carry no table label and `audit_log` shares the Scribe (X2); `verification_runtime.rs` "no runner" negative unreachable under X2 |
| 25 | stalled-tenant audit proof deleted with `audit_publication.rs` | X2 | `wyrd-runtime/src/outbox.rs::a_failing_tenant_does_not_delay_another_tenant` |
| 26 | five S `pg_bifrost_e2e` tests absent | main base | main `6f25f42b4` re-homed them: canonical read-back split by signal (`canonical_*`), queue/backpressure and register race to `wyrd_client::bifrost` unit tests (item 17), ambiguous retry and describe to Scribe journeys (item 24) |
| 27 | `trace_export.rs:737` integer vs `Timestamp(ns)` | fixed | `dff848928` (`to_timestamp_nanos`) |
| 28 | `#[expect(clippy::float_cmp)]`, `#[expect(clippy::struct_field_names)]` | restored / main | removed from S-origin tests and harness (`3b91313c2`); the six remaining are in main-authored journeys and `otlp_decode.rs`, as in main |
| 29 | `Box::pin` in the harness | main-lint | main's `large_futures` |
| 30 | capacity bench conversions | main-lint / X6 | inputs, thresholds, windows and output paths unchanged; benchmark qualification is TASK-006 |
| 31 | capacity files outside the P-028 path list | TASK-006 | benchmark qualification owns them |
| 32 | doc leftovers | fixed / main | `6f370a205` (audit chain wording, `pg_migration` Panics, `production_closeout` comment, result `run_id`); X3/X4/X6/X7 design prose `fcc3aceb1`; `IngestError::CardUnresolved` is main's (dead in main too); `20260910000018` whitespace is main's |

### Per-part ledgers (at `622299986`)

Each part's "Unmapped / suspicious" list is resolved by the table above.



#### Part: P-028 ledger, part A: `crates/vala/vala-sql`

Diff base: S = `cfe21dc9007d5d22d9ae4411be07fb5504b0cc05`. 47 files. Moved test files (`tests/*.rs` to `tests/integration/*.rs`) were diffed against their S originals.

Commit legend. Main-side commits that arrived through the merge: `6c23a6f45` (main: supply Postgres credentials externally, operator_session RLS role model), `515f16c97` (main: pedantic lints and test binary layout), `ce866229d` (main merge of the Forge rewrite), `d47c7fafb` (main: audit retention through Scribe), plus main audit-outbox commits `70a6f4a4e`, `d2af088b9`, `5a5542cbb`, `52e1144b5`, `e54b1244f`. Integration commits: `b356c948f` (merge), `4f9a5c1d1`, `eaa129c4a`, `516479cd5`, `87e9d3f48` (X10 plus S closeout port), `fb23a5d37`.

"Lint" in the table means an adaptation forced by main's workspace `[lints]` (pedantic Clippy): `r#"…"#` to `r"…"` (needless_raw_string_hashes), inlined format args, `Duration::from_mins`, `impl<'a>` to `'_`, items-before-statements `const` moves, `other @ Variant` arms (wildcard_enum_match_arm), `.unwrap()` to `.expect()`, `Path::extension` instead of `ends_with(".sql")`. "Fixture API" means main's `PgFixture::superuser_pool()` is now synchronous (`.await` dropped).

##### 1. Ledger

| File | Hunk summary | Exception / reason | Commit(s) |
|---|---|---|---|
| Cargo.toml | drop `sha2` dep (only used by the deleted migration-checksum pins) | Rev 3 Migrations: tests that pin shipped-migration bytes are deleted | 87e9d3f48, 515f16c97 |
| Cargo.toml | drop eight `[[test]]` targets (one consolidated `tests/integration` binary, main layout); `[lints] workspace = true` | main layout and lints (main's Cargo.toml is byte-identical here) | 515f16c97 |
| migrations/20260601000000_vala_init.sql | `GRANT SELECT ON vala._sqlx_migrations TO wyrd_app, wyrd_platform_admin` replaced by ENABLE/FORCE RLS plus an `operator_access` policy | X7 (main role/RLS convention) | 6c23a6f45 |
| migrations/20260619000000_iceberg_catalog.sql | drop GRANT/REVOKE on schema `iceberg_catalog`; comment rewritten (the platform login owns the schema) | X7 | 6c23a6f45 |
| migrations/20260619000001_olap_minimal.sql | `GRANT … TO wyrd_app` replaced by an `operator_access` policy | X7 | 6c23a6f45, 4f9a5c1d1 |
| migrations/20260801000002_vala_app_schema_usage.sql (deleted) | `GRANT USAGE ON SCHEMA vala TO wyrd_app` | X7 (main roles; the brief names this deletion) | 6c23a6f45 |
| migrations/20260801000003_grant_vala_schema_usage.sql (deleted) | same | X7 | 6c23a6f45 |
| migrations/20260802000000_vala_audit_staging.sql (deleted) | audit staging and chain head never created | X2 | 87e9d3f48 |
| migrations/20260821000000_add_drift_alerts.sql | GRANT replaced by an `operator_access` policy | X7 | 6c23a6f45 |
| migrations/20260906000001_maintenance_leases.sql (deleted) | `vala.maintenance_leases` never created | X10 | 87e9d3f48 |
| migrations/20260910000002_vala_file_list.sql | drop the `wyrd_platform_admin` existence DO-block and GRANTs; add an `operator_access` policy | X7 | 6c23a6f45 |
| migrations/20260910000003_vala_cluster_nodes.sql | comment: Forge coordinates through task rows, not `maintenance_leases` | X10 | 87e9d3f48 |
| migrations/20260910000003_vala_cluster_nodes.sql | drop the DO-block and GRANTs; "BYPASSRLS" comment rewritten | X7 | 6c23a6f45 |
| migrations/20260910000007_forge_lease_fence_assertion.sql (deleted) | `vala.assert_maintenance_lease_fence` never created | X10 | 87e9d3f48 |
| migrations/20260910000009_forge_operation_state.sql | DO-block and GRANTs replaced by an `operator_access` policy | X7 | 6c23a6f45 |
| migrations/20260910000010_forge_tasks.sql | strategy CHECK adds `'manifest_rewrite'` | X10 ("manifest rewrite included, runs as a task") | 87e9d3f48 |
| migrations/20260910000010_forge_tasks.sql | GRANTs replaced by `operator_access` policies; `forge_scheduler_state` gets RLS; the singleton INSERT moves after the policy | X7 | 6c23a6f45 |
| migrations/20260910000011_forge_planning_demands.sql | GRANTs replaced by an `operator_access` policy | X7 | 6c23a6f45 |
| migrations/20260910000012_forge_worker_claim_state.sql | GRANT replaced by RLS plus `operator_access`; the INSERT moves after the policy | X7 | 6c23a6f45 |
| migrations/20260910000013_oracle_coordination.sql | REVOKE/GRANTs replaced by an `operator_access` policy | X7 | 6c23a6f45 |
| migrations/20260910000015_scribe_publication_authority.sql | `assert_scribe_publication_fence`: sql to plpgsql with `app.operator` raise/restore; REVOKE/GRANT EXECUTE dropped | X7 (REQ-022 operator raise/restore convention) | 6c23a6f45 |
| migrations/20260910000018_forge_task_envelope_progress.sql | trailing blank line removed | whitespace only; no exception (see U-7) | 6c23a6f45 |
| migrations/20260910000021_scribe_batch_commits.sql | GRANT replaced by an `operator_access` policy | X7 | 6c23a6f45 |
| migrations/20260910000022_oracle_admission_blocks.sql | REVOKE/GRANTs replaced by two `operator_access` policies | X7 | 6c23a6f45 |
| migrations/20260910000025_oracle_reader_authority.sql | GRANT/REVOKEs replaced by `operator_access` policies; `ALTER FUNCTION … OWNER TO wyrd_platform_admin` and EXECUTE grants dropped; "request role" comment wording | X7 | 6c23a6f45 |
| migrations/20260910000025_oracle_reader_authority.sql | `forge_snapshot_expiration_claims` drops the `lease_key` and `lease_fencing_token` columns; comment now says "current claim on the task row" | X10 | 87e9d3f48 |
| migrations/20260910000026/27/29 (deleted) | ALTERs of `vala.audit_staging` (principal kinds, credential_id, chain-head grants) | X2 | 87e9d3f48 |
| migrations/20261009000000_bifrost_snapshot_retention.sql | REVOKE/GRANT EXECUTE on `oracle_read_table_cut` dropped; supersessions GRANT replaced by an `operator_access` policy | X7 | 4f9a5c1d1 |
| migrations/20261010000000_bifrost_cut_notify.sql | `REVOKE ALL ON FUNCTION notify_bifrost_cut() FROM PUBLIC` dropped | X7 | 4f9a5c1d1 |
| migrations/20261011000000_scribe_wal_fenced_lsn.sql | REVOKE/GRANT EXECUTE on `scribe_wal_newest_fence` dropped | X7 (see U-4) | 4f9a5c1d1, 87e9d3f48 |
| src/lib.rs | re-export `RowScope` instead of `SchemaAccess` (main renamed type) | X7 (main API) | 6c23a6f45 |
| src/lib.rs | `verify_schema` calls `verify_row_security(vala, RowScope::Tenant)` instead of schema-privilege checks; docs | X7 | 6c23a6f45 |
| src/lib.rs | delete the OLAP migration SHA-256 pin and the `GRANT … wyrd_platform_admin` content assertion; docs and `# Panics` | Rev 3 Migrations (byte pins deleted) plus X7 | 87e9d3f48 |
| src/lib.rs | `.sql` extension check via `Path::extension` (two places) | lint | 515f16c97 |
| src/postgres.rs | `verify_login_name(WYRD_APP_ROLE)` replaced by `verify_tenant_pool`; new `shared_login` field from `ResolvedDsns::shares_login`; docs | X7 (main identity/credential model) | 6c23a6f45, eaa129c4a |
| src/postgres.rs | `Duration::from_mins(5/30)` | lint (same values) | 515f16c97 |
| src/queries/mod.rs, src/row_types/mod.rs | drop the `audit_staging` and `maintenance_leases`/`maintenance` modules | X2, X10 | 70a6f4a4e, 87e9d3f48 |
| src/queries/audit_staging.rs, src/row_types/audit_staging.rs (deleted) | audit staging queries and rows | X2 | 70a6f4a4e |
| src/queries/maintenance_leases.rs, src/row_types/maintenance.rs (deleted) | lease acquire/renew/release/assert_fence and `MaintenanceLeaseKey` | X10 | 87e9d3f48 |
| src/queries/cluster_nodes.rs | raw-string hashes | lint | 515f16c97 |
| src/queries/file_list.rs | raw strings; test `superuser_pool()` without `.await` | lint; fixture API | 515f16c97 |
| src/queries/forge_catalog_operator.rs | doc: no "BYPASSRLS"; raw string | X7 wording; lint | 6c23a6f45 |
| src/queries/forge_leader.rs | comments: no "(BYPASSRLS)" | X7 wording | 6c23a6f45 |
| src/queries/olap_catalog.rs | doc: no "wyrd_app"; raw strings | X7 wording; lint | 6c23a6f45 |
| src/queries/scribe_batch_commits.rs | `newest_wal_fence` takes `&OperatorPool` instead of `&mut PgConnection`; `// tenant-isolation:` marker; doc | X7 (OperatorPool capability type; "WAL fences on OperatorPool") | eaa129c4a |
| src/queries/forge_operations.rs | delete `assert_lease_fence` and its three call sites; drop `lease_key`/`lease_fencing_token` from claim INSERT/SELECTs; lock-order and `# Errors` docs say the claim is the fence | X10 | 87e9d3f48 |
| src/queries/forge_operations.rs | raw strings, `impl ForgeOperations<'_>`, inlined `format!` args | lint | 515f16c97 |
| src/row_types/forge_operations.rs | `ForgeExpirationAuthority` and the claim SQL row lose `lease_key`/`lease_fencing_token` | X10 | 87e9d3f48 |
| src/queries/forge_tasks.rs | module doc: the active task row is the table's exclusive authority | X10 | 87e9d3f48 |
| src/queries/forge_tasks.rs | new `ForgeTasks::lock_live_claim` (unexpired-claim `FOR UPDATE`) | X10 (`begin_fenced` locks the live claim first, 3278de536) | 87e9d3f48 |
| src/queries/forge_tasks.rs | `claim_expires_at > statement_timestamp()` added to the Prepared transition and to `apply_transition` | X10 ("every durable transition … fences on it") | 87e9d3f48 |
| src/queries/forge_tasks.rs | successful continuation reads `strategy`; `manifest_rewrite` settles Running to Succeeded | X10 (manifest rewrite as a task) | 87e9d3f48 |
| src/queries/forge_tasks.rs | `assert_lease_fence` calls removed from expired-cleanup prepare/settle and orphan checkpoint/complete; lock-order docs | X10 | 87e9d3f48 |
| src/queries/forge_tasks.rs | **`reclaim_expired` (S public fn) deleted**; its doc text folded into `reclaim_expired_attempts` | not named by X10 (see U-1) | 87e9d3f48 |
| src/queries/forge_tasks.rs | raw strings; `values.push(value);` | lint | 515f16c97 |
| src/row_types/forge_tasks.rs | `ForgeTaskStrategy::ManifestRewrite` (enum, `MAINTENANCE_STRATEGIES`, `is_maintenance`, `as_str`, `FromStr`); docs say "lapsed claim" | X10 | 87e9d3f48 |
| src/row_types/forge_tasks.rs | `const` items moved before statements (4 places) | lint | 515f16c97 |
| tests/integration/main.rs (new) | one consolidated integration binary declaring the 10 surviving modules | main layout (main's file has the same shape, plus `pg_maintenance_leases`, deleted under X10) | 515f16c97, 87e9d3f48 |
| tests/pg_audit_staging.rs (deleted) | | X2 | 515f16c97 |
| tests/pg_maintenance_leases.rs (deleted) | | X10 | 87e9d3f48 |
| tests/integration/pg_file_list_cluster_nodes.rs | `.expect`, inlined args, raw strings, fixture API; "BYPASSRLS" comments become "operator_access" | lint; fixture API; X7 wording | 6c23a6f45, 515f16c97 |
| tests/integration/pg_forge_file_list.rs, pg_forge_leader.rs, pg_olap_catalog.rs | path move plus fixture API/raw-string only | main layout; lint | 515f16c97 |
| tests/integration/pg_schema_usage.rs | test renamed `tenant_login_registers_bifrost_table`; docs and expect message reworded from `wyrd_app` to the tenant login | X7 | 6c23a6f45 |
| tests/integration/pg_oracle_membership.rs | `grants()` helper deleted; function-owner check becomes `proowner = nspowner`; `has_function_privilege('wyrd_tenant')`; PUBLIC-execute and `forge_snapshot_supersessions` grant assertions deleted | X7 (see U-3) | 6c23a6f45, eaa129c4a |
| tests/integration/pg_migration.rs | `ResolvedDsns::from_env` platform DSN instead of `WYRD_TEST_DATABASE_ADMIN_URL`; `include_str!` paths `../../` | X7 (main credential env); layout | 6c23a6f45, 515f16c97 |
| tests/integration/pg_migration.rs | delete `shipped_audit_staging_migration_is_immutable` and `staged_rows_survive_the_credential_attribution_upgrade` | X2 plus Rev 3 Migrations (byte pins deleted) | 87e9d3f48 |
| tests/integration/pg_migration.rs | `# Panics` doc: "a retired relation or function still resolves" | leftover main doc (see U-6) | 6c23a6f45/d47c7fafb |
| tests/integration/pg_stream_identity.rs | new `newest_wal_fence_spans_tenants_on_one_stream` (cross-tenant OperatorPool fence lookup); raw strings | X7 (proof for the OperatorPool fence read; "Rehome the cross-tenant WAL fence assertion") | 516479cd5, 515f16c97 |
| tests/integration/pg_forge_operations.rs | `count_audit` helper and ~14 "Forge transitions append no audit" assertions deleted; docs | X2 (`vala.audit_staging` is gone) | 87e9d3f48, d47c7fafb |
| tests/integration/pg_forge_operations.rs | policy assertion expects `operator_access` plus `tenant_isolation`; `forge_operation_state` grant assertion deleted | X7 | 6c23a6f45 |
| tests/integration/pg_forge_operations.rs | claim-table grant assertion ("claims are immutable: no role holds UPDATE") deleted | X7 (see U-2) | 6c23a6f45 |
| tests/integration/pg_forge_operations.rs | lease seeding, `lease_key`/`lease_fencing_token` fields and columns, and the "lease key disagreement"/"lease fence disagreement" cases deleted | X10 | 87e9d3f48 |
| tests/integration/pg_forge_operations.rs | `other @ ForgeOperationTransition::AlreadyApplied` arms; setup moved below helper items; raw strings | lint | 515f16c97 |
| tests/integration/pg_forge_tasks.rs | audit_staging counts and assertions deleted (reclaim, rollback, settlement, refused enqueue) | X2 | d47c7fafb, 87e9d3f48 |
| tests/integration/pg_forge_tasks.rs | `operator_cannot_read_or_mutate_tenant_audit_rows` deleted | X2 | d47c7fafb |
| tests/integration/pg_forge_tasks.rs | `forge_tasks` grant assertions deleted ("tenant role cannot bypass operator-owned enqueue") | X7 (see U-2) | 6c23a6f45 |
| tests/integration/pg_forge_tasks.rs | `reclaim_expired(10)` calls become `reclaim_expired_attempts(10).len()` | follows U-1 | 87e9d3f48 |
| tests/integration/pg_forge_tasks.rs | lease seeding and the stale-fence case deleted; `seed_cleanup_roots` signature narrowed; new lapsed-claim refusal for `checkpoint_orphan_cleanup_cursor` | X10 | 87e9d3f48, fb23a5d37 |

P-028 item 3 (migrations):
- No migration was added, whether forward, drop, or main-only. HEAD's migration set equals S's minus 8 files.
- Main's `20261003000200_audit_publication_progress.sql` and `20261008000000_drop_audit_staging.sql` are absent.
- Deletions: X2 (4 files: `20260802000000`, `…026`, `…027`, `…029`), X10 (2 files: `20260906000001`, `20260910000007`), X7 (2 files: `20260801000002`, `20260801000003`).
- In-place edits:
  - X10: `manifest_rewrite` in the strategy CHECK, the claim columns dropped, and one comment.
  - X7, in 19 files: every GRANT/REVOKE/DO-block is replaced by `operator_access` RLS policies, and `assert_scribe_publication_fence` gains the `app.operator` raise/restore.
  - One whitespace hunk.
- There are no X5 edits. `vala.dev.agent_traces` was never a Postgres object; the `vala.dev` namespace CHECK values are S's, unchanged.
- **Item 3's wording ("only the in-place X2 edits … and X5 edits") is incomplete.** The diff also holds X7 (19 files, 2 deletions) and X10 (3 files, 2 deletions) edits. Fix the P-028 text so it lists X2, X5, X7 and X10, or this criterion cannot pass as written.

##### 2. Unmapped / suspicious

- **U-1. `src/queries/forge_tasks.rs` ~L860 (S L860–872): S public API `ForgeTasks::reclaim_expired` deleted.**
  - Excerpt: `- pub async fn reclaim_expired(&self, cap: u32) -> Result<u64, SqlError> { let changed=sqlx::query("WITH victims AS (… state IN ('claimed','running') AND claim_expires_at<statement_timestamp() …`
  - Why unmapped: X10 deletes `maintenance_leases`, the lease-fence function and the separate lease. It does not delete the expired-claim reclaim. In S, nothing in production called it (only `tests/integration/forge/orphan_cleanup.rs` and `pg_forge_tasks.rs`), and `reclaim_expired_attempts` covers the behaviour. So this is dead-code cleanup of an S symbol, not exception-forced. S tests were rewritten to the sibling.
  - Stale reference left behind: `crates/vala/vala-bifrost-redux/src/forge/worker.rs:3652` still says "retained for `reclaim_expired` after its lease TTL" (outside part A).
- **U-2. Privilege-boundary assertions in tests deleted with no replacement property.**
  - `tests/integration/pg_forge_operations.rs` (S ~L1908): `- "claims are immutable: no role holds UPDATE"`
  - `tests/integration/pg_forge_tasks.rs` (S ~L1395): `- "tenant role cannot bypass operator-owned enqueue"`
  - Why flagged: X7 removes the `wyrd_app`/`wyrd_platform_admin` GRANT model, so these exact assertions cannot stand. But the properties they protected now have no enforcement or test:
    - Claim rows are still described as immutable (migration 25 comment), yet nothing refuses an UPDATE.
    - The tenant login, which gets default table DML per `pg_schema_usage`, can INSERT `forge_tasks` under `tenant_isolation`.
  - Mapped to X7 in the ledger, but a reviewer should confirm this loss is accepted under main's convention rather than an unrecorded weakening.
- **U-3. `tests/integration/pg_oracle_membership.rs` (S ~L…, `public_exec`): assertion "PUBLIC holds no execute privilege" deleted**, together with every `REVOKE ALL ON FUNCTION … FROM PUBLIC` in migrations 25, 20261009, 20261010 and 20261011.
  - Effect: the SECURITY DEFINER functions are now PUBLIC-executable: `oracle_catalog_metadata_location`, `oracle_acquire_table_cut`, `oracle_read_table_cut`, `scribe_wal_newest_fence` and `assert_scribe_publication_fence`.
  - Main's `wyrd-sql` migrations contain no `REVOKE … ON FUNCTION`, so this follows main's convention (X7).
  - Still, a weakened security assertion; list it for review. `scribe_wal_newest_fence` in particular returns cross-tenant WAL positions to any caller.
- **U-4. `migrations/20261011000000_scribe_wal_fenced_lsn.sql`: `scribe_wal_newest_fence` stays `LANGUAGE sql` with no `app.operator` raise/restore.**
  - REQ-022 says this exact function "follows main's operator raise/restore and bounded search_path/privilege conventions".
  - It works only because the caller now runs on `OperatorPool` (`eaa129c4a`). Under FORCE RLS, a tenant-session caller would see only its own tenant's fences.
  - The hunk itself (grant removal) maps to X7. The REQ-022 shape is not met; record as a deviation or fix.
- **U-5. `src/queries/forge_operations.rs` apply/reset paths: the `AlreadyApplied` idempotent return now happens before any fence check.**
  - Before, `assert_lease_fence` ran first. Now `lock_expiration_task` runs only after the AlreadyApplied branch.
  - Consistent with X10 (a settled claim has no live fence to check), so mapped. Noted because replay-without-fence is a behaviour change.
- **U-6. `tests/integration/pg_migration.rs:17–18`: stale `# Panics` doc.**
  - Excerpt: `+ /// Panics when … a retired relation or function still resolves after migrating.`
  - Main `d47c7fafb` added this together with an `audit_staging`/`audit_chain_head`/`audit_publication` absence check. `87e9d3f48` deleted the check but kept main's doc wording; the body checks only `vala.drift_alerts`.
  - A leftover main doc hunk. Revert to S's "or `vala.drift_alerts` still resolves", or restore an absence check.
- **U-7. `migrations/20260910000018_forge_task_envelope_progress.sql` EOF: trailing blank line removed (`-` empty line).**
  - Whitespace-only edit to an S migration. No exception covers it (probably a `git diff --check` fix in main `6c23a6f45`).
  - Harmless, but item 3 says only exception edits.
- **U-8. P-028 item 3 text mismatch (process finding).** The migration diff contains X7 and X10 in-place edits and deletions, which item 3 does not list (see section 1). The criterion as written fails, even though every content hunk except U-7 maps to an exception.

No `#[allow]`/`#[expect]`/`cfg_attr`, `Box::pin`, timeout/deadline changes, status-code changes, compatibility shims, or leftover main Forge tests were found in vala-sql. The `vala_pool_config` durations are unchanged in value.

##### 3. Counts

- Ledger rows: 78.
- Mapped: 72 (X7 34, X10 21, X2 10, lint/layout/fixture-API main-forced 17; some rows carry two labels).
- Unmapped: 3 hunks (U-1 `reclaim_expired` deletion, U-6 stale doc, U-7 whitespace).
- Suspicious but mapped: 4 (U-2, U-3, U-4, U-5).
- Process finding: 1 (U-8, item 3 wording).

#### Part: P-028 ledger — Part B: Forge (X10 area)

Scope: `crates/vala/vala-bifrost-redux/src/forge/*`, `crates/vala/vala-bifrost-redux/tests/integration/forge/*`,
`crates/wyrd/wyrd-testing/tests/bifrost/forge/*`, `crates/wyrd/wyrd-testing/src/bifrost/forge_harness.rs`.
Diff base S = `cfe21dc9007d5d22d9ae4411be07fb5504b0cc05`. 28 files, 253 `@@` hunks (worker.rs alone 87).

Struck-commit check: `expiry_gates.rs`, `leader.rs` and `scheduler.rs` are byte-identical to S, so 985aa12f8 (Forge gate part) and the expiry_gates part of 136e460d6 are reverted. `rg 'PreparedExpiry|Unsubmitted|ForgeLoopQuiescence|maintenance_lease|forge_lease_key|lease_fencing_token'` over the scope prints nothing. Main Forge commits that show up in path history (3bf7b0d97 fast-reclaim, 3af5bdb5b quiescence, 6aaccebf9, 5ff25deb6 retry-count commits, a26fda2e3, 0e1968570) leave no hunk against S. The merge resolved them to S.

##### 1. Ledger

| File | Hunk summary | Exception / reason | Commit(s) |
|---|---|---|---|
| src/forge/lease.rs | `ForgeLease` rebound from the `maintenance_leases` row (lease_key, fencing_token, takeover, acquire/release/assert_transaction_fence) to a task claim (`tasks, task_id, attempt_id, owner`). Adds `confirm`, `renew` → `ForgeTasks::heartbeat`, `require_fence`, `begin_fenced` → `FencedTransaction` (locks the live claim first), and `fence_lost`. Deletes `forge_lease_key` and its unit test. Adds a custom `Debug`. | X10 | 87e9d3f48, ce8cac7d4, 3278de536 |
| src/forge/error.rs | `ForgeError::Lease` variant removed, along with its test assertion and failure-class arm. `FenceLost` doc reworded to name the claim. | X10 | 87e9d3f48 |
| src/forge/mod.rs | Re-export drops `forge_lease_key`. `operator_pool` doc says "task claims". | X10 | 87e9d3f48 |
| src/forge/metrics.rs, phase.rs | `ForgeTaskStrategy::ManifestRewrite` added to `TASK_TYPES` (5→6) and to `admits_new_effect`, with a test line. | X10 ("manifest rewrite included, runs as a task") | 87e9d3f48 |
| src/forge/compact.rs | S test `forge_transition_writers_do_not_append_audit_directly` deleted. Its source contains the literal `audit_staging::append_`, which the P-028 struck-symbol grep forbids, and the guarded call no longer exists. | X2 (audit staging removed) | 70a6f4a4e via merge b356c948f |
| src/forge/live_reconcile.rs, live_replace.rs, publication.rs, scribe_promotion.rs | `lease.renew(&operator_pool)` / `require_fence(&operator_pool)` → pool-free calls. `ForgeError::FenceLost{lease_key}` → `lease.fence_lost()`. `tenant_conn` + `assert_transaction_fence` + commit → `lease.begin_fenced(...)` + `commit()`. | X10 | 87e9d3f48, ce8cac7d4, 3278de536, fb23a5d37 |
| src/forge/expire.rs | `settled_evidence` → `terminal: Option<ExpiryTerminal>`. New `ExpiryTerminal{Settle,Reset}`, `ExpirySettlement`, `ExpiryReset`. `reset_expiration`/`settle_expiration` now decide without committing. New `commit_expiry_terminal` (the single commit point). Test fixture uses `ForgeLease::confirm` and commits the decided terminal. `expiration_authority` loses lease_key/token. 20 hunks. | X10 (strategy returns ForgeTerminal; settlement seam commits) | 87e9d3f48, 531ed0d26, fb23a5d37 |
| src/forge/gc.rs | Manifest rewrite no longer takes `ForgeLease::acquire` on a table lease. It builds a `ManifestRewrite` `NewForgeTask` (`manifest_rewrite_task`, `MANIFEST_REWRITE_PARAMETER_KIND`) and runs `executor.execute_accepted`. `commit_manifest_rewrite` takes paths and is `pub(super)`. `orphan_cleanup_task` is `pub(super)`. Module doc updated. | X10 | 87e9d3f48, fb23a5d37 |
| src/forge/orphan_gc.rs | Test entry `run_orphan_gc_report_for_test` admits through `insert_claimed` + `start` + `confirm`, then settles via `retry` or `complete_orphan_cleanup` (previously lease acquire/release). Fence calls rebound. Orphan transition opens through `begin_fenced`. | X10 | 87e9d3f48, ce8cac7d4, 3278de536 |
| src/forge/worker.rs (claim/lease) | `acquire_table_lease` → `confirm_claim`. `release_table_lease`, `release_prepared_lease` and the release step in `settle_claim_execution`/`release_unresolved_attempt` deleted. `ClaimExecutionOutcome.lease` removed. `spawn_authority_heartbeat` renews only through `lease.renew()` (the double heartbeat+lease renew is gone) and is now infallible. `begin_attempt` drops its extra heartbeat (the caller's `require_fence` renews). | X10 | 87e9d3f48, ce8cac7d4, 3278de536 |
| src/forge/worker.rs (terminal seam) | `SelfSettled`/`Settled(result)` → `ForgeDispatchResult::Terminal(ForgeTerminal)` / `ForgeExecutionEvidenceState::Terminal`. New `ForgeTerminal{NoOpAcknowledged, ManifestRewritten, OrphanPassBounded, OrphanScanComplete, Expiry}`. `RewriteAdmission::Compact(ForgeTerminal)` replaces `acknowledge_compact_table`. Orphan dispatch returns a decision. New `commit_terminal`. `finish_claim_execution` returns `ForgeTaskResult` and is the single release point after the heartbeat joins (`??`). | X10 (single release point) | 531ed0d26 |
| src/forge/worker.rs (manifest rewrite) | `dispatch_manifest_rewrite`, the `ManifestRewrite` stage/validation arm, `tolerates_base_drift` arm, and `ForgeExecutionStage::ManifestRewrite`. | X10 | 87e9d3f48 |
| src/forge/worker.rs (test seams) | Deleted: `fail_next_lease_release`, `hold_before_next_lease_release_for_test`/wait/release, `heartbeat_beat_now`/`beat_claim_heartbeat_now_for_test`. Renamed: `hold_after_next_durable_settlement_for_test` → `…terminal_decision…`. **New:** `fail_next_failure_settlement` (+ armed/take) injected in failure settlement. | X10 (deleted seams' sites no longer exist; new seam replaces the lease-release fatal path) — see U9 | 87e9d3f48, 531ed0d26, ca978e169 |
| src/forge/worker.rs (unit test) | `SelfSettled(Succeeded)` → `Terminal(ManifestRewritten)` in the publication-priority test | X10 | 531ed0d26 |
| tests/integration/forge/support.rs | `expire_table_lease` → `lapse_table_claim` (ages `forge_tasks.claim_expires_at`). `live_leases` → `live_claims`. | X10 | 87e9d3f48 |
| tests/integration/forge/support.rs | `superuser_pool().await` → `superuser_pool()` (3×) | Main-forced: main `wyrd-dev-fixtures::pg::PgFixture::superuser_pool` is sync (S had it async) | b356c948f / cac9c4969 |
| tests/integration/forge/support.rs | `CreateTableRequest { audit: None }` field removed | X2 (catalog no longer carries an `AuditEvent`; audit on the outbox) | d2af088b9 via merge |
| tests/integration/forge/compaction_admission.rs | `expire_table_lease` → `lapse_table_claim`. `live_leases` → `live_claims` (crash: 1, drained: 0). `release_one_unresolvable_attempt` loses the held lease-release boundary; readiness is still asserted afterwards in `assert_unready_takes_no_new_authority` (l.1633). | X10 | 87e9d3f48, 27f65d206 |
| tests/integration/forge/compaction_admission.rs:2281 | `acceptance_unknown_recovers_from_durable_state`: final `live_leases()==0` assertion deleted, no claim replacement | X10 (partial) — see U1 | 87e9d3f48 |
| tests/integration/forge/publication.rs | `live_leases`→`live_claims`. A drained attempt now keeps 1 claim (previously 0 leases). `HeldAuthorityMutation::expected_state` makes LeaseExpired close `cancelled` instead of `failed`. | X10 — see U11 | 27f65d206 |
| tests/integration/forge/promotion.rs | `live_claims` rename. Barrier seed stops inserting a `maintenance_leases` row and drops lease_key/token. | X10 | 87e9d3f48 |
| tests/integration/forge/expired_cleanup.rs | `live_leases` helper deleted. `assert_prepared_candidate_excludes_competitors` loses its orphan-GC `FenceLost` arm. The cross-table refusal loses its leases-unchanged check. | X10 — see U2 | 87e9d3f48 |
| tests/integration/forge/orphan_cleanup.rs | Lease-count check removed from `cross_table_plan_refuses_before_lease_or_io`. `assert_lease_and_fence_refuse_deletion` → `assert_claim_loss_refuses_deletion` (peer-lease refusal case deleted; fence bump → `expire_claims`). `reclaim_expired` → `reclaim_expired_attempts().len()`. | X10 — see U3; the `reclaim_expired` rename is forced by its deletion in vala-sql (outside this part) | 87e9d3f48, 6b13254ad |
| tests/integration/forge/orphan_cleanup.rs | `superuser_pool().await` → sync | Main-forced (wyrd-dev-fixtures API) | b356c948f |
| tests/integration/forge/production_routes.rs | S test `worker_duration_includes_claim_and_failed_release` deleted. `worker_prepared_fatal_closes_before_release_and_observation` → `…_before_observation`, with lease-release hold/fault and the active-task gauge check at that boundary removed. | X10 — see U4 | 87e9d3f48 |
| tests/integration/forge/snapshot_expiration.rs | 3× `UPDATE vala.maintenance_leases` removed. `settled_expiration_outranks{Shutdown,ObsoleteHeartbeat}` → `decided_expiration_commits_through_shutdown` (the heartbeat case's seam is gone). | X10 (Q16 coverage) | 87e9d3f48, 531ed0d26 |
| src/bifrost/forge_harness.rs | `CommitUncertaintyControls.target_table` + `targets()` + `CommitUncertaintyCatalog::target_table`, so the tenant `audit_log` commits pass through | X2 (audit_log is a maintained Bifrost table) | f6c841d57 via merge |
| src/bifrost/forge_harness.rs | `audit: None` removed from `CreateTableRequest` | X2 | d2af088b9 via merge |
| src/bifrost/forge_harness.rs | Seven `# Panics` doc sections, `from_secs(60)`→`from_mins(1)`, `map().unwrap_or(false)`→`is_ok_and` | Main-forced lint/rustdoc rules (pedantic clippy, AGENTS §16) | 515f16c97 via merge |
| tests/bifrost/forge/live_rewrite.rs | `uncertainty.target_table(shared.binding.table_ident())` | X2 | f6c841d57 |
| tests/bifrost/forge/live_rewrite.rs | `failed_worker_restarts_while_the_api_serves`: tenant `audit_log` opted out of compaction | Q14 | 87e9d3f48 |
| tests/bifrost/forge/live_rewrite.rs | Same journey: fault moved from `fail_next_lease_release` to `fail_next_forge_prepared_audit_for_test` + `fail_next_failure_settlement`, then `expire_running_claims` before release | X10-forced seam move — see U9 | ca978e169, db50b1c76 |
| tests/bifrost/forge/live_rewrite.rs | `RELEASE_BOUND` 120 s → `from_mins(2)` (same value). Imports. Closure → `BifrostRoleResources::forge` path. | Main-forced lints | 515f16c97 |
| tests/bifrost/forge/live_rewrite.rs:2477 | `#[expect(clippy::float_cmp, …)]` added | — see U6 | 515f16c97 |
| tests/bifrost/forge/production_closeout.rs | `compact()` judges on state (non-empty, all `/data/forge/`). Adds `promoted_files`, `load_table`, `snapshot_files`. `snapshot_count` → `rewrite_snapshot_count` (Replace only). `drain_tasks()` before the clock jump in the geometry and ReaderCleanup journeys. | Q15, Q17 | a21f335ac |
| tests/bifrost/forge/production_closeout.rs | `assert_rewrite_evidence(binding)` filters `forge_operation_state` by `resource` | X2 (audit_log rewrites settle beside) | 2880705a0 via merge |
| tests/bifrost/forge/production_closeout.rs | `LeaderJourney::failures()` filters `ForgeError::Shutdown`. `key_in`. Successor/restart/empty-maintenance schedule-size assertions loosened to tolerate `audit_log` membership. | X2 (audit_log maintained) — see U7 | f6c841d57 via merge |
| tests/bifrost/forge/production_closeout.rs | `audit: None` removed | X2 | d2af088b9 |
| tests/bifrost/forge/production_closeout.rs | `REWRITE_BOUND` 600 s → `from_mins(10)` (same value). `scribe_roles` local. `.map(WyrdTestServer::node_id)`. | Main-forced lints | 515f16c97 |
| tests/bifrost/forge/production_closeout.rs:213 | Comment "The one worker drains only the journey's tables…" with no code behind it. S's "Audited reads above keep publishing retained audit…" comment removed. | — see U8 | 2d25fac7c via merge |
| tests/bifrost/forge/production_closeout.rs:1262 | `#[expect(clippy::float_cmp, …)]` added | — see U6 | 515f16c97 |
| tests/bifrost/forge/scribe_promotion.rs | `steal_forge_lease`/`release_forge_lease`/`live_forge_leases` → `lapse_forge_claim`. Test renamed `…claim_loss_and_takeover_settle_once`. Cancellation test loses its lease assertion. | X10 — see U5 | 87e9d3f48, 6b13254ad |
| tests/bifrost/forge/scribe_promotion.rs | `let` moved after nested `fn` items (items_after_statements). `from_secs(60)`→`from_mins(1)`. | Main-forced lints | 515f16c97, 5e7b177b0 |
| tests/bifrost/forge/public_support.rs | `audit: None` removed. `params: Vec::new()` added to `BifrostQueryRequest`. | X2; X6 | d2af088b9, 82972f9f9 |
| tests/bifrost/forge/support.rs | `from_secs(3600)`→`from_hours(1)` | Main-forced lint | 515f16c97 |

##### 2. Unmapped / suspicious

The S defect was two Forge table authorities, and every production-code hunk here maps to the X10 rebinding of that defect. The items below are test-side weakenings, recording gaps, and main leftovers. Nothing below restores a struck main mechanism.

- **U1 — compaction_admission.rs:2281 (`acceptance_unknown_recovers_from_durable_state`), end of test.** Removed with no replacement:
  ```
  -    assert_eq!(promoted.fixture.live_leases().await, 0, "every settled attempt released its table fence");
  ```
  Every other `live_leases` site was renamed to `live_claims`. This one was deleted, so the "every settled attempt released its authority" property is no longer asserted in this test. X10 makes the claim the authority. It does not remove the property. Weakened assertion.

- **U2 — expired_cleanup.rs:262 and :415.** Two checks removed with no claim equivalent: the competing destructive effect check (`run_orphan_gc_report_for_test` → `Err(FenceLost)`) and the cross-table refusal's "acquires no table lease" check (`live_leases` before/after). The orphan test entry still returns `FenceLost` when a table has an active task (orphan_gc.rs `insert_claimed … ok_or_else FenceLost`), so the first check could have stayed. The second could have become a `live_claims` / active-task count. The new doc ("refusing a second claim … refuses every competitor") replaces a test with an argument. Weakened coverage. X10 does not require it.

- **U3 — orphan_cleanup.rs:~766 (`cross_table_plan_refuses_before_lease_or_io`).** Lease-count assertion deleted (`assert_eq!(leases, 0, "the refusal precedes table fence acquisition")`). The test name still promises "before_lease", but nothing checks claim/authority acquisition now. In the same file, the peer-lease-refusal half of `assert_lease_and_fence_refuse_deletion` was deleted. Under X10 that case is a refused claim, and claim refusal is covered in vala-sql pg tests. Stale name plus a dropped assertion.

- **U4 — production_routes.rs:1142.** S test `worker_duration_includes_claim_and_failed_release` deleted entirely (−112 lines). Its release-failure half is gone with X10. Its claim-held/settlement-interval duration half has no direct replacement. Only `worker_prepared_recovery_observes_one_ownership_episode` still exercises `assert_completed_episode`. Also in `worker_prepared_fatal_closes_before_observation`, `assert!((active_tasks(..) - 1.0).abs() < EPSILON)` at the boundary was removed. S test removed without a replacement for the non-lease part.

- **U5 — wyrd-testing tests/bifrost/forge/scribe_promotion.rs:734 (`scribe_promotion_cancellation_drains_without_settlement`).** `live_forge_leases == 0` removed and no claim assertion added. The doc now says the claim is "left to lapse for a successor". The redux counterpart `tests/integration/forge/promotion.rs` (`scribe_promotion_integration_cancellation_drains_without_settlement`) instead asserts `live_claims() == 0, "the drained attempt released its claim"`. The two tests disagree about the same drain, and the journey no longer asserts either. Weakened, and internally inconsistent.

- **U6 — `#[expect(clippy::float_cmp, reason = …)]` added**:
  - `tests/bifrost/forge/live_rewrite.rs:2477` (`failed_worker_restarts_while_the_api_serves`)
  - `tests/bifrost/forge/production_closeout.rs:1262` (`compaction_geometry_exact_rows_and_non_destructive_second_pass`)

  Both come from main 515f16c97 (pedantic lint closeout). S has no such attribute. AGENTS §12 forbids adding `#[allow]`/`#[expect]` to pass a gate unless a sanctioned mechanism applies. No exception covers this; the fix is to compare via `to_bits()` or integer counters (the capacity bin already does `to_bits`).

- **U7 — production_closeout.rs:2418 `LeaderJourney::failures()` plus the schedule assertions at ~2508, ~2856, ~2957.** S asserted `observer.returned_errors().is_empty()` in three journeys. HEAD filters out every `ForgeError::Shutdown` string first. S's exact `sizes_for_test() == (0,0,0)` / `(1,1,1)` checks became `(audit, audit, 0)` / `(1+audit, …)` or per-key membership checks. These came from main commit f6c841d57 (TASK-003 r4 era), not from a TASK-002 Q row. X2 plausibly motivates them, since audit_log is now maintained, but they weaken S assertions: any Shutdown error, not only one from the stopping node's audit attempt, is now silently tolerated. No Q row records a diagnosis.

- **U8 — production_closeout.rs:213–214, stale main prose.**
  ```
  +        // The one worker drains only the journey's tables, so the tenant's audit
  +        // table cannot take a slot a geometry or competing-table plan is owed.
  ```
  Main 2d25fac7c paired this comment with `.without_audit_publication_for_test()`. That call is gone, so the comment describes behaviour that does not exist and contradicts `assert_rewrite_evidence`'s new doc ("the tenant's audit table is maintained alongside it"). The same main commit also removed S's comment "Audited reads above keep publishing retained audit, so the coordinator can plan and start audit-log maintenance after the last drain", which is true again under X2. This is main residue.

- **U9 — live_rewrite.rs `failed_worker_restarts_while_the_api_serves` and worker.rs `fail_next_failure_settlement`.** The new production-crate test-support seam (`fail_next_failure_settlement`, `failure_settlement_failure_armed`, `take_failure_settlement_failure`, plus the injection in failure settlement) is neither S nor main. The journey now also arms `fail_next_forge_prepared_audit_for_test` (S seam) and calls `expire_running_claims` to SQL-lapse the unsettled claim before the restart is released ("proves the recovery, not the wait"). Under S the lease-release fault fired after durable settlement, so the rebuilt worker recovered with no manual claim aging. Under HEAD the failing task stays `running` and the test ages it by SQL. This is a real change in what the journey proves. It is X10-forced (the lease-release site no longer exists), but Q14 records only the `audit_log` opt-out and does not mention ca978e169/db50b1c76 or the new seam. Recording gap.

- **U10 — worker.rs, snapshot-expiry dispatch and `complete_snapshot_expiry`.** Two S fence checks removed outright rather than rebound:
  ```
  -        lease.require_fence(&self.forge.core.operator_pool).await?;
           let table = self.forge.load_table(&binding.table_ident()).await?;      // dispatch_snapshot_expiry tail
  ...
           require_running(stop)?;
  -        lease.require_fence(&self.forge.core.operator_pool).await?;             // complete_snapshot_expiry head
  ```
  Every durable write still opens through `begin_fenced`, and `finish_claim_execution` renews once, so no transition goes unfenced. Still, these deletions go beyond mechanical rebinding. Same for `begin_attempt` dropping its post-`start` heartbeat (the following `require_fence` renews). X10 text ("every … pre-commit check fences on it") does not call for dropping checks. Low severity; confirm intent.

- **U11 — publication.rs `HeldAuthorityMutation::expected_state`.** A claim lapsed mid-commit now closes the row `cancelled` (via the loop's expired-claim reclaim) instead of S's classified `failed`. This X10 consequence changes observable task outcome and retry accounting, since a reclaimed dispatch is `cancelled`. Neither spec X10 nor Q16 states it. Mapped to X10, but the behaviour change should be recorded.

Outside this part (noted only): orphan_cleanup.rs moved from `ForgeTasks::reclaim_expired` to `reclaim_expired_attempts` because HEAD deleted `reclaim_expired` from vala-sql `forge_tasks.rs`. That deletion belongs to the vala-sql part of the ledger.

##### 3. Counts

- Files: 28. Hunks (`@@`): 253.
- Mapped: about 241 hunks. X10: ~205, including all 87 in worker.rs. X2: ~12. X6: 1. Q14/Q15/Q17: ~8. Main-forced lint/rustdoc/fixture-API: ~15.
- Unmapped/suspicious: 11 items (U1–U11) over ~12 hunks.
  - Hard findings: U6 (`#[expect]` ×2) and U8 (stale main comment).
  - Weakened or removed S assertions: U1, U2, U3, U4, U5, U7.
  - Recording gaps for X10-forced changes: U9, U10, U11.
- No struck main Forge mechanism (SQL, leadership, reclaim, expiry, publication-retry, operation-state) is present in scope.

#### Part: P-028 ledger — part C1 (redux oracle/*, resources.rs, cluster/mod.rs, catalog/*, contracts.rs, namespaces.rs)

Diff base: `git diff cfe21dc9007d5d22d9ae4411be07fb5504b0cc05 -- <paths>` (2886 diff lines, 13 files). Main comparison against `main` HEAD; X8 source c63e2ff46 / 087cc0416.

##### 1. Ledger

| File | Hunk summary | Exception / reason | Commit(s) |
|---|---|---|---|
| catalog/bifrost_catalog.rs | Drop `audit: Option<AuditEvent>` from `CreateTableRequest`, `register_dataset` arg, built-in request; delete `append_registration_audit` (audit_staging append in catalog txn) and its two call sites; doc `# Errors` lose "audit" | X2 (no audit staging; registration audit staged on outbox by caller) | 70a6f4a4e, d2af088b9 (main) via b356c948f/597533616 |
| catalog/bifrost_catalog.rs | `use vala_sql::{TenantConn, ValaPostgres}` -> `ValaPostgres` only | X2 fallout (TenantConn only used by deleted append) | 597533616 |
| catalog/bifrost_catalog.rs | `ensure_builtin` doc: built-ins eager (provision + boot), ingest/describe never create; `describe` no longer calls `ensure_builtin` | X5 (eager provisioning) | 1090580f2 (main) |
| catalog/bifrost_catalog.rs | `create_table_locked` doc names `ReservedColumn` (variant exists in S) | doc only; X5/X2 doc sweep | 597533616 |
| catalog/bifrost_catalog.rs | Test `physical_schema_rejects_reordered_columns`: `seq`/`entry_hash` -> `attempt`/`operation` | X2 (no chain column names) | 4056d79e6 (main) |
| catalog/bifrost_catalog.rs | Test docs/panic text "lazy" -> "eager"; new test `every_builtin_is_created_through_ensure_builtin` | X5 | 1090580f2 (main) |
| catalog/bifrost_catalog.rs | Tests drop trailing `None` audit arg (x3) | X2 fallout | 597533616/cac9c4969 |
| catalog/error.rs | Remove `BifrostCatalogError::AuditUnavailable` and its `PublicError::AuditUnavailable` mapping | X2 (no AuditUnavailable) | f451d52be (main) |
| catalog/wire.rs | Describe correlation fields: synthesized `card_ref` Gate input (INPUT_CLASS_GATE_CORRELATION) replaced by stored-or-synthesized `card_uid`; test expects `[CARD_UID, RUN_ID]` | X1 (card_uid correlation) | dadaa3ab9, b5ab2dd5d (main) |
| cluster/mod.rs | New `ClusterTask` (stop token + join/abort, Drop aborts) | X4 | 6aaccebf9 (main), merged b356c948f |
| cluster/mod.rs | Delete `start_heartbeat`; `start_readiness_heartbeat`/`start_snapshot_poller` return `ClusterTask`, no shared shutdown token | X4 (role-owned heartbeat, one poller, no process-token cancellation) | 6aaccebf9 |
| cluster/mod.rs | `publish_snapshot_for_test` (test-support) — used only by `wyrd-server/src/scribe_outbox.rs:1629` | X3/X2 test seam (outbox peer routing) | dd14c0436 (main) |
| cluster/mod.rs | Three `ClusterTask` unit tests | X4 | 6aaccebf9 |
| contracts.rs | Fingerprint projection drops `CARD_REF` | X1 | dadaa3ab9 |
| contracts.rs | `ScribeIngressFrame.attributed_cards: Option<Vec<CardUid>>` | X1 (Gate attribution of unbound writers) + X2 (gateway capture subjects) | b5ab2dd5d, 70a6f4a4e |
| contracts.rs | Remove `ScribeError::CardUnresolved` (+ clone arm) | X1 (card_ref resolution gone) | dadaa3ab9 |
| namespaces.rs | Remove `BifrostNamespace::Dev` / `vala.dev`; `ALL` 11 -> 10 | X5 | 1090580f2 |
| oracle/admission.rs | Delete `AdmittedQueryGuard::drain_children` / `children_idle`; doc of `release_physical_projections` reworded | X8 (no wait on nested children; struck GRAPH_DRAIN/drain_children) | ba86fe129 superseded by 87e9d3f48 (X8 port) |
| oracle/analytical.rs | Delete `GRAPH_DRAIN_INTERVAL`/`GRAPH_DRAIN_POLLS`, `GraphLease::drain`, `AnalyticalGraphRelease` (+`release`/`reclaim` spawned task), `RECLAIM_WITHOUT_RUNTIME`; graph released inline in lifecycle settle and `Drop` | X8 (inline release, no spawned release work) | c63e2ff46 (main), 87e9d3f48 |
| oracle/analytical.rs | `AnalyticalCleanupPause::hold` -> `pub(super)` (called from oracle/mod.rs) | X8 test seam relocation (see Unmapped U2) | 87e9d3f48 |
| oracle/analytical.rs | Peer stage refusal: drop `PeerSecurityError::AuditUnavailable => QueryAuditUnavailable` arm, always `QueryPeerSecurity`; doc | X2 | f451d52be |
| oracle/analytical.rs | Remove `#[allow(clippy::type_complexity)]` + justification on activation return | main lint hygiene (main 8d98c3ceb removed the same dead allow) — deletion of an allow, not an addition | 8d98c3ceb (main) |
| oracle/analytical.rs | Tests: delete `assert_release_stops_at_the_graph_deadline`, `a_dropped_owner_with_an_undrained_envelope_retains_its_graph`, `await_until`; replace `..._is_not_released` with main's `assert_an_idle_graph_past_the_deadline_is_released` (byte-equal to main); drop-revokes test asserts inline release; follower tests use a stray attempt instead of a stuck memory child, renamed `follower_graph_release_waits_for_every_holder` (main name) / `follower_retains_a_graph_it_cannot_release` (integration name); shutdown now asserts `(attempts_settled, graphs_retained) == (1, 0)`; sibling-gate concurrency proof removed from `execute_task_first_without_set_plan_settles_at_signed_deadline` | X8 (tests of the struck drain mechanism removed/adapted) | c63e2ff46, 87e9d3f48 |
| oracle/analytical.rs | Test mock path `PeerSecurityError` -> `super::super::peer::PeerSecurityError` | import fallout of X2 | 87e9d3f48 |
| oracle/analytical_supervisor.rs | `resources` -> `_resources`; delete `graph_children_idle`/`graph_children_debt`/`graph_children` and the live-child refusal in `release_graph`; docs/test text | X8 | c63e2ff46, 87e9d3f48 |
| oracle/mod.rs | `OracleAudit::append_*` -> `stage_*`; blanket `impl<T: AuditStage> OracleAudit for T` + `query_audit_event` builder (byte-equal to main) | X2 (shared outbox, audit content) | 70a6f4a4e/d2af088b9 (main) |
| oracle/mod.rs | All `QueryAuditUnavailable` returns/docs -> `QueryExecutionFailed` (read decision, digests, topology, permission digest) | X2 (no AuditUnavailable); matches main | f451d52be (main) |
| oracle/mod.rs | `build_physical_root` takes `&BifrostQueryRequest`; `plan_logical(..., params)` with `with_param_values(bind_values(params))`; `bind_values`; test `bound_values_replace_placeholders_as_data`; tests pass `&[]` / `params: Vec::new()` | X6 | b620b069f, 82972f9f9 (main) |
| oracle/mod.rs | `settle_attempt_output`: `select!` on `admitted.cancellation` before first batch -> `settle_distributed_failure(QueryExecutionFailed, "first-batch cancellation")`; integration additionally stores `cancellation_marker` true on query telemetry | X8 (one cancellation token; cancel before first batch recorded as `cancelled`) — the marker is the integration's adaptation of 087cc0416 onto S's telemetry owner | c63e2ff46/087cc0416, 87e9d3f48 |
| oracle/mod.rs | Remove `admitted.drain_children()` calls (2); `mut admitted` -> `admitted`; doc on `settle_distributed_failure` | X8 | 87e9d3f48 |
| oracle/mod.rs | Docs: "fail-closed audit" -> "non-blocking audit" | X2 | 70a6f4a4e |
| oracle/peer.rs | Remove `PeerSecurityError::AuditUnavailable`; `PeerSecurityAudit` becomes sync `stage_unverified_ticket_rejection` / `stage_verified_ticket_violation` (no `async_trait`, no Result); `PeerSecurityAuditError` text | X2 (non-blocking audit) | f451d52be, d2af088b9 (main) |
| oracle/query_stream.rs | Remove failed-outcome `drain_children()` before release | X8 | 87e9d3f48 |
| resources.rs | Delete `LiveConsumers` type, view field, register/unregister bookkeeping, `memory_consumers`, `nested_idle`/`nested_memory_consumers`/`nested_memory_bytes`, and the fail-closed nested-child poison in `OracleQueryResources::release` | X8 (late child returns bytes through shared root; view drop is the leak check; struck LiveConsumers) | c63e2ff46, 87e9d3f48 |
| resources.rs | New test `oracle_view_dropped_holding_bytes_poisons_the_root` | Q2 / X8 | Q2 commit (1b5f4d7b3 era) |
| resources.rs | Delete S test `a_registered_zero_byte_child_keeps_its_query_busy`; `oracle_release_paths_are_exact_and_idempotent` now asserts a late child does not poison and returns its bytes | X8 (tests the removed mechanism) | 87e9d3f48 |

##### 2. Unmapped / suspicious

U1. `oracle/mod.rs:868-927` (`query_audit_event`) — REQ-011 gap, not a P-028 mismatch.
```
AuditEvent::new(context.request_id.clone(), ..., context.principal.id, context.principal.kind.tag(), ...)
    .with_detail(detail)
```
The X2 Oracle event builder is byte-equal to main, but it never calls `.with_credential_id(context.principal.credential_id)`. `Principal` carries `credential_id` (`wyrd-runtime/src/principal.rs:37`), and nothing in redux or the server stage sets it on Bifrost events. The only `with_credential_id` callers are `platform_extractor.rs`. REQ-011 asks for "nullable credential attribution through actual Gate/Oracle event builders". The same problem probably applies to the Gate builder (another fork's scope). Oracle audit rows will always have a null `credential_id`.

U2. `oracle/mod.rs:3939-3940` (`settle_distributed_failure`) — a test-support hook in a production function that neither S nor main has.
```
+    #[cfg(feature = "test-support")]
+    analytical::analytical_cleanup_pause_for_test().hold().await;
```
S holds this pause only in `AnalyticalGraphRelease::release`. Main holds it only in analytical.rs (lifecycle settle, plus a spawned drop release). Main's `settle_distributed_failure` has no hold. This second hold point arrived in 87e9d3f48 ("port S closeout code") and is consumed by the new journey `wyrd-testing/tests/bifrost/oracle/workflow.rs:202-228` through `peer_cluster.rs::arm_cleanup_pause`. No exception names it. It is X8-adjacent at most (the first-batch cancellation cleanup path). Every first-batch failure, timeout and cancellation in a test-support build now passes through the pause.

U3. `oracle/mod.rs:3653-3664` — X8 adaptation that goes beyond main.
```
+            if let Some(telemetry) = query_telemetry.as_ref() {
+                telemetry.cancellation_marker().store(true, Ordering::Release);
+            }
```
Main c63e2ff46 has the `select!` but no telemetry marker. 087cc0416 recorded the pre-stream cancel through the registry instead. This marker is the integration's mechanism for "recorded as `cancelled`" on S's telemetry owner. It plausibly fits X8's wording, but it is new code that is neither S nor main, so it needs a reviewer to confirm. A user-observable pin should also be confirmed (journey asserts outcome `cancelled`).

U4. `oracle/analytical.rs` tests (hunks @@ -4943 / -4968 / -5006 / -5027) — S coverage was dropped and not replaced. `execute_task_first_without_set_plan_settles_at_signed_deadline` lost its concurrency proof: "a slow sibling settlement does not delay another graph's cancellation on time" (`sibling-gate` memory child, assertion `the blocked sibling is still owned by the same driver`). X8 removes the memory-drain blocker, but the property "settlement driver settles concurrently, not serially" is S behaviour that is still reachable, for example through a stray attempt as the follower tests now use. Losing that assertion weakens an S test beyond what X8 requires.

U5. `oracle/analytical.rs` `follower_retains_a_graph_it_cannot_release` — the assertion was inverted and is not main-equivalent. S asserted that `shutdown` reports `graphs_retained == 1`. The integration asserts `(attempts_settled, graphs_retained) == (1, 0)` because the blocker is now a cancellable stray attempt. The test name is integration-authored (absent on main). Mapped to X8 as an adaptation, but the scenario changed meaning: it no longer proves that shutdown reports unreleasable residue.

U6. Wording residue (not hunks, but AC-028 "no trace"): `oracle/peer.rs` new doc "Stages a violation on the context-named tenant's **audit chain**". `oracle/mod.rs:760` "not on the audit chain". The `BifrostQueryReadDecision` field doc says "appended to the tenant audit chain". X2/REQ-011 removes the chain. Doc-only.

No `#[allow]`/`#[expect]`/`cfg_attr` was added (one `#[allow(clippy::type_complexity)]` was removed). No new `Box::pin` (the one changed line renames an S call). No production timeout or deadline constant changed (GRAPH_DRAIN_* were deleted under X8). No status-code change beyond X2's `QueryAuditUnavailable -> QueryExecutionFailed`, which matches main. No compatibility shims.

##### 3. Counts

- Logical hunks mapped: 36 (X1: 4, X2: 13, X3/X2 seam: 1, X4: 3, X5: 4, X6: 1, X8: 11, Q2: 1, lint hygiene: 1 — some rows carry two).
- Unmapped / suspicious: 6 (U2 unmapped test hook; U3 new X8 code needing confirmation; U4/U5 weakened or inverted S test coverage; U1 REQ-011 credential gap at parity with main; U6 doc wording).

#### Part: P-028 ledger part C2: vala-bifrost-redux gate/, scribe/, tables/

Scope: `git diff cfe21dc9007d5d22d9ae4411be07fb5504b0cc05 -- crates/vala/vala-bifrost-redux/src/{gate,scribe,tables}`: 34 files.
`crates/vala/vala-bifrost-redux/Cargo.toml` has no diff against S. Every other redux file outside the
forge/oracle/resources/cluster/catalog/contracts/namespaces/tests-forge areas falls in gate/, scribe/ or tables/.
Main-ancestor commits (on `main`, merged in): dadaa3ab9, b5ab2dd5d, bcf4b9248, 70a6f4a4e, 1090580f2, c8febbd49,
71fe71c35, 82972f9f9, 8b7218a92, 6aaccebf9, 515f16c97, 753916bd0, d2af088b9, 5a5542cbb, f451d52be, 52e1144b5, e54b1244f.
Branch commits: b356c948f (merge), 597533616, eaa129c4a, 87e9d3f48, f10f735dd/53dfc2a1c (struck; reverted by 87e9d3f48).

##### 1. Ledger

| File | Hunk summary | Exception / reason | Commit(s) |
|---|---|---|---|
| gate/attribution.rs (new, 298) | `CardRegistry` resolves an unbound writer's distinct `card_uid`s (at most 32) to registered observation-target Cards on `tenant_conn`. Includes per-signal UID collectors and `parse_distinct` | X1 (REQ-013 unbound writer, 32-UID cap, TenantConn per X7) | b5ab2dd5d, dadaa3ab9 |
| gate/auth.rs | `AuthContext::from_verified(VerifiedToken, metadata)`; `authenticate` builds through it. Tenant comes from `principal.tenant_id`, which `verify_on` already pins equal to `expected_tenant`, so behavior is unchanged | X7 (verified-principal Gate entrance, API-key OTLP) | 71fe71c35 |
| gate/error.rs | `CardScopeDenied` becomes fieldless (`card_ref` field dropped) | X1 (REQ-013, c8febbd49 "fieldless denial") | c8febbd49, dadaa3ab9 |
| gate/error.rs | `AuditUnavailable` variant, its `audit_unavailable` rejection reason (array 9→8), and the BifrostError mapping are deleted | X2 (no public AuditUnavailable) | f451d52be, b356c948f |
| gate/error.rs | `ScribeError::CardUnresolved` mapping and its test row are deleted | X1 (Scribe no longer resolves CardRef text) | dadaa3ab9 |
| gate/mod.rs | `pub mod attribution`; `cards: Option<CardRegistry>`, `with_card_registry`, `attributed_cards`. Each OTLP signal and the native path resolve attributed UIDs (else the signed scope `CardRefScope::uids`) and pass `attributed_cards` on `ScribeIngressFrame` | X1 | b5ab2dd5d, dadaa3ab9, bcf4b9248 |
| gate/mod.rs | `GateAudit` blanket impl over `wyrd_runtime::audit::AuditStage` builds `AuditEvent` (`bifrost.record.write`, delegation detail) and stages it. Docs reworded to non-blocking | X2 (shared outbox; REQ-011 credential/delegation attribution) | 70a6f4a4e, d2af088b9, 5a5542cbb |
| gate/mod.rs | A missing audit sink now refuses with `Internal` instead of `AuditUnavailable` (test updated) | X2 | f451d52be |
| gate/mod.rs | SYSTEM-writer branch deleted (`native_frame` param, `require_frame_card_scope`, system→result-table-only rule). All three result tables and `gateway.calls` are now refused for every public principal | X2 (REQ-012: results go through the private internal writer; SYSTEM tokens removed) | 8b7218a92 |
| gate/mod.rs | `admit_otlp_verified(VerifiedToken, metadata)` | X7 (API-key OTLP auth before decode) | 71fe71c35 |
| gate/mod.rs tests | `card_ref`→`card_uid` renames in CountingScribe/scoped fixtures; mixed-OTLP test uses `REFERENCE_TEXT` | X1 | dadaa3ab9 |
| gate/mod.rs tests | `query_request` adds `params: Vec::new()` | X6 | 82972f9f9 |
| gate/mod.rs tests | S tests deleted: `gate_denies_system_result_writes_outside_the_exact_card_scope`, `gate_confines_verification_result_tables_to_the_system_writer`, `gate_refuses_the_system_writer_on_otlp_ingest`. Replaced by `public_result_table_writes_are_refused_for_every_principal` | X2 (REQ-012); see suspicious S3 | 8b7218a92 |
| gate/mod.rs | rustdoc link `logical_data_identity`→`LogicalIdentity` | doc-link fix for an S symbol (compile/doc lint) | b356c948f |
| scribe/execution_lanes.rs | `CardRef`/`CARD_REF` correlation replaced by `card_uid`. `authorized_card_uids(rows, scope)` replaces `validate_card_scope`, `require_card_scope` and `resolve_card_uids`. `card_scope` is added to `IngressDecodeInputs`/`DecodeContext`/`ManagedValues`. `card_uid` is allowed as a caller column, then authorized and restamped | X1 | dadaa3ab9, b5ab2dd5d |
| scribe/execution_lanes.rs | `CorrelationPolicy` argument removed from `source_schema_fingerprint`/`user_fields`/`user_columns`/`is_server_owned` (no CodeAxis `owns_run_id`). Test `code_axis_decode_keeps_declared_run_id_content` deleted | X5 (agent_traces/CodeAxis removed) | 1090580f2, ce866229d |
| scribe/execution_lanes.rs tests | Fixtures rewritten for UID text (`uid_text`, `SuppliedCardUid`). The "client UID loses to signed UID" forged case is removed (no CardRef text exists any more). Expected error `CardUnresolved`→`CardScopeDenied` | X1 | dadaa3ab9 |
| scribe/ingress.rs | `AdmittedRowContext.card_scope` = `frame.attributed_cards` or the signed-scope UIDs; threaded into native/canonical decode | X1 | b5ab2dd5d, dadaa3ab9 |
| scribe/ingress.rs | Lazy `ensure_builtin` removed from `resolve_logical_frame` and `resolve_write_table_uid`; a missing built-in is `TableNotFound` | X5 (eager provisioning) | 1090580f2 |
| scribe/preprocess.rs | `NativeAdmittedRows.card_scope` threaded to `DecodeContext` | X1 | b5ab2dd5d |
| scribe/mod.rs | `control_postgres: Option<Arc<ValaPostgres>>` → `fence_operator: Option<OperatorPool>` for WAL replay fences (`config.operator_pool`). `AdmissionClock` keeps the control pool | X7 (OperatorPool) | eaa129c4a |
| scribe/mod.rs | `attributed_cards: None` on the internal-write frame; `ScribeError::CardUnresolved` removed from the invalid-outcome match | X1 | dadaa3ab9 |
| scribe/mod.rs | `staging_backlog_for_test(table_name: Option<&str>)` | Test seam forced by X2 (pod's `audit_log` stages beside test tables); main mechanism, see S1 | 6aaccebf9 |
| scribe/replay.rs | `ReplayFences` holds `OperatorPool`; `newest_wal_fence(&operator, …)` | X7 | eaa129c4a |
| scribe/replay.rs, tests/scribe_persistence_path.rs, tests/wal_closeout.rs | `attributed_cards: None` added to test frames | X1 (struct field) | b5ab2dd5d |
| scribe/assembly.rs | `backlog()` delegates to new `backlog_of(Option<&str>)`, which filters ready keys and claims by table name | X2-forced test seam (main 6aaccebf9); see S1 | 6aaccebf9 (87e9d3f48 reverted the f10f735dd extension) |
| scribe/staging_runtime.rs | `backlog(table_name)` signature; tests pass `None` | same as above, S1 | 6aaccebf9 |
| scribe/persistence.rs | `PersistenceFaults.target_table` + `target_table_for_test`/`targets`. Claim faults (`take_object_write`, `take_post_commit_client_error`, `take_claim_retirement`) and `note_claim_published` are scoped by table; `staging_backlog_for_test(table_name)` | X2-forced test seam (cfg test/test-support only); main mechanism, see S1 | 6aaccebf9 (f10f735dd/53dfc2a1c reverted by 87e9d3f48) |
| scribe/claim_publication.rs | `fail_claim_retirement(&key.table().name)` (cfg test) | same seam, S1 | 6aaccebf9 |
| scribe/persistence.rs, shards.rs, staging_runtime.rs (pg tests) | `superuser_pool().await` → `superuser_pool()` | Main fixture API (`wyrd_dev_fixtures::pg::PgFixture`) rename forced on S tests | b356c948f / 515f16c97 |
| scribe/registry.rs | Module doc: "do not emit `audit_staging` rows" → "audit events" | X2 (no audit staging) | 70a6f4a4e |
| scribe/wal.rs | `WalWriter::stream_dir()`. `initialize_existing_stream` lists only the current `(node, epoch)` directory, and `open_segment` uses the helper | Q1 (S production fix; recorded diagnosis) | 87e9d3f48 |
| tables/audit/audit_log.rs | Drops `seq`, `entry_hash`, `prev_hash` fields and the `sort_asc("seq")` sort key; 15→12 content columns; docs: publisher→Scribe/internal audit write | X2 (no chain; 12 content fields) | 70a6f4a4e |
| tables/audit/projection.rs | `project_audit_rows` (staging rows, seq contiguity, hash validation, `derive_batch_id`, `AuditProjection`) replaced by `project_audit_event(&AuditEvent, decided_at)`. Error enum reduced to Arrow; tests replaced | X2 (no staging/publisher/chain; direct AuditEvent projection, REQ-011) | 70a6f4a4e, 52e1144b5, e54b1244f, 597533616 |
| tables/dev/agent_traces.rs, tables/dev/mod.rs (deleted) | `vala.dev.agent_traces` table removed | X5 | 1090580f2 |
| tables/mod.rs | `pub mod dev`, `AgentTracesTable`, `CorrelationPolicy::CodeAxis` removed; `BUILTIN_TABLES` 11→10; layout and variant test rows drop `dev.agent_traces` | X5 | 1090580f2, ce866229d |
| tables/mod.rs tests | `verification_results_contract` adds `variant_field("assertion_result", true)`; variant-column matrix adds `verification.results`→`assertion_result` | X9 | b356c948f |
| tables/managed_columns.rs | `CodeAxis` arm, `owns_run_id`, `code_axis_policy_omits_run_id` test removed | X5 | 1090580f2 |
| tables/signal.rs | `CARD_REF_ATTRIBUTE wyrd.card_ref`→`CARD_UID_ATTRIBUTE wyrd.card_uid`; `RecordCorrelation.card_uid`; `extract` takes `Option<&[CardUid]>`; `authorize_card_ref` replaced by `card_uid_text`; `correlation_fields` emits `card_uid`; correlation fixture becomes UID-based | X1 | dadaa3ab9, b5ab2dd5d |
| tables/{traces,metrics,logs}/projection.rs | `card_scope: Option<&CardRefScope>`→`Option<&[CardUid]>`; `card_ref` column→`card_uid` | X1 | dadaa3ab9 |
| tables/{traces,metrics,logs}/mod.rs tests | `wyrd.card_ref`→`wyrd.card_uid`; `WITHOUT_UID`→`REFERENCE_TEXT`; `scope().as_slice()` | X1 | dadaa3ab9 |
| tables/eval/observations.rs | `acknowledged(frame)` reads the stored `card_uid` column instead of re-resolving through the principal (`resolve_card_uids` removed); a bad UID is `InvalidFrame` instead of `CardUnresolved`; test rewritten | X1 (REQ-013/REQ-014 activation from stored UID) | bcf4b9248, dadaa3ab9 |
| tables/verification/results.rs, verification/mod.rs | `ASSERTION_RESULT` nullable Variant column, re-export, docs; added to `SENSITIVE_PAYLOAD_COLUMNS` | X9 | b356c948f, 7f79fb341 |

##### 2. Unmapped / suspicious

No hunk in this scope is purely unmapped. Every table-schema difference in `tables/` maps to X1 (`card_uid` correlation), X2 (audit content and no chain), X5 (agent_traces/CodeAxis) or X9 (`assertion_result`). There are no time-axis, precision or column changes outside those exceptions. These hunks need a reviewer's judgement:

- **S1. Table-scoped Scribe test seams are a main mechanism no Q row records.**
  - Where:
    - `scribe/assembly.rs:1043` `pub fn backlog_of(&self, table_name: Option<&str>)`
    - `scribe/staging_runtime.rs:305` `pub fn backlog(&self, table_name: Option<&str>)`
    - `scribe/persistence.rs:150,181-192,413-449` (`target_table`, `target_table_for_test`, `targets`, table-scoped `take_*` faults)
    - `scribe/claim_publication.rs:475`
    - `scribe/mod.rs:2237`
  - Source: main commit 6aaccebf9 ("Repair shared-outbox Scribe routing, Forge self-reclaim, and Oracle settle and shutdown drain"), carried through the merge.
  - Justification: forced by X2. Every audited call stages an `audit_log` member on the same pod, so S's pod-wide backlog and one-shot faults are no longer test-deterministic. Consumers are `wyrd-testing/tests/bifrost/scribe/{lifecycle,source_boundary_recovery,sustained,telemetry}.rs`.
  - Why flagged:
    - (a) It is main code; no TASK-002 Q row or spec exception names it.
    - (b) `backlog_of` and the re-signed `ScribeStagingRuntime::backlog(Option<&str>)` are not cfg-gated, so a production `pub` API changed shape for a test-only need.
    - (c) The struck commit f10f735dd ("Prove flush ownership under competing audit staging traffic", REQ-005) extended this same seam. 87e9d3f48 reverted that extension, but the base seam remains.
  - Recommend: record it as an X2-forced test seam (add a Q row), or cfg-gate `backlog_of`.
- **S2. Dead `IngestError::CardUnresolved { card_ref }` variant.** `gate/error.rs:49-55,166,220`.
  - X1 removed `ScribeError::CardUnresolved` and its `from_scribe` mapping, so this variant now has no production constructor in redux. Its only remaining uses are the `gate/error.rs:459` test and `wyrd-server/src/http/otlp.rs:667`, which is outside this scope (check whether that is a test).
  - Its doc still speaks of a `card_ref` that "cannot be resolved to card_uid". That is S text, so it is not a diff hunk, but X1/REQ-013 ("UID everywhere") leaves it incomplete: a leftover CardRef-resolution error surface.
- **S3. Deleted S Gate tests with no direct replacement for one property.** `gate/mod.rs` tests.
  - `gate_refuses_the_system_writer_on_otlp_ingest` and the "system writer refused every non-result table" cells of `gate_confines_verification_result_tables_to_the_system_writer` are deleted along with the production rule at old `gate/mod.rs:534-557` (`system && !result_table → RbacDenied`).
  - Under X2/REQ-012 no SYSTEM token reaches Gate, which justifies dropping the rule. But a `PrincipalKind::System` with a wildcard is now governed only by ordinary RBAC on public tables, and nothing asserts the old confinement.
  - Mapped to X2. Flagged because a negative-path S test was deleted rather than adapted.
- **S4. `assertion_result` added to `SENSITIVE_PAYLOAD_COLUMNS`.** `tables/verification/results.rs:1331`.
  - It follows S's rule that the results table's summaries are sensitive, so it is X9-consistent.
  - REQ-016 says "do not add a general elevated-read requirement for verification summaries". Confirm this matches main's X9 column behavior (main 7f79fb341), not a new gate.
- **S5. Results-table rustdoc still says "The managed `run_id` is the Verifier run".** `tables/verification/results.rs` (unchanged S line, near :1309).
  - REQ-016 says result `run_id` is the application Run being judged.
  - This is S text, not a hunk, so it is out of P-028's strict scope. It conflicts with the X9/REQ-016 adaptation next to it; worth a check by the Part C owner.

Searched with no hits in this scope's added lines: `#[allow`, `#[expect(clippy`, `cfg_attr`, `Box::pin`, `Duration::from_secs/millis` changes, 502/`BAD_GATEWAY`, `run_id` None in results. No compatibility shims. None of the struck symbols (`OwnedMembers`, `owns_any`, `audit_staging`, `AuditUnavailable`, `prev_hash`, `chain_hash`, `CodeAxis`, `agent_traces`) appear in the gate/scribe/tables sources.

##### 3. Counts

- Files: 34 (gate 4, scribe 15, tables 15). Logical hunks in the table: 43.
- Mapped to an exception, Q row or forced rename: 43 (X1 18, X2 9, X5 6, X6 1, X7 4, X9 2, Q1 1, main fixture rename 1, doc-link fix 1). Four of these also carry suspicious flags, S1 or S3.
- Unmapped: 0.
- Suspicious: 5 (S1 is the main-mechanism test seam, the most material; S2 dead variant; S3 deleted negative test; S4 and S5 are minor X9/REQ-016 checks).

#### Part: P-028 part D: server seams, wyrd_client::bifrost, bifrost-design.md

Base: `git diff cfe21dc9007d5d22d9ae4411be07fb5504b0cc05 -- <file>`.

Attribution: first-parent history after the merge (`b356c948f^1..HEAD`) touches only
`client/{facade,handle,query}.rs` and `server/bifrost/mod.rs` (`87e9d3f48`, `597533616`),
plus `bifrost-design.md` (`eaa129c4a`, `1b5f4d7b3`, `e01e60b3b`). Every other hunk in this
part arrived as main's side of the merge resolution `b356c948f`. The originating main commit is
given where `git log S..HEAD -- <path>` names it.

##### 1. Ledger

| File | Hunk summary | Exception / reason | Commit(s) |
|---|---|---|---|
| wyrd-server/src/bifrost/gate_audit.rs | File deleted (`OutboxGateAudit` sink over `OracleQueryAudit`, "tenant audit chain") | X2: one Scribe outbox, no audit chain; Gate audit is staged through the shared outbox | b356c948f (main 70a6f4a4e, d2af088b9); 87e9d3f48 removed the leftover doc line |
| wyrd-server/src/bifrost/mod.rs | `pub mod gate_audit` and its doc removed | X2 | b356c948f, 87e9d3f48 |
| wyrd-server/src/bifrost/routes.rs | 3× OpenAPI `default` response no longer lists `WYRD_VALA_500_AUDIT_UNAVAILABLE` | X2 (no public AuditUnavailable error) | b356c948f (main f451d52be) |
| wyrd-server/src/bifrost/service.rs | `register_table`: fail-closed `authorize_recording_denial`/`record_audit`, with the allowed row committed inside `register_dataset`, replaced by a synchronous `audit::authorize(...)?` staged on the outbox; the per-branch `record_allowed()` calls are gone and the `Some(allowed)` arg is dropped from `register_dataset`; doc rewritten | X2 (permission blocks, audit non-blocking, no staging) | b356c948f (main d2af088b9, f451d52be, 70a6f4a4e) |
| wyrd-server/src/bifrost/service.rs | `list_tables`/`describe_table`: `authorize(...).await?` becomes sync `authorize(...)?`; `AuditUnavailable` removed from docs | X2 | b356c948f |
| wyrd-server/src/bifrost/service.rs (pg_tests) | `audit_rows_for` reads `RecordingScribe::audit_decisions()` after `scribe_outbox.settle(30 s)` instead of `audit_staging::list_audit_events_for_resource`; `AuditStagingRow` becomes `AuditDecision`, `principal_id` becomes `audit_principal_id`; tests use `RecordingScribe::attach` | X2 | b356c948f (main 37e3b2ddd) |
| wyrd-server/src/bifrost/service.rs (pg_tests) | `test_pool()`/`test_tenant()` no longer awaited; `caller_with` made sync | Main test-support API (non-Bifrost `crate::test_support`) | b356c948f |
| wyrd-server/src/bifrost/service.rs (pg_tests) | `Duration::from_secs(600)` becomes `Duration::from_mins(10)` (same value) | Main pedantic lint (non-behavioral) | b356c948f (main 515f16c97) |
| wyrd-server/src/grpc/capture_peer.rs | Peer writer widened from two capture tables to six destinations (`ScribeTable`: audit log, gateway calls, trace spans, three result tables); system owner admitted only for `AuditLog`; `VerifierAttribution` required exactly on result tables; `CaptureBatch` becomes `ScribeBatch` from `crate::scribe_outbox`; module/fn docs | X2 (six trusted destinations, REQ-012) + X3 (internal writes routed to a Scribe pod) | b356c948f (main 8b7218a92, f7b962d9e, 70a6f4a4e) |
| wyrd-server/src/grpc/capture_peer.rs (tests) | Acceptance/refusal matrix for six destinations, attribution and system owner | X2 | b356c948f |
| wyrd-server/src/grpc/mod.rs | `mod capture_peer` becomes `pub(crate) mod capture_peer` (used by `scribe_outbox.rs:928` tests) | X2 | b356c948f |
| wyrd-server/src/grpc/mod.rs | Peer capture service gets `.max_decoding_message_size(transport.message_limit_bytes())` | X2/X3: outbox frame bounded by the configured ceiling, not tonic's 4 MiB default (proved by b7b2de753) | b356c948f (main dd14c0436) |
| wyrd-server/src/grpc/mod.rs | `match try_acquire { Err => return }` becomes `let Ok(_lease) = … else` | Main pedantic lint (`manual_let_else`); same behavior | b356c948f (main 515f16c97) |
| wyrd-server/src/grpc/mod.rs | OTLP services built from `&AppState` instead of `Arc<Bifrost>` | X7 (API-key auth needs auth owners in state) | b356c948f (main 71fe71c35) |
| wyrd-server/src/grpc/otlp.rs | New `OtlpGrpcAuthentication`: a call carrying only `x-wyrd-api-key` (no access token) is verified through `TokenGrants::api_key_principal`, then `gate.admit_otlp_verified`; anything else goes through Gate's `authenticate_otlp_metadata`. Runs before decode in all three signal services | X7 (API-key OTLP auth before decode) | b356c948f (main 53d044191→41ae7eb91→71fe71c35) |
| wyrd-server/src/otlp_decode.rs | About 10 `match` arms whose bodies are all `{}` merged into or-patterns (`visit_scope/span/status/span_event/span_link`, `decode_resource_spans/scope_spans/span/key_value`) | Main pedantic lint (`match_same_arms`); behavior unchanged (every arm is still before its `(1..=N, _)` wrong-wire arm) | b356c948f (main 515f16c97) |
| wyrd-server/src/otlp_decode.rs | `#[expect(clippy::struct_field_names, reason = …)]` added on `OtlpDecodePlan` | Main pedantic lint; see suspicious D-S1 | b356c948f (main 515f16c97) |
| wyrd-client/bifrost/{facade,blocking}.rs | `sql`, `sql_as`, `stream` gain `params: &[QueryParam]`; the request carries `params`; `QueryParam` re-exported from `bifrost/mod.rs`; doc says `params[i]` binds `$(i+1)` | X6 | b356c948f (main 82972f9f9) |
| wyrd-client/bifrost/query.rs (tests) | `query_body_server` + `sql_forwards_bind_values_without_interpolation` | X6 | b356c948f |
| wyrd-client/bifrost/query.rs | `validate().map_err(... QueryInvalidSql{detail})` becomes `validate().map_err(\|e\| Transport(e.into()))`; doc lists invalid SQL, non-finite bind and deadline under one code | X6 (non-finite bind refusal) + main `wyrd-spec` validator type (spec now returns a catalog error); code stays `WYRD_VALA_400_QUERY_INVALID_SQL` | b356c948f (main aa0aeb52b, cb54cbb11), 87e9d3f48 (doc) |
| wyrd-client/bifrost/{facade,handle,observe,table,mod}.rs | `Correlation.card_ref: Option<CardRef>` becomes `card_uid: Option<CardUid>` across `insert*`, `observe::record`, `WriterPool::insert/insert_rows`; docs and reserved-column name `card_uid`; tests use a fixed UUID | X1 | b356c948f (main dadaa3ab9) |
| wyrd-client/bifrost/query.rs (tests) | Describe fixture: `card_ref` becomes `card_uid`; its metadata changes from `wyrd:input_class=gate_correlation` to `PARQUET:field_id=1001` | X1 (fixture JSON only; neither key is produced by the server in either tree) | b356c948f |
| wyrd-client/bifrost/facade.rs | `insert_rows_into` delegates to new `pub(crate) insert_observation`; `writer_table` delegates to `resolve_writer_table`; `cached_writer_table` delegates to private `described_table`, so `observe` works without `internal` | Main `internal` feature gate (crate-wide wyrd-client feature, consumed by CLI/MCP/SDK manifests); 87e9d3f48 removed main's `wyrd_event_time` client stamping from it (REQ-017) | b356c948f, 87e9d3f48 |
| wyrd-client/bifrost/{facade,blocking,handle,query,scope,sink,mod}.rs | About 35 `#[cfg(feature = "internal")]` gates on `query`, `collect_bounded`, `with_sink`, `query_only`, `writer_table`, `insert_into`, `insert_rows_into`, `enqueue_batch`, `scope`, `metrics`, `observe_losses`, `into_async`, `register_outcome_name`, `client_from_options`, `QueryOnlySink`, `ClientScope` re-export, `BifrostIngestSink::new`, `QueryResultStream::{settle, collect_bounded, encoded_bytes, client field}`, etc.; `BifrostIngestSink.transport` made `pub(crate)`; `ClientScope::from_client` made `pub(crate)` | Main-forced: the `internal` feature exists only on main (S's Cargo.toml has none) and all main consumers enable it. Not an X item; see D-S2 | b356c948f |
| wyrd-client/bifrost/{facade,blocking}.rs | `describe(fqn)` becomes `describe_table(namespace, name)`; `split_fqn` call removed; facade-surface compile test renamed | Main SDK consumer-forced rename (`sdks/wyrd-sdk-python/src/bifrost/mod.rs`, TS `index.d.ts` call `describe_table`); see D-S3 | b356c948f |
| wyrd-client/bifrost/{facade,blocking}.rs | `stream(query)` gains `deadline: Option<Duration>`; ms saturate to `i64::MAX` and the server validates the range | Main API (consumer-forced); see D-S3 | b356c948f |
| wyrd-client/bifrost/facade.rs | `QueryResult::to_ipc` renamed `to_bytes` | Main SDK consumer-forced (`wyrd-sdk-python/src/bifrost/mod.rs:324`); see D-S3 | b356c948f |
| wyrd-client/bifrost/facade.rs | `from_env`/`client_from_env`/`client_from_options` resolve via `GlobalConfig::load_in(&Environment)` + `ClientConfig::from_environment`; blank explicit `server_url`/`grpc_url`/`credential` refused with `WYRD_CLIENT_400_CONFIG_INVALID`; new `client_from_options_in`; 2 unit tests | Main non-Bifrost client config API (`environment`, `global_config`) forced | b356c948f (main 1ca7a4316, 753916bd0) |
| wyrd-client/bifrost/{facade,query,grpc}.rs | `self.query.client().request_json` becomes `.client().http.request_json`; `client.connect_grpc()` becomes `GrpcConnection::connect(&client.grpc_config, Arc::clone(&client.auth))`; `client.auth()` becomes `&client.auth` | Main `WyrdClient` struct shape (non-Bifrost) forced | b356c948f |
| wyrd-client/bifrost/facade.rs | `BifrostIngestSink::new(..)` becomes a struct literal (because `new` is internal-gated) | Main `internal` gate fallout | b356c948f |
| wyrd-client/bifrost/scope.rs | `fingerprint_credential` handles `ResolvedCredential::Renewable(source) => source.identity()` | Main credential model (renewable credential pooling identity; X7 identity entrance, non-Bifrost auth type) forced by exhaustive match | b356c948f |
| wyrd-client/bifrost/grpc.rs (tests) | `unauthenticated_refusal_forces_one_shared_refresh_and_resends` moves from API-key exchange (wiremock `/auth/token`, `expires_at`) to `ResolvedCredential::Renewable(SequencedSource)`; `retain_held_batch` takes `credential` and `http_timeout_ms` and returns the auth owner | Main credential model forced (API-key exchange removed on main by 71fe71c35) | b356c948f |
| wyrd-client/bifrost/grpc.rs (tests) | New `delayed_refresh_spends_one_transport_budget_then_retains`, `blocking_mint_spends_one_transport_budget_then_retains`, `RefreshHeldIngest`, `HeldSource`, `SequencedSource` | Main tests; see D-S4 | b356c948f |
| wyrd-client/bifrost/mod.rs (tests) | `http://x` becomes `http://127.0.0.1:1` in scope/client fixtures | Main client URL validation forced | b356c948f |
| wyrd-client/bifrost/mod.rs (tests) | New `downstream_stall_remains_bounded_and_recovers_after_drain` (+`ReleasingSink`) and `register_never_stamps_one_tables_identity_onto_another` | Main tests; see D-S4 | b356c948f |
| wyrd-client/bifrost/query.rs | `match u64::try_from` becomes `let Ok(rows) = … else` | Main pedantic lint | b356c948f |
| wyrd-client/bifrost/query.rs | Doc path `tests/pg_bifrost_e2e.rs` becomes `tests/integration/pg_bifrost_e2e.rs` | Main test-binary layout (non-Bifrost) | b356c948f |
| wyrd-client/bifrost/* (all files) | About 60 `# Arguments` rustdoc blocks added; intra-doc links demoted to code spans where the target is internal-gated | Main doc convention / `internal` gate fallout (non-behavioral) | b356c948f |
| wyrd-client/bifrost/{facade,handle}.rs | 87e9d3f48: removes main's `event_time_micros`/`wyrd_event_time` client stamping (`Some(Utc::now()...)`), so the queue call matches S's 3-arg `enqueue_rows` | Restores S (REQ-017 time axes); no residual diff vs S apart from the `insert_observation` indirection | 87e9d3f48 |
| bifrost-design.md §Correlation (L81–91) | `wyrd.card_ref` becomes `wyrd.card_uid`; a client UID is stored only when the verified scope (or, for an unbound principal, the tenant registry) confirms it | X1 | b356c948f (main dadaa3ab9) |
| bifrost-design.md §Built-in tables (L127–138) | Lazy first-use materialization replaced by eager provisioning plus a startup backfill of the canonical inventory | X5 (eager provisioning) | b356c948f (main 1090580f2) |
| bifrost-design.md §Variant columns (L231–237) | `vala.dev.agent_traces` columns removed; `vala.verification.results.assertion_result` added as Variant | X5 + X9 | b356c948f, 1b5f4d7b3 |
| bifrost-design.md §Oracle memory (L668–676) | "after the graph drains" becomes "when the graph ends, without waiting for nested children"; late child returns bytes via shared root; view drop poisons; "Cleanup timeout or failure" becomes "Cleanup failure" | X8 | b356c948f (main c63e2ff46), 1b5f4d7b3 |
| bifrost-design.md §Read audit (L910–925) | Oracle read audit moves from the hash-chain staging + `AuditPublisher` to the Scribe outbox (`outbox_write_failures_total`, `outbox_events_lost_total`, `outbox_pending`) | X2 | b356c948f, 1b5f4d7b3 (main 1e57da5cb) |
| bifrost-design.md §Audit history (L948–965) | `vala.audit_staging` watermark/publisher/in-flight bound prose replaced by the outbox writer, platform audit principal for the system owner, no chain/sequence | X2 | same |
| bifrost-design.md §Forge authority (L1014–1024) | Active `claimed/running/prepared` task row is the table's exclusive lease; heartbeat renews; one terminal transition after the heartbeat stops releases; strategies return their decision to the settlement seam | X10 | e01e60b3b |
| bifrost-design.md §Audit rules (L1232–1240) | "Surfaces not yet converted … fail closed" removed; "audit publication" dropped from engine-internal transitions | X2 | b356c948f, 1b5f4d7b3 |
| bifrost-design.md §Namespaces/permissions (L1370–1393) | `vala.dev` namespace removed; public writes refused for gateway calls and three result tables; peer `IngestCapture` accepts five tables + audit log, attribution exactly on result tables, system owner only for audit | X5 + X2 (six destinations) + X3 | b356c948f, 1b5f4d7b3 |

Checked: post-merge `eaa129c4a` (TenantConn/OperatorPool) and the struck main Forge
"within the original deadline" wording (from main 5ff25deb6) were present at intermediate
commits but leave **no residual diff** against S; `1b5f4d7b3` reverted them.

##### 2. Unmapped / suspicious

None of these brings back a struck main Bifrost mechanism: no audit staging, no publisher, no deadline-refusal code, no Forge SQL. All are either lint/convention attributes or main client surface. That client surface fits no X1–X10 item and is justified only as "main-forced". The reviewer must rule on whether that covers it.

- **D-S1 `crates/wyrd/wyrd-server/src/otlp_decode.rs:19`, `#[expect(...)]` added**
  ```rust
  #[expect(
      clippy::struct_field_names,
      reason = "every field is a byte count; the shared suffix names the unit"
  )]
  pub(crate) struct OtlpDecodePlan {
  ```
  This is a lint suppression attribute added to S code. It comes from main's pedantic-lint closeout (515f16c97). It maps to no exception. AGENTS.md §12 prohibits adding `#[allow]` to make a gate pass. `#[expect]` with a reason is the main-wide precedent, but it is still a suppression on S code. The alternative is renaming the `*_bytes` fields.

- **D-S2 `wyrd-client/src/bifrost/*`, main `internal` feature gates on Bifrost client API (about 35 sites)**
  The gates cover facade, blocking, handle, query, scope, sink and mod. Example, `query.rs:2102`:
  ```rust
  /// owes; only internal settlement reads it.
  #[cfg(feature = "internal")]
  client: QueryClient,
  ```
  plus `#[cfg(not(feature = "internal"))] let _ = client;`.
  - **Behavioral consequence:** without `internal`, `QueryResultStream::settle`, `collect_bounded`, `Bifrost::query`, `with_sink`, `query_only`, `insert_into`, `enqueue_batch`, `metrics`, `observe_losses`, `scope`, `register_outcome_name` and `client_from_options` do not exist. In S they are always public.
  - **Mapping:** this is main's REQ-021 "keep main internal feature gates". Revision 3 struck "retain main's …" obligations inside Bifrost scope, so it is covered by no X item.
  - **Mitigation:** the feature is a crate-wide main wyrd-client feature, and every in-tree consumer enables it (`wyrd-cli`, `wyrd-mcp`, `wyrd-testing`, both SDK native crates). It is effectively main-forced, but it needs an explicit ruling or a recorded "non-Bifrost wyrd-client feature" justification.

- **D-S3 `wyrd-client/src/bifrost/facade.rs` and `blocking.rs`, main public-API reshapes**
  - `describe(fqn)` becomes `describe_table(namespace, name)` (`facade.rs:806`, `blocking.rs:166`).
  - `stream(query)` becomes `stream(query, params, deadline: Option<Duration>)` (`facade.rs:705`). The deadline is converted with `i64::try_from(deadline.as_millis()).unwrap_or(i64::MAX)`: an over-range value saturates to `i64::MAX`, and the server then refuses it.
  - `QueryResult::to_ipc` becomes `to_bytes` (`facade.rs:845`).

  These are main Bifrost-client API shapes, not X6. X6 covers only `params`. The callers that force them sit outside Bifrost scope: the Python SDK (`sdks/wyrd-sdk-python/src/bifrost/mod.rs:324` `to_bytes`; `describe_table` in the `.pyi` and TS `index.d.ts`). Classify them as consumer-forced renames and record that explicitly, or revert to S names and adapt the SDKs.

- **D-S4 leftover main tests inside Bifrost-scope client code** (tests not present in S):
  - `grpc.rs`: `delayed_refresh_spends_one_transport_budget_then_retains` and `blocking_mint_spends_one_transport_budget_then_retains`, with the helpers `RefreshHeldIngest`, `HeldSource` and `SequencedSource`. They test main's renewable-credential refresh against the S Bifrost transport budget.
  - `mod.rs` (`mod sdk`): `downstream_stall_remains_bounded_and_recovers_after_drain` (+`ReleasingSink`) and `register_never_stamps_one_tables_identity_onto_another`. They test main behavior, the register/use_table race and stall recovery. Both appear to hold against S's code paths, since the facade register logic is S's.

  None maps to X1–X10. The renewable ones can be argued as X7-adjacent, because main's credential model is forced. The two `mod sdk` tests are plain main tests. Per the AC-028 rule ("no main … test except X1–X9") they are findings unless kept by ruling.

- **D-S5 `wyrd-client/src/bifrost/facade.rs`, `client_from_options_in`**
  It adds blank-argument refusal (`WYRD_CLIENT_400_CONFIG_INVALID`) and global-config-file resolution, with 2 new unit tests. This is main client config behavior (non-Bifrost owners `environment`/`global_config`), forced by API, but the blank-refusal check is new behavior on a Bifrost-scope door. Low severity. Record it as main-forced.

- **D-S6 `wyrd-client/src/bifrost/query.rs:1920`, describe fixture metadata**
  The fixture changes `wyrd:input_class: gate_correlation` to `PARQUET:field_id: 1001`. Neither value is emitted by the server in S or HEAD (`git grep` finds `1001` only in client fixtures), so this is fixture-only. It is low severity, but the fixture no longer mirrors S's describe output for the correlation column. Confirm against the actual X1 describe output.

- **D-S7 `architecture/bifrost-design.md`, coverage gap (not a hunk)**
  The design doc has no prose for X3 (Scribe peer routing beyond the capture paragraph), X4 (role heartbeat / one membership poller), X6 (typed bind params), or X7 (API-key OTLP auth, TenantConn/OperatorPool); `grep` finds none of these terms. REQ-027 asks the owning architecture to reflect changed contracts, so the change review should decide whether these belong in the doc.

Checked and clean in this part:
- No `Box::pin` was added (the `Box::pin(async move …)` in `otlp.rs` is S's existing tower `call` shape).
- No status-code change: capture refusals are still `PermissionDenied`/`InvalidArgument`, and the API-key refusal maps through `wyrd_error_to_status`.
- No timeout change in production code. The test-only `settle(30 s)` is new with the X2 fixture; `Duration::from_mins(10)` equals 600 s.
- No `#[allow]`, and no compatibility route or alias in the server files.

##### 3. Counts

- Logical hunks recorded: 52.
  - Mapped directly to an exception: 26 (X1: 4, X2: 13, X3: 2 shared, X5: 3, X6: 3, X7: 3, X8: 1, X9: 1, X10: 1; counted once per row).
  - Mapped to a forced non-Bifrost adaptation or lint: 19 (main lint, test-support, WyrdClient shape, config, credential model, layout, docs).
  - Restores S (87e9d3f48): 1.
- Unmapped / suspicious: 7 items (D-S1 to D-S7). D-S2, D-S3 and D-S4 are the substantive ones: main Bifrost-client surface and main tests with no X item. D-S1 is a lint suppression. D-S5 and D-S6 are low. D-S7 is a documentation-coverage note.

#### Part: P-028 part E1: wyrd-testing harness (src/bifrost minus forge_harness) and bin/capacity

Base S = cfe21dc9007d5d22d9ae4411be07fb5504b0cc05. Tree read at HEAD 861e6f19d. The branch moved during the audit: 861e6f19d "Delete the unread telemetry requirement and restore S's cluster docs" landed on top of 622299986.

"Main lint" below means a main commit's workspace pedantic-clippy conformance change, carried into the S file by the merge (mostly main 515f16c97 "Close out the repository check audit, pedantic lints, and test binary layout", via b356c948f). That is not an X exception. The closest authority is REQ-026, "preserve main ... pedantic rules". I count these as forced adaptations and list each behaviour-changing one under Unmapped / suspicious.

##### 1. Ledger

| File | Hunk summary | Exception / reason | Commit(s) |
|---|---|---|---|
| src/bifrost/peer_ca.rs | `.finish()` → `.finish_non_exhaustive()` in the redacted Debug | Main lint (`missing_fields_in_debug`) | 515f16c97 |
| src/bifrost/query_fixture.rs | Removes `audit: None` from the two `TableRegistration` literals. S's `bifrost_catalog.rs:122 pub audit: Option<AuditEvent>` field is gone | X2: audit is no longer carried on catalog registration; decisions go through the Scribe outbox | d2af088b9 (main), b356c948f |
| src/bifrost/query_fixture.rs | Adds a `# Panics` doc ("server started without the Bifrost catalog") | Main lint (`missing_panics_doc`) | 515f16c97 |
| src/bifrost/scribe_workload.rs | Removes `audit: None` from the registration literal | X2 | d2af088b9 |
| src/bifrost/scribe_workload.rs | Adds `params: Vec::new()` to `BifrostQueryRequest` | X6 | 82972f9f9 (main) |
| src/bifrost/scribe_workload.rs | `running_pod(&Option<_>)` → `running_pod(Option<&_>)` with `cluster.as_ref()` at 7 call sites | Main lint (`ref_option`) | 515f16c97 |
| src/bifrost/scribe_workload.rs | `owner.restart_node(node)` → `Box::pin(owner.restart_node(node))` | Main lint (`large_futures`); see S-list | 515f16c97 |
| src/bifrost/write.rs | `Correlation { card_ref: Some(..) }` → `card_uid: self.card_ref.uid.clone()`; doc says "correlated to its UID" | X1 | dadaa3ab9 (main) |
| src/bifrost/telemetry.rs | `FORGE_TASK_TYPES` gains `"manifest_rewrite"`; doc "five" → "six" strategies; test label domains gain `manifest_rewrite` (4 places) | X10: manifest rewrite runs as a durable `ManifestRewrite` task | 87e9d3f48 |
| src/bifrost/telemetry.rs | Deletes `enum TelemetryRequirement`, the `TelemetryBinding.requirement` field, every `requirement:` initialiser (≈40 bindings plus ≈30 test rows), and the unread `let _required = match ...` in `evaluate_cluster_bindings_required` | Not an exception: S dead-data removal; see S-list | 861e6f19d (main had a cfg_attr dead-code expect instead) |
| src/bifrost/telemetry.rs | `as u64` casts of aggregated samples → `whole_count(binding, v)?` (new fn), which errors on a negative, non-finite or overflowing value | Main lint (`cast_possible_truncation`/`cast_sign_loss`). Behaviour change; see S-list | 515f16c97 |
| src/bifrost/telemetry.rs | `seconds_to_micros` rewritten to `.round().to_u64().ok_or_else(..)` | Main lint. Equivalent except that a value just above `u64::MAX` after rounding is now refused instead of saturating | 515f16c97 |
| src/bifrost/telemetry.rs | `begin_gauge_sampling` `async fn` → `fn`; `_checkpoint` → `checkpoint`; caller drops `.await` | Main lint (`unused_async`, `used_underscore_binding`) | 515f16c97 |
| src/bifrost/telemetry.rs | `if kind != Gauge {..} else {..}` and `if identity != .. {..} else {..}` arms swapped (logic identical); `flat_map(\|v\| v.into_iter())` → `flat_map(IntoIterator::into_iter)`; `use num_traits::ToPrimitive` | Main lint (`if_not_else`, `redundant_closure`) | 515f16c97 |
| src/bifrost/cluster.rs | `OracleInspection.audit_pending` → `scribe_pending` (field, doc, aggregation) | X2: one Scribe outbox replaces S's Oracle audit outbox | 70a6f4a4e (main), b356c948f |
| src/bifrost/cluster.rs | `add_tenant` ensures built-ins through `BuiltinTables::new(catalog).ensure_tenant(tenant)` (new import) | X5: eager provisioning | 1090580f2 (main) |
| src/bifrost/cluster.rs | New `await_audit_retained()` loops over servers | X2 | 70a6f4a4e / d47c7fafb (main) |
| src/bifrost/cluster.rs | New `stop_node_restarting_another_during_drain(drained, restarted)`, used by `tests/bifrost/server/query.rs:810` (`oracle_only_pod_retains_audit_staged_before_its_drain`) | X2 + X4: membership keeps refreshing through drain so the outbox finds a late Scribe (REQ-008/REQ-010) | 3712b9ba3 |
| src/bifrost/cluster.rs | `fixture.superuser_pool().await` → `superuser_pool()` (sync) | Main-side API (wyrd-dev-fixtures `PgFixture`; X7 SQL capability work) | b356c948f / 597533616 |
| src/bifrost/cluster.rs | 30+ `Box::pin(...)` wraps around `start_spec*`, `start_with_*`, `build_node`, `restart_node`, `start_with_resources`, `shutdown_and_inspect`, and 3 in-file tests | Main lint (`large_futures`); see S-list | 515f16c97 |
| src/bifrost/cluster.rs | `#[expect(clippy::float_cmp, reason = ..)]` on `OracleTelemetryCapture::delta_since` | Main lint; see S-list (an `#[expect]` was added) | 515f16c97 |
| src/bifrost/cluster.rs | `Duration::from_secs(60/600)` → `from_mins(1/10)` (×3); `match bool` → `if/else`; `# Panics` doc on `terminate_node_abruptly_for_test` | Main lint (`duration_suboptimal_units`, `match_bool`, `missing_panics_doc`); values unchanged | 515f16c97 |
| bin/capacity/query/{logs,report,run,telemetry,workload}.rs | S `bin/bifrost_query_capacity/*` moved under `capacity/query/`; `crate::X` → `crate::query::X` | Spec "Benchmarks": compile into main's one capacity build | 87e9d3f48 |
| bin/capacity/query/mod.rs | S `main.rs` → `mod.rs`. `#[tokio::main] async fn main()` reading `env::args()` → `pub(crate) async fn main(arguments: Vec<String>)`. Docs, consts, output paths and the rest unchanged | Benchmarks (dispatch glue) | 87e9d3f48 |
| bin/capacity/query/run.rs | `params: Vec::new()` in 2 `BifrostQueryRequest` literals | X6 | 87e9d3f48 |
| bin/capacity/query/*.rs | `as usize/u64/i64` → `.to_usize().unwrap_or(0)`, `.cast_unsigned()/.cast_signed()`, `.to_i64().unwrap_or(i64::MAX)`; `Role::ALL: [Self; N]` → `&[Self]` plus `ROLES` const; manual hex loop → `hex::encode`; `AsArray as _` → `AsArray` and `.map(AsArray::as_primitive)`; `assert_eq!(f64, f64)` → `.to_bits()` compare | Main lint (`cast_*`, `float_cmp`, `redundant_closure`). Values identical for all in-range inputs; see S-list | 87e9d3f48 |
| bin/capacity/client_queue/{fixture,proxy,step,mod}.rs | S `bin/bifrost_client_queue/*` moved; `crate::Result` → `crate::client_queue::Result` | Benchmarks | 87e9d3f48 |
| bin/capacity/client_queue/mod.rs | `main()` → `pub(crate) main(arguments)` with `Cli::parse_from(arguments)`; final summary built with `push_str` instead of `format!` (byte-identical output) | Benchmarks glue + main lint (`format_push_string`) | 87e9d3f48 |
| bin/capacity/client_queue/fixture.rs | `.sql_as(sql)` → `.sql_as(sql, &[])` | X6 | 87e9d3f48 |
| bin/capacity/client_queue/step.rs | `WyrdState::from_path(bundle)` + `start_bifrost_with_config(&client, ..)` → `WyrdState::from_path_with_client(bundle, client)` + `start_bifrost_with_config(None, ..)` | Main-side client API (main `state.rs:490`) | 87e9d3f48 |
| bin/capacity/client_queue/step.rs | Cast rewrites (`to_u64().unwrap_or(0)`, `to_f64().unwrap_or(0.0)`, `try_from(..).unwrap_or(u64::MAX)`), `==` on f64 → `total_cmp().is_eq()`, `format!` arg inlined, slope assert → `.to_bits()` | Main lint; see S-list | 87e9d3f48 |
| bin/capacity/{main,evidence,fixture,judge,load,profile,report,step}.rs | Main's consolidated `bench:capacity` (verification mix, not a Bifrost workload). evidence/fixture/judge/profile/report are byte-identical to main | Main non-Bifrost file, kept beside S's workloads per spec | main |
| bin/capacity/main.rs | Adds `mod query; mod client_queue;`, crate docs for the named workloads, and a `main()` that dispatches argv[1] `query-capacity` / `client-queue` (removing the subcommand so flags parse as before), else `capacity()`. Each branch is `Box::pin`ned | Benchmarks dispatch glue | 87e9d3f48 |
| bin/capacity/main.rs | `LocalServer::start(.., Envelope::POD)` → `Placement::Scoped(Envelope::POD)` (×2) | S `release_server::Placement` API (REQ-026 Host vs scoped placement) | 597533616/cac9c4969 |
| bin/capacity/load.rs | Drift SQL `wyrd_event_time` → `created_at` (3 places) | Forced by S schema (REQ-017: managed `wyrd_event_time` removed, drift axis `created_at`) | 4f9a5c1d1 |
| bin/capacity/step.rs | Local `fn cpu_usec(replica)` deleted; uses S's `LocalServer::cpu_usec` | S release_server API | 597533616/cac9c4969 |
| mise.toml (context, read-only) | `bench:bifrost:query-capacity` and `write-capacity` build `--bin capacity` and run `capacity query-capacity --tier ..`; `client-queue` runs `--bin capacity -- client-queue`. Only the binary name and subcommand differ from S; flags, tiers, the `--heavy` branch and env are unchanged | Benchmarks | 87e9d3f48 |

S's `bin/verification_capacity/*` (8 files) is absent at HEAD. Main replaced it with `bench:capacity` (main dec4213fa). It is not a Bifrost workload and is not in the P-028 path list.

##### 2. Unmapped / suspicious

S1. **telemetry.rs: S's `TelemetryRequirement` deleted (861e6f19d).** Removed: `enum TelemetryRequirement { Always, Role(&'static str) }`, the field `TelemetryBinding.requirement`, ≈70 initialisers, and S's `let _required = match binding.requirement {..}` (old ~L1746). The data was unread in S (only `_required`), so behaviour is unchanged. Still, it removes an S structure under no exception. Main kept the field behind a cfg_attr dead-code expect, so this is neither S nor main. Severity: low. Record it as a deliberate deviation, or restore S's field (it would need main's lint expect).

S2. **telemetry.rs ~L2244 `whole_count`, used at the Delta, peak and final_value aggregation sites.** `selected..sum::<f64>() as u64` became `whole_count(binding, ..)?`. S saturated: a negative value became 0, NaN became 0, overflow became `u64::MAX`. HEAD returns `InvalidBinding("metric value is not a representable count")` instead. A Forge telemetry report that S accepted can now fail. This is a behaviour change from main lint, under no exception. Severity: medium (test-harness reporting only).

S3. **telemetry.rs `seconds_to_micros` (~L2542).** S checked `micros.round() > u64::MAX as f64`; HEAD uses `.to_u64()` on the rounded value. These are equivalent except at the boundary. Severity: negligible. Listed only because it changes S code.

S4. **`#[expect(clippy::float_cmp, reason = "Prometheus renders these metrics as whole numbers ...")]` added at cluster.rs:647** on `OracleTelemetryCapture::delta_since`. An added lint expectation in S code (main lint). Not an exception. Severity: low.

S5. **`Box::pin` added through the harness.** cluster.rs has 30+ sites; scribe_workload.rs:1560 has `Box::pin(owner.restart_node(node))`; bin/capacity/main.rs has the 3 dispatch branches. They come from main's `large_futures` lint, are not in S, and are not an exception. Behaviour is unchanged (heap allocation only). The parent listed "Box::pin in tests" as a flag. Severity: low.

S6. **bin/capacity cast rewrites alter S's overflow semantics** (spec: S's workloads must be restored exactly). Examples:
   - query/workload.rs:300,302,315,320: `as usize` → `.to_usize().unwrap_or(0)`.
   - query/run.rs:177-180, 864, 891, 946, 1000.
   - query/telemetry.rs:441, 452: `EXCEPTIONS[(index % len as i64) as usize]` → `EXCEPTIONS[index.to_usize().unwrap_or(0) % len]`. For negative `index` this picks a different element (S wrapped; HEAD uses 0). Indexes are non-negative in practice.
   - query/telemetry.rs:625, 638, 909: `.to_i64().unwrap_or(i64::MAX)`.
   - query/logs.rs:183.
   - query/report.rs:581.
   - client_queue/step.rs:207 (`offered` → 0 on a NaN/negative rate).
   - client_queue/step.rs:219-222: `((sequence as usize + feature*7) % 100) as f64` → `((sequence + feature as u64 * 7) % 100).to_f64().unwrap_or(0.0)`. Same value for in-range inputs.

   For every in-range input these give the values S gives, so reports and assertions are unchanged. Edge semantics differ. Severity: low. They are main-lint adaptations, not exceptions.

S7. **query/telemetry.rs:186 `Role::ALL: [Self; SPANS_PER_TRACE as usize]` → `pub const ALL: &[Self]`, plus new `ROLES` const; query/telemetry.rs:640 manual `{byte:02x}` loop → `hex::encode`.** Same output: `hex::encode` is lowercase. But the array type no longer pins its length to `SPANS_PER_TRACE` at compile time. Severity: low.

S8. **Float-equality changes in S code.**
   - client_queue/step.rs:154: `other == bound` → `other.total_cmp(bound).is_eq()`. This differs for -0.0 vs 0.0 and for NaN.
   - query/run.rs:1329 and client_queue/step.rs:423: test asserts now compare `.to_bits()`. That is stricter (a -0.0 slope now fails), not weaker.
   Severity: negligible.

S9. **Out-of-path S/Bifrost files that differ from S and are not covered by the P-028 path list.** I did not ledger them, but S's restored workloads depend on them:
   - `crates/wyrd/wyrd-testing/src/release_server.rs`: +234/−59 vs S. `migrate` no longer uses `WYRD_TEST_DATABASE_ADMIN_URL` (main external Postgres credentials, REQ-022/X7). There is a new `OperatorRun` with stderr files, new `JOINED_GRPC_PORT=30051`/`JOINED_PEER_PORT=30052` replica ports, `READY_TIMEOUT` written as `from_mins(2)` (same value), `pub STOP_GRACE`, `pub Metrics::parse`, and abnormal-stop log retention.
   - `src/capacity.rs`: percentile cast → `.to_usize().unwrap_or(0)`.
   - `src/profile.rs`: `push_str(&format!)` → `writeln!`.
   - `bin/bifrost_forge_capacity/report.rs`: new `#[expect(clippy::float_cmp)]` on `Report::checks`.

   Recommend that P-028 extend the path list to `crates/wyrd/wyrd-testing/src/{release_server,capacity,profile}.rs` and `src/bin/bifrost_forge_capacity`, or record why they are out of scope. The release_server port and migrate changes feed every S benchmark, which bears on the "S's exact inputs ... output paths" claim.

No unmapped hunk in this part reintroduces a struck main mechanism (audit staging, publisher, maintenance lease), adds a compatibility shim, or changes a timeout, deadline or HTTP status. The `from_mins`/`from_secs` rewrites keep their values.

##### 3. Counts

- Files in E1 scope with diffs: 7 harness files (cluster, peer_ca, query_fixture, scribe_workload, telemetry, write; forge_harness excluded) and 18 capacity files.
- Logical hunk groups in the ledger: 37. Mapped to an X exception or a forced main-side API: 15 (X1 ×1, X2 ×5, X5 ×1, X6 ×4, X10 ×1, main API ×3). Benchmarks relocation/glue or main-owned capacity: 9. Main pedantic-lint conformance (forced-adaptation class, REQ-026): 13.
- Unmapped / suspicious: 9 items (S1–S9).
  - Medium: 1 (S2).
  - Low: 5 (S1, S4–S7).
  - Negligible: 2 (S3, S8).
  - Scope gap: 1 (S9).
  - None reintroduces a struck mechanism.

#### Part: E2 - Bifrost journeys ledger (oracle/, otlp/, scribe/, server/)

Scope: crates/wyrd/wyrd-testing/tests/bifrost/{oracle,otlp,scribe,server}/** (forge/ excluded).
Method: per file `git diff S HEAD`, `git diff main HEAD`, `git log S..HEAD`. Read-only.
"lint" = forced by main's workspace clippy pedantic config (515f16c97): Box::pin (large_futures),
Duration::from_mins/from_hours, cast lints, #[expect(clippy::float_cmp)], method-path closures.

##### 1. Hunk table

| File | Hunk summary | Exception / reason | Commit(s) |
|---|---|---|---|
| oracle/analytical_activation.rs | AnalyticalCleanupPause hold/release test removed; renamed transport_drop_retains_running_status_until_cleanup_joins -> transport_drop_retires_and_releases_the_graph | X8 | 87e9d3f48 |
| oracle/analytical_activation.rs | physical_build_evidence `?` dropped (infallible); client_from_bootstrap sync; params | peer_cluster refactor (main); X6 | 87e9d3f48 |
| oracle/analytical_lifecycle.rs | Box::pin x3; params | lint; X6 | 515f16c97 era |
| oracle/capacity.rs | from_mins, Box::pin, superuser_pool sync | lint | - |
| oracle/capacity.rs | `audit: None` removed from CreateTableRequest | X2 | - |
| oracle/capacity.rs | 2x #[expect(cast_precision_loss)] removed, #[expect(float_cmp)] added | lint | - |
| oracle/capacity.rs | prove_queue_is_fifo_and_tenant_rotated rewritten (completion-order Mutex -> granted_tx grant-order via stall_next_query_after_schema) | UNMAPPED (see 2) | f6c841d57 |
| oracle/distributed.rs | params; timeouts from_mins (same value); `_ =>` -> explicit HotAuthority::Published; RowGroupMetaData method path | X6; lint | - |
| oracle/distributed.rs | `.describe(&fqn)` -> `.describe_table(ns, name)` + expect | main client facade (fork D scope) | 6f25f42b4 |
| oracle/main.rs | `mod workflow;` | main-only journey registration | 114ae55e8 |
| oracle/mcp.rs | token request JSON WyrdApiKey -> RFC token-exchange form | X7 | d841b9b46 |
| oracle/mcp.rs | `release_execute_pause(paused)?` after cancel removed | X8 | c63e2ff46 |
| oracle/peer_cluster.rs | MEMBERSHIP_DEADLINE/await_membership moved in from join.rs; start_with_gateway_provider_root + Url launch param; terminate releases a held execute pause | main workflow.rs support | 114ae55e8, f047831b8, 304166d7f, 047d8bebd |
| oracle/peer_cluster.rs | metric_totals_labeled skips HistogramBucket/HistogramSum samples | UNMAPPED semantic change (see 2) | 047d8bebd era |
| oracle/peer_cluster.rs | physical_build_evidence infallible; READY/STATEMENT_DEADLINE from_mins (same); `audit: None` removed | refactor; lint; X2 | - |
| oracle/peer_network/analytical.rs | Box::pin x3; #[expect(float_cmp)]; usize::try_from INGEST_CHUNK; params | lint; X6 | - |
| oracle/peer_network/join.rs | uses cluster.await_membership; Box::pin; params | refactor; lint; X6 | - |
| oracle/peer_network/listener.rs | new a_frame_above_the_tonic_default_is_acknowledged_and_retained (6 MiB audit row over mTLS peer capture) | X3/X2 (REQ-009 decode ceilings) | b7b2de753 |
| oracle/peer_network/listener.rs | Box::pin; params | lint; X6 | 7d3eb319a |
| oracle/peer_network/security.rs | `ADAPTERS.iter()` -> `&ADAPTERS` | lint | - |
| oracle/published.rs | from_mins; `.sql(&sql,&[])`; #[expect(float_cmp)] | lint; X6 | - |
| oracle/published.rs | WORKLOAD_ROLE -> seeded role variant_sql_no_payload (bifrost_table_read + bifrost_query_read) | X7 | 1678da1ac |
| oracle/support.rs | `audit: None` removed; seed_foreign_hot_row drops AuditEvent + audit_staging::append_audit | X2 | - |
| oracle/support.rs | client_from_bootstrap sync; #[allow(cast_sign_loss)] removed for cast_unsigned; write!; COMPACTION_BUDGET from_mins(3) | lint | - |
| oracle/workflow.rs | NEW main-only: forwarded Workflow bifrost.query settles through cancel / deadline / pod loss | main Workflow product surface consuming Bifrost, + X8 (see 2) | 114ae55e8, c63e2ff46, c6a3276ad, 7ad33ac2d |
| otlp/negative.rs | clone_into | lint | - |
| otlp/negative.rs | new card_attributed_resource_spans + unbound_writer_attributes_spans_to_registered_cards_only | X1 | dadaa3ab9, b5ab2dd5d |
| otlp/support.rs | stores api_key; start_on(builder); api_key(); params | X7; X6 | 53d044191 |
| otlp/trace_export.rs | new stock_exporter_authenticates_with_api_key_after_token_lifetime | X7 (API-key OTLP auth) | 53d044191 |
| scribe/ambiguous_retry.rs | NEW: re-home of S pg_bifrost_e2e public_sdk_owned_batch_timeout_retry_retains_then_deduplicates; card_uid | S test re-homed; X1 | 6f25f42b4, dadaa3ab9 |
| scribe/describe.rs | NEW: re-home of S described_canonical_and_dynamic_schemas_reach_exact_physical_schema; describe_table; writable_schema path; is_extension_key | S test re-homed; X1; Q13 | 6f25f42b4, 005c19b1e, dadaa3ab9, 87e9d3f48 |
| scribe/describe.rs | card_ref INPUT_CLASS metadata assertion -> nullable Utf8 card_uid check | X1 | dadaa3ab9 |
| scribe/describe.rs | vala.file_list row_count==2 durable assertion dropped | UNMAPPED weakening (see 2) | 6f25f42b4 |
| scribe/backpressure.rs, qualification.rs, horizontal_ingest.rs | lint casts, from_mins; `audit: None` removed; params | lint; X2; X6 | - |
| scribe/budgets.rs | start_scribe_server -> builder; await_audit_retained; journey_buckets filter | X2 | f6c841d57, 70a6f4a4e, e03e3b9c8 |
| scribe/lifecycle.rs | .without_audit_publication_for_test() x5 removed; faults.target_table_for_test; staging_backlog(Some(name)) | X2 | f6c841d57 |
| scribe/lifecycle.rs | idle-clock rationale (audit publisher as other writer) deleted, no replacement guard | X2, guard gap (see 2) | f6c841d57 |
| scribe/main.rs | mod ambiguous_retry; mod describe | re-home | 6f25f42b4 |
| scribe/recovery.rs | params; `audit: None`; Box::pin(restart_bound) | X6; X2; lint | 5e7b177b0 |
| scribe/source_boundary_recovery.rs | target_table fault; async assert_no_leaked_scribe_ownership; memory==0 -> memory==audit_log bucket bytes | X2 (weakened, see 2) | f6c841d57 |
| scribe/support.rs | `audit: None`; params; ADMISSION_DEADLINE from_mins(1) | X2; X6; lint | - |
| scribe/sustained.rs | "lazy built-in" wording removed; await_audit_published -> await_audit_retained; staging_backlog(None); lint casts | X5; X2; lint | - |
| scribe/telemetry.rs | without_audit_publication removed; settled_writable_rows / audit_files helpers; #[expect(float_cmp)] x2 | X2; lint | f6c841d57, 70a6f4a4e, e03e3b9c8 |
| scribe/telemetry.rs | exact metric counts -> ranges / audit-inclusive totals | X2 (weakened, see 2) | same |
| scribe/write_read.rs | Box::pin x3; superuser_pool sync; params; from_mins | lint; X6 | - |
| scribe/write_read.rs | card_ref -> registry card_uid; refusal uses random UUID; new scribe_unbound_writer_card_attribution_journey | X1 | b5ab2dd5d, dadaa3ab9 |
| server/main.rs | audit_publication -> audit_retention; + observe_ingest, query_lifecycle | X2; re-home; main journey | - |
| server/audit_publication.rs | DELETED (5 S tests: frozen range replay, reads during chain wait, stalled tenant isolation, service decisions ahead, retire-only-into-retained) | X2 (staging/publisher/chain removed) | 1e57da5cb |
| server/audit_retention.rs | NEW: retained_history_carries_both_credential_shapes, system_owner_security_rejections_retain_once | X2 (REQ-011/012) | 1e57da5cb |
| server/query_lifecycle.rs | NEW: re-home of S oracle_query_grpc_status_cancel_impl as grpc_lifecycle_controls_are_tenant_scoped_and_audited; denied_describe_is_audited_before_admission; staged -> retained audit | S test re-homed; X2 | 6f25f42b4 |
| server/owner_inspection.rs | `bytes[60] ^= 0xff` fixture + doc | Q1 | 87e9d3f48 |
| server/owner_inspection.rs | Box::pin x2; from_mins; `audit: None` | lint; X2 | - |
| server/owner_inspection.rs | new unready_roles_heartbeat_until_teardown_ends_membership_tasks | X4 | f6c841d57 |
| server/owner_inspection.rs | new dedicated_forge_worker_shutdown_drains_its_claim_before_storage_settles, router_only_shutdown_settles_bifrost_before_fixture_release | UNMAPPED main Bifrost shutdown behaviour (see 2) | 5eff7ec29, a14896e34 |
| server/query.rs | await_audit_published -> retained; `audit: None`; Box::pin x3; params; Correlation::default() | X2; lint; X6 | - |
| server/query.rs | AUDIT_UNAVAILABLE fail-closed test removed | X2 (no audit-write error exists) | - |
| server/query.rs | audit count moved before pause; 30 s wait for victim to leave membership | X2/X4 | 21d02b174, e069dbcdc |
| server/query.rs | new oracle_only_pod_retains_audit_staged_before_its_drain | X2/X4 | 3712b9ba3 |
| server/query.rs | prove_scheduled_analytical_completion: pause.release() removed, 502 STREAM_INCOMPLETE -> 500 EXECUTION_FAILED | UNMAPPED status change (see 2) | 5dfee4d3f |
| server/eval_verification.rs | HEAD = main verification product journeys on S time axes (created_at / wyrd_ingested_at, context->>'marker', RowPreflight) | main non-Bifrost verification surface (REQ-014/015), consumes Bifrost | main + adaptation |
| server/eval_verification.rs | `assertion_result ->> field` | Q6 / X9 | - |
| server/eval_verification.rs | SECURITY DEFINER trigger | Q7 / X7 | - |
| server/eval_verification.rs | eval_runs_follow_the_writing_owner, direct_task_judgment_records_one_correlated_result | X1/X9 | - |
| server/eval_verification.rs | S integrated_enqueue_failure_preserves_ack + blocked_activations -> main integrated_enqueue_outage_preserves_ack_and_recovers; EvalReadAuthority checks, direct ProviderRegistry runtime, emit-window assertion removed | REQ-015 gateway (main product); partial weakening (see 2) | - |
| server/verification_runtime.rs | runner ..._through_the_ingest_endpoint -> ..._through_a_peer_scribe; outbox settle; StraddleScribe; SYSTEM token + facade writer -> ScribeSink::local_outbox | X2/X3 | - |
| server/verification_runtime.rs | `unrouted.composes(RuntimeCapability::Runner)` negative check removed | X2/X3 (see 2) | - |
| server/verification_runtime.rs | `{RUN_ID} = '{run}'` -> `{RUN_ID} IS NULL`; RUN_ID join dropped; ResultLayoutJourney run_id: None x2 | UNMAPPED vs REQ-016 (see 2) | ad27e71f3 |
| server/verification_runtime.rs | audit seq/prev_hash/entry_hash recompute -> detail-only check | X2 | - |
| server/verification_runtime.rs | card_ref -> card_uid | X1 | - |
| server/verification_runtime.rs | agent_traces -> vala.eval.observations (messages/tool_io -> context/media) | X5 | - |
| server/verification_runtime.rs | WAIT from_mins(1), DAY from_hours(24) | lint | - |
| server/observe_ingest.rs | NEW main-only: client emit-time, drift burst under byte budget, sustained 100-feature drift exactly-once; adapted: wyrd_event_time -> created_at, dataset record() emits + caller-supplied event time case removed | main observe SDK surface consuming Bifrost; X5 time axes (see 2) | 15155ba5d, 753916bd0, 3e9b80458, 4f9a5c1d1 |

##### 2. Unmapped / suspicious

1. oracle/capacity.rs prove_queue_is_fifo_and_tenant_rotated - completion-order Mutex assertion replaced by `stall_next_query_after_schema` + `granted_tx` grant-order. From f6c841d57 (TASK-003 r4 audit scoping). Not an exception mechanism, no Q row. The test now proves something different (grant order vs completion order); needs a recorded reason or revert to S's assertion.
2. oracle/peer_cluster.rs metric_totals_labeled - now excludes HistogramBucket/HistogramSum samples. Changes the meaning of an S helper used by S assertions; no exception covers it. Check every S caller still sums what it meant to.
3. oracle/workflow.rs (whole file, 671 lines, HEAD == main) - main Workflow product journey. Justifiable as a non-Bifrost surface (Workflow tool) consuming Bifrost query forwarding, with X8 cancellation. But it asserts Bifrost settlement semantics (`POD_LOSS_CODE = "WYRD_VALA_500_QUERY_EXECUTION_FAILED"` at :327, `QUERY_TIMEOUT_CODE` 504 at :64, leader graph cleanup holding the run non-terminal) and pulls peer_cluster changes (execute pause released on terminate). Confirm these codes match S's Oracle; otherwise finding.
4. otlp/trace_export.rs stock_exporter_authenticates_with_api_key_after_token_lifetime - readback `... WHERE scope_name = '{}' OR start_time_unix_nano = {start}` compares a Timestamp(ns) column with an integer literal. Q8 established DataFusion refuses Timestamp(ns) vs Int64; likely a planning failure unless this predicate coerces. Run it.
5. scribe/describe.rs - S's durable `vala.file_list` row_count==2 assertion removed in the re-home; only SQL readback remains. Weakened, not exception-justified (X1 only touches card_ref). The INPUT_CLASS_KEY metadata check -> nullable Utf8 is X1 and fine.
6. scribe/ambiguous_retry.rs - durable check file_list row_count==1 -> sorted_values==[41]; `max_message_bytes: 32 MiB` dropped. Roughly equivalent (dedup still proven) but the physical single-file proof is gone. Minor.
7. scribe/telemetry.rs - exact asserts loosened: ACK `== 4.0` -> `acks >= 4.0 && acks - 4 <= audit_rows`; retry ACK `1.0` -> range; inserted `0.0`/`64.0` -> `audit_rows`/`inserted_rows`; publication files/bytes now include audit_log deltas. X2 makes audit share Scribe, so some looseness is forced, but filtering to the journey table (as budgets.rs does via journey_buckets) would keep exact counts. Weakened.
8. scribe/source_boundary_recovery.rs assert_no_leaked_scribe_ownership - `memory[Active/Immutable] == 0` -> `== bytes held by audit_log buckets`. A leak inside an audit bucket is undetectable. Weakened under X2.
9. scribe/lifecycle.rs idle-clock test - S rationale ("the audit publisher is the one other writer in the pod, and its generations would publish the table without the tick") deleted. Under X2 the outbox still writes audit_log through the same Scribe; no replacement guard that the observed publication is the tick's and not an outbox flush. Guard gap.
10. server/owner_inspection.rs dedicated_forge_worker_shutdown_drains_its_claim_before_storage_settles, router_only_shutdown_settles_bifrost_before_fixture_release (5eff7ec29, a14896e34) - main Bifrost shutdown-order/harness behaviour (REQ-023), not an exception. Forge claim test also touches main Forge design (struck). Finding. Also main->HEAD dropped main's `metadata_cache_enabled()` assertion (outside S diff; informational).
11. server/query.rs prove_scheduled_analytical_completion - `pause.release()` removed and expectation `WYRD_VALA_502_QUERY_STREAM_INCOMPLETE` -> `WYRD_VALA_500_QUERY_EXECUTION_FAILED` (main 5dfee4d3f "expect the owner's failure for a scheduled query cancelled while opening"). 502->500 change. X8 records cancel-before-first-batch as `cancelled`; this is neither S's code nor clearly X8. Finding unless X8 text covers it.
12. server/verification_runtime.rs - result rows queried with `{RUN_ID} IS NULL` (was `= '{run}'`), RUN_ID join dropped, ResultLayoutJourney `run_id: None` x2 (ad27e71f3). REQ-016 says result run_id is the application Run being judged. `run_id` None in results; verify against X2/ad27e71f3 or finding.
13. server/verification_runtime.rs - S negative "a node without Scribe or an endpoint composed no runner" (`unrouted.composes(RuntimeCapability::Runner)`) removed. Under X2 every node has the outbox, so arguably unreachable, but no replacement negative for an unroutable outbox. Record reason.
14. server/eval_verification.rs - S EvalReadAuthority::resolve checks and the emit-window assertion removed with the main replacement. Main product journey, but the authority negative flow has no equivalent visible in HEAD. Check main's test covers read-authority refusal.
15. server/observe_ingest.rs - main observe SDK journeys (justified as non-Bifrost client surface consuming Bifrost), but drift_burst_survives_a_byte_budget_override / sustained_..._exactly_once assert Scribe exactly-once and byte budgets, i.e. Bifrost ingest behaviour. Adaptation removed the dataset `record()` path and the caller-supplied event-time case outright rather than mapping to S time axes. Borderline finding.
16. server/audit_publication.rs deleted - all 5 tests are staging/publisher/chain mechanics removed by X2, except the property "a stalled tenant does not block another tenant's history": the outbox batches per tenant slice, but no audit_retention journey proves cross-tenant isolation. Coverage gap.
17. oracle/distributed.rs, scribe/describe.rs - depend on `describe_table(namespace, name)` (main client API) instead of S `describe(fqn)`. Mapped only if fork D accepts the facade rename.
18. oracle/analytical_activation.rs - AnalyticalCleanupPause test removed under X8, but the pause still exists in redux src (oracle/analytical.rs:2517, :2884; oracle/mod.rs:3940). Either dead test hook (delete) or untested path.
19. Mechanical lint changes (not findings, listed per directive): Box::pin added in analytical_lifecycle, capacity, peer_network/{analytical,join,listener}, recovery, write_read, owner_inspection, query, workflow; #[expect(clippy::float_cmp)] added in capacity, peer_network/analytical, published, telemetry x2; #[expect(cast_precision_loss)] x2 and #[allow(cast_sign_loss)] removed (replaced by checked/unsigned casts). No new #[allow], cfg_attr, or #[ignore] beyond the lane-standard `#[ignore = "requires ..."]` on new journeys. Timeouts converted to from_mins/from_hours keep identical values; the only new bound is query.rs' 30 s membership-leave wait (X4).
20. Out of scope, for the parent: S crates/shared/wyrd-client/tests/pg_bifrost_e2e.rs tests not found anywhere in HEAD: multi_batch_query_stream_reuses_schema, public_sdk_bidi_write_ack_and_durable_readback, observe_and_bifrost_roundtrip, backpressure_and_drain_no_silent_drops, canonical_signal_arrow_write_and_sql_read_round_trip.

##### 3. Counts

- Files reviewed: 41 (oracle 15, otlp 3, scribe 15, server 8 incl. 1 deleted).
- Table rows: 72.
  - Exception-mapped (X1-X9) or re-homed S tests: 46
  - Lint-only: 13
  - Q fixes: 3 (Q1, Q6, Q7; Q13 within describe re-home)
  - Main-only / unmapped rows: 10
- New main-only journey files: 3 (oracle/workflow.rs, server/observe_ingest.rs, server/audit_retention.rs [X2-justified]); re-homed S files: 3 (scribe/ambiguous_retry.rs, scribe/describe.rs, server/query_lifecycle.rs).
- Deleted S tests: 7 in scope (5 audit_publication [X2], 1 AnalyticalCleanupPause [X8], 1 AUDIT_UNAVAILABLE [X2]) + 2 eval_verification replaced; 5 out of scope missing.
- Suspicious / unmapped items: 18 (items 1-18; 19-20 informational).
  - Weakened assertions: 6 (items 5, 6, 7, 8, 13, 14)
  - Status code change 502->500: 1 (item 11)
  - run_id None: 1 (item 12)
  - Main Bifrost behaviour without exception: 3 (items 1, 2, 10) + 2 borderline (3, 15)
