# Admin principals whole-branch review 02 — findings validation

## Immutable subject and result

- Base: `c5c20754a167e8f4d74a555a720bd51df6179a6f`
- Candidate: `5293546f33b3a5fd9de529098e23ea70d472c412`
- Authority: approved `changes/active/admin-principals/spec.md`, revision 7; original task packets; repository `AGENTS.md`; current architecture authorities; and the branch owner's explicit approval that the bundled verified-change-contract work belongs in this cumulative candidate.
- Result: **FIX_REQUIRED**.

The candidate closes much of whole-branch-01, including deletion of the second audit table, platform credential timing parity, served platform credential management, SSRF screening, MCP journeys, and the rustdoc/comment defects. It is not acceptance-complete: sixteen distinct roots remain after consolidating the five Wave-1 reports. The corrections below follow the Ponytail order: delete invalid paths first, reuse the existing transaction/audit/error/test patterns, use current dependency capabilities, and add only the minimum code still required.

## Wave-1 disposition and ID consolidation

Wave-1 reports assigned colliding local `R2-*` labels. This ledger treats those labels as proposals, preserves established whole-branch-01 IDs for recurring roots, and assigns one unambiguous new ID only to each genuinely new root.

| Proposed finding | Validation | Final disposition |
|---|---|---|
| Task/standards/contract `FIND-admin-principals-R2-1` and data-review drift proposal | **REJECTED** | The branch owner explicitly approved the verified-change-contract work inside this cumulative candidate. It is neither unrelated drift nor grounds to remove those commits. |
| Task-review `FIND-admin-principals-1` | **REVISED / CONFIRMED** | `FIND-admin-principals-1`; the alternate audit table is gone, but same-plane platform allowances still commit before their effects. |
| Task-review `FIND-admin-principals-2` | **CONFIRMED** | `FIND-admin-principals-2`. |
| Task-review `FIND-admin-principals-4` | **CONFIRMED** | `FIND-admin-principals-4`. |
| Task-review `FIND-TASK-001-10` | **CONFIRMED** | `FIND-TASK-001-10`. The later scope approval did not waive identity/trailer rules. |
| Task-review `FIND-admin-principals-R2-2` | **CONFIRMED** | `FIND-admin-principals-R2-2` — platform principal kind projection. |
| Task-review/security local `R2-4` credential attribution | **CONFIRMED** | `FIND-admin-principals-R2-3` — one canonical audit-schema root. |
| Security local `R2-1` and data audit coupling | **CONFIRMED / CONSOLIDATED** | Existing `FIND-005-1` — tenant same-plane audit coupling. |
| Security local `R2-2` | **CONFIRMED** | `FIND-admin-principals-R2-4` — unusable TenantAdmin refresh tokens. |
| Security local `R2-5` | **CONFIRMED** | `FIND-admin-principals-R2-5` — tenant admission cached with principal epoch. |
| Security local `R2-3` | **CONFIRMED** | `FIND-admin-principals-R2-6` — tenant API-key timing parity. |
| Contract OpenAPI/catalog findings | **REVISED / CONFIRMED** | Existing `FIND-admin-principals-13`; no new catalog or documentation subsystem. |
| Data provisioning retry finding | **REVISED / CONFIRMED** | Existing `FIND-004-3`; stale slug recovery exists, but undisclosed credentials can still be orphaned. |
| Security proposal that principal revoke must revoke every credential | **REJECTED** | Current approved semantics invalidate already-issued tokens by epoch. The principal is not suspended/deleted, and surviving credentials intentionally remain able to authenticate; no cited requirement requires credential retirement here. |

## Retained findings

### FIND-admin-principals-1 — REOPENED — VIOLATION — Important

**Contract.** REQ-037 and the repository audit authority require a permission allowance and its same-plane mutation to commit in the transaction that made the decision.

**Evidence and caller path.** `crates/wyrd/wyrd-auth/src/platform_authz.rs:105` correctly returns a `TenantConn`, but `crates/wyrd/wyrd-server/src/components/platform/identity.rs:114-125` commits it inside the shared `authorize` helper. `configure_connection`, `remove_connection`, and administrator status mutation call that helper and only then perform their platform SQL writes. `components/platform/credentials.rs:86,181` has the same sequence for issue/revoke. `components/platform/provisioning.rs:287-310` commits the suspension allowance before `set_tenant_suspended`. These calls are reachable from the served platform router and client/CLI surfaces.

**Impact.** A post-authorization database failure can leave a durable allowed audit record for an effect that never occurred. This is exactly the allowance/effect mismatch the canonical transaction rule exists to prevent.

**Minimum remediation.** Delete the commit from the shared authorization helper. Return the existing audited `TenantConn`, perform each same-plane write through it, and commit once. Keep cross-plane provisioning/recovery as explicit resumable boundaries; do not invent a distributed transaction or another audit store.

**Proof.** Inject a failure after platform authorization but before each mutation class and assert that neither effect nor allowed audit row commits. Preserve denied-decision durability.

### FIND-admin-principals-2 — REOPENED — VIOLATION — Important

**Contract.** `AGENTS.md` and the approved task constrain library SQL capabilities to `TenantConn` and `OperatorPool`, including public function signatures and dependency-owning fields.

**Evidence and caller path.** `crates/wyrd/wyrd-sql/src/operator_pool.rs:38` exposes `sqlx::Transaction<'_, Postgres>` from `OperatorPool::begin`. Exported platform query functions in `queries/platform/provisioning.rs:28`, `principals.rs:56`, `credentials.rs:106`, `principal_grants.rs:25`, and `identity.rs:244` accept the raw transaction. Live server owners `components/platform/provisioning.rs:93` and `components/platform/recovery.rs:36` retain `WyrdPostgres` rather than the allowed capability owner. These are production provisioning, recovery, identity, and credential call paths, not test helpers.

**Impact.** Callers regain unrestricted SQL capability and tenant-context construction outside the reviewed owners, weakening the boundary that is meant to make tenant isolation inspectable.

**Minimum remediation.** Keep operator-only SQL as private inherent behavior of `OperatorPool`; acquire `TenantConn` at the composition boundary for tenant work. Remove raw SQLx transaction types from exported query signatures and replace the `WyrdPostgres` fields with the existing narrow owners. Do not add a third wrapper.

**Proof.** Extend the existing boundary source check to the platform query/server modules and run the current principal/platform integration journeys.

### FIND-admin-principals-3 — REOPENED — VIOLATION — Important

**Contract.** Public failures must use stable safe messages while source diagnostics remain server-side.

**Evidence and caller path.** `crates/wyrd/wyrd-server/src/auth/revoke.rs:126`, `crates/wyrd/wyrd-auth/src/revoke.rs:71`, and `crates/wyrd/wyrd-server/src/components/admin/routes.rs:669` interpolate `Display` source errors into `WyrdError::Internal.message`. The admin helper is called by serialization, secret sealing, and binding paths. `WyrdErrorResponse` publishes that message in the HTTP problem body.

**Impact.** SQL, crypto, parser, or provider details can cross the public HTTP boundary despite the candidate's safe mapper elsewhere.

**Minimum remediation.** Reuse `http::error::internal_failure` (or its exact existing static-message pattern), log the source with structured tracing, and delete these parallel leaking constructors.

**Proof.** Force each reachable source failure and assert that the response contains the stable message/code and none of the source text.

### FIND-admin-principals-4 — REOPENED — MISSING — Important

**Contract.** The task required the governing security/design authorities to describe both planes and the approved principal model.

**Evidence.** `architecture/wyrd-security-posture.md:40-69` still describes only `User`, `Service`, `Agent`, and `System`, says every runtime identity is tenant-owned/Card-bound, and describes only tenant token/epoch behavior. The shipped admin model contains tenantless platform principals, `GlobalAdmin`/`TenantAdmin`, and a card-free tenant service principal. `architecture/wyrd-design.md` uses `PlatformAdmin`/`System` terminology while the stable wire and implementation use `GlobalAdmin` and do not project `System` in this flow.

**Impact.** Implementors cannot simultaneously obey the active authorities and the approved/shipped contract; future security work can regress either plane while appearing compliant.

**Minimum remediation.** Edit the existing authorities in place to describe the two-plane lifecycle, exact stable principal names, card-free exceptions, and platform revocation model. Reconcile with the separately approved verified-change-contract material; do not delete or duplicate that authorized work.

**Proof.** A source-level authority check plus docs validation must show one consistent principal set and no claim that all platform identities are tenant/Card-bound.

### FIND-admin-principals-8 — REOPENED — INCORRECT — High

**Contract.** REQ-041..046 require the final independently usable platform administrator to survive status changes.

**Evidence and caller path.** `crates/wyrd/wyrd-sql/src/queries/platform/principals.rs:267` counts any row in `platform.principal_identities` as usable. It does not require a pinned subject or a currently configured OIDC connection. The test registration helper inserts an unpinned identity but names the case as pinned. `set_platform_principal_status` uses this count before suspension.

**Impact.** A merely pre-registered human, or an identity whose connection has been removed, can be counted as the remaining administrator; suspending the real root then locks out the deployment.

**Minimum remediation.** Tighten the existing query: usable means an active credential, or a pinned identity whose connection still exists, plus the fixed admin grant. Retain the advisory lock and existing exhaustive count; add no new state table.

**Proof.** Cover unpinned identity, pinned/current identity, removed connection, live credential, ungranted principal, and concurrent suspension.

### FIND-admin-principals-13 — REOPENED — INCORRECT — Important

**Contract.** AC-014 requires the generated HTTP artifact to include the administrative/auth surfaces, problem media type, and stable route errors.

**Evidence and caller path.** The live `/auth/token`, `/v1/admin/trusted-issuers`, and `/v1/admin/workload-bindings` routes are not registered in `WyrdApiDoc` or the generated `openapi.yaml`; their admin handlers have no matching `utoipa` operations. Existing `body = WyrdProblem` declarations emit ordinary `application/json`, while the runtime error boundary serves `application/problem+json`. The artifact also does not expose the stable route-specific error codes required by the approved spec.

**Impact.** Generated clients and operators cannot discover core authentication/admin operations or faithfully interpret their errors.

**Minimum remediation.** Annotate and register the existing routes under the current OpenAPI owner, reuse the repository's existing `application/problem+json` response construction, and project the existing error catalog per operation. Do not create a second catalog.

**Proof.** Add one source-of-truth comparison between served auth/admin routes and OpenAPI paths, assert problem media and stable codes, then regenerate with the existing codegen lane.

### FIND-004-3 — REOPENED / REVISED — INCORRECT — High

**Contract.** Provisioning cancellation/retry must converge on one active tenant, one administrative principal, and one usable disclosed credential without orphan credentials.

**Evidence and caller path.** `crates/wyrd/wyrd-server/src/components/platform/provisioning.rs:355-426` reuses an existing administrator principal but always generates and inserts a new API key. If the tenant transaction commits and the request is then cancelled or `mark_tenant_active` fails, credential A remains usable but its plaintext was never returned; retry creates credential B. The current journey's failure occurs before principal insertion, and its abandoned-row setup manually resets a completed tenant while asserting principal count, not usable credential count.

**Impact.** Retry can leave an undisclosed live administrative credential, violating recovery convergence and creating a durable secret with no legitimate holder.

**Minimum remediation.** Make the existing administration stage idempotent: on retry, ensure any prior undisclosed provisioning credential cannot remain usable before returning exactly one new credential. Reuse the existing tenant transaction and credential/principal owners; no provisioning ledger or second credential store.

**Proof.** Inject cancellation/failure after the tenant transaction commits but before activation, retry, and assert one administrator plus exactly one usable credential whose plaintext is the returned value.

### FIND-005-1 — REOPENED — VIOLATION — Important

**Contract.** Tenant-plane administrative effects must commit with their allowed audit decision.

**Evidence and caller path.** `crates/wyrd/wyrd-server/src/components/admin/routes.rs:244,334,397,467` writes an allowed audit through the standalone `record_audit`, then opens a different `TenantConn` to create/delete trusted issuers or workload bindings. `crates/wyrd/wyrd-server/src/auth/revoke.rs:86` similarly commits the audit before opening the connection that bumps the principal epoch. Principal credential routes already demonstrate the correct `append_on` pattern.

**Impact.** The server can record an allowed change that failed, or apply a change without its allowance if sequencing is altered. This breaks the canonical atomicity invariant on live HTTP paths.

**Minimum remediation.** Reuse one `TenantConn` and `append_on` for allowed same-plane mutations; commit once. Keep denied decisions independently durable. Delete the route-local instruction that allowed audit must be standalone.

**Proof.** Inject mutation and audit-append failures for issuer, binding, and revocation operations; assert no allowance/effect mismatch.

### FIND-003-2 — REOPENED / REVISED — MISSING — Important

**Contract.** The operator initialization acceptance requires the real `wyrd-server init` command, one-time stdout secret delivery, safe diagnostics, concurrency, and clean retry.

**Evidence.** The binary subcommand exists, but the candidate's initialization journeys call `initialize_platform_root` directly. The nominal CLI operator journey also imports that library function. No test invokes the actual process, clap parsing, stdout/stderr split, exit status, or repeat invocation.

**Impact.** Library correctness does not prove the shipped operator entrypoint prints the secret exactly once, keeps it out of stderr/logs, or rejects repetition safely.

**Minimum remediation.** Extend the existing process journey to spawn the actual `wyrd-server init`; do not introduce another harness.

**Proof.** Capture stdout/stderr for initial success and repeat refusal, assert exactly one credential only on initial stdout, plus current concurrency/failure retry behavior.

### FIND-004-5 — REOPENED / REVISED — MISSING — Important

**Contract.** REQ-033 requires recovery from total loss of platform credentials through deployment-level operator database/secret access, not through another application principal. AC-002 also requires the operator path to complete tenant and OIDC configuration.

**Evidence.** The server binary exposes only `Init`, which refuses an already initialized deployment. Current operator docs state recovery requires another live platform credential and explicitly deny a deployment backstop. The process journey never proves total credential-loss recovery and substitutes library initialization for the binary; it also does not complete the specified tenant/OIDC configuration sequence.

**Impact.** Loss of all platform bearer credentials makes the deployment permanently unadministrable despite the approved recovery decision.

**Minimum remediation.** Add the smallest operator-only recovery action beside initialization, using the existing initialization-class `OperatorPool` and platform credential issuer for the same root. It must not be an HTTP/application-principal path and must not add a principal, role, grant engine, or new secret store. Correct the existing docs and journey.

**Proof.** Through the actual binary, revoke/lose all platform credentials, recover with deployment operator access, authenticate with the new credential, and complete tenant plus OIDC configuration without SQL typed by the operator.

### FIND-TASK-001-10 — REOPENED — VIOLATION — Important

**Contract.** `AGENTS.md` §13 requires the configured contributor identity `Thorrester <sjforrester32@gmail.com>` and forbids AI co-author/session trailers.

**Evidence.** Across the immutable range, 49 of 100 commits have a noncompliant author and/or committer identity, and 84 commits contain `Co-Authored-By: Claude` or `Claude-Session:` trailers. Current local Git identity is correct. The branch owner's scope clarification explicitly did not waive identity/trailer rules.

**Impact.** The candidate cannot satisfy repository provenance policy as committed.

**Minimum remediation.** The branch owner rewrites only the unmerged offending commits with the current configured identity, removes prohibited trailers, and preserves every tree. Do not change git config, set identity environment variables, or add replacement attribution trailers.

**Proof.** Compare pre/post tree IDs and audit the entire base..candidate log for exact author, committer, and forbidden trailer absence.

### FIND-admin-principals-R2-2 — NEW — INCORRECT — Important

**Contract.** REQ-013 requires authorization context to carry principal type; REQ-037 requires audit to identify the actual principal.

**Evidence and caller path.** `crates/shared/wyrd-runtime/src/principal.rs:362` defines `PlatformPrincipal` with only id and permissions. `crates/wyrd/wyrd-auth/src/platform_sessions.rs:47` defines `VerifiedPlatformSession` without kind, and both credential and federated confirmation discard the stored principal kind. `components/auth/platform_extractor.rs:44` therefore builds a kindless context. `wyrd-auth/src/platform_authz.rs:153` hard-codes `PrincipalKind::GlobalAdmin` into every decision event. Registered human administrators are stored as `User`.

**Impact.** Every human platform authorization is falsely audited as a global administrator, defeating principal-type attribution and investigations.

**Minimum remediation.** Propagate the already stored, verified platform-eligible principal kind through session, runtime context, and canonical audit event. Do not infer kind from permissions or introduce another context type.

**Proof.** Authenticate the deployment root and a federated human, perform the same operation, and assert distinct correct kinds in staged and published audit.

### FIND-admin-principals-R2-3 — NEW — MISSING — Important

**Contract.** REQ-037 and AC-009 require every authorization decision to identify the authenticating credential without exposing secret material.

**Evidence and caller path.** `PlatformCaller` has a credential ID, but `PlatformAuthorization` carries only context and loses it. Tenant access claims and `Caller` have no credential ID. `AuditEvent`, `vala.audit_staging`, its row type/hash, and the published `vala.system.audit_log` schema have no credential field. Consequently neither tenant API-key nor platform credential decisions can be attributed to the credential that authenticated them; federated absence is also not represented explicitly.

**Impact.** Rotation, compromise investigation, and per-credential revocation cannot distinguish two credentials for the same principal in the canonical audit history.

**Minimum remediation.** Add one optional, non-secret credential identifier through tenant token issuance/verification and platform caller authorization into the existing canonical `AuditEvent`, staging/hash, publisher, and retained audit schema. Federated sessions use `None`. Do not hide it in free-form detail or create another audit table.

**Proof.** Authenticate two credentials for one principal and assert distinct credential IDs in published records; assert federated records use `None` and no plaintext enters staging/log output.

### FIND-admin-principals-R2-4 — NEW — INCORRECT — Important

**Contract.** The card-free `TenantAdmin` credential exchange response must be usable for the advertised refresh-token lifecycle.

**Evidence and caller path.** API-key exchange mints a refresh token for a `TenantAdmin`. `crates/wyrd/wyrd-auth/src/refresh.rs:100+` rotates only `Service`/`Agent`; `User` remains unimplemented and all other kinds are invalid. The service path also requires a Card reference, which TenantAdmin intentionally lacks. `/auth/token` routes refresh grants through this implementation.

**Impact.** The server returns a refresh token that deterministically fails when used, breaking the shipped authentication contract after access-token expiry.

**Minimum remediation.** Extend the existing rotation match for card-free TenantAdmin, reusing current access-token issuance and `insert_refresh_token_rotated`. Do not add a parallel admin refresh service.

**Proof.** Real exchange -> refresh -> authenticated protected request for TenantAdmin, including replay rejection of the consumed refresh token.

### FIND-admin-principals-R2-5 — NEW — INCORRECT — High

**Contract.** REQ-028, AC-008, and INV-013 require tenant suspension/resume to affect the next request, including already-issued tokens.

**Evidence and caller path.** `crates/wyrd/wyrd-auth/src/revocation_resolver.rs` performs the tenant-admission read inside the five-second principal-epoch Moka cache. Platform suspension only updates the tenant row and publishes no tenant-wide cache invalidation. A previously cached active result therefore admits requests after suspension; a cached denial can also survive resume. The existing journey sets a zero TTL, so it cannot prove production behavior. The orchestrator's aggregate platform journey failed 1/28 at suspend/resume with `credential-revoked`, while the exact isolated rerun passed; that result is consistent with state/cache timing but is not needed to infer the source-level defect.

**Impact.** Suspension is not immediate and resume is nondeterministic for live tokens, violating the principal safety invariant on the public bearer path.

**Minimum remediation.** Move tenant admission outside the principal epoch cache on each request. This is smaller and safer than inventing tenant-wide invalidation messaging; reuse invalidation only if an existing mechanism already covers every server instance.

**Proof.** With a nonzero production-like epoch TTL, prime active, suspend and reject the next live-token request, resume and allow the next request.

### FIND-admin-principals-R2-6 — NEW — INCORRECT — High

**Contract.** INV-012 and AC-010 require credential failures to resist prefix/account-state enumeration with equivalent verifier work.

**Evidence and caller path.** `/auth/token` rejects malformed API keys during route parsing (`components/auth/routes.rs:65-70`) before Argon2. `wyrd-auth/src/exchange_api_key.rs:219+` rejects cross-tenant keys, a non-admitting tenant, unknown prefix, and disabled account before verifying the submitted secret, while a known live prefix with a wrong secret performs Argon2. The platform credential implementation already contains the fixed dummy-verifier pattern.

**Impact.** Remote timing distinguishes live tenant credential prefixes and account/tenant states.

**Minimum remediation.** Reuse the platform credential fixed-cost verifier pattern so every invalid tenant API-key condition performs exactly one Argon2 verification and returns the same stable public error. Avoid wall-clock padding and new crypto abstractions.

**Proof.** Instrument verifier invocation count for malformed, cross-tenant, suspended tenant, unknown prefix, disabled account, revoked/expired, and known-wrong secret; each must be exactly one with the same public response.

## Whole-branch-01 closure ledger

| Stable prior ID | Current status | Validation |
|---|---|---|
| `FIND-admin-principals-1` | **REOPENED / revised** | Canonical table fixed; same-plane platform atomicity remains. |
| `FIND-admin-principals-2` | **REOPENED** | Raw SQLx transaction and broad owner fields remain. |
| `FIND-admin-principals-3` | **REOPENED** | Most mappers fixed; three live leaking helpers remain. |
| `FIND-admin-principals-4` | **REOPENED** | Security/design authorities still contradict the shipped model. |
| `FIND-admin-principals-5` | **CLOSED** | Principal task setup/selectors are runnable and nonempty in the supplied evidence. |
| `FIND-admin-principals-6` | **CLOSED** | Stale comment/private rustdoc link fixed; lane widening remains rejected. |
| `FIND-admin-principals-7` | **CLOSED** | Human registration installs the fixed platform grant. |
| `FIND-admin-principals-8` | **REOPENED** | Usability query counts unpinned/stale identities. |
| `FIND-admin-principals-9` | **CLOSED** | Served client/CLI credential lifecycle exists and is exercised. |
| `FIND-admin-principals-10` | **CLOSED** | Runtime OIDC paths reuse screening/pinning. |
| `FIND-admin-principals-11` | **CLOSED** | Platform credential invalid paths perform fixed verifier work. |
| `FIND-admin-principals-12` | **CLOSED** | MCP journey performs and observes authorized credential revocation. |
| `FIND-admin-principals-13` | **REOPENED / revised** | Some response bodies improved; route/media/code coverage is still incomplete. |
| `FIND-admin-principals-14` | **CLOSED** | Exact MCP catalog and adjacent rustdoc include principal tools. |
| `FIND-TASK-001-10` | **REOPENED** | No valid waiver; noncompliant identities/trailers remain. |
| `FIND-003-2` | **REOPENED / revised** | Library initialization proof exists; actual binary contract is unproved. |
| `FIND-003-3` | **CLOSED** | Dead initialization vocabulary removed. |
| `FIND-004-2` | **CLOSED** | Served tenant list/inspect/suspend/resume exist. Immediate cache semantics are the distinct new `R2-5`. |
| `FIND-004-3` | **REOPENED / revised** | Slug adoption fixed; post-commit orphan credential remains. Prior `FIND-004-7` stays subsumed. |
| `FIND-004-4` | **REOPENED as proof for `FIND-004-3`** | Real failure/race proof improved, but does not cross the credential-commit/activation seam or assert credential cardinality. No duplicate remediation root. |
| `FIND-004-5` | **REOPENED / revised** | General CLI surface exists; total platform-credential-loss recovery and full operator path do not. Prior `FIND-005-3` and `FIND-008-7` remain folded here. |
| `FIND-005-2` | **CLOSED** | Product-path tenant-isolation journey exists. |
| `FIND-006-3` | **CLOSED** | Tenant recovery enforces active lifecycle state. |
| `FIND-006-4` | **CLOSED for credential routes** | Fail-closed append is proved there; issuer/binding/revoke transaction defects are retained under stable `FIND-005-1`. |

Older pre-whole-branch closures remain closed: `FIND-003-1`, `FIND-004-1`, `FIND-006-1`, `FIND-006-2`, `FIND-006-5`, `FIND-006-6`, `FIND-007-3`, `FIND-007-5`, `FIND-007-9`, the configuration-time portion of `FIND-007-4`, `FIND-008-3`, and `FIND-008-5`. `FIND-005-1` is reopened above because current source directly separates tenant allowances from their effects.

## Accepted non-findings and handoffs

- The verified-change-contract files and commits are authorized cumulative-candidate content. `FIND-admin-principals-R2-1` is rejected and must not appear in remediation as drift.
- `auth_e2e::cache_ttl_path_also_flips_verdict` reproduces at the base with `WYRD_AUTH_503_VERIFY_UNAVAILABLE` / `DelegateError::Database(_)`; it is pre-existing red, not candidate-attributable. It does not excuse the distinct source-proven tenant admission cache defect.
- `UNIQUE (data_tenant_id, name)` blocking same-named Cards across spaces remains a spec-owner handoff, not this change's remediation.
- The stale comment formerly at `wyrd-testing/src/server.rs:2670-2672` is fixed.
- The `rustdoc::private_intra_doc_links` warning is fixed. Adding `wyrd-sql` to a permanent rustdoc CI lane is a separate repository-cost decision and is not required here.
- A broad `mise run gate` is not required by VER-003. No finding is based solely on its absence.

## Verification evidence and limits

The orchestrator supplied passing evidence for formatting, client-tier boundary, unwrap audit, workspace lints, rustdoc, principal unit and integration lanes, MCP, CLI, `codegen:check`, and `docs:check`. The first platform journey aggregate passed 27/28 and failed the suspend/resume case with `credential-revoked`; its exact isolated rerun and a second complete aggregate rerun both passed (28/28). This demonstrates nondeterministic lane behavior rather than a deterministic journey failure. `FIND-admin-principals-R2-5` remains source-proven independently: tenant admission is inside a five-second cache and suspension publishes no invalidation, while the journey uses a zero TTL. Its remediation proof must therefore run with a nonzero production-like TTL.

This validator performed static inspection only as directed: no Cargo or mise command was run. Green lanes do not override source-level contradictions, missing public journey coverage, or repository provenance violations.
