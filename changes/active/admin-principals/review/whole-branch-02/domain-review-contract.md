# Public contract and journey review

## Subject and reviewed boundary

- Base: `c5c20754a167e8f4d74a555a720bd51df6179a6f`
- Candidate: `5293546f33b3a5fd9de529098e23ea70d472c412`
- Approved authority: `changes/active/admin-principals/spec.md`, revision 7
- Boundary: HTTP and generated OpenAPI, shared Rust client/Rust SDK projection,
  CLI, MCP catalog and dispatch, operator documentation, stable public errors,
  and the real-server journeys intended to prove those surfaces.

I inspected the complete cumulative range, the current source and generated
artifacts, all original tasks, and the prior `whole-branch-01` findings and
remediation record. The candidate remained at the pinned commit throughout the
review. I did not run Cargo or mise commands, as assigned.

## Material findings

### Important

#### FIND-admin-principals-4 (reopened) — MISSING — the required security authority still excludes the shipped administrative principals

- **Violated obligation:** the spec's **Required architecture amendments**;
  `AC-013`; and `AGENTS.md` sections 1–2, which make the architecture documents
  authoritative over implementation drift.
- **Exact location:** `architecture/wyrd-security-posture.md:40-50`, compared
  with `architecture/wyrd-design.md:141-157` and
  `crates/wyrd-spec/src/auth/principal_kind.rs:18-42`.
- **Evidence:** the security posture still says `PrincipalKind` is closed to
  `User`, `Service`, `Agent`, and `System`, that the sole runtime principal has
  a tenant, and that Service and Agent principals carry a Card. The approved
  implementation exposes `GlobalAdmin` and `TenantAdmin`, a tenant-free
  platform projection, and Card-free Service automation. The current design
  document describes those additions, but the required security authority does
  not. This is the same prior finding, not a new stylistic disagreement; the
  remediation record marked the architecture correction complete even though
  the current candidate still contains the contradictory closed set.
- **Observable consequence:** security reviewers and later implementers are
  directed to reject principal kinds and trust boundaries that the wire and
  server now depend on. The repository therefore still describes more than one
  administrative identity model.
- **Minimal testable correction:** amend the existing principal-lifecycle
  section in `wyrd-security-posture.md` to the revision-7 two-plane model,
  preserving the independently approved internal `System` kind. Do not add a
  parallel explanation. Prove closure with a stale-model search and
  `mise run docs:check`.

#### FIND-admin-principals-13 (reopened and revised) — INCORRECT — the generated contract still omits live administrative/authentication routes and does not publish the promised error contract

- **Violated obligation:** `REQ-029`, `REQ-036`, `AC-013`, `AC-014`, and
  `VER-006`; task 008 requires an independent client to implement the
  administrative flow from the generated artifact, including its
  authentication scheme, typed bodies, and stable errors.
- **Exact location:**
  `crates/wyrd/wyrd-server/src/http/openapi.rs:79-149`,
  `crates/wyrd/wyrd-server/src/components/auth/routes.rs:45`,
  `crates/wyrd/wyrd-server/src/components/admin/routes.rs:55-70,207-226,282-312,363-448`,
  and generated `openapi.yaml:12-117,1399-1635,5021-5054`.
- **Evidence:** the live router serves tenant credential exchange at
  `/auth/token` and tenant OIDC/workload administration at
  `/v1/admin/trusted-issuers` and `/v1/admin/workload-bindings`. None appears in
  `WyrdApiDoc` or `openapi.yaml`; the admin handlers have no `utoipa` operation
  annotations. A client generated only from the artifact therefore cannot
  obtain the tenant access token needed for tenant administration or implement
  the tenant OIDC configuration required by `REQ-029`. For the administrative
  paths that are registered, `body = WyrdProblem` generates
  `application/json`, not the repository's `application/problem+json`
  boundary, and route responses do not declare which stable `WYRD_*` code they
  return—the shared schema's `code` is only an unconstrained string.
  `every_authenticated_path_declares_the_one_wyrd_scheme` iterates only paths
  already admitted to `WyrdApiDoc`, so it cannot detect a served route omitted
  from the contract. The source has no administrative counterpart to the
  adjacent Bifrost problem-response completeness test.
- **Observable consequence:** the supposedly language-agnostic artifact is not
  sufficient to build the approved tenant-admin client. Generated clients miss
  whole served capabilities and cannot select stable errors by operation; they
  also negotiate the wrong problem media type on the administrative paths that
  are present.
- **Minimal testable correction:** annotate and register the existing tenant
  token, trusted-issuer, and workload-binding handlers in the one OpenAPI owner;
  reuse the existing shared `WyrdProblem` response mechanism that emits
  `application/problem+json`; and publish the route-specific stable codes
  without inventing a second catalog. Add one source test that compares the
  complete served administrative/authentication route set with the OpenAPI set
  and validates typed problem responses and stable codes. Regenerate through
  `mise run codegen:check` and retain the existing real-server CLI assertions.

#### FIND-004-5 (reopened and revised) — MISSING — the operator contract still has no total-platform-credential recovery and its journey bypasses the shipped init command

- **Violated obligation:** `REQ-020`, `REQ-033`, `REQ-040`, `AC-001`, and
  `AC-002`. The approved behavior explicitly requires recovery after *every*
  global credential is lost through deployment-level database/secret-store
  authority, and requires the shipped three-command operator journey to begin
  with `wyrd-server init`.
- **Exact location:**
  `docs/src/content/docs/self-hosting/authentication.svx:73-86`,
  `docs/src/content/docs/self-hosting/running-the-server.svx:112-143`,
  `crates/wyrd/wyrd-server/src/main.rs:37-85`, and
  `crates/wyrd/wyrd-cli/tests/operator_journey.rs:49-60`.
- **Evidence:** the docs state that a platform credential is recovered by
  “another live platform credential, and nothing else” and that it “has no such
  backstop.” They describe only ordinary rotation while at least one platform
  credential remains. That directly contradicts `REQ-033`'s deployment-level
  recovery requirement, and there is no operator recovery subcommand beside
  the one-shot `init`, which refuses an initialized deployment. The claimed
  end-to-end binary journey also calls `initialize_platform_root` in-process at
  line 57 instead of executing `wyrd-server init`, so argument parsing,
  operator-only connection setup, one-time stdout, exit behavior, and secret
  exclusion from diagnostics are not exercised through the shipped command.
  The same journey creates a tenant and restricted principal but never executes
  tenant configuration/OIDC configuration, despite `AC-002` naming that step.
- **Observable consequence:** an operator who loses all platform credentials
  has no documented or shipped approved recovery path, and the branch's primary
  CLI proof can stay green even if the actual init binary is unusable or leaks
  its one-time credential at the process boundary.
- **Minimal testable correction:** add the approved deployment-authorized
  recovery operation beside `wyrd-server init`, reusing the existing
  initialization-class `OperatorPool` authority and platform credential issuer
  to issue once for the existing root principal without resetting grants or
  creating another principal. Document that path in the current recovery
  section. Extend the existing journey to invoke the actual `wyrd-server`
  binary for init and total-loss recovery and to perform the tenant
  configuration step through the shipped CLI. The focused proof must assert
  one-time stdout, stable repeat refusal, unchanged principal/grants, and no
  secret in stderr or captured server diagnostics.

#### FIND-admin-principals-R2-1 — DRIFT — a separate Verifier/Card-kind specification and authority rewrite entered the cumulative admin-principals candidate

- **Violated obligation:** the admin-principals non-goal that doctrine's Card
  kinds remain unchanged, the review skill's complete-diff/no-unrelated-change
  PASS condition, and the task boundary established by `SPEC-admin-principals`.
- **Exact location:** `AGENTS.md:66-75`,
  `architecture/wyrd-doctrine.mdx:76-85,133-137`, broad Verifier changes in
  `architecture/wyrd-design.md`, and the 16 files under
  `changes/active/verified-change-contract/` added or modified by commits
  `63bc79127` and `5293546f3`.
- **Evidence:** the candidate changes the repository-wide Card catalog from 16
  native kinds with registrable Drift/Eval to 15 kinds with `Verifier`, rewrites
  the verification architecture, advances another approved specification to
  revision 32, and adds eight implementation task packets. Those 2,262 added
  lines and their authority changes implement planning for
  `SPEC-verified-change-contract`; none is required to deliver administrative
  principals. The admin spec explicitly says its Card-kind set is unchanged.
- **Observable consequence:** the branch cannot be reviewed, merged, or
  reverted as one admin-principals change: accepting it also approves an
  unrelated Card protocol redesign, while failures or later revision of that
  redesign block the admin delivery.
- **Minimal testable correction:** remove the two unrelated
  verified-change-contract commits from this candidate and carry them on their
  own change branch. Preserve only admin-principals architecture hunks where a
  file overlaps. Verify the resulting base-to-candidate name/status diff has no
  `changes/active/verified-change-contract/**` paths and no Card-kind change
  attributable to this task.

## Prior contract-finding closure

| Prior finding | Current result | Evidence |
|---|---|---|
| `FIND-admin-principals-4` | **OPEN** | `wyrd-security-posture.md:40-50` still carries the prohibited closed model. |
| `FIND-004-5` | **OPEN** | CLI/platform commands and most operator prose landed, but total platform-root recovery is contradicted and the binary init path is bypassed by the journey. |
| `FIND-admin-principals-12` | **CLOSED** | `wyrd-mcp/tests/bifrost/mcp/principals.rs` performs discover → authorized revoke → observed retirement and retains the under-scoped refusal. |
| `FIND-admin-principals-13` | **OPEN** | Revoke now honors its typed request and records the reason, but the generated administrative/auth contract is still incomplete and its problem response contract remains wrong. |
| `FIND-admin-principals-14` | **CLOSED** | MCP catalog source and exact discovery/connectivity expectations include the shipped principal tools in order. |
| prior stale-doc/rustdoc contract handoff | **CLOSED** | Current fixture wording and rustdoc source no longer contain the candidate-attributable stale statements. |

## Surface and proof coverage

| Boundary | Result | Reviewed evidence |
|---|---|---|
| Shared Rust client and Rust SDK | **PASS with downstream contract limits** | `wyrd-client::{platform,principals}` owns the transport; the Rust SDK keeps its thin re-export. |
| CLI transport consolidation | **PASS** | Wyrd routes go through `wyrd-client`; the remaining external eval call uses the shared external-stream seam rather than constructing a CLI client. |
| CLI/operator workflow | **FAIL** | Tenant lifecycle, credential rotation, and recovery commands exist, but total platform-root recovery and actual binary init proof do not. |
| MCP catalog and dispatch | **PASS** | Read discovery and scoped write dispatch use the server owners; the real-server journey performs and observes the write. |
| HTTP/OpenAPI | **FAIL** | Several served authentication/admin routes are absent; administrative errors do not publish the required media type or per-operation stable codes. |
| Architecture/documentation | **FAIL** | Security authority contradicts the shipped principal model; operator recovery prose contradicts the approved loss path. |
| Scope discipline | **FAIL** | The cumulative branch includes the separate verified-change-contract specification, tasks, and repository authority rewrite. |

## Verification limits

- I did not run tests or builds. The prior remediation record reports focused
  lanes green, but those lanes cannot prove routes absent from `WyrdApiDoc` and
  the CLI journey's source demonstrably calls the init library function instead
  of the server binary.
- `mise run test:cli:journey` uses `cargo test` for the whole CLI target. The
  implementation record does not include the exact `nextest` expression for
  the named operator journey required by `VER-002`; the orchestrator should
  treat that as missing focused evidence even if the aggregate target passes.
- No broad aggregate is required or appropriate evidence here because approved
  revision 7 `VER-003` expressly excludes it.

## Overall result

**FAIL**

The MCP and most CLI/client remediation is real, but the public contract is not
complete: an authority still rejects the implemented identity model, live admin
routes remain absent from OpenAPI, total platform-root recovery is neither
shipped nor documented, the binary init journey is bypassed, and an unrelated
Card protocol redesign remains in the cumulative candidate.
