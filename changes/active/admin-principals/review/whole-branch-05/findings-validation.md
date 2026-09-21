# Admin principals whole-branch review 05 — structured Ponytail findings validation

## Immutable subject

| Item | Value |
|---|---|
| Repository | `/home/thorrester/Documents/GitHub/wyrd` |
| Base | `c5c20754a167e8f4d74a555a720bd51df6179a6f` |
| Candidate / reviewed HEAD | `c9e1092bbdb4df3781eb91b0eb33150e00df7623` |
| Approved authority | `changes/active/admin-principals/spec.md`, revision 10, status `approved` |
| Approved spec SHA-256 | `05982825655110b7a514f16ffdd953e4b1e1df4b7422e9e77461f2e514a10837` |
| Original task authority | `TASK-001` through `TASK-008` under `changes/active/admin-principals/tasks/` |
| Prior review authority | Whole-branch-04 verdict, validation, and `TASK-001-008-R4-close-cumulative-findings.md` |
| Wave-1 inputs | `task-review.md`, `standards-review.md`, `domain-review-security.md`, `domain-review-data.md`, and `domain-review-contract.md` in this directory |

The candidate and approved-spec checksum matched the values above before this
report was written. `.codegraph/` is present and was used before direct source
inspection. The complete base-to-candidate inventory, the applicable
architecture, the original task packets, the prior ledger/remediation packet,
all Wave-1 reports, and the current implementation and tests behind every
proposal were inspected. This validation changes only this report.

## Validation outcome

**FIX_REQUIRED.** Nine material roots survive independent validation. Four are
prior stable findings that remain open in narrower forms:
`FIND-admin-principals-R4-3`, `FIND-admin-principals-R4-5`,
`FIND-admin-principals-R4-9`, and `FIND-admin-principals-13`. Five are new:
`FIND-admin-principals-R5-1` through `FIND-admin-principals-R5-5`.

The Ponytail ladder was applied to every proposed correction. The retained
corrections reuse the existing authorization transaction, authorization epoch,
shared HTTP transport, `utoipa`/Axum integration point, SQLx forward migration
mechanism, module import blocks, `request_json`, `wyrd-spec` DTOs, and rmcp's
installed typed-schema support. No second cache, blacklist, audit writer,
transport, route catalog, error list, migration framework, or test harness is
justified.

**SPEC_REVISION_REQUIRED: none.** Every retained outcome is already fixed by
revision 10 or higher repository authority. The implementation choices below
are reversible local decisions.

## Wave-1 proposal disposition

| Wave-1 proposal | Validation | Disposition |
|---|---|---|
| `TREV-WB05-1` — same-second role withdrawal | **REVISED** | Stable `FIND-admin-principals-R4-3`; the defect is real, but the correction must order the stored epoch and successor `iat` together rather than merely changing one comparison. |
| `TREV-WB05-2` — federated grants recorded as `global_admin` | **REVISED / CONSOLIDATED** | Stable `FIND-admin-principals-R4-5`, together with `SEC-WB05-2`; both defects are in the one federated platform-grant transaction required by R4-5. |
| `TREV-WB05-3` — MCP duplicates Wyrd auth/retry | **CONFIRMED** | Stable `FIND-admin-principals-R4-9`. |
| `TREV-WB05-4` — duplicate OpenAPI route knowledge | **REVISED / CONSOLIDATED** | Stable `FIND-admin-principals-13`, together with `CONTRACT-05-01`; exact operation and error coverage are one OpenAPI-owner root. |
| `STD-R5-001` — qualified declaration types | **CONFIRMED** | New `FIND-admin-principals-R5-1`. |
| `SEC-WB05-1` — same-second role withdrawal | **REVISED / DUPLICATE** | Stable `FIND-admin-principals-R4-3`. |
| `SEC-WB05-2` — first-login pin commits before grant audit | **REVISED / CONSOLIDATED** | Stable `FIND-admin-principals-R4-5`; the same correction also carries the stored principal kind into the event. |
| `SEC-WB05-3` — logical refusals roll back an appended authorization decision | **REVISED / EXPANDED BY CALLER TRACE** | New `FIND-admin-principals-R5-2`; the same reachable shape also exists in platform connection removal, platform status refusal, and tenant trusted-issuer deletion. |
| `DATA-05-1` — an existing Vala migration was modified | **CONFIRMED** | New `FIND-admin-principals-R5-3`. |
| `CONTRACT-05-01` — `/auth/token` omits reachable stable errors | **REVISED / CONSOLIDATED** | Stable `FIND-admin-principals-13`. |
| `CONTRACT-05-02` — MCP duplicates Wyrd auth/retry | **CONFIRMED / DUPLICATE** | Stable `FIND-admin-principals-R4-9`. |
| `CONTRACT-05-03` — tenant credential revocation bypasses renewal | **CONFIRMED** | New `FIND-admin-principals-R5-4`. |
| `CONTRACT-05-04` — principal MCP schemas are handwritten/incomplete | **REVISED** | New `FIND-admin-principals-R5-5`; use rmcp's installed typed-schema methods and shared DTOs, not a new schema layer. |

### Rejected proposals

None. Duplicate proposals were revised into their shared root rather than
retained as extra findings. No dormant, test-only, speculative, zero-caller, or
out-of-scope proposal survived as optional advice.

## Deduplicated retained ledger

### `FIND-admin-principals-R4-3` — REVISED — INCORRECT — same-second role withdrawal leaves the old token valid

- **Wave-1 sources:** `TREV-WB05-1`, `SEC-WB05-1`.
- **Violated obligation:** `REQ-005`, `INV-013`, `AC-010`, and the prior R4-3
  requirement that changed OIDC roles retire the old token no later than the
  next request while the reduced successor remains usable.
- **Exact location:**
  `crates/wyrd/wyrd-auth/src/callback.rs:219-243`,
  `crates/wyrd/wyrd-sql/src/queries/auth/revocation.rs:87-120`,
  `crates/shared/wyrd-auth-issue/src/lib.rs:453-477`, and
  `crates/shared/wyrd-auth-verify/src/lib.rs:453-563`.
- **Evidence and reachability:** The one production caller of
  `advance_user_epoch_to_second` is the changed-role callback. It stores
  `date_trunc('second', now())`; access-token issuance stores
  `Utc::now().timestamp()`; both cache-hit and cache-miss verifier branches
  reject only `iat < epoch`. An old token minted earlier in that same second
  therefore has `iat == epoch` and remains accepted through every protected
  tenant caller. Existing tests prove an earlier-second token and the epoch
  write, not equality.
- **Observable consequence:** Authority removed by the provider remains usable
  for the old token's remaining lifetime on a reachable timing boundary.
- **Decision-complete minimum correction:** Keep the one epoch and the existing
  `<` verifier rule. Have the role-change transaction choose the next whole
  second as the epoch and return that value; mint the successor with an explicit
  `iat` equal to that returned epoch (and expiry derived from it). This makes
  every token minted before the change strictly older while admitting the
  successor, without a wait, blacklist, second cache, or precision-dependent
  comparison. Unchanged logins must not advance the epoch.
- **Focused closure proof:** A deterministic same-second case presents an old
  token whose `iat` equals the old current-second value, performs a real role
  reduction, then proves the old token is refused, the successor is accepted
  with reduced roles, and an unchanged login leaves the first session accepted.

### `FIND-admin-principals-R4-5` — REVISED — INCORRECT — the federated platform grant is neither fully atomic nor correctly attributed

- **Wave-1 sources:** `TREV-WB05-2`, `SEC-WB05-2`.
- **Violated obligation:** `REQ-037`, `AC-009`, service-identity audit
  authority, and prior R4-5's requirement that identity pinning, session grant,
  canonical append, and commit form one attributable grant boundary.
- **Exact location:**
  `crates/wyrd/wyrd-auth/src/platform_login.rs:214-315`,
  `crates/wyrd/wyrd-auth/src/platform_sessions.rs:195-260`, and
  `crates/wyrd/wyrd-sql/src/queries/platform/identity.rs:248-311`.
- **Evidence and reachability:** `PlatformLogin::complete` is the served
  platform OIDC callback path. First login calls pool-scoped
  `pin_platform_identity`, which autocommits, and only afterwards calls
  `PlatformSessions::issue_federated`, which starts the audited transaction.
  An append/session failure therefore leaves the permanent pin behind. Inside
  that later transaction `platform_principal_by_id_tx` already returns the
  stored `principal_kind`, but `record_grant` discards it and hard-codes
  `GlobalAdmin`; the served registration flow stores federated administrators
  as `User`. The current test seeds `GlobalAdmin` and asserts only credential
  absence.
- **Observable consequence:** A failed first grant leaves unaudited durable
  identity state, and a successful human grant enters retained audit as the
  deployment-root kind.
- **Decision-complete minimum correction:** Make `PlatformSessions` own one
  audited transaction for the verified federated identity resolution: resolve
  an existing subject or pin the verified pre-registration on that transaction,
  re-read the active principal, mint the session, append the canonical event
  with `principal.principal_kind`, and commit. Pass only verified issuer,
  subject, and match claim from `PlatformLogin`; delete the pool-scoped pin from
  the grant path. Credential exchange continues through the same event helper
  with its already-read stored kind and credential id.
- **Focused closure proof:** Inject append failure on an unpinned registered
  `User` and prove no subject/pinned time and no token survives. Retry and prove
  exactly one pin and one grant row with the real principal id, kind `user`, and
  null credential id. Retain the `global_admin` credential-backed case with its
  credential id.

### `FIND-admin-principals-R4-9` — CONFIRMED — VIOLATION — MCP still owns a second Wyrd authentication/replay path

- **Wave-1 sources:** `TREV-WB05-3`, `CONTRACT-05-02`.
- **Violated obligation:** `REQ-047`, `REQ-048`, `AC-018`, and prior R4-9.
- **Exact location:** `crates/wyrd/wyrd-mcp/src/client.rs:31-175` and its five
  `StreamableHttpClient` operations; canonical behavior is in
  `crates/shared/wyrd-client/src/transport/http.rs:49-57,109-180,640-744`.
- **Evidence and reachability:** The sole production MCP adapter is constructed
  from `WyrdClient`, but then clones its raw `reqwest::Client`, redeclares both
  Wyrd headers, formats the bearer, classifies rmcp errors as 401, forces a
  refresh, and owns a one-replay loop. Every rmcp POST/SSE/session-delete call
  traverses this adapter rather than `HttpTransport::send_with_retry`. Sharing
  the pool and token cache does not remove the second policy owner.
- **Observable consequence:** Header, refusal classification, and renewal
  semantics can diverge from every other first-party client while both local
  suites remain green.
- **Decision-complete minimum correction:** Keep rmcp framing and its raw HTTP
  operation in `wyrd-mcp`, but move Wyrd header decoration and the exactly-once
  authentication-refusal refresh/replay decision behind one narrow public
  capability on the existing `HttpTransport`/`AuthMiddleware` owner. The MCP
  adapter supplies the replayable rmcp operation and carries MCP session/custom
  headers; it contains no Wyrd header constants, bearer rendering, 401
  classifier, raw auth handle, or retry policy. Do not add another transport or
  an rmcp dependency to `wyrd-client`.
- **Focused closure proof:** The real MCP transport proves one shared pool, one
  re-exchange and replay after the first 401, terminal behavior after the
  second, and preservation of MCP session/SSE headers; a source assertion proves
  no Wyrd header or status policy remains in `wyrd-mcp`.

### `FIND-admin-principals-13` — REVISED — VIOLATION — the runtime OpenAPI contract is still duplicated and incomplete

- **Wave-1 sources:** `TREV-WB05-4`, `CONTRACT-05-01`.
- **Violated obligation:** `REQ-049`, `AC-014`, `AC-019`, and stable prior
  `FIND-admin-principals-13`: one exact `utoipa` contract for every served
  method/path and every reachable stable error, with no hand-written route or
  error catalog.
- **Exact location:**
  `crates/wyrd/wyrd-server/src/http/openapi.rs:107-177`,
  `crates/wyrd/wyrd-server/tests/pg_openapi_contract.rs:70-205`, and
  `crates/wyrd/wyrd-server/src/components/auth/routes.rs:56-108`.
- **Evidence and reachability:** `WyrdApiDoc` centrally repeats every handler
  already registered by the live Axum route modules. The contract test then
  source-parses `.route`, `.merge`, files, and function bodies and compares only
  paths; a new method on an existing path can be served but undocumented while
  the test remains green. Separately, served `/auth/token` API-key and refresh
  branches pass through `append_auth_audit`, whose failure returns
  `WYRD_AUDIT_503_UNAVAILABLE`, and token signing/configuration failures return
  `WYRD_SPEC_500_INTERNAL`; its annotation declares neither. The stable-code
  test validates only codes already written into descriptions and cannot find
  an omitted code.
- **Observable consequence:** An independent client generated from
  `/openapi.json` can miss a served operation or a real fail-closed token-grant
  response while all current OpenAPI tests pass.
- **Decision-complete minimum correction:** Replace the separate Axum route
  registration plus central `WyrdApiDoc.paths` list with the official
  version-matched utoipa/Axum route integration (the smallest native companion
  mechanism that registers handler and operation together). Compose the final
  document from those owning routers and delete the central path list and the
  source parser. Trace and declare the full `/auth/token` error set, including
  audit-unavailable and internal issuance/configuration failures, on its one
  handler annotation. Do not add a snapshot, YAML endpoint, file generator, or
  independent error set.
- **Focused closure proof:** Request the assembled server's `/openapi.json` and
  compare served **method + normalized path** operations from the co-registered
  routers; prove YAML remains unrouted. Inject token exchange/refresh audit
  failure and signing/configuration failure, then assert each returned problem
  code/status is declared by `POST /auth/token`.

### `FIND-admin-principals-R5-1` — CONFIRMED — VIOLATION — candidate-added declarations retain qualified type paths

- **Wave-1 source:** `STD-R5-001`.
- **Violated obligation:** `architecture/agent-rules.md` requires module-top
  imports and bare names in fields, parameters, returns, bounds, and `where`
  clauses; it is a hard repository acceptance rule.
- **Exact location:** The Wave-1 list is confirmed, including
  `wyrd-auth-oidc/src/screening.rs:96`, `wyrd-client/src/auth.rs:118,248`,
  `wyrd-runtime/src/principal.rs:151,167`, `wyrd-auth/src/revoke.rs:87`,
  `wyrd-mcp/src/client.rs:52,106,131,163,166`,
  `wyrd-server/src/boot/init.rs:186`, `components/admin/routes.rs:184,806,951,961`,
  `http/openapi.rs:30,63,99`, `main.rs:137`, `mcp/principals.rs:180`, the named
  `wyrd-sql` query declarations, and candidate-added test declarations.
- **Evidence and reachability:** A base-to-candidate declaration scan finds
  candidate-added fields/signatures such as `reqwest::Client`,
  `Option<uuid::Uuid>`, `impl std::fmt::Display`,
  `Result<wyrd_sql::OperatorPool, _>`, `serde::de::DeserializeOwned`, and
  `&sqlx::PgPool`. These are compiled production or test declarations, not
  dormant alternatives; the rule applies to both. Expressions using qualified
  paths are not implicated.
- **Observable consequence:** The candidate directly violates a mandatory
  source-shape rule, and dependency ownership is hidden at declaration sites.
- **Decision-complete minimum correction:** For every qualified type path added
  by the cumulative diff in a declaration, extend the existing module-top
  `use` block and replace only that declaration occurrence with the bare name.
  Do not refactor expressions or untouched baseline code.
- **Focused closure proof:** `fmt:check`, lints, affected scoped tests, and a
  base-to-candidate declaration scan returning no candidate-added qualified
  field/signature/bound.

### `FIND-admin-principals-R5-2` — REVISED — INCORRECT — stable no-effect refusals roll back authorization decisions already appended

- **Wave-1 source:** `SEC-WB05-3`.
- **Violated obligation:** `REQ-037`, `AC-009`, and the canonical-audit rule
  require every evaluated permission decision to be durable, allowed and denied
  alike.
- **Exact location:**
  `components/platform/credentials.rs:205-247`,
  `components/platform/identity.rs:360-386,580-634`, and
  `components/admin/routes.rs:410-480,672-701`. The same tenant shape also
  occurs in trusted-issuer deletion at `components/admin/routes.rs:410-480`.
- **Evidence and reachability:** `PlatformAuthorization::authorize` appends an
  allowed row and returns its open transaction. Unknown/wrong-owner/already-
  revoked credential branches return 404 without `commit_decision`; removing a
  missing OIDC connection and status changes returning `NotFound` or
  `WouldStrandDeployment` do the same. Tenant trusted-issuer and workload-
  binding deletion append on `TenantConn` and return 404 before commit when the
  delete affects zero rows. These are all served routes and deliberate stable
  outcomes, not store failures; dropping the connection rolls their audit rows
  back.
- **Observable consequence:** Authorized probes and replays return stable
  administrative responses without durable evidence that permission was
  evaluated.
- **Decision-complete minimum correction:** In each named route, commit the
  existing decision transaction before returning a stable logical no-effect
  outcome (`not found`, already absent/revoked, or last-admin conflict). Keep
  store/commit failures rollback-coupled, and validate request syntax before
  opening a decision where possible. Reuse the existing `commit_decision` or
  `TenantConn::commit`; add no audit helper.
- **Focused closure proof:** Real-Postgres cases for unknown/wrong-owner and
  replayed platform credential revoke, absent platform connection, platform
  status not-found/last-admin conflict, absent trusted issuer, and absent
  workload binding each assert one committed authorization row and no resource
  mutation; injected store failures still commit neither effect nor decision.

### `FIND-admin-principals-R5-3` — CONFIRMED — VIOLATION — an immutable Vala migration was rewritten

- **Wave-1 source:** `DATA-05-1`.
- **Violated obligation:** `architecture/v1/00-foundations/sql-foundation.md`
  and `architecture/operations/deployment-and-release.md` require ordered,
  immutable, checksum-verified migration files and rejection of modified
  applied migrations.
- **Exact location:**
  `crates/vala/vala-sql/migrations/20260802000000_vala_audit_staging.sql:55-63`.
- **Evidence and reachability:** The base file checksum is
  `ce869581019efb6689d9413efa77245f2381f464127e707458f4026178ca5d84`; the
  candidate checksum is
  `e7298d0fa897504baaf162dd2cca45a331a3cb5f19d90061ff9dfad5c36d5d59` after
  inserting `credential_id`. Both `WyrdPostgres::connect_from_dsns` and
  `ValaPostgres::connect_from_dsns` run the embedded migrations before runtime
  pools open. A database that applied the base checksum therefore fails before
  serving. Fresh-database lanes cannot exercise this. The R4 request to delete
  unshipped application/Iceberg compatibility does not override the higher
  persistent-migration authority.
- **Observable consequence:** Upgrade from the immutable base aborts on SQLx
  checksum drift before any administrative capability is available.
- **Decision-complete minimum correction:** Restore the old migration byte for
  byte and add one forward-only Vala migration that adds nullable
  `credential_id`. Keep the deleted application/Iceberg legacy fingerprint,
  alternate hash, field-id, and reconciliation machinery deleted; the SQL
  forward migration is the only compatibility required.
- **Focused closure proof:** Migrate a database through the exact base Vala
  migration set, run the candidate set without checksum drift, then prove the
  nullable column and current append/publication path. Retain fresh-schema SQL
  and Bifrost proofs.

### `FIND-admin-principals-R5-4` — CONFIRMED — INCORRECT — tenant credential revocation bypasses shared reactive renewal

- **Wave-1 source:** `CONTRACT-05-03`.
- **Violated obligation:** `REQ-048` and `AC-018` require every machine-
  authenticated `wyrd-client` request to re-exchange once after an
  authentication refusal and replay at most once.
- **Exact location:**
  `crates/shared/wyrd-client/src/principals/handle.rs:147-170` and
  `crates/shared/wyrd-client/src/transport/http.rs:109-180,315-345,640-744`.
- **Evidence and reachability:** The public `Principals::revoke_credential`,
  used by CLI and MCP administrative flows, is the only control caller of
  `request_raw`. `request_raw` sends once and contains no 401 refresh branch;
  `request_json` already handles empty 204 bodies as JSON `null` and routes
  through `send_with_retry`. The other `request_raw` callers are local storage
  downloads whose streaming body semantics justify the raw helper.
- **Observable consequence:** Credential revocation alone returns a terminal
  401 when a valid durable API key's cached bearer is refused, while neighboring
  control operations renew successfully.
- **Decision-complete minimum correction:** Change only tenant credential
  revocation to `request_json::<(), ()>(DELETE, path, None)`, reusing the
  existing empty-body decoding and shared retry owner. Leave storage download
  callers and `request_raw` unchanged.
- **Focused closure proof:** Exercise the public method with a first 401 and
  prove one re-exchange plus one successful replay; a second 401 is terminal.
  Retain the 204/no-body success case.

### `FIND-admin-principals-R5-5` — REVISED — VIOLATION — principal MCP tools do not publish one typed input/output contract

- **Wave-1 source:** `CONTRACT-05-04`.
- **Violated obligation:** The agent-harness contract requires MCP tools to
  expose stable typed inputs and outputs from shared schemas, and `REQ-036`
  requires MCP to project rather than reinvent the administrative contract.
- **Exact location:**
  `crates/wyrd/wyrd-server/src/mcp/principals.rs:71-162,194-247` and
  `crates/wyrd/wyrd-mcp/tests/bifrost/mcp/principals.rs:65-270`.
- **Evidence and reachability:** The principal descriptors are included in every
  authenticated catalog and dispatched by `WyrdMcpHandler::call_tool`. Their
  input JSON is handwritten separately from the deserialization structs; both
  `Tool.output_schema` values are absent. Listing happens to serialize the
  existing schemars-backed `CredentialListResponse`; revocation returns an ad
  hoc object. The journey proves names and selected fields, not advertised
  schemas or result conformance. rmcp 3.3 is already installed and provides
  `Tool::with_input_schema` and `Tool::with_output_schema`.
- **Observable consequence:** An agent cannot discover result shapes, and the
  advertised input/result can drift from the types actually parsed or returned.
- **Decision-complete minimum correction:** Put the two minimal input DTOs and
  one revocation acknowledgement DTO beside the existing auth wire DTOs in
  `wyrd-spec`, deriving `Serialize`, `Deserialize`, and `JsonSchema`; reuse
  `CredentialListResponse` for listing output. Build descriptors with rmcp's
  installed typed input/output methods, deserialize those same DTOs, and return
  those same output types. Delete the handwritten JSON schemas and private
  duplicate argument structs; add no MCP-specific schema framework.
- **Focused closure proof:** The real catalog journey asserts both tools publish
  input and output schemas, validates representative results against them, and
  retains the scope-gating, stable-error, list, revoke, and observe-after-act
  behavior.

## Correction ordering and seam preservation

1. Restore migration immutability before running any Postgres proof.
2. Correct the federated grant transaction and same-second epoch ordering before
   security journeys, because both change token/audit evidence those journeys
   consume.
3. Close dropped-decision commits without changing store-failure rollback.
4. Consolidate shared client renewal before removing MCP-local auth policy.
5. Co-register Axum/OpenAPI operations and typed MCP schemas, then delete the
   source parser and handwritten tool schemas.
6. Apply the mechanical bare-type import cleanup last and run its diff scan.

Preserve tenant RLS, platform/tenant plane separation, credential secrecy,
single canonical audit append/publisher, platform per-request grant anchoring,
MCP protocol/session semantics, strict current Bifrost schema/hash behavior,
and the approved absence of Python/TypeScript administrative bindings, OpenAPI
snapshots/YAML, and application-level historical audit shims.

## Verification limits

This was a static validation and did not rerun the long environment-owning
suites. The R4 packet's recorded green lanes are credible for their selections,
but none selects the same-second equality, unpinned append failure, stored
platform-human kind, no-effect audit commits, base-migration upgrade, tenant
revocation 401 renewal, method-level OpenAPI closure/omitted token errors, or
MCP schema publication. Those focused proofs are therefore required before the
existing scoped lanes can close the retained ledger.

The candidate remained
`c9e1092bbdb4df3781eb91b0eb33150e00df7623` at the final integrity check.
