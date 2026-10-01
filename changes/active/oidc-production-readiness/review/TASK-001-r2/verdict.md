# TASK-001 r2 verdict

## Immutable subject

- Base: `a5c8041a348a66bfb56fbac492383e8c688b0590`
- Candidate: `bb4895d8e630ee8fb2ba075d6c3eeaa348e49414`
- Candidate tree: `4c2e2b73f4dac882f02dd08290bb6ec102bce075`
- Approved specification: `changes/active/oidc-production-readiness/spec.md`, revision 4
- Original task: `changes/active/oidc-production-readiness/tasks/TASK-001-tenant-connections.md`
- Prior remediation: `changes/active/oidc-production-readiness/review/TASK-001-r1/TASK-001-R1-production-readiness-gaps.md`

The candidate commit and tree matched the supplied immutable subject before
Wave 1, after Wave 1, and after Wave 2. Review artifacts were the only files
written during the audit.

## Acceptance matrix

| Obligation | Result | Evidence |
|---|---|---|
| OIDC remains optional; keyless boot refuses stored provider ciphertext | PASS | Optional owner plus cross-store boot inventory and focused Postgres proof. |
| One Active and at most one Candidate per tenant; tenant isolation | PASS | RLS, partial unique indexes, tenant slot lock, and tenant-isolation evidence. |
| Authorized redacted headless lifecycle API | FAIL | Lifecycle exists, but the candidate PUT route retains a dynamic JSON boundary (`FIND-TASK-001-15`). |
| Exact callback and client-auth qualification | FAIL | Client-auth qualification closes; mixed success/error callback responses still qualify (`FIND-TASK-001-1`). |
| Mandatory TLS, screened provider IO, and bounded responses | FAIL | Proxy pinning closes, but discovered cleartext endpoints and unbounded decoded bodies remain (`FIND-TASK-001-18`, `FIND-TASK-001-19`). |
| Secret validation, redaction, storage, and safe sealing-key rotation | FAIL | Validation, redaction, sealing, and keyless boot close; file permissions and the post-roll journey remain incomplete (`FIND-TASK-001-12`, `FIND-TASK-001-17`). |
| Replacement/deactivation/removal stop old login and renewal | FAIL | New sessions are correctly bound; migration fabricates provenance for legacy refresh rows (`FIND-TASK-001-5`). |
| Tenant SQL capability boundary | FAIL | Remediated connection owners close, but the materially changed issuer resolver still owns a raw pool (`FIND-TASK-001-14`). |
| Canonical transactional audit and second candidate-test decision | PASS | Both decisions are real, recorded at their boundaries, and stamp failure remains fail closed. |
| Repository Rust documentation and import rules | FAIL | Resolver rustdoc remains incomplete and the proxy test has function-local imports (`FIND-TASK-001-8`, `FIND-TASK-001-16`). |
| Migration preserves workload bindings and rejects ambiguous/unsafe Human trust | PASS WITH GAP | Existing preflight obligations pass; legacy refresh provenance fails separately under `FIND-TASK-001-5`. |
| Served OpenAPI, generated errors, docs, and default-feature journey selection | PASS WITH LIMIT | Source and recorded lanes align; long lanes were not rerun during this time-bounded review. |
| Non-goals remain excluded | PASS | No UI, hosted signup, commercial hook, second trust store, compatibility route, new auth method, rotation coordinator, or proxy configuration entered the range. |

## Wave results

| Report | Result | Proposed findings |
|---|---|---|
| `task-review.md` | FAIL | `TASK-REV-R2-001` |
| `standards-review.md` | FAIL | `STD-R2-001` through `STD-R2-005` |
| `domain-review-security.md` | FAIL | `SEC-R2-001` through `SEC-R2-003` |
| `domain-review-tenancy-data.md` | FAIL | `TD-R2-001` |
| `domain-review-secrets.md` | FAIL | `SECRET-R2-001` |
| `findings-validation.md` | FIX_REQUIRED | Four retained prior IDs and six new IDs |

Wave 2 rejected `TASK-REV-R2-001`: the exact pool-allowlist command exits 0.
Its missing-directory diagnostic is stale check noise, not a red gate. Every
other Wave 1 proposal was confirmed or revised; overlaps and incomplete prior
closures were deduplicated under stable IDs.

## Validated finding ledger

| ID | Status | Classification | Required outcome |
|---|---|---|---|
| `FIND-TASK-001-1` | REVISED | INCORRECT | Require exactly one valid callback response arm; reject mixed success/error responses. |
| `FIND-TASK-001-5` | REVISED | INCORRECT | Leave provenance-free legacy human refresh rows unbound so they cannot renew under a replacement provider. |
| `FIND-TASK-001-8` | REVISED | VIOLATION | Complete required rustdoc for the materially changed issuer-resolver surface. |
| `FIND-TASK-001-12` | REVISED | INCORRECT | Remove every K1 writer before the journey's final post-roll verification pass. |
| `FIND-TASK-001-14` | CONFIRMED | VIOLATION | Replace the issuer resolver's raw pool with the existing `WyrdPostgres` owner. |
| `FIND-TASK-001-15` | REVISED | VIOLATION | Use a typed contract decoder over an authorized bounded raw body; remove `Json<Value>` from the handler. |
| `FIND-TASK-001-16` | CONFIRMED | VIOLATION | Move proxy-test dependencies to the test module import block. |
| `FIND-TASK-001-17` | REVISED | VIOLATION | Route active and retained sealing-key files through the existing restrictive secret-file loader. |
| `FIND-TASK-001-18` | REVISED | VIOLATION | Refuse cleartext provider endpoints under production screening while retaining the existing local test policy. |
| `FIND-TASK-001-19` | REVISED | VIOLATION | Apply one shared 1 MiB decoded-response ceiling to discovery, JWKS, probe, and token JSON. |

Exact reachability evidence, caller tracing, consequences, correction
boundaries, and focused closure proofs are authoritative in
`findings-validation.md`.

## Prior-finding closure

- Closed: `FIND-TASK-001-2`, `-3`, `-4`, `-6`, `-7`, `-9`, `-10`, `-11`, and `-13`.
- Still open and revised: `FIND-TASK-001-1`, `-5`, `-8`, and `-12`.
- New validated findings: `FIND-TASK-001-14` through `-19`.

## Verification limits

- Reviewers inspected the complete cumulative diff, governing authorities,
  changed source, callers, migrations, tests, documentation, and recorded
  implementation evidence.
- Focused Wave 1 execution covered callback/client-auth probes, secret input
  validation, keyring rotation, keyless boot, tenant isolation, unwrap and
  Clippy-allow audits, client-tier boundaries, pool construction boundaries,
  legacy-vocabulary checks, and `git diff --check`.
- The orchestrator independently reproduced `mise run
  check:from-pools-allowlist` as exit 0 with the known stale `python/`
  diagnostic.
- Long Postgres/Keycloak journeys and broad format, lint, codegen, and docs
  lanes were not rerun within the review budget. Their committed source and
  recorded results were inspected, but they do not exercise the retained gaps.
- Every required reviewer completed. No reviewer-timeout or missing-report gap
  applies.

## Verdict

**FIX_REQUIRED**

Implement
`changes/active/oidc-production-readiness/review/TASK-001-r2/TASK-001-R2-remaining-production-readiness-gaps.md`
through `$wyrd-implement`, then review the complete original
base-to-remediated-candidate range again.
