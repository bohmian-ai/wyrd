---
id: TASK-001
kind: implementation
status: review
spec: SPEC-surfaces-oracle-integration
spec_revision: 7
requirements: [REQ-001, REQ-002, REQ-003, REQ-010, REQ-011, REQ-012, REQ-013, REQ-014, REQ-015, REQ-026, REQ-026A, REQ-026B, REQ-027, REQ-027A, REQ-027B, REQ-027C, REQ-028, REQ-029, REQ-030, REQ-030A, REQ-048, REQ-049, REQ-050, REQ-051, REQ-052, REQ-053, REQ-053A, REQ-054, REQ-062, REQ-063, INV-002, INV-003, INV-007, INV-008, INV-008A, INV-008B, INV-008C, INV-008D, INV-009, INV-017, INV-018, INV-019, INV-020, INV-021, INV-023, AC-005, AC-006, AC-011, AC-012, AC-013, AC-014, AC-015, AC-016, AC-017, AC-020]
depends_on: []
parent_task:
remediates: []
---

## Objective

Integrate the immutable Oracle checkpoint as the authoritative Bifrost data
plane while preserving the destination's non-Bifrost behavior. The completed
outcome has one Redux engine owning Gate, Scribe, Oracle, incorporated Forge,
canonical telemetry, query execution, maintenance, recovery, and retained
audit publication; legacy `vala-bifrost` is gone in full.

TASK-001 implementation is complete; only its closeout is paused. Do not
restart or reimplement this task. Implement child TASK-005 followed by TASK-006
against the existing integrated result, and only then resume TASK-001 closeout.

## Constraints

- Use the completed revision-4 conflict ledger and only the pinned Oracle
  input. Forge is already incorporated and must not be merged again.
- Preserve Surfaces authority outside Bifrost. A newly discovered material
  conflict returns `SPEC_REVISION_REQUIRED`; a clean Git merge is not proof.
- Keep all listeners, authentication, authorization, readiness, and durable
  orchestration server-owned. Preserve tenant isolation and the exact audit,
  reader-protection, WAL, resource, cancellation, and fail-closed boundaries.
- Complete audit publication directly against Redux. Do not port legacy
  sealing, derivation, typed-read, or direct-Iceberg relay machinery.
- TASK-005 owns the revised authorization-boundary audit flow. It MUST be
  implemented on this task's existing integration branch, then TASK-006 MUST
  make its retained publication race-safe and Scribe-owned before TASK-001 may
  enter review or closeout.
- Do not add compatibility crates, routes, aliases, a second scheduler,
  cluster-wide Oracle quotas, or alternate durable formats.
- Live UI integration, SDK package convergence, repository-wide CI closeout,
  and the single data-root follow-up are owned by later tasks.
- There is no such thing as a pre-existing failure anymore. All failures must be explicitly handled within the current execution context.

## Relevant Surface

- `crates/vala/vala-bifrost-redux` and deletion of
  `crates/vala/vala-bifrost`
- Bifrost-owned contracts in `crates/wyrd-spec`
- `crates/vala/vala-sql`, `crates/wyrd/wyrd-sql`, and their greenfield
  migration sources where data-plane authority requires them
- Bifrost integration in `crates/wyrd/wyrd-server`
- Bifrost capability journeys in `crates/wyrd/wyrd-testing`
- Bifrost, security, reliability, and operations architecture authorities

Paths are ownership guidance, not a private implementation allowlist.

## Approach

1. Reconcile the pinned Oracle Redux tree and its server/SQL contracts against
   the ledger, retaining Surfaces behavior outside Bifrost.
2. Remove the legacy engine and redirect every live server, audit, test,
   manifest, feature, generated, and documentation consumer to Redux or its
   approved server owner.
3. Preserve Scribe WAL v6, admission, staging, publication, replay, shutdown,
   and retirement behavior as one bounded durability lifecycle.
4. Preserve Oracle's one-build planning, pod-local admission, scoped
   authorization, reader epochs/protection, one-attempt execution, terminal
   streaming, cancellation, and readiness behavior.
5. Preserve incorporated Forge scheduling, independent maintenance,
   publication, conflict/ambiguous recovery, protection serialization, and
   fail-closed readiness behavior.
6. Converge OTLP and canonical Arrow writes on the three canonical signal
   tables with trusted attribution and SQL-only reads.
7. Keep closeout paused while TASK-005 implements the authorization-only audit
   flow and TASK-006 implements frozen-range coordination and direct-Scribe
   publication; then complete the remaining integration evidence.

## Acceptance Criteria

- The tree and dependency graph contain exactly one Bifrost engine, Redux;
  no legacy package, symbol, feature, route, schema, migration owner, test,
  benchmark, documentation alias, or compatibility facade remains.
- Acknowledged Scribe writes survive replay and progress through staging and
  publication without weakening fences, idempotency, bounded ownership, or
  tenant-qualified physical identity.
- Interactive and distributed queries use one physical build, local fair
  admission, complete object authorization, durable reader protection before
  source IO, one deadline, and one selected execution attempt. Failure never
  becomes partial success or a successor attempt.
- Reader protection and Forge expiration serialize per tenant-qualified table;
  lease loss, uncertain authority, and unresolved maintenance remove readiness
  and fail closed.
- Forge retains independently committed sibling progress, bounded conflict
  retry, ambiguous-outcome reconciliation, worker-local FIFO estimated-memory
  admission, and separate cleanup protocols.
- Stock OTLP and canonical Arrow writes produce equivalent rows in only
  `vala.traces.spans`, `vala.logs.records`, and `vala.metrics.points`, with
  trusted principal/correlation attribution and exact partial-success rules.
- Allowed and denied permission decisions reach the canonical tenant staging
  chain before the operation proceeds or refuses; Oracle reads retain their
  WAL-first exception. Publication into `vala.system.audit_log` is bounded and
  idempotent, and watermark advancement plus garbage collection occur only
  after durable publication.
- Scoped-role, cross-tenant, audit-unavailable, replay, backpressure,
  cancellation, peer-failure, restart, and cleanup journeys fail or recover
  exactly as revision 7 requires.

## Verification

- `mise run fmt`
- `mise run lints`
- `mise run test:sql`
- `mise run check:tenant-isolation`
- `mise run check:object-store-pin`
- `mise run check:unwrap-audit`
- `git diff --check`

Run and record exact focused `mise exec -- cargo nextest run` commands for the
Redux, Scribe, Oracle, Forge, OTLP, server, and audit scenarios changed during
implementation. Record the no-legacy inventory and requirement-to-evidence
closure. Do not run a Bifrost aggregate in this task.

## Status Amendment — in progress

### Implemented

**Redux is the sole engine.** `vala-bifrost` is gone in full: zero
`vala_bifrost` symbols, `crates/vala/` holds only `vala-bifrost-redux`, and the
remaining string matches live in `.dev/` history, `changes/` planning docs, and
`scripts/checks/client-tier.sh` (kept — the token still matches Redux by prefix
and guards a live dependency-direction boundary). Empty Oracle and server test
stubs byte-identical to the pinned input were deleted with their `mod` lines.

**Merge defects resolved against the current tree, not tolerated.**

- `expired_uploads_batch` reaped only `pending` rows on `expires_at`, so an
  orphaned `initiating` row was never swept. Adopted the pinned input's
  `init_grace` predicate plus its `WYRD_STORAGE_SWEEPER_INIT_GRACE_SECS` knob
  (default 30s, clamped 5–600).
- `check:tenant-isolation` kept the Surfaces rule body while both Oracle
  allowlists still passed a now-inert exemption flag (19 failures). Restored the
  guard with Oracle's explicit tenant-predicate rule in the exempt branch.
- `check:unwrap-audit` flagged 10 call sites of a harness helper named `expect`
  that returns `Result`. Renamed it `require`.
- Redux tier-2 `forge::orphan_cleanup` flake: the count helper included
  `forge.task.*` worker bookkeeping. It now returns the ordered operation list
  and excludes that prefix, so a failure names the transition.

**Audit publication was non-functional end to end; three production defects.**
The capability had no journey covering it, so the path was dead and green. A new
journey (`wyrd-testing/tests/bifrost/server/audit_publication.rs`) exercises it
and found:

1. `publish_audit_projection` could never append — the `audit_log` content
   column `principal_id` is a reserved Redux correlation name, so ingest refused
   every projection. Renamed to `audit_principal_id`, matching the sibling
   `audit_card_ref` precedent.
2. `AuditPublisher::sweep` read the tenant directory on the RLS application
   pool, which holds no `SELECT` on `platform.tenants`. Routed through the
   cross-tenant operator pool.
3. `list_active_tenant_ids` validated every directory row as a UUIDv7, and the
   directory always carries the nil-UUID system tenant, so the read failed for
   every tenant. The sentinel now maps to `DataTenantId::SYSTEM_OWNER`.

Either of (2) or (3) alone disabled publication for every tenant, silently: the
sweep logs `warn!` and returns.

**Consequences of publication actually running.** With audit rows reaching
Scribe for the first time, Forge refused to promote them: `audit_log` is
registered without the universal correlation columns that ingest stamps on every
batch, so the physical object could never agree with its table. Interim fix
(under review, see Remaining) set the table to `CorrelationPolicy::Observation`
and retired the unwritable `None` variant. Forge journeys that read
`vala.audit_outbox` as if it were history now read retained history as well; the
anti-enumeration leak check is scoped to the problem members that carry request
data; the process-wide Scribe row counter is held to a floor now that the same
Scribe also accepts published audit history.

### Verification status

| Lane | Result |
|---|---|
| `test:bifrost:journey:server` | 7/7 pass |
| `test:bifrost:journey:oracle` | 28/28 pass |
| `test:bifrost:journey:otlp` | 10/10 pass |
| `test:bifrost:journey:forge` | 13/13 pass, intermittent (see Remaining) |
| `test:sql` | pass |
| `check:tenant-isolation`, `check:unwrap-audit` | pass |
| `vala-bifrost-redux` lib | 977/977 pass |

### Remaining

1. **Correlation-policy direction reversed by review.** Rather than giving
   `audit_log` the correlation envelope, ingest will honour
   `CorrelationPolicy::None`: `DecodeContext` already carries
   `definition.correlation_policy`, so the unconditional append becomes
   conditional at the one site. Dynamic tables (`definition: None`) keep today's
   envelope. This restores the `None` variant and the table's policy, and leaves
   `audit_log` with exactly one principal, one request id, and one trace id.
2. **`audit_log` physical layout** does not follow the house pattern: `seq` is
   bloomed though it is monotonic and already the secondary sort key, while the
   two real audit access paths are uncovered. Target:
   `bloom = ["audit_principal_id", "resource", "operation"]`, sort unchanged.
3. **Forge lane stability.** Two `production_closeout` scenarios intermittently
   fail under the lane's parallelism since publication began running — once on
   an empty orphan set, twice on a public query hitting the handler timeout. A
   pending edit makes the retained-history fallback fire only when the staging
   answer is empty, keeping the fused query off the hot path.
4. `mise run codegen:check` and regeneration (the `audit_log` schema change
   moves generated contracts and its fingerprint).
5. `mise run skills:sync` / `check:skills-sync`, `check:object-store-pin`,
   final `fmt`, `lints`, `git diff --check`.
6. Acceptance-evidence table, no-legacy inventory, and requirement-to-evidence
   closure.

### Notes for the change owner

- Card MCP tools have no server-side equivalent: HEAD's dead
  `register_card_tools` was deleted along with the old `wyrd-mcp` surface.
- `CardPyResult` → `WyrdPyResult` (REQ-024) stays deferred to TASK-002.

### Closeout order — TASK-005, then TASK-006

No further TASK-001 closeout work proceeds until TASK-005
(`TASK-005-audit-at-the-authorization-boundary.md`) is implemented and TASK-006
(`TASK-006-coordinate-audit-publication.md`) completes its publication
coordination and direct-Scribe ownership. TASK-005
supersedes the audit portions of the status amendment above: the interim
`CorrelationPolicy::Observation` change and the `audit_principal_id` rename are
withdrawn, and the remaining audit work in this task is defined by TASK-005
rather than by items 1 and 2 of Remaining. TASK-006 then closes the changing-
tail replay and Gate-bypass gaps discovered in that publication flow.

Revision 7 and the required architecture direction are approved. Resume from
the existing in-progress work; do not restart TASK-001 or repeat completed
integration steps. Implement and verify TASK-005, then TASK-006, then continue
items 3–6 under Remaining and finish TASK-001 closeout.

## Closeout Evidence

TASK-005 is implemented and carries its own acceptance matrix in
`TASK-005-audit-at-the-authorization-boundary.md`. The four Remaining items
this task owed after it are resolved below.

### Remaining items 1–6 disposition

| Item | Disposition |
|---|---|
| 1. Correlation-policy direction | Superseded by TASK-005, which restored `CorrelationPolicy::None` for `vala.system.audit_log`. Ingest now resolves every built-in definition (`scribe/ingress.rs::builtin_definition`) and stamps the correlation envelope only where the declared policy asks for it. |
| 2. `audit_log` physical layout | `bloom = ["audit_principal_id", "resource", "operation"]`, sort unchanged. The `audit_principal_id` rename was withdrawn by TASK-005's closeout order; the content column is `principal_id` under `CorrelationPolicy::None`. |
| 3. Forge lane stability | Root-caused, not mitigated. The proposed retained-history fallback edit was unnecessary. Two distinct defects: (a) `audit_log` decoded with `definition: None`, so its `CorrelationPolicy::None` and `PAST_EVENT_TIME_EXEMPT` declarations were both inert and Scribe sealed 22 columns against a registered 18 — fixed in `ebfd269ef`; (b) `coordinator_object_store_failure_clears_readiness` never settled the coordinator's boot planning pass — fixed in `d5c0f1643`. |
| 4. `codegen:check` and regeneration | `mise run codegen:check` clean; `openapi.yaml` regenerated and `openapi.yaml.tmp` removed from tracking and ignored. |
| 5. Sync and boundary checks | All clean — see Commands below. |
| 6. Evidence, inventory, closure | This section. |

### Acceptance-criteria evidence

| Acceptance criterion | Implementation evidence | Verification evidence | Result |
|---|---|---|---|
| Exactly one Bifrost engine, Redux; no legacy package, symbol, feature, route, schema owner, test, benchmark, doc alias, or compatibility facade | `crates/vala/` holds only `vala-bifrost-redux`; one workspace member in `Cargo.toml`; `vala-bifrost` deleted in full | No-legacy inventory below; `mise run check:client-tier` clean | PASS |
| Acknowledged Scribe writes survive replay and progress through staging and publication without weakening fences, idempotency, bounded ownership, or tenant-qualified physical identity | Scribe WAL v6, durable batch-id dedup fence, admission, staging, publication, shutdown and retirement retained from the pinned input | `mise run test:bifrost:integration:redux` 973/973; `mise run test:bifrost:journey:server` 7/7 (`replayed_audit_publication_retains_each_event_once`) | PASS |
| Queries use one physical build, local fair admission, complete object authorization, durable reader protection before source IO, one deadline, one selected attempt; failure never becomes partial success | Oracle one-build planning, pod-local admission, reader epochs/protection, one-attempt execution retained | `mise run test:bifrost:journey:oracle` 28/28; `mise run test:bifrost:integration:server` 67/67 — `oracle_authority_is_installed_before_source_io` now gates on `vala.oracle_table_protections`, proving protection commits before resolver entry | PASS |
| Reader protection and Forge expiration serialize per tenant-qualified table; lease loss, uncertain authority, and unresolved maintenance remove readiness and fail closed | Oracle reader authority and Forge expiry gates | `oracle_epoch_cutoff_removes_readiness_and_retirement_joins_loss_owner` (three loss-owner races); `blocked_renewal_cannot_suppress_cutoff_or_bounded_settlement`; `worker_recovery_failure_never_publishes_ready`, `worker_registration_failure_never_publishes_ready`, `prepared_evidence_validation_failure_never_publishes_ready` | PASS |
| Forge retains independently committed sibling progress, bounded conflict retry, ambiguous-outcome reconciliation, worker-local FIFO estimated-memory admission, and separate cleanup protocols | `forge/managed/`, `forge/publication.rs`, `forge/scribe_promotion.rs`, `forge/orphan_gc.rs` — the incorporated Forge, newer than the pinned input | `mise run test:bifrost:journey:forge` 13/13; redux `forge::*` integration targets within 973/973 | PASS |
| Stock OTLP and canonical Arrow writes produce equivalent rows in only the three canonical signal tables, with trusted attribution and exact partial-success rules | OTLP ingress and canonical Arrow path converge on `vala.traces.spans`, `vala.logs.records`, `vala.metrics.points` | `mise run test:bifrost:journey:otlp` 10/10; redux `gate::tests::mixed_otlp_projection_assigns_only_accepted_contiguous_ordinals` and `all_invalid_otlp_returns_existing_outcome_without_scribe` | PASS |
| Allowed and denied decisions reach the canonical tenant staging chain before the operation proceeds or refuses; Oracle reads keep the WAL-first exception; publication is bounded and idempotent; watermark advance and GC follow durable publication | TASK-005 — Gate decision append, `drain_through_watermark`, Oracle audit WAL relay | TASK-005 acceptance matrix, all 8 criteria PASS; `mise run test:sql` 218/218 | PASS |
| Scoped-role, cross-tenant, audit-unavailable, replay, backpressure, cancellation, peer-failure, restart, and cleanup journeys fail or recover exactly as revision 6 requires | Journey suites across server, oracle, forge, otlp | 7/7, 28/28, 13/13, 10/10 respectively; `mise run check:tenant-isolation` clean | PASS |

### No-legacy inventory

| Probe | Result |
|---|---|
| `crates/vala/` members | `vala-bifrost-redux`, `vala-core`, `vala-drift`, `vala-eval`, `vala-ingest`, `vala-sdk`, `vala-sql` — no `vala-bifrost` |
| Workspace manifest | one Bifrost member, `vala-bifrost-redux` |
| `vala_bifrost` symbols outside Redux (`*.rs`, `*.toml`) | 0 |
| `vala-bifrost` strings, tracked, excluding Redux/`changes/`/`.dev/` | 3, all in `scripts/checks/client-tier.sh`, where the token matches Redux by prefix and guards a live dependency-direction boundary. Retained per AGENTS.md §12: the boundary it enforces is still violable. |
| Compatibility routes or aliases | none; the `legacy`/`compatibility` matches in Redux are rustdoc stating that no such path exists |
| Generated contracts | `mise run codegen:check` clean |

### Requirement-to-evidence closure

| Requirements | Evidence |
|---|---|
| REQ-001, REQ-002, REQ-003, INV-002, INV-003 | Merge defects resolved against the current tree rather than tolerated — sweeper `init_grace`, `check:tenant-isolation` Oracle exemption, `check:unwrap-audit` harness rename, orphan-cleanup count helper (Status Amendment above). Git's textual result was never accepted as proof. |
| REQ-010, REQ-011, REQ-012, REQ-013, INV-023 | One engine, one serving surface, tenant-qualified physical identity; no-legacy inventory and `check:client-tier`. |
| REQ-014, REQ-049, REQ-050, INV-017, INV-018, INV-020, AC-011, AC-012, AC-013 | Scribe WAL v6 and Oracle admission/epoch/protection behavior; `journey:oracle` 28/28, `integration:server` 67/67. |
| REQ-015, INV-009, INV-019 | One execution attempt, no partial success; `journey:oracle`, redux 973/973. |
| REQ-048, REQ-051, REQ-052, REQ-030A, INV-021, AC-014 | One-build planning; incorporated Forge scheduling, publication, reconciliation, worker-local FIFO admission; `vala.forge_operation_state` self-contained. `journey:forge` 13/13, `test:sql` 218/218. |
| REQ-053, REQ-053A, AC-015 | Three canonical signal tables with trusted attribution; `journey:otlp` 10/10. |
| REQ-054, AC-016, AC-017 | Typed scoped `Permission`; scoped-role journeys and TASK-005's retained dynamic permission. |
| REQ-026, REQ-026A, REQ-026B, REQ-027, REQ-027A, REQ-027B, REQ-028, REQ-029, REQ-030, INV-008, INV-008A, INV-008B, INV-008C, AC-005 | TASK-005 acceptance matrix in full. |
| REQ-062, REQ-063, AC-020 | Redux replaces `vala-bifrost` in full and owns Gate, Scribe, Oracle, Forge, telemetry, query, maintenance, recovery, and retained audit publication; no-legacy inventory. |
| INV-007, AC-006 | No operation derives tenant identity from an untrusted source; `mise run check:tenant-isolation` clean. |
| REQ-024 | Out of scope here — `CardPyResult` → `WyrdPyResult` remains deferred to TASK-002 (Notes for the change owner). |

### Commands

Re-run in full against the post-TASK-006 tree, since this task's closeout order
holds items 3-6 until TASK-006's publication coordination lands and that work
changed `vala-sql`, `vala-bifrost-redux`, and `wyrd-server`.

```
mise run fmt                               # clean
mise run lints                             # exit 0
mise run codegen:check                     # All checks passed!
mise run test:sql                          # 220/220
mise run test:bifrost:integration:redux    # 973/973
mise run test:bifrost:integration:server   # 67/67
mise run test:bifrost:journey:server       # 7/7
mise run test:bifrost:journey:scribe       # 21/21
mise run test:bifrost:journey:oracle       # 28/28
mise run test:bifrost:journey:forge        # 13/13
mise run test:bifrost:journey:otlp         # 10/10
mise run check:tenant-isolation            # clean
mise run check:object-store-pin            # clean
mise run check:unwrap-audit                # clean
mise run check:client-tier                 # clean
mise run check:pyo3-scope                  # clean
mise run skills:sync / check:skills-sync   # clean
git diff --check                           # clean
```

`test:sql` moved from 218 to 220: TASK-006 added the two frozen-range cases in
`vala-sql/tests/pg_audit_staging.rs`. `test:bifrost:journey:scribe` is new to
this block; TASK-006 owns that lane and it was not part of the earlier closeout.

`codegen:check` failed on this re-run and exposed real committed drift:
`4a3c5ee22` renamed the `AUDIT_UNAVAILABLE` doc comment from "audit outbox" to
"audit staging" without regenerating `bifrost_audit_event.json`. Regenerated and
committed in `358cf636e`; the check is clean.

No Bifrost aggregate was run in this task, per its Verification section, and
`mise run verify:bifrost` is withheld by explicit instruction until this
change's merge work completes.

### Defects found and fixed during closeout

| Commit | Defect |
|---|---|
| `ebfd269ef` | `scribe/ingress.rs` resolved a built-in definition only when it declared a canonical validator, so `audit_log`'s `CorrelationPolicy::None` and `PAST_EVENT_TIME_EXEMPT` were both inert. Scribe sealed 22 columns against a registered 18 and Forge's promotion invariant refused the object. |
| `634fbf8f2` | `crates/wyrd/wyrd-sql/src/queries/cards/audit.rs` still wrote the pre-revision audit columns and hash preimage. |
| `566f7782c` | `WyrdTestServerBuilder` accepted `oracle_audit_wal_root` but never composed it into `BifrostRuntimeConfig`, so every test server fell back to a process-id-named directory under the system temp dir that nothing removes. Once the OS recycled a pid, a later server recovered a dead process's WAL and refused to start with `QueryAuditUnavailable`. 14933 directories had accumulated locally. |
| `d5c0f1643` | The Oracle audit WAL development fallback is now one stable path rather than one per process id, so it neither accumulates nor adopts a foreign WAL. |
| `d5c0f1643` | `coordinator_object_store_failure_clears_readiness` never settled the coordinator's boot planning pass. `c3db2fc9b` made a coordinator plan on start and applied `await_boot_scheduler_pass` to two sibling router-smoke cases but missed this one, so its worker barrier could close on an attempt that had nothing to promote. |
| `6bb6f1b12` | Oracle epoch and reader-protection races were gated with a table lock on `vala.audit_staging`, which stopped blocking anything once those transitions no longer wrote audit. Re-gated on the durable rows they do write. |

### Material limits

- One `test:bifrost:integration:redux` run reported `972 passed (1 leaky), 1
  failed`. Its log was overwritten before the failing test was named, and four
  subsequent runs were 973/973. It is recorded here as an unnamed
  non-reproducing flake rather than presented as a clean lane.
- `ForgeSchedulerTrigger::owner_for_test` denotes a pinned lease owner for
  restart fixtures, and `c3db2fc9b` additionally reads it as "this fixture
  drives every scheduler pass". Those are different properties, which is why a
  test that drives every pass without pinning an owner still receives a boot
  pass it must remember to settle. The branch is test-support-only and
  production is unconditional, so this is flagged rather than changed.
