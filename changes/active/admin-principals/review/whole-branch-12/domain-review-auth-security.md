# Auth, security, RBAC, tenancy, and audit domain review

## Immutable subject

- Base: `c5c20754a167e8f4d74a555a720bd51df6179a6f`
- Candidate: `261168087376095fa5ad9d66946e755f3baa8fe4`
- Approved specification: `changes/active/admin-principals/spec.md`, revision 14,
  SHA-256 `66d884ba4379c98ade7571e4d9bb4248abbb8a7ce6b73dec94db1b052594ed02`
- Original tasks: `changes/active/admin-principals/tasks/TASK-001-*.md`
  through `TASK-008-*.md`
- Cumulative remediation reviewed through
  `changes/active/admin-principals/review/whole-branch-11/TASK-001-008-R11-close-r9-r10-findings.md`
- The accepted five-minute stateless-JWT revocation window was excluded as
  directed.

The candidate remained at the stated commit throughout this review.

## Reviewed boundary and source coverage

| Boundary | Authority | Source and proof inspected | Result |
|---|---|---|---|
| Tenant issuance and verification | `REQ-005`, `REQ-012` through `REQ-012b`, `INV-009`, `INV-013` | `wyrd-auth/src/issuance.rs`; `wyrd-auth-issue/src/lib.rs`; `wyrd-auth-verify/src/lib.rs`; API-key, OIDC, refresh, workload, and delegation callers; recorded auth and identity lanes | PASS |
| RFC 8693 delegation identity | `REQ-012c`, `INV-013a`, `AC-020` | `wyrd-spec/src/auth/token.rs`; `wyrd-auth/src/exchange_api_key.rs`; `TenantGrant::Delegation`; access-token issue/verify conversion; focused exchange tests and the real Bifrost journey | PASS |
| Delegation authority attenuation | `REQ-012c`, `REQ-017`, `AC-012`, `AC-020` | `PermissionSet::intersection`; actor current-grant resolution in `TenantTokenIssuer`; exact/schema/wildcard/disjoint tests; A-read/B-read-write journey | PASS |
| Audience confinement | `REQ-012a`, `REQ-012c`, `AC-020` | `TokenAudience`; `TokenVerifier::verify` / `verify_on`; Wyrd HTTP extractor; Bifrost HTTP and gRPC admission; Bifrost-token replay refusal in the journey | PASS |
| Directed invoke policy | `REQ-012c`, `INV-013a`, `AC-020` | `policy_context`; production `PolicyHook` composition checks; `DirectedInvokePolicy`; A-to-B allowance and reverse B-to-A denial with valid tokens | PASS |
| Delegation audit and fail-closed behavior | `REQ-012c`, `REQ-037`, `AC-009`, `AC-020` | `record_decision`; `exchange_audit_event`; canonical append; commit ordering; denied, allowed, allowed-then-refused, and audit-failure tests | PASS |
| Request attribution | `REQ-013` through `REQ-016`, `REQ-037` | `AuthenticatedPrincipal`; `Caller::from_authenticated`; HTTP, MCP, Oracle, query, and Gate projections; delegated read and native-ingest audit assertions | PASS |
| Tenant isolation | `REQ-014`, `REQ-018`, `REQ-031`, `INV-003`, `INV-007` | unverified tenant selection followed by tenant-bound verification; `TenantConn` RLS paths; cross-tenant token refusal; tenant isolation check evidence | PASS |
| Platform/tenant plane separation | `REQ-012a`, `REQ-013`, `REQ-018`, `INV-004` through `INV-004b` | `AuthContext`; `PlatformCaller`; platform session scope validation; per-request credential/principal/grant reads through `OperatorPool`; tenant extractor refusal of platform kinds | PASS |
| Principal and credential lifecycle | `REQ-001` through `REQ-011`, `REQ-029` through `REQ-033`, `INV-001`, `INV-002`, `INV-008`, `INV-011`, `INV-012` | principal-generic credential stores/routes, active-state issuance reads, verifier-only persistence, ownership checks, indistinguishable invalid-credential paths, lifecycle journeys | PASS |
| Secret handling | `REQ-008`, `REQ-009`, `INV-002` | `SecretBearer` redacted `Debug`; `SecretString`; CLI ambient/file/stdin inputs; removed secret-valued argv options; one-time response/terminal paths; secret-option rejection proofs | PASS |

## Security Audit

### Critical

None.

### High

None.

### Medium

None.

### Low / Defense In Depth

None required by the approved task.

### Positive Controls

- Tenant requests verify Ed25519 signature, issuer, audience, expiry, and
  tenant locally, authorize only from the signed `permissions` claim, and
  perform no request-path database introspection or positive token caching.
- Platform requests accept only platform-scoped sessions and re-read the live
  credential or principal plus current platform grant on every request.
- Delegation uses `sub=A`, outer `act.sub=B`, rejects cross-tenant, self,
  Card-free-actor, delegated-actor, malformed, and unverifiable pairs, and
  resolves B's current grants before issuing.
- Delegated authority is the semantic intersection of A's signed authority and
  B's current authority; the real journey proves B's direct write works while
  the delegated client can read A's table but cannot register a table or write
  a record.
- The directed invoke question is built as A-subject to B-actor; the same two
  valid tokens reversed are denied and issue no token.
- Every post-policy exchange outcome is committed through the canonical audit
  path before return, and an unavailable audit refuses issuance.
- Delegation audit records operation `auth.token.exchange`, permission
  `invoke`, subject A, actor B, Bifrost audience, and B's authenticating
  credential; later HTTP/query/native-ingest decisions retain B in the actor
  chain while authorizing as A.
- `SecretBearer` redacts `Debug`, credentials are stored only as Argon2
  verifiers, metadata listings omit plaintext, and shipped CLI commands do not
  accept secrets in process arguments.

## Proposed findings

None. No reachable auth, authorization, tenant-isolation, audit-attribution,
or secret-handling defect introduced by the cumulative candidate remained
after independently tracing the R11 corrections. In particular, the accepted
five-minute stateless-JWT revocation behavior is not reclassified as a finding.

## Verification limits

- This was a review-only source and evidence audit; no source file or test was
  changed and no long-running lane was rerun by this reviewer.
- The task records positive focused selections for the exchange outcomes,
  directed delegation journey, audience refusal, Python and TypeScript
  delegated-client journeys, native-ingest attribution, UUID contract, and CLI
  secret-option refusal, plus passing auth, identity, principals, Bifrost,
  tenant-isolation, client-tier, generated-contract, lint, and documentation
  lanes.
- The final candidate additionally records a green aggregate gate and removal
  of false-positive early-return test guards; those unrelated gate-enabling
  changes were explicitly authorized by the user and were inspected only for
  security regression.

## Overall result

**PASS**
