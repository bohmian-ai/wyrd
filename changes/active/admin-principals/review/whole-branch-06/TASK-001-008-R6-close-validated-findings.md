# TASK-001-008-R6 — Close validated whole-branch findings

## Route and authority

Implement this packet with `$wyrd-implement`. The next `$wyrd-task-review`
must reassess the complete cumulative candidate, not only this remediation.

- Approved specification: `changes/active/admin-principals/spec.md`, revision
  10, status `approved`, SHA-256
  `05982825655110b7a514f16ffdd953e4b1e1df4b7422e9e77461f2e514a10837`.
- Original tasks: `changes/active/admin-principals/tasks/TASK-001-*.md` through
  `TASK-008-*.md`.
- Review base: `c5c20754a167e8f4d74a555a720bd51df6179a6f`.
- Reviewed candidate: `2c0408b683f7a548cec6dd08b35698d761d33b31`.
- Product-code candidate: `5ecc8a4e5a3a76390bc32f00353e558ea6a685d2`.
- Validated ledger:
  `changes/active/admin-principals/review/whole-branch-06/findings-validation.md`.

## Outcome

Close the five validated roots by reusing the existing route/OpenAPI owner,
typed identifiers, authorization transactions, direct epoch query, shared
verifier, schema validation support, and module imports. Add no route catalog,
schema layer, validator, cache, invalidation protocol, audit writer, or test
harness.

## Diagnoses and decision-complete corrections

### `FIND-admin-principals-R6-1` — stale authorization-epoch cache

Production `SqlRevocationCheck` memoizes present and absent epochs for five
seconds. The supposed invalidation listener is never spawned, excludes
`tenant_admin`, receives no role-change notification, and is best-effort. A
verified token can therefore retain revoked authority after the revocation
transaction commits, violating `REQ-005`, `INV-013`, `AC-008`, and `AC-010`.
Existing zero-TTL/direct-database tests do not exercise production wiring.

Stop caching authorization epochs. The resolver already acquires the tenant
connection and reads tenant admission on every request; extend that existing
SQL read to return both admission and the principal epoch in one database
round trip. Preserve fail-closed mapping and the separate verified-token cache.
Delete the epoch cache key, TTL constructor, invalidation API, listener,
notification fan-out and now-unused `moka` dependency. Do not add a second
query or replace them with another cache, listener or blacklist.

### `FIND-admin-principals-R5-2` — additional no-effect audit rollbacks

Served tenant-status, platform-admin registration and tenant principal
create/issue/list paths append an allowed authorization decision and then drop
the transaction on stable missing, foreign, wrong-state, missing-connection or
unknown-role outcomes. The response is deliberate but the decision disappears,
violating `REQ-037`, `AC-009`, and the canonical audit rule. Earlier R5 proof
did not cover these callers.

Reuse each route's existing decision transaction. Commit it before stable
logical no-effect responses. For principal creation, resolve all requested
roles before inserting the tentative principal; on an unknown role commit the
decision-only transaction and return the existing validation error. Preserve
rollback of decision and effect together on query, mutation, hashing, append or
commit failure. Add no helper or secondary transaction.

### `FIND-admin-principals-R5-5` — MCP UUID schema/runtime mismatch

The principal tool DTOs advertise identifier fields as unconstrained strings,
but dispatch separately parses UUIDs and rejects catalog-valid values. This
violates `REQ-036`, `AC-013`, the agent-harness typed-contract rule and the
prior R5-5 outcome that schema and consumed DTO be one contract. Current tests
only assert property names.

Use existing `PrincipalId` for principal fields and `uuid::Uuid` for credential
fields in the shared input and acknowledgement DTOs. Consume them directly in
the handlers, remove the duplicate UUID parser, and convert `PrincipalId` with
`as_uuid()` only at the SQL boundary. Reuse workspace schemars/UUID support and
the installed schema validator; add no MCP-specific validation layer or new ID
type.

### `FIND-admin-principals-13` — local transfer operations omitted from OpenAPI

The supported local backend serves authenticated upload and download routes
outside `utoipa-axum`, while server planning emits their URLs and the shared
client/CLI consumes them. They are therefore public Wyrd operations under
`REQ-049`, `AC-014`, and `AC-019`; a source comment cannot authorize their
omission. The contract test checks a selected list, and two routed references
still name a removed OpenAPI proof command. Existing CLI/storage journeys prove
the undocumented routes work, not that the contract is complete.

Keep local byte transport and the shared client. Make upload an ordinary typed
`UploadId` path operation. Move the slash-bearing download locator into one
typed request position representable by both Axum and OpenAPI, directly using a
query parameter, and update only the server-owned URL producer and shared
consumer expectation. Restore typed `#[utoipa::path]` declarations for binary
body/response, authentication, problem media and reachable errors, and
co-register both through `routes!`. Delete the exception comment; add no alias,
manual path or second document. Extend the assembled-server proof and update
both architecture references to the current nonzero
`test:principals:integration` / `pg_openapi_contract` proof.

### `FIND-admin-principals-R5-1` — residual qualified declaration types

The cumulative candidate still uses qualified type paths such as
`reqwest::Client`, `reqwest::Error`, `sqlx::Error`, and `fmt::Result` in new
fields, signatures, associated types and bounds. This violates the mandatory
module-import/bare-name rule; format and Clippy do not enforce it. The complete
confirmed location inventory is in `findings-validation.md`.

Finish the mechanical correction by adding unambiguous aliases to each
affected module's existing top-level import block and changing only
candidate-added declaration occurrences. Leave expressions and baseline
declarations untouched and add no permanent scanner.

## Constraints and preserved behavior

- Preserve the five closed principal kinds, platform/tenant plane separation,
  tenant RLS, `TenantConn`/`OperatorPool`, credential secrecy, fixed-cost
  credential refusal, ordered OIDC successors, human refresh rotation, and
  machine durable-credential re-exchange.
- Preserve one canonical audit append and sole publisher, audit fail-closed
  behavior, and rollback coupling for actual store/effect failures.
- Preserve the shared HTTP authentication owner, local nested artifact paths,
  MCP scope gating and framing, runtime `/openapi.json`, strict current Bifrost
  schema/hash behavior, and all already-closed R5 findings.
- Make the smallest owner-local correction; do not broaden the approved spec.

## Explicit non-goals

- No revocation cache/listener/blacklist replacement, route or error catalog,
  manual OpenAPI path, compatibility route, MCP validator/schema framework,
  audit helper/sink/table, migration framework, or permanent source scanner.
- No OpenAPI YAML/snapshot/generator, application/Iceberg historical audit
  compatibility, UI/browser state, Python or TypeScript admin bindings,
  platform-human CLI expansion, or workspace-wide cleanup.
- No history rewrite, identity/provenance edit, merge, push or deployment.

## Acceptance criteria

| Finding | Acceptance |
|---|---|
| `FIND-admin-principals-R6-1` | Under production resolver wiring, one combined admission-and-epoch query makes the next verification after role, tenant-admin credential, principal or tenant admission change observe durable state; ordered successors remain valid and read failures fail closed; obsolete cache/listener machinery is absent. |
| `FIND-admin-principals-R5-2` | Every newly named stable no-effect branch commits exactly one decision and no resource mutation; actual store failures commit neither. |
| `FIND-admin-principals-R5-5` | Advertised MCP schemas reject malformed UUIDs before dispatch and the same typed DTOs drive handlers/results while existing scope and journey behavior remains. |
| `FIND-admin-principals-13` | Both local transfer operations are typed, co-registered and present in served OpenAPI with real methods, authentication, binary contracts, problem media and stable errors; nested local transfer and CLI journeys pass; current proof guidance selects nonzero tests. |
| `FIND-admin-principals-R5-1` | No candidate-added declaration retains a qualified type path; format, lint and affected tests pass. |

## Focused proof

- Warm the verified-token path under the production resolver, commit separate
  same-second role withdrawal, tenant-admin credential revocation and principal
  revocation cases, and prove the next verification refuses each predecessor,
  admits the ordered successor, fails closed on resolver errors, and observes
  tenant suspension/resumption immediately.
- Exercise wrong-state/replayed suspension, missing platform connection,
  unknown/foreign principal issue/list over HTTP and MCP, and unknown-role
  creation against real Postgres; assert one decision and no mutation, while
  injected store failures commit neither.
- Validate valid and malformed UUID arguments against every advertised
  principal-tool input schema, validate real outputs, and retain the scoped
  discover/list/revoke/observe journey.
- Against a local-backend test server, inspect served OpenAPI for both transfer
  operations and their exact contract, then retain nested-path shared-client,
  storage-matrix and CLI artifact journeys; prove the documented lane selects
  a nonzero `pg_openapi_contract` suite and YAML remains unrouted.
- Run a cumulative declaration scan covering every confirmed residual site.

Record exact nonzero `mise exec -- cargo nextest run --locked` selectors for
named Rust tests with the owning environment wrapper. Confirm names with
`cargo nextest list`; no positional or zero-selection proof is acceptable.

## Broader verification

Run the narrowest existing `mise` tasks covering the changed owners, at
minimum:

- `mise run fmt:check`
- `mise run lints`
- `mise run check:client-tier`
- `mise run check:unwrap-audit`
- `mise run check:clippy-allow-audit`
- `mise run check:tenant-isolation`
- `mise run test:shared`
- `mise run test:principals:unit`
- `mise run test:principals:integration`
- `mise run test:platform:journey` twice consecutively
- `mise run test:identity:journey`
- `mise run test:cli:journey`
- `mise run test:bifrost:journey:mcp`
- `mise run test:storage:matrix`
- `mise run test:sql`
- `mise run test:bifrost:integration:redux`
- `mise run test:bifrost:integration:server`
- `mise run test:bifrost:integration:sql`
- `mise run codegen:check`
- `mise run check:examples`
- `mise run docs:check`
- strict affected-crate rustdoc where required
- cumulative diff-based Rustdoc and declaration audits
- `git diff --check c5c20754a167e8f4d74a555a720bd51df6179a6f <final-candidate>`

Do not substitute `mise run gate`; approved `VER-003` excludes it. Record one
evidence table mapping every acceptance row to commits, exact focused proof,
broader lanes and result.

## Implementation evidence

Status: `IMPLEMENTED`. Commits `4d185da9`..`9b36111a` on
`claude/admin-principals-spec-qfsmjc`.

| Acceptance criterion | Implementation evidence | Verification evidence | Result |
|---|---|---|---|
| `FIND-admin-principals-R6-1` | `34647786`: epoch cache, listener, NOTIFY fan-out, `moka`/`tokio-util` deps deleted; `user_admission`/`service_account_admission` read admission + `tokens_not_before` in one query (`wyrd-sql/src/queries/auth/revocation.rs`); `SqlRevocationCheck` uncached (`wyrd-auth/src/revocation_resolver.rs`); zero-TTL overrides removed from identity/CLI journeys; warm-token tenant-admin revocation test added | `test:platform:journey` ×2 (37/37), `test:identity:journey`, `test:cli:journey`, `test:principals:integration` | PASS |
| `FIND-admin-principals-R5-2` | `34647786`, `32a836bf`, `9b36111a`: unknown principal issue/list, absent role, unknown tenant status target, replayed resume, admin without connection commit the decision before refusing | `platform_admin_e2e::an_authorized_request_that_changes_nothing_still_records_the_decision` (in `test:platform:journey` ×2); MCP unknown-principal case in `test:bifrost:journey:mcp` | PASS |
| `FIND-admin-principals-R5-5` | `50fec116`: `ListCredentialsArgs`/`RevokeCredentialArgs`/`CredentialRevoked` use `PrincipalId`/`Uuid`; `uuid_arg` removed | `test:bifrost:journey:mcp` (schema accept/reject + output validation), `codegen:check` | PASS |
| `FIND-admin-principals-13` | `090a98fd`, `8b462d78`: upload/download co-registered with `#[utoipa::path]`; download locator is `?path=`; producer + client consumer updated; exception comment removed; both architecture references updated | `pg_openapi_contract::local_transfer_operations_publish_their_binary_contract` (in `test:principals:integration`), `test:storage:e2e`, `test:storage:matrix` e2e leaf, `test:cli:journey`, `wyrd-storage service::tests::local_download_url_uses_mounted_http_blob_route` | PASS |
| `FIND-admin-principals-R5-1` | `4d185da9`: `FmtResult`, `SqlxError`, `ReqwestClient`/`ReqwestError` aliases at every inventoried site | cumulative declaration scan (no declaration hits remain), `fmt:check`, `lints` | PASS |

Broader lanes passing: `fmt:check`, `lints`, `check:client-tier`,
`check:unwrap-audit`, `check:clippy-allow-audit`, `check:tenant-isolation`,
`test:shared` (668/668), `test:principals:unit`, `test:principals:integration`,
`test:sql`, `test:bifrost:integration:{redux,server,sql}`, `codegen:check`,
`check:examples`, `docs:check`; `git diff --check c5c20754..HEAD` clean.

Focused selector: `mise exec -- cargo nextest run --locked -p wyrd-storage --lib -E 'test(=service::tests::local_download_url_uses_mounted_http_blob_route)'` (1 passed).

Limits: the Python-based checks need `python` on PATH; this host only has
`python3`, so they ran with a local `python` → `python3` link. Service-account
credential revocation still sets `tokens_not_before = now()` (sub-second), so a
successor exchanged in the same second as the revocation is refused; this
predates the task and is not changed here.
