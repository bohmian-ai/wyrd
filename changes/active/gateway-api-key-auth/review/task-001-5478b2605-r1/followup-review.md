# Focused revocation follow-up

Result: **RESOLVED**. Both disputed paths are distinguishable from source. This is discovery evidence for independent validation, not a task verdict.

## Subject and scope

Base: `e0a926031155f263a8721c47d4662dfed8c125ea`. Candidate and inspected HEAD: `5478b2605acae3cecd2522941c30ee7f1674f0be`.

Read the approved r1 specification, original TASK-001 including its scenario and evidence sections, cumulative base-to-candidate diff across all 15 changed files, applicable AGENTS/agent rules, reference router, spec-driven-development and maintainer guidance, design identity/delegation sections, doctrine, security posture and security foundation. Read the behavior, invariant, security, concurrency and system reports to isolate their conflicting claims. CodeGraph was attempted first; its broad path query returned unrelated matches, so precise source reads supplied the missing owners.

Only investigated (1) what J1 actually revokes, (2) whether a successful exchange can reach cache admission after credential retirement, and (3) which authority determines the deadline. No production or test changes and no Cargo/runtime verification were performed.

## 1. Principal suspension is not credential retirement

Exact inspected path:

- `crates/wyrd/wyrd-testing/tests/gateway/native.rs:765-802`: J1 warms the key and posts to `/v1/principals/{revoked_id}/revoke` with `principal_kind: service`. It asserts the principal was revoked, then checks exchange failure. The late +50/+61-second checks are at `native.rs:1025-1059`.
- `crates/wyrd/wyrd-server/src/auth/revoke.rs:1-23,72-119`: the route explicitly suspends a principal and calls `revoke_principal_in_conn` before committing.
- `crates/wyrd/wyrd-auth/src/revoke.rs:40-104`: the machine branch resolves the service principal and calls `suspend_service_account_principal`; it does not call credential retirement.
- `crates/wyrd/wyrd-sql/src/queries/auth/revocation.rs:1-7,47-68`: that mutation updates `auth_service_accounts.status` to `suspended`, leaving credentials intact.
- `crates/wyrd/wyrd-auth/src/issuance.rs:565-625` and `crates/wyrd/wyrd-sql/src/queries/auth/service_accounts.rs:221-240`: issuance reads the active principal through a separate lookup, which excludes the suspended row. This independently makes re-exchange fail.

The actual key-retirement path is distinct:

- `crates/wyrd/wyrd-server/src/components/principals/routes.rs:450-556`: `DELETE /principals/{principal_id}/credentials/{credential_id}` calls the shared HTTP/MCP owner `revoke_credential_for`, verifies ownership, invokes `revoke_api_key`, and commits.
- `crates/wyrd/wyrd-sql/src/queries/auth/api_keys.rs:12-33`: retirement updates `auth_api_keys.revoked_at`, leaving the principal active. Metadata/listing and ownership support are in the same file at `:36-122`.
- `crates/wyrd/wyrd-sql/src/queries/auth/service_accounts.rs:352-380`: credential exchange's prefix lookup filters `k.revoked_at IS NULL`; this is independent of the later active-principal filter.

Resolution: INVREV-001 describes a real negative-journey coverage gap. J1 proves cached authority eventually re-enters issuance and fails for a suspended principal. It does not prove that retiring one key while its principal remains active stops gateway re-exchange. Its `/auth/token` failure assertion does not distinguish these causes. Removing the revoked-key filter would leave this journey passing because principal resolution would still refuse. The behavior report's PASS therefore cannot establish this particular obligation from J1.

This is not evidence of a production credential-retirement bypass in the sequential path. Source currently rejects a retired credential correctly. The approved spec AC-002 explicitly requires the revoked-key negative and TASK-001 Scenario 3 names the existing credential lifecycle; AGENTS §11 requires relevant negatives at journey tier. The smallest proof correction remains replacing J1's suspension step with existing credential listing/retirement, leaving the principal active and preserving the existing cache timing/refusal/no-dispatch checks. No second minute-long journey or new revocation implementation is needed.

## 2. Late successful exchange admission is reachable

Exact inspected producer-to-consumer path:

1. `crates/wyrd/wyrd-server/src/components/gateway/api_keys.rs:91-117`: a miss awaits `TokenGrants::api_key`, verifies its JWT, and only then samples `Instant::now()` for insertion. Every concurrent successful miss reaches the same insertion; there is no lock held across the grant.
2. `crates/wyrd/wyrd-server/src/components/auth/routes.rs:237-260`: the grant executes the existing exchange and awaits transaction commit before returning its token response.
3. `crates/wyrd/wyrd-auth/src/exchange_api_key.rs:159-177,221-242`: key verification reads the row, verifies its secret, awaits the use update, then issues for the row's principal.
4. `crates/wyrd/wyrd-sql/src/queries/auth/service_accounts.rs:352-380,416-433`: active-key lookup is an ordinary SELECT, with no `FOR UPDATE`; the subsequent use UPDATE matches key id/tenant and does not recheck `revoked_at`.
5. `crates/wyrd/wyrd-auth/src/issuance.rs:424-481,565-625`: issuance resolves active tenant/principal/grants and signs snapshot authority; it does not reread credential revocation. Credential retirement leaves these principal/grant reads eligible.
6. `crates/wyrd/wyrd-sql/src/tenant_conn.rs:67-73`: commit awaits SQLx. The cache insertion is outside the transaction and outside any SQL lock.
7. `crates/wyrd/wyrd-server/src/components/gateway/api_keys.rs:127-163`: insertion unconditionally replaces the same hash with `now + min(60s,lifetime)`; lookup checks that deadline and does not consult durable revocation.
8. `crates/wyrd/wyrd-server/src/components/auth/token_extract.rs:143-153` and `crates/shared/wyrd-auth-verify/src/lib.rs:315-365,888-899`: normal token verification checks the self-contained signed token, tenant and claims. It performs no credential/principal store read.

Two reachable interleavings establish the disputed path without inventing a new provider/network dependency:

- **Retirement before use UPDATE:** exchange E reads an active credential; retirement R updates and commits that credential before E's use UPDATE executes. E's UPDATE can update the retired row because it has no live-key predicate. The principal remains active, so issuance and commit can succeed. This does not require a SQL isolation anomaly or a lock bypass. A variation where R holds the update lock lets E's ordinary SELECT observe the prior live row and its later UPDATE wait for R to finish.
- **Retirement after exchange commit:** E has already signed the token and PostgreSQL commits its transaction, releasing any use-row lock. Before E's response is processed and gateway admission runs, R retires and commits the credential. The successful response remains successful, and no post-commit revocation read occurs before cache admission. Existing row locks therefore do not close the admission interval, even if one disregards the first interleaving.

For either path, let R commit at t=0 and E insert at t=2. Its cache deadline is t=62. A fresh gateway request at t=61 succeeds using the cached still-valid JWT despite more than 60 seconds having elapsed since retirement. This needs only a positive overlap delay, not a 60-second stalled request. The protected edge's 30-second default timeout (`state.rs:1428`, `http/router.rs:187-199`, `http/middleware/edge_timeout.rs:140-169`) can bound an exchange's delay but cannot remove the extra delay from the later 60-second cache window.

Resolution: CONC-001, SEC-001 and SYS-001 identify one shared deadline-origin defect. The claim should not be justified by saying a credential revoke can commit freely while E already holds its last-used row lock: it may wait for that transaction. The two concrete orderings above avoid that incorrect premise. A delayed older successful miss can also replace a same-key entry with a new admission deadline; single-flight remains unnecessary and unrequested.

U2 directly supplies one insertion instant and J1 revokes only after warming returns. Neither exercises this interval. This is source-proven reachability, not a dynamically reproduced race.

## 3. The approved bound governs private admission wording

- Approved spec REQ-001 says both that exchanged tokens are cached for 60 seconds and that revoking a key takes effect within 60 seconds. AC-002 also requires refusal once the cache entry expires. No approved text qualifies the revocation guarantee as applying only to exchanges completed before revocation.
- TASK-001 lines 51-58 prescribe expiry 60 seconds after admission, but expressly prohibit synchronization from extending the revocation window. Its material stop condition likewise excludes a different revocation bound. The admission phrase cannot authorize an additional post-retirement window.
- `architecture/wyrd-design.md:714-725` caps minted-token reuse at 60 seconds. It does not require granting every delayed exchange a fresh 60 seconds after asynchronous completion.
- `architecture/wyrd-security-posture.md:141-144,203-207` distinguishes retiring a credential from suspending a principal and preserves already-issued tenant JWT snapshot authority until expiry. The security foundation repeats this. Therefore no JWT revocation list, current-state check on cache hits, altered issuance workflow, or invalidation of already-issued caller JWTs is authorized or required to correct this gateway reuse deadline.
- `architecture/references/languages/spec-driven-development.md:10-24,182-197` places approved specification above derived tasks and permits private task conflicts to be corrected through planning under the same spec. Review findings create no new product authority.

Resolution: the spec's revocation deadline is explicit; this does not need an invented product/concurrency decision. A conservative pre-exchange clock bounds cache reuse without changing the existing grant's concurrent issuance semantics: every exchange that could observe a pre-retirement credential began no later than retirement, so its start plus 60 seconds is no later than retirement plus 60 seconds. Preserve that instant across completion/replacement, and give already-elapsed results no reusable cache authority. Already-issued token verification and the in-flight request's grant result stay with their existing owners. This is a private gateway-cache correction; the derived task's literal admission wording should be corrected rather than treated as a waiver of REQ-001. Independent validation should select the final correction/proof boundary.

## Proposed finding disposition for validation

| Existing proposal | Focused result | Remaining validation |
|---|---|---|
| INVREV-001 | Supported: credential-retirement journey proof is missing, distinct from current production sequential behavior | Validate the minimal replacement of J1's lifecycle step |
| CONC-001 / SEC-001 / SYS-001 | Supported as one shared cache deadline-origin defect; reachability narrowed to the concrete SQL/commit orderings above | Deduplicate and validate production deadline propagation and focused overlap proof |
| Behavior report PASS on revocation | Insufficient for these two obligations: sequential suspension/expiry proof does not establish credential retirement or overlap timing | Reconcile acceptance rows using source and the validated ledger |

No additional proposed finding was discovered within this focused scope. Source and authority resolve the conflicts; no unresolved question remains for this follow-up.

Skipped runtime reproduction and broad verification. Risk: current proof misses credential retirement, and late exchange admission can authorize new requests beyond the approved revocation bound.
