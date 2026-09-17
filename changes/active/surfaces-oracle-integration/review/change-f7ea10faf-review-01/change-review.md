# Integrated change review — f7ea10faf

**Verdict: FIX_REQUIRED.** One narrowly scoped AC-005 proof gap remains. The six focused test passes are accepted; no production behavior defect is established. The owner waived literal Surfaces-parent ancestry and repeat broad verification.

## Immutable subject and authority

- Base `861f8d86cc3f9d7e70fb59489e80f8be62afddbf`; target `f7ea10fafe772498ca0f5a0c63b783d578521d61`, tree `e7cbd93b25e967ca4faa1966db6450a3a55ce969`.
- Approved specification revision 9, original TASK-001..006, committed human approvals for TASK-001/005/006, cumulative TASK-003-R4 PASS for TASK-002/003/004, R1 and R2 tasks, prior integrated reviews, AGENTS/rules and Wyrd/Bifrost/testing authorities. No `.codegraph/` index.
- Since `e0e08c5c3`, source changes are only the test-support append signal in `wyrd-server/src/audit/publication.rs` and the existing real-server audit journey. The signal is feature-gated and unset on the server's background publisher. No public contract, migration, dependency, or ordinary production behavior changed.
- The R2 record reports six exact-selector passes on the final source tree, each selecting one test; prior same-tree gate-child and local cloud evidence is accepted for unchanged behavior. The single earlier flaky server run and resource-killed runs are excluded. This reviewer did not rerun tests.

## Material finding

### CHANGE-002 — MISSING: the surviving cycle's post-crash Scribe replay is not observed

**Obligation:** AC-005's real-server replay proof and TASK-CHANGE-R2; REQ-027C/029 provide the behavior being tested. **Location:** `crates/wyrd/wyrd-testing/tests/bifrost/server/audit_publication.rs:437-492`, with `crates/wyrd/wyrd-server/src/audit/publication.rs:220-294`.

The test now proves the frozen bound survives with a growing tail; two full cycles begin; `crashing` itself returns from a Scribe append for exactly that bound and is cancelled before settlement. It also proves final 3+1 retained counts and drained staging. But only `crashing` has the append observer. `survivor` is not shown to append either before or after `crashing` is aborted. It may still be waiting at freeze/read when the row fence is released. The server's background publisher can then settle first. `publish_range` deliberately returns `Ok(())` on an empty range, and `publish_tenant` still returns `Published { seq_lo, seq_hi, retired: 0 }`. Thus the asserted `Published` outcome and final counts can pass without the survivor submitting the frozen batch to Scribe. Bound reuse is proved; post-crash append replay by the surviving cycle is not.

**Consequence:** The required real-server evidence for Scribe's dedup fence absorbing a replay after an uncertain append is incomplete, although SQL seam tests and final-state evidence support production behavior. **Required outcome:** In this existing journey, observe the surviving/restarted cycle's own append of the same frozen range *after* the crashing cycle is cancelled and while the row fence still blocks settlement; then release the fence and keep the exact 3+1/drain assertions. Reuse the existing test-only `observe_appends` seam and existing lock/fence; no production change or new test file. **Closure proof:** the task's exact nonzero Postgres-wrapped focused command. No repeated gate, broad journey, format/lint, or cloud run solely for this test-only follow-up.

## Requirement, invariant, and acceptance matrix

Evidence paths are repository-relative. The unchanged-source rows were checked against the prior cumulative review and same-tree evidence; the 1557a4f9e test-only diff was reviewed directly. Recorded tests were not rerun by this reviewer. REQ-001's literal ancestry clause is waived by the owner.

| Obligation | Implementation evidence | Verification evidence | Result |
|---|---|---|---|
| REQ-001 — Preserve Surfaces behavior; literal parent ancestry waived by owner | `review-ledger.json`, `merge-inventory.md`; target content | ledger status complete; explicit owner waiver of ancestry | PASS |
| REQ-002 — Every textual conflict and semantic overlap MUST be resolved | `review-ledger.json`, `merge-inventory.md`; target history | ledger status complete; Git ancestry checks | PASS |
| REQ-003 — When compatible changes affect the same capability, the result | `review-ledger.json`, `merge-inventory.md`; target history | ledger status complete; Git ancestry checks | PASS |
| REQ-060 — The refreshed Oracle versions of wyrd-client and wyrd-queue | `wyrd-client`, `sdks/wyrd-sdk-*`, public route/contracts | gate + Python/TS, Cards, CLI, MCP journeys; codegen | PASS |
| REQ-061 — Oracle's current Bifrost client behavior, presently implemented | `wyrd-client`, `sdks/wyrd-sdk-*`, public route/contracts | gate + Python/TS, Cards, CLI, MCP journeys; codegen | PASS |
| REQ-004 — Card registration MUST preserve Surfaces' composite lifecycle, | `wyrd-spec/src/cards`, `wyrd-server/components/cards/service.rs` | Cards/CLI/WyrdState and Python/TS journeys | PASS |
| REQ-005 — Reference slots MUST use Ref or InlineableRef<T>; authored | `wyrd-spec/src/cards`, `wyrd-server/components/cards/service.rs` | Cards/CLI/WyrdState and Python/TS journeys | PASS |
| REQ-006 — CardRef MUST contain kind, name, one version, optional | `wyrd-spec/src/cards`, `wyrd-server/components/cards/service.rs` | Cards/CLI/WyrdState and Python/TS journeys | PASS |
| REQ-007 — Eval and Drift Cards MUST remain reusable and subject-less. | `wyrd-spec/src/cards`, `wyrd-server/components/cards/service.rs` | Cards/CLI/WyrdState and Python/TS journeys | PASS |
| REQ-008 — The v1 public catalog MUST contain the 16 registrable native | `wyrd-spec/src/cards`, `wyrd-server/components/cards/service.rs` | Cards/CLI/WyrdState and Python/TS journeys | PASS |
| REQ-009 — An Audit Card MUST remain an immutable investigator-created | `wyrd-spec/src/cards`, `wyrd-server/components/cards/service.rs` | Cards/CLI/WyrdState and Python/TS journeys | PASS |
| REQ-010 — The result MUST conform to architecture/bifrost-design.md and | `vala-bifrost-redux`, `wyrd-server/oracle`, `wyrd-testing/tests/bifrost` | gate Bifrost 9 lanes; scoped Oracle/Forge/OTLP journeys | PASS |
| REQ-011 — Each Bifrost physical analytical table MUST be identified by | `vala-bifrost-redux`, `wyrd-server/oracle`, `wyrd-testing/tests/bifrost` | gate Bifrost 9 lanes; scoped Oracle/Forge/OTLP journeys | PASS |
| REQ-012 — The only deployment targets MUST be all, server, oracle, | `vala-bifrost-redux`, `wyrd-server/oracle`, `wyrd-testing/tests/bifrost` | gate Bifrost 9 lanes; scoped Oracle/Forge/OTLP journeys | PASS |
| REQ-013 — One logical Wyrd server MUST support one or more tenants across | `vala-bifrost-redux`, `wyrd-server/oracle`, `wyrd-testing/tests/bifrost` | gate Bifrost 9 lanes; scoped Oracle/Forge/OTLP journeys | PASS |
| REQ-014 — Scribe MUST preserve Oracle's pod-local WAL v6 and its | `vala-bifrost-redux`, `wyrd-server/oracle`, `wyrd-testing/tests/bifrost` | gate Bifrost 9 lanes; scoped Oracle/Forge/OTLP journeys | PASS |
| REQ-015 — A selected analytical query MUST have one server execution | `vala-bifrost-redux`, `wyrd-server/oracle`, `wyrd-testing/tests/bifrost` | gate Bifrost 9 lanes; scoped Oracle/Forge/OTLP journeys | PASS |
| REQ-048 — Oracle MUST build each query once through the pinned distributed | `vala-bifrost-redux`, `wyrd-server/oracle`, `wyrd-testing/tests/bifrost` | gate Bifrost 9 lanes; scoped Oracle/Forge/OTLP journeys | PASS |
| REQ-049 — Oracle admission MUST be pod-local. Each pod MUST own bounded | `vala-bifrost-redux`, `wyrd-server/oracle`, `wyrd-testing/tests/bifrost` | gate Bifrost 9 lanes; scoped Oracle/Forge/OTLP journeys | PASS |
| REQ-050 — Oracle MUST establish one fenced renewable reader epoch per | `vala-bifrost-redux`, `wyrd-server/oracle`, `wyrd-testing/tests/bifrost` | gate Bifrost 9 lanes; scoped Oracle/Forge/OTLP journeys | PASS |
| REQ-051 — Forge MUST preserve the integrated maintenance authority: | `vala-bifrost-redux`, `wyrd-server/oracle`, `wyrd-testing/tests/bifrost` | gate Bifrost 9 lanes; scoped Oracle/Forge/OTLP journeys | PASS |
| REQ-052 — Forge plan admission MUST use the retained strict worker-local | `vala-bifrost-redux`, `wyrd-server/oracle`, `wyrd-testing/tests/bifrost` | gate Bifrost 9 lanes; scoped Oracle/Forge/OTLP journeys | PASS |
| REQ-053 — The canonical OpenTelemetry persistence contract MUST use only | `vala-bifrost-redux`, `wyrd-server/oracle`, `wyrd-testing/tests/bifrost` | gate Bifrost 9 lanes; scoped Oracle/Forge/OTLP journeys | PASS |
| REQ-053A — Every accepted telemetry row MUST carry the authenticated | `vala-bifrost-redux`, `wyrd-server/oracle`, `wyrd-testing/tests/bifrost` | gate Bifrost 9 lanes; scoped Oracle/Forge/OTLP journeys | PASS |
| REQ-054 — Permission MUST carry required typed scope. The closed initial | `vala-bifrost-redux`, `wyrd-server/oracle`, `wyrd-testing/tests/bifrost` | gate Bifrost 9 lanes; scoped Oracle/Forge/OTLP journeys | PASS |
| REQ-055 — The server MUST derive every Bifrost-managed local path from | `wyrd-server/boot/data_root.rs`, `config.rs` | root unit 4/4; server journey; gate | PASS |
| REQ-055A — REQ-055 MUST be delivered as a required follow-up task after | `wyrd-server/boot/data_root.rs`, `config.rs` | root unit 4/4; server journey; gate | PASS |
| REQ-062 — vala-bifrost-redux MUST replace legacy vala-bifrost in full. | `vala-bifrost-redux`, `wyrd-server/oracle`, `wyrd-testing/tests/bifrost` | gate Bifrost 9 lanes; scoped Oracle/Forge/OTLP journeys | PASS |
| REQ-063 — Bifrost Redux is the complete engine authority, including Gate, | `vala-bifrost-redux`, `wyrd-server/oracle`, `wyrd-testing/tests/bifrost` | gate Bifrost 9 lanes; scoped Oracle/Forge/OTLP journeys | PASS |
| REQ-016 — wyrd-server MUST own all server logic and durable behavior. | `wyrd-client`, `sdks/wyrd-sdk-*`, public route/contracts | gate + Python/TS, Cards, CLI, MCP journeys; codegen | PASS |
| REQ-017 — wyrd-client MUST compose and publicly expose the client | `wyrd-client`, `sdks/wyrd-sdk-*`, public route/contracts | gate + Python/TS, Cards, CLI, MCP journeys; codegen | PASS |
| REQ-018 — sdks/wyrd-sdk-rust, sdks/wyrd-sdk-python, and | `wyrd-client`, `sdks/wyrd-sdk-*`, public route/contracts | gate + Python/TS, Cards, CLI, MCP journeys; codegen | PASS |
| REQ-019 — Bifrost client behavior MUST be exposed through | `wyrd-client`, `sdks/wyrd-sdk-*`, public route/contracts | gate + Python/TS, Cards, CLI, MCP journeys; codegen | PASS |
| REQ-019A — ClientConfig::credential MUST be the single explicit shared | `wyrd-client`, `sdks/wyrd-sdk-*`, public route/contracts | gate + Python/TS, Cards, CLI, MCP journeys; codegen | PASS |
| REQ-020 — The Python SDK MUST preserve the public wyrd.cards, | `wyrd-client`, `sdks/wyrd-sdk-*`, public route/contracts | gate + Python/TS, Cards, CLI, MCP journeys; codegen | PASS |
| REQ-021 — The TypeScript SDK MUST preserve Oracle's implemented | `wyrd-client`, `sdks/wyrd-sdk-*`, public route/contracts | gate + Python/TS, Cards, CLI, MCP journeys; codegen | PASS |
| REQ-056 — HTTP and gRPC query streams MUST carry exactly one schema frame, | `wyrd-client`, `sdks/wyrd-sdk-*`, public route/contracts | gate + Python/TS, Cards, CLI, MCP journeys; codegen | PASS |
| REQ-057 — The server-owned /mcp surface MUST expose exactly | `wyrd-client`, `sdks/wyrd-sdk-*`, public route/contracts | gate + Python/TS, Cards, CLI, MCP journeys; codegen | PASS |
| REQ-058 — wyrd query MUST preserve one-of --sql/--file, visibility, | `wyrd-client`, `sdks/wyrd-sdk-*`, public route/contracts | gate + Python/TS, Cards, CLI, MCP journeys; codegen | PASS |
| REQ-059 — Generated OpenAPI and protobuf authority MUST include the public | `wyrd-client`, `sdks/wyrd-sdk-*`, public route/contracts | gate + Python/TS, Cards, CLI, MCP journeys; codegen | PASS |
| REQ-022 — WYRD_REGISTRY_* MUST be the sole stable registry error-code | `wyrd-client`, `sdks/wyrd-sdk-*`, public route/contracts | gate + Python/TS, Cards, CLI, MCP journeys; codegen | PASS |
| REQ-023 — Public Rust errors MUST preserve the Surfaces general catalog, | `wyrd-client`, `sdks/wyrd-sdk-*`, public route/contracts | gate + Python/TS, Cards, CLI, MCP journeys; codegen | PASS |
| REQ-024 — Python MUST expose one structured WyrdError root. Expected | `wyrd-client`, `sdks/wyrd-sdk-*`, public route/contracts | gate + Python/TS, Cards, CLI, MCP journeys; codegen | PASS |
| REQ-024B — Python MUST NOT restore BifrostQueryError, | `wyrd-client`, `sdks/wyrd-sdk-*`, public route/contracts | gate + Python/TS, Cards, CLI, MCP journeys; codegen | PASS |
| REQ-024A — Every stable failure reachable through a public Python | `wyrd-client`, `sdks/wyrd-sdk-*`, public route/contracts | gate + Python/TS, Cards, CLI, MCP journeys; codegen | PASS |
| REQ-025 — TypeScript MUST expose a runtime WyrdError and a generated | `wyrd-client`, `sdks/wyrd-sdk-*`, public route/contracts | gate + Python/TS, Cards, CLI, MCP journeys; codegen | PASS |
| REQ-026 — Every authorization decision that evaluates a principal's | `wyrd-server/audit/publication.rs`, `vala-sql/queries/audit_staging.rs`, audit journey | SQL `pg_audit_staging`; server journey 12/12 | PASS |
| REQ-026A — One logical Oracle query MUST produce one read-audit event and | `wyrd-server/audit/publication.rs`, `vala-sql/queries/audit_staging.rs`, audit journey | SQL `pg_audit_staging`; server journey 12/12 | PASS |
| REQ-026B — Object-scoped denial MUST be durably audited before refusal is | `wyrd-server/audit/publication.rs`, `vala-sql/queries/audit_staging.rs`, audit journey | SQL `pg_audit_staging`; server journey 12/12 | PASS |
| REQ-027 — A bounded publisher MUST move tenant audit events idempotently | `wyrd-server/audit/publication.rs`, `vala-sql/queries/audit_staging.rs`, audit journey | SQL `pg_audit_staging`; server journey 12/12 | PASS |
| REQ-027A — The per-tenant entry_hash MUST be reproducible from the | `wyrd-server/audit/publication.rs`, `vala-sql/queries/audit_staging.rs`, audit journey | SQL `pg_audit_staging`; server journey 12/12 | PASS |
| REQ-027B — Retained audit history MUST preserve the effective dynamic | `wyrd-server/audit/publication.rs`, `vala-sql/queries/audit_staging.rs`, audit journey | SQL `pg_audit_staging`; server journey 12/12 | PASS |
| REQ-027C — Before publication leaves Postgres, the publisher MUST durably | `wyrd-server/audit/publication.rs`, `vala-sql/queries/audit_staging.rs`, audit journey | SQL `pg_audit_staging`; server journey 12/12 | PASS |
| REQ-028 — After a range is durably published, one tenant-scoped Postgres | `wyrd-server/audit/publication.rs`, `vala-sql/queries/audit_staging.rs`, audit journey | SQL `pg_audit_staging`; server journey 12/12 | PASS |
| REQ-029 — Audit recovery MUST safely retry events whose publication, | `wyrd-server/audit/publication.rs`, `vala-sql/queries/audit_staging.rs`, audit journey | SQL `pg_audit_staging`; server journey 12/12 | PASS |
| REQ-030 — Neither a legacy direct-Iceberg relay nor a second audit table | `wyrd-server/audit/publication.rs`, `vala-sql/queries/audit_staging.rs`, audit journey | SQL `pg_audit_staging`; server journey 12/12 | PASS |
| REQ-030A — vala.forge_operation_state MUST be Forge's self-contained | `wyrd-server/audit/publication.rs`, `vala-sql/queries/audit_staging.rs`, audit journey | SQL `pg_audit_staging`; server journey 12/12 | PASS |
| REQ-031 — Repository-managed Postgres verification MUST use Oracle's | `mise.toml`, `.github/workflows`, Postgres scripts, generated outputs | same-tree gate-child map; server lane rerun 12/12; local cloud 2/backend | PASS |
| REQ-032 — wyrd-testing MUST preserve the refreshed Oracle production- | `mise.toml`, `.github/workflows`, Postgres scripts, generated outputs | same-tree gate-child map; server lane rerun 12/12; local cloud 2/backend | PASS |
| REQ-033 — Pull requests MUST run only verification lanes selected by the | `mise.toml`, `.github/workflows`, Postgres scripts, generated outputs | same-tree gate-child map; server lane rerun 12/12; local cloud 2/backend | PASS |
| REQ-034 — The complete non-credentialed correctness suite MUST run | `mise.toml`, `.github/workflows`, Postgres scripts, generated outputs | same-tree gate-child map; server lane rerun 12/12; local cloud 2/backend | PASS |
| REQ-035 — Required-check aggregation MUST remain stable when unaffected | `mise.toml`, `.github/workflows`, Postgres scripts, generated outputs | same-tree gate-child map; server lane rerun 12/12; local cloud 2/backend | PASS |
| REQ-035A — The Bifrost capability gate MUST cover its Rust, Python, and | `mise.toml`, `.github/workflows`, Postgres scripts, generated outputs | same-tree gate-child map; server lane rerun 12/12; local cloud 2/backend | PASS |
| REQ-064 — The final integrated candidate MUST pass every repository-owned | `mise.toml`, `.github/workflows`, Postgres scripts, generated outputs | same-tree gate-child map; server lane rerun 12/12; local cloud 2/backend | PASS |
| REQ-036 — Generated schemas, OpenAPI, stubs, declarations, and other | `mise.toml`, `.github/workflows`, Postgres scripts, generated outputs | same-tree gate-child map; server lane rerun 12/12; local cloud 2/backend | PASS |
| REQ-037 — Obsolete protocol documents, sample specs, aliases, routes, | `review-ledger.json`, `merge-inventory.md`; target history | ledger status complete; Git ancestry checks | PASS |
| REQ-038 — The completed change MUST remain on the dedicated integration | `review-ledger.json`, `merge-inventory.md`; target history | ledger status complete; Git ancestry checks | PASS |
| REQ-039 — Task decomposition and merge execution MUST use the completed | `review-ledger.json`, `merge-inventory.md`; target history | ledger status complete; Git ancestry checks | PASS |
| REQ-040 — Existing owner crates MAY retain optional python features | `wyrd-client`, `sdks/wyrd-sdk-*`, public route/contracts | gate + Python/TS, Cards, CLI, MCP journeys; codegen | PASS |
| REQ-041 — New or materially relocated Python logic MUST live in | `wyrd-client`, `sdks/wyrd-sdk-*`, public route/contracts | gate + Python/TS, Cards, CLI, MCP journeys; codegen | PASS |
| REQ-042 — This integration targets a greenfield database because Wyrd has | `mise.toml`, `.github/workflows`, Postgres scripts, generated outputs | same-tree gate-child map; server lane rerun 12/12; local cloud 2/backend | PASS |
| REQ-043 — Oracle's deletion of the legacy CLI and admin audit-integrity | `mise.toml`, `.github/workflows`, Postgres scripts, generated outputs | same-tree gate-child map; server lane rerun 12/12; local cloud 2/backend | PASS |
| REQ-044 — Surfaces' deletion of wyrd dev bootstrap MUST be preserved. | `mise.toml`, `.github/workflows`, Postgres scripts, generated outputs | same-tree gate-child map; server lane rerun 12/12; local cloud 2/backend | PASS |
| REQ-045 — The two Oracle-tracked .node binaries MUST be excluded from the | `mise.toml`, `.github/workflows`, Postgres scripts, generated outputs | same-tree gate-child map; server lane rerun 12/12; local cloud 2/backend | PASS |
| REQ-046 — History sanitization of the two formerly tracked .node paths | `review-ledger.json`, `merge-inventory.md`; target history | ledger status complete; Git ancestry checks | PASS |
| REQ-047 — Task decomposition and merge execution MUST use the immutable | `review-ledger.json`, `merge-inventory.md`; target history | ledger status complete; Git ancestry checks | PASS |
| INV-001 — No Surfaces-authoritative non-Bifrost behavior may be silently | `review-ledger.json`, `merge-inventory.md`; target history | ledger status complete; Git ancestry checks | PASS |
| INV-002 — No Oracle-authoritative Bifrost behavior may be silently lost | `review-ledger.json`, `merge-inventory.md`; target history | ledger status complete; Git ancestry checks | PASS |
| INV-003 — Git's textual merge result is never sufficient proof for a | `review-ledger.json`, `merge-inventory.md`; target history | ledger status complete; Git ancestry checks | PASS |
| INV-004 — Language SDKs never own durable server behavior or parallel Wyrd | `wyrd-client`, `sdks/wyrd-sdk-*`, public route/contracts | gate + Python/TS, Cards, CLI, MCP journeys; codegen | PASS |
| INV-005 — No public SDK exposes Gate, Scribe, Oracle, Forge, | `wyrd-client`, `sdks/wyrd-sdk-*`, public route/contracts | gate + Python/TS, Cards, CLI, MCP journeys; codegen | PASS |
| INV-006 — No client-tier crate depends on SQL, cloud SDKs, DataFusion, | `wyrd-client`, `sdks/wyrd-sdk-*`, public route/contracts | gate + Python/TS, Cards, CLI, MCP journeys; codegen | PASS |
| INV-007 — No operation derives effective tenant identity from an | `vala-bifrost-redux`, `wyrd-server/oracle`, `wyrd-testing/tests/bifrost` | gate Bifrost 9 lanes; scoped Oracle/Forge/OTLP journeys | PASS |
| INV-008 — No staging row is deleted before its corresponding audit-log | `wyrd-server/audit/publication.rs`, `vala-sql/queries/audit_staging.rs`, audit journey | SQL `pg_audit_staging`; server journey 12/12 | PASS |
| INV-008B — The retained audit event and its predecessor contain everything | `wyrd-server/audit/publication.rs`, `vala-sql/queries/audit_staging.rs`, audit journey | SQL `pg_audit_staging`; server journey 12/12 | PASS |
| INV-008A — Garbage-collecting staging never removes information required | `wyrd-server/audit/publication.rs`, `vala-sql/queries/audit_staging.rs`, audit journey | SQL `pg_audit_staging`; server journey 12/12 | PASS |
| INV-008C — Retained audit publication cannot append an audit event, and a | `wyrd-server/audit/publication.rs`, `vala-sql/queries/audit_staging.rs`, audit journey | SQL `pg_audit_staging`; server journey 12/12 | PASS |
| INV-008D — A growing staging tail cannot change the identity of an | `wyrd-server/audit/publication.rs`, `vala-sql/queries/audit_staging.rs`, audit journey | SQL `pg_audit_staging`; server journey 12/12 | PASS |
| INV-009 — No failed or partial analytical result is represented as a | `vala-bifrost-redux`, `wyrd-server/oracle`, `wyrd-testing/tests/bifrost` | gate Bifrost 9 lanes; scoped Oracle/Forge/OTLP journeys | PASS |
| INV-010 — No compatibility shim preserves a rejected contract or stale | `wyrd-client`, `sdks/wyrd-sdk-*`, public route/contracts | gate + Python/TS, Cards, CLI, MCP journeys; codegen | PASS |
| INV-011 — The active source worktrees and their branches remain | `review-ledger.json`, `merge-inventory.md`; target history | ledger status complete; Git ancestry checks | PASS |
| INV-012 — Generated output never overrides its owning source contract. | `mise.toml`, `.github/workflows`, Postgres scripts, generated outputs | same-tree gate-child map; server lane rerun 12/12; local cloud 2/backend | PASS |
| INV-013 — A Rust or TypeScript SDK build never activates PyO3 or a | `wyrd-client`, `sdks/wyrd-sdk-*`, public route/contracts | gate + Python/TS, Cards, CLI, MCP journeys; codegen | PASS |
| INV-014 — Merge resolution never resurrects a file or public testing API | `mise.toml`, `.github/workflows`, Postgres scripts, generated outputs | same-tree gate-child map; server lane rerun 12/12; local cloud 2/backend | PASS |
| INV-015 — No migration compatibility path is added solely for a Wyrd | `mise.toml`, `.github/workflows`, Postgres scripts, generated outputs | same-tree gate-child map; server lane rerun 12/12; local cloud 2/backend | PASS |
| INV-016 — No integrated source tree or generated-artifact workflow treats | `mise.toml`, `.github/workflows`, Postgres scripts, generated outputs | same-tree gate-child map; server lane rerun 12/12; local cloud 2/backend | PASS |
| INV-017 — Oracle query admission has no PostgreSQL or cluster-wide quota | `vala-bifrost-redux`, `wyrd-server/oracle`, `wyrd-testing/tests/bifrost` | gate Bifrost 9 lanes; scoped Oracle/Forge/OTLP journeys | PASS |
| INV-018 — No snapshot-dependent source IO begins without durable reader | `vala-bifrost-redux`, `wyrd-server/oracle`, `wyrd-testing/tests/bifrost` | gate Bifrost 9 lanes; scoped Oracle/Forge/OTLP journeys | PASS |
| INV-019 — No typed observation-read API or duplicate trace-child or GenAI | `vala-bifrost-redux`, `wyrd-server/oracle`, `wyrd-testing/tests/bifrost` | gate Bifrost 9 lanes; scoped Oracle/Forge/OTLP journeys | PASS |
| INV-020 — No query reads a table outside the verified principal's complete | `vala-bifrost-redux`, `wyrd-server/oracle`, `wyrd-testing/tests/bifrost` | gate Bifrost 9 lanes; scoped Oracle/Forge/OTLP journeys | PASS |
| INV-021 — Forge has no hard DataFusion memory-pool or spill guarantee; its | `vala-bifrost-redux`, `wyrd-server/oracle`, `wyrd-testing/tests/bifrost` | gate Bifrost 9 lanes; scoped Oracle/Forge/OTLP journeys | PASS |
| INV-022 — No role becomes ready with an unusable Bifrost data root, | `wyrd-server/boot/data_root.rs`, `config.rs` | root unit 4/4; server journey; gate | PASS |
| INV-023 — The final dependency graph contains exactly one Bifrost engine: | `vala-bifrost-redux`, `wyrd-server/oracle`, `wyrd-testing/tests/bifrost` | gate Bifrost 9 lanes; scoped Oracle/Forge/OTLP journeys | PASS |
| INV-024 — No Surfaces-era wyrd-client or wyrd-queue implementation | `wyrd-client`, `sdks/wyrd-sdk-*`, public route/contracts | gate + Python/TS, Cards, CLI, MCP journeys; codegen | PASS |
| INV-025 — The integration has no accepted baseline failures. A failing or | `mise.toml`, `.github/workflows`, Postgres scripts, generated outputs | same-tree gate-child map; server lane rerun 12/12; local cloud 2/backend | PASS |
| AC-001 | `review-ledger.json`, `merge-inventory.md`; target history | ledger status complete; Git ancestry checks | PASS |
| AC-002 — Contract and generated-artifact evidence shows one coherent Card, | `wyrd-client`, `sdks/wyrd-sdk-*`, public route/contracts | gate + Python/TS, Cards, CLI, MCP journeys; codegen | PASS |
| AC-003 — Focused user-journey evidence demonstrates the preserved Surfaces | `wyrd-client`, `sdks/wyrd-sdk-*`, public route/contracts | gate + Python/TS, Cards, CLI, MCP journeys; codegen | PASS |
| AC-004 — Real Rust, Python sync/async, and TypeScript SDK journeys through | `wyrd-client`, `sdks/wyrd-sdk-*`, public route/contracts | gate + Python/TS, Cards, CLI, MCP journeys; codegen | PASS |
| AC-005 — Real-server tail, competing publishers, replay and drain | `wyrd-testing/tests/bifrost/server/audit_publication.rs:350-492`; `wyrd-server/audit/publication.rs:220-294` | Two full cycles and crashing actor's post-append signal proved; survivor's post-crash Scribe replay not observed; final 3+1/drain proved | FAIL |
| AC-006 — Tenant and authorization evidence demonstrates isolation across | `vala-bifrost-redux`, `wyrd-server/oracle`, `wyrd-testing/tests/bifrost` | gate Bifrost 9 lanes; scoped Oracle/Forge/OTLP journeys | PASS |
| AC-007 — Fixture evidence demonstrates isolated database lifecycle under | `mise.toml`, `.github/workflows`, Postgres scripts, generated outputs | same-tree gate-child map; server lane rerun 12/12; local cloud 2/backend | PASS |
| AC-007A — The completed conflict ledger enumerates the refreshed Oracle | `mise.toml`, `.github/workflows`, Postgres scripts, generated outputs | same-tree gate-child map; server lane rerun 12/12; local cloud 2/backend | PASS |
| AC-008 — Workflow evidence demonstrates affected-code pull-request lane | `mise.toml`, `.github/workflows`, Postgres scripts, generated outputs | same-tree gate-child map; server lane rerun 12/12; local cloud 2/backend | PASS |
| AC-009 — Final static review maps every REQ-* and INV-* to credible | `mise.toml`, `.github/workflows`, Postgres scripts, generated outputs | same-tree gate-child map; server lane rerun 12/12; local cloud 2/backend | PASS |
| AC-010 — Dependency and feature evidence shows that only | `wyrd-client`, `sdks/wyrd-sdk-*`, public route/contracts | gate + Python/TS, Cards, CLI, MCP journeys; codegen | PASS |
| AC-011 — Distributed query journeys prove the single physical-build | `vala-bifrost-redux`, `wyrd-server/oracle`, `wyrd-testing/tests/bifrost` | gate Bifrost 9 lanes; scoped Oracle/Forge/OTLP journeys | PASS |
| AC-012 — Deterministic local-admission tests and a multi-replica journey | `vala-bifrost-redux`, `wyrd-server/oracle`, `wyrd-testing/tests/bifrost` | gate Bifrost 9 lanes; scoped Oracle/Forge/OTLP journeys | PASS |
| AC-013 — Reader-authority integration and process journeys prove durable | `vala-bifrost-redux`, `wyrd-server/oracle`, `wyrd-testing/tests/bifrost` | gate Bifrost 9 lanes; scoped Oracle/Forge/OTLP journeys | PASS |
| AC-014 — Forge production journeys prove independent per-plan publication, | `vala-bifrost-redux`, `wyrd-server/oracle`, `wyrd-testing/tests/bifrost` | gate Bifrost 9 lanes; scoped Oracle/Forge/OTLP journeys | PASS |
| AC-015 — Stock Rust, Python, and TypeScript OTLP exporters plus canonical | `vala-bifrost-redux`, `wyrd-server/oracle`, `wyrd-testing/tests/bifrost` | gate Bifrost 9 lanes; scoped Oracle/Forge/OTLP journeys | PASS |
| AC-016 — Scoped-role journeys prove All, schema, and stable table-UID | `vala-bifrost-redux`, `wyrd-server/oracle`, `wyrd-testing/tests/bifrost` | gate Bifrost 9 lanes; scoped Oracle/Forge/OTLP journeys | PASS |
| AC-017 — Audit evidence proves delegation and typed scoped permission | `wyrd-server/audit/publication.rs`, `vala-sql/queries/audit_staging.rs`, audit journey | SQL `pg_audit_staging`; server journey 12/12 | PASS |
| AC-018 — Configuration, boot, restart, and deployment evidence proves the | `wyrd-server/boot/data_root.rs`, `config.rs` | root unit 4/4; server journey; gate | PASS |
| AC-019 — MCP discovery and invocation journeys prove the exact three-tool | `wyrd-client`, `sdks/wyrd-sdk-*`, public route/contracts | gate + Python/TS, Cards, CLI, MCP journeys; codegen | PASS |
| AC-020 — Static tree, workspace metadata, dependency-graph, feature, code, | `vala-bifrost-redux`, `wyrd-server/oracle`, `wyrd-testing/tests/bifrost` | gate Bifrost 9 lanes; scoped Oracle/Forge/OTLP journeys | PASS |
| AC-021 — Base-to-candidate review shows the Oracle wyrd-client and | `wyrd-client`, `sdks/wyrd-sdk-*`, public route/contracts | gate + Python/TS, Cards, CLI, MCP journeys; codegen | PASS |
| AC-022 — Completion evidence records a passing result for every | `mise.toml`, `.github/workflows`, Postgres scripts, generated outputs | same-tree gate-child map; server lane rerun 12/12; local cloud 2/backend | PASS |

## Constraints and non-goals

| Obligation | Implementation evidence | Verification evidence | Result |
|---|---|---|---|
| C-01 architecture and human decisions govern both inputs | Approved spec; architecture; conflict ledger | Ledger/current source review | PASS |
| C-02 tenant, audit, WAL, terminal, resource safety | Redux/Oracle/audit source | Same-tree mapped lanes and domain journeys; AC-005 evidence assessed separately | PASS |
| C-03 SDKs consume internal behavior only through shared client | `wyrd-client`, SDK manifests | client-tier/PyO3 checks, language journeys | PASS |
| C-04 retained owner Python features only as migration state | Python SDK aggregation/manifests | pyo3-scope, wheel, type checks | PASS |
| C-05 language-specific code has runtime/authoring need | SDK packages and N-API/PyO3 boundaries | package builds/journeys | PASS |
| C-06 no shared client abstraction beside `wyrd-client` | workspace/public exports | client-tier check | PASS |
| C-07 enforce single data root in production | `boot/data_root.rs`, `config.rs` | root/server journeys | PASS |
| B-01 SDK → shared client → server; engines server-owned | manifests, router, clients | client-tier check and journeys | PASS |
| B-02 query and Forge safety ordering | Oracle/Forge owners | Bifrost journeys | PASS |
| B-03 audit staging → frozen range → local Scribe → retention | audit publisher, SQL, real-server journey | Frozen bound and crashing append/abort proved; survivor's Scribe replay after abort not observed | FAIL |
| NG-01 no merge/push/release/deploy to main | branch/target history | target inspection | PASS |
| NG-02 no source-branch/worktree mutation | source refs and candidate | read-only inspection | PASS |
| NG-03 no older multi-repo decomposition | `sdks/` and shared client | dependency check | PASS |
| NG-04 no compatibility aliases or parallel APIs | public exports/routes | codegen/client-tier | PASS |
| NG-05 no second scheduler/shuffle/external write/client durability | Redux/Oracle/SDK source | Bifrost journeys | PASS |
| NG-06 no v1 Card catalog expansion or registrable External | `wyrd-spec` kinds | schema/codegen | PASS |
| NG-07 no unrelated owner-Python extraction | feature graph | pyo3-scope | PASS |
| NG-08 no retired benchmark/qualification/testing API restoration | testing tree and mise | inventory | PASS |
| NG-09 no retired audit verify/admin integrity surface | server router/CLI | route/codegen checks | PASS |
| NG-10 no `wyrd dev bootstrap` | CLI source | CLI/gate | PASS |
| NG-11 no tracked compiled native outputs | `git ls-files '*.node'` empty | static inventory | PASS |
| NG-12 no legacy `vala-bifrost` alias/engine | workspace and source tree | client-tier/no-legacy inventory | PASS |
| NG-13 no live Oracle query UI claim | UI source/docs | static inspection | PASS |
| NG-14 no implementation verification during spec drafting | approved revision history | packet history | PASS |

## Task gates, seams, and verification limits

| Task | Gate authority at target | Integrated observation |
|---|---|---|
| TASK-001 | Committed human approval; status approved | Redux and audit sources present; literal ancestry expressly waived. |
| TASK-002/003/004 | Cumulative TASK-003-R4 PASS | Shared Bifrost facade, three SDKs, CI/owner/storage lanes and data-root proof present. |
| TASK-005/006 | Committed human approval; status approved | Authorization and publication source present; AC-005 replay proof incomplete. |
| TASK-CHANGE-R1 | Committed evidence | Tail-above-bound and competing SQL freeze added. |
| TASK-CHANGE-R2 | Committed implementation/evidence | Crashing actor append and abort now proved; surviving actor's post-abort replay remains unproved. |

`git diff --check e0e08c5c3..f7ea10faf` reports one trailing blank line at EOF in the prior review Markdown, not a source defect; this is a nonmaterial verification limit, not a new finding. Unrelated dirty `verified-change-contract` files and historical untracked reviews were excluded. No implementation, merge, push, deployment, or completion was performed.
