# Contract, CLI, MCP, and shared-client domain review

## Review Findings

### Critical

None.

### Important

- **`CONTRACT-CLI-1` — VIOLATION — reachable CLI authentication and administration commands still accept secrets in argv and retain them in debug-printable `String` fields.** `crates/wyrd/wyrd-cli/src/auth/issue_key.rs:16-42`, `auth/trusted_issuer.rs:31-113`, `auth/workload_binding.rs:25-80`, `auth/refresh.rs:11-20`, and `principal/revoke.rs:27-43` all derive `Debug` while accepting `--token` or `--refresh-token`; `trusted-issuer add` also continues to accept an inline `--client-secret`. These are not dormant parsers: `auth/mod.rs:18-47` and `principal/mod.rs:10-31` route every variant to the changed shared-client handlers, and the candidate's new `crate::client::client` converts the exposed `&str` only after Clap and the derived command tree already own the raw string. This violates the repository rule to hold secrets in `SecretString` with redacted `Debug`, leaves credentials in shell history and process listings, and makes assertion or diagnostic formatting capable of rendering a live credential. The affected auth/admin modules were materially changed by this candidate under `REQ-047` and TASK-008 Scenario 3, so this is not untouched pre-existing debt. **Required correction:** delete the secret-valued options from these commands; retain `--server`; reuse `crate::client::from_global(Some(server))` and the existing `ClientConfig` ambient chain for tenant access, read refresh material only from `WYRD_REFRESH_TOKEN` into `SecretString`, and retain only the existing file/environment sources for the OIDC client secret. Add no credential-source abstraction or second client builder. **Closure proof:** parser tests must reject each removed secret flag without echoing its value, formatted parsed arguments must contain no secret, missing ambient credentials must fail, and the existing real CLI journey must exercise the environment/config path.

- **`CONTRACT-CLI-2` — VIOLATION — the new credential contract and both shared administration handles discard the credential UUID type at the public boundary.** `crates/wyrd-spec/src/auth/tenant_principals.rs:37-67` exposes `IssuedCredential.id` and `CredentialMetadata.id` as unformatted `String`, while the same contract uses `Uuid` for `RevokeCredentialArgs::credential_id` at lines 93-104; `crates/shared/wyrd-client/src/principals/handle.rs:149-171` and `platform/handle.rs:294-311` then accept arbitrary `&str` and interpolate it directly into a path whose served OpenAPI and Axum extractor require a UUID. The CLI compounds this at `principal/credential.rs:73-84,183-190` and `platform/credential.rs:93-104,184-198` by accepting the durable identifier as a raw string with no edge validation. This violates AGENTS.md and `languages/rust-core.md`'s requirement to use domain types for durable identifiers, weakens the typed HTTP/SDK contract required by `REQ-036`, `AC-013`, and `AC-014`, and permits a shared-client caller to construct malformed or path-shaping credential identifiers that only fail after a request. **Required correction:** use the already-installed `Uuid` type consistently for issued/listed credential IDs and both shared-client revoke parameters, and parse CLI text once at the CLI boundary into `Uuid`; add no new identifier wrapper or compatibility string path. **Closure proof:** generated JSON schemas and served OpenAPI must publish credential IDs with UUID format, MCP schemas must remain aligned, a focused shared-client/CLI check must prove malformed IDs cannot reach request construction, and the existing credential rotation journeys must still pass.

### Suggestions

None.

## Open Questions

None.

## Domain coverage

| Boundary | Authority and source coverage | Result |
|---|---|---|
| CLI command tree and secret flow | Approved spec revision 13 (`REQ-036`, `REQ-047`, `INV-002`, `AC-010`, `AC-013`, `AC-014`, `VER-001`); R8 `FIND-admin-principals-R8-2`; AGENTS.md Rust secret rules; TASK-008 Scenario 3; all reachable `auth`, `principal`, and `platform` argument owners; `wyrd-cli::client`; `ClientConfig::resolve_credential` | **FAIL** — `CONTRACT-CLI-1` |
| Shared Rust client and SDK projection | AGENTS.md ownership/domain-type rules; `rust-core.md`; `wyrd-client::{Principals, Platform}`; `wyrd-sdk-rust` re-export; absence of new Python/TypeScript admin bindings | **FAIL** — `CONTRACT-CLI-2`; ownership and SDK-boundary shape otherwise pass |
| HTTP and served OpenAPI | `REQ-036`, `REQ-049`, `AC-014`, `AC-019`; administrative routers, typed path extractors, canonical `path_rejection`, served-document contract tests | **PASS** — changed administrative paths publish typed UUID parameters, list the reachable 400 validation problem, and map Axum path rejection through the canonical problem mapper |
| Stable public errors | AGENTS.md server/error rules; `languages/errors.md`; derive-backed `WyrdError`; administrative response annotations and shared-client decoding | **PASS** — no parallel error catalog or hand-built problem envelope found in the reviewed surfaces |
| MCP runtime and schemas | `agent-harness.md`; `REQ-036`, `AC-013`, `AC-014`; `mcp/principals.rs`; real MCP journey | **PASS** — inputs and outputs derive from shared DTOs, UUID inputs match runtime parsing, reads remain discoverable, writes remain scope-gated and re-authorized by the shared server operation |

## Verification Notes

- Immutable subject reviewed: `c5c20754a167e8f4d74a555a720bd51df6179a6f..84b7f4ad6eeaf2f20006761c35e34ce8d51ce7f6`; `HEAD` was rechecked as `84b7f4ad6eeaf2f20006761c35e34ce8d51ce7f6` after inspection.
- Reviewed the R8 evidence for the final-tree focused OpenAPI, MCP, CLI, codegen, docs, lint, journey, strict-rustdoc, and whitespace runs. No Cargo-backed command was rerun during this read-only review.
- The R8 CLI proof covers only the new `principal credential` and `platform credential` parsers; it cannot close `CONTRACT-CLI-1` because the shipped auth and principal-revoke command tree still advertises and accepts the secret flags above.
- Pre-existing `query --token` and `eval run --token` fields were inspected and excluded from findings here: their secret fields predate the review base and belong to unrelated query/eval surfaces, while the finding above is limited to the auth and administration surface this approved change materially consolidated.

## Overall Result

**FAIL**
