# TASK-002 review verdict — R12

## Immutable subject

- Repository: `/home/thorrester/Documents/GitHub/wyrd-oidc-complete`
- Original task base: `3fc085acf5b3a710d5dc80892bd2e664b3db6174`
- Candidate: `2d57a6605da9bc1cee61140d7c09742cc4636efa`
- Latest remediation range: `bae424cc647dad4be80e7debec976d0b7b3e4cf8..2d57a6605da9bc1cee61140d7c09742cc4636efa`
- Authority: approved `changes/active/oidc-production-readiness/spec.md` revision 5, historical original `tasks/TASK-002-tenant-login.md`, R1–R11 verdicts and remediation, `AGENTS.md`, and `architecture/agent-rules.md`.

The candidate stayed unchanged through both review waves. `.codegraph/` is absent. Reviewers examined the full cumulative diff and used the latest range to locate R11 corrections. Only this new review directory was written.

## Verdict

**PASS — TASK-002 is accepted.** All four Wave 1 reviewers passed, the independent Ponytail validation returned an empty finding ledger, and `FIND-TASK-002-25` through `-27` are closed. Earlier `FIND-TASK-002-1` through `-24` remain closed. No new TASK-002 remediation task is needed.

## Acceptance matrix

| Obligation | Source and verification evidence | Result |
|---|---|---|
| REQ-006–008, INV-001/002/004, AC-002/003: state-bound tenant login, full ID-token validation, tenant User and roles | Auth login/callback, shared verifier, SQL login-state owner; recorded identity journeys including refusals and same-issuer tenant separation | PASS |
| REQ-013, AC-005: independent API-key and RFC 7523 workload assertion paths | Shared verifier applies assertion binding requirements separately from ID-token-only checks; recorded valid/missing-claim/future-`nbf` tests and machine journey | PASS |
| REQ-014–016, AC-006/007: provider replacement, no email linking, connection-bound renewal, current roles and serialized authority writes | Active revision checks and refresh-family locking in issuance, callback, refresh and revocation; recorded switch and concurrency journeys | PASS |
| REQ-005 rev5: sealed, single-use human credential handoff, missing-key refusal, rotation; machine independence | Connection activation, callback completion and login-state SQL; recorded keyless test, rotation and machine journeys | PASS |
| REQ-017 and `FIND-TASK-002-25`: audit the allowed decision before keyless activation refuses | `HumanConnections::activate` enters `begin_locked`, then commits the existing refusal; focused Postgres test finds exactly one redacted staged allowed row and no Active connection; recorded audit-failure journey | PASS |
| `FIND-TASK-002-26`: accurate resolver module documentation | `wyrd-auth/src/pg_resolvers.rs` header names its actual owner, `WyrdPostgres` and tenant RLS; latest subdiff changes no resolver behavior | PASS |
| R10 SQL boundary and `FIND-TASK-002-27`: one operator-backed slug resolver and scoped live audit entry points | `WyrdPostgres::resolve_tenant_slug` remains the SQL owner; both standalone audit forms and all production callers use `ValaPostgres` to acquire `TenantConn`; existing tenant-isolation check scans audit; recorded identity, principals, gateway and Bifrost lanes | PASS |
| Canonical audit and excluded scope | Existing `append_audit` and publisher remain sole path; gateway keeps its tracked nonblocking task. No email linking, platform fallback, provider-token bearer, new machine model, or early TASK-003/004 redemption | PASS |

## Independent review results

| Report | Result |
|---|---|
| `task-review.md` | PASS; no new task finding |
| `standards-review.md` | PASS for the cumulative TASK-002 boundary; no new repository-rule finding in this task |
| `domain-review-security.md` | PASS; no new security or OIDC finding |
| `domain-review-tenancy-data.md` | PASS; no new tenancy, data or concurrency finding |
| `findings-validation.md` | Empty validated ledger; all three R11 findings CLOSED |

## Verification and limits

The committed implementation record reports the exact keyless activation test and rotation journey, `test:identity:journey` (27/27), `test:principals:integration`, `test:gateway:native`, `test:bifrost:journey:server` (16/16), `check:tenant-isolation`, `check:from-pools-allowlist`, format and lints passing. Reviewers checked source and a fresh cumulative `git diff --check`; they did not rerun Cargo, Postgres, Docker or live-provider tests. TASK-003 browser redemption and TASK-004 CLI claim remain downstream.

**This verdict does not certify repository-wide compliance with the raw-pool ban.** Unchanged `ServerPostgres::app_pool()` and `vala_pool()` expose `&PgPool`; the live health probe uses `app_pool().acquire()`; the dormant eval resolver accepts `&PgPool`. These are not approved exceptions. They are outside the original TASK-002 and R11 remediation boundaries and require a separate scoped correction. The existing tenant-isolation check covers selected production boundaries, not every server module.
