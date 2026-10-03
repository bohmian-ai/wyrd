# Maintainer Review

## Immutable subject

- Repository: `/home/thorrester/Documents/GitHub/wyrd/.claude/worktrees/agent-aad682fbca5074900`
- Base: `cf5ee4128ce0b842e00a0eb20770ab5c285dedd8`
- Candidate: `e54b1244f32950d1ab251dae6530c4e1694c78d5`
- Reviewed range: `cf5ee4128ce0b842e00a0eb20770ab5c285dedd8..e54b1244f32950d1ab251dae6530c4e1694c78d5`
- Approved specification: `changes/active/audit-outbox/spec.md`, revision 3
- Prior review and remediation authority:
  `changes/active/audit-outbox/review/r2/{verdict.md,findings-validation.md,TASK-AUDIT-OUTBOX-R2-bounded-remediation.md}`
- User-directed scope: closure of `FIND-AUDIT-OUTBOX-11`, `-12`, `-13`,
  `-14`, `-7`, and `-3`, plus regressions introduced by the reviewed range.
  `FIND-AUDIT-OUTBOX-5`, `bench:capacity`, and `mise run gate` remain
  integration-deferred.

The repository has no `.codegraph/` index, so navigation used the immutable
diff, direct owner-module reads, and caller/consumer/test searches.

## Changed-surface coverage

| Surface | Material owners and symbols reviewed | Callers, consumers, and proof traced | Result |
|---|---|---|---|
| Generic outbox panic recovery | `OutboxWriter::{dispatch,finish}`, `contain_panic`, `panic_message`, `Written`, and the in-memory `MemorySink`/metrics harness | `AuditSink::write`, retry/front-of-queue state, loss/write-failure/pending metrics, `a_panicking_write_is_retried_once_in_order_without_loss` | PASS for `FIND-AUDIT-OUTBOX-12`: the normal sink-construction and sink-poll panic paths return the owned batch to the existing retry path, preserve ordering, and do not count loss |
| Generic outbox shutdown | `Outbox::{queue,stage,shutdown}`, `QueueSender`, `OutboxWriter::run`, pending/gauge settlement | Server shutdown ownership, concurrent stage/shutdown behavior, `shutdown_refuses_items_staged_after_it_begins`, `shutdown_counts_items_unwritten_at_the_deadline_as_lost`, existing recovery-before-deadline test | PASS for `FIND-AUDIT-OUTBOX-13`: the handle closes admission before awaiting, pre-fence items drain, deadline remainder is counted once, and terminal pending state is cleared |
| Staging identity and retained projection | `AuditSink`, `StagedAuditEvent`, `AuditStagingRow`, `append_audit_events`, `AuditRows`, publication-range reads, `AuditLogTable::{EVENT_ID,arrow_fields}`, `project_audit_rows`, `project_record_batch` | `AuditPublisher` input/output shape, staging uniqueness, Scribe publication schema, projection unit tests, unknown-outcome production-publisher journey | PASS for carrying the original `event_id` into every retained row and preserving tenant/sequence projection; **FAIL** for the complete `FIND-AUDIT-OUTBOX-11` read contract (`MAINT-R3-01`) |
| Audit decision reads and proof | `WyrdTestServer::{retained_audit_records,retained_audit_rows,retained_audit_count,retained_audit_operation_count}` and `unknown_outcome_retries_retain_each_decision_at_most_twice_and_read_once` | Public HTTP and gRPC query services, MCP `bifrost.query`, CLI query, shared Rust `Bifrost::sql`, Python `Bifrost.sql`, TypeScript `Bifrost.sql`; canonical `vala.system` SQL authority | **FAIL**: only the test harness collapses duplicate event IDs; shipped SQL reads return the raw duplicate retained rows (`MAINT-R3-01`) |
| Unwrap/expect check | `CFG_TEST_MODULES`, `is_ignored_path`, extracted `audit`, fixture script and both fixture tests | All four allowlisted owner declarations, production `tests.rs` fixture, repository scan | PASS for `FIND-AUDIT-OUTBOX-14`: the basename-wide exemption is gone and the explicit entries name their cfg-test owners |
| Bare type ownership | `MutexGuard` import/use in the outbox test module; `Uuid` import/use in `AuditRows` and staging queries | Changed declarations and their tests | PASS for `FIND-AUDIT-OUTBOX-7`: changed signatures and fields use top-level imports and bare names |
| Security and operations wording | Bifrost design, security posture, runbook, docs-site architecture | Shared outbox metrics and owner, publisher/retirement behavior, retained-schema/read wording | PASS for `FIND-AUDIT-OUTBOX-3`; **FAIL** only where the revised documents claim read collapsing that production does not implement (`MAINT-R3-01`) |
| Retained-schema compatibility risk | `AuditLogTable::arrow_fields`, built-in registration fingerprinting, `BifrostCatalog::ensure_builtin/create_table_locked` | Prior removal of the unshipped audit compatibility path, revision-3 expensive-to-reverse decision, current registration behavior | PASS / accepted scope: the fingerprint changes and an older physical registration would fail strict startup validation, but revision 3 explicitly approves the retained-schema change, this schema has no shipped compatibility contract, and the repository deliberately removed the earlier unshipped compatibility mechanism. Adding a path here would reintroduce out-of-scope compatibility machinery |
| Range-wide regression scan | All 17 changed paths, including the approved spec/task records, authority/docs changes, SQL row/query changes, projection, harness, journey, and checker | Nearest owner modules, public consumers, changed tests, and recorded implementation evidence | No additional material regression found within the user-directed boundary |

## Material findings

### MAINT-R3-01 — `FIND-AUDIT-OUTBOX-11` is closed only in the test harness, not in shipped audit reads

- **Classification:** `INCORRECT`
- **Changed location:**
  `crates/wyrd/wyrd-testing/src/server.rs:1700-1800,1803-1856` adds duplicate
  collapsing only to `WyrdTestServer`: listings fetch raw rows and discard
  repeated `event_id` values in a local `HashSet`, while counts issue
  `SELECT DISTINCT event_id`. The changed journey explicitly characterizes the
  result as "the harness's audit count and listing" at
  `crates/wyrd/wyrd-testing/tests/bifrost/server/audit_publication.rs:909-929`
  and exercises those helpers at lines 1033-1049. The production table's new
  rustdoc promises that a reader collapses duplicates at
  `crates/vala/vala-bifrost-redux/src/tables/audit/audit_log.rs:22-25`, but no
  production read owner changed to enforce that promise.
- **Governing rule / guide principle:** revision-3 REQ-009 and AC-009 require
  audit reads that count or list decisions to collapse rows sharing
  `(tenant,event_id)`. `architecture/bifrost-design.md:639-644` repeats that
  contract, while lines 989-1004 establish ordinary canonical SQL as the only
  read contract for `vala.system`. `AGENTS.md` requires durable server behavior
  to live in its Rust server owner and forbids language-specific duplicate
  implementations. Maintainer Style requires tests to prove the caller-visible
  outcome rather than replace its implementation with harness behavior.
- **Caller and reachability evidence:** `POST /v1/query` forwards the supplied
  `BifrostQueryRequest` to `stream_query`
  (`crates/wyrd/wyrd-server/src/query/routes.rs:241-295`), and the gRPC method
  does the same (`crates/wyrd/wyrd-server/src/grpc/query.rs:103-143`). MCP
  forwards its arbitrary SQL request through that owner
  (`crates/wyrd/wyrd-server/src/mcp/bifrost.rs:376-427`). The CLI streams the
  same request unchanged (`crates/wyrd/wyrd-cli/src/query/mod.rs:67-81`), and
  the shared Rust facade documents and executes SQL over any authorized table
  (`crates/shared/wyrd-client/src/bifrost/facade.rs:592-625`). Python and
  TypeScript likewise pass arbitrary SQL through their shared Bifrost clients
  (`sdks/wyrd-sdk-python/python/wyrd/bifrost/__init__.py:551-572` and
  `sdks/wyrd-sdk-ts/wyrd/src/index.ts:840-880`). No distinct shipped audit
  list/count owner or UI audit reader was found; therefore every shipped audit
  read is the ordinary SQL path, and a `COUNT(*)` or row listing over
  `vala.system.audit_log` observes both retained copies.
- **Concrete maintenance cost:** maintainers are given a green AC-009 journey
  and architecture prose saying one authorization decision is read once, but
  users of HTTP, gRPC, MCP, CLI, Rust, Python, or TypeScript receive two rows
  and a count of two after the exact accepted retirement/retry race. The only
  correct semantics live in a test-only helper that no production caller can
  reach. This makes audit cardinality wrong at every supported headless surface
  and invites each client to invent a different downstream guard.
- **Smallest testable correction:** enforce one logical row per tenant/event ID
  at the existing server-owned canonical Bifrost read boundary for
  `vala.system.audit_log`, before arbitrary SQL projection or aggregation, so
  the shared HTTP/gRPC engine and MCP/CLI/Rust/Python/TypeScript projections all
  inherit one implementation. Preserve both raw retained rows for publication,
  recovery, and internal evidence; do not add client-side filters, a second
  audit table, a second publisher, or another storage authority. Replace the
  harness-only proof with a production-publisher journey that creates the
  two-row retained state, then uses an ordinary authenticated `Bifrost` query
  to prove both `COUNT(*)` and a row listing expose one decision. Because MCP is
  agent-facing, include its existing `bifrost.query` path in the owning journey
  coverage or demonstrate that it consumes the exact same corrected query
  result without a separate implementation.

## Calibration notes

- The retained `audit_log` schema fingerprint does change without an upgrade
  path. That is a real operational fact, but not a finding in this review:
  revision 3 explicitly approves the retained-schema addition, the repository's
  earlier `b7185d0ee` change deliberately deleted compatibility for this
  unshipped schema, and the approved remediation forbids expanding into a new
  compatibility mechanism. `BifrostCatalog::create_table_locked` will continue
  to reject a genuinely older registration rather than silently reinterpret it.
- `OutboxWriter::finish` retains its defensive `JoinError` loss branch for a
  panic that escapes construction/poll containment (its comment names a panic
  while dropping a panic payload). The changed production `AuditSink` supplies
  no reachable instance of that exotic path, while the in-scope ordinary sink
  panic now returns the owned items and is directly proved. I therefore did not
  reopen `FIND-AUDIT-OUTBOX-12` on a hypothetical custom panic payload.
- The `CFG_TEST_MODULES` values are documentation/evidence, not dynamically
  validated owner paths. That matches the r2 correction's explicitly allowed
  narrow allowlist and does not justify adding a Rust parser or a check that
  verifies another check.
- No dedicated audit UI surface was found. This does not narrow REQ-009: Wyrd
  is headless and the shipped SQL surfaces already make the duplicate result
  reachable.

## Verification assessment

- Independently ran the three exact `wyrd-runtime` closure tests for panic
  retry, shutdown fencing, and deadline loss: 3/3 passed.
- Independently ran `vala-bifrost-redux`'s audit projection unit selection:
  7/7 passed.
- Independently ran `mise exec -- python3 scripts/test_check_unwrap_audit.py`
  and the production checker directly through the pinned mise environment;
  both passed. `mise run check:unwrap-audit` itself could not acquire the
  sandboxed uv cache lock because that cache is read-only in this review
  environment; the implementer's recorded owning-lane result is PASS.
- Independently ran `git diff --check base..candidate`; it passed.
- The implementer's Postgres journey evidence is credible for event-ID
  propagation, staging uniqueness, raw retained duplication, and the harness
  helpers. It is not closure evidence for the shipped read contract because the
  asserted collapse occurs after the production query returns raw duplicates.
- The user-deferred capacity benchmark and aggregate gate were not run.

## Overall result

**FAIL**

`FIND-AUDIT-OUTBOX-12`, `-13`, `-14`, `-7`, and `-3` are maintainably closed,
and the range introduces no other material regression found in this audit.
`FIND-AUDIT-OUTBOX-11` remains open: revision 3's retained identity is present,
but the required one-decision read semantics exist only in test-only helpers,
while every shipped audit read surface still exposes the duplicate retained
rows through canonical SQL.
