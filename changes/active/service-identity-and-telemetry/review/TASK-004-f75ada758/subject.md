# TASK-004 cumulative remediation re-review subject

- Root: `/home/thorrester/Documents/GitHub/wyrd-worktrees/service-identity-and-telemetry-task-004`.
- Original base: `3e7c59db7a0f5e9506273eab826875cc8b59f226`.
- Prior reviewed candidate / remediation base: `c4dd61029a54c44017c63bd4ff469472c406bc13`.
- Immutable new candidate: `f75ada758827266ac0b25a05d6a7b530f86d845a`.
- Full cumulative subject: `git diff 3e7c59db7...f75ada758`. Remediation locator: `git diff c4dd61029...f75ada758`. Read the complete range yourself; latest fixes do not replace cumulative acceptance.
- Approved spec: `changes/active/service-identity-and-telemetry/spec.md`, revision 13, and `supplements/table-storage-and-correlation.md`. Revision 13 supersedes revision-12 composite recipe semantics. Original task: `tasks/TASK-004-shared-table-storage.md`.
- Prior reports/verdict/ledger and R1/R2 tasks: `changes/active/service-identity-and-telemetry/review/TASK-004-c4dd61029/`. Read `verdict.md`, `findings-validation.md`, `TASK-004-R1-retained-storage-authority.md`, and `TASK-004-R2-single-table-version.md`. Preserve FIND-TASK-004-1 through -7; assign new IDs only after independent validation. R2 supersedes R1 correction, not original obligations unaffected by revision 13.
- No CodeGraph directory exists. Use repository tools. Reports only in this new review directory; no source edits, diff archives, patches, commits or pushes.
- Initial working tree has unrelated benchmark authoring: modified `changes/active/opitimization-and-benchmarks/spec.md` and untracked `changes/active/benchmark-experiments/`. Preserve and exclude these uncommitted artifacts; use committed authority for this subject. The user has another verification run ongoing; do not run Cargo-backed checks concurrently. No benchmarks are authorized.

## Integration provenance

The user explicitly includes merge `dfa16bb54ff904ad959ce12f522c789f9d417610` of `change/verifier-response-data` (parents `3b0f6d6126500a620a9029245b7bc5e9a407dc43` and `423fc3d94ae5aa9a4d0c3d799c0b8d156ed57192`). The cumulative diff includes those approved integrated changes. Inspect the full range and relevant integration seams; do not treat an explicitly supplied merge as unauthorized scope drift or review unrelated task completion as TASK-004 acceptance. For attribution, `git diff c4dd61029...3b0f6d612` isolates pre-merge remediation and `git diff 3b0f6d612 f75ada758` locates integration changes. Schemar 1.x, generated TS, flat Card identity, dependency/workspace-hack and client/binding seams warrant attention. No material TASK-004 regression may be waived because the merge supplied it.

## Common starting navigation map

Expand and verify this map independently; it is not a conclusion or coverage allowlist.

| Changed boundary | Existing owners / likely consumers | Relevant proof |
| --- | --- | --- |
| One positive complete table version | `wyrd-spec/src/vala` TableVersion, API/errors; redux catalog layout/physical identity/versioned descriptor; `catalog/bifrost_catalog.rs` and `iceberg_sql.rs`; native fork SQL catalog CAS/reference publication | R2 domain/codec tests, create/describe, exact decimal JSON/int64/TS bigint projections |
| Admission, version checks and retry identity | Gate/grpc/HTTP native frames; `scribe/{ingress,preprocess,committed_batches,shards,wal,replay,seal_key,write_recipe}.rs`; vala-sql batch fence queries/migration and grouped multi-row insertion/reconciliation | stale-write refusal, warm/cold accepted retries, canonical attempts and replay, operation counts/no extra normal SQL |
| Version-bearing staging/publication/read sources | assembly/hot stage/staging runtime/member/claims/file-list/promotion and memtable/hot source/tail capture | retained Bloom/schema/spec/sort resolution, crash/restart, complete-key captures, previous FIND-2/4/5 closures |
| Native retained definitions and maintenance | pinned Iceberg SQL catalog creation/update/CAS/retained references; Forge `expire`, `orphan_gc`, promotion/publication; pinned compaction grouping | native fork tests; historical definition retention; previous spec-grouping closure; actual output truth |
| Client bind/record/drain and language projections | `wyrd-client::Bifrost::{bind_table,resolve_table}`, facade/TableConfig, writer frames, `RecordAdmission`; Python binding resolve/locks; TS binding and generated declarations/protobuf | `shutdown_waits_for_an_admitted_record`, SDK/observe/Python/TS journeys, denied-writer and stale-version behavior |
| Physical output/query evidence | writer metadata tests and `oracle/distributed::bloom_policy_versions_answer_exactly_through_compaction`; zoned time pruning journey | actual complete/partial/empty policy Oracle results, geometry/pruning, FIND-7 closure; throughput is explicitly unmeasured |

## Rules, calibration and available evidence

Read current root AGENTS, all touched nested rules (docs, Python SDK, TS SDK, wyrd-sql), spec-driven workflow, maintainer style and applicable Rust/errors/agent-surfaces guides. Use owning Bifrost/security/deployment/recovery sections and revision-13 authority. R2's Implementation evidence/measurements/diagnoses are supplied verification, not independent reruns. Do not accept the completion summary as evidence or infer measured performance from fewer statements. If current verify:bifrost results become available, record their exact commit and scope.

Caller calibration: do not be nitpicky; report only required reachable behavioral/security/tenancy/durability/public-contract failures or deletion of unearned machinery. Rustdoc remains an explicit gate; batch genuine omissions, do not invent panics or style-only rounds. Do not add speculative guards, alternate services/caches/ledgers, or new normal-ingest SQL to remediation. Benchmarks are deferred: do not run them or block solely on honestly recorded unmeasured throughput when current authority permits it. Decisions already approved in revision 13 must not be reopened using superseded R1 mechanics.

For each prior finding, demonstrate closure or retained failure from actual source. For any suspected new failure, trace producer and sibling consumers through real callers, respect TASK-005 public-edit boundaries, and distinguish existing supported catalog-seam evolution explicitly required by TASK-004 from hypothetical future API work. If findings remain, corrections must be decision-complete; any genuinely required spec change must include a concrete recommended decision and follow-on remediation, not merely stop at a revision label.
