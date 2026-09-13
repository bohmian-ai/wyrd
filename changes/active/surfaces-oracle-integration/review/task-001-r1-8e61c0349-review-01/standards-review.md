# TASK-001-R1 repository-standards review

## Immutable subject

- Repository: `/Users/stevenforrester/Documents/GitHub/wyrd-bifrost-surfaces`
- Remediation base: `8377fff9f03cc60de4be3e088569e38984382dc4`
- Candidate: `8e61c03493f5c0ebcc12cdb4a9998df284f7fc8f`
- Result: **FAIL**
- CodeGraph: unavailable because `.codegraph/` is absent

The candidate remained immutable and the worktree was clean throughout the
independent standards audit.

## Authority coverage

| Changed surface | Applicable authority | Result |
|---|---|---|
| Active task and review records | `AGENTS.md` §§14–16; spec-driven development; implementation execution; testing workflows | FAIL: unsupported R1 status and incomplete focused proof |
| Redux Gate and Scribe | `AGENTS.md` Rust/async/test rules; `architecture/agent-rules.md`; Bifrost design; security posture; Rust, Vala, OLAP, and reliability references | FAIL only on touched signature/rustdoc rules; static Gate composition and engine/audit ownership pass |
| Audit staging and publisher | SQL/audit ownership rules; Bifrost/security/reliability authorities | FAIL on source documentation and proof; SQL capability ownership, RLS, frozen ranges, and bounded source shape pass |
| Server audit/auth/admin/Card/storage/Eval routes | Wyrd design/doctrine; security posture; permission-check authority; errors and agent-harness references | FAIL: transaction coupling, authorization chokepoint, and touched Rust rules |
| `wyrd-sql` tenant directory and `wyrd-storage` | SQL capability, tenant isolation, storage ownership, and reliability rules | FAIL only on touched Rust rules; `OperatorPool`, `TenantConn`, RLS, and operational-lineage boundaries pass |
| CLI, MCP, server, storage, and Bifrost tests | `AGENTS.md` §§11 and 16; testing workflows; agent harness; spec-driven development | FAIL: one changed scenario is unexecuted, exact proof is incomplete, and touched test docs fail |
| Architecture and public docs | Authority hierarchy; Wyrd/Bifrost design; security/Vala/OLAP/reliability references; generated-artifact rules | PASS for the edited pages; one cumulative Card dead-letter statement remains stale |
| Dependency, feature, client-tier, and PyO3 boundaries | `AGENTS.md` ownership/boundary rules; architecture constraints | PASS |

## Rule results

| Rule | Result | Evidence |
|---|---|---|
| Immutable review subject | PASS | Candidate stayed `8e61c0349`; `git diff --check 8377fff9f..8e61c0349` passed |
| Task lifecycle vocabulary | FAIL | R1 uses `status: implemented`; only `proposed`, `ready`, `in_progress`, `review`, `approved`, and `superseded` are valid |
| Struct-centered ownership | PASS | `AuditPublisher` owns its dependencies; Gate remains the workflow owner |
| SQL capability and RLS boundaries | PASS | Raw pool propagation and redundant `TenantConn` predicates were removed |
| Authorization-only canonical audit | PASS for removed engine events | Login, reconciliation, Scribe/Forge, and storage lifecycle mechanics no longer append canonical audit |
| Authorized-operation transaction coupling | FAIL | Card registration and both deletion routes commit Allowed audit separately from their authoritative SQL mutation |
| Runtime `PermissionCheck` chokepoint | FAIL | `authorize_service_accounts_write` directly calls the shortcut authorization helper |
| Bare signature types/top-level imports | FAIL | Touched server, storage, state, Eval, and journey signatures retain fully qualified types |
| Complete and accurate rustdoc | FAIL | Touched fallible routes/helpers/tests lack required sections; several Card comments still describe removed lifecycle audit |
| Exact focused verification | FAIL | A changed Card reconciliation test never ran; other materially changed named tests lack exact recorded selectors |
| Generated artifacts and public docs | PASS | Recorded and independently rerun `codegen:check` and `docs:check` are clean |
| No gate circumvention | PASS | No new suppressions, weakened assertions, or ignored required tests were found |
| Client/PyO3/dependency boundaries | PASS | No manifest/feature change; recorded boundary checks pass |

## Material standards findings

### STD-R1-01 — Allowed Card write decisions do not share the operation transaction

`architecture/agent-rules.md` and the architecture constraints require an
Allowed decision to share the authoritative operation transaction where one
exists. `audit::authorize` commits standalone, and Card registration plus both
delete routes call it before service-owned registry transactions. The audit can
therefore say Allowed after the SQL effect rejects or rolls back. The correction
must reuse `authorize_recording_denial`, `append_on`, and the existing Card
service transactions; reads, completion, and storage sagas remain standalone
because no single transaction may span their external IO.

### STD-R1-02 — Service-account authorization bypasses `PermissionCheck`

`crates/wyrd/wyrd-server/src/audit/mod.rs:260-287` calls
`require_service_accounts_write` directly. The permission-check authority makes
the configured `PermissionCheck` the only runtime RBAC chokepoint. Admin, API-key,
and revocation audit may therefore record a verdict different from the configured
checker. Evaluate the typed permission through the configured checker and retain
the existing action-specific public denial text.

### STD-R1-03 — Touched Rust documentation is incomplete or stale

`AGENTS.md` §16 and `architecture/agent-rules.md` make this a merge blocker.
Representative defects include missing `# Errors` on materially changed
handlers, missing `# Panics` on changed tests, no rustdoc on the renamed sweeper
test, and stale Card service/test prose still claiming lifecycle/dead-letter audit
events after those events were removed. The correction is a diff-scoped inventory,
not a crate-wide documentation sweep.

### STD-R1-04 — Fully qualified types remain in touched signatures

The bare-type rule is violated in `audit/mod.rs`, the `ServerGate` alias,
Eval authorization, storage sweeper signatures, and audit-publication journey
helpers. Import each owning type at module scope and use its bare name; add no
alias or wrapper.

### STD-R1-05 — R1 uses an unsupported lifecycle state

`TASK-001-R1-close-task-review-findings.md:4` says `implemented`. A candidate
awaiting review must say `review`; `approved` is reserved for a later PASS.

### STD-R1-06 — Required focused proof remains incomplete

The changed `card_reconciler_dead_letters_after_three_failures` assertion never
executed because its existing Local-storage server composition cannot become
ready. R1 also records broad or prefix proof where exact changed-test selectors
were required. INV-025 and TASK-001 expressly disallow a pre-existing-failure
waiver. Repair only the existing test composition needed by the changed scenario
and record exact executions; unrelated CLI baseline failures are not candidate
defects and are not remediation scope.

## Verification limits

The specialist independently ran `git diff --check`, `lints`, `docs:check`,
`codegen:check`, `check:from-pools-allowlist`, `check:tenant-isolation`,
`check:unwrap-audit`, `check:client-tier`, and `check:pyo3-scope`; all passed.
Expensive Postgres, Bifrost, CLI, MCP, storage, and journey results were audited
from the candidate's recorded evidence. Passing broad lanes do not close the
unexecuted changed Card test or source-level rule failures above.
