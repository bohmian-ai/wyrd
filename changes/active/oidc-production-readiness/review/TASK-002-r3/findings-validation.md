# TASK-002-r3 findings validation

## Immutable subject

- Repository: `/home/thorrester/Documents/GitHub/wyrd-oidc-complete`
- Base: `3fc085acf5b3a710d5dc80892bd2e664b3db6174`
- Candidate: `d861845f3f5d89aca413857dcfb8c1bbfaee349d`
- Approved specification: `changes/active/oidc-production-readiness/spec.md`, revision 4
- Original task: `changes/active/oidc-production-readiness/tasks/TASK-002-tenant-login.md`
- Prior remediation: `TASK-002-R1-tenant-login-corrections.md` and
  `TASK-002-R2-repository-rule-corrections.md`

The candidate was the checked-out `HEAD` during validation. The repository has
no `.codegraph/` index, so caller and source inspection used Git and `rg`.
Lead-authorized reuse cleanups recorded in the R2 evidence and the two unrelated
`test:wyrd` flaky-test repairs were treated as directed scope, not drift.

## Wave 1 finding dispositions

| Wave 1 report | Proposed finding | Disposition | Final finding | Validation |
|---|---|---|---|---|
| `task-review.md` | None | **VALIDATED EMPTY** | None | The requirement matrix covers the original task, all nine prior findings, non-goals, and the authorized follow-up work. Independent source inspection found no contradictory acceptance defect. |
| `standards-review.md` | `STD-R3-001` | **CONFIRMED** | `FIND-TASK-002-10` | The helper's implementation accepts an advertised HMAC header while its rustdoc says the helper requires and enforces an asymmetric advertised algorithm. This is a directly visible hard-rule violation. |
| `domain-review-security.md` | None | **VALIDATED EMPTY** | None | The composed callback remains fail-closed: the advertised-set check runs first and `ExternalVerifier::verify_external_against` then rejects every HMAC algorithm before `kid` lookup. No security correction is required. |
| `domain-review-tenancy-data.md` | None | **VALIDATED EMPTY** | None | The post-R2 changes do not alter tenant selection, RLS, transactions, durable state, or concurrency. No data-boundary finding is required. |

The empty task, security, and tenancy ledgers are independently retained as
empty. The standards finding does not contradict them: it is an inaccurate
source contract, not a reachable authentication bypass or tenancy defect.

## Independent validation and Ponytail correction boundary

`verify_id_token_algorithm` has one repository caller:
`AuthorizationCodeExchange::finish_id_token_exchange` in
`crates/wyrd/wyrd-auth/src/callback.rs`. The complete caller first invokes the
helper with fresh discovery metadata, immediately invokes
`ExternalVerifier::verify_external_against`, and only after both succeed checks
nonce and authorized party, revalidates the bound connection, resolves the
tenant User, replaces roles, audits, issues, seals, and commits the completion.
The helper itself decodes the header, parses every advertised algorithm, and
returns success for any parsed advertised algorithm equal to `header.alg`.
After commit `3747b2d26`, that includes `HS256`, `HS384`, and `HS512`.

The complete shared-verifier body at
`crates/shared/wyrd-auth-verify/src/lib.rs` rejects those three symmetric
algorithms immediately after decoding the header and before `kid` extraction or
JWKS lookup. Therefore the only production composition still requires an
advertised asymmetric algorithm and closes prior `FIND-TASK-002-4`; restoring a
duplicate HMAC filter would add no security. The helper's own summary and
`# Errors` contract nevertheless claim that *it* requires an asymmetric
advertised algorithm and rejects algorithms outside an advertised asymmetric
set. That claim is false for direct calls and violates the mandatory accurate
rustdoc rule for this materially modified fallible function.

Applying the required simplification ladder:

1. Deleting the finding would leave an explicit hard documentation rule
   violated.
2. Deleting or inlining the helper is not required by the task and would turn a
   documentation-only correction into executable churn.
3. Restoring the removed HMAC filter would duplicate the existing shared
   verifier and undo the authorized reuse cleanup.
4. No library, abstraction, configuration, or runtime test is needed.
5. The minimum safe correction is to edit only this helper's existing rustdoc:
   describe advertised-set membership, name malformed-header and
   not-advertised errors, and state that the immediately following shared
   verifier owns symmetric-algorithm rejection.

The surrounding `finish_id_token_exchange` documentation describes the
composed outcome rather than attributing the asymmetric rejection solely to
the helper, so it does not need a code or documentation change.

## Final deduplicated finding ledger

### FIND-TASK-002-10 — Algorithm-policy helper documents a filter it no longer performs

- **Wave 1 source:** `STD-R3-001`
- **Status:** CONFIRMED
- **Classification:** VIOLATION
- **Violated obligation:** `AGENTS.md` section 16,
  `architecture/agent-rules.md`, and
  `architecture/references/languages/rust-core.md` require accurate,
  substantive rustdoc for every materially modified Rust item and require a
  fallible function's `# Errors` section to name its real error conditions.
  Incomplete or false rustdoc is a hard blocker.
- **Exact location:**
  `crates/wyrd/wyrd-auth/src/callback.rs:568-585`.
- **Evidence:** the summary says `verify_id_token_algorithm` requires an
  asymmetric algorithm advertised for ID tokens, and `# Errors` promises
  `InvalidToken` outside the advertised asymmetric set. Its complete body now
  accepts a header whenever `Algorithm::from_str` parses an equal advertised
  value, including advertised `HS256`, `HS384`, and `HS512`. The sole caller's
  next operation, `ExternalVerifier::verify_external_against`, supplies the
  actual HMAC rejection at
  `crates/shared/wyrd-auth-verify/src/lib.rs:514-522`.
- **Reachability and task relevance:** every successful tenant callback reaches
  the helper through `AuthorizationCodeExchange::complete` and
  `finish_id_token_exchange`. The helper is therefore not dormant or test-only,
  and its contract explains a required REQ-007/INV-004 trust-boundary step.
- **Observable consequence:** runtime authentication remains safe, but a
  maintainer reading or reusing the public helper is told it enforces an
  asymmetric-policy boundary that its body does not enforce. The candidate
  therefore fails the repository's hard source-contract rule despite green
  runtime verification.
- **Decision-complete correction:** keep the implementation and shared verifier
  unchanged. Revise only the helper's summary, explanation, and `# Errors`
  text to say it checks whether the decoded header algorithm occurs in the
  provider-advertised set; name malformed-header and absent-from-advertised-set
  failures; and identify `ExternalVerifier::verify_external_against` as the
  subsequent owner of symmetric-algorithm rejection. Do not restore the
  duplicate filter, change visibility or callers, extract a helper, or add a
  dependency.
- **Focused closure proof:** inspect the corrected rustdoc against the unchanged
  complete helper and shared-verifier bodies, then run `mise run fmt`,
  `mise run lints`, and `git diff --check`. No runtime test is warranted because
  executable behavior must remain unchanged and existing algorithm-refusal
  coverage already proves the composed boundary.

## Prior-finding closure

| Prior finding | Independently validated closure | Result |
|---|---|---|
| `FIND-TASK-002-1` | Human-connection request validation and stored-row decode require exact `sub`; same-email/different-subject login remains distinct. | CLOSED |
| `FIND-TASK-002-2` | Tenant callback checks OIDC `azp` semantics before identity or issuance. | CLOSED |
| `FIND-TASK-002-3` | Changed roles append one canonical role-sync event in the issuance transaction; unchanged roles append none, and audit failure rolls back. The `principal_event` reuse preserves the event contract. | CLOSED |
| `FIND-TASK-002-4` | Fresh discovery constrains the header to the advertised set, and the immediately following shared verifier rejects HMAC before JWKS lookup. The false helper rustdoc is separated as new standards finding `FIND-TASK-002-10`; runtime closure remains intact. | CLOSED |
| `FIND-TASK-002-5` | Login-state transitions rely on forced RLS without duplicate tenant predicates, with cross-tenant transition proof. | CLOSED |
| `FIND-TASK-002-6` | State-owner resolution remains a narrow inherent `WyrdPostgres` operation over the private app pool. | CLOSED |
| `FIND-TASK-002-7` | `LoginState` stores the PKCE verifier as `SecretString`, and the debug regression proof remains. | CLOSED |
| `FIND-TASK-002-8` | The exact trait projections, SQL constants, signing helper panic behavior, and test alias cited in R2 now carry substantive rustdoc. | CLOSED |
| `FIND-TASK-002-9` | The SHA-256, Utoipa, and Wiremock imports cited in R2 now reside in their module import blocks under the required feature gate and alias. | CLOSED |

## Verification limits

- I inspected all four Wave 1 reports, the original task, applicable mandatory
  rustdoc authority, both prior validation ledgers, the post-R2 commit sequence,
  and the complete bodies and repository callers of the proposed finding's
  helper and correction boundary.
- I did not rerun Cargo, Postgres, IdP, or broad repository lanes. Wave 1 and R2
  record green focused identity proofs, all four required journeys, the full
  identity lane, principals unit/integration, SQL, tenant and pool boundary
  checks, codegen, docs, format, lints, and final `test:wyrd` with 2,213 passing
  tests. Those runtime results neither detect nor contradict the static false
  documentation.
- No reviewer, source, authority, or caller trace was unavailable. TASK-003's
  BFF completion route and TASK-004's CLI handoff remain intentional non-goals.

## Validation result and recommendation

**VALIDATED WITH FINDINGS.** The final ledger contains one bounded finding,
`FIND-TASK-002-10`. Correcting the existing rustdoc is sufficient; it requires
no product, public API, architecture, security, concurrency, resource-ownership,
or persistent-data decision. The appropriate task verdict is `FIX_REQUIRED`.
