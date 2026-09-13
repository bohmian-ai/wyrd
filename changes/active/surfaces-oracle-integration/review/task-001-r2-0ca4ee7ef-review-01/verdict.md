# TASK-001-R2 review verdict

**Verdict: FIX_REQUIRED**

## Immutable subject

- Approved spec: `changes/active/surfaces-oracle-integration/spec.md`, revision 7
- Original task: `changes/active/surfaces-oracle-integration/tasks/TASK-001-integrate-redux-data-plane.md`
- Remediation task: `review/task-001-r1-8e61c0349-review-01/TASK-001-R2-close-r1-review-findings.md`
- Candidate: `0ca4ee7ef7416f870ce3244add406901b57337de` (branch `change/surfaces-oracle-integration`); R2 base `8e61c0349`; R1 remediation base `8377fff9f`
- Candidate unchanged through both waves.

## Wave 1 results

| Reviewer | Report | Result | Proposed findings |
|---|---|---|---|
| `task-rev` | `task-review.md` | FAIL | TR-1..TR-5 |
| `repo-rev` | `standards-review.md` | FAIL | SR-1 |
| `domain-rev` (security/tenancy/audit durability) | `domain-review-security-tenancy-durability.md` | FAIL | DR-1 |
| `ponytail-rev` (Wave 2) | recorded in this verdict | FIX_REQUIRED | ledger below |

## Acceptance matrix

| Obligation | Result | Evidence |
|---|---|---|
| R1-1 system-owner audit retained once; non-audit nil ingress fails | PASS | `audit_log.rs:23-35`, `tenant_table.rs:122`, `scribe/ingress.rs:58-61`; 3 unit tests + journey pass (task-rev ran) |
| R1-2 every Bifrost registration verdict once, incl. pre-commit failure and same-FQN race | PASS | `bifrost_catalog.rs:993-1018`, `bifrost/service.rs:155-176`; 2 tests pass (race branch not forced; both branches read correct) |
| R1-3 issuer Allowed recorded before discovery, no spanning transaction | PASS | `admin/routes.rs:232-278`; test passes |
| R1-4 Card register/delete verdicts transactional; no-write outcomes record once | PASS | `append_on` in write/delete transactions; Card set 8/8 |
| R1-5 sweep never exceeds fixed ceiling | FAIL | No proof; approval unrecorded (FIND-TASK-001-R1-5) |
| R1-6 exact passing proof; TASK-002 lanes outside R2; required lanes green | FAIL | `journey:mcp` 7/8 on validator run; `662bf33bc` rewrote TASK-002 lanes (FIND-TASK-001-R1-6) |
| R1-7 bare types and rustdoc on touched items | FAIL | `bifrost_catalog.rs:1490`; Card HTTP handlers lack `# Errors` (FIND-TASK-001-R1-7) |
| R1-8 lifecycle states | PASS | frontmatter |
| R1-9 service-account verdicts through configured `PermissionCheck` | PASS | `authorize_recording_denial`; test passes |
| REQ-026 every permitted service-account write records its verdict exactly once | FAIL | rolled-back Allowed rows on error paths; false module doc (FIND-TASK-001-R2-1) |
| Original task invariants (Redux sole engine, RLS, gapless chain, frozen range, Scribe fences, Oracle WAL-first, Forge lineage) | PASS | no R2 change; domain-rev traced |
| Non-goals (no new seam/config/dependency/migration; no TASK-002 broadening) | FAIL | `662bf33bc` (FIND-TASK-001-R1-6 b) |

`ebf8dd7ae` (skills, review packet, TASK-002 notes) is review/planning material, not implementation drift. `check:skills-sync` fails on mtimes only; contents identical.

## Validated finding ledger

| ID | Class | Summary |
|---|---|---|
| FIND-TASK-001-R1-5 | MISSING | Fixed publication concurrency ceiling (8) has no executable proof; claimed human approval is unrecorded and the pool blocker is incorrect |
| FIND-TASK-001-R1-6 (a) | MISSING | `test:bifrost:journey:mcp` fails intermittently because the test reads transient `vala.audit_staging`; INV-025 requires green lanes |
| FIND-TASK-001-R1-6 (b) | DRIFT | `662bf33bc` delivered TASK-002 Postgres lane rewrites inside R2; record transfer to TASK-002, do not revert |
| FIND-TASK-001-R1-7 (a) | VIOLATION | New `append_registration_audit` signature uses qualified `wyrd_sql::TenantConn` |
| FIND-TASK-001-R1-7 (b) | VIOLATION | Changed Card HTTP handlers lack rustdoc audit behavior and `# Errors` |
| FIND-TASK-001-R2-1 | MISSING + DRIFT | Five service-account write handlers roll back their Allowed audit row on error paths; R2 module doc falsely claims exactly-once |

Full corrections and closure proofs are in `TASK-001-R3-close-r2-review-findings.md`. The `ponytail-rev` ledger is recorded here because writing a separate `findings-validation.md` was blocked by tooling.

### Wave 2 validation decisions (`ponytail-rev`)

- TR-1 CONFIRMED -> R1-5. Approval unrecorded; stated pool blocker wrong (publisher uses Vala pool 16; directory uses `OperatorPool`; app-pool cap overridable; per-server ephemeral DB). Proof feasible; no spec revision or deferral.
- TR-2 REVISED -> R1-6 (b). Drift is real, but record the transfer to TASK-002 rather than revert (revert restores tasks depending on deleted `setup:db-roles`; R2 did not need the change).
- TR-3 + SR-1 CONFIRMED, merged -> R1-7 (a). Pre-R2 qualified uses out of scope.
- TR-4 CONFIRMED -> R1-7 (b).
- TR-5 REVISED -> R1-6 (a). Not caused by R2, but INV-025 blocks and no TASK-006 ownership exists. Validator reproduced `journey:mcp` 7/8 (`delegated_agent_query_is_attributed_in_its_durable_audit_record` `RowNotFound`); root cause is the test reading transient `vala.audit_staging`. `journey:server` 9/9 in one run; no server code change unless reproduced.
- DR-1 CONFIRMED -> R2-1. Reachable under REQ-026 (TASK-001 imports TASK-005 matrix); R2 module doc at `admin/routes.rs:15` is false.
- Rejected: unrecorded approval as evidence; reverting `662bf33bc`; server-lane code change without reproduction; pre-R2 qualified types; standards observations O-1..O-4.

## Verification limits

- Postgres lanes run one at a time. Validator ran `journey:mcp` (7/8) and `journey:server` (9/9, once). Full cumulative set (`lints`, `test:sql`, other journeys, checks) not re-run by reviewers; `fmt`, `unwrap-audit`, `clippy-allow-audit`, inventory, and clippy on four touched crates passed (repo-rev).
- Same-FQN race test does not force the concurrent-winner branch.

## Prior finding closure

Closed: R1-1, R1-2, R1-3, R1-4, R1-8, R1-9. Open: R1-5, R1-6, R1-7. New: R2-1.

## Remediation

`TASK-001-R3-close-r2-review-findings.md` in this directory, routed to `$wyrd-implement`.
