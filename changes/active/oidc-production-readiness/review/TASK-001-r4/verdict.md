# TASK-001 r4 verdict

## Immutable subject

- Base: `a5c8041a348a66bfb56fbac492383e8c688b0590`
- Candidate: `c2787b37d456cd2eeeee01e04a9f8bbfdf8866e5`
- Candidate tree: `e389f56318a664920b5ebead0aee45d20fda6b23`
- Approved specification: `changes/active/oidc-production-readiness/spec.md`, revision 4
- Original task: `changes/active/oidc-production-readiness/tasks/TASK-001-tenant-connections.md`
- Prior remediation tasks: `TASK-001-R1-production-readiness-gaps.md`,
  `TASK-001-R2-remaining-production-readiness-gaps.md`, and
  `TASK-001-R3-production-readiness-gaps.md`
- Review: `changes/active/oidc-production-readiness/review/TASK-001-r4/`

The candidate commit and tree matched the supplied immutable subject before
Wave 1, after Wave 1, and after Wave 2. Review artifacts were the only files
written during the audit.

## Acceptance matrix

| Obligation | Result | Evidence |
|---|---|---|
| OIDC remains optional; connectionless boot and machine authentication remain available | PASS | Optional durable connection owner, keyless ciphertext inventory, and committed connectionless proof. |
| One Active and at most one Candidate per tenant; tenant isolation and concurrent lifecycle serialization | PASS | Forced RLS, partial unique indexes, `TenantConn`, tenant slot lock, and two-tenant/concurrent journey source. |
| Authorized, redacted, typed headless connection lifecycle API | PASS | Six typed routes, bearer-derived tenancy, authorization-before-decode, durable owner, and served-contract proof. |
| Exact callback, supported client authentication, issuer/discovery/JWKS qualification | PASS | Deployment-owned callback, exact callback/client-auth probes, typed unsupported-auth refusal, and focused tests. |
| Provider IO is TLS-controlled, DNS-bounded, pinned, proxy-free, redirect-free, response-bounded, and blocks cloud metadata in every profile | FAIL | The common controls are present, but `AllowInternal` admits Alibaba `100.100.100.200` and AWS IMDS IPv6 `fd00:ec2::254` (`FIND-TASK-001-24`). |
| Provider secrets are authorized, sealed, redacted, rotation-safe, and loaded from restrictive bounded files | PASS | Typed secret boundary, encrypted stores, cross-store CAS rewrap, post-writer retirement proof, and shared secret-file loader. |
| Replacement/deactivation/removal stop old login and renewal across replicas | PASS | Exact connection provenance, shared locked rechecks, tombstones, and unbound legacy refresh refusal. |
| Tenant SQL, RLS, migration, audit transactionality, and recovery authority remain correct | PASS | Sanctioned connection capabilities, migration preflight, two attributed activation decisions, and rollback proof. |
| Public contracts, stable errors, OpenAPI, generated artifacts, docs, and journeys agree | PASS WITH LIMIT | Source and recorded lanes align; Cargo-backed and long integration lanes were not rerun during this review. |
| New and materially changed Rust follows mandatory module-import and bare-signature rules | FAIL | Two R3-added signatures retain qualified type paths (`FIND-TASK-001-25`). |
| Non-goals remain excluded | PASS | No UI, hosted signup, commercial hook, second trust store, compatibility route, new auth method, or new configuration surface entered the range. |

## Wave results

| Report | Result | Proposed findings |
|---|---|---|
| `task-review.md` | PASS | None |
| `standards-review.md` | FAIL | `STD-R4-001` |
| `domain-review-security.md` | FAIL | `SEC-R4-001` |
| `domain-review-tenancy-data.md` | PASS | None |
| `domain-review-secrets.md` | PASS | None |
| `findings-validation.md` | FIX_REQUIRED | `FIND-TASK-001-24`, `FIND-TASK-001-25` |

Wave 2 independently traced every cited caller and complete correction owner,
confirmed both proposals, and found no additional material issue in the three
empty Wave 1 ledgers.

## Validated finding ledger

| ID | Status | Classification | Required outcome |
|---|---|---|---|
| `FIND-TASK-001-24` | CONFIRMED | VIOLATION | Always reject the exact Alibaba IPv4 and AWS IMDS IPv6 metadata endpoints in the existing shared OIDC classifier while preserving ordinary CGNAT/ULA access under `AllowInternal`. |
| `FIND-TASK-001-25` | CONFIRMED | VIOLATION | Import the two qualified signature types into their modules and use bare names, with no behavioral or ownership change. |

Exact reachability evidence, consequences, correction boundaries, and focused
closure proofs are authoritative in `findings-validation.md`.

## Prior-finding closure

`FIND-TASK-001-1` through `FIND-TASK-001-23` remain closed at their validated
correction boundaries. `FIND-TASK-001-24` is a distinct always-blocked address
classification gap, not a reopening of the prior provider scheme, pinning,
proxy, response-bound, or DNS-deadline corrections. `FIND-TASK-001-25` covers
two new R3-added signature sites and does not reopen the earlier import fixes.
The previously rejected host-selected callback-tenant proposal remains outside
TASK-001 and is not revived.

## Verification limits

- This was a time-bounded static audit. No Cargo-backed, Postgres, provider,
  OpenAPI, codegen, docs, format, lint, or broad journey lane was rerun.
- The committed tests and all implementation evidence recorded by TASK-001 and
  R1/R2/R3 were inspected but not independently reproduced.
- No live cloud metadata endpoint, commercial IdP, stalled system resolver,
  proxy environment, or deployed multi-pod topology was exercised.
- CodeGraph was unavailable because the repository has no `.codegraph/`
  directory; reviewers used `rg`, the cumulative diff, and complete source
  bodies for caller tracing.
- Every required Wave 1 reviewer and the Wave 2 validator completed within the
  20-minute reviewer limit. No missing-reviewer verification gap applies.

## Verdict

**FIX_REQUIRED**

Implement
`changes/active/oidc-production-readiness/review/TASK-001-r4/TASK-001-R4-production-readiness-gaps.md`
through `$wyrd-implement`, then review the complete original
base-to-remediated-candidate range again.
