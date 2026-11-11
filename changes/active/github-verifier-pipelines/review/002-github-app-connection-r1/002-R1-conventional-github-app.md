---
id: TASK-002-R1
kind: remediation
status: proposed
parent_task: TASK-002
approved_spec_revision: 5
findings: [FIND-002-1, FIND-002-2, FIND-002-3, FIND-002-4, FIND-002-5, FIND-002-6, FIND-002-8, FIND-002-9, FIND-002-10, FIND-002-11, FIND-002-12, FIND-002-13, FIND-002-15]
---

# Complete the conventional GitHub App connection

Implement this packet with `$wyrd-implement` against the current working tree.
The user approved revision 5 to use GitHub.com’s documented App installation
and OAuth flow. The specification decision raised by the original review is
resolved. This packet is implementation-ready under that scope.

## Authority and immutable review context

- Approved spec is [revision 5](../../spec.md), with [contracts](../../contracts.md)
  and [App design](../../design/approved-runtime-and-github.md).
- Original task is [002 GitHub App connection](../../tasks/002-github-app-connection.md).
- Reviewed base is `395274d58bf76a2075f245ca3a5c1bd044925027`.
- Reviewed candidate is `3dd1555a0f40117ee512ecaaff1f5b51845ef3b6`.
- Preserve [the original verdict](verdict.md), [validated ledger](findings-validation.md),
  all specialist reports, and [independent verification](verification.md) as
  evidence for revision 4. [Current disposition](scope-resolution.md) records
  the subsequent human decision. No implementation correction is claimed yet.

Paths below refer to the reviewed candidate. `service.rs`, `app.rs` and
`routes.rs` belong to `crates/wyrd/wyrd-server/src/components/github/`.
`slots.rs` means `crates/wyrd/wyrd-sql/src/queries/github_installations.rs`.
The server integration owner is
`crates/wyrd/wyrd-server/tests/integration/pg_github_connection.rs`.

## GitHub.com scope and finding disposition

Production uses `https://github.com` and `https://api.github.com`. Keep local
provider origins for development mocks. Apply this scope through the existing
GitHub configuration validation, configuration tests and operator guide. Use
GitHub.com’s documented setup and OAuth callback with ordinary state, PKCE and
installation confirmation. The existing verified callback already supplies
that workflow. Add no provider discovery, capability negotiation, alternative
authorization protocol or additional user confirmation step.

Only the active IDs in this packet require work. The removed Enterprise Server
scope is historical context in `scope-resolution.md` and supplies no additional
implementation requirements. Use the existing GitHub.com callback and mock to
prove ordinary connection and refusal of a code substituted from another
transaction.

FIND-002-2 is limited to existing saved association and OAuth transaction
settings. It adds comparisons against persisted fields, with no provider
management or cross-host test matrix.

## Diagnosis, selected correction and closure proof

### FIND-002-1 — Credential handoff after revocation

REQ-001, AC-001 and O19/O22/O23 require access loss to fence source use.
`service.rs:635–668` commits an unlocked slot snapshot before provider IO and
returns the token directly. Disconnect can release that slot and another tenant
can claim it while the response waits. The old tenant then receives usable
authority. Sequential refusal assertions never overlap these operations.

Make credential handoff conditional on the same active slot revision after
provider IO using the existing tenant slot owner. Discard a stale provider
result. Refuse released, replaced, suspended or deselected access. If the revision changed, return the existing typed stale-connection refusal
after discarding the result. A normal new request checks current authority.
Prove a new request for an unaffected repository succeeds. A revision changed by release/reclaim
cannot authorize handing out the old result. End SQL transactions before
provider waits. This is the shared token owner’s trust boundary and needs no
new queue or durable state.

Hold an existing mock mint response, commit disconnect, then release the
response and assert the old request returns no credential. Exercise selection
loss and an unrelated-repository control. Update the inaccurate serialization
claim at `service.rs:325–329` alongside the correction.

### FIND-002-2 — Saved identity ignored by current configuration

INV-001 and O6/O18/O19 require saved authority to describe the provider used.
The slot persists host, and login state persists issuer, client ID and callback
URL. `service.rs:177–198,369–393,439–451,640–668` uses current settings without
checking those saved values. Configuration replacement can reinterpret old
state or send an old authorization code to a different configured client.

Check saved slot identity before projecting active access or requesting a
token, preserving authorized release of stale associations. Compare saved
issuer, OAuth client and callback with current configuration at setup and
callback before redirect, consumption or code exchange. Reuse current typed
transaction refusal and existing persisted fields. Prove a mismatched saved
slot yields no active authority or token request, and a transaction begun under
old settings cannot redirect or exchange under replacement settings. Normal
release and a fresh legitimate connection must succeed. No cross-host product
workflow is needed.

### FIND-002-3 — Invalid signing key accepted at startup

O3/O16 require typed configuration failure for invalid present App settings.
`config.rs:2439–2453` and `app.rs:148–156` parse an RSA encoding key, but the
installed `jsonwebtoken` decoder accepts the fixture’s public PEM. Actual
RS256 signing fails only on the first provider request, misreported as provider
unavailability.

Validate RS256 signing through the installed signer during configuration
resolution and let the App consume the validated key. Remove duplicate
decoding. Use the checked-in public half to assert a field-specific typed
configuration error, retain the valid private-key case, and verify the signed
App request. Keep signing material out of diagnostics.

### FIND-002-4 — OAuth response media negotiation

O7/O8/O10 require the provider callback to work. `app.rs:221–241` inherits the
REST vendor Accept header and decodes JSON, while the web token endpoint
documents JSON negotiation through `Accept: application/json`. The mock at
`wyrd-testing/src/github.rs:531–555` always responds with JSON and masks this.

Set the documented JSON Accept header at the web exchange and keep the REST
API’s existing headers. Make the existing mock require that negotiation and
drive the normal callback through it. Reuse screened transport and the existing
JSON decoder. A new response parser or GitHub library is unnecessary.

### FIND-002-5 — Suspension delivery order corrupts current access

AC-001 and O21/O22 require synchronized access. `service.rs:587–595` writes the
arriving action, and `slots.rs:69–78` skips equal updates. A newer unsuspend
followed by an older suspend can leave an active installation fenced
indefinitely. Ordered delivery assertions cannot establish recovery.

On either signed suspension transition, use the existing App installation
lookup to obtain current provider state. Apply that observation through the
existing suspension owner conditional on the routed slot identity and revision.
Keep provider IO outside the SQL transaction. If the revision changed during
the lookup, discard the observation and return an existing typed failure so
the delivery can be retried through the ordinary recovery path.
Advance the private fence even for an unchanged successful observation so an
older in-flight observation cannot overwrite it. Preserve public timestamps
when the public state is unchanged. Provider failure supplies no observation.
Keep genuine suspension ownership and ordinary deletion/release semantics.

Prove reverse deliveries and an older paused observation overtaken by a newer
matching observation. Assert the final public state and scoped token decision
match current provider state. Do not add event history or a delivery store.

### FIND-002-6 — Repository removal misses a pending selection

O10/O21/O22 require acknowledged access loss to fence stale callbacks.
`slots.rs:81–96` advances revision only when removed IDs overlap current
selection. With A selected and a callback already having read A+B, a signed
removal of B is acknowledged without a fence, then the callback restores B.
The SQL assertion at `pg_github_installations.rs:482–488` encodes this gap.

Advance the existing private revision on a nonempty removal for the exact
associated host/installation even if selection has no overlap. Subtract only
named repositories and retain public timestamps when selection is unchanged.
Using real SQL, capture the binding for A, remove B, and attempt A+B with the
old binding. Assert stale refusal and A retained. The opposite commit ordering
must remove B normally. Keep the approved associated-installation routing
boundary. No pending claim or event precedence system is authorized.

### FIND-002-8 — Browser setup response permits caching

The security posture’s credential response rule applies to
`routes.rs:189–207`, which returns the binding cookie and redirect state
without `Cache-Control: no-store`. The router supplies no equivalent directive.
Use the existing `auth/oauth.rs:140–151` response owner, as browser login does.
Assert no-store, the binding cookie and the redirect together on the served
setup response. Cookie flags alone cannot close the cache-policy gap.

### FIND-002-9 — Unearned local HMAC implementation

The repository reuse rule applies to `wyrd-crypt/src/lib.rs:449–480`, which
implements HMAC key normalization and padding despite resolved RustCrypto
`hmac` 0.12.1. Keep the existing shared signing/verification API and implement
it through that installed primitive and its constant-time full-tag verifier.
Remove the direct `subtle` dependency if unused. Preserve the independent RFC
4231 short/long-key and wrong/truncated-tag assertions and signed webhook proof.
No new crypto package or provider dependency is needed.

### FIND-002-10 — Required Rust documentation is incomplete

The explicit changed-item rustdoc gate covers the inventory in
`standards-review.md`, REP-GH-01, including new module declarations, SQL test
constant and associated type, fixture panic contracts and callback cancellation
after state consumption. `slots.rs:169–171` also calls an uncommitted
caller-owned transaction result committed.

Complete the following inventory while making these corrections and relocating
the tests.

- Document new module declarations in `wyrd-client/src/lib.rs:22`,
  `wyrd-spec/src/lib.rs:19`, `wyrd-cli/src/lib.rs:19`, server
  `components/mod.rs:7`, `components/github/mod.rs:10–12`, `mcp/mod.rs:70`,
  SQL `queries/mod.rs:16`, `wyrd-testing/src/lib.rs:7`, Python `src/lib.rs:16`,
  both TypeScript native `lib.rs` files and the CLI/MCP/server/SQL test registrations.
- Document SQL test `HOST` at `pg_github_installations.rs:33` and
  `TryFrom::Error` at `queries/auth/login_state.rs:372`.
- Add actual panic contracts for CLI helpers at `github_journey.rs:21,35,46,56`,
  server test helpers at `pg_github_connection.rs:24,33`, config fixture helper
  at `config.rs:6790`, the OpenAPI test at `pg_openapi_contract.rs:300`, and
  Rust SDK helpers at `github_connection.rs:24,33,46,69,88,114,122`.

Document caller-owned commit, spent authorization state after failure,
and actual credential fencing. Include the changed test declarations. Inspect
the complete changed-item set and run the narrow format/lint checks. Batch this
work with behavior fixes.

### FIND-002-11 — SQL tests silently pass without PostgreSQL

SQL directory rules forbid silent environment skips. Every new test in
`pg_github_installations.rs` returns before `PgFixture::start` when its local
environment helper finds no database URL. A miswired lane reports success
without testing uniqueness, isolation or revocation.

Delete that guard and rely on the existing fixture’s visible failure. Run the
focused SQL cases under repository-managed PostgreSQL and one representative
direct selection without the environment. The latter must fail visibly.

### FIND-002-12 — Real-server tests lack required source placement

The CLI `tests/github_journey.rs` and Rust SDK integration
`github_connection.rs` run server/PostgreSQL tests at top level without the
required `pg_tests` module or `pg_*` filename. Use the existing source marker
convention, update target registration and affected `mise.toml` selectors, and
prove the same cases are discovered and pass. Keep existing binaries and lane
membership.

### FIND-002-13 — Published setup commands cannot be followed

O30/AC-011 and docs publishing rules apply to
`docs/src/content/docs/operate/connect-github.md:48–67`. Bare angle-bracket
assignments are shell redirections. The CLI journey injects environment in Rust
and never executes the published setup snippets, so its green result misses
this failure.

Use valid substitutable values and the existing local-example convention for
operator setup and tenant CLI commands. Drive that minimal example with the
existing mock credentials and real-server CLI journey, then verify the guide
and generated docs through their owners. Update provider scope to GitHub.com.
The permissions/events and served URL assertions already pass and carry forward.

### FIND-002-15 — Exact named proof commands are incomplete

The task records module filters and abbreviated commands for individually
named config, SQL, server, CLI, SDK and in-process callback tests. The repository
requires executable exact selectors for named proof. Independent verification
already supplies exact HMAC/OpenAPI selectors, real-provider OIDC evidence and
TypeScript format/lint/type results.

Record and run only missing exact selectors, grouped under the existing
repository wrappers and target. Use final names after test placement changes.
Carry forward already valid proof. Preserve the disclosed missing historical
RED; record genuine focused failure and success for new behavioral repairs.

## Acceptance and execution boundaries

Each section’s stated observation is the acceptance criterion for its finding.
All thirteen active IDs must have source correction or the specified evidence
attached. Batch the HMAC reuse, Rust documentation, SQL fixture, test placement
and exact command recording with the relevant behavior changes. These are
small repository-required corrections, with no separate implementation or
review round for each item.

The historical diagnosis item is satisfied by the independent source tracing
already in `findings-validation.md`, FIND-002-14, and the behavior and standards
reports. They trace the Card fixture, authentication header and catalog causes.
Reference that evidence in the task record and retain the lack of original
before-edit traces as a historical limit. No new diagnostic agent or report is
required. This is an amended disposition of the earlier remediation demand.

Preserve tenant RLS, inverse uniqueness, OIDC behavior, current permission/audit
owners, transient/redacted credentials, committed receipts, unrelated repository
access and existing language projections. Use the dedicated table and existing
login state. Task 006 adds actual PR-linked queued work to the same fence.
Keep UI, new queues/tables, capability negotiation, alternate provider protocols
and unrelated refactors outside this packet.

## Focused verification and handoff

Record exact selectors for new regression tests once they exist. Use the
existing server/mock integration for credential and webhook interleavings, SQL
integration for revision/claim behavior, config unit tests for provider/key
validation, and the existing CLI story for executable guide proof. Reuse the
repository PostgreSQL lifecycle and migration tasks. No new harness is needed.

Run only the changed exact config, SQL, server, CLI and Rust SDK cases. Add the
existing HMAC vector and matching callback/webhook assertions when their owners
change. If callback transport affects projections, run their existing focused
GitHub cases on the shipped surfaces. Run `mise run fmt`, `mise run lints`,
`mise run check:tenant-isolation` and `mise run check:deps` for this write set,
plus `mise run docs:check` for guide/example changes. Run codegen and language
checks only when their contracts or SDK sources change. Full journey suites
and broad aggregates run at integrated change review.

Hand this packet directly to `$wyrd-implement`. Later task review checks the
candidate-to-remediation diff, finding closure and affected consumers, carrying
forward the original reviewed evidence for unchanged behavior.

## Obligations

Recorded after implementation began (process exception, see Evidence). Paths:
SRV `crates/wyrd/wyrd-server/tests/integration/pg_github_connection.rs`, SQLT
`crates/wyrd/wyrd-sql/tests/integration/pg_github_installations.rs`, CFG
`crates/wyrd/wyrd-server/src/config.rs`, SVC `.../components/github/service.rs`,
MOCK `crates/wyrd/wyrd-testing/src/github.rs`, CLI
`crates/wyrd/wyrd-cli/tests/github_journey.rs`, SDK
`sdks/wyrd-sdk-rust/tests/integration/github_connection.rs`.

| ID | Source | Planned assertion |
|---|---|---|
| R1-1a | FIND-002-1 "Hold an existing mock mint response, commit disconnect … no credential" | SRV held mint across disconnect → `CONNECTION_CONFLICT`, no token |
| R1-1b | FIND-002-1 "Refuse released, replaced, suspended or deselected access" | SRV held mint across disconnect+reclaim, suspension, own removal → conflict; then current refusal |
| R1-1c | FIND-002-1 "a new request for an unaffected repository succeeds" / unrelated control | SRV unrelated removal discards the held token; fresh request mints |
| R1-1d | FIND-002-1 "Update the inaccurate serialization claim" | SVC rustdoc on `fence_and_release` / `installation_token` |
| R1-2a | FIND-002-2 mismatched saved slot → no active authority or token request | SRV host rewrite → `not_connected`, 404, GitHub not asked |
| R1-2b | FIND-002-2 "preserving authorized release of stale associations" | SRV disconnect of the host-mismatched slot succeeds |
| R1-2c | FIND-002-2 transaction under old client cannot redirect | SRV `client_id` rewrite → `transaction_invalid`, no authorize hop |
| R1-2d | FIND-002-2 transaction under old callback cannot exchange | SRV `redirect_uri` rewrite → `transaction_invalid`, exchange count unchanged |
| R1-2e | FIND-002-2 "fresh legitimate connection must succeed" | SRV fresh connect → active, token minted |
| R1-3a | FIND-002-3 public PEM → field-specific typed config error | CFG case naming `WYRD_GITHUB_APP_PRIVATE_KEY_FILE` |
| R1-3b | FIND-002-3 valid private key retained and signs a verified App JWT | CFG `assert_signs_app_jwts` with the public half |
| R1-3c | FIND-002-3 "Remove duplicate decoding"; signing material out of diagnostics | `GithubApp::new` infallible, key from settings; CFG refusal never quotes key |
| R1-4 | FIND-002-4 JSON Accept at exchange; mock requires it; normal callback through it | MOCK answers JSON only on `application/json`; SRV/SDK callbacks succeed |
| R1-5a | FIND-002-5 reverse deliveries end at GitHub's current state, view and token match | SRV unsuspend→suspend (GitHub active) and suspend→unsuspend (GitHub suspended) |
| R1-5b | FIND-002-5 older paused observation overtaken → refused for redelivery | SRV held installation read → 409, state active, mint |
| R1-5c | FIND-002-5 unchanged observation advances fence, keeps public timestamps | SQLT `again.revision`, timestamps equal; SRV `updated_at` equal |
| R1-6a | FIND-002-6 removal of unselected B fences stale A+B callback; A retained | SQLT stale → `Stale`, selection `[A]` |
| R1-6b | FIND-002-6 opposite order removes B normally | SQLT callback lands then removal → `[A]` |
| R1-6c | FIND-002-6 nonempty removal advances revision, unchanged selection keeps timestamp | SQLT `fenced.revision`, `updated_at` equal; empty removal writes nothing |
| R1-8 | FIND-002-8 no-store, binding cookie and redirect together on the served setup response | SRV setup 302 headers |
| R1-9 | FIND-002-9 HMAC through installed `hmac`; `subtle` removed; RFC 4231 + wrong/truncated tags | `wyrd-crypt` vector test |
| R1-10 | FIND-002-10 rustdoc inventory | doc lines at every listed site; fmt/lints |
| R1-11a | FIND-002-11 guard deleted; focused SQL cases pass on repository Postgres | SQLT exact selectors |
| R1-11b | FIND-002-11 direct run without DB fails visibly | `env -u` run panics in fixture |
| R1-12 | FIND-002-12 `pg_tests` placement, selectors updated, same cases discovered and pass | CLI/SDK exact selectors; `mise.toml` |
| R1-13a | FIND-002-13 valid substitutable values, local-example convention | `examples/connect-github/*.sh` embedded with `CodeFromFile`; run as published |
| R1-13b | FIND-002-13 tenant CLI commands driven by the real-server CLI journey | CLI runs `connect.sh`/`status.sh` against the mock-backed server |
| R1-13c | FIND-002-13 operator setup driven with mock credentials | CFG runs `server.sh`, resolves in production |
| R1-13d | FIND-002-13 provider scope GitHub.com; docs verified through owners | guide text; `docs:generate`, `docs:check` |
| R1-S | Scope: production GitHub.com only, local origins for mocks | CFG GHES-origin cases refuse in production; local origins resolve outside production |
| R1-14 | Historical diagnosis disposition | reference `findings-validation.md` FIND-002-14 |
| R1-15 | FIND-002-15 exact selectors for named tests | exact-command block below |

## Evidence

| Obligation | Implementation evidence | Assertion | Verification evidence | Result |
|---|---|---|---|---|
| R1-1a | SVC `installation_token` re-reads the slot after minting; binding mismatch drops the token and returns `connection_changed` (SVC:755–765) | SRV:578–583, SRV:584 | `pg_github_connection::tokens_minted_across_an_access_change_are_discarded`; RED F1 (fence disabled) panicked SRV:530 "the token is discarded" | PASS |
| R1-1b | same; suspension and any removal advance revision | SRV:545–557 (suspension), SRV:541–543 (own removal), SRV:563–576 (disconnect+reclaim) | same test | PASS |
| R1-1c | revision movement refuses only the held result; new request decides on current slot | SRV:534–539 | same test | PASS |
| R1-1d | SVC module, `fence_and_release`, `installation_token` rustdoc describe the post-IO binding check | inspection | checker | PASS |
| R1-2a | SVC `associated` filters on `row.host == settings.host()` (SVC:188–194) | SRV:640–648 | `…::state_saved_under_other_app_settings_grants_nothing`; RED F2 (host filter off) panicked SRV:623 `left: "active"` | PASS |
| R1-2b | disconnect keeps the unfiltered slot read | SRV:649 (`disconnect` panics unless 200) | same test | PASS |
| R1-2c | SVC `begun_here` (SVC:198) applied before setup redirect (SVC:410) | SRV:652–671 | same test; RED F2b (`begun_here` true) panicked SRV:637 (view active) | PASS |
| R1-2d | `begun_here` before callback commit/exchange (SVC:481) | SRV:673–701 | same test (`mock.exchanges()` unchanged) | PASS |
| R1-2e | — | SRV:737–741 | same test | PASS |
| R1-3a | CFG `resolve` signs a probe JWT via `EncodingKey::from_rsa_pem` + `jsonwebtoken::encode` (CFG:2459–2475) | CFG:7072, CFG:7094 | `config::tests::github_app_config_is_optional_but_complete_once_present`; no RED captured (exception) | PASS |
| R1-3b | `GithubAppSettings.signing_key` | CFG:6876, CFG:6969, CFG:7017 | config github tests | PASS |
| R1-3c | `GithubApp::new` infallible, `app_jwt` uses `settings.signing_key` (app.rs); refusal names the env var only | CFG:6771 (no key material in any refusal) | config github tests | PASS |
| R1-4 | `exchange_code` → `request_accepting(POST, url, "application/json")` (app.rs:229); MOCK:591–619 answers form-urlencoded otherwise | SRV:136 callback 200; SDK/CLI/MCP/Python/TS callbacks | `pg_github_connection::installation_tokens_follow_repository_access`; RED F4 (vendor Accept) panicked at the callback assert with `WYRD_GITHUB_503_PROVIDER_UNAVAILABLE` | PASS |
| R1-5a | SVC `observe_suspension` (SVC:667) reads `app.installation()` after commit, applies `set_github_installation_suspended(observed, current.suspended)` | SRV:600–605, SRV:607–611 | `…::suspension_follows_github_whatever_the_delivery_order`; RED F5 (applied action instead of observation) panicked SRV:589 | PASS |
| R1-5b | SQL suspend conditional on `connection_id`+`revision`; None → `connection_changed` 409 | SRV:613–622 | same test | PASS |
| R1-5c | `SUSPEND_SQL` always bumps revision, `updated_at`/`suspended_at` change only on state change | SQLT:240–256, SRV:604 | `pg_github_installations::pg_tests::installation_belongs_to_one_tenant_until_released` | PASS |
| R1-6a | `REMOVE_REPOSITORIES_SQL` matches any nonempty removal for host+installation | SQLT:529–538 | `pg_github_installations::pg_tests::removal_fences_a_callback_that_read_the_removed_repository`; RED under the old overlap guard panicked SQLT:521 "the removal applies" | PASS |
| R1-6b | — | SQLT:540–549 | same test | PASS |
| R1-6c | `updated_at` changes only on overlap | SQLT:472–488 | `…::slots_are_tenant_confined_and_track_removed_access` | PASS |
| R1-8 | routes.rs:204 `no_store(StatusCode::FOUND, [LOCATION, SET_COOKIE])` | SRV:719–735 | `…::state_saved_under_other_app_settings_grants_nothing`; RED F8 (plain tuple) panicked SRV:707 (no `Cache-Control`) | PASS |
| R1-9 | `wyrd-crypt` `hmac_sha256`/`verify_hmac_sha256` via `hmac::Hmac::verify_slice` (lib.rs:450–470); Cargo.toml `hmac`, no `subtle` | lib.rs:507–545 (RFC 4231 short/long key, wrong at :536, truncated) | `wyrd-crypt tests::hmac_sha256_matches_rfc_4231`; refactor with unchanged vectors, no RED (no behavior change) | PASS |
| R1-10 | `///` on every listed module and test registration, `TryFrom::Error`, SQLT `HOST` (SQLT:32), `# Panics` on all listed helpers (CLI, SRV `refused`/`minted`, CFG `github_key_file`, OpenAPI test, SDK helpers); SVC callback "spent state" and SQL `Associated` caller-owned commit docs | inspection | `mise run fmt`, `mise run lints` clean; checker | PASS |
| R1-11a | SQLT `database_url()` guard deleted | SQLT selectors | 6 SQLT exact selectors pass on repository Postgres | PASS |
| R1-11b | — | fixture panic | `env -u WYRD_DATABASE_URL … -E 'test(=pg_github_installations::pg_tests::installation_belongs_to_one_tenant_until_released)'` panicked "fixture starts: … test DB env unset (WYRD_TEST_DATABASE_ADMIN_URL)" | PASS |
| R1-12 | `mod pg_tests` in CLI and SDK files; `mise.toml` selectors `github_journey::pg_tests::`, `github_connection::pg_tests::` | discovery under new names | CLI 1 and SDK 4 exact selectors pass | PASS |
| R1-13a | `examples/connect-github/{server,connect,status}.sh` with quoted `"<…>"`; guide embeds each with `<CodeFromFile … lang="bash" />`; `examples/README.md` row | CLI:58–76 (guide embeds it; script runs as published with `wyrd` stubbed), CFG:6929 | CLI + CFG guide tests; RED with bare `<…>` restored: CFG panicked CFG:6929 and CLI panicked CLI:74 "syntax error near unexpected token `newline'" | PASS |
| R1-13b | CLI `guide_step` runs the scripts with the shipped `wyrd` on PATH, the server address and admin key substituted, env scrubbed by shared `run_scrubbed` | CLI:176–190 (connect receipt from `connect.sh`), CLI:201–220 (`status.sh` reports active with both repositories) | `github_journey::pg_tests::tenant_connects_github_through_the_cli` | PASS |
| R1-13c | `config::tests::github_guide_settings_configure_a_production_server` fills placeholders, mounts the secret files, exports via bash, resolves with `resolve(true)` | CFG:6959–6975 | that test | PASS |
| R1-13d | guide prerequisite and step 2 name GitHub.com only; `docs/public/llms-full.txt` regenerated | `docs:check` | `mise run docs:generate`, `mise run docs:check` clean | PASS |
| R1-S | CFG `resolve` refuses non-GitHub.com origins in production (CFG:2411–2428) | CFG:7079, CFG:7086 (refused in production); CFG:6872 (local mock origin outside production) | config github tests; no RED captured (exception) | PASS |
| R1-14 | Historical diagnosis is satisfied by the independent tracing in `findings-validation.md` FIND-002-14 and the behavior and standards reports; the missing before-edit traces remain a historical limit | — | reference | PASS |
| R1-15 | exact selectors below | — | all pass | PASS |

Non-goals stayed out: no provider discovery, capability negotiation, new tables
or queues, event history, PKCE downgrade or Enterprise Server path. Unrelated
working-tree edits (`.agents/skills/wyrd-task-review/SKILL.md`,
`changes/spec-driven-development.md`) are not part of this change.

### Exact commands

All under `scripts/postgres/with-test-postgres.sh -- bash -lc 'mise run db:migrate:all:inner; …'`:

```bash
cargo nextest run --locked -p wyrd-server --lib -E 'test(=config::tests::github_app_config_parses_from_env_and_files_and_redacts) | test(=config::tests::github_app_config_is_optional_but_complete_once_present) | test(=config::tests::github_app_config_reads_toml_fields_and_secret_files) | test(=config::tests::github_secrets_never_come_from_toml) | test(=config::tests::github_guide_settings_configure_a_production_server) | test(=auth::callback::pg_tests::finish_creates_and_audits_a_browser_session)'  # 6 passed
cargo nextest run --locked -p wyrd-sql --test integration -E 'test(=pg_github_installations::pg_tests::association_is_conditional_on_the_bound_slot_revision) | test(=pg_github_installations::pg_tests::concurrent_claims_have_one_winner_in_both_directions) | test(=pg_github_installations::pg_tests::slots_are_tenant_confined_and_track_removed_access) | test(=pg_github_installations::pg_tests::github_transactions_are_flow_principal_and_browser_bound) | test(=pg_github_installations::pg_tests::installation_belongs_to_one_tenant_until_released) | test(=pg_github_installations::pg_tests::removal_fences_a_callback_that_read_the_removed_repository) | test(=pg_migration::pg_tests::migrations_apply_and_are_idempotent)'  # 7 passed
cargo nextest run --locked -p wyrd-server --test integration -E 'test(=pg_github_connection::installation_tokens_follow_repository_access) | test(=pg_github_connection::tokens_minted_across_an_access_change_are_discarded) | test(=pg_github_connection::suspension_follows_github_whatever_the_delivery_order) | test(=pg_github_connection::state_saved_under_other_app_settings_grants_nothing) | test(=pg_openapi_contract::github_connection_operations_publish_their_contract)'  # 5 passed
cargo nextest run --locked -p wyrd-crypt --lib -E 'test(=tests::hmac_sha256_matches_rfc_4231)'  # 1 passed
cargo nextest run --locked -p wyrd-cli --test cli -E 'test(=github_journey::pg_tests::tenant_connects_github_through_the_cli)'  # 1 passed
cargo nextest run --locked -p wyrd-ai --test integration -P journey --run-ignored=all -E 'test(=github_connection::pg_tests::connections_associate_only_after_github_confirms_them) | test(=github_connection::pg_tests::installation_events_synchronize_the_connection) | test(=github_connection::pg_tests::spoofed_foreign_stale_and_conflicting_connections_are_refused) | test(=github_connection::pg_tests::an_unconfigured_deployment_refuses_only_github)'  # 4 passed
cargo nextest run --locked -p wyrd-mcp --test mcp -P journey --run-ignored=all -E 'test(=github::pg_tests::an_agent_manages_the_github_connection)'  # 1 passed
(cd sdks/wyrd-sdk-python && uv run python -m pytest -q -m 'integration and not local_dev' tests/integration/test_github_connection.py)  # 3 passed, after mise run py:setup
(cd sdks/wyrd-sdk-ts/wyrd && pnpm exec vitest run tests/integration/github-connection.test.ts tests/integration/github-connection-not-configured.test.ts)  # 3 passed, after mise run ts:build ts:build:testing
```

`mise run fmt`, `mise run lints`, `mise run check:tenant-isolation`,
`mise run check:deps`, `mise run docs:check` and `git diff --check` are clean.
RED runs temporarily mutated one production line each (F1, F2, F2b, F4, F5,
F8), ran the exact selector, and restored the source.

### Process exceptions and deviations

- The ledger was recorded after implementation began. FIND-002-3 and the
  production-origin scope have no captured RED.
- FIND-002-13 operator setup is driven by the config unit test rather than
  the CLI story. A production server cannot point at the mock, because only
  GitHub.com origins are accepted, so configuration resolution is the owner
  that can prove the published exports. The tenant commands run in the
  real-server CLI journey.
- **Diagnosis, two suspension tests.** Symptom: SRV
  `installation_tokens_follow_repository_access` minted a token after
  `suspend`, and SDK `installation_events_synchronize_the_connection:493` saw
  `Active`, not `Suspended`. Evidence: the trace showed "suspension
  synchronized suspended=false revision=3". Cause: both tests delivered
  `suspend` while the mock still reported the installation unsuspended, and
  suspension now applies GitHub's observed state (R1-5). Fix site: the tests'
  mock setup, where `set_suspended(101, true)` now precedes the delivery; no
  assertion changed. A fresh read-only diagnostician independently found the
  same cause and fix site, and found no other affected tests across CLI, MCP,
  Python or TypeScript.
