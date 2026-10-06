# System-resilience review

## Immutable subject

- Repository: `/home/thorrester/Documents/GitHub/wyrd-bifrost-variant`
- Base: `80b33286e7dcaab8ed04d06b63eb0ca329b0c2a2`
- Candidate: `3cf911fce699bbfe197f8b95e72b13e2f551f766`
- Approved specification: `changes/active/bifrost-variant/spec.md`, revision 10
- Task: `changes/active/bifrost-variant/tasks/TASK-001-variant-storage-and-query.md`

The candidate remained at the stated commit throughout this review.

## Deployed-path coverage

| Changed runtime path | Deployed topology and source evidence | Affected capabilities | Assessment |
|---|---|---|---|
| Variant ingest and built-in projections | Variant values are encoded and validated through the installed Arrow/Parquet Variant crates in `crates/shared/wyrd-queue/src/variant.rs:1-179`; the size and depth limits are enforced before a value becomes `EncodedVariant`. Scribe's existing admission, WAL, staging, and publication ownership remains unchanged. | Built-in verification, drift/eval, gateway, audit, agent-trace, and OTLP writes; user-facing Rust, Python, and TypeScript reads | No new process, queue, retry loop, durable side channel, or availability dependency was added. Invalid Variant input is refused before durable admission, so it cannot leave a partial acknowledged write. |
| Interactive Oracle queries | The planning session installs the shared Variant functions before provider setup and physical planning at `crates/vala/vala-bifrost-redux/src/oracle/mod.rs:3091-3119`; the admitted execution session installs the same owner through `OracleExecution::session_state` at `crates/vala/vala-bifrost-redux/src/resources.rs:4235-4255`. | SQL `->`, `->>`, `parse_json`, `try_parse_json`, `to_json`; HTTP/MCP/SDK query terminals | Registration is process-local immutable state and introduces no availability dependency. Existing deadline, cancellation, source-degradation, active-read, and terminal ownership remains in the Oracle stream (`oracle/mod.rs:4200-4219`). |
| Distributed and analytical Oracle queries | Worker sessions install the same registry at `crates/vala/vala-bifrost-redux/src/oracle/analytical.rs:525-543`; analytical execution rebuilds it before the distributed planner at `analytical.rs:7677-7693`. The function contract version is bound into follower plan and stage digests at `oracle/codec.rs:25-37` and `oracle/peer.rs:302-323`, before decode. | Leader, follower, admission, analytical worker, and multi-stage query execution | A mixed-version peer fails the affected query before plan decode or provider IO instead of executing different semantics. Failure remains query-scoped; no code path turns a function mismatch into shared-server termination. |
| Variant query failures and terminals | Local typed errors survive the DataFusion source chain; distributed message-only errors are reconstructed and otherwise fall back to the existing generic execution failure at `crates/vala/vala-bifrost-redux/src/oracle/mod.rs:4246-4315`. | Stable HTTP/gRPC/MCP/SDK error identity | Resource exhaustion retains precedence. Malformed or unrelated remote text does not panic and falls back to `QueryExecutionFailed`, preserving terminal safety. |
| Iceberg table creation and admission | Every new physical table is created as v3 at `crates/vala/vala-bifrost-redux/src/catalog/bifrost_catalog.rs:1047-1058`; existing physical state is accepted only when it is v3 at `bifrost_catalog.rs:1062-1100`. | Scribe publication, Oracle scans, Forge maintenance | The change is fail-closed and deliberately has no migration or compatibility branch because the approved specification states Bifrost has not shipped. A stale/non-v3 table blocks that table's registration rather than corrupting it or widening the failure to unrelated capabilities. |
| Forge data-file rewrite publication | A rewrite output is rejected before catalog mutation unless both reserved lineage columns have one non-null metric value per row at `crates/vala/vala-bifrost-redux/src/forge/publication.rs:395-466,1456-1492`. | Compaction, subsequent reads, repeated rewrites | Failed or incomplete lineage evidence commits nothing. Existing durable task/attempt identity and reconciliation retain the output for retry/cleanup; the failure is isolated to the rewrite, not the shared server. |
| Forge manifest rewrite and garbage collection | The existing table lease, cancellation check, catalog timeout, and commit owner remain in place at `crates/vala/vala-bifrost-redux/src/forge/gc.rs:223-340`; the former v3 skip is removed because the pinned Iceberg implementation preserves/assigns `first_row_id`. | Manifest maintenance, snapshot expiry, orphan cleanup, fresh row-id allocation | A dependency outage or timeout returns through the existing Forge error path; the per-table maintenance loop logs the failure and continues other tables. Lease-release failure remains recoverable by expiry. |

## Failure and recovery assessment

### Process restart and rolling replacement

- Scribe durability and replay ownership are not changed by Variant encoding. Validation and canonical encoding happen before the existing WAL acknowledgement boundary; restart recovery therefore sees only accepted Arrow/Parquet data, not half-normalized rows.
- Oracle Variant registration is deterministic process-local construction. A restarted process rebuilds the registry with the session. In-flight queries retain the existing cancellation/drop semantics and must be retried by their existing caller path.
- Rolling overlap with a peer carrying another Variant SQL contract is refused by the plan/stage digest before decode. Because v3 is an irreversible durable-format boundary and the approved spec explicitly has no shipped data, deployment must treat this candidate as a coordinated pre-release contract change rather than roll back after writes. This is an existing deployment rule, not a new mechanism requested from the implementation.
- Forge keeps durable attempt identity, table leases, catalog timeouts, and reconciliation. A crash before the lineage check publishes nothing; a crash after an uncertain catalog commit uses the existing snapshot/reconciliation identity rather than deriving a second commit.

### Dependency outage, timeout, and cancellation

- Object-store or catalog failure during Forge rewrite/GC remains table-local. The candidate does not add unbounded retries or a second publication path.
- Oracle peer unavailability still follows the established query contract: pre-row known live-source loss may degrade, post-row loss fails, and terminal validation prevents partial rows from becoming success. Variant registration adds no remote call.
- Oracle deadline and cancellation ownership is unchanged. Variant UDF work executes within the admitted DataFusion query and its governed runtime; fixed stored-value limits bound each persisted Variant, while query memory/resource exhaustion continues to map before Variant errors.
- A malformed rewrite output fails closed at the publication derivation boundary. It does not crash the Forge process, alter the snapshot, or make unrelated Scribe/Oracle capability unavailable.

### Recovery proof

- `v3_row_lineage_survives_repeated_rewrite` exercises built-in and user tables through real Scribe promotion, two successive Forge rewrites, an injected unencodable-lineage refusal with no commit, recovery after the poison is withdrawn, v3 manifest rewrite, snapshot expiry, and fresh row-id allocation (`crates/vala/vala-bifrost-redux/tests/integration/forge/managed_rewrite.rs:1547-1729`).
- `variant_sql_registry_covers_every_session` uses a four-pod topology, executes both Interactive and Analytical paths through followers, checks stable invalid-JSON failure on both, and proves sensitive Variant access is refused before follower graph leases advance (`crates/wyrd/wyrd-testing/tests/bifrost/oracle/published.rs:1092-1273`).
- The task records successful focused evidence for those two journeys plus the fork's Variant round trip, compaction lineage test, built-in/OTLP/SDK/MCP query journeys, Bloom geometry, code generation, and `git diff --check`.

## Standard-mechanism and drift assessment

No resilience DRIFT was found. The candidate uses the established mechanisms already authoritative in Wyrd and common in comparable OLAP systems: Arrow extension arrays and DataFusion session UDF registration for query semantics, Iceberg v3 row lineage and manifest/snapshot transactions for persistent identity, Parquet column metrics as publication evidence, table leases and idempotent reconciliation for maintenance recovery, and peer contract fingerprints for mixed-version refusal. It adds no bespoke supervisor, retry protocol, shadow WAL, second reader, capability toggle, migration mode, or deployment setting.

## Material findings

None.

## Verification limits

- This reviewer inspected the complete base-to-candidate diff, production callers, lifecycle owners, and the named journey source, and relied on the task's recorded all-green command evidence. It did not re-run the Postgres/object-store multi-process journeys.
- No separate crash-at-every-instruction fault-injection suite was required: the changed durable transition is directly covered by the no-commit lineage refusal and existing Forge reconciliation boundary, while session registration is stateless and rebuilt on restart.

## Overall result

**PASS**

The candidate changes the deployed system without introducing an unbounded retry, shared-process crash path, partial-success terminal, or unrecoverable new durable transition. The required failure and recovery paths have direct source and journey evidence, and no material system-resilience finding remains.
