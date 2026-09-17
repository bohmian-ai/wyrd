# Integrated change review — ee63cba17

**Verdict: FIX_REQUIRED.** Only the required combined real-server audit-publication race proof (AC-005) remains. The owner explicitly waived the literal Surfaces-parent ancestry condition in REQ-001; no ancestry merge is required. No source, branch, or historical review was changed by this audit.

## Immutable subject and authority

- Repository: `/Users/stevenforrester/Documents/GitHub/wyrd-bifrost-surfaces`; base `861f8d86cc3f9d7e70fb59489e80f8be62afddbf`; target `ee63cba1798a9e4fa6a21afbcdf1eda6c2f5b1f6`, tree `04193bd3fc315ab671f5d7fb33d3bff22081392c`.
- Approved authority: `changes/active/surfaces-oracle-integration/spec.md` revision 9, AGENTS.md, agent rules, Wyrd/Bifrost/security/operations design, six original tasks, review packet, and current user instructions.
- Last tested source: `d6de890d83f269eab834fa323f3ebe31f1adc573`, tree `d8a727b5846f24bac7b55bbc786bcb391c55cb03`. Subsequent commits `24b8e78d7`, `10d579fbd`, and `ee63cba17` only change review/task documentation; `git diff --check` base..target is clean. No `.codegraph/` index exists.
- The user explicitly accepts TASK-001, TASK-005, and TASK-006 as approved without another task-review cycle. The committed [approval record](../human-task-approval-2026-09-16.md) supersedes their task gate, not the approved spec or this independent integrated audit. Historical verdicts remain historical. TASK-002/TASK-004/TASK-003 have the cumulative TASK-003-R4 `PASS` bound to the tested source.
- The owner explicitly waived literal ancestry from the named Surfaces parent on 2026-09-16. The target still does not descend from that commit; this is an acknowledged waiver, not a passing ancestry check. REQ-001's content-preservation obligations remain in scope.
- Final-tree evidence records `CARGO_BUILD_JOBS=4 mise run -j 1 gate` exit 0, nine Bifrost lanes, `check:bifrost`, six additional owner lanes, focused checks, language and storage journeys, and local `mise.local.toml` S3/GCS/Azure (two tests each). This is the R4 task packet's recorded result, not a fresh execution or raw-log attestation.

## Findings

### CHANGE-002 — MISSING: required real-server tail-growth/competition proof is not exercised

AC-005 expressly requires existing **real-server** retained-history evidence including a growing staging tail with competing publishers and crash replay without duplication. The SQL-level `pg_audit_staging::frozen_range_survives_tail_growth_competition_and_stale_settlement` does prove freeze coordination. The server journey `frozen_audit_range_replays_once_while_its_tail_waits` at `crates/wyrd/wyrd-testing/tests/bifrost/server/audit_publication.rs:256-309` freezes three rows, crashes/replays and settles them at line 302, **then** appends the tail at line 305. It never grows the tail while the old bound is in flight or forces a competing server publisher to observe that bound with a later row present. Its comments claim that combined scenario, but the executable order does not. A green server lane therefore cannot prove the specified cross-boundary race, even though source and lower-tier SQL tests support the intended implementation.

Required outcome: extend this existing real-server journey, without a new harness/file, so the tail commits above a still-frozen range before replay settlement and a competing publication cycle operates while that bound remains live; assert the original batch identity/retained uniqueness, delayed tail's eventual publication, and zero idle staging. Run its exact focused nonzero nextest selection through the repository Postgres wrapper, then the owning `test:bifrost:journey:server` lane. Do not alter the production publisher merely to make a test pass.

## Requirement, invariant, and acceptance matrix

Evidence paths are repository-relative; `wyrd-spec`, `wyrd-client`, and similar short prefixes denote the owning crate under `crates/`. Test counts and broad commands come from the committed R4 evidence and are independently matched to `mise.toml`/source selectors, but were not rerun here. A PASS row reflects source plus recorded proof, except REQ-001's expressly waived ancestry clause; it does not override the remaining FAIL row.

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
| REQ-031 — Repository-managed Postgres verification MUST use Oracle's | `mise.toml`, `.github/workflows`, Postgres scripts, generated outputs | gate exit 0; owner lanes; local cloud 2/backend | PASS |
| REQ-032 — wyrd-testing MUST preserve the refreshed Oracle production- | `mise.toml`, `.github/workflows`, Postgres scripts, generated outputs | gate exit 0; owner lanes; local cloud 2/backend | PASS |
| REQ-033 — Pull requests MUST run only verification lanes selected by the | `mise.toml`, `.github/workflows`, Postgres scripts, generated outputs | gate exit 0; owner lanes; local cloud 2/backend | PASS |
| REQ-034 — The complete non-credentialed correctness suite MUST run | `mise.toml`, `.github/workflows`, Postgres scripts, generated outputs | gate exit 0; owner lanes; local cloud 2/backend | PASS |
| REQ-035 — Required-check aggregation MUST remain stable when unaffected | `mise.toml`, `.github/workflows`, Postgres scripts, generated outputs | gate exit 0; owner lanes; local cloud 2/backend | PASS |
| REQ-035A — The Bifrost capability gate MUST cover its Rust, Python, and | `mise.toml`, `.github/workflows`, Postgres scripts, generated outputs | gate exit 0; owner lanes; local cloud 2/backend | PASS |
| REQ-064 — The final integrated candidate MUST pass every repository-owned | `mise.toml`, `.github/workflows`, Postgres scripts, generated outputs | gate exit 0; owner lanes; local cloud 2/backend | PASS |
| REQ-036 — Generated schemas, OpenAPI, stubs, declarations, and other | `mise.toml`, `.github/workflows`, Postgres scripts, generated outputs | gate exit 0; owner lanes; local cloud 2/backend | PASS |
| REQ-037 — Obsolete protocol documents, sample specs, aliases, routes, | `review-ledger.json`, `merge-inventory.md`; target history | ledger status complete; Git ancestry checks | PASS |
| REQ-038 — The completed change MUST remain on the dedicated integration | `review-ledger.json`, `merge-inventory.md`; target history | ledger status complete; Git ancestry checks | PASS |
| REQ-039 — Task decomposition and merge execution MUST use the completed | `review-ledger.json`, `merge-inventory.md`; target history | ledger status complete; Git ancestry checks | PASS |
| REQ-040 — Existing owner crates MAY retain optional python features | `wyrd-client`, `sdks/wyrd-sdk-*`, public route/contracts | gate + Python/TS, Cards, CLI, MCP journeys; codegen | PASS |
| REQ-041 — New or materially relocated Python logic MUST live in | `wyrd-client`, `sdks/wyrd-sdk-*`, public route/contracts | gate + Python/TS, Cards, CLI, MCP journeys; codegen | PASS |
| REQ-042 — This integration targets a greenfield database because Wyrd has | `mise.toml`, `.github/workflows`, Postgres scripts, generated outputs | gate exit 0; owner lanes; local cloud 2/backend | PASS |
| REQ-043 — Oracle's deletion of the legacy CLI and admin audit-integrity | `mise.toml`, `.github/workflows`, Postgres scripts, generated outputs | gate exit 0; owner lanes; local cloud 2/backend | PASS |
| REQ-044 — Surfaces' deletion of wyrd dev bootstrap MUST be preserved. | `mise.toml`, `.github/workflows`, Postgres scripts, generated outputs | gate exit 0; owner lanes; local cloud 2/backend | PASS |
| REQ-045 — The two Oracle-tracked .node binaries MUST be excluded from the | `mise.toml`, `.github/workflows`, Postgres scripts, generated outputs | gate exit 0; owner lanes; local cloud 2/backend | PASS |
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
| INV-012 — Generated output never overrides its owning source contract. | `mise.toml`, `.github/workflows`, Postgres scripts, generated outputs | gate exit 0; owner lanes; local cloud 2/backend | PASS |
| INV-013 — A Rust or TypeScript SDK build never activates PyO3 or a | `wyrd-client`, `sdks/wyrd-sdk-*`, public route/contracts | gate + Python/TS, Cards, CLI, MCP journeys; codegen | PASS |
| INV-014 — Merge resolution never resurrects a file or public testing API | `mise.toml`, `.github/workflows`, Postgres scripts, generated outputs | gate exit 0; owner lanes; local cloud 2/backend | PASS |
| INV-015 — No migration compatibility path is added solely for a Wyrd | `mise.toml`, `.github/workflows`, Postgres scripts, generated outputs | gate exit 0; owner lanes; local cloud 2/backend | PASS |
| INV-016 — No integrated source tree or generated-artifact workflow treats | `mise.toml`, `.github/workflows`, Postgres scripts, generated outputs | gate exit 0; owner lanes; local cloud 2/backend | PASS |
| INV-017 — Oracle query admission has no PostgreSQL or cluster-wide quota | `vala-bifrost-redux`, `wyrd-server/oracle`, `wyrd-testing/tests/bifrost` | gate Bifrost 9 lanes; scoped Oracle/Forge/OTLP journeys | PASS |
| INV-018 — No snapshot-dependent source IO begins without durable reader | `vala-bifrost-redux`, `wyrd-server/oracle`, `wyrd-testing/tests/bifrost` | gate Bifrost 9 lanes; scoped Oracle/Forge/OTLP journeys | PASS |
| INV-019 — No typed observation-read API or duplicate trace-child or GenAI | `vala-bifrost-redux`, `wyrd-server/oracle`, `wyrd-testing/tests/bifrost` | gate Bifrost 9 lanes; scoped Oracle/Forge/OTLP journeys | PASS |
| INV-020 — No query reads a table outside the verified principal's complete | `vala-bifrost-redux`, `wyrd-server/oracle`, `wyrd-testing/tests/bifrost` | gate Bifrost 9 lanes; scoped Oracle/Forge/OTLP journeys | PASS |
| INV-021 — Forge has no hard DataFusion memory-pool or spill guarantee; its | `vala-bifrost-redux`, `wyrd-server/oracle`, `wyrd-testing/tests/bifrost` | gate Bifrost 9 lanes; scoped Oracle/Forge/OTLP journeys | PASS |
| INV-022 — No role becomes ready with an unusable Bifrost data root, | `wyrd-server/boot/data_root.rs`, `config.rs` | root unit 4/4; server journey; gate | PASS |
| INV-023 — The final dependency graph contains exactly one Bifrost engine: | `vala-bifrost-redux`, `wyrd-server/oracle`, `wyrd-testing/tests/bifrost` | gate Bifrost 9 lanes; scoped Oracle/Forge/OTLP journeys | PASS |
| INV-024 — No Surfaces-era wyrd-client or wyrd-queue implementation | `wyrd-client`, `sdks/wyrd-sdk-*`, public route/contracts | gate + Python/TS, Cards, CLI, MCP journeys; codegen | PASS |
| INV-025 — The integration has no accepted baseline failures. A failing or | `mise.toml`, `.github/workflows`, Postgres scripts, generated outputs | gate exit 0; owner lanes; local cloud 2/backend | PASS |
| AC-001 | `review-ledger.json`, `merge-inventory.md`; target history | ledger status complete; Git ancestry checks | PASS |
| AC-002 — Contract and generated-artifact evidence shows one coherent Card, | `wyrd-client`, `sdks/wyrd-sdk-*`, public route/contracts | gate + Python/TS, Cards, CLI, MCP journeys; codegen | PASS |
| AC-003 — Focused user-journey evidence demonstrates the preserved Surfaces | `wyrd-client`, `sdks/wyrd-sdk-*`, public route/contracts | gate + Python/TS, Cards, CLI, MCP journeys; codegen | PASS |
| AC-004 — Real Rust, Python sync/async, and TypeScript SDK journeys through | `wyrd-client`, `sdks/wyrd-sdk-*`, public route/contracts | gate + Python/TS, Cards, CLI, MCP journeys; codegen | PASS |
| AC-005 — SQL and audit evidence demonstrates fail-closed audit of allowed | `wyrd-server/audit/publication.rs`, `vala-sql/queries/audit_staging.rs`, audit journey | SQL `pg_audit_staging`; server journey 12/12 | FAIL |
| AC-006 — Tenant and authorization evidence demonstrates isolation across | `vala-bifrost-redux`, `wyrd-server/oracle`, `wyrd-testing/tests/bifrost` | gate Bifrost 9 lanes; scoped Oracle/Forge/OTLP journeys | PASS |
| AC-007 — Fixture evidence demonstrates isolated database lifecycle under | `mise.toml`, `.github/workflows`, Postgres scripts, generated outputs | gate exit 0; owner lanes; local cloud 2/backend | PASS |
| AC-007A — The completed conflict ledger enumerates the refreshed Oracle | `mise.toml`, `.github/workflows`, Postgres scripts, generated outputs | gate exit 0; owner lanes; local cloud 2/backend | PASS |
| AC-008 — Workflow evidence demonstrates affected-code pull-request lane | `mise.toml`, `.github/workflows`, Postgres scripts, generated outputs | gate exit 0; owner lanes; local cloud 2/backend | PASS |
| AC-009 — Final static review maps every REQ-* and INV-* to credible | `mise.toml`, `.github/workflows`, Postgres scripts, generated outputs | gate exit 0; owner lanes; local cloud 2/backend | PASS |
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
| AC-022 — Completion evidence records a passing result for every | `mise.toml`, `.github/workflows`, Postgres scripts, generated outputs | gate exit 0; owner lanes; local cloud 2/backend | PASS |

## Constraints and non-goals

| Obligation | Implementation evidence | Verification evidence | Result |
|---|---|---|---|
| C-01 architecture and human decisions govern both inputs | Approved spec; architecture; conflict ledger | Ledger/current source review | PASS |
| C-02 tenant, audit, WAL, terminal, resource safety | Redux/Oracle/audit source | Gate and domain journeys | PASS, except AC-005 proof |
| C-03 SDKs consume internal behavior only through shared client | `wyrd-client`, SDK manifests | client-tier/PyO3 checks, language journeys | PASS |
| C-04 retained owner Python features only as migration state | Python SDK aggregation/manifests | pyo3-scope, wheel, type checks | PASS |
| C-05 language-specific code has runtime/authoring need | SDK packages and N-API/PyO3 boundaries | package builds/journeys | PASS |
| C-06 no shared client abstraction beside `wyrd-client` | workspace/public exports | client-tier check | PASS |
| C-07 enforce single data root in production | `boot/data_root.rs`, `config.rs` | root/server journeys | PASS |
| B-01 SDK → shared client → server; engines server-owned | manifests, router, clients | client-tier check and journeys | PASS |
| B-02 query and Forge safety ordering | Oracle/Forge owners | Bifrost journeys | PASS |
| B-03 audit staging → frozen range → local Scribe → retention | audit publisher, SQL | SQL and server journeys | FAIL for combined AC-005 proof |
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
| TASK-001 | Human approval record; status approved | Redux/audit sources present; prior R2 findings have later source-level corrections; REQ-001 ancestry condition explicitly waived. |
| TASK-002 | Cumulative TASK-003-R4 PASS | Shared Bifrost facade/three SDKs present; language, cards, CLI, MCP and codegen evidence recorded. |
| TASK-003 | Cumulative TASK-003-R4 PASS | Gate, distinct owner lanes, CI, local cloud evidence on tested source. |
| TASK-004 | Cumulative TASK-003-R4 PASS | Data-root source and focused proof present. |
| TASK-005 | Human approval record; status approved | Later source removes login audit and Oracle audit WAL; current authorization and retained schema are checked against rev9. |
| TASK-006 | Human approval record; status approved | Current `publish_range`/`settle` are private and sweep is bounded concurrent; AC-005 server proof still incomplete. |

No full test suite or credentialed cloud task was rerun; raw runner logs were not supplied. The recorded gate and local-cloud counts are credible but cannot supply a scenario the test does not execute. The historical blocked report in a separate untracked review directory and unrelated `verified-change-contract` edits were excluded. No push, merge, deployment, source edit, or completion occurred.
