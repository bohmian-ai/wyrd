# Audit outbox r3 security and tenancy domain review

## Result

**PASS**

No material security, RBAC, tenant-isolation, attribution, or audit-integrity
finding remains in the closure scope. The reviewed range closes
`FIND-AUDIT-OUTBOX-11`, `-12`, `-13`, `-14`, `-7`, and `-3` for this domain and
does not introduce a security or tenancy regression.

## Immutable subject

- Base: `cf5ee4128ce0b842e00a0eb20770ab5c285dedd8`
- Candidate: `52e1144b5c186ccacd85d9c779a4e60c3cce5ac2`
- Range: `cf5ee4128ce0b842e00a0eb20770ab5c285dedd8..52e1144b5c186ccacd85d9c779a4e60c3cce5ac2`
- Approved authority: `changes/active/audit-outbox/spec.md`, revision 4
- Prior finding authority: `changes/active/audit-outbox/review/r2/verdict.md`
  and `findings-validation.md`
- Remediation authority:
  `TASK-AUDIT-OUTBOX-R2-bounded-remediation.md` and
  `TASK-AUDIT-OUTBOX-R3-commit-outcome.md`
- `HEAD` was the candidate when this report was completed.
- The repository has no `.codegraph/` directory, so source navigation used
  repository search and direct caller/source inspection.

## Reviewed boundary

This review traced the security- and tenancy-sensitive path end to end:

1. a typed `DataTenantId` enters the generic outbox alongside one
   `AuditEvent`;
2. the writer groups items by that tenant and permits at most one write in
   flight for that tenant;
3. `AuditSink::write` opens a `TenantConn` bound to the same tenant, obtains the
   transaction's own `xid8`, and calls the one canonical staging append;
4. the append relies on RLS for `vala.audit_chain_head` and
   `vala.audit_staging`, derives the stored tenant from the bound connection,
   and preserves the event's principal, credential, permission, resource, and
   decision fields;
5. only after an ambiguous `COMMIT` does the sink query `pg_xact_status` on a
   fresh runtime-pool connection, using the internally produced transaction ID
   rather than tenant- or caller-controlled input;
6. committed outcomes settle without resend, aborted outcomes return to the
   existing same-tenant front-of-queue retry, unresolved outcomes wait without
   resend, and a discarded status counts and logs the batch as lost without
   resend; and
7. the publisher continues to read tenant-bound contiguous staging ranges,
   rejects foreign-tenant rows at projection, and retains them through the
   existing tenant-qualified `vala.system.audit_log` chain.

The transaction-status query does not read tenant data or confer cross-tenant
authority. It observes only the global commit state of the exact transaction ID
created inside the preceding tenant-bound append. Its parameter is bound, not
interpolated. The result cannot change the event tenant, principal, permission,
resource, outcome, or retained projection.

## Authority and source coverage

| Boundary | Authority and source evidence | Result |
|---|---|---|
| Tenant selection and SQL authority | `architecture/wyrd-security-posture.md:339-354`; `architecture/references/doctrine/architecture-constraints.md:69-83`; `crates/shared/wyrd-runtime/src/outbox.rs:148-161,304-352`; `crates/vala/vala-sql/src/audit_outbox.rs:128-158` | PASS — typed grouping is preserved through a tenant-bound `TenantConn`; no caller-controlled tenant or administrative pool is introduced. |
| Canonical append and attribution | `crates/vala/vala-sql/src/queries/audit_staging.rs:61-174` | PASS — the connection supplies `data_tenant_id`; principal, credential, permission, resource, outcome, request, and trace fields remain attached to the same event. RLS remains the tenant boundary. |
| Ambiguous commit outcome | `spec.md` REQ-009/AC-009; `crates/vala/vala-sql/src/audit_outbox.rs:65-124,133-157` | PASS — committed is success, aborted is the only resend path, unresolved waits, and `NULL` records accepted loss without resend. No event ID or reader deduplication remains. |
| PostgreSQL semantics | PostgreSQL system-information-function documentation for `pg_current_xact_id()` and `pg_xact_status(xid8)` | PASS — the documented outcomes are `in progress`, `committed`, `aborted`, and `NULL` when status was discarded; PostgreSQL documents this function for resolving a disconnect during `COMMIT`. The sink does not use prepared transactions, for which `in progress` would have a distinct meaning. |
| Retained tenant chain | `crates/vala/vala-sql/src/queries/audit_staging.rs:269-483`; `crates/vala/vala-bifrost-redux/src/tables/audit/projection.rs:76-160`; `architecture/bifrost-design.md:613-649` | PASS — publication remains tenant-bound and contiguous, projection rejects a foreign tenant, and Scribe's existing frozen-range identity remains the only publication deduplication mechanism. |
| Panic and shutdown loss boundaries | `crates/shared/wyrd-runtime/src/outbox.rs:189-226,304-345,368-423`; focused tests at `outbox.rs:732-823` | PASS — a contained sink panic returns its owned batch for ordered retry; shutdown fences later admission and accounts abandoned work exactly once. |
| Unwrap-audit security control | `scripts/check_unwrap_audit.py:31-55,218-265`; `scripts/test_check_unwrap_audit.py:14-44` | PASS — the basename-wide production bypass is gone; only four explicit cfg-test module paths are excluded, while a production `tests.rs` is rejected. |
| Security operations and incident ownership | `architecture/wyrd-security-posture.md:356-396,420-433`; `architecture/operations/runbooks.md:116-165` | PASS — live authority now attributes failures to the shared audit outbox, documents transaction-status resolution and accepted loss, and no longer assigns commit ownership to Oracle. |
| Dependency and migration surface | `crates/vala/vala-sql/Cargo.toml`; deletion of `20261003000001_audit_staging_event_id.sql` | PASS — `tokio` is an existing workspace dependency used only for bounded retry timing. The deleted event-ID migration was unreleased and its schema is absent from the candidate contract, so retaining it would create the obsolete revision-2/3 write requirement. |

## Prior-finding closure

| Finding | Security/tenancy judgment |
|---|---|
| `FIND-AUDIT-OUTBOX-11` | **Closed under revision 4.** The commit transaction ID is obtained inside the tenant-bound transaction before append. A failed acknowledgement is resolved before any retry, so publisher retirement cannot erase the only duplicate fence: the writer does not resend a committed decision. The production writer/publisher journey exercises three acknowledged commits with lost replies and one commit lost before Postgres receives it, and asserts one retained row per decision plus a gap-free retained prefix. |
| `FIND-AUDIT-OUTBOX-12` | **Closed.** Sink panics are contained while the task still owns the items and follow the same front-of-tenant retry path. This removes the prior live-process audit-loss path. |
| `FIND-AUDIT-OUTBOX-13` | **Closed.** Shutdown takes and drops the sender before awaiting, so post-fence decisions cannot enter the queue; pre-fence items drain, and deadline remainder is counted and released from pending state. |
| `FIND-AUDIT-OUTBOX-14` | **Closed.** A production file can no longer evade the unwrap/expect control solely by being named `tests.rs`. |
| `FIND-AUDIT-OUTBOX-7` | **Closed with no security regression.** The touched declarations use top-level imports and bare `MutexGuard`/`Uuid` names; no boundary or type semantics changed. |
| `FIND-AUDIT-OUTBOX-3` | **Closed.** The security posture and runbook name the shared audit outbox as the commit owner and give operators the correct failure, pending, loss, and publication signals. |

## User-directed risk judgments

### In-progress/unreachable wait and `NULL` loss have no direct test

This is a verification limit, not a material finding in this domain.

- The implemented branches directly match revision 4 and PostgreSQL's closed
  result set: resend occurs only for `aborted`; `in progress`, an unavailable
  status connection, or any temporarily unresolved response loops without
  releasing the batch to retry; `NULL` increments
  `outbox_events_lost_total{outbox="audit"}`, logs tenant/xact/error/count, and
  settles without resend.
- The ambiguous-commit journey directly proves the security-critical opposites:
  a landed commit whose acknowledgement is lost is not resent, and a commit
  Postgres never receives is retried once. It also retires each landed row
  before the next ambiguity, closing the prior retirement race.
- A dedicated `NULL` integration test is difficult because it requires safely
  advancing PostgreSQL beyond retained transaction status. The lack of that
  harness does not create a caller-controlled or cross-tenant path, but the
  branch should remain an explicit verification limit for final integration.

### A waiting batch holds one of four writer slots

Accepted for this closure. One unresolved tenant can occupy only its own one
in-flight slot, leaving three audit writer slots for other tenants. An external
tenant cannot choose a transaction ID, trigger prepared-transaction state, or
control the database connection fault needed to enter this path. Four
simultaneous unresolved tenants could temporarily consume all four slots, but
that requires an infrastructure-level ambiguous-commit event across four
tenants; while Postgres is unreachable no tenant could commit through another
implementation either, and after reachability returns each status query can
settle without resend. This is a bounded availability tradeoff of the approved
"wait and ask again" behavior, not an RBAC bypass, cross-tenant data path, or
audit misattribution defect.

### Deleted unreleased migration

Accepted. Revision 4 removes the event-ID contract completely; deleting the
unreleased migration keeps a fresh deployment aligned with the candidate code
and avoids leaving a `NOT NULL event_id` column that the revision-4 append no
longer supplies. This judgment depends on the user-supplied fact that the
migration was never released. Any persistent environment that applied the
intermediate migration would require an explicit cleanup migration before this
candidate could serve writes; that rollout condition is not present for the
reviewed unreleased change.

## Verification

- PASS: `mise exec -- cargo nextest run --locked -p wyrd-runtime --lib -E
  'test(=outbox::tests::a_panicking_write_is_retried_once_in_order_without_loss)
  or test(=outbox::tests::shutdown_refuses_items_staged_after_it_begins) or
  test(=outbox::tests::shutdown_counts_items_unwritten_at_the_deadline_as_lost)'`
  — 3/3 passed.
- PASS: `mise exec -- python3 scripts/test_check_unwrap_audit.py`.
- Static PASS: the exact audit-specific search found no remaining event-ID
  machinery, reader collapse, at-least-once audit wording, or Oracle-owned
  audit-commit wording in the live source and authority paths.
- The focused Postgres journey was attempted independently, but the environment
  denied access to the configured Docker socket before Postgres could start.
  Therefore this review relies on the task's recorded passing journey,
  integration, family, formatting, lint, documentation, and boundary evidence
  for database-backed execution. `FIND-5`, `mise run bench:capacity`, and
  `mise run gate` remain explicitly deferred to integration by user direction.

## Security Audit

### Critical

- None.

### High

- None.

### Medium

- None.

### Low / Defense In Depth

- None. The untested status-wait and `NULL` branches are recorded as proof
  limits above, not speculative vulnerabilities.

### Positive Controls

- Tenant audit writes use typed tenant identity, `TenantConn`, transaction-local
  RLS, and a publisher-side foreign-tenant rejection.
- The transaction-status lookup uses a bound, internally derived `xid8`; no SQL
  interpolation or external identifier reaches it.
- Retry is fail-safe for audit integrity: no resend until an abort is known,
  and an unresolvable outcome is surfaced as counted loss instead of risking a
  duplicate or misattributed decision.
- Logs expose tenant, transaction ID, database error, retry delay, and count,
  but not audit payloads, credentials, tokens, prompts, or request bodies.
- The obsolete event-ID/deduplication path is removed from staging, retained
  projection, readers, docs, and migration state, leaving one audit writer and
  one publisher.
- The production unwrap audit no longer has a basename-wide bypass.
