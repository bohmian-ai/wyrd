# TASK-002 R5 Structured Ponytail Findings Validation

## Subject

- Repository: `/home/thorrester/Documents/GitHub/wyrd-oidc-complete`
- Base: `3fc085acf5b3a710d5dc80892bd2e664b3db6174`
- Candidate: `b57d43d501c136591125b98fe78352e657b093b6`
- Approved specification: `changes/active/oidc-production-readiness/spec.md`, revision 4
- Original task: `changes/active/oidc-production-readiness/tasks/TASK-002-tenant-login.md`
- Remediation authority: TASK-002 R1 through R4 and their validated ledgers
- Reviewed range: complete cumulative `base..candidate` diff and current candidate source

`HEAD` equaled the candidate before validation and immediately before this
report was written. The untracked R5 review directory is outside the immutable
subject. Lead-directed reuse and test commits identified in the evidence tables
were treated as authorized and still checked for regression.

## Review-input completeness

| Required input | Evidence | Result |
|---|---|---|
| Immutable base and candidate | Both commits resolve; `HEAD` is the candidate; cumulative changed-file and commit inventories were inspected | COMPLETE |
| Repository authority | `AGENTS.md`, `architecture/agent-rules.md`, `architecture/wyrd-design.md`, `architecture/wyrd-doctrine.mdx`, `architecture/wyrd-security-posture.md`, and the reference router plus applicable architecture, Rust, testing, error, and spec-driven references | COMPLETE |
| Approved behavior | `spec.md` revision 4 and original `TASK-002` | COMPLETE |
| Remediation history | R1 through R4 remediation tasks, validations, verdicts, and stable findings `FIND-TASK-002-1` through `FIND-TASK-002-12` | COMPLETE |
| Wave 1 reports | Task, standards, security, and tenancy/data reports are present, identify the immutable subject, state their limits, and each proposes an empty ledger with `PASS` | COMPLETE |
| Actual source and callers | Cumulative diff, R4 executable delta, production owners, route transaction owner, SQL helpers, verifier callers, tests, generated contracts, and relevant consumers were inspected | COMPLETE |

The repository has no `.codegraph/` index, so caller tracing used `rg`, Git,
and direct full-body source inspection. No required input or unresolved
authority conflict blocks validation.

## Validation method

For the empty Wave 1 union and every prior finding, validation applied the
structured Ponytail ladder: first ask whether the alleged correction can be
deleted, then whether an existing repository owner, standard-library/native
mechanism, or installed dependency already supplies it, and only then retain
the minimum bounded correction. Production callers were enumerated and the
complete relevant bodies were read before judging reachability.

The R4 ID-token correction was traced from both OIDC callers through
`ExternalVerifier::verify_id_token_against` and its shared generic verifier;
the workload path remains on `verify_external_against`. The refresh correction
was traced from the HTTP route through `RefreshTokens::execute`, family lock,
active/stale classification, human-session issuance and connection-slot lock,
canonical audit, and the route-owned success or `Reused` commit. The concurrent
ancestor-replay proof exercises the retained race directly. Earlier identity,
algorithm, role-audit, RLS, pool-ownership, secret-redaction, documentation,
and import findings were checked on current source and at their callers.

This inspection confirms the runtime correction but exposes one stale public
helper contract adjacent to it: `refresh_by_hash` still documents the retired
consume-then-lookup order even though the only production refresh caller now
looks up first to identify and lock the family. The helper also has an admin
revocation caller, so deleting it is not valid. No new abstraction, dependency,
or executable test is needed; correcting the existing rustdoc is the smallest
safe closure.

## Wave 1 disposition

| Wave 1 report | Proposed ledger | Disposition | Validation |
|---|---|---|---|
| `task-review.md` | Explicitly empty | **CONFIRMED as empty for task acceptance** | The requirement matrix and source/caller traces demonstrate no reachable `MISSING`, `INCORRECT`, `DRIFT`, `VIOLATION`, or `REGRESSION` in executable TASK-002 behavior. |
| `standards-review.md` | Explicitly empty | **EMPTY LEDGER REJECTED** | The report correctly validates ownership, tenancy, locking, and prior standards closure, but misses the stale public rustdoc at `refresh_tokens.rs:122-126`, retained below as `FIND-TASK-002-13`. |
| `domain-review-security.md` | Explicitly empty | **CONFIRMED as empty** | ID-token claim enforcement, exact identity, advertised algorithm and `azp`, screened provider IO, audit coupling, refresh containment, and plane separation remain closed on reachable paths. |
| `domain-review-tenancy-data.md` | Explicitly empty | **CONFIRMED as empty** | Forced RLS, state capability lookup, caller-owned transactions, family-before-connection lock order, replay containment, and migration behavior remain closed. |

Thus the Wave 1 union is not accepted as empty. One new repository-standards
finding is independently retained; no Wave 1 source ID exists for it.

## Prior-finding closure

| Prior finding | Independent current-source validation | Result |
|---|---|---|
| `FIND-TASK-002-1` — mutable human subject mapping | Authored and stored human connections require the exact `sub` claim; callback identity remains tenant-qualified `(issuer, sub)` and same-email identities do not merge. | CLOSED |
| `FIND-TASK-002-2` — missing OIDC `azp` semantics | The tenant callback invokes `verify_authorized_party` after shared verification and before identity persistence or issuance. | CLOSED |
| `FIND-TASK-002-3` — missing provider role-change audit | A changed mapped-role set appends the canonical role-sync event in the issuance transaction; an append failure rolls back roles and session state. | CLOSED |
| `FIND-TASK-002-4` — unpinned ID-token algorithm | The callback checks the fresh advertised algorithm set before the shared verifier rejects symmetric algorithms and verifies the selected JWKS key/signature. | CLOSED |
| `FIND-TASK-002-5` — duplicate tenant predicates | Login-state transitions rely on forced RLS; the cross-tenant operation remains only the least-disclosure state-owner lookup. | CLOSED |
| `FIND-TASK-002-6` — raw-pool state lookup | `WyrdPostgres::login_state_tenant` remains the narrow inherent owner; no raw pool is propagated through the callback or domain signature. | CLOSED |
| `FIND-TASK-002-7` — printable PKCE verifier | Public durable login state holds `SecretString`, the SQL decode wraps immediately, and redaction proof remains. | CLOSED |
| `FIND-TASK-002-8` — incomplete Rust documentation | The exact R2 item inventory retains substantive contract, error, panic, and invariant documentation. The new finding below concerns a distinct R4-staled helper contract and does not reopen this inventory. | CLOSED |
| `FIND-TASK-002-9` — function-scoped imports | The cited SHA-256, Utoipa, and Wiremock imports remain in module import blocks. | CLOSED |
| `FIND-TASK-002-10` — false algorithm-helper rustdoc | `verify_id_token_algorithm` accurately limits itself to advertised-set membership and identifies the shared verifier as the symmetric-algorithm owner. | CLOSED |
| `FIND-TASK-002-11` — optional ID-token binding/time claims | `verify_external_against` requires `exp`, `iss`, and `aud` and validates present `nbf`; `verify_id_token_against` additionally requires numeric, non-future `iat`. Tenant and platform OIDC callers use the ID-token entry while workload assertions retain the generic entry. | CLOSED |
| `FIND-TASK-002-12` — replay containment race | `RefreshTokens::execute` resolves the immutable family identity, takes the tenant-qualified transaction advisory lock before classification and the connection-slot lock, and holds it through successor issuance or revocation/audit and the route-owned commit. The deterministic ancestor-overlap test proves the committed successor is contained. | CLOSED |

## Final deduplicated finding ledger

### FIND-TASK-002-13 — Refresh lookup rustdoc describes the retired ordering

- **Wave 1 source IDs:** None; newly demonstrated during structured validation.
- **Status:** CONFIRMED.
- **Classification:** VIOLATION — mandatory Rust documentation accuracy.
- **Violated obligation:** `architecture/agent-rules.md` requires substantive
  rustdoc for every new or materially modified Rust item and marks missing or
  placeholder documentation `BLOCK_BEFORE_MERGE`; `AGENTS.md` requires the
  documented workflow role and invariants to remain accurate. R4 materially
  changed refresh classification order specifically to preserve replay
  containment.
- **Exact location:** `crates/wyrd/wyrd-sql/src/queries/auth/refresh_tokens.rs:122-126`.
- **Evidence:** The rustdoc says `refresh_by_hash` is used after
  `consume_active_refresh` returns `None`. The reachable production refresh
  caller does the opposite at `crates/wyrd/wyrd-auth/src/refresh.rs:121-128`:
  it calls `refresh_by_hash`, derives the stored principal family, takes
  `lock_refresh_family`, and only then calls `consume_active_refresh`. `rg`
  confirms the helper is also legitimately used by the refresh-revocation
  owner, so deletion is not available.
- **Observable consequence:** The public query helper documents the exact
  transaction ordering that R4 retired. A maintainer following it would bypass
  the family-before-classification invariant and can reintroduce the
  ancestor-replay/successor race closed by `FIND-TASK-002-12`.
- **Decision-complete correction:** Keep the existing helper and executable
  order unchanged. Replace only its stale use/order paragraph with the current
  contract: it looks up any lifecycle state; refresh rotation uses the returned
  immutable principal kind/id to acquire the existing family lock before
  active/stale classification, while revocation may use it for lookup. Do not
  add a helper, wrapper, lock, configuration, dependency, or runtime test.
- **Focused closure proof:** Directly inspect the corrected rustdoc beside the
  full `RefreshTokens::execute` body and both production helper callers; run
  `mise run fmt`, `mise run lints`, and `git diff --check`. No behavioral test
  is warranted because the retained defect and correction are documentation
  only and the existing deterministic Postgres test already proves the runtime
  ordering.

No other retained finding exists. `FIND-TASK-002-1` through
`FIND-TASK-002-12` remain closed; `FIND-TASK-002-13` is the next unused stable
ID.

## Recommendations and verification limits

- Remediate only `FIND-TASK-002-13` in the existing rustdoc. The runtime
  implementation is already the minimum safe correction: one installed
  PostgreSQL transaction advisory lock in the existing SQL owner and one
  ID-token semantic method in the existing verifier owner. No abstraction or
  dependency can be deleted without losing required behavior.
- This was a bounded static cumulative review. No costly Cargo, Postgres,
  Docker, Keycloak/Dex, or broad lane was rerun. Recorded candidate evidence
  includes the exact ID-token verifier proof, deterministic refresh overlap
  proof, four required identity journeys, full identity lane, principals,
  SQL, tenant-isolation, codegen/docs, format, and lint checks. Source and test
  bodies were inspected rather than treating those records as self-proving.
- `git diff --check` passed for the immutable range. TASK-003 BFF completion,
  TASK-004 CLI handoff persistence/local credential storage, and live
  Okta/Entra qualification remain explicit non-goals of TASK-002.
- The correction requires no product, public API, architecture, security,
  compatibility, concurrency, resource-ownership, or persistent-data decision;
  `SPEC_REVISION_REQUIRED` does not apply.

## Overall validation result

**VALIDATED WITH FINDING — FIX_REQUIRED.** The executable TASK-002 behavior and
all prior findings pass independent validation, but the Wave 1 empty union is
overturned by bounded `FIND-TASK-002-13`. Correct the one stale rustdoc contract
before advancing the task.
