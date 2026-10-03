# Audit outbox r3 repository-standards review

## Immutable subject and scope

- Repository: `/home/thorrester/Documents/GitHub/wyrd/.claude/worktrees/agent-aad682fbca5074900`
- Base: `cf5ee4128ce0b842e00a0eb20770ab5c285dedd8`
- Candidate: `52e1144b5c186ccacd85d9c779a4e60c3cce5ac2`
- Approved authority: `changes/active/audit-outbox/spec.md`, revision 4
- Closure scope: `FIND-AUDIT-OUTBOX-11` under revision 4, `-12`, `-13`,
  `-14`, `-7`, and `-3`, plus regressions introduced by the immutable range
- Explicit deferrals: `FIND-AUDIT-OUTBOX-5`, `mise run bench:capacity`, and
  `mise run gate`

`HEAD` matched the candidate before and after inspection. The repository has
no `.codegraph/` directory. This report does not assess general task
acceptance, reopen earlier passed code, or propose optional improvements.

## Material findings

### STD-R3-001 — the new journey proxy repeats the mandatory import/type-ownership violation

- **Classification:** `REGRESSION / VIOLATION`
- **Governing rule:** `architecture/agent-rules.md:9-10` requires types in
  fields and signatures to be imported at module scope and used by bare name,
  and permits no ordinary function-scoped `use` statements.
- **Changed location:**
  `crates/wyrd/wyrd-testing/tests/bifrost/server/audit_publication.rs:839-841,915-921`.
- **Evidence:** the newly added `CommitCutter::addr` field uses
  `std::net::SocketAddr`; `relay` names `tokio::net::TcpStream` in two
  parameters and `std::io::Result` in its return type; and `relay` imports
  `AsyncReadExt`/`AsyncWriteExt` inside the function. None is one of the two
  allowed exceptions. This repeats the same mandatory dependency-manifest
  rule whose earlier violations were tracked as `FIND-AUDIT-OUTBOX-7`.
- **Consequence:** the r3 closure introduces a fresh instance of the exact
  repository-rule violation the cumulative candidate claims to close, hiding
  dependencies from the module-level import manifest.
- **Testable correction:** import `SocketAddr`, the IO result type,
  `TcpStream`, `AsyncReadExt`, and `AsyncWriteExt` at the module top and use
  bare type names in the field and function signature. Prove with `mise run
  fmt`, `mise run lints`, and a focused diff/search showing no function-scoped
  imports or fully qualified declaration types remain in the added proxy.

### STD-R3-002 — two required commit-outcome branches have no deterministic proof

- **Classification:** `VIOLATION`
- **Governing rules:** `AGENTS.md:398-405` assigns edge and negative branches
  to supporting integration/unit tests; `AGENTS.md:718-733` requires changed
  code to be directly testable; and
  `architecture/references/languages/rust-core.md:773-783` requires new core
  logic to cover success, edge cases, and stable failures. Revision 4
  `REQ-009` makes the wait/no-resend and unknown/loss outcomes part of the
  exact-once audit contract.
- **Changed location:** `crates/vala/vala-sql/src/audit_outbox.rs:90-123`;
  current journey proof at
  `crates/wyrd/wyrd-testing/tests/bifrost/server/audit_publication.rs:996-1055`.
- **Evidence:** the production resolver handles `in progress` or a failed
  status query by waiting and re-querying without re-sending
  (`audit_outbox.rs:112-121`), and handles `NULL` by counting the batch lost
  and returning success so it is never re-sent (`audit_outbox.rs:100-110`).
  The implementer evidence explicitly records that neither branch has a
  direct test. The `CommitLost` journey may transiently observe `in progress`,
  but neither forces nor asserts that state, it never makes the status query
  unreachable, and it cannot reach `NULL`.
- **Consequence:** the two branches that prevent an uncertain commit from
  becoming a duplicate—or turn irrecoverable uncertainty into a counted audit
  gap—can regress while the reported AC-009 journey remains green. That can
  make retained audit results duplicated or silently lost, which is inside the
  user-directed regression boundary.
- **Testable correction:** add deterministic focused proof through the
  production audit sink that (1) an in-progress or temporarily unreachable
  status causes no re-send, continues resolution, and eventually follows the
  returned committed/aborted outcome, and (2) a `NULL` status increments
  `outbox_events_lost_total{outbox="audit"}` exactly by the batch size, sends
  nothing again, and settles pending ownership. Keep the existing real
  writer/publisher exactly-once journey; do not replace it with only a pure
  classifier test or sleep-based timing.

## Authority coverage

The reference router selected the complete applicable slices:
`doctrine/architecture-constraints.md`, `architecture/patterns.md`,
`languages/spec-driven-development.md`, `languages/maintainer-style.md`,
`languages/rust-core.md`, `languages/testing-workflows.md`,
`domain/vala-architecture.md`, `domain/olap-serving.md`, and
`domain/analytical-operations-reliability.md`. Governing authorities read were
`AGENTS.md`, `architecture/agent-rules.md`, `architecture/wyrd-design.md`,
`architecture/wyrd-doctrine.mdx`, `architecture/bifrost-design.md`,
`architecture/wyrd-security-posture.md`, and
`architecture/operations/runbooks.md`. The migration assessment also checked
the migration contract in
`architecture/v1/00-foundations/sql-foundation.md` and
`architecture/operations/deployment-and-release.md`.

| Changed surface | Applicable repository authority and rule | Result | Exact evidence |
|---|---|---:|---|
| Generic outbox owner, panic containment, shutdown fence, counters, and tests (`wyrd-runtime/src/outbox.rs`) | `AGENTS.md` §§5-6, 11, 16; `agent-rules.md` async, struct owner, rustdoc, testing, and audit rules; `rust-core.md`; `maintainer-style.md`; `testing-workflows.md` | **PASS** | `Outbox<S>` remains the meaningful state owner; `stage`, `settle`, and `shutdown` are inherent methods; `contain_panic` is a focused future helper. Lines 90-214 close admission under the handle, drain pre-fence work, settle pending/gauge state, and count deadline loss. Lines 296-360 return panicked items to the existing tenant-front retry path. Added focused tests cover sink panic and shutdown admission/loss, and changed declarations carry substantive rustdoc. |
| Audit sink transaction-ID capture and outcome resolution (`vala-sql/src/audit_outbox.rs`) | `AGENTS.md` §§2, 3, 6, 9, 11, 16; `agent-rules.md` one audit path, `TenantConn`, async IO, and docs; `architecture-constraints.md` Audit Boundaries; `patterns.md` Audit Pattern; `rust-core.md` Postgres/Audit; security posture Audit Integrity | **FAIL — STD-R3-002** | Lines 144-157 obtain `pg_current_xact_id()` in the tenant transaction and route an errored commit to lines 83-125. Committed returns success; aborted returns the original error for the existing outbox retry; unresolved waits without re-send; `NULL` counts loss and returns success. The owner and RLS boundary are correct, but the latter two required branches lack deterministic tests. |
| Canonical staged append and row arrays (`vala-sql/src/queries/audit_staging.rs`) | `agent-rules.md` `TenantConn`/RLS, caller-owned transaction, bare types, one append path, name-based fields; `AGENTS.md` §§4, 5, 15, 16; `patterns.md` Storage/Audit | **PASS** | `append_audit_events` remains crate-private and has only the production `AuditSink` caller plus feature-gated seeding wrappers. It accepts `&mut TenantConn`, relies on RLS rather than a manual tenant predicate for reads/updates, never commits, imports `Uuid` at module top, and builds named column arrays. Removal of event-ID skip logic restores the revision-4 schema without adding another writer or authority. |
| `vala-sql` manifest | `AGENTS.md` §§1, 2, 4; `agent-rules.md` Cargo-feature/dependency cost; `rust-core.md` async; ownership boundaries | **PASS** | Adding existing workspace `tokio` with only `time` is earned by the real IO retry wait in `AuditSink::resolve_commit`; no feature, SQL edge into a shared/client crate, cloud SDK, or new dependency family was added. The dependency remains in the narrow SQL owner rather than `wyrd-runtime`. |
| Removed event-ID migration | SQL migration contract; approved revision-4 spec; spec-driven authority order | **PASS (user-stipulated unreleased migration)** | `20261003000001_audit_staging_event_id.sql` was introduced only for the superseded revision-2/3 event-ID approach and revision 4 explicitly removes that column and migration. The user states it was unreleased; no shipped/applied production checksum therefore exists to preserve. Current source, staging row type, projection, and tests contain no audit `event_id` dependency. A forward migration would retain a prohibited schema field. This exception does not authorize deleting or editing any released migration. |
| Postgres integration tests (`pg_audit_outbox.rs`) | `AGENTS.md` §11; `agent-rules.md` external Postgres-test placement and exact `mise` execution; `testing-workflows.md` | **PASS for changed coverage; AC-009 gap is recorded on the production resolver** | The external `pg_*` test legitimately drives Postgres. It retains replica chain/order, tenant independence, and shutdown loss coverage; removal of the old event-ID retry test matches revision 4. The missing resolver branches are not excused by this surface and are captured as `STD-R3-002`. |
| Retained audit projection (`vala-bifrost-redux`) and server/harness readers | `AGENTS.md` §§3, 9-11, 16; `bifrost-design.md` Read Audit; `vala-architecture.md`; `olap-serving.md`; `patterns.md` Audit/Verification | **PASS** | The projection returns to 15 canonical columns and readers no longer collapse by an event ID. `AuditPublisher` remains the sole publisher and the harness reads raw retained decisions. Added rustdoc on materially changed projection test items satisfies the hard documentation rule. |
| Commit-outcome production journey and proxy (`wyrd-testing/.../audit_publication.rs`) | `AGENTS.md` §§11, 16; `agent-rules.md` import placement, Postgres/live-server test placement, journey proof; `testing-workflows.md`; `maintainer-style.md` | **FAIL — STD-R3-001, STD-R3-002** | The ignored server journey is correctly housed in the real Bifrost journey binary and uses the production `AuditSink`, server publisher, and retained query path. Three acknowledgement-loss rounds retire between commits, and the commit-loss round proves retry after abort. New types/functions/tests are documented. The proxy declarations/imports violate the module import rule, and the proof does not deterministically cover status-unreachable/in-progress or `NULL`. |
| One-of-four writer-slot occupancy while commit status is unresolved | `REQ-002`, `REQ-008`, `REQ-009`; `analytical-operations-reliability.md` bounded ownership; `audit_outbox.rs` docs | **PASS — accepted bounded behavior, not a regression** | `AuditSink::resolve_commit` deliberately remains the one in-flight tenant write and holds one logical outbox slot while it polls; it does not hold a database connection during backoff. `AUDIT_WRITER_CONNECTIONS` stays fixed at four, leaving three slots for other tenants, and line 77 documents the ownership. Revision 4 requires waiting without re-send; no authority requires uncertain batches to leave the writer concurrency accounting. Four simultaneously unresolved tenants can consume all four slots, but that is bounded dependency unavailability under the approved semantics, not an unbounded queue or a new cross-tenant correctness defect. |
| Unwrap/expect checker and Python fixtures | `AGENTS.md` §§11-12, 16; `agent-rules.md` no gate circumvention; Python top-level-test rule; `testing-workflows.md` boundary checks | **PASS** | The blanket `tests.rs` exclusion is removed. `CFG_TEST_MODULES` lists four proven cfg-test modules with declaring files, while `is_ignored_path` scans any other production `tests.rs`. `scripts/test_check_unwrap_audit.py` uses top-level `test_*` functions and identical allowlisted/production bodies. `mise exec -- python3 scripts/test_check_unwrap_audit.py` passed; `UV_CACHE_DIR=/tmp/wyrd-review-uv-cache mise run check:unwrap-audit` passed. |
| Architecture, security posture, runbook, server module docs, and public Bifrost docs | `wyrd-design.md`; `bifrost-design.md`; `wyrd-security-posture.md`; operations runbook; `architecture-constraints.md`; `patterns.md`; `wyrd-doctrine.mdx` | **PASS** | Live authority consistently removes event-ID/dedup reader claims and states `pg_xact_status` resolution: retry only aborted, wait on unresolved, count `NULL` as loss, one retained row per decision. Security operations now names audit outbox write/publication failure rather than “Oracle audit commit failure,” closing FIND-3. No second audit table, publisher, WAL, relay, or public contract was added. |
| Approved spec and r2 remediation/evidence packets | `spec-driven-development.md`; `AGENTS.md` §14 | **PASS** | Revision 4 is approved, records the superseded revision-2/3 event-ID decisions, and provides stable REQ/AC mapping. The two r2 remediation tasks preserve evidence and explicitly identify the unresolved test limits. Review prose stays inside the active change packet rather than production code. |

## Prior-finding closure under repository rules

| Prior finding | Standards result |
|---|---|
| `FIND-AUDIT-OUTBOX-11` | Implementation shape and live authorities align with revision 4, but closure proof is incomplete for two required resolver branches (`STD-R3-002`). |
| `FIND-AUDIT-OUTBOX-12` | Closed: a sink panic is contained while the task owns its items and goes through the existing retry path with direct proof. |
| `FIND-AUDIT-OUTBOX-13` | Closed: shutdown owns a one-way admission fence, drains pre-fence work, refuses/counts late stages, and clears pending/gauge state after deadline loss. |
| `FIND-AUDIT-OUTBOX-14` | Closed: no basename-wide exclusion remains and checker fixtures prove the distinction. |
| `FIND-AUDIT-OUTBOX-7` | Prior cited declarations are fixed, but the r3 journey introduces a fresh violation of the same rule (`STD-R3-001`), so the cumulative repository is not standards-clean. |
| `FIND-AUDIT-OUTBOX-3` | Closed: live security authority uses shared audit-outbox write/publication terminology. |

## Verification notes

- Reviewed the complete committed diff and every materially changed source,
  test, manifest, migration, authority, runbook, public-doc, checker, and
  active-packet surface in the range.
- Independently passed: `git diff --check
  cf5ee4128ce0b842e00a0eb20770ab5c285dedd8..52e1144b5c186ccacd85d9c779a4e60c3cce5ac2`;
  `mise exec -- python3 scripts/test_check_unwrap_audit.py`; and
  `UV_CACHE_DIR=/tmp/wyrd-review-uv-cache mise run check:unwrap-audit`.
- Reviewed implementer-recorded green evidence for the exact AC-009 journey,
  Bifrost SQL integration, Bifrost server journeys, `test:wyrd`, formatting,
  lints, docs, and unwrap audit. This standards pass did not rerun Cargo lanes.
- The first `mise run check:unwrap-audit` attempt could not create a temporary
  cache file in the sandbox's read-only user cache; rerunning the same task
  with its cache under `/tmp` passed. This is an execution-environment limit,
  not a source failure.
- Per user direction, `mise run bench:capacity` and `mise run gate` remain
  deferred to integration and are not findings here.

## Overall result

**FAIL** — `STD-R3-001` and `STD-R3-002` are material repository-rule
violations in the remediation range. The deleted unreleased migration and the
single unresolved tenant's use of one of four writer slots are accepted under
the approved revision-4 boundary and do not add findings.
