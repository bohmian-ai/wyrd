# TASK-001 r2 Wave 2 findings validation

## Immutable subject

- Base: `a5c8041a348a66bfb56fbac492383e8c688b0590`
- Candidate: `bb4895d8e630ee8fb2ba075d6c3eeaa348e49414`
- Candidate tree: `4c2e2b73f4dac882f02dd08290bb6ec102bce075`
- Approved specification: `changes/active/oidc-production-readiness/spec.md`, revision 4
- Original task: `changes/active/oidc-production-readiness/tasks/TASK-001-tenant-connections.md`
- Remediation task: `changes/active/oidc-production-readiness/review/TASK-001-r1/TASK-001-R1-production-readiness-gaps.md`

The complete cumulative range, prior validated ledger, all five r2 Wave 1
reports, applicable authorities, and current source were inspected. `HEAD`
remained the supplied candidate throughout validation.

## Wave 1 proposal dispositions

| Wave 1 proposal | Disposition | Stable finding | Validation |
|---|---|---|---|
| `task-review.md:TASK-REV-R2-001` | **REJECTED** | — | `mise run check:from-pools-allowlist` exits 0. The leading shell `!` turns ripgrep's missing `python/` status into success. The diagnostic is stale check noise, not a red gate or implementation defect. |
| `standards-review.md:STD-R2-001` | **CONFIRMED** | `FIND-TASK-001-14` | `PgIssuerResolver` was materially changed and still owns `Arc<PgPool>` and calls `TenantConn::acquire` directly; its production and test constructor sites are live. |
| `standards-review.md:STD-R2-002` | **REVISED** | `FIND-TASK-001-15` | The public handler accepts `Json<Value>` while advertising `ConnectionInput`. Closure must retain authorization before semantic body interpretation and the stable unsupported-auth refusal; a generic typed Axum extractor alone would break those requirements. |
| `standards-review.md:STD-R2-003` | **REVISED** | `FIND-TASK-001-8` | This is incomplete closure of the prior broad rustdoc finding, not a new finding. Materially changed resolver fields, variants, and secret-decoding helpers remain undocumented. |
| `standards-review.md:STD-R2-004` | **CONFIRMED** | `FIND-TASK-001-16` | The new proxy regression test has two ordinary function-local `use` statements; neither repository exception applies. |
| `standards-review.md:STD-R2-005` | **REVISED** | `FIND-TASK-001-17` | Both active and retained sealing-key file inputs participate in the task's rotation procedure and bypass the existing owner-only, regular-file, bounded loader. Expanding this task to the unrelated signing-key loader is rejected. |
| `domain-review-security.md:SEC-R2-001` | **REVISED** | `FIND-TASK-001-1` | The prior callback finding remains open: `code_is_valid || error_is_valid` accepts the ambiguous mixed success/error response that the remediation required to fail closed. |
| `domain-review-security.md:SEC-R2-002` | **REVISED** | `FIND-TASK-001-18` | Discovery endpoints can be public cleartext URLs. The minimum shared correction belongs in `ScreenedHttp` and must preserve the existing development/local-provider path rather than add another policy knob. |
| `domain-review-security.md:SEC-R2-003` | **REVISED** | `FIND-TASK-001-19` | Discovery, JWKS, probe, and real token responses buffer decoded provider data without a ceiling. Use reqwest's installed streaming API at the shared OIDC owner; do not add a dependency or per-caller limits. |
| `domain-review-tenancy-data.md:TD-R2-001` | **REVISED** | `FIND-TASK-001-5` | Runtime binding is fixed, but the migration fabricates that binding for provenance-free legacy families. This is incomplete closure of the prior exact-session-binding finding. |
| `domain-review-secrets.md:SECRET-R2-001` | **REVISED** | `FIND-TASK-001-12` | The runbook and engine are corrected, but the mandated proof is not: replica A remains a live K1 writer through the purported post-roll pass and is used again afterward. |

The retained corrections are bounded implementation or proof changes under the
approved behavior. None requires a new product, public API, architecture,
security policy, compatibility path, concurrency semantic, resource owner, or
persistent-data decision.

## Prior-finding closure

| Prior finding | Result | Evidence |
|---|---|---|
| `FIND-TASK-001-1` | **OPEN, REVISED BELOW** | Origin/path/state checks exist, but mixed `code` plus `error` still qualifies. |
| `FIND-TASK-001-2` | **CLOSED** | Only a client-error `invalid_grant` qualifies the deliberately invalid-code probe. |
| `FIND-TASK-001-3` | **CLOSED** | Runtime login calls `HumanConnections::require_callback`; request headers no longer construct the redirect URI. |
| `FIND-TASK-001-4` | **CLOSED** | The shared reqwest builder calls `.no_proxy()` and the regression test exercises the ambient-proxy path. |
| `FIND-TASK-001-5` | **OPEN, REVISED BELOW** | New sessions carry and transactionally recheck exact connection provenance; the migration assigns unsupported provenance to legacy sessions. |
| `FIND-TASK-001-6` | **CLOSED** | `HumanConnections` and `PgLoginStateStore` now own `WyrdPostgres` and acquire through it. `FIND-TASK-001-14` is a distinct materially changed resolver. |
| `FIND-TASK-001-7` | **CLOSED** | Candidate testing performs a real second decision and commits its event with the stamp. |
| `FIND-TASK-001-8` | **OPEN, REVISED BELOW** | The originally cited sites were documented, but other materially changed resolver items still violate the same repository-wide obligation. |
| `FIND-TASK-001-9` | **CLOSED** | The prior cited declarations now use top-level imports and bare type names. |
| `FIND-TASK-001-10` | **CLOSED** | The identity list and run commands no longer force `--all-features`. |
| `FIND-TASK-001-11` | **CLOSED** | Keyless boot inventories all existing provider-secret stores and refuses stored ciphertext before readiness. |
| `FIND-TASK-001-12` | **OPEN, REVISED BELOW** | The procedure is documented correctly, but its journey never removes the K1 writer before the final verification pass. |
| `FIND-TASK-001-13` | **CLOSED** | `Public` rejects every present secret; secret methods reject missing or empty values. |

## Final deduplicated finding ledger

### FIND-TASK-001-1 — REVISED — INCORRECT: mixed success/error callback responses still qualify

- **Wave 1 source:** `domain-review-security.md:SEC-R2-001`.
- **Violated obligation:** R1 requires only an exact, unambiguous standard authorization response to qualify the callback and every ambiguous response to leave the revision untested.
- **Location:** `crates/wyrd/wyrd-auth/src/connections.rs:836-865`, with the proof gap at `:1048-1101`.
- **Evidence:** `callback_redirect_qualifies` independently validates `code` and `error`, then joins them with `||`. A redirect containing both a nonempty `code` and recognized `error` therefore passes. Its sole production caller is `probe_callback`, whose success feeds `probe_candidate` and then the stamp transaction.
- **Observable consequence:** A nonconformant provider response can stamp and activate a candidate even though it did not return one valid OIDC authorization response arm.
- **Decision-complete correction:** Keep the existing helper and require exactly one arm: one nonempty `code` and no `error`, or one recognized `error` and no `code`. Reject duplicates and all mixed responses; change no provider client or callback contract.
- **Focused closure proof:** Extend the existing table test with mixed `code`/`error` and duplicate-opposite parameters and prove every case is false while each single valid arm remains true.

### FIND-TASK-001-5 — REVISED — INCORRECT: migration assigns provenance-free legacy sessions to the current provider

- **Wave 1 source:** `domain-review-tenancy-data.md:TD-R2-001`.
- **Violated obligation:** REQ-016 and R1 require every human refresh family to remain bound to the exact connection that authenticated it; replacement must stop an old-provider family.
- **Location:** `crates/wyrd/wyrd-sql/migrations/20260925000000_auth_human_connections.sql:177-214`; the legacy refresh schema at `20260601000001_auth.sql:177-205` has no issuer or connection provenance.
- **Evidence:** The migration updates every unrevoked user refresh row to whichever connection it just made Active. Before upgrade, the live delete route removes Human issuer A without revoking refresh rows, after which issuer B can be configured. The migration then labels A's family as B. The only runtime refresh route calls `RefreshTokens::execute`, which trusts that fabricated binding and copies it to successors.
- **Observable consequence:** An old-provider session can renew under a replacement provider it never authenticated against, retaining the old principal's authority after upgrade.
- **Decision-complete correction:** Delete the inference update. Leave all legacy user refresh rows unbound so the existing fail-closed `RefreshTokens::execute` path requires re-login; do not infer from tenant, current issuer, email, or user identity. Preserve post-migration login binding and machine-row behavior.
- **Focused closure proof:** Extend `pg_tests::human_connection_upgrade_preflight` with a live legacy user refresh family, replace A with B before migration, migrate, and prove the row remains unbound and cannot rotate. Then prove a fresh B login creates a bound, renewable family.

### FIND-TASK-001-8 — REVISED — VIOLATION: materially changed resolver items still lack mandatory rustdoc

- **Wave 1 source:** `standards-review.md:STD-R2-003`.
- **Violated obligation:** `AGENTS.md` section 16 and `architecture/agent-rules.md` require substantive rustdoc for every new or materially modified Rust item, including private fields, variants, helpers, and `# Errors` on fallible functions.
- **Location:** `crates/wyrd/wyrd-auth/src/pg_resolvers.rs:65-66,246-264,357-370`.
- **Evidence:** The candidate changed both resolver field types, exposed and changed `IssuerDecodeError`, and rewrote the secret-opening path. The fields and error variants have no item docs; `open_secret` lacks `# Errors`; `decode_secret` has no rustdoc. These are live in issuer, human-connection, and platform secret decoding.
- **Observable consequence:** A repository-declared hard documentation gate remains unsatisfied at a security-sensitive owner.
- **Decision-complete correction:** Document only the materially changed fields, variants, and helpers with their ownership and failure invariants, including `# Errors` for both fallible helpers. Add no lint allowance or unrelated documentation sweep.
- **Focused closure proof:** Format/lints pass and direct diff inspection finds no undocumented item in the materially changed resolver surface.

### FIND-TASK-001-12 — REVISED — INCORRECT: the rotation journey never establishes a post-K1-writer verification pass

- **Wave 1 source:** `domain-review-secrets.md:SECRET-R2-001`.
- **Violated obligation:** REQ-005 and R1 require a late K1 write, all writers moving to K2, a final verification pass, and K2-only serving before K1 retirement.
- **Location:** `crates/wyrd/wyrd-server/tests/identity_e2e.rs:2146-2228`.
- **Evidence:** Replica A writes the late K1 candidate, remains live while replica C performs the claimed post-roll pass, and is used again after the K2-only check. `start_replica` explicitly leaves the source replica running. A can therefore create another K1 ciphertext immediately after C reports zero.
- **Observable consequence:** The journey can pass without proving the ordering that makes `remaining = 0` safe retirement evidence.
- **Decision-complete correction:** After the deliberate late K1 write, shut down every K1-write-capable replica before starting the final K2-write/K1-retained pass. Start that pass from an already K2-writing replica, then perform K2-only login and route the rest of the journey through K2 writers. Preserve the CAS engine and runbook.
- **Focused closure proof:** The existing rotation journey shows the late value under K1, removes A from service, runs the final pass, proves the value is current under K2, and serves login from a K2-only replica.

### FIND-TASK-001-14 — CONFIRMED — VIOLATION: the materially changed issuer resolver still owns a raw pool

- **Wave 1 source:** `standards-review.md:STD-R2-001`.
- **Violated obligation:** `architecture/agent-rules.md` permits only `TenantConn` and `OperatorPool` in library fields/signatures and requires production tenant acquisition through `WyrdPostgres`.
- **Location:** `crates/wyrd/wyrd-auth/src/pg_resolvers.rs:57-95,126-160` and constructor sites in server boot, server tests, auth tests, and `wyrd-testing`.
- **Evidence:** `PgIssuerResolver` was materially changed from `SecretKey` to `SealingKeyring`, but still stores `Arc<PgPool>`, accepts it in `new`, and calls `TenantConn::acquire` directly. All constructor sites are reachable; production boot already has `postgres.wyrd()` and the adjacent remediated owner uses it.
- **Observable consequence:** This live resolver can still be assembled from an arbitrary pool outside the repository's role-separated acquisition owner.
- **Decision-complete correction:** Make `PgIssuerResolver` own the existing cloned `WyrdPostgres`, acquire with `WyrdPostgres::tenant_conn`, and update its constructor sites. Do not convert the untouched workload resolver, add a trait, wrapper, feature, or allowlist entry.
- **Focused closure proof:** Auth/server compile and resolver tests pass; static inspection finds no `PgPool` field/signature or direct `TenantConn::acquire` in `PgIssuerResolver`.

### FIND-TASK-001-15 — REVISED — VIOLATION: the candidate PUT handler exposes a dynamic JSON boundary

- **Wave 1 source:** `standards-review.md:STD-R2-002`.
- **Violated obligation:** `AGENTS.md` section 9 requires public request bodies to be typed structs; the packet also requires authorization before semantic body interpretation and a stable `UNSUPPORTED_CLIENT_AUTH` refusal.
- **Location:** `crates/wyrd/wyrd-server/src/components/admin/identity.rs:139-192`; contract decoder at `crates/wyrd-spec/src/auth/human_connection.rs:202-220`.
- **Evidence:** OpenAPI advertises `ConnectionInput`, but the actual handler accepts `Json<Value>` and interprets it later. `ConnectionInput::from_json` has one production caller, so the mismatch is reachable on every candidate PUT.
- **Observable consequence:** The compiler cannot keep the HTTP body shape aligned with the advertised contract, and future field changes can drift between route extraction and schema.
- **Decision-complete correction:** Use Axum's native bounded raw-body extraction so permission is still decided before fields are interpreted, then move byte-to-`ConnectionInput` decoding into the existing contract owner. That decoder must preserve the specific `PrivateKeyJwt` error before ordinary validation. Remove `Json<Value>` from the handler; add no generic extractor framework or second public DTO.
- **Focused closure proof:** HTTP tests prove an authorized valid body stages, `PrivateKeyJwt` returns the stable code, malformed input returns validation, and an unauthorized caller is refused/audited before semantic input errors are exposed; served OpenAPI remains `ConnectionInput`.

### FIND-TASK-001-16 — CONFIRMED — VIOLATION: the proxy regression test hides module dependencies inside the function

- **Wave 1 source:** `standards-review.md:STD-R2-004`.
- **Violated obligation:** `architecture/agent-rules.md` requires ordinary imports at module top; neither the `use super::*` nor generic-trait exception applies.
- **Location:** `crates/shared/wyrd-auth-oidc/src/screening.rs:310-312`.
- **Evidence:** The candidate-added proxy test imports `wiremock::matchers` and `wiremock` inside its function. The surrounding test module already owns its dependency block.
- **Observable consequence:** The module dependency manifest is incomplete and the candidate violates an explicit repository shape rule.
- **Decision-complete correction:** Move exactly those imports into the existing `#[cfg(test)] mod tests` import block; change no test behavior or helper shape.
- **Focused closure proof:** The proxy test plus format/lints pass and no ordinary function-local import remains.

### FIND-TASK-001-17 — REVISED — VIOLATION: sealing-key files bypass the existing restrictive secret-file loader

- **Wave 1 source:** `standards-review.md:STD-R2-005`.
- **Violated obligation:** Security posture requires file-mounted secrets to have restrictive permissions and atomic replacement; REQ-005 makes the active and retained sealing files part of the tested rotation procedure.
- **Location:** `crates/wyrd/wyrd-server/src/config.rs:3408-3472`; runbook at `docs/src/content/docs/self-hosting/authentication.svx:61-84`.
- **Evidence:** Both sealing file loaders use `std::fs::read_to_string` directly and accept permissive, non-regular, or oversized files. The repository already re-exports `wyrd_gateway::read_secret_file`, which opens once, validates the opened handle as regular and owner-only, and enforces a byte cap. `apply_env_overrides` is the sole production caller of both loaders.
- **Observable consequence:** Boot accepts group/world-readable decryption-capable key material during normal operation and rotation.
- **Decision-complete correction:** Reuse `read_secret_file` for both `WYRD_SEALING_KEY_FILE` and `WYRD_SEALING_RETAINED_KEYS_FILE`, map its redacted static refusal into the existing config error path, and document owner-only mode plus atomic file replacement. Do not create another loader or broaden this remediation to the signing-key path.
- **Focused closure proof:** Config tests accept an owner-only regular sealing file and reject permissive, non-regular, and oversized active and retained files without quoting key material; docs check passes.

### FIND-TASK-001-18 — REVISED — VIOLATION: screened production provider requests permit discovered cleartext endpoints

- **Wave 1 source:** `domain-review-security.md:SEC-R2-002`.
- **Violated obligation:** INV-004 and the security posture require mandatory production TLS and adapter-approved schemes for every tenant-directed provider fetch.
- **Location:** `crates/shared/wyrd-auth-oidc/src/provider.rs:44-78`; `crates/shared/wyrd-auth-oidc/src/screening.rs:79-131`; secret-bearing callers at `wyrd-auth/src/connections.rs:756-792` and `wyrd-auth/src/callback.rs:311-368`.
- **Evidence:** The configured issuer is scheme-validated, but discovery parses authorization, token, and JWKS URLs as unrestricted `Url`. Every downstream caller uses `ScreenedHttp::client_for`, which checks addresses but not scheme. An HTTPS issuer can therefore direct production token exchange to public `http://` and receive the client secret and authorization code in cleartext.
- **Observable consequence:** A compromised or misconfigured provider can expose tenant client credentials and login codes on a cleartext network hop.
- **Decision-complete correction:** Enforce the scheme once in `ScreenedHttp::client_for`: `BlockInternal` accepts only HTTPS; `AllowInternal` retains the existing HTTP local-provider/test path; every other scheme is refused. This reuses deployment policy and protects discovery, JWKS, probes, and real exchanges without a new knob or caller patches.
- **Focused closure proof:** A production-policy screened request refuses an HTTP endpoint before sending anything, including an HTTP endpoint learned from HTTPS discovery; existing local-provider tests continue under `AllowInternal`.

### FIND-TASK-001-19 — REVISED — VIOLATION: provider responses have no decoded-body ceiling

- **Wave 1 source:** `domain-review-security.md:SEC-R2-003`.
- **Violated obligation:** The security posture requires response, body-size, decompression, and total-operation bounds for tenant-supplied URLs; TASK-001 requires screened provider network behavior.
- **Location:** discovery at `crates/shared/wyrd-auth-oidc/src/provider.rs:115-156`; JWKS at `crates/shared/wyrd-auth-oidc/src/jwks.rs:141-180`; candidate token probe at `crates/wyrd/wyrd-auth/src/connections.rs:756-792`; real token response at `crates/wyrd/wyrd-auth/src/callback.rs:311-368`.
- **Evidence:** These paths call reqwest `json()` or `bytes()`, which accumulate the decoded body without a ceiling. They are reached by candidate testing, login/callback, platform login, and JWKS refresh. The client timeout does not bound memory, and gzip/brotli are enabled workspace-wide.
- **Observable consequence:** A tenant-controlled provider can stream a chunked or decompression-amplified response until a shared serving replica exhausts memory.
- **Decision-complete correction:** Add one fixed 1 MiB decoded-response limit in the shared OIDC crate and use reqwest's existing chunked response API to stop once the accumulated decoded bytes exceed it. Route discovery, JWKS, candidate probe, and real token JSON through that one mechanism, preserving current timeout/cancellation and error mapping. Add no configuration knob, dependency, or per-caller reader.
- **Focused closure proof:** Shared focused tests reject oversized declared, chunked, and compressed decoded bodies at the limit; a candidate receiving one remains untested, while ordinary discovery, JWKS, and token responses still pass.

## Caller tracing and verification limits

- Complete correction caller sets were traced: `probe_callback` to candidate stamping; every `PgIssuerResolver::new` site; the sole production `ConnectionInput::from_json` caller; all screened discovery/JWKS/probe/token-exchange consumers; both sealing file loaders through `apply_env_overrides`; the migration-to-refresh route; and the rotation journey's replica lifecycle.
- `mise run check:from-pools-allowlist` was rerun at the candidate and exited 0 while printing `rg: python/: No such file or directory`. The diagnostic is a check-maintenance gap only; it is not retained as a finding because the gate succeeds and the missing root cannot hide Rust matches under the still-scanned `crates/` root.
- Long Postgres/provider journeys and broad Cargo/codegen/docs lanes were not rerun in Wave 2. Their committed source and Wave 1 evidence were inspected. Focused execution evidence in Wave 1 covers callback/client-auth probes, secret validation, keyless boot, keyring rotation, tenant-isolation, and boundary checks, but does not cover the retained gaps above.
- No sub-reviewer was awaited by this Wave 2 reviewer; there is no missing-sub-reviewer verification limit.

## Recommendation

**FIX_REQUIRED** — retain `FIND-TASK-001-1`, `FIND-TASK-001-5`,
`FIND-TASK-001-8`, `FIND-TASK-001-12`, and add
`FIND-TASK-001-14` through `FIND-TASK-001-19`.
