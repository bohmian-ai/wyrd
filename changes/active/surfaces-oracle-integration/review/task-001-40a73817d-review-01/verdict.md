# TASK-001 task-review verdict

## Immutable subject

- Approved spec: `changes/active/surfaces-oracle-integration/spec.md`, revision 7
- Original task: `changes/active/surfaces-oracle-integration/tasks/TASK-001-integrate-redux-data-plane.md`
- Base: `089f626c7681f4c8bf8abdaddedb61e4a35a26d5`
- Candidate: `40a73817d415e9a1626e6ec7a91edda083e344d3`
- Verdict: **SPEC_REVISION_REQUIRED**

The source candidate stayed immutable. Review artifacts are outside the candidate commit.

## Acceptance matrix

| Obligation | Implementation evidence | Verification evidence | Result |
|---|---|---|---|
| Redux is the sole Bifrost engine; legacy authority is absent | Redux tree, workspace graph, and no-legacy inventory | Recorded boundary checks | PASS |
| Scribe durability, replay, bounded ownership, and tenant-qualified identity | Redux WAL and batch-fence owners | Redux/Scribe/SQL lanes | PASS |
| Oracle one-build execution, local admission, authorization, reader protection, and terminal failure | Redux Oracle owners | Oracle/server lanes | PASS |
| Reader/Forge serialization and fail-closed maintenance | Reader and Forge durable lineage owners | Forge/server lanes | PASS |
| Forge progress, retry/reconciliation, FIFO admission, and cleanup separation | Redux Forge owners | Forge lane | PASS |
| Canonical OTLP/Arrow convergence and attribution | Three canonical table owners | OTLP lane | PASS |
| Every allowed/denied permission decision is recorded exactly once and fails closed | Bifrost catalog and other route checks omit decisions; non-permission transitions still append | Existing tests encode missing allowed rows | FAIL |
| Oracle WAL replay obeys the permitted duplicate bound | Relay appends before checkpoint without a stable Postgres dedup identity | Existing test asserts only `count >= 2` | FAIL |
| Frozen-range publication is bounded and idempotent | Revision-7 bound and direct-Scribe path | SQL/server/Scribe lanes | PASS |
| Required negative/recovery journeys and exact evidence closure | Broad lanes recorded; exact selectors and complete cross-surface proof absent | No aggregate/MCP/docs proof | FAIL |
| Repository standards | Independent standards review | `standards-review.md` | FAIL |
| Non-goals and Ponytail minimum | No second engine, lease, claim table, scheduler, compatibility path, or added dependency | Diff inspection | PASS |

## Material findings

### FIND-TASK-001-1 — MISSING: allowed permission evaluations are not audited

REQ-026, AC-005, and TASK-001 require every allowed and denied permission decision to append exactly once before proceeding/refusing and to fail closed when audit is unavailable. `crates/wyrd/wyrd-server/src/bifrost/service.rs:30-55` returns immediately on `PermissionVerdict::Allow`. Successful list/describe explicitly record nothing (`:181-259`), and idempotent, invalid, or conflicting registration branches proceed after the same unaudited Allow (`:101-178`). The existing test at `:470-525` asserts this absence. Legitimate decisions therefore disappear from retained history and can proceed during audit failure. The required outcome is one tenant-owned event per actual verdict, preserving the create transaction coupling and avoiding duplicate registration events.

### FIND-TASK-001-2 — INCORRECT: canonical audit still contains non-permission transitions

The approved boundary records permission decisions and nothing else. Candidate paths including `auth/login.rs:51-75`, card reconciliation/lifecycle audit calls in `components/cards/service.rs`, and backend lifecycle audit in `wyrd-storage/src/audit.rs` convert authentication, reconciliation, blob, or storage outcomes into `Allowed`/`Denied` audit events without a permission evaluation. The retained ledger's cardinality therefore does not describe authorization decisions. Remove these canonical audit appends while preserving their owning operational lineage or diagnostics.

### FIND-TASK-001-3 — INCORRECT: repeated Oracle relay crashes exceed REQ-026A's duplicate bound

`wyrd-server/src/oracle/query_audit.rs:471-488` commits the WAL event to Postgres and checkpoints afterward, but staging persists no stable relay identity. A crash after each commit and before each checkpoint appends the same accepted logical query at sequences A, B, C, and onward. The test at `:751-787` accepts `count >= 2` and cannot detect a third replay. REQ-026A permits one valid duplicate, not unbounded duplicates; REQ-029 forbids duplicated retained history. Closing this safely requires choosing durable Postgres idempotency identity/semantics, an expensive persistent-data decision not fixed by revision 7, so the review cannot prescribe it as ordinary remediation.

### FIND-TASK-001-4 — MISSING: required focused and cross-surface proof is absent

TASK-001 requires exact focused commands for every named changed Redux, Scribe, Oracle, Forge, OTLP, server, and audit scenario. Its evidence records broad lanes only and omits complete revision-7 closure. The broader required `verify:bifrost`, owning MCP runtime proof, and `docs:check` are also absent.

### FIND-TASK-001-5 through FIND-TASK-001-11 — VIOLATION: repository standards fail

The independent audit's stable mapping is: `FIND-TASK-001-5` raw `PgPool` state (`STD-001`); `-6` manual tenant predicates on `TenantConn` (`STD-002`); `-7` rustdoc blockers (`STD-003`); `-8` fully-qualified signature types (`STD-004`); `-9` contradictory architecture/public docs (`STD-005`); `-10` incomplete required verification (`STD-006`); and `-11` invalid task lifecycle metadata (`STD-007`). Their exact locations, consequences, and testable corrections are preserved in `standards-review.md`.

## High-risk boundary review

The independent boundary specialist confirmed the missing authorization audit and repeated-crash Oracle duplication. It otherwise passed the frozen publication range, RLS behavior, stale settlement, Scribe batch fencing, and WAL-first read acceptance.

## Verification limits

The audit used immutable commit blobs and recorded results rather than rerunning the expensive lanes. One unnamed Redux failure did not reproduce in four later runs. TASK-006's initially stale Scribe evidence was superseded by the recorded 21/21 closeout run, but this does not close the findings above.

## Prior-finding closure

No prior TASK-001 task-review verdict exists in this review directory. The implementation's earlier defect log is evidence, not a prior independent verdict.
