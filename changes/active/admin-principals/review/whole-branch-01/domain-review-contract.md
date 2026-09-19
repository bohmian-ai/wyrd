# Contract and public-surface domain review

## Subject

- Repository: `/Users/stevenforrester/Documents/GitHub/wyrd-bifrost-surfaces`
- Base: `c5c20754a167e8f4d74a555a720bd51df6179a6f`
- Candidate: `072cf8b30c7135e8cf15f92da3e371a9c999703c`
- Approved authority: `changes/active/admin-principals/spec.md`, revision 7
- Tasks: `TASK-001` through `TASK-008` and their tracked remediation artifacts
- Reviewed boundary: public wire contracts, HTTP/OpenAPI, `wyrd-client`, CLI,
  MCP, Rust SDK projection, generated artifacts, operator documentation, and
  the user journeys that are supposed to prove those surfaces.

The candidate commit was still `072cf8b30c7135e8cf15f92da3e371a9c999703c`
after inspection.

## Material findings

### Important

#### CONTRACT-1 — MISSING — the change ships an identity model that its design authorities still prohibit

- **Violated obligation:** the spec's **Required architecture amendments** are
  part of the approved change; `AGENTS.md` §§1–2 make
  `architecture/wyrd-design.md` the active design authority; `AC-013` requires
  one administrative identity model across documentation and contracts.
- **Location:** `architecture/wyrd-design.md:145-156`,
  `architecture/wyrd-security-posture.md:40-48`,
  `architecture/v1/00-foundations/service-identity.md:1-15`,
  `architecture/v1/00-foundations/rbac-crud.md:8-16`, and
  `changes/active/tenant-oidc-federation/spec.md:95-100`. The complete
  base-to-candidate architecture diff is empty.
- **Evidence:** the active design still closes `PrincipalKind` to `User`,
  `Service { card_ref }`, and `Agent { card_ref }`, requires Service/Agent
  identities to be Card-bound, and gives every principal a `tenant_id`.
  `wyrd-security-posture.md` repeats that model. The OIDC spec still says every
  human OIDC connection belongs to exactly one tenant. The candidate instead
  adds `GlobalAdmin` and `TenantAdmin`, Card-free service principals,
  platform-scope principals with no tenant, and a deployment-owned platform
  OIDC connection.
- **Observable consequence:** the repository's highest design authority tells
  the next implementer to reject or remove behavior this branch exposes on the
  wire. Client authors and security reviewers cannot determine whether the
  implementation or the architecture is authoritative, so `AC-013` cannot
  pass.
- **Required testable correction:** update the named owning architecture and
  foundation documents, plus the tenant-OIDC spec handoff, to describe exactly
  the approved revision-7 model. Do not add a second explanation or migration
  vocabulary. Prove closure with a tree-wide stale-model search and
  `mise run docs:check`; retain the existing contract/codegen checks.

#### CONTRACT-2 — MISSING — the shipped CLI and MCP cannot perform or prove the approved administrative workflow

- **Violated obligation:** `REQ-036`, `REQ-040`, `AC-002`, and `AC-014` require
  the administrative contract to be projected and exercised by the CLI and
  MCP against a real server. `AGENTS.md` §11 and design doctrine 20 require an
  agent surface journey to discover, act, and observe.
- **Location:** `crates/wyrd/wyrd-cli/src/cli.rs:55-84`,
  `crates/wyrd/wyrd-server/src/mcp/principals.rs:32-57`, and
  `crates/wyrd/wyrd-mcp/tests/bifrost/mcp/principals.rs:55-154`.
- **Evidence:** the complete CLI command enum has no platform, tenant-create,
  tenant-recovery, service-principal-create, credential-list, credential-issue,
  or credential-revoke command; `Principal` exposes revoke only. Therefore the
  `init -> create tenant -> configure tenant/create restricted principal`
  operator path in `AC-002` cannot be executed through the shipped CLI even
  though `wyrd-client::Platform` and `Principals` already own most transport.
  MCP exposes only `principals.list_credentials` and
  `principals.revoke_credential`. Its sole journey verifies catalog visibility,
  a denied write, and an admin read; it never invokes a permitted revoke and
  therefore never completes discover -> act -> observe for its only write.
- **Observable consequence:** an operator must fall back to hand-written HTTP
  for the primary provisioning workflow, and the branch has no evidence that
  an authorized MCP agent can complete even the write it advertises. The task's
  claim that the administrative contract is reachable and proved through CLI
  and MCP is false.
- **Required testable correction:** reuse the existing `wyrd-client::Platform`
  and `Principals` handles to expose the approved operator commands; do not add
  another transport. Add one real-server CLI journey for the exact `AC-002`
  workflow. Extend the existing MCP journey so an authorized caller revokes a
  real credential and observes it retired, while retaining the under-scoped
  refusal. If revision 7 intentionally meant only a subset of administrative
  operations to exist on MCP, that subset must be made explicit by spec
  authority before implementation; the current `AC-014` says the CLI and MCP
  exercise the administrative operations.

#### CONTRACT-3 — INCORRECT — the generated OpenAPI contract omits typed administrative errors and contradicts the principal-revocation contract

- **Violated obligation:** `REQ-036` and `AC-014` require every administrative
  path, typed bodies, stable error codes, and its authentication scheme in the
  generated OpenAPI document. `languages/errors.md` requires HTTP failures to
  project the shared `WyrdProblem`/catalog contract.
- **Location:** `crates/wyrd/wyrd-server/src/components/principals/routes.rs:203-214`
  and the other administrative `#[utoipa::path]` response lists;
  `crates/wyrd/wyrd-server/src/auth/revoke.rs:29-45`;
  `crates/wyrd-spec/src/auth/revoke.rs:10-46`;
  `crates/shared/wyrd-client/src/principals/handle.rs:133-145`;
  `crates/wyrd/wyrd-server/src/http/openapi.rs:170-214`; generated
  `openapi.yaml` administrative paths.
- **Evidence:** every administrative error response in `openapi.yaml` has only
  a description and no `application/problem+json` body. The existing OpenAPI
  regression test checks authentication only; the adjacent Bifrost test at
  `http/openapi.rs:216-262` demonstrates the repository mechanism that asserts
  `WyrdProblem` bodies. Principal revocation is worse: the public
  `RevokePrincipalRequest` requires `principal_kind` and an audit `reason`, the
  shared client sends it, but the server handler has no `Json` extractor and
  ignores the body; OpenAPI declares no request body. `RevokePrincipalResponse`
  declares a detailed response while the server and client use an empty unit
  response. The CLI journey passes precisely because no assertion checks that
  its kind/reason reached the server.
- **Observable consequence:** an independent client generated from
  `openapi.yaml` cannot type or reliably decode an administrative refusal and
  will omit the supposedly required revocation body. A caller can send an empty
  body or an invalid/empty reason and still revoke, while generated schemas and
  the CLI claim the opposite.
- **Required testable correction:** use the existing `WyrdProblem` annotation
  pattern for every administrative refusal and add one OpenAPI source test that
  enumerates the administrative operations and proves their problem bodies.
  Make principal revocation one contract end to end by honoring the existing
  typed request (including its validation/audit semantics) and response, or by
  routing a deliberate compatibility change through spec authority; do not
  retain unused request/response schemas beside a bodyless route. Regenerate
  with `mise run codegen:check` and add a real HTTP/client assertion that an
  invalid or missing required body is refused and a valid body is reflected in
  the operation's observable/audit result.

#### CONTRACT-4 — INCORRECT — operator documentation neither covers `REQ-040` nor describes the shipped principal model

- **Violated obligation:** `REQ-040` requires the three-command operator
  journey, the SaaS custody model, credential rotation, and both credential-loss
  recovery paths; `AC-013` requires documentation to describe one identity
  model.
- **Location:** `docs/src/content/docs/self-hosting/authentication.svx:13-30,73-75`,
  `docs/src/content/docs/self-hosting/local-development.svx:30-45`, and
  `docs/src/content/docs/self-hosting/running-the-server.svx:31-46`.
- **Evidence:** the authentication page still says the complete principal set
  is User/Service/Agent and that every Service/Agent is Card-bound. It omits
  `global_admin`, `tenant_admin`, Card-free automation, and platform scope. The
  docs contain no executable first-tenant/restricted-principal sequence, no
  SaaS statement that Wyrd retains the global credential, no overlap rotation
  procedure, and no distinct tenant-admin versus deployment-level global
  recovery procedure. Local-development prose still calls `init` a bootstrap
  API-key mint and shows the obsolete `wyrd_sk_acme_…` output while
  `running-the-server.svx` shows `wyrd_global_…`.
- **Observable consequence:** a self-hosted operator cannot complete the
  approved workflow from the docs and is taught a principal/credential model
  that the server no longer implements. The stale recovery sentence (“issue a
  replacement”) also omits that loss of every global credential is database
  and secret-store operator recovery only.
- **Required testable correction:** consolidate the operator workflow in the
  existing self-hosting pages, using commands that actually exist after
  `CONTRACT-2`; state the SaaS custody boundary, overlap rotation, tenant-admin
  recovery, and separate global recovery. Replace the stale principal and
  prefix prose rather than adding a parallel page. Run `mise run docs:check`
  and a targeted stale-vocabulary search.

### Suggestions

#### CONTRACT-5 — DRIFT — two branch-adjacent comments now describe mechanics the candidate replaced

- **Violated obligation:** `AC-013` requires one model across surfaces and
  generated/documented contracts; repository Rust documentation must explain
  current intent and operation rather than stale mechanics.
- **Location:** `crates/wyrd/wyrd-testing/src/server.rs:2670-2671` and
  `crates/wyrd/wyrd-sql/src/queries/auth/service_accounts.rs:174-179`.
- **Evidence:** the test helper says its UID-less principal `card_ref` exists
  for an “exact JSONB lookup”, while this branch changed the lookup to JSONB
  containment (`card_ref @> $3`). Separately, public rustdoc links to the private
  `SERVICE_ACCOUNT_BY_CARD_REF_SQL`; `mise exec -- cargo doc --locked -p
  wyrd-sql --no-deps` emits `rustdoc::private_intra_doc_links` at line 176.
- **Observable consequence:** maintainers are told the opposite predicate at
  the fixture seam, and generated public rustdoc renders the private-item link
  as an unresolved-looking literal. Neither changes runtime behavior, but both
  are attributable documentation drift around the security-sensitive lookup.
- **Required testable correction:** rewrite the two-line fixture comment to say
  the UID-less ref is the client-expressible containment selector, and replace
  the private intra-doc link with plain code formatting or self-contained
  wording. Re-run `cargo doc -p wyrd-sql --no-deps` and require that the
  candidate-attributable `private_intra_doc_links` warning is gone. Do **not**
  widen the repository rustdoc lane as part of this correction; deciding to add
  `wyrd-sql` or a new denied lint to CI is a separate permanent-cost decision.

## Authority and source coverage

| Boundary | Governing authority | Source and proof inspected | Result |
|---|---|---|---|
| Protocol and identity vocabulary | `AGENTS.md` §§2, 3, 9; `architecture/wyrd-design.md`; `architecture/wyrd-doctrine.mdx`; spec `REQ-001`–`REQ-019`, `INV-001`–`INV-015` | `wyrd-spec/src/auth/*`, `wyrd-runtime`, generated schemas, current architecture documents | **FAIL** — required architecture amendments did not land and current authority contradicts the shipped model |
| HTTP and generated contract | `AGENTS.md` §§9, 11; `architecture/references/architecture/patterns.md`; `languages/errors.md`; spec `REQ-036`, `AC-013`, `AC-014`, `VER-006` | administrative route annotations, `http/openapi.rs`, `openapi.yaml`, schema goldens, `docs/api/*` | **FAIL** — administrative refusals are untyped in OpenAPI and principal revocation has three conflicting public shapes |
| Shared client and Rust SDK projection | `AGENTS.md` §§2, 3, 9; `architecture/references/architecture/patterns.md`; task 008 rev. 7 | `wyrd-client::{auth,platform,principals,transport}`, `wyrd-sdk-rust/src/lib.rs`, callers | **FAIL at the consumer seam** — the handles exist, but the required operator/agent surfaces do not project the complete workflow |
| CLI | `AGENTS.md` §§9, 11; `agent-harness.md`; spec `REQ-036`, `REQ-040`, `REQ-047`, `AC-002`, `AC-013`, `AC-014` | complete command tree, auth/principal/query/eval/card callers, CLI journeys and `mise` lane selection | **FAIL** — no platform or tenant-provisioning command exists; the mandated operator journey cannot be run through the shipped CLI |
| MCP | `AGENTS.md` §§2, 9, 11; `agent-harness.md`; spec `REQ-036`, `AC-013`, `AC-014`; task 008 | catalog, dispatch, principal tools, real-server MCP journey and lane | **FAIL** — only credential list/revoke is projected and the journey never executes the advertised write successfully |
| Operator documentation | `AGENTS.md` §§2, 9, 12; spec `REQ-040`, `AC-013` | self-hosting authentication, local development, running-server, generated API docs | **FAIL** — the required workflow/SaaS/rotation/recovery material is absent and surviving text describes the replaced identity model |
| Journey proof | `AGENTS.md` §11; design doctrine 20; `testing-workflows.md`; `agent-harness.md`; spec `AC-001`–`AC-017` | platform HTTP journey, CLI journeys, MCP journey, task evidence, `mise.toml` | **FAIL** for public-surface acceptance — HTTP is exercised extensively, but the specified CLI and MCP paths are not |

## Validated handoffs and non-findings

- **Same-named Card principals across spaces — real, but requires separate spec
  authority.** `crates/wyrd/wyrd-sql/migrations/20260601000001_auth.sql:51`
  enforces `UNIQUE (data_tenant_id, name)` on
  `wyrd.auth_service_accounts`. A Card's protocol identity includes `space`, so
  provisioning two Card-bound principals with the same name in two spaces
  collides even though the Cards are distinct. The candidate's containment
  lookup explicitly relies on that uniqueness to bound an omitted-space match.
  Correcting the key changes persistent identity, conflict semantics, and the
  lookup predicate; `SPEC-admin-principals` makes changes to `wyrd apply`
  Card-bound provisioning a non-goal. This is a confirmed spec-owner handoff,
  not a correction this review can decision-completely prescribe under revision
  7.
- **Rustdoc CI widening — not retained.** The new warning is confirmed and the
  one-line source correction is included in `CONTRACT-5`. Adding `wyrd-sql` to
  `check:docs` or denying `private_intra_doc_links` repository-wide is not
  necessary to close this candidate defect and would add an unrelated permanent
  gate cost.
- **Known `auth_e2e` red — confirmed as pre-existing evidence, not attributed to
  the candidate.** The tracked prior evidence reproduces
  `auth_e2e::cache_ttl_path_also_flips_verdict` at the candidate and its base
  with `WYRD_AUTH_503_VERIFY_UNAVAILABLE`, mapping from
  `DelegateError::Database(_)` at `exchange_api_key.rs:791`. It therefore is not
  a base-to-candidate regression finding. It does mean the user's stronger
  “entire repository green” expectation is not currently true.

## Verification limits and evidence

- Independently inspected the complete base-to-candidate name/status diff and
  every public-surface source named above, plus surrounding handlers,
  consumers, route registration, tests, generated artifacts, docs, and lanes.
- `mise exec -- cargo doc --locked -p wyrd-sql --no-deps` completed and emitted
  two warnings: the candidate-attributable private-item link at
  `service_accounts.rs:176` and a pre-existing unresolved `IdError` link at
  `error.rs:204`.
- The focused OpenAPI authentication test was started with the exact nextest
  selector; compilation was still in progress when this report was finalized.
  Its source only proves scheme placement and cannot close `CONTRACT-3`.
- Existing task evidence reports `fmt`, `lints`, client-tier checks,
  `codegen:check`, `docs:check`, CLI journey, and platform journey as passing.
  Those passes do not exercise the missing CLI workflow, an authorized MCP
  revoke, the revocation request contract, or administrative problem bodies.
- No broad repository aggregate was completed in this review. Revision 7's
  `VER-003`–`VER-005` explicitly excluded broad verification, while the current
  user now expects all repository tests green. The known base-reproduced
  `auth_e2e` failure prevents claiming that stronger state even though it is not
  a candidate regression.

## Overall result

**FAIL**

The candidate does not satisfy the approved public-contract task: architecture
authority was not amended, the required CLI/MCP workflows are unavailable or
unproved, OpenAPI does not publish typed administrative failures and contradicts
principal revocation, and operator documentation remains incomplete and stale.
