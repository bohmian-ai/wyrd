# TASK-010 R3 repository standards review

## Immutable subject

- Repository: `/home/thorrester/Documents/GitHub/wyrd-oidc-mainline`
- Base: `fe51df0af6f6852fa7a8bc7276f3ce1d31c86daa`
- Candidate: `1f4466a9ae482eb1311f6e5206484758bc272ad8`
- Complete range: `fe51df0af6f6852fa7a8bc7276f3ce1d31c86daa..1f4466a9ae482eb1311f6e5206484758bc272ad8`
- Review role: repository standards only. This report does not decide task
  acceptance and does not perform the Ponytail validation pass.

The candidate remained the stated commit throughout this review. The
repository has no `.codegraph/` directory, so ordinary repository navigation
was used.

## Authority coverage

| Changed surface | Applicable authority inspected | Coverage result |
| --- | --- | --- |
| Workspace and crate manifests, lockfile, feature/dependency placement, and `mise` lanes | `AGENTS.md` §§1-6, 11-12, 15-16; `architecture/agent-rules.md`; workspace and changed crate manifests; `mise.toml` | Complete. The test-only `oauth2` dependency is pinned in the workspace and enabled only as a `wyrd-server` dev dependency; the removed application governor dependencies have no remaining auth use. |
| `wyrd-spec` OAuth/OIDC forms, token and callback contracts, schema generator, checked-in schemas, and error wire | `AGENTS.md` §§2-4, 8-9, 12, 16; `architecture/wyrd-design.md` runtime identity; `architecture/wyrd-doctrine.mdx`; `architecture/references/architecture/patterns.md`; `languages/rust-core.md`; `languages/errors.md`; `languages/agent-harness.md` | Complete. Source contracts, generated schema copies, endpoint error shapes, and public metadata were inspected together. |
| Shared Rust client and CLI auth projection, saved renewal, and first-class language journey consumers | `AGENTS.md` §§2-4, 8-11; `architecture/references/doctrine/architecture-constraints.md`; `patterns.md`; `testing-workflows.md` | Complete. Durable grant behavior remains server-owned; the Rust client/CLI and Python/TypeScript test consumers project the same form wire and credential behavior. No production Python, PyO3, or TypeScript API implementation changed. |
| `wyrd-auth` authorization-code, callback, device, issuance, refresh, revocation, sealing, connection, and audit owners | `AGENTS.md` §§3-6, 9, 11-12, 16; `architecture/agent-rules.md` tenancy/audit/async/documentation rules; `architecture/wyrd-security-posture.md`; `patterns.md`; `rust-core.md`; `errors.md` | Complete. Workflow owners, secret handling, transaction and audit boundaries, async IO, error conversion, rustdoc, and focused tests were inspected. |
| Server authorize/callback/token/device/revoke/metadata routes, config, boot, route composition, and served OpenAPI | Same server/security authorities plus `architecture/references/languages/agent-harness.md` and the approved RFC-shaped OAuth error exception | Complete. Typed forms, client identification, server-owned tenancy, request tracing, central OAuth refusal mapping, runtime route registration, config parsing, and OpenAPI tests were covered. |
| Auth SQL queries, row types, migrations, RLS and transaction ownership | `AGENTS.md` §§3-6, 9, 11-12, 15-16; `architecture/agent-rules.md` SQL capability rules; `patterns.md`; `rust-core.md`; `architecture/operations/deployment-and-release.md` migration contract | Complete. Production signatures and fields were inspected for `TenantConn`/`OperatorPool`, raw-pool propagation, caller-owned commit/rollback, RLS reliance, and schema/query agreement. The new `active_refresh` lookup relies on RLS alone; older tenant predicates outside that new lookup predate this range. |
| Rust unit/integration tests, real-server identity journeys, SQL/OpenAPI tests, CLI tests, and Rust/Python/TypeScript journey support | `AGENTS.md` §11; `architecture/agent-rules.md` test placement/runtime ownership/gate integrity; `spec-driven-development.md`; `testing-workflows.md` | Complete. Test tier, external-test placement, exact focused selectors, runtime ownership, and the task's recorded narrow verification were inspected. Full unfiltered and every-language journeys remain final change-review work under the standing direction. |
| Official image NGINX, startup script, deployment documentation, and the R3 lead correction | `AGENTS.md` §§11-12, 15; `architecture/agent-rules.md` gate-integrity rule; `architecture/operations/deployment-and-release.md` supported topologies and gateway ownership; `architecture/wyrd-design.md` production composition; `architecture/wyrd-security-posture.md` trust boundaries | Complete. The image-local limiter and its assertions are deleted; the supported single-tenant official-image startup proof remains intact; the operator documentation assigns `POST /auth/device` limiting to the public ingress. No edge manifest, application limiter, forwarded-header trust mechanism, setting, or state was introduced or required. |
| Documentation and active change artifacts | `AGENTS.md` §§11-16; `spec-driven-development.md`; applicable design, security, operations, and documentation authority | Complete. Public route and configuration prose, the superseding lead direction, and the routed TASK-011 finding were inspected. |

`architecture/references/README.md` routed this review to the authorities above.
Bifrost execution/storage, PyO3 implementation, and production Python or
TypeScript package rules are not materially changed by this task.

## Rule results

| Repository rule | Exact evidence | Result |
| --- | --- | --- |
| Durable identity and grant behavior stays in Rust server owners; clients only project the wire | `AuthorizationCodeExchange`, `CliLogins`, `RefreshTokens`, `TenantTokenIssuer`, `OAuthClients`, `TokenGrants`, and server route owners retain the durable behavior. `wyrd-client` form encoding and CLI saved-login changes call those endpoints rather than duplicating lifecycle or persistence. Current `mise run check:client-tier` passed. | PASS |
| `wyrd-spec` remains pure and generated contracts follow their source | The auth request/response and OAuth metadata types remain synchronous, IO-free, SQL-free, and PyO3-free. Generator inputs and both schema snapshot locations move together. Current `mise run codegen:check` passed. | PASS |
| Stateful workflows use cohesive concrete owners and pure helpers stay narrow | Human connections, callback completion/code redemption, device grants, issuance, refresh/revoke, OAuth client identification, and token routing are methods on dependency-owning structs. Free helpers are parsing, conversion, URL construction, error classification, or test-only fixture operations. No new single-implementation trait or utility owner was added. | PASS |
| Async is limited to IO and intentional IO composition | Added async paths await HTTP, SQL, server calls, or composed grant workflows. Form parsing, OAuth classification, PKCE validation, metadata construction, and conversions remain synchronous. | PASS |
| Production SQL uses `TenantConn` or `OperatorPool`, does not propagate raw pools, and leaves tenant transaction completion to the caller | Changed auth query/service signatures use the repository capabilities. Query helpers do not commit or roll back. The R1 `active_refresh` parallel tenant selector is absent. Current `check:tenant-isolation` and `check:from-pools-allowlist` passed. | PASS |
| Authorization outcomes use the canonical audit path at the owning transaction boundary | Callback completion, authorization-code/device redemption, refresh/reuse containment, revocation, issuance, and API-key exchange append through the existing auth audit owner before their caller-owned commit where authority or durable effects are established. No alternate audit table, relay, or publisher was added. | PASS |
| Secrets are redacted and never become public errors, logs, schemas, or durable plaintext | Authorization codes, refresh/device tokens, API keys, PKCE verifiers, and client secrets use `SecretString`, `SecretBearer`, hashes, or redacted debug behavior. Diagnostics and OAuth responses expose registered codes rather than credential material. | PASS |
| Public errors use the derive-backed Wyrd catalog except for the approved RFC OAuth response boundary | Non-OAuth handlers retain `WyrdError`/problem projection. OAuth endpoints centrally render the typed RFC 6749/RFC 8628 error bodies with no-store headers and do not create a second general error catalog. | PASS |
| New and materially changed Rust items satisfy mandatory rustdoc, including tests and fallible helpers | Direct inspection covered new structs, fields, variants, constants, workflow methods, private helpers, and tests. The prior tuple-field, associated-type, extractor, and parser gaps are documented; added fallible items name their error conditions and test helpers document panics. No placeholder rustdoc was found on the changed items. | PASS |
| No production `unwrap`, unjustified lint suppression, ignored check, or gate bypass was added | Added `expect` calls are in tests and name their invariant. No new production `#[allow]` or task-induced `#[ignore]` was found. Current `check:unwrap-audit` and `check:clippy-allow-audit` passed. | PASS |
| Dependencies and features stay in the narrowest owner | `oauth2 = 5.0.0` is exact, disables default features, and is a server dev dependency for the named off-the-shelf journey. Removing `governor`/`tower_governor` deletes the nonstandard application limit rather than replacing it. No Cargo feature or runtime dependency was added. | PASS |
| Served OpenAPI, schemas, docs, CLI, and client projections describe the same OAuth wire | Form request bodies, public `client_id`, RFC Basic alternative, metadata, typed responses, and route registrations align. Task evidence records the served-document integration lane green; current codegen and docs checks passed. | PASS |
| Test placement and verification follow the ranked taxonomy and narrowest-task direction | Pure cases stay in-module; SQL/server cases use repository fixtures and gated tests; language-runtime behavior remains in its owning runtime. TASK-010 records focused identity, SQL, principals/OpenAPI, platform, CLI, contract, format, lint, and typecheck evidence. Unfiltered journeys are intentionally deferred to change review. | PASS |
| The device user-code admission correction follows the native deployment boundary without inventing Wyrd-owned edge machinery | `docker/official/extras/nginx/nginx.conf.template` contains no `map`, `limit_req_zone`, `limit_req_status`, or `limit_req` device rule; the startup script no longer claims to prove client-address/fleet admission; the self-hosting page tells operators to limit `POST /auth/device` per client address at the public ingress. This matches the deployment authority assigning rate-limit integration to the operator-owned gateway and the standing comparable-project direction. | PASS |
| The startup lane remains a valid, non-weakened proof of the topology it declares | The script explicitly selects tenant `acme` and the supported owner-only file KEK, so it proves the official image's single-tenant self-hosted production topology. Config and Postgres integration tests own the separate multi-tenant fail-start invariant; no removed device-limit assertion was a valid multi-replica/public-edge proof. Current `mise run test:server:startup` passed its write, migration refusal/retry, and production verify phases. | PASS |
| Unreleased migration edits do not create a compatibility fork | The edited/deleted auth migrations belong to this unshipped active change, and current SQL/codegen evidence covers their resulting schema. No legacy table, route, grant alias, or compatibility migration was added. | PASS |
| No unsupported mechanism, check, file, setting, or option is introduced or required | The cumulative task uses RFC-defined OAuth/OIDC wire behavior, native Postgres RLS/transactions, the existing canonical audit path, the existing screened OIDC transport, and conventional operator-ingress limiting documentation. It adds no edge manifest, application limiter, trusted-header parser, cache/store, setting, or parallel protocol. `FIND-TASK-010-1` remains routed to TASK-011 and is not reopened. | PASS |

## Material findings

None.

## Non-blocking notes

- `git diff --check` reports one extra blank line at the end of the prior
  review artifact `review/TASK-010-r2/verdict.md`. It has no behavioral,
  security, tenancy, durability, public-contract, or executable-gate
  consequence. Under the standing direction, formatting/placement/wording-only
  observations do not block and are not findings.
- A few pre-existing changed-module signatures use fully qualified paths, and
  some SQL constants predating this task retain explicit tenant predicates.
  The task did not introduce those source-shape choices; the new R1
  `active_refresh` predicate was removed. These are not material findings for
  this immutable range.

## Verification evidence and limits

This independent standards pass ran against the stated candidate:

- `mise run fmt:check` — PASS
- `mise run check:client-tier` — PASS
- `mise run check:tenant-isolation` — PASS
- `mise run check:from-pools-allowlist` — PASS
- `mise run check:unwrap-audit` — PASS
- `mise run check:clippy-allow-audit` — PASS
- `mise run codegen:check` — PASS
- `mise run docs:check` — PASS
- `mise run test:server:startup` — PASS (`startup_image_journey:write`,
  `migration_refusal_and_retry`, `startup_image_journey:verify`)

The task's durable evidence additionally records green focused auth/server,
principals/OpenAPI, SQL, platform, CLI, Rust/Python/TypeScript identity,
format/lint, Python lint, and TypeScript typecheck lanes. Per the standing
narrowest-lane direction, full unfiltered and cross-language journey sweeps
remain change-review work and are not a task-review standards gap.

## Overall result

**PASS**

Repository-rule coverage is complete and no material standards violation
remains. The R3 correction deletes the invalid image-local limiter and places
only conventional operator guidance at the existing public-ingress boundary,
without adding or requiring Wyrd-owned edge or application machinery.
