# TASK-002 R6 Structured Ponytail Findings Validation

## Immutable subject

- Repository: `/home/thorrester/Documents/GitHub/wyrd-oidc-complete`
- Base: `3fc085acf5b3a710d5dc80892bd2e664b3db6174`
- Candidate: `0ca117a744ddcb7414b104c4382027970531b608`
- Approved specification: `changes/active/oidc-production-readiness/spec.md`, revision 4
- Original task: `changes/active/oidc-production-readiness/tasks/TASK-002-tenant-login.md`
- Remediation authority: TASK-002 R1 through R5 and stable findings
  `FIND-TASK-002-1` through `FIND-TASK-002-13`

`HEAD` equaled the candidate before source inspection, after diff validation,
and immediately before this report was written. The untracked R6 review
directory is outside the immutable subject. Lead-directed reuse and test
commits recorded in the evidence tables were treated as authorized and not as
scope drift.

## Review-input completeness and method

The approved specification, original task, all five remediation tasks, prior
verdicts and validated ledgers, repository rules, applicable security, Rust,
SQL-tenancy, testing, and spec-driven authorities, the complete cumulative
changed-file inventory, current relevant source and tests, and all four R6
Wave 1 reports were inspected. The repository has no usable CodeGraph index:
`codegraph explore` explicitly reported that no index is available, so caller
tracing used `rg`, Git, and direct full-body inspection.

The Wave 1 union contains one proposed finding, `TD-R6-001`. Validation traced
every production caller of `revoke_refresh_family`, `lock_refresh_family`, and
`revoke_principal_in_conn`, plus the complete refresh rotation, issuance,
server route, SQL update, transaction-commit, and existing concurrency-test
bodies. It then applied the Ponytail ladder: deletion would violate the
existing principal-revocation contract; the repository already has the exact
tenant-qualified transaction advisory lock needed; no new table, service,
trait, lock type, isolation mode, dependency, or public contract is justified.

## Wave 1 disposition

| Wave 1 report | Proposed ledger | Disposition | Independent validation |
|---|---|---|---|
| `task-review.md` | Explicitly empty | **CONFIRMED as empty for its inspected task matrix** | The current login, callback, OIDC verification, identity, role, connection-cutoff, replay-containment, public-contract, and journey paths satisfy the mapped obligations. Its refresh conclusion did not enumerate the separate production administrative family-revocation caller. |
| `standards-review.md` | Explicitly empty | **CONFIRMED as empty for repository-rule findings** | No independent ownership, API, generated-artifact, import, documentation, or boundary violation was retained. The concurrency defect below is a task/durability behavior gap, not a separate standards-only finding. |
| `domain-review-security.md` | Explicitly empty | **CONFIRMED as empty** | Exact identity, ID-token claims and algorithm policy, screened IO, redaction, audit coupling, authority-plane separation, and replay containment remain closed on the inspected security paths. |
| `domain-review-tenancy-data.md` | `TD-R6-001` | **CONFIRMED and retained as `FIND-TASK-002-14`** | Administrative User revocation and refresh rotation mutate the same family concurrently, but only rotation takes the family lock. The reachable interleaving can leave the newly inserted successor outside the revocation statement snapshot. |

## Validation of `TD-R6-001`

### Reachability and caller trace

`revoke_refresh_family` has two production callers. The replay branch in
`RefreshTokens::execute` first resolves the stored family, calls
`lock_refresh_family`, classifies under that lock, and keeps it through family
revocation/audit and the route-owned `Reused` commit. The User branch of
`revoke_principal_in_conn` instead reads the User, suspends it, and calls
`revoke_refresh_family` without the lock. That branch is reached by the live
`POST /v1/principals/{id}/revoke` server route, which commits the suspension,
family update, and authorization audit together.

The competing rotation holds the family advisory lock, consumes current row
`B`, takes the connection-slot lock, re-reads the User as Active through
`TenantTokenIssuer::issue`, and inserts successor `C` before its caller
commits. Because the administrative path does not participate in the family
lock, it can suspend the User and begin its family-wide `UPDATE` while `C` is
uncommitted. The update waits on `B`; after rotation commits, PostgreSQL can
recheck that changed target row, but the new `C` was absent from the update's
statement snapshot and is not a target of that scan. Administrative revocation
can therefore commit with `C.revoked_at IS NULL`.

This is not speculative or a dormant helper: both callers are production
routes, OIDC refresh rows are created by the task-owned human-session path,
and the administrative owner expressly promises that User suspension and
refresh-family retirement commit together. It is also distinct from
`FIND-TASK-002-12`: the replay and rotation paths now serialize correctly;
the missed seam is the other production family-wide mutation that R4's new
shared lock made responsible for participating in the same invariant.

### Ponytail correction boundary

The minimum safe correction is one call to the existing
`lock_refresh_family(conn, "user", id_uuid)` in the existing User-revocation
owner, after the not-found check and before suspension or family revocation.
The route already owns the transaction and commit, so that call holds the lock
through both durable effects. This also preserves the established lock order:
family first, then connection for rotation; administrative revocation takes no
connection lock. Moving the lock into a new service or adding a table, retry,
mutex, isolation-level change, or dependency would add machinery without
closing a different requirement.

One deterministic Postgres overlap test is sufficient. Reuse the existing
refresh service, fixture, family lock, and principal-revocation function: hold
a legitimate rotation open after successor insertion, start administrative
User revocation, observe it wait, commit both route-equivalent transactions,
then read from a fresh transaction and prove the successor carries
`principal_revoked` and no active row from that family survived. No new test
harness or public API is warranted.

## Prior-finding closure

| Prior finding | Independent current-candidate result |
|---|---|
| `FIND-TASK-002-1` — exact human `sub` identity | **CLOSED.** Human connection input and stored decode require `sub`; tenant identity remains exact `(issuer, subject)` and email is display-only. |
| `FIND-TASK-002-2` — OIDC authorized party | **CLOSED.** Tenant callback verifies present and multi-audience `azp` before persistence. |
| `FIND-TASK-002-3` — provider role-change audit | **CLOSED.** Changed mapped roles append canonical evidence in the issuance transaction; audit failure prevents commit. |
| `FIND-TASK-002-4` — advertised algorithm | **CLOSED.** Fresh advertised membership precedes shared asymmetric signature/key verification and HMAC rejection. |
| `FIND-TASK-002-5` — duplicate tenant selection | **CLOSED.** Tenant login-state transitions rely on forced RLS; only the least-disclosure owner lookup crosses it. |
| `FIND-TASK-002-6` — raw pool propagation | **CLOSED.** State-owner lookup remains the narrow inherent `WyrdPostgres` capability. |
| `FIND-TASK-002-7` — printable PKCE verifier | **CLOSED.** Durable PKCE state remains secret-backed and redacted. |
| `FIND-TASK-002-8` — incomplete Rust documentation | **CLOSED.** The cited R2 inventory retains substantive item, error, panic, and invariant documentation. |
| `FIND-TASK-002-9` — function-scoped imports | **CLOSED.** The cited imports remain in module import blocks. |
| `FIND-TASK-002-10` — false algorithm-helper contract | **CLOSED.** The helper documents advertised membership only and names the shared verifier as the asymmetric-policy owner. |
| `FIND-TASK-002-11` — optional binding/time claims | **CLOSED.** Tenant and platform ID-token callers require the configured binding and time claims; workload assertions retain the generic contract. |
| `FIND-TASK-002-12` — replay/rotation race | **CLOSED.** Replay and rotation share the family lock before classification, and the deterministic overlap proof establishes successor containment. The new finding covers a different production mutation caller. |
| `FIND-TASK-002-13` — stale lookup rustdoc | **CLOSED.** Candidate `0ca117a74` accurately documents lookup, family lock, classification, and test-only lifecycle observation. |

## Final deduplicated finding ledger

### FIND-TASK-002-14 — Administrative User revocation can miss a concurrent refresh successor

- **Wave 1 source ID:** `TD-R6-001`
- **Status:** CONFIRMED
- **Classification:** INCORRECT
- **Violated obligation:** TASK-002's tenant-transaction renewal contract and
  R4's family-serialization correction require a family-wide mutation not to
  miss a concurrently inserted successor. The existing administrative User
  revocation contract also requires suspension and family retirement to commit
  together.
- **Exact location:**
  `crates/wyrd/wyrd-auth/src/revoke.rs:43-56`; competing rotation at
  `crates/wyrd/wyrd-auth/src/refresh.rs:121-218`; family update at
  `crates/wyrd/wyrd-sql/src/queries/auth/refresh_tokens.rs:180-200`; route
  transaction owner at `crates/wyrd/wyrd-server/src/auth/revoke.rs:107-123`.
- **Evidence:** The replay caller of `revoke_refresh_family` takes
  `lock_refresh_family`; the administrative User caller does not. A rotation
  can consume `B` and insert uncommitted `C` while revocation's family update
  takes a snapshot without `C`, waits on `B`, and then commits without ever
  targeting `C`.
- **Observable consequence:** `POST /v1/principals/{id}/revoke` can report and
  commit successful User suspension and family retirement while durable state
  still contains an unrevoked successor refresh credential, contradicting the
  operation's promised retirement of renewable authority.
- **Decision-complete correction:** Reuse
  `lock_refresh_family(conn, "user", id_uuid)` in the User branch of
  `revoke_principal_in_conn`, after confirming the User exists and before
  suspension and `revoke_refresh_family`. Keep the existing `TenantConn`,
  route-owned commit, SQL helper, family-before-connection order, and all
  service/agent behavior. Do not add another lock abstraction, persistence
  object, retry protocol, isolation mode, or public API.
- **Focused closure proof:** Add one deterministic Postgres test using the
  existing fixture and production owners that overlaps an open current-token
  rotation with administrative User revocation, proves revocation waits, then
  commits both and verifies from a fresh transaction that the inserted
  successor is revoked with `principal_revoked` and no active family row
  remains. Run its exact `mise exec -- cargo nextest run` selector with the
  repository Postgres wrapper, then the narrow auth/SQL lane, `mise run fmt`,
  `mise run lints`, and `git diff --check`.
- **Specification decision:** No revision is required. The existing lock and
  transaction owners already express the required semantics; this is one
  omitted caller and one missing overlap proof.

No other finding is retained.

## Verification limits

- Static validation covered the cumulative diff, all R1-R5 authority and
  finding history, current relevant full source bodies, every production
  caller named above, and the existing sequential revocation and deterministic
  replay/rotation overlap tests.
- `git diff --check
  3fc085acf5b3a710d5dc80892bd2e664b3db6174..0ca117a744ddcb7414b104c4382027970531b608`
  passed, and `HEAD` remained the immutable candidate.
- This bounded Wave 2 review did not rerun Cargo, Postgres, Docker,
  Keycloak/Dex, migration, lint, or broad identity lanes. Recorded evidence
  covers the prior focused verifier and replay-overlap proofs, all four tenant
  identity journeys, the full identity lane, principals, SQL, tenant
  isolation, codegen/docs, format, and lints. No recorded or current test
  overlaps administrative User revocation with refresh rotation.
- TASK-003 BFF completion, TASK-004 CLI persistence, and live Okta/Entra
  qualification remain intentional downstream or change-level work.

## Overall validation recommendation

**VALIDATED WITH FINDING — FIX_REQUIRED.** Retain
`FIND-TASK-002-14`. It is a bounded cross-caller omission closed by the
existing family lock in the existing revocation owner plus one focused
Postgres overlap proof; no specification revision is required.
