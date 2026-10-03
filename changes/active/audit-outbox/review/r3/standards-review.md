# Repository standards review

## Review result

**FAIL.** The remediation range closes the repository-rule defects behind
FIND-AUDIT-OUTBOX-12, -13, -14, -7, and -3, and it carries the durable event
ID required by revision 3. Two material repository-rule failures remain at the
new retained-data/read boundary: an already-created retained table has no
schema-evolution path, and the shipped audit query surfaces still expose
duplicate physical rows as duplicate decisions.

This is a repository-standards audit only. It does not independently adjudicate
task acceptance outside the user-directed closure scope.

## Material findings

### STD-R3-001 — the required retained-schema change has no upgrade-compatible path

- **Severity:** important
- **Classification:** VIOLATION / REGRESSION
- **Location:**
  `crates/vala/vala-bifrost-redux/src/tables/audit/audit_log.rs:56-73`;
  `crates/vala/vala-bifrost-redux/src/catalog/bifrost_catalog.rs:1020-1042,1110-1125`;
  `crates/vala/vala-bifrost-redux/src/scribe/ingress.rs:183-205`.
- **Violated authority:** `architecture/references/domain/iceberg.md:22-29`
  requires explicit compatibility for nullability and required-field changes;
  `docs/src/content/docs/bifrost/architecture.svx:281-285` states that a
  registered physical schema is frozen and requires an explicit migration.
- **Evidence:** the range adds required, non-null `event_id` to
  `AuditLogTable::arrow_fields`. Scribe calls `ensure_builtin` before accepting
  the logical frame, and `create_table_locked` rejects any existing catalog row
  whose stored fingerprint differs from the new declaration. The range has no
  retained-Iceberg migration, catalog-fingerprint transition, old-row backfill
  policy, or other compatibility path. The only event-ID migration in the
  cumulative change is for transient Postgres staging, not retained
  `vala.system.audit_log`.
- **Consequence:** a deployment that already materialized
  `vala.system.audit_log` cannot publish the new projection. Each publisher
  sweep reaches the schema fingerprint mismatch, leaves staging outstanding,
  and retries later; retained audit history therefore stops advancing. Old
  retained rows also have no event ID from which the new decision-cardinality
  contract can be reconstructed.
- **Testable correction:** make the persistent-data decision explicit and
  implement it at the catalog/table owner: preserve field identities and define
  how the new column and pre-change rows evolve, or approve and implement a
  deliberate replacement/backfill policy. Add an upgrade journey that creates
  the old retained table with data, starts the candidate, publishes a new audit
  decision, and proves both old and new history remain queryable with the chosen
  event-ID semantics. A fresh-install test is not sufficient.

### STD-R3-002 — shipped audit reads do not collapse retained duplicates by event ID

- **Severity:** important
- **Classification:** INCORRECT / REGRESSION
- **Location:**
  `crates/wyrd/wyrd-testing/src/server.rs:1700-1801,1803-1850` versus
  `crates/wyrd/wyrd-server/src/query/routes.rs:241-292`,
  `crates/wyrd/wyrd-server/src/query/service.rs:200-213`,
  `crates/wyrd/wyrd-server/src/mcp/bifrost.rs:376-420`,
  `crates/wyrd/wyrd-cli/src/query/mod.rs:94-122,145-201`, and
  `crates/shared/wyrd-client/src/bifrost/facade.rs:645-693`.
- **Violated authority:** `architecture/bifrost-design.md:636-645`,
  `architecture/wyrd-security-posture.md:367-374`, and
  `docs/src/content/docs/bifrost/architecture.svx:323-330` all require audit
  reads that count or list decisions to collapse rows sharing tenant and event
  ID. `AGENTS.md` requires audit to remain foundational across HTTP, MCP, CLI,
  SDK, UI, server, and Vala surfaces and requires user-observable behavior to be
  proved at the shipped journey tier.
- **Evidence:** the only decision-aware readers in the range are
  `WyrdTestServer` helpers: listings use an in-memory `HashSet<event_id>` and
  counts issue `SELECT DISTINCT event_id`. The production HTTP `/query` route
  passes caller SQL unchanged to Oracle; `bifrost.query`, the CLI query command,
  Rust `Bifrost`, and the Python and TypeScript Bifrost facades project that
  same raw SQL result. A shipped `SELECT * FROM vala.system.audit_log` listing
  therefore returns both rows for one decision, and `SELECT count(*)` counts
  both. The remediation journey proves this exact physical duplicate exists at
  `crates/wyrd/wyrd-testing/tests/bifrost/server/audit_publication.rs:1006-1049`
  but proves collapse only through the harness helpers. No dedicated retained
  audit-log reader exists in HTTP, MCP, CLI, Rust SDK, Python SDK, TypeScript
  SDK, or the UI; the UI's unrelated change-workspace “Audit” view is not a
  `vala.system.audit_log` consumer.
- **Consequence:** the revision-3 at-least-once behavior makes ordinary shipped
  audit listings and counts report one authorization decision more than once.
  This is an audit-result error, not merely a presentation difference.
- **Testable correction:** define one server-owned decision-level audit read
  contract (for example, a canonical decision view/query owner) that collapses
  by `(tenant, event_id)`, project it consistently through every shipped audit
  count/list surface, and retain an explicitly named raw-row path only if the
  architecture requires forensic physical delivery inspection. Add a real
  public-surface journey that creates the post-retirement duplicate and proves
  decision count/list cardinality through the HTTP contract and each exposed
  MCP/CLI/SDK projection. Do not hide this solely in test helpers or attempt an
  unsafe rewrite of arbitrary caller SQL.

## Immutable subject and scope

| Item | Reviewed value |
|---|---|
| Base | `cf5ee4128ce0b842e00a0eb20770ab5c285dedd8` |
| Candidate | `e54b1244f32950d1ab251dae6530c4e1694c78d5` |
| Range | Base-exclusive remediation range only |
| Approved authority | `changes/active/audit-outbox/spec.md`, revision 3, approved in `d61114979` |
| Prior review inputs | r2 `verdict.md`, `findings-validation.md`, and `TASK-AUDIT-OUTBOX-R2-bounded-remediation.md` including implementer evidence |
| Closure scope | FIND-AUDIT-OUTBOX-11, -12, -13, -14, -7, -3, plus regressions introduced by the range |
| Explicit deferrals | FIND-5 / `bench:capacity` and `mise run gate` remain integration-owned and were not assessed as failures |

No `.codegraph/` directory exists at the repository root, so the repository's
CodeGraph-first rule did not apply.

## Authority coverage

| Changed surface / risk | Owning boundary | Authorities and routed references read completely | Source, consumer, and proof inspected | Coverage |
|---|---|---|---|---|
| Generic non-blocking outbox, panic retry, shutdown fence, metrics | `crates/shared/wyrd-runtime` | `AGENTS.md`; `architecture/agent-rules.md`; `architecture/wyrd-design.md`; `architecture/references/languages/{rust-core,maintainer-style,implementation-execution,testing-workflows}.md`; `architecture/references/architecture/patterns.md` | `outbox.rs`, its remediation diff and focused tests; `wyrd-runtime/Cargo.toml`; concrete audit sink | Complete |
| Audit append, staged event identity, RLS, row decoding | `crates/vala/vala-sql` | design/security authorities; `architecture/references/domain/{vala-architecture,olap-serving}.md`; Rust/testing rules | `audit_outbox.rs`; `queries/audit_staging.rs`; `row_types/audit_staging.rs`; SQL test evidence; manifest | Complete |
| Retained schema, projection, catalog compatibility, Scribe ingress | `crates/vala/vala-bifrost-redux` | `architecture/bifrost-design.md`; `architecture/references/domain/{iceberg,analytical-operations-reliability,olap-serving}.md`; public Bifrost docs | `audit_log.rs`; `projection.rs`; table registry; `ensure_builtin`/fingerprint branch; Scribe ingress; manifest | Complete; **FAIL STD-R3-001** |
| Audit publisher and at-least-once lifecycle | `crates/wyrd/wyrd-server` | Wyrd design; Bifrost design; security posture; operations runbook | production `AuditPublisher`, frozen range, Scribe append, settlement and retry path | Complete |
| Audit decision counts/listings and public readers | server query plus first-class clients | headless/client-server doctrine; Bifrost design; security posture; agent-harness/testing references | harness readers and journey; HTTP query route; MCP `bifrost.query`; CLI query; Rust/Python/TypeScript Bifrost clients; UI search | Complete; **FAIL STD-R3-002** |
| Unwrap/expect production check | repository tooling | `AGENTS.md` completion/check rules; `architecture/agent-rules.md`; implementation/testing references | checker, four cfg-test owner declarations, fixtures, canonical mise task | Complete |
| Changed architecture and operator prose | repository authorities/docs | `architecture/references/README.md`; Wyrd/Bifrost designs; doctrine; security posture; operations runbook; docs Bifrost architecture | range diff and ownership wording | Complete |
| Dependency, feature, and verification ownership | workspace/manifests | `AGENTS.md` §§3-11, 15-16; Rust/testing references | `mise.toml`; affected crate manifests; range contains no dependency or feature changes | Complete |

Also read completely because the router marked them applicable:
`architecture/wyrd-doctrine.mdx`,
`architecture/references/languages/spec-driven-development.md`,
`architecture/references/languages/agent-harness.md`, and
`architecture/references/doctrine/architecture-constraints.md`.

## Applicable repository-rule results

| Rule family | Exact evidence | Result |
|---|---|---|
| One audit write path and one publisher | The range adds no writer, table, WAL, relay, or publisher. `AuditSink` remains the canonical staging append caller and the existing `AuditPublisher` remains the only retained-history mover. | PASS |
| Permission blocks; audit does not | The range leaves surface staging asynchronous and changes only the shared outbox owner, retained projection, harness, checker, and authority prose. No request path begins awaiting an audit commit. | PASS |
| Failed accepted batches retry; live-process panic is not an allowed loss boundary | `outbox.rs:328-343` catches unwind during sink-future construction and polling while the child still owns `items`; `finish` routes the returned error through the existing front-of-queue/backoff path. The concrete sink error/display path is non-panicking, and the focused panic test reported green. | PASS — FIND-12 closed |
| Shutdown is a handle-owned one-way fence and terminal gauges settle | `outbox.rs:148-160` sends while holding the read lock; `shutdown` takes/drops the sender under the write lock before awaiting, and `pending.swap(0)` decrements the gauge/counts the deadline remainder once. Focused fence/deadline tests are recorded green. | PASS — FIND-13 closed |
| Tenant isolation | The event-ID lookup and inserts still run through `TenantConn`; retained projection validates every row tenant before construction; no range change widens query or publication tenant authority. | PASS |
| Required retained-schema compatibility | Required `event_id` changes the persisted fingerprint; the existing-registration branch returns `FingerprintMismatch`, and no upgrade path is present. | **FAIL — STD-R3-001** |
| Audit decision reads collapse at-least-once duplicates | Harness-only `HashSet`/`DISTINCT` collapse is correct, but shipped raw HTTP/MCP/CLI/SDK audit queries return physical duplicates. | **FAIL — STD-R3-002** |
| Struct-centered ownership / async only at IO | `Outbox`/`OutboxWriter`, `AuditSink`, catalog, publisher, and test-server handle remain cohesive dependency-owning structs; pure projection/validation stays synchronous. No zero-sized utility owner or speculative trait was added. | PASS |
| Crate and dependency boundaries | Runtime remains SQL-free; SQL and DataFusion/Iceberg dependencies stay in Vala/server owners; no manifest, feature, lockfile, client-tier SQL/cloud edge, or PyO3 scope changed. | PASS |
| Rust import style | `MutexGuard` and `Uuid` are imported at module scope and used bare in the cited declarations. | PASS — FIND-7 closed |
| Rust documentation | New/materially modified Rust items, fields, helpers, async/durable workflows, fallible functions, and tests have intent/invariant documentation and applicable Errors/Panics/cancellation/retry detail. | PASS |
| Production unwrap audit cannot be bypassed by basename | `CFG_TEST_MODULES` names four exact files and their declaring modules; `is_ignored_path` no longer exempts arbitrary `tests.rs`; the identical-body production fixture is rejected. | PASS — FIND-14 closed |
| Live authority names the actual owner | Security posture now uses shared audit-outbox write/publication vocabulary and no longer assigns commit failure to Oracle. | PASS — FIND-3 closed |
| Test hierarchy and shipped-surface coverage | The Postgres publisher journey exercises the retirement duplicate and harness-level collapse. It does not prove a shipped decision-level read surface, and there is no old-schema upgrade journey. | **FAIL — STD-R3-001, STD-R3-002** |
| No unrelated churn / minimum cohesive remediation | Production changes stay within the six closure findings and the required authority updates. The checker fixture is narrow; no new abstraction, table, queue, relay, or dependency was added. | PASS |
| Public/generated contract checks | No wire request/response, proto, OpenAPI registration, Python stub, or TypeScript declaration was changed. `event_id` is a retained internal table field, so no generated public schema was expected from this range. | PASS |

## Closure status under repository rules

| Prior finding | Standards disposition |
|---|---|
| FIND-AUDIT-OUTBOX-11 | **Not closed overall.** Event identity is correctly retained and the production publisher journey reaches the accepted at-least-once case, but the retained schema is not upgrade-compatible and only harness readers collapse the duplicate. |
| FIND-AUDIT-OUTBOX-12 | CLOSED |
| FIND-AUDIT-OUTBOX-13 | CLOSED |
| FIND-AUDIT-OUTBOX-14 | CLOSED |
| FIND-AUDIT-OUTBOX-7 | CLOSED |
| FIND-AUDIT-OUTBOX-3 | CLOSED |

## Open questions

- None that changes this standards result. “The schema was unshipped” is not a
  committed repository authority or deployment constraint and therefore cannot
  substitute for the required compatibility decision.

## Verification notes

- Independently ran `mise exec -- python3 scripts/test_check_unwrap_audit.py`:
  passed.
- Independently ran `mise run check:unwrap-audit` with a writable temporary uv
  cache after the default user cache was read-only: passed.
- Independently ran
  `git diff --check cf5ee4128ce0b842e00a0eb20770ab5c285dedd8..e54b1244f32950d1ab251dae6530c4e1694c78d5`:
  passed.
- The immutable remediation record reports: focused `wyrd-runtime` outbox tests
  7/7; Redux audit tests 7/7; Postgres Bifrost SQL 119/119; audit publisher
  journey and server journey 31/31; `test:wyrd` 2329/2329; format, lints,
  docs, client-tier, and unwrap checks green.
- I did not rerun Cargo/nextest lanes in parallel with the other independent
  reviewers; the recorded exact commands and source-local assertions were
  inspected. No source diagnosis here depends solely on those recorded results.
- FIND-5 / `mise run bench:capacity` and `mise run gate` remain explicitly
  deferred to integration and are not residual gaps in this report.
- Candidate HEAD was `e54b1244f32950d1ab251dae6530c4e1694c78d5`
  before review actions. The only workspace write by this reviewer is this
  report; final HEAD verification follows report creation.
