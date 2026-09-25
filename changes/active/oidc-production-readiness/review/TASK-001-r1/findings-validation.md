# TASK-001 Wave 2 findings validation

## Immutable subject

- Base: `a5c8041a348a66bfb56fbac492383e8c688b0590`
- Candidate: `e126cdca7d4bf5bc467279df05cc3e199eb7fdf2`
- Candidate tree: `2f18add99a7c3575580c224cdf26eb5afe211335`
- Approved specification: `changes/active/oidc-production-readiness/spec.md`, revision 4
- Original task: `changes/active/oidc-production-readiness/tasks/TASK-001-tenant-connections.md`

The candidate commit and tree matched the supplied immutable subject before and
after validation. The complete base-to-candidate diff, all five Wave 1 reports,
the applicable authorities, and the current candidate source were inspected.

## Wave 1 proposal dispositions

Source-local `SEC-*` identifiers collide between the security and secrets
reports, so every disposition below names its report path.

| Wave 1 proposal | Disposition | Stable finding | Validation |
|---|---|---|---|
| `task-review.md:TASK-REV-001` | **REVISED** | `FIND-TASK-001-1`, `FIND-TASK-001-2` | The authorization-endpoint and token-endpoint probes are independently reachable failures and need separate closure. The token half duplicates `domain-review-security.md:SEC-002`. |
| `task-review.md:TASK-REV-002` | **CONFIRMED** | `FIND-TASK-001-3` | The sole runtime helper derives the callback from request headers although the connection owner already holds the deployment callback. |
| `standards-review.md:REPO-001` | **CONFIRMED** | `FIND-TASK-001-6` | Deduplicated with `domain-review-tenancy-data.md:TD-002`. |
| `standards-review.md:REPO-002` | **REVISED** | `FIND-TASK-001-7` | Removing the second row would violate the packet's same-transaction mutation-audit rule. The minimum correction is a real second permission evaluation after provider IO, not a synthetic event. |
| `standards-review.md:REPO-003` | **CONFIRMED** | `FIND-TASK-001-8` | Representative missing rustdoc was verified on added fields, constants, helpers, and test items; the repository declares this a hard blocker. |
| `standards-review.md:REPO-004` | **CONFIRMED** | `FIND-TASK-001-9` | The cited qualified paths were added by this candidate and violate the explicit bare-signature rule. |
| `standards-review.md:REPO-005` | **CONFIRMED** | `FIND-TASK-001-10` | The task packet gives the exact no-`--all-features` command. The candidate retained one old occurrence and added two more while materially rewriting the lane. |
| `domain-review-security.md:SEC-001` | **CONFIRMED** | `FIND-TASK-001-4` | Reqwest 0.12.28 enables system proxies by default; every screened provider caller therefore shares the pinning bypass. |
| `domain-review-security.md:SEC-002` | **REVISED** | `FIND-TASK-001-2` | Confirmed and deduplicated with the token half of `TASK-REV-001`; only an OAuth `invalid_grant` response proves the deliberately invalid code reached grant validation after client authentication. |
| `domain-review-tenancy-data.md:TD-001` | **CONFIRMED** | `FIND-TASK-001-5` | Refresh rotation never reads a connection, and callback issuance reuses an unlocked issuer snapshot after provider IO. REQ-016 and the packet expressly require immediate old-connection cutoff. |
| `domain-review-tenancy-data.md:TD-002` | **CONFIRMED** | `FIND-TASK-001-6` | Confirmed and deduplicated with `REPO-001`. |
| `domain-review-secrets.md:SEC-001` | **REVISED** | `FIND-TASK-001-11` | The failure is confirmed, but there is no separate existing inventory owner. Reuse the existing cross-store query slots directly from boot rather than introducing one. |
| `domain-review-secrets.md:SEC-002` | **REVISED** | `FIND-TASK-001-12` | The rolling-writer race is confirmed. Requiring duplicate race journeys for all three stores is unnecessary; one late-write journey closes the temporal defect because the same rewrap owner processes every store. |
| `domain-review-secrets.md:SEC-003` | **CONFIRMED** | `FIND-TASK-001-13` | `Some(SecretBearer(""))` is observably present but is treated as absent and silently accepted for `Public`. |

No Wave 1 proposal was rejected. The retained corrections are bounded by the
approved task and require no new product, public-API, architecture, security,
compatibility, concurrency-semantics, resource-ownership, or persistent-data
decision. The recommended outcome is **FIX_REQUIRED**.

## Final deduplicated finding ledger

### FIND-TASK-001-1 — REVISED — INCORRECT: callback qualification accepts ambiguous authorization responses

- **Wave 1 sources:** `task-review.md:TASK-REV-001` (authorization-endpoint half).
- **Violated obligation:** The packet-local contract requires candidate testing to validate the configured callback before stamping the exact revision; Scenario 2 requires unsafe or rejected configuration to fail closed.
- **Location:** `crates/wyrd/wyrd-auth/src/connections.rs:541-603`.
- **Evidence:** `probe_callback` rejects only 4xx and 5xx. It accepts a `200` body that ignored the request and any 3xx, including a redirect whose `Location` is unrelated to the configured callback. Its only caller is `probe`, which stamps the candidate after this return; no later path validates the provider registration.
- **Observable consequence:** A candidate whose callback is not registered can be stamped and activated, retiring a working provider into a login configuration that cannot return to Wyrd.
- **Decision-complete correction:** Keep the existing screened, redirect-disabled client. Send a non-interactive OIDC authorization request (`prompt=none`) with the generated state and configured callback, and accept only a redirect whose `Location` has the exact configured callback origin/path, echoes the state, and carries either a code or a standard OIDC authorization error. Refuse success pages, missing/foreign locations, state mismatch, non-OIDC bodies, and every other response without stamping. Do not add a second HTTP client or compatibility mode.
- **Focused closure proof:** A focused provider probe returns (a) a valid callback redirect with matching state, (b) `200`, (c) a redirect to another origin/path, and (d) a malformed/non-OIDC response. Only (a) may stamp the candidate; every refusal must leave activation at `CONNECTION_NOT_TESTED`.

### FIND-TASK-001-2 — REVISED — INCORRECT: client-auth qualification accepts responses that never authenticated the client

- **Wave 1 sources:** `task-review.md:TASK-REV-001` (token-endpoint half); `domain-review-security.md:SEC-002`.
- **Violated obligation:** REQ-004 and the packet require candidate testing to validate the configured client-authentication method before activation.
- **Location:** `crates/wyrd/wyrd-auth/src/connections.rs:606-659`.
- **Evidence:** `probe_client_auth` refuses only HTTP 401, `invalid_client`, and `unauthorized_client`; it accepts `200`, malformed bodies, and unrelated OAuth errors. Its only caller is `probe`, so any accepted response leads directly to a test stamp.
- **Observable consequence:** Wyrd can activate a candidate whose secret or selected authentication method was never accepted, causing tenant login lockout.
- **Decision-complete correction:** Parse the token response fail-closed and accept exactly an OAuth error response with code `invalid_grant` for the deliberately invalid authorization code. Continue mapping `invalid_client`/`unauthorized_client` to client-auth refusal; reject success, undecodable bodies, every other OAuth error, and every other status. Reuse the existing screened client and response JSON parsing.
- **Focused closure proof:** One focused probe test proves `invalid_grant` stamps while `invalid_client`, `invalid_request`, malformed/empty bodies, and `200` do not; retain the existing real-provider wrong-secret journey.

### FIND-TASK-001-3 — CONFIRMED — INCORRECT: runtime login does not use the deployment-controlled callback

- **Wave 1 sources:** `task-review.md:TASK-REV-002`.
- **Violated obligation:** REQ-004 and the packet fix the callback to configured public origin plus `/auth/callback`; the shown and tested connection must be the one real login uses.
- **Location:** `crates/wyrd/wyrd-server/src/auth/login.rs:84-140`.
- **Evidence:** `try_initiate_login`, the sole caller of `callback_redirect_uri`, derives the redirect from `Host` and `X-Forwarded-Proto`. `HumanConnections`, already constructed there, exposes the deployment-derived callback. The derived value is then persisted in login state and replayed at token exchange.
- **Observable consequence:** Request headers can make runtime login use a redirect URI different from the one administration displayed and tested.
- **Decision-complete correction:** Delete the header-derived callback helper from this flow and pass `HumanConnections::callback_url()` to `prepare_login`, refusing login when no deployment callback exists. Preserve host-based tenant entry resolution; it is a separate responsibility.
- **Focused closure proof:** Initiate login with a tenant-resolving Host and forwarded scheme that differ from configured public origin and assert the authorization request still contains exactly the configured callback.

### FIND-TASK-001-4 — CONFIRMED — VIOLATION: ambient proxies bypass screened-address pinning

- **Wave 1 sources:** `domain-review-security.md:SEC-001`.
- **Violated obligation:** INV-004, agent rules 29-30, and the security posture require provider IO to connect to the exact screened address without re-resolution.
- **Location:** `crates/shared/wyrd-auth-oidc/src/screening.rs:97-124`.
- **Evidence:** Reqwest 0.12.28's builder starts with `auto_sys_proxy = true`; `ScreenedHttp::client_for` pins DNS with `resolve_to_addrs` but never calls `no_proxy()`. The complete caller set—discovery, JWKS refresh, login discovery, callback token exchange, tenant candidate probes, and legacy issuer discovery—uses this shared builder, so the bypass is reachable on every provider path.
- **Observable consequence:** An ambient HTTP(S) proxy can receive the original hostname and select a destination Wyrd never screened, defeating the SSRF/DNS-rebinding boundary.
- **Decision-complete correction:** Add reqwest's native `.no_proxy()` to the one shared `ScreenedHttp` builder before applying pinned addresses. Do not patch individual callers or add proxy configuration.
- **Focused closure proof:** With an ambient proxy configured, a screened request reaches the pinned local endpoint and the proxy observes no request; retain blocked-range and rebinding checks.

### FIND-TASK-001-5 — CONFIRMED — MISSING: session issuance and renewal are not bound to the active connection revision

- **Wave 1 sources:** `domain-review-tenancy-data.md:TD-001`.
- **Violated obligation:** REQ-016 and the packet require replacement, deactivation, and removal to block old-connection login and renewal immediately, require renewal to read durable Active state, and retain tombstone identity while session references exist.
- **Location:** `crates/wyrd/wyrd-auth/src/callback.rs:76-117,172-217`; `crates/wyrd/wyrd-auth/src/refresh.rs:78-143`; `crates/wyrd/wyrd-auth/src/login.rs:120-135`; refresh-token query/schema paths under `crates/wyrd/wyrd-sql/src/queries/auth/refresh_tokens.rs`.
- **Evidence:** Callback reads Active state before provider IO, then issues inside a later tenant transaction without rechecking or locking it. Refresh consumes and rotates by token family, user status, and grants only. Login state and refresh rows carry issuer/family data but no exact connection id/revision. The one refresh route at `components/auth/routes.rs:216-247` opens a tenant transaction and calls this unchanged path.
- **Observable consequence:** A callback already past its first read can establish a session after lifecycle cutoff, and an old-provider refresh family can mint successors after replacement, deactivation, or removal.
- **Decision-complete correction:** Bind login state and every human refresh family to the exact human-connection id and revision selected at login. In the same tenant transaction that issues the initial session or a refresh successor, take the existing tenant connection slot lock and require that exact id/revision still be Active; otherwise refuse without inserting a token. Persist the binding on the initial refresh row and copy it on rotation. Preserve existing refresh replay containment, grant re-resolution, audit, tombstones, and five-minute access-token semantics.
- **Focused closure proof:** A real-server two-replica journey obtains an old-connection refresh token, commits replacement, deactivation, and removal on the other replica, and proves each old family is refused with no successor. A controlled in-flight callback paused after provider IO must also fail after the lifecycle mutation commits.

### FIND-TASK-001-6 — CONFIRMED — VIOLATION: the connection owner propagates a raw `PgPool`

- **Wave 1 sources:** `standards-review.md:REPO-001`; `domain-review-tenancy-data.md:TD-002`.
- **Violated obligation:** `architecture/agent-rules.md:6` permits tenant work only through `TenantConn` and bans raw pool fields/signatures in library code.
- **Location:** `crates/wyrd/wyrd-auth/src/connections.rs:75-79,106-125,469-473`; `crates/wyrd/wyrd-server/src/components/auth/state.rs:56-68`; callers in server login, callback, and admin identity adapters.
- **Evidence:** `HumanConnections` owns a cloned `PgPool`, `ServerAuth::human_connections` accepts `&PgPool`, and the owner calls `TenantConn::acquire` directly. All real callers already have `AppState.postgres`, whose `tenant_conn` is the sanctioned acquisition path.
- **Observable consequence:** The security-sensitive owner can be constructed from an arbitrary application pool, bypassing the repository's role-separated capability boundary by construction.
- **Decision-complete correction:** Replace the raw pool in `HumanConnections` with the existing cloned `WyrdPostgres` runtime owner, make `ServerAuth::human_connections` accept that sanctioned owner instead of `&PgPool`, and acquire every transaction through `WyrdPostgres::tenant_conn`. This preserves the cohesive lifecycle workflow, including its two transactions around provider IO, without a new trait, factory, allowlist entry, or database abstraction.
- **Focused closure proof:** Auth/server compile and focused lifecycle tests pass, and the changed connection construction/call path contains no `PgPool` field or signature.

### FIND-TASK-001-7 — REVISED — VIOLATION: the post-provider stamp audit is not backed by a permission evaluation

- **Wave 1 sources:** `standards-review.md:REPO-002`.
- **Violated obligation:** Agent rules 12-14 require one row per permission evaluation; the packet additionally requires authorization/audit before provider IO and a mutation decision in the stamp transaction.
- **Location:** `crates/wyrd/wyrd-server/src/components/admin/identity.rs:190-250`; `crates/wyrd/wyrd-auth/src/connections.rs:287-318`.
- **Evidence:** The handler evaluates once and records that decision before IO, then constructs a second Allowed event with `audit_event` without evaluating permission. `HumanConnections::test_candidate` appends this synthetic row with the stamp.
- **Observable consequence:** Audit history presents one permission evaluation as two authorization decisions and can stamp from a fabricated Allowed event.
- **Decision-complete correction:** Preserve the first evaluated-and-recorded decision before provider IO. After provider checks succeed, call the existing `decide` path a second time for the stamp operation and pass that evaluated event into the locked stamp transaction. Do not remove the same-transaction stamp decision or invent a mechanics audit event.
- **Focused closure proof:** One successful test produces exactly two allowed rows backed by two evaluations—pre-network test and transactional stamp—while a denied second evaluation produces no stamp; audit append failure remains fail-closed.

### FIND-TASK-001-8 — CONFIRMED — VIOLATION: added Rust items lack mandatory rustdoc

- **Wave 1 sources:** `standards-review.md:REPO-003`.
- **Violated obligation:** `AGENTS.md` section 16 and `architecture/agent-rules.md:35` require meaningful rustdoc for every new/materially modified Rust item, including private fields, helpers, constants, and tests.
- **Location:** Confirmed examples include `connections.rs:75-104` (private fields), `wyrd-sql/src/queries/auth/human_connections.rs:35,390,403`, and `wyrd-server/tests/identity_e2e.rs:780,796-798,934-937`; these are candidate-added items.
- **Evidence:** The fields/helpers/constants have no item rustdoc; adjacent grouped comments do not document subsequent items. Ordinary public-doc linting cannot cover these private omissions.
- **Observable consequence:** A hard repository acceptance gate remains unsatisfied, and the ownership/security invariants carried by these items are undocumented at their declaration.
- **Decision-complete correction:** Add intent/invariant rustdoc to every missing item in the candidate-added or materially modified Rust surfaces, beginning with the confirmed locations above; add required `# Errors`, `# Panics`, and cancellation/partial-progress notes where applicable. Do not add lint allowances or placeholder restatements.
- **Focused closure proof:** Format/lints/docs pass and a diff review finds no added/materially modified undocumented Rust item.

### FIND-TASK-001-9 — CONFIRMED — VIOLATION: added signatures use qualified type paths

- **Wave 1 sources:** `standards-review.md:REPO-004`.
- **Violated obligation:** `architecture/agent-rules.md:9-10` requires top-of-module imports and bare types in fields/signatures.
- **Location:** `wyrd-spec/src/auth/human_connection.rs:209,292-294`; `wyrd-auth/src/connections.rs:82-83,663,801,849`; `wyrd-sql/tests/pg_migration.rs:165,202,229`; `wyrd-testing/src/server.rs:498,3896`.
- **Evidence:** These candidate-added declarations inline `serde_json::Value`, `crate::auth::IssuerTokenPolicy`, `std::fmt::*`, `reqwest::Client`, `wyrd_dev_fixtures::pg::UnmigratedDatabase`, or `url::Url` instead of using the module import block.
- **Observable consequence:** The changed modules split their dependency manifest between imports and declarations, violating an explicit repository convention.
- **Decision-complete correction:** Import each cited type at its module's existing top-level `use` block and use the bare name in the added fields/signatures; do not otherwise refactor the functions.
- **Focused closure proof:** Format/lints pass and the cited added declarations contain no qualified type path.

### FIND-TASK-001-10 — CONFIRMED — VIOLATION: the identity journey forces all features

- **Wave 1 sources:** `standards-review.md:REPO-005`.
- **Violated obligation:** `AGENTS.md` section 4 and testing-workflows reserve `--all-features` for lint/type-check lanes; the task packet gives the exact focused journey command without it.
- **Location:** `mise.toml:660,669,678`.
- **Evidence:** The candidate materially rewrites this lane, retains the old all-feature run, and adds all-feature list and filtered-run invocations. No task requirement needs the feature union.
- **Observable consequence:** Test listing and every identity journey pay a needless heavy recompilation cost in local and CI runs.
- **Decision-complete correction:** Remove `--all-features` from all three identity `nextest` commands and use the packet's default-feature command. Add no feature knob.
- **Focused closure proof:** Both filtered connection journeys and the unfiltered identity lane select and pass under the repository setup wrapper without `--all-features`.

### FIND-TASK-001-11 — REVISED — INCORRECT: boot permits stored provider secrets with no sealing key

- **Wave 1 sources:** `domain-review-secrets.md:SEC-001` (not the colliding security report ID).
- **Violated obligation:** REQ-005 requires a deployment sealing secret while provider secrets are stored; the shipped operator documentation promises fail-closed boot.
- **Location:** `crates/wyrd/wyrd-server/src/boot/mod.rs:1272,1338,1499-1513,2143-2174`; cross-store query slots used by `crates/wyrd/wyrd-auth/src/sealing.rs:61-103`.
- **Evidence:** `build_sealing_keyring` returns `None`, and `rewrap_sealed_secrets` immediately returns when the keyring is absent. No later boot validation inventories tenant human, workload issuer, or platform ciphertext.
- **Observable consequence:** The server reports ready although configured provider authentication is undecryptable and fails only on use.
- **Decision-complete correction:** At the existing boot rewrap boundary, when the keyring is absent, use the existing `sealed_tenant_secrets` table slots plus `platform_sealed_secret` to detect any ciphertext and return `ServerBootError::SealingKey`; preserve successful keyless startup when all stores are secret-free. Do not create a new inventory abstraction or duplicate SQL.
- **Focused closure proof:** A real-Postgres boot test proves secret-free keyless startup succeeds and a stored secret-bearing human connection makes keyless restart fail before readiness.

### FIND-TASK-001-12 — REVISED — INCORRECT: the documented zero rewrap report is not a safe retirement proof

- **Wave 1 sources:** `domain-review-secrets.md:SEC-002` (not the colliding security report ID).
- **Violated obligation:** REQ-005 and the packet require retaining the old key through rewrap and verification and retiring it only after no ciphertext references it.
- **Location:** `crates/wyrd/wyrd-auth/src/sealing.rs:1-14,61-111`; `crates/wyrd/wyrd-server/src/boot/mod.rs:1499-1513`; `docs/src/content/docs/self-hosting/authentication.svx:74-83`; `identity_e2e.rs:2063-2104`.
- **Evidence:** One new replica can report zero from a snapshot while an old replica still writes K1 afterward. The runbook authorizes retirement after that report, and the journey never performs a post-roll pass or starts K2-only after a late K1 write.
- **Observable consequence:** Following the documented successful procedure can strand a newly written provider secret when K1 is removed.
- **Decision-complete correction:** Preserve the idempotent CAS rewrap. Change the runbook and module contract so retirement requires a new verification pass only after every serving writer uses K2; only `remaining = 0` from that post-roll pass authorizes K1 removal. Do not add leases, coordination state, or another rewrap engine.
- **Focused closure proof:** Extend the two-replica rotation journey so an old K1 writer creates a secret after the first K2 scan, all writers then switch to K2, a final pass rewraps/verifies it, and a K2-only replica opens and uses it. One human-connection case is sufficient for this shared temporal rule.

### FIND-TASK-001-13 — CONFIRMED — INCORRECT: `Public` accepts an explicitly present empty secret

- **Wave 1 sources:** `domain-review-secrets.md:SEC-003`.
- **Violated obligation:** REQ-004 and the packet say `Public` carries no secret; secret methods require a nonempty secret.
- **Location:** `crates/wyrd-spec/src/auth/human_connection.rs:221-252`; sole production caller at `components/admin/identity.rs:179`.
- **Evidence:** Validation defines `has_secret` as `Some` and nonempty. `Public` plus `Some("")` therefore falls through as valid, after which staging silently drops the supplied value.
- **Observable consequence:** The API accepts a contract-invalid shape and hides client/operator misconfiguration.
- **Decision-complete correction:** Branch on `Option` presence and content separately: every `Some` is invalid for `Public`; `SecretBasic` and `SecretPost` require `Some` with nonempty contents. Preserve the database invariant that public rows store `NULL`.
- **Focused closure proof:** Extend the existing contract unit test with `Public` plus empty secret and both secret methods plus empty secret; all must fail with the appropriate field validation, while omitted Public and nonempty secret-method inputs pass.

## Caller tracing and verification limits

- The correction boundaries were traced through their complete real caller sets: shared screened HTTP consumers; the private provider probes; server login's sole callback helper; all `HumanConnections` constructors; callback issuance; the sole refresh route and issuance owner; boot's sole production rewrap call plus the test harness call; and the sole production `ConnectionInput::from_json` boundary.
- No candidate implementation file was modified and no long provider/Postgres/Cargo lane was rerun within the Wave 2 time budget. Findings rely on immutable source, local dependency source for reqwest proxy defaults, committed test source, and the Wave 1 execution evidence.
- No sub-reviewer was awaited by this Wave 2 reviewer. There is therefore no missing sub-reviewer verification gap.

## Recommendation

**FIX_REQUIRED** — retain `FIND-TASK-001-1` through `FIND-TASK-001-13`.
