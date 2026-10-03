# Persistent-data and transaction domain review

## Immutable subject

- Repository: `/home/thorrester/Documents/GitHub/wyrd/.claude/worktrees/agent-aad682fbca5074900`
- Base: `cf5ee4128ce0b842e00a0eb20770ab5c285dedd8`
- Candidate: `52e1144b5c186ccacd85d9c779a4e60c3cce5ac2`
- Range: `cf5ee4128ce0b842e00a0eb20770ab5c285dedd8..52e1144b5c186ccacd85d9c779a4e60c3cce5ac2`
- Approved authority: `changes/active/audit-outbox/spec.md`, revision 4

`HEAD` remained the candidate while this report was produced. The repository
has no `.codegraph/` index, so navigation used the source tree and git diff.
The abandoned earlier contents of `review/r3/` were not used as review
evidence.

## Reviewed boundary

This review followed audit decisions from the generic process queue through the
tenant transaction, commit-outcome reconciliation, hash-chain staging,
publisher range identity, retained history, and staging retirement. It was
limited to closure of `FIND-AUDIT-OUTBOX-11`, `-12`, `-13`, `-14`, `-7`, and
`-3`, plus regressions introduced by the immutable range that could lose,
duplicate, misattribute, or cross tenants for audit decisions.

| Boundary | Authority and source coverage | Assessment |
|---|---|---|
| Commit identity and outcome | Revision-4 REQ-009/AC-009; `crates/vala/vala-sql/src/audit_outbox.rs:65-158`; PostgreSQL `pg_current_xact_id` / `pg_xact_status` documented semantics | The transaction ID is obtained inside the same tenant transaction before append. A failed commit is classified on a separately acquired pool connection. `committed` completes without retry, `aborted` returns the original error to the generic front-of-queue retry, `in progress` and query failure wait with capped backoff, and `NULL` counts the batch lost and completes without resend. This is the state transition revision 4 requires. |
| Tenant and chain transaction | `architecture/agent-rules.md`; `crates/vala/vala-sql/src/queries/audit_staging.rs:50-175`; `TenantConn` lifecycle | The existing RLS-bound `TenantConn` remains the tenant authority. Transaction ID read, chain-head lock, all row inserts, head update, and commit are one transaction. An aborted attempt consumes no sequence; a committed attempt advances the gap-free chain once. |
| Generic pending/retry ownership | REQ-003/003a/007/008; `crates/shared/wyrd-runtime/src/outbox.rs:143-227,266-433,435-468` | A sink batch remains in the one in-flight task while its outcome is unresolved, so later same-tenant work cannot overtake it. Confirmed abort returns the owned batch to the tenant queue front. Committed and unresolvable outcomes release generic pending exactly once; only the `NULL` branch increments audit loss before returning success, so generic shutdown/retry accounting does not count that loss twice. |
| Publisher and retirement seam | `architecture/bifrost-design.md:613-645`; `crates/vala/vala-sql/src/queries/audit_staging.rs`; `crates/vala/vala-bifrost-redux/src/tables/audit/projection.rs`; production `AuditPublisher` journey | Publisher identity remains the frozen tenant sequence range. It can retire a committed row while the writer resolves the lost acknowledgement without creating the r2/r3 duplicate: the writer never resends a transaction that Postgres reports committed. Confirmed abort retries before later same-tenant items; `NULL` never resends. Readers contain no event-ID collapse. |
| Event-ID schema removal | Deleted `20261003000001_audit_staging_event_id.sql`; `AuditStagingRow`; staging queries; `AuditLogTable`; projection and test-harness reads | The final schema and all writer/publisher/read projections consistently omit audit `event_id`; no partial event-ID authority remains in production audit code. Revision 4 explicitly requires this removal. |
| Migration lifecycle | `architecture/operations/deployment-and-release.md:135-173` and its statement that no Wyrd image has yet been published; range history | The deleted migration was introduced only in the unreleased revision-2/3 candidate and implemented a contract revision 4 explicitly supersedes. No released application/schema compatibility interval can contain it, so removing it is not a shipped migration rewrite. An ephemeral or development database that applied the intermediate candidate must be rebuilt; it is not a supported upgrade source and no accepted production audit evidence is being contracted. |
| Closure-adjacent loss fixes | `outbox.rs:328-423,435-463,764-823`; unwrap checker and fixtures; security authority wording | The sink future's construction and polling panics retain ownership and re-enter retry (`FIND-12`); shutdown fences admission and settles pending/lost accounting (`FIND-13`); the filename-wide unwrap exemption is replaced by explicit cfg-test entries (`FIND-14`); bare imports close `FIND-7`; and shared outbox ownership wording closes `FIND-3`. No persistent-data regression was found in those changes. |

PostgreSQL's primary documentation confirms that `pg_xact_status(xid8)`
returns exactly `in progress`, `committed`, `aborted`, or `NULL` when the
status has aged out, and specifically identifies disconnected `COMMIT`
resolution as an intended use:
<https://www.postgresql.org/docs/16/functions-info.html>.

## Prior-finding closure

| Finding | Persistent-data closure result |
|---|---|
| `FIND-AUDIT-OUTBOX-11` | **CLOSED under revision 4.** The production sink resolves the exact transaction before returning to the generic writer. The journey at `crates/wyrd/wyrd-testing/tests/bifrost/server/audit_publication.rs:976-1054` cuts three commit acknowledgements, lets the production publisher retire each row between rounds, then cuts a commit before Postgres receives it. It proves one retained row per decision and a retained sequence population equal to the chain head. |
| `FIND-AUDIT-OUTBOX-12` | **CLOSED.** The write task catches panics while it still owns the item vector and converts them to the ordinary retry path. The focused test proves in-order recovery, one write-failure count, zero lost count, and zero terminal pending. No reachable audit-sink panic path in the reviewed range bypasses this containment. |
| `FIND-AUDIT-OUTBOX-13` | **CLOSED.** Shutdown takes and drops the only sender before awaiting the writer; late stages are refused/count lost and pre-fence work drains. Deadline abandonment clears both atomic pending and its gauge after the writer has stopped. |
| `FIND-AUDIT-OUTBOX-14` | **CLOSED.** The production `tests.rs` fixture is scanned while only the four explicitly mapped cfg-test modules are skipped. No persistent-data exception was introduced. |
| `FIND-AUDIT-OUTBOX-7` | **CLOSED.** The changed declarations use top-level `MutexGuard` and `Uuid` imports and bare names. |
| `FIND-AUDIT-OUTBOX-3` | **CLOSED.** Live security authority names the shared audit-outbox write failure rather than an Oracle commit owner. |

## Implementer-reported risk judgments

### In-progress/unreachable wait and `NULL` loss lack direct tests

Accepted as a verification limit, not a material closure finding. The
revision-4 production journey directly proves the two commit outcomes that
decide resend (`committed` and `aborted`) and repeats committed ambiguity across
publisher retirement. The remaining branches are a literal mapping of the
native closed status set: the wait branch performs no append and loops, while
`NULL` increments the audit loss counter and returns success so the generic
writer cannot resend. Forcing aged-out commit status or a durable in-progress
commit through the production server would require a new database fault harness
or production seam not required by AC-009. This remains residual risk because
the metric/no-resend behavior is source-validated rather than directly
executed.

### A waiting batch holds one of four writer slots

Accepted and consistent with the approved protocol. The slot is the ownership
fence that prevents the unresolved batch's tenant from being dispatched again.
One unresolved tenant leaves three bounded slots available. If Postgres is
unreachable, no audit batch could commit through an otherwise freed slot; on
recovery the held resolver asks Postgres before any resend. Moving this wait
outside the write owner would require another unresolved-state scheduler and
would weaken the simple no-resend invariant. Capacity qualification remains the
explicitly deferred `bench:capacity` obligation.

### Deleted unreleased migration

Accepted. The file encoded the superseded event-ID design, entered only the
unreleased candidate history, and revision 4 requires that audit staging and
retained history have no event ID. Keeping or replacing it would leave durable
contract drift. This judgment depends on the repository's explicit no-release
state; deleting an already shipped migration would fail the migration contract.

## Verification evidence and limits

- Reviewed the implementer-recorded green exact journey, SQL integration,
  Bifrost server journey, Wyrd family, formatting, lint, documentation, unwrap
  audit, and diff-check results in
  `review/r2/TASK-AUDIT-OUTBOX-R3-commit-outcome.md`.
- Source-inspected the journey's commit proxy, all four resolution branches,
  the generic writer's retry/pending/loss interaction, publisher range identity,
  staging retirement, final retained projection, migration registry ownership,
  and the earlier bounded-remediation changes.
- Independently ran `git diff --check` on the persistent-data source slice; it
  returned clean.
- No Cargo-backed lane was rerun by this domain reviewer. The in-progress /
  temporarily unreachable loop and aged-out `NULL` loss branch have no direct
  test, as assessed above.
- `mise run bench:capacity` and `mise run gate` remain deferred to integration
  by user direction and are not domain-review failures.

## Material proposed findings

None.

No reviewed range path was found that loses an audit decision outside the
approved and counted boundaries, duplicates it after publisher retirement,
misattributes it, crosses tenant authority, or allows later same-tenant work to
overtake an unresolved/aborted batch.

## Overall result

**PASS**
