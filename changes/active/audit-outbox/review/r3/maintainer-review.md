# Maintainer review

## Subject and result

- Repository: Wyrd
- Base: `cf5ee4128ce0b842e00a0eb20770ab5c285dedd8`
- Candidate: `52e1144b5c186ccacd85d9c779a4e60c3cce5ac2`
- Scope: closure of `FIND-AUDIT-OUTBOX-11` under approved specification
  revision 4 and `FIND-AUDIT-OUTBOX-12`, `-13`, `-14`, `-7`, and `-3`, plus
  regressions introduced by the immutable range
- Overall result: **FAIL**

The changed runtime ownership is findable and cohesive: `Outbox` still owns
generic queueing and lifecycle, `AuditSink` owns Postgres commit-outcome
resolution, and the public journey drives the production writer and publisher.
Three bounded maintainer defects remain: two changed descriptions still state
the superseded unconditional-retry behavior, one materially changed test sink
and its new recorder methods lack the repository-required rustdoc, and the new
journey hides Tokio IO imports inside a function.

## Changed-surface coverage

| Changed surface | Owner, callers, and consumers inspected | Relevant proof inspected | Result |
|---|---|---|---|
| `wyrd-runtime::outbox`: `Outbox`, `OutboxWriter`, `QueueSender`, `Written`, `contain_panic`, `panic_message`, shutdown/admission state, metrics test recorder | Full owning module; `AuditOutbox` alias; server boot (`AuditSink::outbox`), `AppState` staging and server shutdown; representative auth, Gate, Oracle, gateway, and test-harness users of `stage`, `settle`, and `shutdown` | Seven inline outbox tests, especially panic retry, admission fencing, deadline loss, pending/gauge settlement, tenant independence, and ordered retry | **FAIL** for changed-item documentation only (`MAINT-R3-02`); ownership and test scenarios are otherwise clear |
| `vala-sql::audit_outbox`: `AuditSink::write` and `resolve_commit` | Full owner; `ValaPostgres::pool`/tenant-connection boundary; canonical `append_audit_events`; `AuditSink::outbox` callers in boot, state construction, auth tests, Oracle tests, and journeys | Exactly-once ambiguous-commit journey; `pg_audit_outbox` integration tests; implementer-reported focused and family results treated as claims, not proof by summary | PASS |
| Audit staging append and event-ID removal | `append_audit`, `append_audit_batch`, private `append_audit_events`, `AuditRows`; publisher reads through `list_publication_batch`; staging row projection | Existing staging/outbox integration coverage and exactly-once journey; event-ID retry test removal is consistent with revision 4 | PASS |
| Retained projection, publisher-facing harness reads, and ambiguous-commit journey | `project_audit_rows`; server publisher flow; `WyrdTestServer` retained-count/record helpers; new `CommitCutter`, `relay`, and journey | Projection test and the production writer/publisher journey, including three acknowledgement-loss rounds and one aborted transaction | **FAIL** for the function-scoped import (`MAINT-R3-03`); scenario naming and assertions otherwise make the required outcome legible |
| Unwrap/expect audit | `CFG_TEST_MODULES`, `is_ignored_path`, `audit`, CLI `main`; all four declaring modules named by the allowlist | Fixture script passed independently; `mise run check:unwrap-audit` passed independently with a writable temporary tool cache | PASS |
| Architecture, runbook, security posture, server module docs, and public Bifrost docs | Changed passages compared with revision 4 `REQ-009`/`AC-009`, the runtime owner, and nearby audit authority | Static source comparison | **FAIL** because two live descriptions retain superseded unconditional-retry wording (`MAINT-R3-01`) |
| Deleted `20261003000001_audit_staging_event_id.sql` migration | Migration history, neighboring Vala migrations, migration integration test, revision 4 removal requirement | Git history shows the event-ID migration belonged to the unreleased event-ID implementation; current tree ends at audit publication progress for this change | PASS |

## Material findings

### MAINT-R3-01 — live owner documentation contradicts revision 4's unresolvable-outcome loss

- **Changed locations:**
  - `crates/vala/vala-sql/src/audit_outbox.rs:12-15`
  - `architecture/bifrost-design.md:589-595`
- **Governing principle:** `maintainer-style.md` requires documentation to
  explain durable failure and retry behavior accurately; approved revision 4
  `REQ-009` says a transaction status that Postgres no longer holds is counted
  lost and is not re-sent.
- **Evidence:** the changed `AuditSink` module description says a batch that
  fails to commit is "retried ... never dropped," and the live Bifrost read
  contract says a failed commit is "retried until it lands." The same range's
  `AuditSink::resolve_commit` returns success after counting `Ok(None)` events
  lost, and the changed runbook and later Bifrost publication section describe
  that loss correctly.
- **Maintenance cost:** there are now two incompatible instructions in the
  owner and design authority for the highest-risk branch. A maintainer changing
  retry or operating an audit gap cannot tell whether `NULL` is an accepted
  terminal loss or a retry obligation without rediscovering the revision-4
  decision from implementation.
- **Smallest testable correction:** update only these two descriptions to use
  the already-established wording nearby: a returned commit error is resolved
  from transaction status; committed succeeds, aborted retries, in-progress or
  unreachable waits, and unavailable status is counted lost without re-send.
  A repository text search for unconditional audit "retried until it lands" or
  "never dropped" wording is sufficient focused proof; no new check is needed.
- **Nearby pattern:** `architecture/operations/runbooks.md:119-134` and
  `AuditSink::resolve_commit`'s rustdoc already state the revision-4 behavior
  precisely.

### MAINT-R3-02 — materially changed test behavior is undocumented

- **Changed locations:**
  - `crates/shared/wyrd-runtime/src/outbox.rs:524-543`
  - `crates/shared/wyrd-runtime/src/outbox.rs:614-629`
- **Governing rule:** `AGENTS.md` section 16 and
  `architecture/agent-rules.md` require substantive rustdoc on every new or
  materially modified Rust item, including private test helpers and methods;
  fallible methods require `# Errors`, and panic/async behavior must be stated
  where relevant.
- **Evidence:** `MemorySink::write` was materially changed to signal dispatch,
  optionally wait, consume a one-shot panic injection, and then apply ordinary
  failure behavior, but has no item rustdoc. The newly added `Recorder`
  implementation's six methods also have no item rustdoc.
- **Maintenance cost:** the panic-closure proof depends on the precise order of
  notification, hanging, one-shot panic consumption, and ordinary failure. A
  later test edit can reorder those stages and silently stop proving the
  ownership invariant. The recorder's name-only aggregation and no-op
  histogram behavior are likewise hidden in method bodies even though the
  assertions rely on that deliberately narrow model.
- **Smallest testable correction:** add substantive rustdoc to the changed sink
  method describing its injection order, `# Errors`, `# Panics`, and wait/
  cancellation behavior, and to each new recorder method describing whether it
  records or intentionally ignores the metric. `mise run fmt` and the focused
  `wyrd-runtime` outbox tests are sufficient proof.
- **Nearby pattern:** the new `TestMetrics::{cell,raw,counter,gauge}` methods
  immediately above already document their test-specific semantics.

### MAINT-R3-03 — the new journey hides a module dependency inside `relay`

- **Changed location:**
  `crates/wyrd/wyrd-testing/tests/bifrost/server/audit_publication.rs:921`
- **Governing rule:** `architecture/agent-rules.md` requires all `use`
  statements at module scope so the module's dependency surface is visible;
  neither allowed exception applies here.
- **Evidence:** `relay` contains
  `use tokio::io::{AsyncReadExt, AsyncWriteExt};` inside the function.
- **Maintenance cost:** the already protocol-heavy journey cannot be audited
  from its module imports to see that it depends on Tokio's read/write extension
  traits; that is exactly the dependency-manifest cost the repository rule
  prevents.
- **Smallest testable correction:** move both trait imports to the existing
  module-level import block and keep the relay unchanged. `mise run fmt` and
  the exact ambiguous-commit journey are sufficient proof.
- **Nearby pattern:** the same file's new SQLx and standard-library dependencies
  are declared at the top of the module.

## Directed risk judgments and verification limits

- **No direct test for in-progress/unreachable or `NULL`:** accepted as a
  verification limitation in this maintainer pass, not an additional finding.
  `resolve_commit` keeps the complete policy in one documented owner, and the
  journey directly proves the two retry/duplicate decisions required by
  `AC-009` (committed and aborted). The in-progress/unreachable loop and
  unavailable-status loss remain source-inspection evidence only. Creating a
  trait or configurable status provider solely for these branches would add a
  production abstraction for one test; a future focused Postgres test would be
  useful if it can drive the real states without that seam.
- **A waiting batch holds one of four writer slots:** accepted. The behavior is
  stated in `AuditSink::resolve_commit`, the fixed pool share is named by
  `AUDIT_WRITER_CONNECTIONS`, and it preserves the required rule that nothing
  is re-sent while the transaction outcome is unknown. Three other tenants can
  still progress; no extra queue, task, or connection owner is justified by
  the approved behavior.
- **Deleted unreleased migration:** accepted and preferable. Revision 4 removes
  the event-ID schema entirely, the migration was introduced only for the
  superseded unpublished approach, and retaining a forward migration for a
  column absent from the final contract would leave dead schema and misleading
  upgrade history. Shipped migration immutability remains covered separately
  by the existing migration test.
- **Verification evidence:** the implementer reports the focused journey,
  Bifrost SQL integration, server journey, `test:wyrd`, format, lint, docs, and
  unwrap-audit lanes as green. This review independently ran the unwrap checker
  fixtures and `check:unwrap-audit`; both passed. `bench:capacity` and
  `mise run gate` remain deferred exactly as directed. The three findings are
  source-static and are not disproved by those green lanes.

## Calibration preferences (non-blocking)

- `CommitCutter` is substantial test-only protocol code, but it earns its place:
  no existing Postgres commit-acknowledgement injection owner was found, it
  avoids sleeps, and it proves the production writer/publisher seam required by
  `AC-009`. Extracting a generic proxy now would add an unsupported second
  consumer.
- The allowlist fixture mutates the checker module's `ROOT` for its short-lived
  test process. Keeping it local rather than adding a reset helper is acceptable
  while the script remains a standalone fixture runner.
