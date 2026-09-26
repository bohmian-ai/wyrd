# Repository standards review — TASK-002-r3

## Immutable subject

- Repository: `/home/thorrester/Documents/GitHub/wyrd-oidc-complete`
- Base: `3fc085acf5b3a710d5dc80892bd2e664b3db6174`
- Candidate: `d861845f3f5d89aca413857dcfb8c1bbfaee349d`
- Reviewed range: the complete cumulative base-to-candidate diff (68 files;
  task and prior-review artifacts, Rust contracts/client/auth/server/SQL,
  migration and generated schemas, docs, fixtures, tests, Docker, and mise
  configuration).
- The lead-authorized reuse-cleanup commits recorded in the R2 evidence table
  and the two unrelated `test:wyrd` flaky-test repairs were treated as
  directed scope and audited against repository rules, not classified as
  scope drift.
- The candidate identity was rechecked after verification and remained
  `d861845f3f5d89aca413857dcfb8c1bbfaee349d`.

The repository has no `.codegraph/` directory, so inspection used Git and
source directly.

## Authority coverage

| Changed surface | Applicable authority | Coverage and result |
|---|---|---|
| Repository workflow, task/remediation evidence, completion, and documentation | `AGENTS.md` §§1–6, 9, 11–12, 15–16; `architecture/agent-rules.md`; `architecture/references/languages/spec-driven-development.md`; `architecture/references/languages/implementation-execution.md` | The approved task, both remediation tasks, cumulative diff, R2 evidence table, and verification records are present. **FAIL** only because one reuse cleanup leaves materially modified Rust documentation false (STD-R3-001). |
| Public auth contracts, client/server ownership, HTTP/OpenAPI/schema alignment | `architecture/wyrd-design.md`; `architecture/wyrd-doctrine.mdx`; `architecture/references/doctrine/architecture-constraints.md`; `architecture/references/architecture/patterns.md`; `architecture/references/languages/errors.md` | Typed login contracts remain in `wyrd-spec`; durable login behavior remains on `HumanConnections` and `AuthorizationCodeExchange`; server routes, shared client, generated schemas, OpenAPI proof, CLI removal, and docs agree. No new compatibility surface or parallel error catalog entered the range. **PASS**. |
| Identity, external-provider trust, secrets, authorization, and audit | `architecture/wyrd-security-posture.md`; `architecture/wyrd-design.md` runtime identity; `architecture/agent-rules.md`; architecture patterns for server, external network, and audit | Tenant and connection selection remain state/route-key bound as designed; provider IO uses screened owners; PKCE and completions are redacted/sealed; issuer, audience, signature, time, nonce, `azp`, and advertised-algorithm checks precede issuance. Role-sync and token-exchange events use the canonical transactional append. The audit reuse preserves operation, resource, principal, outcome, and detail behavior. **PASS**, with the documentation violation in STD-R3-001; the actual composed verifier still rejects HMAC. |
| Tenant SQL, RLS, transactions, and migration | `AGENTS.md` §§9, 15; `architecture/agent-rules.md`; `architecture/wyrd-security-posture.md`; architecture constraints/patterns; `architecture/references/languages/rust-core.md` Postgres rules | Tenant state transitions use `TenantConn`, rely on forced RLS, contain no duplicate tenant selector, and leave commit ownership to service callers. State-to-tenant lookup is an inherent `WyrdPostgres` capability over the private application pool; the SECURITY DEFINER function is least-disclosure and restricted. **PASS**. |
| Rust structure, imports, async, errors, documentation, and reuse cleanup | `AGENTS.md` §§4–6, 9, 16; `architecture/agent-rules.md`; `architecture/references/languages/rust-core.md`; errors reference | Stateful workflows remain on concrete owners; pure checks remain synchronous; async paths await IO; module-top imports and R2 rustdoc gaps are corrected. `principal_event` is the existing-owner extraction needed by role-sync audit. **FAIL** for STD-R3-001: `verify_id_token_algorithm` documentation still promises a filter deleted by the cleanup. |
| Tests, journey placement, generated artifacts, OpenAPI, and lane repairs | `AGENTS.md` §11; `architecture/agent-rules.md`; `TESTING.md`; `architecture/references/languages/testing-workflows.md` | Required login/refusal/switch/machine journeys remain in the gated identity target; SQL and served-OpenAPI integration proofs remain in their proper tiers. Gateway terminal frames now compare parsed JSON, and the Forge readiness test waits for the boot pass; neither repair weakens or ignores an assertion. Generated artifacts reproduce cleanly. **PASS**. |
| CLI/docs/operator guidance | `AGENTS.md` §§2, 9, 12; Wyrd doctrine/design; architecture patterns | Obsolete direct code exchange/CLI login documentation is removed consistently. The sealing-key runbook now preserves the old key for the two-minute completion TTL after the post-roll pass, matching the runtime exclusion documented in `sealing.rs`. **PASS**. |
| Fixtures and repository tooling | `AGENTS.md` §§11–12, 15–16; `TESTING.md`; testing-workflows reference; `mise.toml` | Keycloak fixtures and identity selectors remain repository-managed integration infrastructure. No credentials, production dependency, Cargo feature, or parallel test harness was added. **PASS**. |

## Applicable rule results

| Rule | Evidence | Result |
|---|---|---|
| Durable behavior stays in Rust server owners; public contracts stay typed and language agnostic | `wyrd-spec/src/auth/oidc.rs`, `wyrd-auth/src/login.rs`, `wyrd-auth/src/callback.rs`, server adapters, generated schemas, and docs preserve the required split. | PASS |
| Stateful and dependency-backed workflows use cohesive concrete owners | Login begin/redemption are `HumanConnections` methods; callback completion is an `AuthorizationCodeExchange` method; state resolution is on `WyrdPostgres`. | PASS |
| Async is limited to real IO/composition | New async paths await database, HTTP, mock server, or assembled-server IO; validation, hashing, claim checks, and URL construction remain synchronous. | PASS |
| Tenant work uses `TenantConn`, forced RLS, and caller-owned transactions | Login-state query signatures and SQL in `wyrd-sql/src/queries/auth/login_state.rs`; fresh `check:tenant-isolation` and `check:from-pools-allowlist` both passed. | PASS |
| Secrets stay redacted and external provider fetches use the screened owner | `LoginState.code_verifier` is `SecretString`; completion payloads are sealed; discovery, token, and JWKS calls use the existing screened HTTP path. | PASS |
| Public errors use stable Wyrd mappings | New refusals use existing `WyrdError` variants and the single server response mapper; no hand-written problem JSON or code catalog was added. | PASS |
| Audit uses the canonical transactional path | `principal_event` factors the common event core; `auth_event` adds the prior detail unchanged; `roles_sync_event` supplies the same principal resource, operation/permission, and allowed outcome before the issuing transaction commits. | PASS |
| Imports live at module top | The SHA-256, Utoipa, and Wiremock imports cited by R2 now reside in their module import blocks with the existing feature gate/alias. | PASS |
| New/materially modified Rust items have accurate, substantive rustdoc including `# Errors`/`# Panics` | R2's cited schema traits, SQL constants, signing helper, and alias are documented. The later HMAC-filter cleanup materially changed `verify_id_token_algorithm` without making its contract documentation true. | **FAIL — STD-R3-001** |
| Generated artifacts derive from owning sources | Fresh `mise run codegen:check` regenerated schemas/stubs and reported `All checks passed!`; cumulative `git diff --check` passed. | PASS |
| User-visible login behavior has journey and supporting integration proof | Four gated identity journeys, callback/SQL seam tests, and served OpenAPI contract tests remain in the candidate; recorded final `test:wyrd` result is 2213 passing tests. | PASS |
| Lead-directed flaky-test repairs do not circumvent a gate | Gateway assertions still require exact terminal JSON values while ignoring object key order; Forge readiness still asserts standby false then recovered true after synchronizing with the boot pass. No `allow`, deleted assertion, or new ignore was introduced by either repair. | PASS |

## Material findings

### STD-R3-001 — BLOCK_BEFORE_MERGE: the algorithm-policy helper documents a filter it no longer performs

- **Violated authority:** `AGENTS.md` §16, `architecture/agent-rules.md`, and
  `architecture/references/languages/rust-core.md` require every materially
  modified Rust item to carry substantive, accurate rustdoc and require a
  fallible function's `# Errors` section to name its real error conditions.
- **Exact location and evidence:**
  `crates/wyrd/wyrd-auth/src/callback.rs:568-585` says
  `verify_id_token_algorithm` requires an asymmetric advertised algorithm and
  returns `InvalidToken` outside the advertised asymmetric set. Commit
  `3747b2d26` removed the HMAC filter; the implementation now returns `Ok(())`
  whenever an advertised `HS256`, `HS384`, or `HS512` equals the header.
  The only current caller immediately invokes
  `ExternalVerifier::verify_external_against`
  (`callback.rs:211-216`), whose independent HMAC rejection is at
  `crates/shared/wyrd-auth-verify/src/lib.rs:514-522`, so the composed login
  remains fail-closed.
- **Consequence:** the current runtime path is safe, but the helper's own
  documented contract is false. A maintainer can legitimately reuse the
  public helper believing it supplies the asymmetric-policy check, and the
  mandatory documentation no longer describes the operation at the level
  required to change or call it safely.
- **Testable correction:** keep the reuse cleanup and implementation unchanged;
  revise only `verify_id_token_algorithm`'s summary, explanation, and
  `# Errors` text to state that it checks membership in the provider-advertised
  set and that `ExternalVerifier::verify_external_against` performs the
  subsequent symmetric-algorithm rejection. Static source inspection plus
  `mise run fmt`, `mise run lints`, and `git diff --check` closes the rule; no
  new helper, abstraction, dependency, or runtime test is warranted.

## Verification notes and limits

Freshly executed on the immutable candidate, all exit 0:

1. `mise run check:from-pools-allowlist`
2. `mise run check:tenant-isolation`
3. `mise run codegen:check`
4. `git diff --check 3fc085acf5b3a710d5dc80892bd2e664b3db6174..d861845f3f5d89aca413857dcfb8c1bbfaee349d`
5. Candidate identity recheck against `d861845f3f5d89aca413857dcfb8c1bbfaee349d`

The R2 final evidence records successful `fmt`, `lints`, `codegen:check`,
`docs:check`, cumulative diff check, and `test:wyrd` (2213 passed), in addition
to the earlier focused identity, SQL, principals, boundary, and journey lanes.
This bounded review did not rerun the long runtime lanes; it inspected their
source coverage and the two lead-directed test repairs. The documentation
finding is a source-contract failure that those green runtime lanes do not
detect.

## Overall result

**FAIL**

The cumulative implementation satisfies the audited ownership, tenancy,
security behavior, audit, contract, generated-artifact, journey, fixture, and
test-integrity rules. It cannot receive a repository-standards pass until
STD-R3-001 corrects the materially modified helper's false rustdoc, which is an
explicit hard blocker under the repository rules.
