# Public contract domain review

## Subject and reviewed boundary

- Repository: `/home/thorrester/Documents/GitHub/wyrd`
- Cumulative base: `c5c20754a167e8f4d74a555a720bd51df6179a6f`
- Immutable candidate: `eb9b2f69cb883fa508ed168f21cb868451e61b82`
- Approved authority: `changes/active/admin-principals/spec.md`, revision 12
- Original delivery: `changes/active/admin-principals/tasks/TASK-001-*.md`
  through `TASK-008-*.md`
- Remediation under review:
  `changes/active/admin-principals/review/whole-branch-07/TASK-001-008-R7-close-validated-findings.md`

This review traced the changed public-contract boundary from the typed Rust
contracts through HTTP/OpenAPI extraction and problem projection, shared-client
and CLI consumers, MCP discovery and dispatch, generated schemas, and published
documentation. In particular it rechecked the local upload/download rejection
fix, principal UUID MCP schemas, tenant issuance and renewal descriptions,
administrative UUID path parameters, and the removal of the rejected
request-time revocation/permission-resolution design. The candidate remained
`eb9b2f69cb883fa508ed168f21cb868451e61b82` during inspection.

## Authority and source coverage

| Boundary | Governing authority | Source and consumer evidence | Result |
|---|---|---|---|
| Tenant-token public model | `REQ-012`–`REQ-012b`, `REQ-048`, `R7-AUTH-1`–`R7-AUTH-9` | `wyrd-auth/src/issuance.rs`; `wyrd-auth-verify/src/lib.rs`; server auth extractors; shared-client renewal; auth/identity/authorization docs | **FAIL — `CONTRACT-08-01`.** Runtime uses a local five-minute permission snapshot, but active CLI, MCP, rustdoc, OpenAPI, and docs still describe the rejected immediate-revocation, tenant-verifier, role-claim, or request-time database-resolution model. |
| Administrative HTTP/OpenAPI parameters and failures | `REQ-036`, `REQ-049`, `AC-014`, `AC-019`; AGENTS.md §9; error reference | Platform tenant, platform identity/credential, tenant principal/credential, and principal-revoke routes; `WyrdErrorResponse`; served OpenAPI tests | **FAIL — `CONTRACT-08-02`.** New UUID-backed path parameters are advertised as unconstrained strings and Axum rejects malformed values before the handler with an undocumented plain-text `400`. |
| Local transfer operations | Stable `FIND-admin-principals-13`; `REQ-049`; AGENTS.md §§9,11 | `components/storage/routes.rs:312-523`; `wyrd-storage/src/service.rs:703-744`; `pg_openapi_contract.rs:694-764`; shared-client local URL dispatch | PASS — upload `PathRejection` and download `QueryRejection` are received at the route boundary, mapped through `WyrdErrorResponse`, named in OpenAPI, and exercised through the assembled authenticated router; query-encoded nested paths and binary bodies remain intact. |
| MCP principal contracts | `REQ-036`, `AC-013`; agent-harness tool-contract rules | `wyrd-spec/src/auth/tenant_principals.rs`; `wyrd-server/src/mcp/principals.rs`; `wyrd-mcp/tests/bifrost/mcp/principals.rs` | PASS for schema and behavior — `PrincipalId`/`Uuid` drive the advertised input and output schemas and runtime parsing, malformed UUIDs are rejected by the catalog schema, real results validate, and the write operation still authorizes at dispatch. Its revocation description is part of `CONTRACT-08-01`. |
| Shared client and CLI ownership | `REQ-047`, `REQ-048`, `AC-014`, `AC-018` | `wyrd-client::{Principals,Platform,HttpTransport,AuthMiddleware}`; CLI principal/platform commands and journeys | PASS for transport ownership and renewal behavior — Wyrd-owned callers reuse `wyrd-client`, machine access tokens are cached and re-exchanged, and platform sessions keep their distinct current-state behavior. CLI descriptions are part of `CONTRACT-08-01`. |
| SDK and generated-artifact scope | TASK-008 approved scope; `AC-013`, `AC-014`; AGENTS.md generated-artifact rules | Rust SDK re-export; absence of new Python/TypeScript admin bindings; generated schemas and docs outputs | PASS — the approved delivery adds no Python or TypeScript administrative binding, the Rust SDK retains the shared-client re-export, and no hand-maintained OpenAPI snapshot returned. |

## Material proposed findings

### `CONTRACT-08-01` — VIOLATION — active public contracts still describe the deleted tenant revocation and permission-resolution model

- **Violated obligation:** `REQ-012`, `REQ-012a`, `REQ-017`, `INV-013`,
  `AC-013`, `R7-AUTH-5`, and `R7-AUTH-7` require tenant JWTs to carry their
  `permissions` snapshot, verify locally without a tenant-specific database
  resolver, and remain valid until their five-minute expiry after credential,
  principal, or tenant revocation. `REQ-049` also requires OpenAPI to name only
  reachable stable errors.
- **Exact location:**
  `crates/wyrd/wyrd-server/src/components/auth/token_extract.rs:44-52,103-111`
  still says an unverified tenant selects the “right tenant” verifier;
  `crates/wyrd/wyrd-server/src/mcp/principals.rs:129-136` says already-minted
  tokens stop authorizing on credential revocation;
  `crates/wyrd/wyrd-cli/src/principal/revoke.rs:45` and
  `crates/wyrd/wyrd-cli/src/principal/mod.rs:15` describe revoking outstanding
  tokens or credentials immediately;
  `docs/src/content/docs/self-hosting/running-the-server.svx:80-82` says tenant
  suspension stops already-minted tokens;
  `docs/src/content/docs/concepts/authorization.svx:3,30-31` says role claims
  are resolved against Postgres at verification;
  `docs/src/content/docs/concepts/identity-and-auth.svx:45,58-61,125-128`
  repeats role-claim and tenant-verifier resolution; and
  `docs/src/content/docs/reference/cli.svx:134-138` repeats the immediate
  credential-revocation claim. Tenant route annotations including
  `components/principals/routes.rs:253-260,348-355,395-402,479-486` and
  `components/storage/routes.rs:332-339,394-401,466-473` still list
  `WYRD_AUTH_401_CREDENTIAL_REVOKED`, although the concrete
  `TokenVerifier`/`AuthError` path has no revocation result.
- **Evidence and reachability:** `TokenVerifier::verify` now owns only local
  keys, issuer, audience, expiry/skew, and claim conversion; its `AuthError`
  variants include invalid, expired, malformed, and unavailable verification,
  but no credential or principal revocation result. Current state is read only
  when a new tenant token is issued. The MCP descriptor is returned by live
  tool discovery, Clap rustdoc becomes live `--help`, the docs are published
  (and copied into `docs/public/llms*.txt`), and the route descriptions are
  served at `/openapi.json`, so these are observable contract statements rather
  than dormant comments.
- **Observable consequence:** operators and agents are told a containment
  action kills existing tenant JWTs immediately or that verification consults
  current roles, while the accepted behavior intentionally preserves each
  token's authority until expiry; generated clients also see a tenant-route
  error code the request verifier can no longer produce.
- **Required testable correction:** update the existing descriptions in place
  to say revocation/suspension blocks new issuance immediately and existing
  tenant JWTs lapse at their five-minute expiry; describe `permissions` as the
  authoritative claim and local verification as one concrete verifier; remove
  `WYRD_AUTH_401_CREDENTIAL_REVOKED` only from tenant protected-operation
  responses where it is unreachable, preserving any issuance- or
  platform-plane use that remains real. Add no new documentation layer, error,
  checker, or revocation mechanism. Extend the served-document contract check
  so the tenant protected-route 401 inventory matches the concrete verifier
  outcomes, and make the existing MCP discovery assertion pin the corrected
  bounded-delay description.

### `CONTRACT-08-02` — INCORRECT — malformed administrative UUID paths bypass the published Wyrd problem contract

- **Violated obligation:** `REQ-036`, `REQ-049`, `AC-014`, `AC-019`, AGENTS.md
  §9, and the HTTP error authority require the administrative contract to be
  implementable from typed OpenAPI and public failures to flow through the
  canonical structured Wyrd error mapper.
- **Exact location:** representative pairs are
  `components/principals/routes.rs:348-372,395-419,478-504`,
  `components/platform/routes.rs:288-307,322-343`, and
  `components/platform/credentials.rs:64-84,120-139,180-204`.
  The same shape appears at
  `components/platform/identity.rs:570-592` and
  `auth/revoke.rs:50-77`: OpenAPI declares `String` while the handler directly
  extracts `Uuid`, `PrincipalId`, or `DataTenantId` through `Path<T>`.
- **Evidence and reachability:** a request such as authenticated
  `GET /v1/principals/not-a-uuid/credentials` or
  `GET /platform/tenants/not-a-uuid` is accepted by the advertised string
  schema but fails Axum's `Path<T>` extractor before the handler can return
  `WyrdErrorResponse`. Axum therefore serves its own plain-text `400`; several
  affected operations do not publish a `400` at all, and those that do publish
  a different domain failure. The local-transfer correction in this same
  candidate demonstrates the existing route-boundary mechanism: receive
  `PathRejection` and map it to the canonical problem response. Current
  `pg_openapi_contract` coverage checks route presence, media declarations,
  and the three local-transfer rejections, but never sends a malformed
  administrative path identifier.
- **Observable consequence:** an independent client generated from
  `/openapi.json` accepts path values the runtime parser refuses, then receives
  an undocumented non-JSON error with no stable code instead of the public Wyrd
  problem envelope.
- **Required testable correction:** advertise the existing typed UUID/domain
  identifier schema for every changed administrative path parameter, receive
  the corresponding existing Axum path rejection at each affected route
  boundary, and map malformed identifiers through the existing
  `WyrdErrorResponse` validation error; add the actually reachable `400`
  problem/code to those operations. Reuse the local-transfer rejection pattern
  and existing catalog—do not add middleware, a second parser, a route alias,
  or a new error type. Prove one tenant-principal, one platform-principal, and
  one platform-tenant malformed path through the assembled authenticated
  router, asserting the typed OpenAPI parameter plus `400
  application/problem+json` and its operation-listed stable code.

## Verification limits

This was a static, review-only audit. I inspected the final source and the
appended R7 evidence rather than rerunning its Cargo-backed lanes. The recorded
`fmt:check`, lints, client-tier checks, principal/MCP/CLI/identity/Bifrost
journeys, `codegen:check`, docs checks, strict rustdoc attribution, and exact
R7 selectors are credible for what they select. They do not falsify the two
findings above: codegen does not cover runtime OpenAPI, the served-document
suite does not compare tenant 401 codes to verifier outcomes, and its malformed
extractor proof covers only the local transfer routes.

## Overall result

**FAIL**
