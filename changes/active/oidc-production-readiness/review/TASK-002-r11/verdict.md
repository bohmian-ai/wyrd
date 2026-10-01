# TASK-002 Review Verdict — R11

## Immutable subject

- Repository: `/home/thorrester/Documents/GitHub/wyrd-oidc-complete`
- Original task base: `3fc085acf5b3a710d5dc80892bd2e664b3db6174`
- Candidate: `bae424cc647dad4be80e7debec976d0b7b3e4cf8`
- Latest remediation range: `454bdb90ef8eedca4bbd4754e083d41f15d0c4ae..bae424cc647dad4be80e7debec976d0b7b3e4cf8`
- Authority: approved `changes/active/oidc-production-readiness/spec.md` revision 5, historical original `tasks/TASK-002-tenant-login.md`, prior R1–R10 verdicts and remediation packets, and the R10 human-directed packet.

The candidate commit stayed unchanged across both review waves. Only new R11
review artifacts were written. `.codegraph/` is absent, so reviewers used the
repository source and Git diff directly.

## Verdict

**FIX_REQUIRED.** The tenant login, JWT, slug resolution, lock ordering, and
sealed-handoff corrections have credible source and recorded test evidence.
Three bounded gaps remain: an allowed activation decision is lost on a
missing-key refusal, a changed resolver module has stale ownership rustdoc,
and a live server audit boundary still accepts raw SQL pools. The latter
violates the active no-exceptions instruction even though its signatures
predate the latest fix. None requires a specification revision.

## Acceptance matrix

| Obligation | Implementation and verification evidence | Result |
|---|---|---|
| REQ-006–008, INV-001/002/004: state-bound tenant login, full ID-token checks, `(issuer, sub)` User and tenant role grants | Login/callback/verifier owners; recorded provider-backed login and refusal journeys | PASS |
| REQ-013, AC-005: API-key and workload paths remain independent; RFC 7523 binding claims are enforced | Shared verifier and exact workload binding; recorded valid/missing-claim/future-`nbf` proofs and machine journey | PASS |
| REQ-014–016, AC-003/006/007: provider replacement, no email linking, connection-bound renewal, current roles and serialized User authority writes | Active revision checks, refresh-family locks, revocation lock move; recorded switch and concurrency proofs | PASS |
| REQ-005 rev5: keyless human activation/login refusal, sealed single-use completion, expiry and retained-key rotation | `HumanConnections`, callback, login-state SQL; recorded focused keyless, redemption and rotation tests | PASS for credential safety; audit gap below |
| R10 human-directed SQL boundary: one slug resolver, operator lookup, RLS tenant work and no raw pool in changed auth/boot/query paths | `WyrdPostgres::resolve_tenant_slug`; recorded boundary checks, focused slug tests and identity journey | PASS for those paths |
| REQ-017: every evaluated activation decision is committed through canonical audit | New keyless guard returns before `begin_locked` appends an allowed decision | **FAIL — FIND-TASK-002-25** |
| `AGENTS.md` §16: materially modified Rust module documentation is accurate | `pg_resolvers.rs` still documents a different crate and stored `PgPool` | **FAIL — FIND-TASK-002-26** |
| Active no-raw-pool instruction and `architecture/agent-rules.md`: live production audit uses scoped SQL capability | Touched server audit module still exposes `record_audit(&PgPool)` and owned `PgPool` to live identity/shared-denial callers | **FAIL — FIND-TASK-002-27** |
| R10 findings 23/24 and prior findings 1–22 | Source-checked corrected token-test and auth-routes rustdoc; earlier behavior owners remain closed | PASS |
| Exclusions and downstream boundaries | No email linking, platform fallback, provider-token bearer, second machine model, browser BFF or CLI claim implementation | PASS |

## Independent review results

| Reviewer | Result | Validated disposition |
|---|---|---|
| Task implementation | FAIL | Activation audit proposal revised into FIND-TASK-002-25 |
| Repository standards | FAIL | Resolver rustdoc confirmed as FIND-TASK-002-26; raw audit pool revised as FIND-TASK-002-27 |
| Security/OIDC domain | FAIL | Same activation audit defect, deduplicated with task finding |
| Tenancy/data/concurrency domain | PASS | No additional finding |
| Structured Ponytail validation | FIX_REQUIRED | Three retained findings; no new architecture or duplicate mechanism |

## Validated findings and required closure

| Finding | Status | Correction boundary | Focused proof |
|---|---|---|---|
| `FIND-TASK-002-25` | REVISED | Enter the existing activation transaction and canonical audit append before checking for the sealing key; commit the existing refusal without promoting a connection. | Keyless activation test asserts exactly one redacted allowed decision and no promotion; existing injected activation-audit failure journey remains green. |
| `FIND-TASK-002-26` | CONFIRMED | Correct only the resolver module header to describe `wyrd-auth`, `WyrdPostgres` and tenant RLS; remove stale `PgPool` and ephemeral commit wording. | Inspect header against both resolver bodies; documentation-only subdiff, format and lint. |
| `FIND-TASK-002-27` | REVISED | Route both standalone audit forms and every caller through the existing `ValaPostgres` tenant-connection owner and canonical append; extend the existing tenant-isolation check. | Boundary check rejects a raw-pool audit signature; identity refusal and gateway/query audit behavior remain green. |

The complete diagnoses, caller traces, and smallest corrections are in
`findings-validation.md`. The R10 human-directed additions remain human
direction, not retroactive reviewer findings.

## Prior-finding closure and verification limits

`FIND-TASK-002-1` through `FIND-TASK-002-24` remain closed by source evidence.
The implementation record reports `mise run test:identity:journey` (27/27),
focused Rust tests, format, lints, and both SQL boundary checks passing. This
review ran a fresh cumulative `git diff --check` and inspected the committed
source; it did not rerun Cargo, Postgres, Docker, or live provider suites.
TASK-003 browser redemption, TASK-004 CLI claim, and live-provider
qualification remain downstream and are not counted as TASK-002 proof.

The unchanged, currently uncalled eval resolver also has a raw-pool
signature. It is a separate existing violation, not an approved exception;
this acceptance audit gives it no TASK-002 finding or remediation scope.

## Remediation

Implement `TASK-002-R11-audit-and-resolver-boundaries.md` in this directory
through `$wyrd-implement`, then review the complete original-base-to-new-
candidate range. Do not review only the R11 fix diff.
