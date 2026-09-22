# Bifrost authentication and delegation domain review

## Immutable subject

- Repository: `/home/thorrester/Documents/GitHub/wyrd`
- Base: `c5c20754a167e8f4d74a555a720bd51df6179a6f`
- Candidate: `84b7f4ad6eeaf2f20006761c35e34ce8d51ce7f6`
- Candidate rechecked before writing: `HEAD` remained `84b7f4ad6eeaf2f20006761c35e34ce8d51ce7f6`.
- Approved authority: `changes/active/admin-principals/spec.md`, revision 13.

## Boundary and authority coverage

| Boundary | Authority and source inspected | Result |
|---|---|---|
| Tenant JWT verification at HTTP and public gRPC admission | `REQ-012a`, `REQ-012b`, `INV-013`; `architecture/v1/00-foundations/security.md`; `architecture/wyrd-security-posture.md`; `wyrd-auth-verify/src/lib.rs`; `wyrd-server/src/components/auth/token_extract.rs`; `vala-bifrost-redux/src/gate/auth.rs`; `wyrd-server/src/grpc/query.rs` | PASS — one concrete synchronous verifier checks signature, issuer, audience, expiry, and signed tenant locally, constructs authority from `permissions`, and performs no request-time auth-store read or positive cache lookup. |
| Bifrost coarse admission and stable-object authorization | `AC-012`; `architecture/bifrost-design.md` object-scoped RBAC contract; `wyrd-server/src/query/service.rs`; `vala-bifrost-redux/src/oracle/planner.rs`; `vala-bifrost-redux/src/oracle/mod.rs` | PASS — a scoped grant admits the operation, Oracle resolves tenant-bound catalog identities and stable table UIDs, and every resolved table must be covered before reader guards, provider construction, physical planning, admission, audit acceptance, peer dispatch, or source reads. |
| HTTP/gRPC parity and exact/schema/global scope | R8 finding 6; `wyrd-testing/tests/bifrost/server/query.rs` | PASS — the real generated gRPC client drains a covered schema-scoped query and receives `WYRD_VALA_403_QUERY_FORBIDDEN` before opening a stream for an uncovered table; the surrounding HTTP matrix covers schema, exact-table, global, mixed-table, denial-audit, and audit-failure behavior. |
| Stream token-expiry semantics | Revision 12 decision; `wyrd-server/src/grpc/query.rs`; `wyrd-testing/tests/bifrost/server/query.rs` | PASS — verification occurs once before Oracle stream creation, admitted work retains its existing query deadline, and the journey crosses a real short token expiry then proves the expired bearer cannot establish another stream. |
| Tenant isolation and distributed authority | `REQ-014`, `REQ-031`, `AC-004`; `architecture/bifrost-design.md`; `architecture/wyrd-security-posture.md`; `gate/auth.rs`; Oracle resolved-table and scoped-permission-digest paths | PASS — tenant comes only from the signed token, catalog resolution is tenant-bound, and distributed assignments bind the approved table scopes into the permission digest without a grant lookup or second checker. |
| Delegated Bifrost authority | `REQ-012c`, `INV-013a`, `AC-020`; `wyrd-runtime/src/permission.rs`; `wyrd-auth/src/exchange_api_key.rs`; `wyrd-auth/src/issuance.rs` | PASS — the delegated token carries the semantic caller/target intersection, including schema-to-table narrowing and disjoint removal, so delegation cannot widen Bifrost table authority. |
| Delegation decision audit | `REQ-012c`, `AC-009`, `AC-020`; repository audit rules; `architecture/wyrd-security-posture.md`; delegation execution and issuance audit paths | **FAIL** — the decision count, commit-before-response, and fail-closed behavior are correct, but the successful path drops the credential attribution carried by the verified caller token. |

## Verification evidence and limits

- R8 evidence records the focused scoped-gRPC journey as `1/1` and `mise run test:bifrost:journey:server` as `14/14` on the final tree.
- R8 evidence records focused permission-intersection and delegation decision tests as `1/1`, the real auth delegation journey as `1/1`, and the relevant shared, SQL, server integration, Bifrost journey, lint, isolation, codegen, docs, and rustdoc lanes passing.
- I did not rerun the environment-owning journeys; this review validated their exact commands, selectors, assertions, and production call paths against the immutable source. The existing successful-delegation tests mint fixture subject tokens with `credential_id: None`, so their green result cannot detect the finding below.

## Material finding

### `BIFROST-R9-1` — successful delegation loses the authenticating credential ID

- **Classification:** INCORRECT
- **Violated obligation:** `AC-009` requires each covered authorization decision to name the credential; `architecture/wyrd-security-posture.md` requires every decision made from a credential-minted token to record that credential's non-secret ID; `REQ-012c` permits the successful token-exchange row to stand in for the allowed `delegation:issue` row only when it satisfies the full audit contract.
- **Exact location:** `crates/wyrd/wyrd-auth/src/issuance.rs:129-134`, `:334-351`, and `:503-548`; caller propagation originates at `crates/wyrd/wyrd-auth/src/exchange_api_key.rs:243-310`.
- **Evidence:** the verified caller principal retains the subject token's `credential_id`, and the denial and allowed-then-refused paths attach it at `exchange_api_key.rs:325-347`; on success, however, `TenantGrant::Delegation` makes `credential_id()` return `None`, that value is used both for the new token and the issuance setup, and `exchange_audit_event` returns the delegated allowed event without calling `with_credential_id`, so `vala.audit_staging.credential_id` is null even when the caller token was minted from an API key.
- **Observable consequence:** a successful delegated-token issuance cannot be attributed to the particular live credential used by a multi-credential principal, while an equivalent denied or later-refused delegation can; incident response therefore cannot identify which key authorized the successful delegation.
- **Required testable correction:** preserve the intentional rule that the newly delegated token has no `cid`, but carry the verified caller principal's optional credential ID into the successful `delegation:issue` audit event before the transaction commits; reuse the existing shared issuer and canonical audit append, with no second event or audit path.
- **Focused closure proof:** exchange a real tenant API key to obtain a subject JWT with a known `cid`, successfully delegate it, and assert exactly one allowed `auth.token.exchange`/`delegation:issue` staging row names the caller principal and original API-key ID while the resulting delegated JWT still has no credential ID.

## Overall result

**FAIL**

The Bifrost request path, scoped gRPC authorization, stable table resolution, tenant isolation, stream-expiry boundary, and delegation attenuation satisfy the approved architecture; the single retained finding is the reachable successful-delegation audit-attribution defect above.
