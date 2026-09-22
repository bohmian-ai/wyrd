# TASK-001-008-R9 — Close the final validated findings

## Route and authority

Implement this remediation with `$wyrd-implement`. The next
`$wyrd-task-review` must reassess the complete cumulative candidate.

- Approved specification: `changes/active/admin-principals/spec.md`, revision
  13, status `approved`, SHA-256
  `57f91317e68b06e7b4d34ea94b964e4a1dd99678275a2ee67d1d51f9b4b46332`.
- Original tasks: `changes/active/admin-principals/tasks/TASK-001-*.md`
  through `TASK-008-*.md`.
- Parent remediation:
  `changes/active/admin-principals/review/whole-branch-08/TASK-001-008-R8-close-validated-findings.md`.
- Review base: `c5c20754a167e8f4d74a555a720bd51df6179a6f`.
- Reviewed cumulative candidate:
  `84b7f4ad6eeaf2f20006761c35e34ce8d51ce7f6`.
- Validated ledger:
  `changes/active/admin-principals/review/whole-branch-09/findings-validation.md`.
- Remediates: `FIND-admin-principals-R8-2`,
  `FIND-admin-principals-R9-1`, and `FIND-admin-principals-R9-2`.

## Outcome

Finish the existing security and contract boundaries without adding another
auth, credential-source, audit, or identifier layer: no shipped CLI accepts a
secret in argv; successful delegation audit identifies the caller credential
without attributing it to the delegated JWT; and credential IDs remain UUIDs
through contracts, clients, and CLI parsing.

## Findings and required corrections

### `FIND-admin-principals-R8-2` — CLI secret removal is incomplete

R8 removed `--credential` and `--token` from the two new administration
endpoint structs, but the shipped command tree still exposes secret-valued
options in auth issue-key, trusted-issuer, workload-binding, refresh, principal
revoke, query, and remote eval. Environment fallbacks do not remove the long
option, and most bearer/refresh values remain plain strings beneath derived
`Debug`. Operators can therefore leak credentials through shell history,
process argv, or diagnostics, contrary to `INV-002` and the repository security
posture.

Delete every secret-valued option from the shipped CLI command tree while
retaining endpoints and non-secret arguments. Reuse the existing ambient
`ClientConfig` chain through `crate::client::from_global(Some(server))` for all
tenant-authenticated commands, including query and remote eval. Read refresh
material only from `WYRD_REFRESH_TOKEN` into `SecretString`; retain only the
existing environment/file sources for trusted-issuer client secrets. Delete
the explicit-token `crate::client::client` builder once no caller remains.
Do not add a credential-source abstraction, second builder, compatibility flag,
or another redaction wrapper.

### `FIND-admin-principals-R9-1` — successful delegation loses caller credential attribution

`DelegateToken` verifies a caller token that can carry the API key or refresh
credential which authenticated that principal. Denied and allowed-then-refused
decisions preserve that identifier, but the successful path reuses the issuer's
token-exchange event after dropping it. The resulting audit row names the
caller and `delegation:issue` permission but cannot identify which overlapping
caller credential authorized the exchange.

Carry the verified caller's optional credential ID as audit-only context
through the existing delegation grant to the successful exchange event, and
attach it before the one canonical row commits. Keep delegation's normal token
credential accessor returning `None`, so the delegated JWT remains
credential-unattributed: it was minted by delegation, not by presenting the
caller's credential. Preserve the subject, actor chain, permission
intersection, TTL, Card scope, denial/no-effect behavior, and exactly one event.
Add no second audit row, sink, or issuance path.

### `FIND-admin-principals-R9-2` — credential identifiers lose their UUID contract

The new issue/list DTOs expose credential IDs as strings, shared tenant and
platform revoke methods accept arbitrary string references, and CLI revoke
arguments carry those strings to request construction. The served route and
MCP contracts already use UUID, so the public surfaces disagree and malformed
credential text reaches transport unnecessarily.

Use the already installed `Uuid` type for issued and listed credential IDs and
for both shared-client revoke parameters. Parse CLI input once at the CLI edge
before calling the shared client. Reuse existing UUID serialization and schema
support and the MCP contract's current UUID shape. Add no credential-ID
newtype, compatibility overload, string fallback, or validation abstraction.

## Constraints and preserved behavior

- Preserve one `TenantTokenIssuer`, five-minute `permissions` JWTs, local
  tenant verification, and database-backed platform authorization.
- Preserve delegation authority intersection, chain representation and depth,
  target identity, Card scope, no-refresh behavior, and audit transaction
  semantics.
- The caller credential ID belongs only to the successful authorization audit;
  it must not enter the newly delegated JWT.
- Preserve the canonical audit append/publisher path and keep
  `67b4d0ba`'s `FOR UPDATE NOWAIT` behavior and replay proof.
- Preserve forced RLS, `TenantConn`/`OperatorPool` ownership, shared-client
  transport, canonical errors, runtime OpenAPI, MCP behavior, and generated
  artifact ownership.
- Wyrd is unshipped; add no predecessor audit compatibility machinery.
- Add no credential-source type, second client builder, identifier wrapper,
  compatibility form, second audit event/sink, or new test harness.
- Do not broaden this task into unrelated CLI UX or delegation redesign.

## Acceptance criteria

| Finding | Required observable result |
|---|---|
| `FIND-admin-principals-R8-2` | The root CLI parser rejects every former secret-valued option without echoing its value; no secret-bearing Clap field or explicit-token client-builder caller remains; ambient tenant credentials, refresh environment input, and trusted-issuer environment/file input work; missing credentials fail clearly; debug output never contains a supplied sentinel secret |
| `FIND-admin-principals-R9-1` | A real caller API key produces a caller JWT, successful delegation commits exactly one allowed `delegation:issue` row naming that credential and caller, and verification of the delegated JWT yields no credential ID; existing denial, no-effect, attenuation, and audit-failure behavior remains |
| `FIND-admin-principals-R9-2` | Issued/listed credential IDs and shared revoke APIs are UUID-typed; JSON Schema, served OpenAPI, and MCP agree; malformed CLI IDs fail before request construction; issue/list/revoke/rotation behavior remains unchanged |

## Focused proof and broader verification

Use existing owners and fixtures. Every named Rust test in the implementation
evidence must record its literal `mise exec -- cargo nextest run --locked ...
-E 'test(=...)'` command, positive selected count, result, and owning lane. Use
the repository-managed Postgres wrapper where required.

Focused proof must cover:

- the real root CLI parser rejecting every removed secret option without
  exposing a sentinel, plus ambient/environment/file success and missing-input
  failure through the existing CLI journey;
- successful API-key-backed delegation producing one credential-attributed
  allowed audit row while the delegated JWT has no credential ID; and
- UUID-formatted generated and served contracts, malformed CLI rejection
  before transport, and unchanged credential lifecycle journeys.

Run the narrowest owning lanes, including:

- `mise run fmt:check`
- `mise run lints`
- `mise run check:client-tier`
- `mise run check:unwrap-audit`
- `mise run check:clippy-allow-audit`
- `mise run check:tenant-isolation`
- `mise run check:from-pools-allowlist`
- `mise run test:principals:unit`
- `mise run test:principals:integration`
- `mise run test:shared`
- `mise run test:sql`
- `mise run test:platform:journey` twice consecutively
- `mise run test:identity:journey`
- `mise run test:cli:journey`
- `mise run test:bifrost:journey:mcp`
- `mise run test:bifrost:integration:server`
- `mise run test:bifrost:journey:server`
- `mise run codegen:check`
- `mise run docs:check`
- strict rustdoc for every affected crate
- `git diff --check c5c20754a167e8f4d74a555a720bd51df6179a6f <final-candidate>`

Do not substitute `mise run gate`, `test:rust`, another broad family aggregate,
or an ad hoc `--all-features` test lane. Append one evidence table mapping each
acceptance row to implementation commits, literal focused commands and selected
counts, owning lanes, and results.

## Implementation evidence

Candidate: `c65997085` (evidence recorded on top). Commits: `d699849ed`
(R9-2), `272b93b0f` (R9-1), `83bea8a1b` (R8-2), `580889579` (CLI reference),
`ec3b15fab` (identity journey reads CLI credentials from the environment),
`85ffc7baf` (operator journey supplies the tenant API key via `WYRD_API_KEY`),
`c65997085` (pre-existing broken `WyrdClient` intra-doc link, required for
strict rustdoc).

| Acceptance criterion | Implementation evidence | Verification evidence | Result |
|---|---|---|---|
| `FIND-admin-principals-R8-2` | `83bea8a1b`, `580889579`, `ec3b15fab`, `85ffc7baf`: every `--token`, `--refresh-token`, and inline `--client-secret` field removed; `client::client(server, token)` builder deleted, all callers use `client::from_global`; refresh reads `WYRD_REFRESH_TOKEN`; trusted-issuer secret from `--client-secret-file` or `WYRD_ISSUER_CLIENT_SECRET`; `ServerRequiresToken` removed, `WYRD_CLI_401_NO_REFRESH_TOKEN` added | `mise exec -- cargo nextest run --locked -p wyrd-cli --lib -E 'test(=principal::credential::tests::a_malformed_credential_id_is_refused_before_dispatch) \| test(=cli::tests::no_shipped_command_takes_a_secret_argument) \| test(=cli::tests::the_root_parser_refuses_every_former_secret_option_without_echo) \| test(=auth::refresh::tests::refresh_requires_only_the_server) \| test(=auth::refresh::tests::refresh_refuses_a_refresh_token_argument) \| test(=auth::trusted_issuer::tests::add_accepts_optional_client_secret_file_and_ttl) \| test(=auth::trusted_issuer::tests::add_refuses_an_inline_client_secret) \| test(=auth::trusted_issuer::tests::resolve_client_secret_reads_a_file)'` → 8 run, 8 passed (lane `test:principals:unit`/`test:cli:journey`); `mise exec -- cargo nextest run --locked -p wyrd-cli --test cli -E 'test(=secret_sources::trusted_issuer_add_reads_the_client_secret_from_the_environment) \| test(=secret_sources::trusted_issuer_add_reads_the_client_secret_from_a_file) \| test(=secret_sources::a_tenant_command_without_an_ambient_credential_fails_clearly) \| test(=secret_sources::refresh_reads_the_refresh_token_from_the_environment)'` → 4 run, 4 passed (lane `test:cli:journey`); `scripts/postgres/with-test-postgres.sh -- bash -lc "mise run db:migrate:inner && WYRD_CLI_E2E=1 mise exec -- cargo nextest run --locked -p wyrd-cli --test cli -E 'test(=operator_journey::operator_administers_a_deployment_through_the_cli)'"` → 1 run, 1 passed (lane `test:cli:journey`) | PASS |
| `FIND-admin-principals-R9-1` | `272b93b0f`: `TenantGrant::Delegation.caller_credential_id` set from the verified caller in `exchange_api_key::mint`; `exchange_audit_event` attaches it via `with_credential_id`; `TenantGrant::credential_id()` stays `None` for delegation so the delegated JWT carries no `cid` | `scripts/postgres/with-test-postgres.sh -- bash -lc "mise run db:migrate:all:inner && mise exec -- cargo nextest run --locked -p wyrd-auth --lib -E 'test(=exchange_api_key::pg_tests::a_successful_delegation_names_the_callers_credential_only_in_audit)'"` → 1 run, 1 passed (asserts exactly one `allowed` `delegation:issue` row naming caller + API-key id, delegated JWT verifies with `credential_id == None`; fails with the fix reverted) (lane `test:principals:integration`) | PASS |
| `FIND-admin-principals-R9-2` | `d699849ed`: `IssuedCredential.id`/`CredentialMetadata.id` are `Uuid`; `Principals::revoke_credential`/`Platform::revoke_credential` take `Uuid`; CLI `--credential-id` parses to `Uuid` at the Clap edge; server drops `.to_string()`; `codegen:regen` produced no checked-in schema diff | `scripts/postgres/with-test-postgres.sh -- bash -lc "mise run db:migrate:all:inner && mise exec -- cargo nextest run --locked -p wyrd-server --test pg_openapi_contract -E 'test(=credential_ids_publish_their_uuid_contract)'"` → 1 run, 1 passed (lane `test:principals:integration`); `scripts/postgres/with-test-postgres.sh -- bash -lc "mise run db:migrate:inner && mise exec -- cargo nextest run --locked -p wyrd-mcp --test mcp -P journey --run-ignored=all -E 'test(=principals::pg_tests::a_write_tool_is_scoped_at_dispatch_not_merely_hidden)'"` → 1 run, 1 passed (lane `test:bifrost:journey:mcp`); malformed CLI id: `principal::credential::tests::a_malformed_credential_id_is_refused_before_dispatch` in the wyrd-cli `--lib` command above | PASS |

Lanes (run on `ec3b15fab`; `test:cli:journey` rerun on `85ffc7baf`), all rc=0:
`fmt:check`, `lints`, `check:client-tier`, `check:unwrap-audit`,
`check:clippy-allow-audit`, `check:tenant-isolation`,
`check:from-pools-allowlist`, `test:principals:unit`,
`test:principals:integration`, `test:shared`, `test:sql`,
`test:platform:journey` (twice consecutively), `test:identity:journey`,
`test:cli:journey` (28 passed), `test:bifrost:journey:mcp`,
`test:bifrost:integration:server`, `test:bifrost:journey:server`,
`codegen:check`, `docs:check`. After `85ffc7baf`/`c65997085` (test-only and
rustdoc-only): `mise run fmt:check` rc=0 and
`mise exec -- cargo clippy --locked -p wyrd-cli --all-features --all-targets -- -D warnings` rc=0.
Strict rustdoc: `RUSTDOCFLAGS="-D missing_docs -D rustdoc::broken_intra_doc_links" mise exec -- cargo doc --locked --no-deps --all-features -p wyrd-spec -p wyrd-client -p wyrd-cli -p wyrd-server -p wyrd-auth -p wyrd-mcp` rc=0.
`git diff --check c5c20754a167e8f4d74a555a720bd51df6179a6f HEAD` rc=0.

Non-goals held: no credential-source type, second client builder (one was
deleted), identifier wrapper, compatibility form, second audit event/sink, or
new test harness; `FOR UPDATE NOWAIT` publisher code untouched. Local-only
note: the `check:*-audit`/`check:tenant-isolation` lanes invoke `python`,
absent on this host; they ran with a scratch `python -> python3` shim on
`PATH`, no repository change.
