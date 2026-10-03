# Audit outbox r3 behavior review

## Immutable subject and scope

- Repository: `/home/thorrester/Documents/GitHub/wyrd/.claude/worktrees/agent-aad682fbca5074900`
- Base: `cf5ee4128ce0b842e00a0eb20770ab5c285dedd8`
- Candidate: `e54b1244f32950d1ab251dae6530c4e1694c78d5`
- Reviewed range: `cf5ee4128..e54b1244f`
- Authority: `changes/active/audit-outbox/spec.md`, approved revision 3
- Prior hypotheses: `review/r2/verdict.md`, `review/r2/findings-validation.md`, and `review/r2/TASK-AUDIT-OUTBOX-R2-bounded-remediation.md`
- User-directed closure scope: `FIND-AUDIT-OUTBOX-11`, `-12`, `-13`, `-14`, `-7`, `-3`, and regressions introduced by the reviewed range. `FIND-5`, `bench:capacity`, and `mise run gate` remain deferred.

The repository has no `.codegraph/` directory. I traced the changed outbox, SQL staging, retained projection, production publisher, journey harness, unwrap checker, and the shipped query paths. I did not reopen earlier passed code except where revision 3's newly accepted retained duplicates make a shipped audit result incorrect.

## Caller-to-result trace

1. `Outbox::stage` owns the shutdown admission fence; `OutboxWriter` owns per-tenant dispatch, retry, panic containment, pending settlement, and deadline abandonment (`crates/shared/wyrd-runtime/src/outbox.rs`).
2. `AuditSink` commits a `StagedAuditEvent`, including its stable `event_id`, through the one SQL append (`crates/vala/vala-sql/src/audit_outbox.rs`, `queries/audit_staging.rs`).
3. `AuditPublisher` reads a frozen staging range and sends `project_audit_rows` through the existing Scribe path (`crates/wyrd/wyrd-server/src/audit/publication.rs`). The changed projection carries `event_id` into the fifteenth retained content column (`crates/vala/vala-bifrost-redux/src/tables/audit/{audit_log.rs,projection.rs}`).
4. An unknown-outcome retry after retirement creates a second retained row with the same tenant/event ID, as revision 3 allows and the new journey proves (`crates/wyrd/wyrd-testing/tests/bifrost/server/audit_publication.rs:988-1049`).
5. Only the test harness collapses those rows: listings use an in-process `HashSet`, and counts issue `SELECT DISTINCT event_id` (`crates/wyrd/wyrd-testing/src/server.rs:1715-1800,1828-1856`). Production HTTP and gRPC accept the caller's `BifrostQueryRequest` unchanged; MCP, CLI, and the Rust/Python/TypeScript SDKs project the same arbitrary SQL query. Thus `SELECT * FROM vala.system.audit_log` returns both copies and `SELECT count(*) ...` counts both.

## Closure acceptance matrix

| Obligation | Implementation evidence | Verification evidence | Result |
|---|---|---|---|
| `FIND-AUDIT-OUTBOX-11`; revision 3 REQ-009/AC-009: retain `event_id`, permit at most one post-retirement retained duplicate, and collapse duplicate decisions on audit count/list reads | `AuditLogTable` adds non-null `event_id` at `audit_log.rs:45-73`; `AuditStagingRow` and every publication select carry it; `projection.rs` projects it. The journey forces commit-success/error, retirement, retry, and two raw retained rows. Collapse exists only in `wyrd-testing/src/server.rs:1715-1800,1828-1856`. | Projection tests pass 7/7. Implementer evidence records the Postgres production-publisher journey passing. That journey asserts collapse through test-only helpers, not a shipped read surface. | **FAIL — `BEH-R3-001`** |
| `FIND-AUDIT-OUTBOX-12`; REQ-003/003a/008: a panicking sink does not lose accepted work | `OutboxWriter::dispatch` catches both future construction and polling panics while retaining ownership of `items`, maps them into the existing error/retry path, and restores the batch at the tenant queue front. | Independently ran all `outbox::tests`: 7/7 pass, including `a_panicking_write_is_retried_once_in_order_without_loss`. | PASS |
| `FIND-AUDIT-OUTBOX-13`; REQ-007/008: shutdown fences admission, drains pre-fence work, settles exact deadline loss, and leaves pending count/gauge at zero | `Outbox::queue` is a handle-owned `RwLock<Option<Sender>>`; `shutdown` takes the sender before its first await; late `stage` calls are refused/count lost; terminal shutdown swaps pending to zero and decrements the gauge. | Independently ran all `outbox::tests`: 7/7 pass, including the admission-race and deadline-loss/gauge scenarios. | PASS |
| `FIND-AUDIT-OUTBOX-14`: production `tests.rs` files remain covered by unwrap audit | The basename exemption is deleted. `CFG_TEST_MODULES` explicitly names the four cfg-test modules and their declaring files; an identical production fixture is scanned. | `mise exec -- python3 scripts/test_check_unwrap_audit.py`: PASS. Direct checker invocation: PASS. The canonical `mise run check:unwrap-audit` could not acquire the sandbox-read-only uv cache lock, so its wrapper was not independently completed. | PASS |
| `FIND-AUDIT-OUTBOX-7`: changed declarations use top-level imports and bare type names | `MutexGuard` is imported in the outbox test module and used bare; production `Uuid` is imported at the top of `audit_staging.rs` and used bare in the affected declarations. | Implementer reports `mise run fmt` and `mise run lints` passing; focused source inspection confirms the cited declarations. | PASS |
| `FIND-AUDIT-OUTBOX-3`: live authority names the shared audit-outbox owner | `architecture/wyrd-security-posture.md:424-427` now says `audit outbox write failure`; related design/runbook text consistently describes shared outbox retry and retained duplicates. | Implementer reports `mise run docs:check` passing; focused source search found no remaining "Oracle audit commit failure" phrase. | PASS |
| Range introduces no regression | Outbox lifecycle, projection, SQL rows, checker, tests, and docs were inspected across the full range. | `git diff --check`: PASS. One user-visible audit-cardinality regression remains: raw shipped query reads expose the newly accepted retained duplicate. | **FAIL — `BEH-R3-001`** |

## Explicit risk judgments

### Retained schema fingerprint changes without a compatibility path

**No behavior finding in this closure review.** Adding non-null `event_id` changes the built-in `audit_log` fingerprint, and `BifrostCatalog::create_table_locked` will reject an already registered older fingerprint rather than migrate it. That would block publication on an installation carrying the earlier physical table. In this immutable subject, however, the older shape is an intermediate of the still-active, not-yet-integrated audit-outbox change. Repository history explicitly removed the earlier audit compatibility path because the schema had not shipped (`b7185d0ee`), and no release tag contains the reviewed base. Revision 3 explicitly approves the retained-schema change. Adding an upgrade-only compatibility mechanism for an unshipped intermediate would be scope expansion, not closure of an approved obligation.

This judgment is limited to this active change. If integration evidence establishes that the pre-`event_id` retained schema has shipped or is an upgrade-supported deployment state, the candidate is not compatible: `ensure_builtin` will return `FingerprintMismatch` and the publisher cannot retain new decisions. That new fact would require an explicit migration decision and proof.

### Only harness readers collapse duplicates

**Confirmed defect.** There is no dedicated production audit count/list API. Instead, every shipped headless surface reaches the generic Oracle SQL path:

- HTTP: `crates/wyrd/wyrd-server/src/query/routes.rs:241-295`
- gRPC: `crates/wyrd/wyrd-server/src/grpc/query.rs:120-136`
- MCP: `crates/wyrd/wyrd-server/src/mcp/bifrost.rs:142-156,386-420`
- CLI: `crates/wyrd/wyrd-cli/src/query/mod.rs:67-80,100-122`
- Rust SDK: `crates/shared/wyrd-client/src/bifrost/facade.rs:645-693`
- Python SDK: `sdks/wyrd-sdk-python/src/bifrost/mod.rs:448-482`
- TypeScript SDK: `sdks/wyrd-sdk-ts/wyrd/src/index.ts:871-880`

An authorized caller can therefore list or count retained audit decisions with ordinary SQL, and the server returns the physical duplicate rows. No production UI audit count/list surface was found, but absence of a UI does not repair the primary HTTP/MCP/CLI/SDK contracts. The journey itself documents that “raw history shows both” while only “the harness's audit count and listing” collapse them (`audit_publication.rs:915-921`). That falls short of REQ-009 and AC-009.

## Proposed findings

### BEH-R3-001 — shipped audit reads expose one authorization decision twice

- **Classification:** `MISSING / REGRESSION`
- **Violated obligation:** revision 3 REQ-009 and AC-009: audit reads that count or list decisions collapse rows sharing `(tenant, event_id)`; the range must introduce no regression in audit cardinality.
- **Changed location:** `crates/vala/vala-bifrost-redux/src/tables/audit/audit_log.rs:22-25`; insufficient production proof at `crates/wyrd/wyrd-testing/src/server.rs:1715-1800,1828-1856` and `crates/wyrd/wyrd-testing/tests/bifrost/server/audit_publication.rs:915-921,1033-1049`.
- **Producer-to-consumer evidence:** the production publisher retains the same event ID twice after the tested retirement race. The authoritative table declares both rows. The test harness removes the second row after Oracle has returned it or asks Oracle for `DISTINCT event_id`; production query transports do neither and send arbitrary caller SQL through the shared query service unchanged.
- **Reachable consequence:** after an unknown commit outcome and publication-before-retry race, an authorized HTTP, gRPC, MCP, CLI, Rust SDK, Python SDK, or TypeScript SDK caller running a normal audit listing sees two decisions, and `count(*)` reports two authorization decisions where one occurred. Compliance, alerting, and incident reconstruction overcount the authoritative audit history.
- **Required testable correction:** own duplicate collapse in the production retained-audit read boundary shared by every transport, before caller-visible count/list semantics are evaluated, while preserving the two raw retained rows and tenant isolation internally. Do not duplicate guards in each client or leave SQL callers responsible for remembering `DISTINCT event_id`. Extend the production-publisher unknown-outcome journey to query through at least the public HTTP and MCP/Rust client surfaces and prove both a row listing and an aggregate count return one decision for the duplicated event ID; retain an internal proof that physical history carries two rows with the same ID.

No other material finding was established in the user-directed scope.

## Verification notes

- Independently run:
  - `mise exec -- cargo nextest run --locked -p wyrd-runtime --lib -E 'test(/^outbox::tests::/)'` — 7/7 PASS.
  - `mise exec -- cargo nextest run --locked -p vala-bifrost-redux --lib -E 'test(/^tables::audit::/)'` — 7/7 PASS.
  - `mise exec -- python3 scripts/test_check_unwrap_audit.py` — PASS.
  - `mise exec -- python3 scripts/check_unwrap_audit.py` — PASS.
  - `git diff --check cf5ee4128..e54b1244f` — PASS.
- `mise run check:unwrap-audit` did not complete in this sandbox because uv could not create its lock file under the read-only user cache. Its underlying checker passed directly.
- The implementer records the Postgres-backed unknown-outcome production-publisher journey, `test:bifrost:journey:server`, SQL integration, formatting, lints, docs, client-tier, and broader Wyrd tests as passing. I reviewed their source and evidence but did not rerun the Postgres-backed/broad lanes here.
- `bench:capacity` and `mise run gate` are deferred by user direction.

## Overall result

**FAIL**

`FIND-AUDIT-OUTBOX-12`, `-13`, `-14`, `-7`, and `-3` are closed. `FIND-AUDIT-OUTBOX-11` is only partially closed: the retained identity and at-most-one-extra-row behavior are implemented, but the required collapse is test-harness-only and not present on shipped audit reads. The candidate therefore does not yet satisfy revision 3's REQ-009/AC-009.
