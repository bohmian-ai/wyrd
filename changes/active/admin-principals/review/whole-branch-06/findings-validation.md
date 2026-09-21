# Admin principals whole-branch review 06 — structured Ponytail findings validation

## Immutable subject

| Item | Value |
|---|---|
| Repository | `/home/thorrester/Documents/GitHub/wyrd` |
| Base | `c5c20754a167e8f4d74a555a720bd51df6179a6f` |
| Candidate / reviewed HEAD | `2c0408b683f7a548cec6dd08b35698d761d33b31` |
| Product-code candidate | `5ecc8a4e5a3a76390bc32f00353e558ea6a685d2` |
| Approved authority | `changes/active/admin-principals/spec.md`, revision 10, status `approved` |
| Approved spec SHA-256 | `05982825655110b7a514f16ffdd953e4b1e1df4b7422e9e77461f2e514a10837` |
| Original task authority | `TASK-001` through `TASK-008` under `changes/active/admin-principals/tasks/` |
| Prior review/remediation | Whole-branch-05 verdict, validation, `TASK-001-008-R5-close-validated-findings.md`, and its implementation evidence |
| Wave-1 inputs | `task-review.md`, `standards-review.md`, `domain-review-security.md`, `domain-review-data.md`, and `domain-review-contract.md` in this directory |

The candidate and approved-spec checksum matched the values above before this
report was written. `.codegraph/` was used before direct source inspection. The
complete cumulative inventory, the applicable repository and architecture
authority, all eight original tasks, the whole-branch-05 ledger and remediation
evidence, all Wave-1 reports, and the complete current bodies and callers behind
every proposal were inspected. This validation changes only this report.

## Validation outcome

**FIX_REQUIRED.** Five material roots survive independent validation. Four are
stable prior findings that remain open in narrower or expanded form:
`FIND-admin-principals-13`, `FIND-admin-principals-R5-1`,
`FIND-admin-principals-R5-2`, and `FIND-admin-principals-R5-5`. One is genuinely
new: `FIND-admin-principals-R6-1`.

The Ponytail ladder removes machinery where possible. The retained corrections
reuse the existing `utoipa-axum` route owner, runtime OpenAPI suite, typed
identifier contracts, authorization transactions, direct epoch query, shared
verifier, installed `jsonschema` support, and module import blocks. No second
route catalog, schema layer, validator, cache, listener, invalidation protocol,
audit writer, or test harness is justified.

**SPEC_REVISION_REQUIRED: none.** Revision 10 already decides every retained
outcome. The local blob endpoints are served, authenticated Wyrd operations
consumed by `wyrd-client` and the CLI, so `REQ-049` requires them in
`/openapi.json`. No approved specification or architecture authority creates an
OpenAPI exception for a local backend or an Axum wildcard. The implementation
comment at `components/storage/routes.rs:45-51` cannot create one. Making the
locator representable, deleting a stale cache, using existing identifier types,
committing existing decisions, and cleaning declarations/guidance are reversible
implementation choices, not new product decisions.

## Wave-1 proposal disposition

| Wave-1 proposal | Validation | Disposition |
|---|---|---|
| `TREV-WB06-1` — local blob routes omitted from OpenAPI | **CONFIRMED / CONSOLIDATED** | Stable `FIND-admin-principals-13`. Both operations are reachable public Wyrd routes; no authority permits the exception. |
| `RS-R6-1` — stale OpenAPI proof commands | **REVISED / CONSOLIDATED** | Stable `FIND-admin-principals-13`. This is the proof/guidance half of the same incomplete R5 OpenAPI correction. |
| `RS-R6-2` — residual qualified declarations | **CONFIRMED** | Stable `FIND-admin-principals-R5-1`; the prior mechanical cleanup was incomplete. |
| `SEC-WB06-1` — production epoch cache outlives revocation | **REVISED** | New `FIND-admin-principals-R6-1`. The reachable defect is confirmed; the minimum correction deletes epoch memoization and its now-useless invalidation machinery rather than wiring or expanding the listener. |
| `SEC-WB06-2` — more stable no-effect audit rollbacks | **REVISED / EXPANDED** | Stable `FIND-admin-principals-R5-2`; the earlier caller inventory was incomplete. |
| `CONTRACT-06-01` — local routes absent from OpenAPI | **CONFIRMED / DUPLICATE** | Stable `FIND-admin-principals-13`. |
| `CONTRACT-06-02` — MCP UUID schema/runtime mismatch | **REVISED** | Stable `FIND-admin-principals-R5-5`; typed schemas exist, but their identifier constraints still drift from dispatch. |
| Data-domain report — no proposal | **VALIDATED EMPTY** | Migration immutability, tenant isolation, audit staging/publication, and current retained schema remain closed at the reviewed boundary. |

### Rejected proposals and alternatives

- **REJECTED — authorize the local wildcard routes as an undocumented
  exception.** `REQ-049` says every served public route, and these routes are
  issued by server-owned upload/download planning, consumed by the shared
  client, authenticated by `Caller`, authorized, and exercised by real storage
  and CLI journeys. Local-only availability does not make them private or
  operational like `/healthz`; implementation notes are not product authority.
- **REJECTED — keep wildcard routing and add a manual OpenAPI path.** That
  restores the duplicate route knowledge stable `FIND-admin-principals-13`
  exists to delete and can again document a path different from the one Axum
  serves.
- **REJECTED — start or widen `RevocationListener` as the revocation fix.** It
  has no production spawn caller, omits `tenant_admin`, and asynchronous
  best-effort `NOTIFY` cannot prove the approved next-request guarantee. Once
  epoch results are read directly, the listener, notifications, cache key,
  configurable TTL, and `wyrd-auth`'s `moka` dependency have no caller or job.
- **REJECTED — add MCP-only UUID validators or schema annotations.**
  `PrincipalId`, `uuid::Uuid`, workspace `schemars` UUID support, and the
  installed `jsonschema` dependency already provide the contract and proof.
- **REJECTED — `SPEC_REVISION_REQUIRED`.** Every correction below implements
  already-approved behavior and requires no new public, security, concurrency,
  ownership, compatibility, or persistent-data decision.

## Deduplicated retained ledger

### `FIND-admin-principals-13` — REVISED — VIOLATION — the one runtime OpenAPI contract still omits two served Wyrd operations and its guidance names a removed proof

- **Wave-1 sources:** `TREV-WB06-1`, `RS-R6-1`, `CONTRACT-06-01`.
- **Violated obligation:** `REQ-049`, `AC-014`, `AC-019`, original TASK-008's
  independent-client outcome, the stable R5 finding, `AGENTS.md` §11, and the
  reference-router requirement that focused guidance agree with governing
  authority.
- **Exact location:**
  `crates/wyrd/wyrd-server/src/components/storage/routes.rs:35-57,320-345,396-423`,
  `crates/wyrd/wyrd-storage/src/service.rs:702-730,942-956`,
  `crates/wyrd/wyrd-server/tests/pg_openapi_contract.rs:1-12,106-147`,
  `architecture/references/languages/testing-workflows.md:160-166`, and
  `architecture/references/languages/agent-harness.md:77-81`.
- **Evidence and reachability:** `storage_router` is merged into the protected
  `/v1` router. Under the supported local backend it mounts
  `PUT /cards/upload/local/{*id}` and
  `GET /cards/download/local/{*path}` through plain Axum registration, outside
  the document. Upload initialization emits the former URL; download
  initialization emits the latter; `wyrd-client` dispatches both and the CLI
  storage journey reaches them. Both handlers authenticate, authorize, return
  Wyrd success/error shapes, and therefore are public Wyrd operations under the
  literal `REQ-049` boundary. The upload value is an `UploadId` and never needs
  a wildcard; only the download object's nested path carries slashes. The
  served-document test checks a selected list and repeats the false invariant
  that an undocumented served method cannot be written. Separately, both
  changed architecture references still prescribe the deleted
  `wyrd-server --lib http::openapi` target even though `mise.toml` now owns the
  suite as `pg_openapi_contract` under `test:principals:integration`.
- **Observable consequence:** An independent client reading `/openapi.json`
  cannot implement local artifact byte transfer or discover its body,
  authentication, success, problem media, and stable-error contract. A
  maintainer following either routed reference can also run a green selector
  that exercises no current OpenAPI contract test.
- **Decision-complete minimum correction:** Keep local byte transport and the
  existing shared client. Make the upload route an ordinary typed `UploadId`
  path operation. Move the slash-bearing local download locator into one typed
  request position representable by both Axum and OpenAPI (a query parameter is
  the direct fit), and update only the server-owned URL producer plus its shared
  consumer expectation. Restore `#[utoipa::path]` declarations with the real
  binary request/response, authentication, problem media, and reachable stable
  errors, and co-register both through `routes!`. Delete the exception comment;
  add no compatibility alias or manual path entry. Extend the assembled-server
  proof to include the two real method/path operations and update both reference
  documents to the canonical `mise run test:principals:integration` /
  `pg_openapi_contract` proof already named by `AGENTS.md`.
- **Focused closure proof:** With a local-backend `WyrdTestServer`, assert both
  transfer operations appear in served `/openapi.json` with their method,
  typed locator/body or binary response, global authentication, problem media,
  and stable codes; retain the nested-object local client/server round trip and
  CLI artifact journey. Confirm the documented `mise` lane selects a nonzero
  `pg_openapi_contract` suite and YAML remains unrouted.

### `FIND-admin-principals-R5-1` — CONFIRMED — VIOLATION — the cumulative candidate still contains qualified declaration types

- **Wave-1 source:** `RS-R6-2`.
- **Violated obligation:** `architecture/agent-rules.md` requires module-top
  imports and bare type names in fields, parameters, returns, associated types,
  bounds, and `where` clauses.
- **Exact location:** Residual production additions include
  `crates/wyrd/wyrd-mcp/src/client.rs:66,103,106,123`;
  `crates/wyrd/wyrd-sql/src/queries/auth/api_keys.rs:66,96`;
  `queries/auth/revocation.rs:111`;
  `queries/auth/role_assignments.rs:147`;
  `queries/auth/service_accounts.rs:232`;
  `queries/platform/tenant_resolver.rs:54`;
  `crates/shared/wyrd-client/src/auth.rs:126,280`;
  `eval/handle.rs:40,102`; `platform/handle.rs:38`;
  `principals/handle.rs:30`;
  `crates/wyrd/wyrd-auth/src/platform_authz.rs:114`;
  `platform_login.rs:99`; `platform_sessions.rs:120`; and
  `crates/wyrd/wyrd-server/src/components/platform/provisioning.rs:116` and
  `recovery.rs:48`, plus candidate-added test declarations found by the same
  cumulative declaration scan.
- **Evidence and reachability:** The cited lines are additions in the
  base-to-candidate range and use `reqwest::Client`, `reqwest::Error`,
  `sqlx::Error`, or `fmt::Result` in declarations. The MCP module even imports
  `reqwest::Error as ReqwestError` while retaining `reqwest::Error` in its
  return, bound, and associated type. These are compiled declarations in live
  owners, not expression paths or untouched baseline code. Format and Clippy do
  not enforce this repository-specific rule.
- **Observable consequence:** The candidate still fails a mandatory repository
  source-shape rule, and the new transport/SQL owners hide dependency ownership
  at their declarations.
- **Decision-complete minimum correction:** Complete the prior mechanical edit:
  add unambiguous aliases to each affected module's existing top-level import
  block (`ReqwestClient`, `ReqwestError`, `SqlxError`, `FmtResult`, or the local
  equivalent) and replace only candidate-added declaration occurrences. Leave
  expressions and untouched baseline declarations unchanged; add no permanent
  scanner.
- **Focused closure proof:** A cumulative base-to-candidate declaration scan
  returns no candidate-added qualified type in a field, parameter, return,
  associated type, bound, or `where` clause; `mise run fmt:check` and
  `mise run lints` pass.

### `FIND-admin-principals-R5-2` — REVISED — INCORRECT — additional stable no-effect outcomes discard an authorization decision

- **Wave-1 source:** `SEC-WB06-2`.
- **Violated obligation:** `REQ-037`, `AC-009`, and the canonical audit rule
  require every evaluated permission decision to be durable, allowed and
  denied alike.
- **Exact location:**
  `crates/wyrd/wyrd-server/src/components/platform/provisioning.rs:353-383`,
  `components/platform/identity.rs:424-462`, and
  `components/principals/routes.rs:175-190,209-230,270-321,360-375,426-450`.
- **Evidence and reachability:** Every path is served. `set_suspended` appends
  an allowance, then drops the transaction when the tenant is missing, already
  in the requested state, or otherwise ineligible. `register_admin` authorizes
  before the stable missing-platform-connection validation refusal and drops
  that decision. Credential issue and list authorize before
  `require_principal`; an unknown or foreign principal returns the stable
  non-enumerating not-found response without commit. Principal creation inserts
  a tentative row before learning that a syntactically valid requested role is
  absent; rollback correctly removes the row but also erases the already-made
  allowance. These are the same reachable root as R5-2, but were omitted from
  its caller inventory.
- **Observable consequence:** Authorized probes, replays, and invalid
  administrative attempts return stable responses with no durable record that
  permission was evaluated.
- **Decision-complete minimum correction:** Reuse each existing decision
  transaction. Commit it before returning missing/wrong-state tenant,
  missing-connection, and missing/foreign-principal logical refusals. For
  principal creation, resolve every requested role before inserting the
  principal; if a role is absent, commit the decision-only transaction and
  return the existing validation error. Preserve rollback of both decision and
  effect on actual query, mutation, hashing, append, or commit failure. Add no
  audit helper or secondary transaction.
- **Focused closure proof:** Real-Postgres cases cover wrong-state/replayed
  suspension, platform registration with no connection, HTTP and MCP issue/list
  against an unknown or foreign principal, and creation with an unknown role.
  Each asserts exactly one committed authorization row and no resource change;
  injected store failures retain zero committed effect and zero decision.

### `FIND-admin-principals-R5-5` — REVISED — VIOLATION — MCP advertises plain strings where dispatch requires UUIDs

- **Wave-1 source:** `CONTRACT-06-02`.
- **Violated obligation:** `REQ-036`, `AC-013`, the agent-harness typed-contract
  and durable-validation rules, and R5-5's requirement that the published and
  consumed MCP DTO be one contract.
- **Exact location:**
  `crates/wyrd-spec/src/auth/tenant_principals.rs:77-115`,
  `crates/wyrd/wyrd-server/src/mcp/principals.rs:98-114,142-170,178-229`, and
  `crates/wyrd/wyrd-mcp/tests/bifrost/mcp/principals.rs:22-41,83-122`.
- **Evidence and reachability:** `ListCredentialsArgs`,
  `RevokeCredentialArgs`, and `CredentialRevoked` describe UUID fields but type
  them as unconstrained `String`, so the derived schemas accept any string.
  Both dispatched tools then run a second `Uuid` parser and can reject an input
  the advertised schema accepts. The real MCP journey inspects only required
  property names and output presence. `PrincipalId` already derives
  serialization and `JsonSchema`; the workspace `schemars` feature supports
  `uuid::Uuid`; `jsonschema` is already installed.
- **Observable consequence:** An agent can construct a schema-valid tool call
  that fails before the administrative operation, and must infer UUID
  requirements from prose or a runtime error instead of the catalog.
- **Decision-complete minimum correction:** Type principal fields as the
  existing `PrincipalId` and credential fields as `uuid::Uuid` in the shared
  input and acknowledgement DTOs. Consume those values directly in the MCP
  handlers and delete `uuid_arg`; convert `PrincipalId` with `as_uuid()` only at
  the existing SQL-facing boundary. Add no MCP validator or identifier wrapper.
- **Focused closure proof:** Validate representative valid and invalid tool
  argument objects against each advertised input schema using the installed
  schema validator: UUIDs pass and malformed strings fail before dispatch.
  Validate the real listing and revocation results against advertised outputs,
  and retain scope gating plus discover → list/revoke → observe behavior.

### `FIND-admin-principals-R6-1` — REVISED — INCORRECT — production memoizes revocation epochs beyond the approved next-request boundary

- **Wave-1 source:** `SEC-WB06-1`.
- **Violated obligation:** `REQ-005`, `INV-013`, `AC-008`, `AC-010`, and the
  security posture's rule that verifier/permission caches cannot outlive the
  authorization epoch.
- **Exact location:**
  `crates/wyrd/wyrd-auth/src/revocation_resolver.rs:1-21,34-83,86-157`,
  `revocation_listener.rs:1-145`,
  `crates/wyrd/wyrd-server/src/boot/auth.rs:77-84`,
  `crates/wyrd/wyrd-auth/src/callback.rs:215-253`,
  `crates/wyrd/wyrd-server/src/auth/revoke.rs:100-139`, and
  `components/principals/routes.rs:540-603`.
- **Evidence and reachability:** Production boot constructs
  `SqlRevocationCheck::new`, which memoizes both present and absent epochs for
  five seconds. `TokenVerifier::verify` calls the resolver on both verified-token
  cache hits and misses, but the resolver can answer from that stale epoch
  cache. The listener named as the invalidation guarantee has no production
  `spawn` caller. Its parser also omits `tenant_admin`; the role-change callback
  advances a user epoch without any notification; and notification failure is
  intentionally best-effort. The task's zero-TTL and direct-database proofs do
  not exercise production settings. This cache/listener defect predates the
  branch, but it is in scope: revision 10 explicitly requires next-request
  credential, principal, role, and tenant-admin revocation, and the candidate
  materially changed this resolver, boot wiring, epoch writers, principal kinds,
  and acceptance evidence while claiming that obligation closed.
- **Observable consequence:** A token used once to warm a replica's epoch cache
  can retain removed role or principal/credential authority for up to five
  seconds after the revocation transaction commits.
- **Decision-complete minimum correction:** Stop caching authorization epochs.
  `SqlRevocationCheck::epoch` already acquires a tenant connection and reads
  tenant admission on every request; read the principal epoch directly through
  that same connection, preserving fail-closed error mapping and the verified-
  token cache. Then delete the epoch cache key/constants/TTL constructor,
  `invalidate`, `revocation_listener`, its notify fan-out call sites, and the
  now-unused `moka` dependency from `wyrd-auth`. Do not replace them with a
  listener, blacklist, or another cache.
- **Focused closure proof:** Under the production constructor, first verify an
  old token to warm the verified-token cache, then commit (separately) a
  same-second human role withdrawal, tenant-admin credential revocation, and
  principal revocation. The very next verification refuses each predecessor;
  the explicitly ordered OIDC successor is admitted. A resolver-read failure
  still returns `VerifyUnavailable`, and tenant suspension/resumption remains
  immediate.

## Prior-finding closure and ordering

R5 closes `FIND-admin-principals-R4-3`, `FIND-admin-principals-R4-5`,
`FIND-admin-principals-R4-9`, `FIND-admin-principals-R5-3`, and
`FIND-admin-principals-R5-4` at their named boundaries. Stable
`FIND-admin-principals-13`, `FIND-admin-principals-R5-1`,
`FIND-admin-principals-R5-2`, and `FIND-admin-principals-R5-5` remain open only
in the forms above. Earlier findings recorded closed by whole-branch-05 remain
closed; the owner's full waiver of `FIND-TASK-001-10` and the withdrawn
historical application/Iceberg audit-compatibility proposal are unchanged.

Implement in dependency order: remove stale epoch caching first; close every
decision-only refusal before its security proofs; correct shared MCP DTOs before
the MCP journey; make local transfer routes representable and co-registered
before extending the served-document suite and updating its guidance; finish
the declaration-only import cleanup last. Preserve tenant RLS, plane
separation, fixed-cost credential refusal, credential secrecy, ordered OIDC
successors, one canonical audit append/publisher, shared-client ownership,
local nested artifact paths, MCP scope gating, and the approved absence of
OpenAPI snapshots/YAML and Python/TypeScript administrative bindings.

## Verification limits

This was a static Wave-2 validation. It did not rerun the long environment-owning
suites. The R5 packet's recorded lanes are credible for their exact selections,
and the data-domain reviewer independently reran its migration and no-effect
proofs. None selects a documented local transfer operation, the current proof
command from both references, invalid UUIDs against the advertised MCP schema,
a production-constructor epoch read after warming caches, the newly traced
no-effect outcomes, or a cumulative declaration scan that includes the residual
sites. Those focused proofs are required before the retained ledger can close.

The candidate remained
`2c0408b683f7a548cec6dd08b35698d761d33b31` at the final integrity check.
