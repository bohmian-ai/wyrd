# Whole-branch 09 findings validation

## Immutable subject

- Repository: `/home/thorrester/Documents/GitHub/wyrd`
- Base: `c5c20754a167e8f4d74a555a720bd51df6179a6f`
- Candidate and reviewed HEAD:
  `84b7f4ad6eeaf2f20006761c35e34ce8d51ce7f6`
- Approved specification: `changes/active/admin-principals/spec.md`, revision
  13, SHA-256
  `57f91317e68b06e7b4d34ea94b964e4a1dd99678275a2ee67d1d51f9b4b46332`
- Original delivery: `changes/active/admin-principals/tasks/TASK-001-*.md`
  through `TASK-008-*.md`
- Prior cumulative review and remediation:
  `changes/active/admin-principals/review/whole-branch-08/`

I inspected the complete cumulative diff, every Wave 1 report, the current
source and callers, and the applicable authority. CodeGraph was used before
direct source inspection. The retained corrections reuse the existing ambient
client configuration, tenant issuer, canonical audit append, `Uuid` wire type,
and existing test owners. They add no credential-source abstraction, audit
path, identifier wrapper, compatibility path, or test harness.

The explicit human decisions are preserved: commit `67b4d0ba` and its
`FOR UPDATE NOWAIT` audit-publisher behavior remain, and no unshipped audit
schema/hash compatibility machinery is required.

## Wave 1 proposal disposition

| Wave 1 proposal | Disposition | Reason |
|---|---|---|
| `TREV-WB09-1`, `STD-R9-1`, `AUTH-SEC-01`, `CONTRACT-CLI-1` — CLI secrets remain in argv/debug-visible fields | **REVISED / CONSOLIDATED** | The path is live and the root is the still-incomplete stable `FIND-admin-principals-R8-2`. The minimum complete boundary is every shipped secret-valued option in the CLI command tree, not another command-family allowlist: TASK-008 placed pre-existing CLI auth/admin commands in scope, materially changed query/auth/eval consumers, and requires every Wyrd-calling command to use the shared client; the security posture unconditionally prohibits credentials in command arguments. Retained as `FIND-admin-principals-R8-2`. |
| `TREV-WB09-2`, `STD-R9-2`, `AUTH-SEC-02`, `BIFROST-R9-1`, `DATA-R9-01` — successful delegation loses caller credential attribution | **REVISED / CONSOLIDATED** | The successful path is reachable and violates the audit contract, but only the successful `delegation:issue` audit row needs the authenticating caller credential. The proposal to copy that id into the delegated JWT is rejected: `architecture/wyrd-security-posture.md` explicitly says a delegated token names no credential, and the token was minted by delegation rather than by presenting that API key. Retained as new `FIND-admin-principals-R9-1`. |
| `CONTRACT-CLI-2` — credential UUIDs are raw strings in the new contract and clients | **CONFIRMED** | These are durable UUID identifiers introduced by this change. The served handlers and existing MCP request/response contract already use `Uuid`, while the new issue/list DTOs, shared revoke methods, and CLI revoke arguments discard that type. Reusing `Uuid` closes the mismatch without a new wrapper or compatibility form. Retained as new `FIND-admin-principals-R9-2`. |

No Wave 1 proposal requires a specification revision. The rejected part of
`AUTH-SEC-02` is omitted from remediation rather than softened into optional
work.

## Final deduplicated finding ledger

### `FIND-admin-principals-R8-2` — REVISED — VIOLATION — the shipped CLI still accepts credentials through arguments

- **Wave 1 sources:** `TREV-WB09-1`, `STD-R9-1`, `AUTH-SEC-01`,
  `CONTRACT-CLI-1`.
- **Violated obligation:** stable `FIND-admin-principals-R8-2`, TASK-008's
  complete CLI consumer closure, AGENTS.md's secret-type rule, and
  `architecture/wyrd-security-posture.md` require credentials to stay out of
  command arguments and secret-bearing Rust values to have redacted debug.
- **Exact evidence and reachability:** the live top-level command tree routes to
  ordinary `String` or `Option<String>` fields for `--token` in
  `auth/issue_key.rs:37-42`, `auth/trusted_issuer.rs:80-113`,
  `auth/workload_binding.rs:40-79`, `principal/revoke.rs:37-42`,
  `query/mod.rs:23-29`, and `eval/run.rs:50-55`; `auth/refresh.rs:11-19`
  exposes `--refresh-token`; and `auth/trusted_issuer.rs:47-53` exposes the
  OIDC `--client-secret` even though that value is redacted after parsing.
  `auth/mod.rs`, `principal/mod.rs`, the top-level CLI, and the query/eval
  dispatchers make every path reachable. Environment fallbacks do not remove
  the corresponding long option, and most parsed fields remain printable
  beneath derived `Debug`. The branch materially moved these commands onto the
  shared client; R8 proved only the two newly added credential subcommands and
  explicitly left these sibling callers behind.
- **Observable consequence:** an operator can place an administrative bearer,
  refresh token, or OIDC client secret in shell history and process argv; most
  bearer fields can also appear in parsed-command diagnostics.
- **Decision-complete minimum correction:** delete every secret-valued CLI
  option named above while retaining endpoint and non-secret arguments. Reuse
  `crate::client::from_global(Some(server))` and the existing
  `ClientConfig` ambient credential chain for every tenant-authenticated
  command, including query and remote eval. Read refresh material only from
  `WYRD_REFRESH_TOKEN` into `SecretString`; retain only the existing
  environment/file sources for the issuer client secret. Once its callers are
  gone, delete the explicit-token `crate::client::client` builder and its stale
  documentation rather than preserving a second credential path. Do not add a
  credential-source type, another client builder, or another redaction layer.
- **Focused closure proof:** exercise the real root parser and prove every
  former `--token`, `--refresh-token`, and inline `--client-secret` spelling is
  rejected without echoing a sentinel value; prove ambient tenant credentials,
  refresh environment input, and issuer environment/file input still work and
  missing input fails clearly; run the existing CLI journey through those
  sources. A tree scan must find no secret-valued Clap field and no remaining
  caller of an explicit-token CLI client builder.

### `FIND-admin-principals-R9-1` — REVISED — INCORRECT — successful delegation omits the caller credential from its audit row

- **Wave 1 sources:** `TREV-WB09-2`, `STD-R9-2`, `AUTH-SEC-02`,
  `BIFROST-R9-1`, `DATA-R9-01`.
- **Violated obligation:** `REQ-037`, `INV-013a`, `AC-009`, and `AC-020`
  require each delegation authorization decision to be committed exactly once
  through the canonical audit path and to identify the credential that
  authenticated that decision when one exists.
- **Exact evidence and reachability:**
  `exchange_api_key.rs:235-313` verifies the presented subject token and retains
  its `verified.principal.credential_id`; denied and allowed-then-refused paths
  attach it in `record_decision` at `:325-347`. On successful issuance,
  `issuance.rs:111-134` carries no audit credential in
  `TenantGrant::Delegation`, and `exchange_audit_event` at `:503-548` builds the
  one allowed `delegation:issue` row without `with_credential_id`. A real caller
  whose JWT came from an API key therefore reaches a successful committed row
  with `credential_id = NULL`. Existing success tests synthesize a caller with
  no credential and assert count/attenuation only.
- **Observable consequence:** retained audit identifies the delegating
  principal and permission but cannot distinguish which of that principal's
  overlapping live credentials authorized the successful delegation.
- **Decision-complete minimum correction:** carry the verified caller's
  optional credential id as audit-only context through the existing
  `DelegateToken` to `TenantTokenIssuer` success path, then attach it to the one
  existing token-exchange/`delegation:issue` event before the transaction
  commits. Preserve `TenantGrant::credential_id()` returning `None` for
  delegation so the newly delegated JWT's `cid` remains absent, exactly as the
  security posture requires. Preserve subject identity, actor chain, permission
  intersection, TTL, Card scope, denial/no-effect behavior, and the single
  canonical event; add no second audit row, sink, or issuance path.
- **Focused closure proof:** exchange a real API key for a caller JWT with a
  known credential id, delegate successfully, and assert exactly one committed
  allowed `delegation:issue` row names both the caller principal and that API-key
  id while verification of the resulting delegated JWT still yields no
  credential id. Retain the denied, allowed-then-refused, attenuation, and
  audit-failure/no-token proofs.

### `FIND-admin-principals-R9-2` — CONFIRMED — VIOLATION — the new credential contract discards its UUID type

- **Wave 1 source:** `CONTRACT-CLI-2`.
- **Violated obligation:** AGENTS.md's durable-identifier rule and `REQ-036`,
  `AC-013`, and `AC-014` require one typed administrative contract across the
  server, generated schemas, shared client, CLI, and MCP.
- **Exact evidence and reachability:**
  `wyrd-spec/src/auth/tenant_principals.rs:37-67` declares
  `IssuedCredential.id` and `CredentialMetadata.id` as `String`, although the
  same module's `RevokeCredentialArgs.credential_id` and
  `CredentialRevoked.credential_id` are `Uuid`. The new shared methods in
  `wyrd-client/src/principals/handle.rs:157-171` and
  `platform/handle.rs:303-317` accept arbitrary `&str`, while the served
  handlers at `components/principals/routes.rs:510-535` and
  `components/platform/credentials.rs:210-213` extract `(Uuid, Uuid)`. The
  corresponding CLI revoke arguments in `principal/credential.rs:73-84` and
  `platform/credential.rs:93-104` also retain raw strings until request
  construction.
- **Observable consequence:** issue/list schemas omit the UUID contract and a
  shared-client or CLI caller can send malformed or path-shaping credential
  text that the typed server can only reject after transport.
- **Decision-complete minimum correction:** use the already imported/installed
  `uuid::Uuid` directly for issued and listed credential ids and for both shared
  client's revoke parameters; parse CLI text once at the CLI boundary before
  calling those methods. Reuse the existing UUID serializer/schema support and
  the MCP contract's current UUID shape. Add no credential-id newtype,
  string-compatibility overload, or client-side validation abstraction.
- **Focused closure proof:** generated JSON Schema and served OpenAPI publish
  credential ids with UUID format; MCP input/output validation remains aligned;
  one focused CLI/shared-client proof shows malformed ids cannot reach request
  construction; existing issue/list/revoke rotation journeys still pass.

## Validation limits

- This was a read-only static review. Recorded R8 lanes were inspected but not
  rerun.
- The three retained corrections are bounded implementation work within the
  approved specification; none needs a product, security, compatibility,
  concurrency, storage, or migration decision.
- Candidate HEAD remained
  `84b7f4ad6eeaf2f20006761c35e34ce8d51ce7f6` throughout validation.
