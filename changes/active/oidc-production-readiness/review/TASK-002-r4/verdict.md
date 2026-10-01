# TASK-002 Review Verdict — R4

## Immutable subject

- Repository: `/home/thorrester/Documents/GitHub/wyrd-oidc-complete`
- Base: `3fc085acf5b3a710d5dc80892bd2e664b3db6174`
- Candidate: `63e6a545db156a095b670a5bb8bc36f6f36fba32`
- Approved specification: `changes/active/oidc-production-readiness/spec.md`, revision 4
- Original task: `changes/active/oidc-production-readiness/tasks/TASK-002-tenant-login.md`
- Prior remediation tasks: TASK-002 R1, R2, and R3
- Reviewed range: complete cumulative base-to-candidate diff, 75 files,
  7,185 insertions and 1,713 deletions

The candidate remained `63e6a545db156a095b670a5bb8bc36f6f36fba32`
through both review waves. Review artifacts are outside the immutable subject.
Lead-directed reuse and test commits recorded separately in the evidence tables
were treated as authorized work rather than scope drift and were still checked
for regression.

## Verdict

**FIX_REQUIRED**

The cumulative tenant-login implementation closes all ten prior findings and
passes the route, provider binding, tenant isolation, identity, role mapping,
audit, connection lifecycle, contract, documentation, and machine-separation
obligations. Two independently validated defects remain. The shared external
verifier does not require the configured issuer/audience claims and the OIDC
ID-token path does not enforce the required issued-at/future-time contract.
Refresh replay containment also lacks serialization against rotation of a
different current row in the same family, so an overlapping ancestor replay can
commit containment while the new successor remains usable. Both corrections
fit existing owners and approved behavior; no specification revision is needed.

## Acceptance matrix

| Obligation | Implementation and verification evidence | Result |
|---|---|---|
| REQ-006 / INV-001: route and headers are context only; callback authority comes from one-use server state | `HumanConnections::begin_login`, `AuthorizationCodeExchange`, `WyrdPostgres::login_state_tenant`, header-hostility/refusal/OpenAPI proofs | PASS |
| REQ-007 / INV-004: PKCE, nonce, exact redirect, screened provider IO, signature/key/algorithm, issuer/audience/time/claims, and replay protection fail closed | State, callback, screened IO, shared verifier, nonce/`azp`, and refusal tests pass for the covered cases; `FIND-TASK-002-11` shows missing binding/time claims remain accepted | **FAIL** |
| REQ-008 / INV-002: exact `(issuer, sub)` tenant User identity and mapped tenant roles only | Exact-`sub` validation, issuer/subject identity resolution, current role mapping, human-login and distinct-subject proofs | PASS |
| REQ-013 / INV-003: machine paths remain independent of human SSO | Separate API-key/workload grants and machine-independence journey | PASS |
| REQ-014–016: provider replacement, tenant isolation, exact refresh provenance, cutoff, and five-minute access snapshot | Active connection/revision checks, forced RLS, switch/cutoff/same-issuer journeys and migration proofs | PASS |
| Packet renewal contract and INV-004: lock the refresh family and preserve replay-family containment | Existing same-row races serialize and sequential replay tests pass; `FIND-TASK-002-12` proves ancestor replay can overlap current-row rotation and miss the successor | **FAIL** |
| REQ-017: issuance and role changes use canonical transactional audit | Role-sync/token-exchange append and rollback proofs | PASS |
| Packet begin/callback/completion contract and authorization-code retirement | Typed begin/state/completion flow, fixed callback response, removed legacy token grant/client/CLI route, schema/OpenAPI proofs | PASS |
| Required real-server journeys and negative boundaries | Login, refusal, provider switch, machine independence, Postgres, and served-contract evidence is recorded; retained findings identify two missing focused cases | **FAIL** |
| Prior findings `FIND-TASK-002-1` through `FIND-TASK-002-10` | Independently revalidated against current source and post-R3 changes | PASS |
| Lead-directed reuse and test commits | Audit/verifier reuse and gateway/identity test changes preserve intended behavior and repository rules | PASS |
| Non-goals and scope | No compatibility route, alternate audit path, new dependency, TASK-003 BFF flow, or TASK-004 CLI handoff implementation entered the candidate | PASS |

The full requirement matrix is preserved in `task-review.md`; authority and
domain coverage are preserved in the other Wave 1 reports.

## Wave 1 results

| Review | Result | Proposed findings |
|---|---|---|
| Task implementation | PASS | None |
| Repository standards | PASS | None |
| Security/RBAC domain | FAIL | `SEC-R4-001` |
| Tenancy/persistence/concurrency domain | FAIL | `TD-R4-001` |

Wave 2 validated the standards empty ledger, rejected the task-review empty
ledger because the domain findings violate mapped task obligations, and revised
both proposed findings after complete caller and dependency/transaction tracing.

## Validated finding ledger

| Finding | Status | Classification | Violated obligation | Decision-complete correction and closure proof |
|---|---|---|---|---|
| `FIND-TASK-002-11` | REVISED from `SEC-R4-001` | INCORRECT | REQ-007, INV-004, full ID-token verification, fail-closed issuer/audience/time binding | Require `exp`/`iss`/`aud` and validate present `nbf` in the shared verifier; add one reused OIDC ID-token verification entry for tenant and platform callers that requires numeric, non-future `iat` under existing skew, while leaving workload assertions on the generic path. Prove missing binding/time claims and future times fail, and extend the refusal journey with no persisted login effects. |
| `FIND-TASK-002-12` | REVISED from `TD-R4-001` | INCORRECT | Task family-lock requirement, INV-004 replay protection, refresh-family containment | Serialize every family operation through one existing PostgreSQL transaction-lock pattern keyed by the stored tenant principal family, re-read after locking, retain fixed family-then-connection lock order, and hold through rotation or containment commit. Prove an ancestor replay overlapping current-token rotation leaves the successor revoked and unusable with durable canonical audit. |

Full caller traces, dependency semantics, interleaving evidence, rejected broader
alternatives, and minimum correction boundaries are in
`findings-validation.md`.

## Prior-finding closure

| Prior finding | Result |
|---|---|
| `FIND-TASK-002-1` — exact human `sub` identity | CLOSED |
| `FIND-TASK-002-2` — OIDC `azp` validation | CLOSED |
| `FIND-TASK-002-3` — canonical role-change audit | CLOSED |
| `FIND-TASK-002-4` — advertised asymmetric algorithm enforcement | CLOSED |
| `FIND-TASK-002-5` — RLS-only login-state transitions | CLOSED |
| `FIND-TASK-002-6` — owner-bound state lookup | CLOSED |
| `FIND-TASK-002-7` — redacted PKCE verifier | CLOSED |
| `FIND-TASK-002-8` — required Rust-item documentation | CLOSED |
| `FIND-TASK-002-9` — module-top imports | CLOSED |
| `FIND-TASK-002-10` — accurate algorithm-helper rustdoc | CLOSED |

The new findings do not reopen those closures: finding 11 concerns mandatory
claims after signature verification, while finding 12 concerns cross-row family
serialization.

## Verification limits

- Reviewers inspected the complete cumulative diff, applicable authorities,
  prior remediation, full named bodies and callers, the locked
  `jsonwebtoken 9.3.1` behavior, and the concrete refresh transaction ordering.
  The repository has no `.codegraph/` index, so Git, `rg`, and direct source
  inspection were used.
- Recorded candidate evidence is green for the four required journeys, the full
  identity lane, principals unit/integration, SQL, tenant/pool/client boundary
  checks, docs, codegen, format, lints, rustdoc, gateway tests, and `test:wyrd`
  with 2,213 passing tests. Reviewers did not rerun expensive Docker, Postgres,
  IdP, or broad lanes during this bounded audit.
- Existing tests do not exercise missing ID-token binding/time claims or the
  ancestor-replay/current-rotation overlap. The focused proofs in the
  remediation task are required for closure.
- All required subreviews completed within the 20-minute cutoff; no reviewer or
  evidence gap was omitted.
- TASK-003 BFF completion and TASK-004 CLI handoff persistence remain intentional
  non-goals.

## Remediation

Implement `TASK-002-R4-oidc-claims-and-refresh-family-serialization.md` through
`$wyrd-implement`, then reassess the complete original base-to-new-candidate
range.
