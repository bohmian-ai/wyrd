# TASK-004 R3 Security, Tenancy, and Audit Domain Review

## Result

**PASS**

No material security, tenancy, authorization, or audit finding remains in the
immutable cumulative base-to-candidate range. The R2 scheduler/source-shape
remediation and the owner-directed deletion of the uncalled
`VerifierRunQueue::new` and `ResultPublisher::endpoint` do not widen the
reviewed security boundary or regress prior `FIND-TASK-004-4` closure.

## Reviewed boundary

- Base: `9431906eeb1c7b67a0efcec09487fd1d848f70a8`
- Candidate: `29721b7e33854633b025b25948fd5d2eaebe7bfd`
- Approved specification: `changes/active/verified-change-contract/spec.md`, revision 33
- Original task: `changes/active/verified-change-contract/tasks/TASK-004-generic-verification-runtime-and-results.md`
- Prior review/remediation: `changes/active/verified-change-contract/review/TASK-004-r1/` and `TASK-004-r2/`

The review traced SYSTEM provisioning and public exclusion; internal token
minting, claim validation, and tenant binding; exact Verifier-scoped result
admission; the closed SYSTEM/result-table matrix; canonical allowed/denied
audit writes; and trusted Scribe tenant/principal/Card attribution. It also
inspected the full cumulative diff for credential, secret, injection,
dependency, and deployment-boundary changes.

## Authority and source coverage

| Boundary | Governing authority | Source and proof inspected | Result |
|---|---|---|---|
| SYSTEM identity and lifecycle | `AGENTS.md` §§2, 7, 9; `architecture/wyrd-design.md`; `architecture/wyrd-security-posture.md`; `REQ-086`; `AC-023` | `20260601000028_system_principal.sql:20-110`; provisioning; `service_accounts.rs:150-267`; public-route and issuance tests | PASS |
| Token and tenant integrity | Security posture trust/identity rules; `REQ-086`; `INV-007` | `TenantTokenIssuer::issue_system_token` (`issuance.rs:427-485`); `AccessTokenClaims::system_kind` (`wyrd-auth-verify/src/lib.rs:735-805`); Gate authentication; widened-claim and cross-tenant tests | PASS |
| Reserved-table and exact-Verifier authorization | `architecture/bifrost-design.md`; `REQ-086`, `REQ-119`, `REQ-121`; `AC-023`, `AC-030` | `Gate::authorize_record_write` (`gate/mod.rs:431-500`), table-owned reserved set (`958-974`), mandatory frame scope (`976-1000`), Scribe's repeated scope validation and UID stamping | PASS |
| Canonical audit | `AGENTS.md` §2; `architecture/agent-rules.md`; security posture audit rules; `INV-007`; `AC-030` | Gate combines RBAC, table matrix, and exact scope into one outcome; `PostgresGateAudit` (`gate_audit.rs:28-63`) commits one allowed/denied event before admission/refusal; internal mint/Scribe mechanics add no authorization event | PASS |
| Supply chain and secret exposure | Security posture secret rules; repository dependency rules | `Cargo.toml`/`Cargo.lock` add only existing workspace crates (`vala-drift`, test-only optional `wyrd-queue`); publisher errors redact token/signing material; no credential bytes enter result rows, audit, logs, schemas, or examples | PASS |

## Prior-finding closure

### `FIND-TASK-004-4` — CLOSED

Gate still validates every SYSTEM result row against the token's one signed,
UID-bearing Verifier scope before selecting and committing its single audit
outcome and before dispatching to Scribe. Absent, null, malformed, empty, or
foreign `card_ref` values are denied; non-SYSTEM principals cannot write any of
the three result tables; SYSTEM cannot write any other table. Candidate changes
after R2 only adjust import spelling, scheduler ordering/tests, and remove two
uncalled accessors/constructors, so the security correction is unchanged.

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

- SYSTEM tokens have a closed claim shape: UUIDv7 subject, verified tenant,
  exactly `bifrost_record:write`, no root Card, roles, credential, refresh, or
  delegation, and exactly one UID-bearing Verifier scope.
- The persisted SYSTEM principal is tenant-RLS scoped, unique per tenant,
  credentialless, forced active, and excluded at the shared public lookup and
  lifecycle owners.
- Gate fails closed if its audit sink or audit commit is unavailable and emits
  exactly one canonical decision for each evaluated result-write request.
- Managed tenant and publisher identities come from the verified token; Arrow
  input cannot select tenant or `principal_id`, and `card_uid` is stamped only
  from the signed scope.

## Verification

The following focused commands passed on the candidate:

- `wyrd-auth-verify`: four SYSTEM claim-shape, delegation, and cross-tenant tests (4/4).
- `wyrd-auth-issue`: direct SYSTEM grant and widened-grant refusal tests (2/2).
- `vala-bifrost-redux`: exact Verifier scope and closed SYSTEM/result-table matrix tests (2/2).

## Verification limits

- This reviewer inspected the complete cumulative diff and the candidate
  source but did not rerun the Postgres-backed public-route/gRPC tests, the
  role-separated journey, or the broad repository lanes. The recorded R1/R2
  evidence covers those paths; focused pure/unit authorization tests were
  independently rerun here.
- `.codegraph/` is absent, so source and caller tracing used repository search
  and direct inspection.
- Candidate identity was rechecked immediately before writing and remained
  `29721b7e33854633b025b25948fd5d2eaebe7bfd`.

## Material findings

None.
