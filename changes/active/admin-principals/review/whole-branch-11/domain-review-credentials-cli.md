# Credential and CLI security/domain review

## Immutable subject

- Repository: `/home/thorrester/Documents/GitHub/wyrd`
- Base: `c5c20754a167e8f4d74a555a720bd51df6179a6f`
- Candidate: `41e60be61958c92f562fceb5b7a03f40f611bbc8`
- Approved authority: `changes/active/admin-principals/spec.md`, revision 14
- Reviewed remediation tasks:
  `whole-branch-09/TASK-001-008-R9-close-validated-findings.md` and
  `whole-branch-10/TASK-001-008-R10-correct-rfc8693-delegation.md`
- Boundary: shipped CLI credential inputs and debug/error surfaces; ambient
  tenant, platform, refresh, and trusted-issuer secret resolution; credential
  identifier contracts through spec, client, CLI, HTTP/OpenAPI, and MCP;
  delegated-token credential attribution in audit versus JWT claims; retained
  audit-publisher `FOR UPDATE NOWAIT` behavior.

## Authority and source coverage

| Boundary | Authority | Source and proof inspected | Result |
|---|---|---|---|
| CLI secrets stay out of argv, parser diagnostics, and derived `Debug` | Spec `INV-002`, `REQ-047`; security posture “Secrets”; R9 `FIND-admin-principals-R8-2`; `AGENTS.md` Rust secret rule | `wyrd-cli/src/cli.rs`, `auth/{issue_key,refresh,trusted_issuer,workload_binding}.rs`, `principal/{credential,revoke}.rs`, `platform/credential.rs`, `query/mod.rs`, `eval/run.rs`, `client.rs`; `wyrd-client` `ClientConfig`, `CredentialSource`, `ResolvedCredential`, `AuthMiddleware`; `SecretBearer` | PASS |
| Ambient secret sources preserve one client/auth owner | Spec `REQ-047`; R9 required correction | Tenant commands route through `client::from_global`; refresh reads only `WYRD_REFRESH_TOKEN`; platform commands read only `WYRD_PLATFORM_CREDENTIAL`; trusted-issuer client secrets come from `WYRD_ISSUER_CLIENT_SECRET` or `--client-secret-file`; the removed explicit-token CLI builder has no caller | PASS |
| Credential identifiers are UUID contracts on every affected surface | R9 outcome and `FIND-admin-principals-R9-2`; spec `REQ-009`, `REQ-036` | `tenant_principals.rs`, `issue_key.rs`, shared `Principals`/`Platform`, tenant/platform CLI, server route/OpenAPI declarations, MCP argument/output types and journey, `pg_openapi_contract.rs` | **FAIL** (`CRED-CLI-1`) |
| Successful delegation attributes the actor's credential only to audit | Spec `REQ-012c`, `REQ-037`, `INV-013a`; R9 `FIND-admin-principals-R9-1`; R10 audit criterion | `DelegateToken::execute`, `TenantGrant::Delegation`, `TenantGrant::credential_id`, `TenantGrant::into_access_grant`, `exchange_audit_event`, focused PostgreSQL assertions | PASS |
| Delegation input secrets remain redacted and memory-only | Spec `REQ-047`, `INV-002`; R10 client-helper criterion | `TokenRequest`, `SecretBearer`, `ResolvedCredential::Delegated`, `AuthMiddleware::on_behalf_of`/`exchange_delegated`, Rust/Python/TypeScript projections | PASS |
| Audit publisher contention behavior remains intact | R9 preserved behavior; doctrine canonical audit publisher | `vala-sql/src/queries/audit_staging.rs`; diff from `67b4d0ba4` through candidate shows no change to the `FOR UPDATE NOWAIT` freeze implementation | PASS |

## Material proposed finding

### `CRED-CLI-1` — Card-bound issued credential IDs still publish as free-form strings

- **Classification:** INCORRECT
- **Violated obligation:** R9 requires credential IDs to remain UUIDs through
  contracts, clients, CLI, MCP, and OpenAPI, using the already-installed
  `Uuid` type without a second identifier layer.
- **Exact location:**
  `crates/wyrd-spec/src/auth/issue_key.rs:30` declares
  `IssueKeyResponse.key_id: String`; `crates/wyrd/wyrd-auth/src/issue_api_key.rs:127`
  converts the database UUID with `api_key_id.to_string()`; and
  `crates/wyrd/wyrd-server/tests/pg_openapi_contract.rs:526-538` checks only
  `IssuedCredential.id` and `CredentialMetadata.id`, omitting the shipped
  `IssueKeyResponse.key_id` contract.
- **Evidence:** `POST /auth/issue-key` returns `IssueKeyResponse` from the same
  UUID-backed `wyrd.auth_api_keys` credential model, and the shipped
  `wyrd auth issue-key` command exposes that value, but its derived OpenAPI and
  JSON Schema advertise an unconstrained string while the generic issue/list
  credential responses advertise `format: uuid`.
- **Observable consequence:** generated clients and independent API consumers
  cannot rely on the promised UUID credential-ID contract for Card-bound key
  issuance, leaving one issuance surface inconsistent with list/revoke/MCP and
  permitting malformed values in generated client models.
- **Required testable correction:** change the existing
  `IssueKeyResponse.key_id` field to `Uuid`, pass `api_key_id` directly from
  the existing issuer, and extend the existing served-OpenAPI credential-ID
  contract test (and normal schema regeneration) to assert
  `IssueKeyResponse.key_id` has `format: uuid`; do not add a newtype,
  compatibility field, parser, or route.
- **Focused closure proof:** the served `/openapi.json` test includes
  `IssueKeyResponse` beside `IssuedCredential` and `CredentialMetadata` and
  observes `properties.key_id.format == "uuid"`; the existing issue-key route
  test parses the returned ID as the same UUID produced by issuance.

## Security Audit

### Critical

- None.

### High

- None.

### Medium

- [`crates/wyrd-spec/src/auth/issue_key.rs:30`] The Card-bound credential
  issuance response weakens its UUID identifier to an arbitrary string, so
  generated clients do not enforce the same identifier contract as the other
  credential surfaces; retain the database UUID through the wire type and
  generated schemas as described in `CRED-CLI-1`.

### Low / Defense In Depth

- None within the approved R9/R10 boundary.

### Positive Controls

- The real root parser rejects all former secret options without echoing the
  supplied sentinel, and no shipped Clap field is backed directly by a secret
  environment variable.
- `SecretBearer`, `ClientConfig`, `CredentialSource`, `ResolvedCredential`,
  `AuthMiddleware`, and the principal/platform handles redact or omit secret
  state from `Debug`.
- Refresh, platform, ambient tenant, and trusted-issuer secret paths fail before
  dispatch when their required input is absent and do not place secret bytes in
  structured errors.
- Delegation records the actor credential on the one canonical successful
  exchange audit event while `TenantGrant::credential_id()` remains `None` for
  delegation, so the delegated JWT carries no `cid`.
- Subject and actor bearers are `SecretBearer`/`SecretString` in the Rust owner,
  are skipped by tracing instrumentation, are cached only in memory for
  delegation, and are not reimplemented by the Python or TypeScript wrappers.
- The audit publisher retains its non-blocking `FOR UPDATE NOWAIT` chain-head
  lock and replay-safe frozen range behavior.

## Verification and limits

- Independently ran the exact eight-test `wyrd-cli` selector from the R9
  evidence: **8 selected, 8 passed**. It covers the root secret-option scan and
  refusals, refresh argument refusal, trusted-issuer file source, and malformed
  credential-ID refusal before dispatch.
- Read the focused PostgreSQL delegation assertions that require exactly one
  successful allowed exchange row under the subject with the actor credential,
  and verify the delegated principal has `credential_id == None`; the database
  lane was not rerun in this review.
- The task evidence reports the owning PostgreSQL, MCP, OpenAPI, codegen, CLI
  journey, lint, and documentation lanes passing; this review did not rerun
  those broader lanes.
- Per reviewer instruction, the accepted five-minute self-contained-JWT
  revocation window was excluded from findings.

## Overall result

**FAIL** — the CLI secrecy, ambient credential paths, delegation credential
attribution, and retained audit-publisher behavior satisfy the reviewed
obligations, but `CRED-CLI-1` leaves one shipped credential issuance contract
outside R9's UUID correction.
