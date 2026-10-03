# Audit outbox r3 closure-review verdict

## Immutable subject

- Repository: `/home/thorrester/Documents/GitHub/wyrd/.claude/worktrees/agent-aad682fbca5074900`
- Base: `cf5ee4128ce0b842e00a0eb20770ab5c285dedd8`
- Candidate: `52e1144b5c186ccacd85d9c779a4e60c3cce5ac2`
- Reviewed range: `cf5ee4128ce0b842e00a0eb20770ab5c285dedd8..52e1144b5c186ccacd85d9c779a4e60c3cce5ac2`
- Branch: `worktree-agent-aad682fbca5074900`
- Approved specification: `changes/active/audit-outbox/spec.md`, revision 4
- Prior review: `changes/active/audit-outbox/review/r2/`
- Remediation inputs:
  `review/r2/TASK-AUDIT-OUTBOX-R2-bounded-remediation.md` and
  `review/r2/TASK-AUDIT-OUTBOX-R3-commit-outcome.md`

The repository has no `.codegraph/` directory. The abandoned committed r3
reports were not used as evidence and were replaced by this review. `HEAD`
remained the candidate throughout discovery, follow-up, validation, and
verdict writing. Only review artifacts under `review/r3/` were changed.

## Reconciled closure matrix

| Scoped obligation | Reconciled implementation and proof | Result |
|---|---|---|
| `FIND-AUDIT-OUTBOX-11`; revision-4 REQ-009 / AC-009 | `AuditSink` records the transaction xid, resolves a returned commit error through `pg_xact_status`, treats committed as success, returns confirmed abort to ordered retry, waits without re-send while unresolved, and counts terminal `NULL` loss without re-send. The production writer/publisher journey covers three landed-but-unacknowledged commits separated by retirement and one confirmed abort, with one retained decision per round and a gap-free retained prefix. Event-ID staging, projection, reader collapse, and the unreleased migration are removed. | **CLOSED** |
| `FIND-AUDIT-OUTBOX-12`; REQ-003 / REQ-003a / REQ-008 / AC-008 | Construction and polling panics now return the child-owned batch to ordered retry. Error normalization still calls `S::Error::to_string()` outside containment; a valid panicking `Display` produces `JoinError`, after which the parent has only tenant/count and counts/releases the accepted batch as lost. | **OPEN — `FIND-AUDIT-OUTBOX-12`** |
| `FIND-AUDIT-OUTBOX-13`; shutdown fencing and terminal accounting | `shutdown` takes the sole sender before awaiting; racing stages linearize before or after that fence. Deadline abandonment joins the writer, atomically clears residual pending state, decrements the gauge, counts exact loss once, and wakes waiters. Focused admission and deadline tests pass. | **CLOSED** |
| `FIND-AUDIT-OUTBOX-14`; production `tests.rs` remains scanned | The basename exemption is gone. Four explicit cfg-test module paths are allowlisted, while an otherwise identical production `tests.rs` is rejected. The fixture script and canonical unwrap audit pass. | **CLOSED** |
| `FIND-AUDIT-OUTBOX-7`; top-level imports and bare declaration types | The prior `MutexGuard` and `Uuid` locations are fixed, but the new AC-009 journey uses fully qualified field/signature types and a function-scoped Tokio IO import. | **OPEN — `FIND-AUDIT-OUTBOX-7`** |
| `FIND-AUDIT-OUTBOX-3`; shared audit owner in live security authority | The security posture now names an audit-outbox write failure rather than an Oracle audit commit owner. | **CLOSED** |
| Range regression: required rustdoc on changed Rust items | The materially changed `MemorySink::write` and six new `Recorder for TestMetrics` methods have no rustdoc, including the injection order and intentionally ignored metric operations on which the proof depends. | **FAIL — `FIND-AUDIT-OUTBOX-15`** |
| User-directed risk: in-progress/unreachable wait and `NULL` loss lack direct tests | These remain a verification limit, not a closure finding. AC-009 selects committed ambiguity, abort, and repeated ambiguity across retirement for production integration proof; those paths are covered. Source tracing shows no append/re-send from the wait branch and one counted non-resending terminal from `NULL`. | ACCEPTED LIMIT |
| User-directed risk: an unresolved batch holds one of four writer slots | The wait retains one logical tenant-write slot and no database connection during backoff, preserving sole ownership and same-tenant order while leaving three slots. Four simultaneous unresolved outcomes can occupy all four slots; this is the bounded availability tradeoff required by revision 4, with capacity explicitly deferred. | ACCEPTED |
| User-directed risk: deleted migration | The event-ID migration belonged only to the user-stipulated unreleased revision-2/3 implementation. Removing it keeps the schema, writer, projection, and readers aligned with revision 4; retaining it would preserve a prohibited column. | ACCEPTED |
| Deferred integration obligations | `FIND-AUDIT-OUTBOX-5`, `mise run bench:capacity`, and `mise run gate`. | DEFERRED BY USER |

## Independent review results

| Review | Result | Material proposals after reconciliation |
|---|---|---|
| Behavior review | PASS | None |
| Invariant review | FAIL | Incomplete panic containment (`FIND-12`) |
| Repository standards review | FAIL | Repeated import/type violation (`FIND-7`); mandatory extra Postgres-branch tests rejected in validation |
| Maintainer review | FAIL | Import/type rule, missing rustdoc, and documentation proposals; only the import/type and rustdoc proposals survived validation |
| System-resilience review | FAIL | Incomplete panic containment (`FIND-12`) |
| Concurrency/resource domain | PASS | None |
| Durability/persistent-data domain | PASS | None |
| Security/tenancy domain | PASS | None |

The focused follow-up was required because discovery conflicted on panic
reachability, test sufficiency, and scope. It resolved all uncertainties and
forwarded four proposals. Structured Ponytail validation retained three,
rejected a mandatory direct-test expansion, rejected the changed AuditSink
overview proposal, and excluded the stale pre-range Bifrost sentence under the
user-directed scope.

## Validated finding ledger

The final independently validated ledger is preserved in
`findings-validation.md`:

- `FIND-AUDIT-OUTBOX-12` — **REVISED / INCORRECT**: error rendering can panic
  outside containment and lose the child-owned batch in a live process.
- `FIND-AUDIT-OUTBOX-7` — **REVISED / REGRESSION / VIOLATION**: the new
  closure journey repeats the module-import and bare-declaration-type rule.
- `FIND-AUDIT-OUTBOX-15` — **CONFIRMED / VIOLATION**: changed and new outbox
  test methods lack mandatory rustdoc.

No retained correction requires a new product, public API, architecture,
security, compatibility, cross-service, concurrency-semantics, resource-
ownership, or persistent-data decision.

## Verification and limits

- Independently passed:
  `mise exec -- cargo nextest run --locked -p wyrd-runtime --lib -E
  'test(/^outbox::tests::/)'` — 7/7.
- Independently passed: `mise exec -- python3
  scripts/test_check_unwrap_audit.py` and `mise run check:unwrap-audit` with
  the tool cache redirected to a writable temporary directory.
- The implementer records the exact ambiguous-commit journey, Bifrost SQL
  integration, Bifrost server journey, `test:wyrd`, formatting, lints, docs,
  unwrap audit, and diff check as green.
- This review could not independently start the Docker-backed Postgres lane
  because the sandbox lacked permission to the configured Docker socket. The
  production journey and injection path were inspected directly; its recorded
  green result remains available evidence rather than an independent rerun.
- Existing green tests do not exercise the retained `Display` panic and cannot
  satisfy the source-static import or rustdoc rules.

## Verdict

**FIX_REQUIRED.** Revision-4 `FIND-AUDIT-OUTBOX-11` is closed, as are `-13`,
`-14`, and `-3`; the directed slot and migration risks are accepted. The
cumulative candidate still has the three bounded findings above. Remediation is
defined in `TASK-AUDIT-OUTBOX-R4-closure.md`.
