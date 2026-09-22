# Repository standards review

Immutable subject: `c5c20754a167e8f4d74a555a720bd51df6179a6f..84b7f4ad6eeaf2f20006761c35e34ce8d51ce7f6`

## Review Findings

### Critical

None.

### Important

- **`STD-R9-1` — Secret-bearing CLI options violate the repository credential boundary.** [`crates/wyrd/wyrd-cli/src/auth/trusted_issuer.rs:52`](../../../../../crates/wyrd/wyrd-cli/src/auth/trusted_issuer.rs), [`crates/wyrd/wyrd-cli/src/auth/trusted_issuer.rs:84`](../../../../../crates/wyrd/wyrd-cli/src/auth/trusted_issuer.rs), [`crates/wyrd/wyrd-cli/src/auth/workload_binding.rs:44`](../../../../../crates/wyrd/wyrd-cli/src/auth/workload_binding.rs), [`crates/wyrd/wyrd-cli/src/auth/refresh.rs:18`](../../../../../crates/wyrd/wyrd-cli/src/auth/refresh.rs), [`crates/wyrd/wyrd-cli/src/auth/issue_key.rs:41`](../../../../../crates/wyrd/wyrd-cli/src/auth/issue_key.rs), [`crates/wyrd/wyrd-cli/src/principal/revoke.rs:41`](../../../../../crates/wyrd/wyrd-cli/src/principal/revoke.rs), [`crates/wyrd/wyrd-cli/src/query/mod.rs:28`](../../../../../crates/wyrd/wyrd-cli/src/query/mod.rs), and [`crates/wyrd/wyrd-cli/src/eval/run.rs:54`](../../../../../crates/wyrd/wyrd-cli/src/eval/run.rs) still publish `--token`, `--refresh-token`, or inline `--client-secret` options; most retain the value as a `String` inside a `Debug`-derived command tree. The `env` fallback does not make the long option safe: a caller may still place live bearer, refresh, or OIDC client credentials in shell history and process argv, and derived debug output can reproduce the `String` values. This violates `architecture/wyrd-security-posture.md` (“Credentials are not accepted through command arguments”), AGENTS.md's `SecretString` rule, and the same ambient-credential boundary R8 established for the new administration commands. Delete every secret-valued option from these materially touched CLI surfaces, reuse the existing tenant ambient credential chain, read refresh and issuer-client credentials only from their existing environment/file sources, and retain secret values in `SecretString` until the wire boundary; prove the shipped command tree rejects the removed option names, environment/file authentication still works, missing secrets fail clearly, and no debug representation contains the sentinel secret. Add no credential-source abstraction.

- **`STD-R9-2` — A successful delegated exchange drops the credential that authenticated its caller from audit.** [`crates/wyrd/wyrd-auth/src/issuance.rs:129`](../../../../../crates/wyrd/wyrd-auth/src/issuance.rs) returns `None` for `TenantGrant::Delegation`, and the successful delegation event at [`crates/wyrd/wyrd-auth/src/issuance.rs:533`](../../../../../crates/wyrd/wyrd-auth/src/issuance.rs) never calls `with_credential_id`; by contrast, the denied and allowed-with-no-effect branches at [`crates/wyrd/wyrd-auth/src/exchange_api_key.rs:325`](../../../../../crates/wyrd/wyrd-auth/src/exchange_api_key.rs) attach `verified.principal.credential_id`. Therefore the same directly credential-authenticated caller is attributable on refusal but loses credential attribution when delegation succeeds. This violates the security posture's rule that an authorization decision records the credential that minted the presented token, `REQ-037`, and `INV-013a`; it also leaves the successful-path test blind because its synthetic subject token sets no credential and asserts only outcome count. Carry the verified caller's optional credential ID through the in-memory delegation grant solely to the successful audit event, while keeping the newly delegated JWT's own `credential_id` absent, and add a Postgres-backed successful-delegation test whose caller token names a credential and whose single canonical audit row retains that exact ID; preserve the existing denied, allowed-no-effect, no-decision, and audit-failure proofs.

### Suggestions

None.

## Authority coverage

| Changed surface | Applicable authority read | Result |
|---|---|---|
| Tenant and platform principals, credentials, five tenant-token issuance paths, local JWT verification, delegation | `AGENTS.md` §§2, 4-6, 9; `architecture/agent-rules.md`; `architecture/wyrd-design.md` Runtime identity; `architecture/wyrd-security-posture.md`; `architecture/v1/00-foundations/security.md`; `service-identity.md`; `architecture/references/languages/rust-core.md` | **FAIL** — `STD-R9-2` |
| CLI, shared client, HTTP, OpenAPI, MCP, public errors, generated schemas | `AGENTS.md` §§2-4, 8-9; `architecture/wyrd-doctrine.mdx`; `architecture/references/architecture/patterns.md`; `languages/agent-harness.md`; `languages/errors.md`; security-posture secret handling | **FAIL** — `STD-R9-1` |
| Tenant SQL, platform SQL, migrations, RLS and connection ownership | `AGENTS.md` §§2, 4, 9; `architecture/agent-rules.md`; `architecture/v1/00-foundations/tenancy.md`; `architecture/references/languages/rust-core.md` Postgres boundary | PASS |
| Canonical audit staging, audit projection, publisher concurrency and retained Bifrost history | `AGENTS.md` §2; `architecture/agent-rules.md`; `architecture/wyrd-security-posture.md` Audit integrity; `architecture/bifrost-design.md` Query/audit contract; `architecture/references/domain/olap-serving.md` | PASS except the decision-attribution defect in `STD-R9-2`; the retained `FOR UPDATE NOWAIT` publisher behavior is not a standards violation |
| Rust modules, manifests, dependencies, imports, documentation and async boundaries | `AGENTS.md` §§4-6; `architecture/agent-rules.md`; `architecture/references/languages/rust-core.md` | PASS on the reviewed candidate and recorded strict-rustdoc/import/dependency evidence |
| Journey, integration, SQL, generated-artifact, docs and focused-test evidence | `AGENTS.md` §§11-12; `architecture/references/languages/testing-workflows.md`; `languages/spec-driven-development.md` | PASS for the recorded R8 commands, subject to the missing closure proofs identified above |
| Active specification and architecture/documentation amendments | `AGENTS.md` §§1-2, 14; `architecture/wyrd-design.md`; `architecture/wyrd-doctrine.mdx`; `architecture/wyrd-security-posture.md`; `languages/spec-driven-development.md` | PASS |

## Rule results

| Repository rule | Source and verification evidence | Result |
|---|---|---|
| Tenant work uses `TenantConn`; cross-tenant work uses `OperatorPool`; forced RLS is the sole tenant query boundary | Four redundant predicates were removed; `check:tenant-isolation`, `check:from-pools-allowlist`, the focused RLS test, and `test:sql` are recorded green | PASS |
| Every authorization decision uses the canonical audit path, commits at its decision boundary, fails closed, and is attributable | Delegation refusal and no-effect paths append and commit correctly, but successful `exchange_audit_event` omits the caller credential | **FAIL (`STD-R9-2`)** |
| Credentials never enter argv, logs, debug output, or generated artifacts; secret-bearing Rust values are redacted | New administration commands use ambient credentials, but the listed touched commands retain secret-valued clap flags and debug-visible `String`s | **FAIL (`STD-R9-1`)** |
| Tenant access tokens are five-minute permission snapshots verified synchronously without DB/cache/epoch machinery | `TokenVerifier` is concrete and store-free; issuance is centralized in `TenantTokenIssuer`; deleted-auth scans and protected-operation contract tests are recorded green | PASS |
| Delegation cannot amplify authority | `PermissionSet::intersection` and `TenantGrant::Delegation::ceiling` implement caller/target attenuation; focused unit, Postgres, and real-server journey evidence is recorded green | PASS |
| Public HTTP/MCP contracts use typed schemas and stable structured errors; generated artifacts come from their owners | Typed path IDs, canonical `PathRejection` mapping, served OpenAPI/MCP proofs, `codegen:check`, and docs checks are recorded green | PASS |
| Rust uses cohesive concrete owners, top-level imports, earned async, no unearned production lint suppression, and complete rustdoc | Concrete issuer/verifier owners remain; the scoped import was moved; lint audits and strict rustdoc for the R8-affected crates are recorded green | PASS |
| User-facing behavior has real journey proof and named tests select nonzero cases | All sixteen recorded focused commands selected one test; platform, identity, CLI, MCP, and Bifrost journeys are recorded green | PASS, with no proof yet for the two retained findings |

## Open Questions

None. Both corrections reuse existing owners and do not require a specification, public-contract, storage, concurrency, or migration decision.

## Verification Notes

- Candidate identity was checked before and after inspection and remained `84b7f4ad6eeaf2f20006761c35e34ce8d51ce7f6`.
- `git diff --check c5c20754a167e8f4d74a555a720bd51df6179a6f..84b7f4ad6eeaf2f20006761c35e34ce8d51ce7f6` is clean.
- The R8 evidence records all required focused selectors, lanes, strict rustdoc, generated-artifact checks, and docs checks as passing. Four Python-backed checks used a temporary `python -> python3` PATH entry because this host lacks `python`; the repository tasks were not changed.
- Existing passing delegation tests verify attenuation, outcome count, no-effect audit, and fail-closed audit behavior, but they do not assert successful credential attribution.
- Existing CLI parser tests accept several prohibited secret options; they are evidence of the live surface, not closure proof.

## Overall result

**FAIL**

The candidate violates two material repository security/audit rules. Both are bounded implementation corrections.
