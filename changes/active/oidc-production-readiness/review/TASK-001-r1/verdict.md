# TASK-001 r1 verdict

## Immutable subject

- Base: `a5c8041a348a66bfb56fbac492383e8c688b0590`
- Candidate: `e126cdca7d4bf5bc467279df05cc3e199eb7fdf2`
- Candidate tree: `2f18add99a7c3575580c224cdf26eb5afe211335`
- Approved specification: `changes/active/oidc-production-readiness/spec.md`, revision 4
- Original task: `changes/active/oidc-production-readiness/tasks/TASK-001-tenant-connections.md`
- Review: `changes/active/oidc-production-readiness/review/TASK-001-r1/`

The candidate remained at the supplied commit and tree throughout both review
waves. The review covered the complete base-to-candidate range.

## Verdict

**FIX_REQUIRED**

The implementation establishes most of the tenant connection contract, but 13
bounded findings remain. They can be corrected within the approved behavior and
do not require a specification revision.

## Acceptance matrix

| Obligation | Result | Evidence or finding |
|---|---|---|
| Optional OIDC and connectionless startup | PASS | Optional configuration and connectionless journeys are present. |
| One Active and at most one Candidate per tenant; same-issuer tenant isolation | PASS | RLS, partial unique indexes, tenant slot locking, and concurrent/two-tenant journeys. |
| Typed headless create/read/test/activate/deactivate/remove API | PASS | Six routes, bearer-derived tenant, redacted views, durable reads, and journey coverage. |
| Candidate callback and client-auth qualification | FAIL | `FIND-TASK-001-1`, `FIND-TASK-001-2`. |
| Runtime uses the exact deployment-controlled callback | FAIL | `FIND-TASK-001-3`. |
| DNS screening and connection pinning for provider IO | FAIL | `FIND-TASK-001-4`. |
| Replacement/deactivation/removal immediately stop old login and renewal | FAIL | `FIND-TASK-001-5`. |
| Tenant SQL capability boundary | FAIL | `FIND-TASK-001-6`. |
| Canonical audit cardinality and transactional mutation decision | FAIL | `FIND-TASK-001-7`. |
| Required Rust documentation and signature/import style | FAIL | `FIND-TASK-001-8`, `FIND-TASK-001-9`. |
| Minimal-feature identity journey lane | FAIL | `FIND-TASK-001-10`. |
| Keyless boot fails when stored provider ciphertext exists | FAIL | `FIND-TASK-001-11`. |
| Sealing-key retirement follows a post-roll zero-reference proof | FAIL | `FIND-TASK-001-12`. |
| Public client authentication carries no secret | FAIL | `FIND-TASK-001-13`. |
| Secret redaction, versioned sealing, and durable encrypted storage | PASS | Views/schema/audit are redacted; ciphertext and key IDs are durable; CAS rewrap exists. |
| Legacy Human migration and workload-binding preservation | PASS | Preflight, atomic migration, binding preservation, and isolated migration tests. |
| Old Human admin/CLI/boot write paths refuse; Workload paths remain | PASS | Typed refusal and journey coverage. |
| No hosted signup, commercial hook, UI implementation, second trust store, or unsupported auth kind | PASS | Complete diff inspection found no prohibited scope. |
| Served OpenAPI, generated contract, docs, and exact journey selectors | PASS WITH LIMIT | Candidate records green lanes; reviewers inspected source but did not rerun long lanes. |

## Wave results

| Report | Result | Proposed findings |
|---|---|---|
| `task-review.md` | FAIL | `TASK-REV-001`, `TASK-REV-002` |
| `standards-review.md` | FAIL | `REPO-001` through `REPO-005` |
| `domain-review-security.md` | FAIL | `SEC-001`, `SEC-002` |
| `domain-review-tenancy-data.md` | FAIL | `TD-001`, `TD-002` |
| `domain-review-secrets.md` | FAIL | `SEC-001`, `SEC-002`, `SEC-003` |
| `findings-validation.md` | FIX_REQUIRED | `FIND-TASK-001-1` through `FIND-TASK-001-13` |

The two domain reports use colliding source-local `SEC-*` identifiers. The
validation report qualifies them by report path. All 14 Wave 1 proposals were
independently confirmed or revised; overlaps were deduplicated into 13 stable
findings.

## Validated finding ledger

| ID | Status | Classification | Required outcome |
|---|---|---|---|
| `FIND-TASK-001-1` | REVISED | INCORRECT | Accept only a state-matching OIDC callback redirect from the authorization probe. |
| `FIND-TASK-001-2` | REVISED | INCORRECT | Accept only OAuth `invalid_grant` as proof that client authentication succeeded. |
| `FIND-TASK-001-3` | CONFIRMED | INCORRECT | Use configured public-origin callback for real login, never request headers. |
| `FIND-TASK-001-4` | CONFIRMED | VIOLATION | Disable ambient proxies in the shared screened HTTP client. |
| `FIND-TASK-001-5` | CONFIRMED | MISSING | Bind initial and renewed sessions to the exact active connection id/revision and recheck under the slot lock. |
| `FIND-TASK-001-6` | CONFIRMED | VIOLATION | Remove raw `PgPool` propagation and acquire `TenantConn` through `WyrdPostgres`. |
| `FIND-TASK-001-7` | REVISED | VIOLATION | Back the post-provider transactional audit row with a real second permission evaluation. |
| `FIND-TASK-001-8` | CONFIRMED | VIOLATION | Document all added/materially modified Rust items as repository rules require. |
| `FIND-TASK-001-9` | CONFIRMED | VIOLATION | Import cited types at module scope and use bare names in signatures. |
| `FIND-TASK-001-10` | CONFIRMED | VIOLATION | Remove `--all-features` from identity journey list/run commands. |
| `FIND-TASK-001-11` | REVISED | INCORRECT | Refuse keyless boot when any provider ciphertext exists. |
| `FIND-TASK-001-12` | REVISED | INCORRECT | Require a post-writer-roll zero-reference pass before retiring the old sealing key. |
| `FIND-TASK-001-13` | CONFIRMED | INCORRECT | Reject every present secret for `Public` and empty secrets for secret methods. |

The exact locations, reachability evidence, observable consequences,
decision-complete corrections, and focused closure proofs are authoritative in
`findings-validation.md`.

## Prior-finding closure

This is the first review of TASK-001. There are no prior stable findings or
remediation verdicts to close.

## Verification limits

- Reviewers performed static inspection of the immutable diff, current source,
  callers, migrations, task evidence, and committed tests. They did not rerun
  the long provider/Postgres/Cargo lanes within the review budget.
- The repository-standards reviewer independently ran
  `mise run check:from-pools-allowlist`, `mise run check:tenant-isolation`, and
  `git diff --check`; they passed, although the pool allowlist check does not
  enforce the raw-pool field/signature rule in `FIND-TASK-001-6`.
- The implementation record reports all task-prescribed focused journeys,
  migration, OpenAPI, SQL, codegen, docs, format, lint, and boundary lanes green.
- Every required reviewer and report completed within the 20-minute reviewer
  limit. No missing-reviewer gap applies.

## Remediation

Implement
`changes/active/oidc-production-readiness/review/TASK-001-r1/TASK-001-R1-production-readiness-gaps.md`
through `$wyrd-implement`, then reassess the complete original
base-to-remediated-candidate range.
