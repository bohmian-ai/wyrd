# TASK-001 r3 verdict

## Immutable subject

- Base: `a5c8041a348a66bfb56fbac492383e8c688b0590`
- Candidate: `eb4142cc95fa24b0525c71f66bfc978577cd4b7c`
- Candidate tree: `09ffa00e468cc5d518333b8e5ad20ffd4380c150`
- Approved specification: `changes/active/oidc-production-readiness/spec.md`, revision 4
- Original task: `changes/active/oidc-production-readiness/tasks/TASK-001-tenant-connections.md`
- Prior remediation tasks: `TASK-001-R1-production-readiness-gaps.md` and
  `TASK-001-R2-remaining-production-readiness-gaps.md`
- Review: `changes/active/oidc-production-readiness/review/TASK-001-r3/`

The candidate commit and tree matched the supplied immutable subject before
Wave 1, after Wave 1, and after Wave 2. Review artifacts were the only files
written during the audit.

## Acceptance matrix

| Obligation | Result | Evidence |
|---|---|---|
| OIDC remains optional; keyless boot refuses stored provider ciphertext | PASS | Optional connection owner, cross-store inventory, and committed connectionless proof. |
| One Active and at most one Candidate per tenant; tenant isolation | PASS | RLS, partial unique indexes, tenant slot lock, and two-tenant/concurrent journey source. |
| Authorized redacted typed headless lifecycle API | PASS | Six routes, bearer-derived tenancy, bounded authorized decode, durable lifecycle owner, and served-contract proof. |
| Exact callback and client-auth qualification | PASS | Exact response-arm qualification, deployment-owned callback, typed unsupported-auth refusal, and focused tests. |
| Production-safe provider endpoints and bounded IO | FAIL | Server fetches are HTTPS-screened and body-bounded, but live login accepts a newly discovered HTTP browser destination and DNS lookup has no deadline (`FIND-TASK-001-20`, `FIND-TASK-001-23`). |
| Secret validation, redaction, sealing, key-file safety, and rotation | PASS | Versioned sealing, restrictive file loading, cross-store CAS rewrap, retired K1 writer, and K2-only serving proof. |
| Replacement/deactivation/removal stop old login and renewal | PASS | Exact connection provenance, locked rechecks, and unbound legacy refresh rows. |
| Tenant SQL capability and migration boundaries | PASS | `WyrdPostgres`/`TenantConn`, RLS, preserved workload binding, and preflight/refusal coverage. |
| Canonical transactional audit for every permission evaluation | FAIL | Activation audits the bearer decision but not the recovery principal's separate permission decision (`FIND-TASK-001-22`). |
| Repository Rust documentation rules | FAIL | The R2-added fallible `bounded_get` test helper lacks mandatory `# Errors` documentation (`FIND-TASK-001-21`). |
| Served OpenAPI, generated contracts, docs, and journey selection | PASS WITH LIMIT | Source and recorded lanes align; long and Cargo-backed lanes were not rerun during this time-bounded review. |
| Non-goals remain excluded | PASS | No UI, hosted signup, commercial hook, second trust store, compatibility route, new auth method, or new configuration surface entered the range. |

## Wave results

| Report | Result | Proposed findings |
|---|---|---|
| `task-review.md` | FAIL | `TASK-REV-R3-001` |
| `standards-review.md` | FAIL | `STD-R3-001` |
| `domain-review-security.md` | FAIL | `SEC-R3-001` through `SEC-R3-003` |
| `domain-review-tenancy-data.md` | FAIL | `TD-R3-001`, `TD-R3-002` |
| `domain-review-secrets.md` | PASS | None |
| `findings-validation.md` | FIX_REQUIRED | `FIND-TASK-001-20` through `FIND-TASK-001-23` |

Wave 2 deduplicated the live-login and recovery-audit proposals. It rejected
`TD-R3-001`: host-selected callback tenancy exists in the base, is outside
TASK-001's declared requirement set, and requires a separate callback-routing
and persistent-state decision that this task explicitly does not own.

## Validated finding ledger

| ID | Status | Classification | Required outcome |
|---|---|---|---|
| `FIND-TASK-001-20` | REVISED | INCORRECT | Apply the existing deployment scheme rule to every freshly discovered live-login authorization endpoint before state is persisted or a redirect is returned. |
| `FIND-TASK-001-21` | CONFIRMED | VIOLATION | Add the mandatory `# Errors` rustdoc to the existing fallible bounded-body test helper. |
| `FIND-TASK-001-22` | CONFIRMED | VIOLATION | Append the recovery principal's allowed or denied permission decision through the canonical audit path in the activation transaction. |
| `FIND-TASK-001-23` | CONFIRMED | VIOLATION | Bound the shared DNS lookup with the existing provider timeout and preserve redacted unresolved handling and address pinning. |

Exact reachability evidence, caller tracing, consequences, correction
boundaries, and focused closure proofs are authoritative in
`findings-validation.md`.

## Prior-finding closure

`FIND-TASK-001-1` through `FIND-TASK-001-19` are closed at their validated
correction boundaries. `FIND-TASK-001-20` is distinct from
`FIND-TASK-001-18`: the prior correction protects server-side provider
requests, while the new finding concerns the browser destination returned by
live discovery.

## Verification limits

- Reviewers inspected the complete cumulative diff, governing authorities,
  changed source, complete caller bodies, migrations, tests, documentation,
  prior findings, and recorded implementation evidence.
- The review was static and time-bounded. No Cargo-backed, Postgres, provider,
  OpenAPI, codegen, docs, format, lint, or broad journey lane was rerun.
- `git diff --check` passed. CodeGraph was unavailable because the repository
  has no `.codegraph/` index.
- DNS-stall reachability was proven from await/timeout ordering rather than by
  disrupting the shared workstation resolver; closure should use deterministic
  Tokio time.
- Every required Wave 1 reviewer and the Wave 2 validator completed within the
  20-minute reviewer limit. No missing-reviewer verification gap applies.

## Verdict

**FIX_REQUIRED**

Implement
`changes/active/oidc-production-readiness/review/TASK-001-r3/TASK-001-R3-production-readiness-gaps.md`
through `$wyrd-implement`, then review the complete original
base-to-remediated-candidate range again.
