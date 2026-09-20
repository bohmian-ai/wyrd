# HTTP, client, CLI, MCP, and generated-contract domain review

## Immutable subject and reviewed boundary

- Base: `c5c20754a167e8f4d74a555a720bd51df6179a6f`
- Candidate: `a9a7c9c1e8502ccf3befa74c4283c78e5d70c132`
- Approved authority: `changes/active/admin-principals/spec.md`, revision 7;
  `TASK-003` through `TASK-008`; both prior whole-branch verdicts and their
  remediation packets; `AGENTS.md`; `architecture/agent-rules.md`;
  `architecture/wyrd-design.md`; `architecture/wyrd-doctrine.mdx`; and the
  applicable language, HTTP, error, agent-harness, and testing references.
- Boundary: real `wyrd-server` initialization and root recovery; served HTTP
  administration and authentication routes; generated OpenAPI and public DTOs;
  shared Rust client; operator CLI and documentation; MCP discovery, scope
  gating, dispatch, and observation; credential refresh; and the real-server
  journey sources and lane selectors that claim these surfaces.

The candidate was still checked out at the exact commit above at the end of
inspection. This review made no product-code changes and ran no Cargo or `mise`
command.

**Overall result: FAIL.** The binary initialization/recovery, MCP subset,
credential-refresh journey, canonical-header consolidation, and much of the
route registration are real. Four material contract-boundary defects remain.

## Review Findings

### Critical

None.

### Important

#### CONTRACT-R3-1 — INCORRECT — `FIND-admin-principals-13` is only partially closed: OpenAPI still does not publish the required typed, stable administrative contract

- **Violated obligation:** specification `REQ-036`, `AC-013`, and `AC-014`;
  `AGENTS.md` section 9. An independent client must be able to implement every
  administrative path from typed bodies, declared stable errors, and the
  generated authentication scheme.
- **Exact evidence:**
  - `crates/wyrd/wyrd-server/src/http/openapi.rs:328-365` applies the stable-code
    assertion only when an operation is tagged `Auth` or `Admin`. Lines 354-356
    explicitly skip the newly registered `Platform` and `Principals` operations.
    The adjacent comment at lines 320-322 acknowledges that the stable-code half
    covers only the tags whose descriptions already carry codes.
  - The route annotations consequently publish generic failures without a
    stable code, including `components/platform/routes.rs:71-79,119-130`,
    `components/platform/identity.rs:166-177,259-268`,
    `components/principals/routes.rs:234-245,315-326,351-362`, and
    `auth/revoke.rs:41-54`. The generated Platform paths at
    `openapi.yaml:289-844` and Principals paths at `openapi.yaml:1888-2100`
    preserve those code-free descriptions. This is not merely a weak test:
    `platform_token` can return its not-configured/internal path at
    `components/platform/routes.rs:86-110`, while its operation declares only
    success and 401 at lines 75-78.
  - Public response types are not fully typed. `crates/wyrd-spec/src/auth/admin.rs:100-121`
    declares `TrustedIssuerView.claim_mapping`, `group_role_map`, and
    `default_roles` as `serde_json::Value`; lines 143-152 declare
    `WorkloadBindingView.card_ref` as `Value`, even though the corresponding
    input types already use `ClaimMappingPayload`, concrete map/vector shapes,
    and `CardRef`. The generated properties at `openapi.yaml:5576-5590` and
    `:5904-5905` have descriptions but no `type`, `$ref`, or constrained shape.
  - `crates/wyrd/wyrd-server/src/auth/revoke.rs:21-24` still says an allowed
    audit row commits before the revocation. The implementation now appends,
    revokes, and commits on one connection at lines 86-93. Because utoipa copies
    the rustdoc, `openapi.yaml:2067-2073` publishes the obsolete, opposite
    transaction contract.
- **Reachability and consequence:** all named operations are registered in
  `WyrdApiDoc` (`http/openapi.rs:141-229`) and served by production routers.
  Generated clients cannot enumerate the promised stable errors for Platform or
  Principals operations, cannot produce typed issuer/binding response models,
  and are told an obsolete revocation durability rule. Passing the current
  OpenAPI test does not prove `AC-014` because the exact missing tags are skipped.
- **Required testable correction:** keep the existing DTO/OpenAPI owners. Replace
  the response-side `Value` fields with the already-existing concrete contract
  types, correct the revoke documentation, and annotate each reachable
  Platform/Principals failure with its actual catalog code. Expand the existing
  generator test—not a second catalog or checker—so every administrative tag is
  subject to the stable-code assertion, then regenerate and prove the generated
  schemas and descriptions.

#### CONTRACT-R3-2 — VIOLATION — `/auth/issue-key` still commits an allowed authorization audit separately from the authorized effect

- **Violated obligation:** specification `REQ-037` and `AC-009`; `AGENTS.md`
  section 2's canonical rule that an authorization decision is transactionally
  audited in the transaction that made it. This is the same root contract as
  prior `FIND-005-1`, not a request for another audit mechanism.
- **Exact evidence:** `crates/wyrd/wyrd-server/src/components/auth/routes.rs:365-378`
  authorizes `service_accounts:write` and calls `audit::record_audit` against the
  Vala pool. That helper durably commits independently. Only afterwards do
  lines 379-393 acquire the tenant transaction, issue the API key, append the
  issuance record, and commit the effect.
- **Reachability and consequence:** `auth_router` serves `POST /auth/issue-key`,
  and `crates/wyrd/wyrd-cli/tests/auth_issue_key_journey.rs:40-130` reaches it
  through the CLI/shared-client path. If tenant acquisition, issuance, issuance
  audit, or final commit fails after line 378, the durable record says the
  permission was allowed although no key was issued. The later
  `IssueApiKey::audit` call records issuance lineage; it cannot make the earlier
  authorization decision atomic retroactively.
- **Required testable correction:** reuse the `TenantConn`/`audit::append_on`
  pattern already used by principal writes (`components/principals/routes.rs:262-304`)
  so the allowed decision, issuance, issuance record, and effect commit once.
  Preserve the standalone durable denied-decision path. Add an injected
  post-authorization issuance failure proving no allowed audit remains when the
  key effect does not commit.

#### CONTRACT-R3-3 — VIOLATION — the tenant issuer secret reintroduces the exact plain-`String`/derived-`Debug` hazard previously closed for platform OIDC

- **Violated obligation:** `AGENTS.md` section 4 requires `SecretString` for
  secrets and redacted custom `Debug` implementations for secret-bearing
  structs; specification `INV-002` excludes secret material from diagnostics.
- **Exact evidence:** `crates/wyrd-spec/src/auth/admin.rs:65-80` derives `Debug`
  for `CreateTrustedIssuerRequest` while storing `client_secret` as
  `Option<String>`. The shipped CLI repeats the shape in
  `crates/wyrd/wyrd-cli/src/auth/trusted_issuer.rs:28-50`, where `AddArgs`
  derives `Debug` and stores the same secret as `Option<String>`; its resolver
  returns another `Option<String>` at lines 259-278.
- **Reachability and consequence:** `wyrd auth trusted-issuer add` accepts the
  secret from a flag, file, or environment and constructs the public request.
  Current handler instrumentation skips the request, but formatting either
  public request or parsed CLI arguments reveals the secret. This is the same
  failure mode prior `FIND-007-9` corrected for `PlatformClientAuth`, while
  `crates/wyrd-spec/src/auth/secret_bearer.rs:9-39` already provides the
  string-compatible, schema-marked, redacted owner.
- **Required testable correction:** reuse `SecretBearer` (and `SecretString` at
  internal boundaries) for the issuer wire secret, and ensure the CLI argument
  holder has redacted `Debug` rather than deriving secret-revealing output.
  Prove both request and CLI debug formatting exclude a sentinel secret; add no
  new secret wrapper.

#### CONTRACT-R3-4 — MISSING — the approved CLI projection and operator journey still omit administrative configuration paths

- **Violated obligation:** specification `REQ-036`, `REQ-040`, `AC-002`, and
  `AC-014`; TASK-004's required real-server operator path; TASK-008's requirement
  that the CLI perform administrative operations against a real server.
- **Exact evidence:**
  - The served and generated contract includes platform OIDC connection
    configure/read/remove and platform administrator register/list/status
    (`components/platform/identity.rs:161-367,436-633`; generated paths
    `openapi.yaml:289-638`). `wyrd_client::Platform` projects configure/read and
    register/list/status at `crates/shared/wyrd-client/src/platform/handle.rs:186-251`
    but has no DELETE connection operation. The CLI's complete platform command
    enum at `crates/wyrd/wyrd-cli/src/platform/mod.rs:18-38` contains only
    `Credential` and `Tenant`, so none of those platform-identity operations is
    an operator command.
  - The real-server CLI journey
    `crates/wyrd/wyrd-cli/tests/operator_journey.rs:49-224` creates and inspects
    a tenant, creates/rotates a restricted principal, changes tenant lifecycle,
    and recovers its administrator. It never performs the explicit `AC-002`
    tenant-configuration/OIDC step. There is no real-server CLI test for the
    existing `auth trusted-issuer` or `auth workload-binding` commands; their
    coverage in the command modules is parser/unit/mock-server coverage.
  - Operator documentation has the same hole: the tenant-administration commands
    at `docs/src/content/docs/self-hosting/running-the-server.svx:81-110` create
    and rotate a principal but do not configure tenant OIDC. The page also says
    only `init` exists at line 11 and omits `recover-root` from the command-line
    synopsis at lines 15-25, while correctly describing the shipped
    `recover-root` later at lines 44 and 112-154.
- **Reachability and consequence:** operators cannot execute the newly served
  platform human-identity administration through the designated CLI surface,
  and the claimed three-command journey never performs its configuration step.
  A green `test:cli:journey` lane therefore cannot establish `AC-002` or the
  stated CLI half of `AC-014`.
- **Required testable correction:** project the existing platform identity
  routes through the existing `wyrd-client::Platform` owner and CLI transport;
  do not add another transport or lifecycle service. Extend the existing
  real-server operator journey to configure tenant OIDC through the shipped CLI
  and cover the platform identity commands required by the approved contract.
  Correct the existing operator page, including its recovery synopsis, rather
  than adding a new page.

### Suggestions

None. Optional cleanup and unrelated refactoring are intentionally excluded.

## Authority and acceptance coverage

| Boundary / obligation | Source inspected | Result |
|---|---|---|
| Real binary `init`, repeat refusal, stdout/stderr secrecy | `wyrd-server/src/main.rs:24-105`; `boot/init.rs:100-147`; `platform_admin_e2e.rs:172-269` | **PASS** — real process proof exists and credential material is isolated to stdout. |
| Total platform-credential-loss recovery | `main.rs:31-39,86-105`; `boot/init.rs:186-213`; `platform_admin_e2e.rs:271-439`; operator docs | **PASS behavior / documentation residue in CONTRACT-R3-4** — same root principal is recovered through the real binary. |
| Served administrative route registration and auth scheme | `http/openapi.rs:11-229`; router owners; generated paths | **PASS for route presence and security scheme**. |
| Typed bodies and stable errors sufficient for an independent client | DTOs, utoipa annotations, generator tests, `openapi.yaml` | **FAIL — CONTRACT-R3-1**. |
| Every Wyrd-owned CLI HTTP caller uses `wyrd-client` | CLI command modules and shared handles | **PASS** — no second Wyrd transport implementation was found. |
| CLI projects and proves the approved operator/admin paths | CLI command tree, `operator_journey.rs`, `mise.toml:154-163`, docs | **FAIL — CONTRACT-R3-4**. |
| MCP reads discoverable; write requires explicit scope; direct invocation re-authorizes | MCP descriptors/handler and `wyrd-mcp/tests/bifrost/mcp/principals.rs:34-277` | **PASS** — list is catalog-visible, revoke is scope-gated, unauthorized direct dispatch is refused, and successful revoke is observed. No platform MCP tools or secret-returning issuance are required by the approved narrowed subset. |
| Canonical authorization audit and fail-closed coupling | tenant principal routes, auth issue-key, prior remediation | **FAIL — CONTRACT-R3-2**. |
| Secret-safe public and CLI contracts | platform and tenant issuer DTOs; CLI argument types | **FAIL — CONTRACT-R3-3**. |
| Tenant-admin credential refresh after rotation | `platform_admin_e2e.rs:3733-3838` and refresh owner | **PASS on source proof** — replacement authenticates, the protected request succeeds, and replay is rejected. |

## Prior-finding disposition

| Prior finding | Disposition in this review |
|---|---|
| `FIND-admin-principals-9` | **CLOSED.** Platform credential issue/list/revoke remains served, projected through the shared client/CLI, and journey-covered. |
| `FIND-admin-principals-12` | **CLOSED.** The MCP journey now discovers, successfully performs, and observes the approved credential revocation. |
| `FIND-admin-principals-13` | **OPEN / REVISED by CONTRACT-R3-1.** Route registration and problem media are corrected; stable-code coverage deliberately skips Platform/Principals, response schemas retain unconstrained JSON, and generated revoke prose is stale. |
| `FIND-admin-principals-14` | **CLOSED.** The MCP catalog expectations include the principal tools without weakening order/exactness. |
| `FIND-003-2` | **CLOSED for the previously reported binary gap.** The real `wyrd-server init` and `recover-root` processes are exercised. |
| `FIND-004-5` | **OPEN / REVISED by CONTRACT-R3-4.** Root recovery and the tenant/credential CLI landed, but the required configuration step, platform identity CLI projection, and fully consistent operator prose did not. |
| `FIND-005-1` | **OPEN / REVISED by CONTRACT-R3-2.** Issuer, binding, and principal-revoke coupling was corrected; the reachable issue-key authorization path retains the same separate-commit defect. |
| `FIND-007-9` | **CLOSED for `PlatformClientAuth`, but the same rule is violated on the tenant issuer path (CONTRACT-R3-3).** Wave 2 should decide whether to reopen the stable ID or allocate the next ID. |
| Whole-branch-02 credential-refresh finding | **CLOSED.** The real-server refresh/rotation/replay evidence exists in current source. |

## Open Questions

None. Each correction above reuses an existing owner and remains within approved
behavior; no specification decision is required.

## Verification Notes

- Per the review assignment, no Cargo, nextest, rustdoc, codegen, docs, or
  `mise` task was run. Results quoted by prior implementation records were not
  treated as proof of the missing properties.
- Source inspection confirms the exact candidate is checked out. Other Wave 1
  reports in `whole-branch-03` are concurrent review artifacts and are outside
  this report's product-diff judgment.
- The configured `test:cli:journey` lane at `mise.toml:154-163` runs the CLI
  integration target with the repository Postgres wrapper; the defect is that
  its current test source never selects the omitted behaviors, not that its
  selector can select zero tests.
- The previously reported `auth_e2e::cache_ttl_path_also_flips_verdict` failure
  is accepted as base-reproducible and is not a finding here. The same-named
  Card uniqueness issue and the rustdoc-lane widening decision remain explicit
  owner handoffs, not this review's findings.
- The user-approved verified-change-contract content is in scope and accepted.
  `FIND-TASK-001-10` is explicitly waived in full and is not a finding.

## Verdict

**FAIL** — bounded implementation remediation is required for
`CONTRACT-R3-1` through `CONTRACT-R3-4`.
