# Admin principals whole-branch review 09 — task implementation review

## Immutable subject

- Repository: `/home/thorrester/Documents/GitHub/wyrd`
- Cumulative base: `c5c20754a167e8f4d74a555a720bd51df6179a6f`
- Candidate / reviewed HEAD:
  `84b7f4ad6eeaf2f20006761c35e34ce8d51ce7f6`
- Code candidate: `20e5becad783d48121010b673da75e211d4dc497`
- Approved specification: `changes/active/admin-principals/spec.md`, revision
  13, status `approved`, SHA-256
  `57f91317e68b06e7b4d34ea94b964e4a1dd99678275a2ee67d1d51f9b4b46332`
- Original tasks: `changes/active/admin-principals/tasks/TASK-001-*.md`
  through `TASK-008-*.md`
- R8 remediation:
  `changes/active/admin-principals/review/whole-branch-08/TASK-001-008-R8-close-validated-findings.md`

The complete cumulative diff and current source were inspected. CodeGraph was
used first to trace the command tree and credential resolution, the RFC 8693
delegation path, token issuance, and canonical audit attribution. The candidate
completion summary and later change-review conclusions were not used as
evidence.

## Review findings

### Critical

None.

### Important

#### `TREV-WB09-1` — VIOLATION — the shipped CLI still accepts live credentials in argv and stores them in debug-printable strings

- **Violated obligation:** `INV-002`, `VER-001`, R8 finding
  `FIND-admin-principals-R8-2`, its acceptance criterion that tenant and
  platform CLI administration accept no secret-valued option, and AGENTS.md's
  requirement to hold secrets in `SecretString` with redacted debug.
- **Exact locations:**
  `crates/wyrd/wyrd-cli/src/auth/issue_key.rs:37-42,69`,
  `auth/trusted_issuer.rs:49-56,80-85,88-113`,
  `auth/workload_binding.rs:40-45,57-62,74-79`,
  `principal/revoke.rs:37-42,62`,
  `auth/refresh.rs:11-19,28-33`,
  `eval/run.rs:30-55`, and `query/mod.rs:15-29`.
- **Evidence:** each listed command exposes `--token`, `--refresh-token`, or
  `--client-secret`; the token and refresh-token values are ordinary `String`
  fields beneath the top-level command tree's derived `Debug`. The R8 change
  correctly removed this shape only from the new `principal credential` and
  `platform` endpoints. `trusted_issuer::AddArgs.client_secret` is redacted in
  `Debug`, but the optional inline secret still enters process argv and shell
  history. An environment fallback does not remove the corresponding CLI
  option. The evidence packet itself classifies three of these reachable
  commands as follow-up rather than satisfying the acceptance result.
- **Observable consequence:** an operator following the accepted CLI can expose
  a bearer, refresh token, or issuer client secret through shell history,
  process listings, or a diagnostic rendering of parsed arguments.
- **Required testable correction:** remove every secret-valued option from the
  shipped command tree. Reuse the existing `ClientConfig` ambient credential
  chain for tenant access tokens, keep `WYRD_PLATFORM_CREDENTIAL` for platform
  commands, read refresh tokens from `WYRD_REFRESH_TOKEN`, and retain the
  existing environment/file inputs for issuer client secrets. Carry resolved
  values in existing redacted secret types. Add no credential-source
  abstraction or second client builder.
- **Focused closure proof:** parser/help tests refuse each former secret option
  without echoing its value; a debug test cannot render a supplied secret; and
  the existing CLI journeys authenticate through environment/config sources,
  including missing-credential refusal.

#### `TREV-WB09-2` — INCORRECT — a successful delegation decision drops the credential that authenticated the delegating caller

- **Violated obligation:** `REQ-037`, `INV-013a`, `AC-009`, `AC-020`, and the R8
  constraint to preserve canonical credential attribution require each audited
  authorization decision to identify the acting principal and the credential
  used for that request.
- **Exact locations:**
  `crates/wyrd/wyrd-auth/src/issuance.rs:111-134,333-367,497-548` and
  `crates/wyrd/wyrd-auth/src/exchange_api_key.rs:235-268,292-313,325-347`.
- **Evidence:** verification retains the caller credential on
  `verified.principal.credential_id`, and refused or allowed-then-refused
  delegation rows attach it in `record_decision` at
  `exchange_api_key.rs:331-345`. On success, however,
  `TenantGrant::Delegation` carries only `caller` and `ceiling`,
  `TenantGrant::credential_id()` returns `None` for delegation, and
  `exchange_audit_event` builds the allowed row for the correct caller but never
  calls `with_credential_id`. The successful-delegation test asserts only
  outcome count and permission attenuation, so the null attribution passes.
- **Observable consequence:** retained audit can say which principal delegated
  and what permission it spent, but cannot answer which of that principal's
  simultaneously live credentials authorized the successful exchange.
- **Required testable correction:** in the existing `DelegateToken` →
  `TenantTokenIssuer` path, carry the verified caller's optional credential id
  far enough for `exchange_audit_event` to attach it to the one successful
  `delegation:issue` row. Preserve the delegated target token's current identity
  and credential-claim semantics, the one-row decision count, permission
  attenuation, transaction boundary, and the already-correct refused paths;
  add no second audit event or sink.
- **Focused closure proof:** exchange a real API key for the caller token,
  delegate successfully, and assert exactly one committed `delegation:issue`
  row names both the caller principal id and that API-key row id. Retain the
  denied, allowed-then-refused, audit-failure, and attenuation proofs.

### Suggestions

None.

## Acceptance matrix

| Requirement, acceptance criterion, constraint, or non-goal | Implementation evidence | Verification evidence | Result |
|---|---|---|---|
| `REQ-001`–`REQ-011`; `INV-001`, `INV-003`, `INV-008`, `INV-009`: independent principals, generic multi-credential ownership, verifier-only persistence, overlap rotation | Durable principal/grant stores remain separate from credential rows; issuance resolves the principal from verified grant evidence | Principal unit/integration, SQL, platform, identity, and CLI lanes recorded in the R8 packet | PASS |
| `REQ-012`, `REQ-012a`, `REQ-012b`; `AC-018`: all five grants share one current-state issuer and requests verify one five-minute `permissions` JWT locally | `TenantTokenIssuer` owns state/grant resolution; `TokenVerifier` is concrete, synchronous, key-only, and database-free | Issuance, refresh, identity, verifier, and journey evidence retained cumulatively | PASS |
| `REQ-012c`; `INV-013a`; `AC-020`: delegation attenuates authority and every decision is durably attributable exactly once | `PermissionSet::intersection`; `DelegateToken::execute`; successful event names caller and permission but omits its credential | Intersection and allow/deny/failure tests pass, but no successful attribution assertion exists | **FAIL — `TREV-WB09-2`** |
| `REQ-013`–`REQ-019`; `INV-004`, `INV-004a`, `INV-004b`: closed platform/tenant contexts and permission-based authorization | Typed extractors and current `PermissionSet` checks remain distinct by plane | Platform, principal, identity, authz, and cross-plane journeys recorded | PASS |
| `REQ-020`–`REQ-024`; `INV-005`; `AC-001`: one explicit transactional deployment initialization | `boot/init.rs` and server command remain the sole initialization boundary | Platform journey covers success, retry, concurrency, and failure | PASS |
| `REQ-025`–`REQ-028`; `INV-006`; `AC-002`, `AC-007`, `AC-008`: transactional tenant provisioning and lifecycle | Platform provisioning/lifecycle owners plus tenant issuance admission | Platform journey passed twice consecutively; focused lifecycle evidence recorded | PASS |
| `REQ-029`–`REQ-033`; `INV-007`; `AC-004`–`AC-006`, `AC-012`: tenant administration, scoped principals, credential lifecycle, and recovery | Tenant operations use `TenantConn`; four redundant manual tenant filters are removed | Principal integration, SQL, tenant-isolation, recovery, rotation, and scoped-Bifrost evidence recorded | PASS |
| `REQ-034`, `REQ-035`, `REQ-041`–`REQ-046`; `AC-011`, `AC-015`–`AC-017`: federated tenant/platform humans and current-state platform authorization | OIDC remains issuance-side; platform requests re-read credential, principal, and grant state via `OperatorPool` | Identity and platform journeys recorded | PASS |
| `REQ-036`, `REQ-047`–`REQ-049`; `AC-013`, `AC-014`, `AC-019`: one shared client/contract projection and runtime OpenAPI | CLI calls Wyrd through `wyrd-client`; `utoipa` owns the served document; malformed changed path ids map to canonical problems | CLI/MCP, codegen, docs, and served-OpenAPI lanes recorded | PASS for contract/client ownership; secret-input failure is tracked separately |
| `REQ-037`; `INV-010`, `INV-011`; `AC-009`: authorization decisions are transactional, fail closed, and name principal, credential, permission, resource, tenant, outcome | Canonical append/publisher path is used; no-effect and refused delegation paths retain credential attribution; successful delegation does not | Existing audit tests prove count/outcome and failure closure but omit successful delegation credential id | **FAIL — `TREV-WB09-2`** |
| `REQ-038`–`REQ-040`: removed bootstrap identity/schema and documented operator/recovery flow | Bootstrap-key artifacts are absent; platform stores replace the unreachable model; active docs describe the operator flow | Docs/codegen/platform evidence recorded | PASS |
| `INV-002`: raw credentials never reach durable or diagnostic surfaces | Server/audit paths use redacted secret types, but reachable CLI args retain token strings and inline secret options | R8 parser tests cover only the two new endpoint families | **FAIL — `TREV-WB09-1`** |
| `INV-012`: invalid credentials remain publicly indistinguishable | Shared credential verification and stable errors remain unchanged | Credential negative tests and journeys recorded | PASS |
| `INV-013`; `AC-005`, `AC-008`, `AC-010`: tenant revocation blocks new issuance while issued tokens expire within five minutes; platform revalidates next request | No tenant epoch/checker/cache/admission read remains; platform store reads remain live | Revocation, expiry, suspension, and deleted-design classification evidence recorded | PASS |
| `INV-014`: administrative state remains server-owned | CLI, MCP, and shared client only project server contracts | Contract/source inspection | PASS |
| `INV-015`: all Wyrd planes use `X-Wyrd-Access-Token` and never consume application `Authorization` | Shared transport/extractors retain the canonical header | Extractor, CLI, and cross-plane evidence retained | PASS |
| `FIND-admin-principals-R6-1`: deleted auth design has no live contract/dependency residue | Live prose and route errors were reconciled; direct server `moka` was removed | Active-tree classification, served OpenAPI, MCP, codegen, and docs evidence recorded | PASS |
| `FIND-admin-principals-R7-5`: literal nonzero focused evidence | R8 packet records 16 literal exact commands with selected count and result | Each recorded as 1/1 on code candidate `20e5becad` | PASS |
| `FIND-admin-principals-R8-2`: no CLI administrative secret options or debug exposure | New platform/principal endpoints are corrected, but the remaining command tree still exposes reachable secrets | Only two parser tests prove option removal | **FAIL — `TREV-WB09-1`** |
| `FIND-admin-principals-R8-3`: no function-scoped migration import | `sha2::Digest` is in the test module import block | Exact migration checksum selector 1/1 | PASS |
| `FIND-admin-principals-R8-5`: forced RLS is the sole tenant filter on the four statements | Redundant predicates/binds are removed; insert keys and composite joins remain | Exact RLS selector 1/1; SQL and tenant-isolation lanes pass | PASS |
| `FIND-admin-principals-R8-6`: scoped allow/refuse over real gRPC | Existing bound-server journey uses the generated gRPC client for covered and uncovered tables | Exact server-journey selector 1/1; owning journey lane 14/14 | PASS |
| `FIND-admin-principals-R8-7`: malformed changed administrative ids match served OpenAPI problems | Typed parameters and `path_rejection` mapping are present | Exact assembled-router/OpenAPI selector 1/1 | PASS |
| Rejected `R8-1`: retain `67b4d0ba`'s `FOR UPDATE NOWAIT` publisher behavior | Cumulative candidate retains the production change and replay proof | No contradictory R8 edit | PASS |
| Rejected `R8-4`: no pre-release audit compatibility machinery | No predecessor schema/hash path was added | Diff/source inspection | PASS |
| R8 non-goals: no checker, cache, epoch, introspection, second issuer/verifier, credential-source abstraction, test harness, compatibility layer, or second audit sink | Existing concrete owners are reused; no prohibited replacement appears | Dependency/source inspection and recorded boundary lanes | PASS |
| `VER-001`–`VER-006`: scoped credible verification, exact selectors, canonical lints and generated-contract proof | Evidence is bound to code candidate `20e5becad` and cumulative candidate `84b7f4ad6` adds evidence only | Required lanes pass; 16 exact selectors selected 1 each; strict rustdoc passes for eight affected crates; `git diff --check` is clean | PASS, except behavioral gaps above |

## Open questions

None. Both corrections use existing owners and require no new product,
architecture, security, compatibility, concurrency, resource-ownership, or
persistent-data decision.

## Verification notes

- This was a read-only static review; recorded long-running lanes were inspected
  but not rerun.
- The R8 evidence is credible for the seven closed findings and attenuation
  behavior, but its CLI proof covers only two endpoint structs and its
  successful-delegation proof never reads `credential_id`.
- The local task shell's `python`/`python3` substitution is recorded and does not
  change the tested scripts.
- `git diff --check c5c20754a..84b7f4ad6` is clean.
- Reviewed HEAD remained
  `84b7f4ad6eeaf2f20006761c35e34ce8d51ce7f6` after the report was written.

## Overall result

**FAIL** — R8 closes the architecture replacement and its other validated
findings, but CLI secret handling remains incomplete and a successful
delegation decision is not fully credential-attributable.
