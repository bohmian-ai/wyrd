# Repository Standards Review

- Base: `c5c20754a167e8f4d74a555a720bd51df6179a6f`
- Candidate: `96a1bd81b1d028fa81d6e0f385e3cd9b62080e30`
- Approved authority: `changes/active/admin-principals/spec.md`, revision 10,
  SHA-256 `05982825655110b7a514f16ffdd953e4b1e1df4b7422e9e77461f2e514a10837`
- Review mode: fresh repository-standards audit of the complete cumulative diff

## Review Findings

### Critical

None.

### Important

- `[crates/wyrd/wyrd-server/src/http/openapi.rs:146]` The retained `utoipa`
  document is not the complete served public HTTP contract required by
  `REQ-049` and `AC-019`. `WyrdApiDoc` registers only
  `components::storage::routes::download_init` from the storage router and
  omits the served upload routes at
  `components/storage/routes.rs:34-45`; it also omits all four eval routes at
  `components/eval/routes.rs:46-51`, `/v1/authz/check` at
  `components/authz/routes.rs:9-15`, and the three mounted OTLP endpoints at
  `http/otlp.rs:145-149`. These are not dormant handlers:
  `http/router.rs:52-60` merges every one of those routers into the protected
  `/v1` surface. The route-completeness test at `http/openapi.rs:257-298`
  cannot detect this because it reads only the auth and admin route modules and
  asserts a six-route total. Consequently an independent client generated from
  `/openapi.json` cannot discover or implement the whole API the server serves,
  despite the specification's exact-coverage requirement at
  `spec.md:393-400`. Annotate every omitted public handler, register it in the
  one `WyrdApiDoc`, and replace the auth/admin-only proof with a complete
  served-router-to-document comparison that does not create a second route
  catalog.

- `[architecture/wyrd-design.md:491]` The authoritative architecture and public
  documentation contradict the approved and implemented machine-renewal
  contract. `wyrd-design.md:495-506` still says an API key is exchanged once
  and the SDK auto-refreshes; `wyrd-design.md:117-118,624-626` still attributes
  scope resolution to token “refresh.” The implementation instead correctly
  returns `refresh_token: None` for card-free and card-bound machine exchange
  at `crates/wyrd/wyrd-auth/src/exchange_api_key.rs:429-434,499-504`, and the
  client re-exchanges the durable credential at
  `crates/shared/wyrd-client/src/auth.rs:415-438`, as `REQ-048` requires. Public
  docs preserve the same removed behavior:
  `docs/src/content/docs/self-hosting/authentication.svx:19-30`,
  `docs/src/content/docs/concepts/identity-and-auth.svx:34-40,89-93`,
  `docs/src/content/docs/concepts/authentication.svx:38-56`, and
  `docs/src/content/docs/bifrost/reading-data.svx:157-160` all describe API-key
  refresh. The first three also continue to describe only User, Service, and
  Agent even though the canonical discriminator has five kinds at
  `crates/wyrd-spec/src/auth/principal_kind.rs:18-41`. This is active contract
  drift: maintainers and users are told to build a renewal path that the server
  deliberately does not issue. Update `wyrd-design.md` and the affected public
  docs to distinguish human OIDC refresh rotation from machine durable-
  credential re-exchange, and describe the two administrative principal kinds
  and their plane boundaries.

- `[crates/shared/wyrd-auth-issue/src/lib.rs:629]` New and materially modified
  Rust items do not satisfy the mandatory all-items Rustdoc rule in
  `AGENTS.md:661-675` and `architecture/agent-rules.md:35`. Confirmed examples
  include the card-issuance tests at
  `wyrd-auth-issue/src/lib.rs:629-710`, the machine re-exchange/cache tests at
  `crates/shared/wyrd-client/src/auth.rs:713-797`, the materially renamed and
  changed revocation tests at `crates/wyrd/wyrd-auth/src/revoke.rs:110-132,
  201-202,304-305`, and the new canonical-kind serialization tests at
  `crates/wyrd-spec/src/auth/principal_kind.rs:62-86`. They have no Rustdoc at
  all, and their assertions/`expect` calls also leave undocumented panic
  behavior. The recorded strict-doc command covered only `wyrd-sql` public
  documentation and cannot validate private helpers or test functions in these
  crates. Because the repository makes missing Rustdoc on any touched item a
  hard merge blocker, document every new, renamed, or materially modified Rust
  item across the cumulative diff, including test functions and applicable
  `# Panics`, `# Errors`, cancellation, and partial-progress contracts; then
  repeat a diff-based all-item audit rather than relying on public-item
  `cargo doc` alone.

### Suggestions

None.

## Open Questions

None. The three failures are directly resolvable from the approved
specification and repository authorities.

## Authority Coverage

| Changed surface | Applicable authority read | Result | Exact evidence |
|---|---|---:|---|
| Repository rules, active change/task/review records | `AGENTS.md`; `architecture/agent-rules.md`; `references/languages/spec-driven-development.md`; `wyrd-task-review` | PASS | The cumulative base/candidate and revision-10 authority are identifiable and immutable for this review. The explicitly approved `verified-change-contract` cumulative content and provenance waiver were honored and are not findings. |
| Higher architecture: Wyrd identity, security, tenancy, service identity, doctrine | `wyrd-design.md`; `wyrd-doctrine.mdx`; `wyrd-security-posture.md`; `v1/00-foundations/service-identity.md`; `v1/00-foundations/tenancy.md` | **FAIL** | Principal planes and five-kind wire model are present at `wyrd-design.md:141-158` and `wyrd-security-posture.md:52-64`, but the same design still promises machine refresh at `wyrd-design.md:491-506`; see finding 2. |
| Approved `verified-change-contract` architecture and eval/runtime additions | Approved cumulative change packet; Wyrd design/doctrine; client/server and testing authorities | **FAIL** | The typed eval DTO/client/server/test projection is in the proper owners, but its served eval HTTP routes at `components/eval/routes.rs:46-51` are absent from the mandatory OpenAPI owner; see finding 1. No separate unapproved runtime owner was found. |
| Workspace manifests, lockfile, `mise.toml`, repository checks | `AGENTS.md` §§4, 11, 12; testing workflows; deployment/release | PASS | `utoipa` remains server-owned without YAML duplication; the removed YAML generator/snapshot has no live replacement. `mise run check:client-tier`, `check:unwrap-audit`, and `check:clippy-allow-audit` pass. Dependencies remain in their owning tiers. |
| Shared authorization/runtime types (`wyrd-auth-check`, `wyrd-runtime`) | Wyrd design; security posture; Rust core; struct-centered style | PASS | Closed typed principal/permission/request context replaces stringly cross-plane decisions; no new alternate authorization path or broadly privileged database handle appears in these shared crates. |
| Shared token issuance, verification, OIDC, SSRF screening | Security posture; service identity; agent rules SSRF/secret requirements; Rust core | **FAIL** | Secret-bearing inputs use redacted types, issuer fetches use the canonical screening path, and access/refresh claims remain typed. Mandatory test-item Rustdoc is missing in `wyrd-auth-issue`; see finding 3. |
| Shared client, Rust SDK re-export, platform/principal/eval handles and transport | Client/server ownership; security posture; async/runtime rules; client-tier rule | **FAIL** | `TokenExchange`, `Platform`, and `Principals` are dependency-owning handles; machine renewal re-exchanges rather than storing refresh tokens; `check:client-tier` passes. Test-item Rustdoc is missing at `wyrd-client/src/auth.rs:713-797`; see finding 3. |
| `wyrd-spec` DTOs, principal kinds, stable errors, audit fields, schema goldens | `AGENTS.md` §§3, 4, 9; error, agent-harness, public-contract, generated-artifact rules | **FAIL** | DTOs/errors are typed and schema goldens exist in both generated/test trees; available `codegen:check` evidence is green. The canonical-kind test items lack mandatory Rustdoc, and the runtime OpenAPI projection is incomplete; see findings 1 and 3. |
| Tenant/platform SQL migrations, operator/tenant connections, query modules and SQL tests | SQL/tenancy authority; security posture; service identity; tenant-isolation rules | PASS | Platform tables use `OperatorPool`; tenant work uses `TenantConn`; migration/query ownership remains in `wyrd-sql`; independent `mise run check:tenant-isolation` passes. No production `PgPool` escape or tenant-id SQL parameter bypass was found in the changed administration paths. |
| Tenant and platform auth services: credentials, OIDC, refresh, revocation, authorization, sessions | Security posture; service identity; server-contract and audit rules | **FAIL** | Machine grants issue no refresh token, User refresh rotation/replay stays in the human path, and exact credential/principal types remain closed. The revocation tests have mandatory Rustdoc omissions; see finding 3. |
| Server boot/config/state, extractors, HTTP routes, admin/platform/principal services | Server/API patterns; deployment/release; security posture; tenancy; audit rules | **FAIL** | Platform and tenant extraction remain distinct, the server owns all durable effects, and route operations use typed bodies/stable errors. The one runtime OpenAPI document omits live route families; see finding 1. |
| Audit staging, retained Vala audit table, credential-id migration, Bifrost Gate/Scribe/Oracle paths | `bifrost-design.md`; Wyrd audit doctrine; security posture; SQL/tenancy rules | PASS | The change retains `vala.audit_staging` and the single `AuditPublisher`, evolves the one retained `audit_log`, preserves the legacy hash preimage when `credential_id` is absent, and does not introduce a competing audit sink/lease/relay. Independent tenant-boundary static checking passes. |
| CLI platform/principal/auth/eval surfaces and journeys | Headless/client projection doctrine; CLI/client ownership; testing workflows | PASS | CLI operations call the shared client/auth owners, secrets are redacted, and dedicated operator/principal/auth/eval journey targets exist. The recorded `test:cli:journey` selection exercises the real server path rather than an in-process durable owner. |
| MCP principal projection and Bifrost MCP journey | MCP first-class/scoped-write doctrine; agent harness; testing rules | PASS | MCP remains a projection over server/client contracts; the changed Bifrost MCP journey has an explicit dedicated task and recorded nine-test pass. No alternate durable principal store or authorization vocabulary was introduced. |
| Test fixtures, OIDC fixture, real-server/platform/identity/SQL/Bifrost journeys | `AGENTS.md` §11; testing workflows; runtime-ownership rules | **FAIL** | The change supplies unit, Postgres integration, and real client→server journey lanes for the new administration behavior, including two consecutive platform runs. Rust test items remain subject to the same Rustdoc hard requirement and violate it in the cited files; see finding 3. |
| Public docs, API docs generator, LLM indexes, generated schema/error pages | Doctrine; public-contract/agent-harness rules; docs verification | **FAIL** | `/openapi.json` correctly replaces the removed checked-in YAML artifact, and the docs lane is recorded green, but generated runtime coverage is incomplete and multiple public identity/renewal pages contradict the shipped contract; see findings 1 and 2. |
| Final diff hygiene and committed artifacts | `AGENTS.md` completion standard; agent rules | **FAIL** | No committed build output, temporary database state, or stray binary was found. `git diff --check base..candidate` nevertheless fails on seven committed Markdown files with an added blank line at EOF (listed under Verification Notes). |

## Rule Compliance

| Rule | Result | Exact evidence |
|---|---:|---|
| Review subject remains the requested immutable candidate | PASS | `git rev-parse HEAD` returned `96a1bd81b1d028fa81d6e0f385e3cd9b62080e30` before writing this report. |
| Active architecture must reflect approved behavior | **FAIL** | `wyrd-design.md:495-506` contradicts `spec.md:382-392` and `exchange_api_key.rs:429-434,499-504`; finding 2. |
| Durable behavior remains Rust server-owned; clients project shared contracts | PASS | SQL/auth/audit mutations stay under `wyrd-server`, `wyrd-auth`, `wyrd-sql`, and Vala; `wyrd-client` owns transport/ergonomics, and `sdks/wyrd-sdk-rust` only re-exports the shared surface. |
| Struct-centered Rust ownership | PASS | Cohesive owners include `TokenExchange`, `AuthMiddleware`, `Platform`, `Principals`, `PlatformAuthorization`, `TenantProvisioning`, and recovery/platform credential services; no new one-caller inheritance trait or dependency-threaded free-function workflow was found. |
| Async only at IO/composition boundaries | PASS | Changed async operations await HTTP, SQL, server, or orchestration IO; pure principal-kind/schema transformations remain synchronous. |
| Typed public bodies, stable catalog errors, exact runtime OpenAPI | **FAIL** | DTOs and `WyrdError` projections are typed, but live storage/eval/authz/OTLP routes are absent from `WyrdApiDoc`; finding 1. |
| Secrets are redacted and external issuer URLs are SSRF-screened | PASS | Trusted issuer request/CLI values use `SecretBearer`/`SecretString`; OIDC registry uses the shared screened-fetch path; no secret-bearing derived `Debug` regression was found. |
| Platform/tenant DB capabilities and RLS boundaries stay narrow | PASS | `mise run check:tenant-isolation` passed; changed platform owners use `OperatorPool`, tenant owners use `TenantConn`, and the changed production paths do not accept raw `PgPool`. |
| Authorization decisions use canonical audit append and same decision/effect transaction | PASS | Allowed same-plane paths carry the transaction returned by authorization through the mutation and commit once; denials commit their canonical staged decision; no second audit writer/table/log sink was added. Provisioning/recovery preserve their documented cross-plane phase boundaries. |
| Bifrost retained audit evolution preserves one publisher/history | PASS | The exact previous fingerprint has a single additive reconciliation path, null credentials keep the historical hash preimage, and `AuditPublisher` remains the only staging-to-retained mover. |
| Generated artifacts have one owner and regenerate through sanctioned lanes | **FAIL** | JSON schemas/error/language artifacts retain their generators and recorded `codegen:check` pass; checked-in OpenAPI/YAML machinery is removed. The remaining runtime-generated OpenAPI document is incomplete; finding 1. |
| Every new/materially modified Rust item has complete Rustdoc | **FAIL — BLOCK_BEFORE_MERGE** | Confirmed undocumented test functions at `wyrd-auth-issue/src/lib.rs:629-710`, `wyrd-client/src/auth.rs:713-797`, `wyrd-auth/src/revoke.rs:110-132,201-202,304-305`, and `wyrd-spec/src/auth/principal_kind.rs:62-86`; finding 3. |
| New user/agent-facing behavior has the required tiered journeys | PASS | Dedicated tasks cover principal unit/integration, platform real-server journey (twice), CLI real-server journey, Bifrost MCP, SQL, identity refresh/replay, and retained audit publication. These selections match the changed boundaries. |
| Public docs describe the shipped contract | **FAIL** | API-key refresh and three-kind identity prose contradict the implementation and canonical five-kind contract; finding 2. |
| No legacy OpenAPI duplicate or unapproved parallel mechanism remains | PASS | Root `openapi.yaml`, generator example, YAML endpoint/feature/dependency, codegen task, and release digest are deleted; `utoipa` plus `/openapi.json` remains the single owner. |
| Diff is formatting-clean and free of committed junk | **FAIL** | No material junk artifact was found, but `git diff --check` reports seven Markdown EOF whitespace errors. |

## Verification Notes

Independently executed against the candidate:

| Command | Result |
|---|---:|
| `mise run check:tenant-isolation` | PASS |
| `mise run check:client-tier` | PASS |
| `mise run check:unwrap-audit` | PASS |
| `mise run check:clippy-allow-audit` | PASS |
| `git diff --check c5c20754a167e8f4d74a555a720bd51df6179a6f 96a1bd81b1d028fa81d6e0f385e3cd9b62080e30` | **FAIL** |

The whitespace failure reports “new blank line at EOF” in:

- `changes/active/admin-principals/review/whole-branch-01/task-review.md:298`
- `changes/active/admin-principals/review/whole-branch-02/task-review.md:293`
- `changes/active/admin-principals/review/whole-branch-03/verdict.md:119`
- `changes/active/verified-change-contract/tasks/TASK-001-verifier-contract-and-registration.md:180`
- `changes/active/verified-change-contract/tasks/TASK-003-binding-projection-and-runtime-activity.md:177`
- `changes/active/verified-change-contract/tasks/TASK-005-production-drift-verifier.md:194`
- `changes/active/verified-change-contract/tasks/TASK-007-operator-connections-and-delivery.md:229`

The appended implementation evidence records green final-tree runs for
`fmt:check`, workspace lints, all three static checks above, principal unit and
Postgres integration lanes, two consecutive platform journeys, CLI and MCP
journeys, SQL, code generation, examples, docs, strict public `wyrd-sql`
Rustdoc, and the three broad Bifrost integration groups. Source inspection
confirms those lanes target the relevant changed owners. They do not close the
findings above:

- the six-test OpenAPI lane proves selected operations and only compares the
  auth/admin route modules, not all mounted public routers;
- the docs build proves syntax/link validity, not semantic agreement with the
  machine-renewal contract; and
- `cargo doc -p wyrd-sql` cannot enforce required documentation on private/test
  items in `wyrd-spec`, `wyrd-auth-issue`, `wyrd-client`, or `wyrd-auth`.

`mise run gate` was correctly not substituted because approved `VER-003`
selects the narrower change-specific proof set. No result from a prior
task-review verdict was used as a review conclusion.

## Overall Result

**FAIL**

The candidate is not mergeable under repository standards until the complete
served HTTP surface is represented in the single runtime OpenAPI contract, the
architecture and public docs match machine re-exchange/human refresh semantics,
and every touched Rust item satisfies the repository's mandatory Rustdoc
contract. The committed Markdown whitespace errors must also be cleaned before
the final diff check can pass.
