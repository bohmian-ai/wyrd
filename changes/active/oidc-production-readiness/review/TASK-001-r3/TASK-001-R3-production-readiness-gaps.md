---
id: TASK-001-R3
kind: remediation
status: ready
spec: SPEC-oidc-production-readiness
spec_revision: 4
requirements: [REQ-004, REQ-017, INV-004, AC-007]
depends_on: []
parent_task: TASK-001
remediates: [FIND-TASK-001-20, FIND-TASK-001-21, FIND-TASK-001-22, FIND-TASK-001-23]
---

# Close final tenant-connection live-login, audit, DNS, and documentation gaps

## Authority and immutable review subject

- Approved spec: `changes/active/oidc-production-readiness/spec.md`, revision 4
- Original task: `changes/active/oidc-production-readiness/tasks/TASK-001-tenant-connections.md`
- Prior remediation: `TASK-001-R1-production-readiness-gaps.md` and
  `TASK-001-R2-remaining-production-readiness-gaps.md`
- Reviewed base: `a5c8041a348a66bfb56fbac492383e8c688b0590`
- Reviewed candidate: `eb4142cc95fa24b0525c71f66bfc978577cd4b7c`
- Validated ledger:
  `changes/active/oidc-production-readiness/review/TASK-001-r3/findings-validation.md`

Implement this task through `$wyrd-implement`. A later review must reassess the
complete cumulative base-to-candidate range, not only this remediation diff.

## Outcome

Finish TASK-001 by enforcing the existing production scheme rule on the live
browser authorization destination, auditing the recovery principal's real
permission decision, bounding shared provider DNS resolution, and completing
the one missing rustdoc contract. Reuse the current owners, timeout, audit path,
and tests; add no product scope, public contract, dependency, or configuration
surface.

## Issue diagnoses and required corrections

### FIND-TASK-001-20 — live login permits a newly discovered cleartext authorization endpoint

TASK-001 requires discovered provider endpoints to be validated and INV-004
requires production TLS to fail closed. Candidate qualification screens the
authorization endpoint, but `HumanConnections::begin_login` performs fresh
discovery at `crates/wyrd/wyrd-auth/src/login.rs:137-157`, accepts the
`AbsoluteUrl`, persists login state, and returns the browser destination without
reapplying the deployment scheme rule. Because `AbsoluteUrl` permits HTTP, an
active provider can change its discovery document after qualification and send
live state, nonce, PKCE challenge, client id, and callback parameters to a
cleartext endpoint.

Apply the existing deployment address policy's scheme rule to the effective
authorization endpoint before constructing or persisting login state.
`BlockInternal` must require HTTPS and return the existing redacted discovery
refusal; `AllowInternal` must retain the repository-managed HTTP provider path.
This is browser-destination validation, not a server fetch: do not build a
throwaway pinned client, cache provider metadata, add a policy knob, or alter
token, JWKS, or platform-login behavior.

### FIND-TASK-001-21 — fallible bounded-body test helper lacks `# Errors`

The R2-added private test helper `bounded_get` at
`crates/shared/wyrd-auth-oidc/src/screening.rs:481-496` returns
`Result<Vec<u8>, BodyError>` but documents only its panic condition. Repository
rules require substantive rustdoc and `# Errors` for every new fallible Rust
item, including private test helpers.

Add only the missing `# Errors` section. It must state that the helper returns
`BodyError::TooLarge` when the shared cap is exceeded and `BodyError::Read` for
transfer or decoding failure. Do not change the helper or add a behavioral test
for a documentation-only correction.

### FIND-TASK-001-22 — recovery principal permission decision is unaudited

Activation first audits the bearer caller, then the locked activation workflow
at `crates/wyrd/wyrd-auth/src/connections.rs:451-497,918-952` verifies a second
credential, loads that principal's current grants, and evaluates
`identity_connections:write`. The allowed and valid-but-underprivileged paths
append no audit event for the recovery principal. Retained evidence therefore
cannot identify the recovery authority or its denial, and an audit failure
cannot fail that decision closed.

Within the existing locked activation transaction and canonical audit writer,
append a second decision whenever a recovery key resolves to an active
principal and reaches permission evaluation. Attribute it to that principal
and the verified non-secret credential id, record the same typed permission and
Allowed or Denied outcome, and append it before promotion or committed refusal.
An append failure must leave the old Active and Candidate unchanged. Preserve
the existing bearer decision and the indistinguishable response for malformed,
unknown, cross-tenant, hash-mismatched, or inactive credentials; those cases do
not resolve a principal permission decision and must not gain a fabricated row.
Add no alternate audit path or public field.

### FIND-TASK-001-23 — shared provider DNS lookup has no deadline

`ScreenedHttp::client_for` reaches `resolve_and_screen` at
`crates/shared/wyrd-auth-oidc/src/screening.rs:154-215`, which awaits
`tokio::net::lookup_host` before the reqwest client carrying `FETCH_TIMEOUT`
exists. Discovery, JWKS, token exchange, and candidate probes all share this
path, so a stalled system resolver can retain serving work indefinitely before
an outbound connection.

Wrap the existing single DNS lookup in the existing fixed `FETCH_TIMEOUT` and
map timeout to the same redacted `ScreenError::Unresolved` result. Preserve the
exact resolved address set supplied to `resolve_to_addrs`, the current network
classification, and cancellation behavior. Do not add a resolver trait,
dependency, retry, configuration knob, or per-caller timeout.

## Constraints and preserved behavior

- Keep OIDC optional and retain connectionless boot and machine authentication.
- Preserve the one-Active/one-Candidate lifecycle, `TenantConn`/RLS boundary,
  exact connection provenance, migration behavior, and current public API.
- Keep production server fetches HTTPS-only, DNS-pinned, proxy-free,
  redirect-free, response-bounded, and redacted; keep local HTTP providers
  working only under `AllowInternal`.
- Preserve one canonical audit writer and existing bearer-caller decision.
- Do not expose provider secrets, recovery material, credential hashes, or
  lookup distinctions in responses, logs, traces, audit, or tests.
- Keep the callback-routing contract unchanged; the rejected host-selection
  observation is outside TASK-001 and requires separate approved work.

## Non-goals

- No UI, CLI login, hosted signup, commercial hook, callback-routing redesign,
  platform OIDC redesign, new client-auth method, cached discovery, custom DNS
  resolver, timeout configuration, audit sink, or compatibility route.
- No unrelated rustdoc sweep, helper refactor, dependency, feature, or test
  harness.

## Acceptance criteria and proof

| Finding | Acceptance criterion | Focused proof |
|---|---|---|
| `FIND-TASK-001-20` | `BlockInternal` refuses a freshly discovered HTTP authorization endpoint before login-state insertion or redirect; `AllowInternal` local login still succeeds. | Extend the existing tenant-login/probe coverage at the owning auth boundary and run its exact `mise exec -- cargo nextest run --locked` selector. |
| `FIND-TASK-001-21` | `bounded_get` has accurate `# Errors` documentation with no behavior change. | Direct source inspection, `mise run fmt`, and `mise run lints`. |
| `FIND-TASK-001-22` | A distinct recovery principal produces a second attributed Allowed decision; a valid underprivileged recovery principal produces Denied; injected append failure commits neither activation nor retirement. | Extend the existing activation journey or focused Postgres-backed activation tests and run every named test with its exact repository-managed selector. |
| `FIND-TASK-001-23` | A held DNS resolution reaches the fixed deadline and returns `Unresolved`; existing screening/pinning/proxy/local-provider behavior remains green. | Add one deterministic Tokio-time check in the existing screening test module; do not stall or perturb the host resolver. |

After focused proof, run the original TASK-001 and prior-remediation coverage:
the filtered and unfiltered identity journeys, `test:principals:integration`,
`test:sql`, `test:platform:journey`, `codegen:check`,
`check:tenant-isolation`, `check:from-pools-allowlist`, applicable client,
PyO3, unwrap, and Clippy-allow boundaries, `docs:check`, `fmt`, `lints`, and
`git diff --check`. Keep identity journeys on their intended minimal features
and prove every named selector selects exactly one test.

## Implementation evidence

| Acceptance criterion | Implementation evidence | Verification evidence | Result |
|---|---|---|---|
| `FIND-TASK-001-20` BlockInternal refuses a freshly discovered HTTP authorization endpoint before login-state insertion; AllowInternal local login still succeeds | `crates/wyrd/wyrd-auth/src/login.rs` `browser_authorization_endpoint` (called in `begin_login` before state is built or written) over `ScreenedHttp::screen_scheme` (`crates/shared/wyrd-auth-oidc/src/screening.rs`, also used by `client_for`) | `login::destination_tests::production_refuses_a_discovered_cleartext_authorization_endpoint`, `login::destination_tests::a_permissive_deployment_keeps_a_local_http_authorization_endpoint`; local HTTP login through the unfiltered identity journeys | PASS |
| `FIND-TASK-001-21` `bounded_get` documents `# Errors` with no behavior change | `screening.rs` `tests::bounded_get` rustdoc | source inspection, `mise run fmt`, `mise run lints` | PASS |
| `FIND-TASK-001-22` distinct recovery principal → attributed Allowed; underprivileged → Denied; injected append failure commits neither activation nor retirement | `crates/wyrd/wyrd-auth/src/connections.rs` `recovery_key_authorizes` appends the recovery principal's decision (principal, kind, card, verified `credential_id`, `identity_connections:write`) on the locked activation transaction before promotion or committed refusal; unresolved keys append nothing | `tenant_connection_rotation_journey` steps 3, 4, 7, 8: malformed key has no row; runtime-admin key gives a Denied row with its key id; the distinct recovery admin gives two Allowed rows with its key id; a trigger failing only the recovery row returns 5xx and leaves the Active id/revision and the Candidate revision unchanged | PASS |
| `FIND-TASK-001-23` held DNS resolution hits the fixed deadline as `Unresolved`; screening, pinning, proxy, and local-provider behavior unchanged | `screening.rs` `bounded_lookup` wraps the single `lookup_host` in `FETCH_TIMEOUT`, returning the resolver's address set unchanged | `screening::tests::a_stalled_lookup_is_unresolved_at_the_deadline` (paused Tokio time, pending lookup, host resolver untouched); full `wyrd-auth-oidc` lib suite 38/38 | PASS |

Reuse findings (human-accepted, each verified against the code first):
1. `fixtures-no-server.sh` and `no-tonic-outside-wyrd-tonic.sh` now use a `forbid` helper that fails on a match or an `rg` error, and no longer scan the missing `python/` path. The manifest glob `!Cargo.toml` matched every crate manifest, so it is now scoped to the generated `workspace-hack` manifest. The repaired check surfaced a real violation: a direct `prost` dependency in `vala-bifrost-redux`. That crate now reaches prost through `wyrd_tonic::prost`, and its derives use `#[prost(prost_path = "::wyrd_tonic::prost")]`.
2. `pg_migration.rs` case (d) is reduced to: seed user and machine refresh rows, migrate, assert they are live and unbound, and check that `consume_active_refresh` returns `(None, None)`. Renewal under a bound connection is covered by `refresh::pg_tests::rotation_copies_the_connection_binding` and `rotation_refuses_an_inactive_or_unbound_connection`.
3. `read_bounded_body`: the redundant `Content-Length` pre-check is deleted. `an_oversized_declared_body_is_refused` still passes through the chunk cap.
4. `callback.rs` discovery refusals now use `provider_unreachable`.
5. The root `BodyError` re-export is dropped. The type stays nameable at `wyrd_auth_oidc::screening::BodyError`.
6. `load_signing_key` reads the file through `read_secret_file`, and `ConfigError::ReadSigningKey` now carries the static reason. Covered by `config::tests::the_signing_key_file_requires_a_restrictive_regular_bounded_file`; the docs are updated in `self-hosting/authentication.svx`.
7. The `boot::auth` test `issuer_resolver()` uses a lazy pool that never connects. It is added to the `from-pools` allowlist's documented in-src test sites. Both assembler tests pass without Postgres.

Verification (all exit 0 in this session): `mise run fmt`, `mise run lints`, `git diff --check`, `mise run codegen:check`, `mise run docs:check`, `mise run test:principals:integration`, `mise run test:sql`, `mise run test:platform:journey`, `WYRD_IDENTITY_FILTER=tenant_connection_rotation_journey mise run test:identity:journey`, `mise run test:identity:journey` (23/23, with the exactly-one checks for the named journeys), `check:tenant-isolation`, `check:from-pools-allowlist`, `check:fixtures-no-server`, `check:no-tonic-outside-wyrd-tonic`, `check:client-tier`, `check:pyo3-scope`, `check:unwrap-audit`, `check:clippy-allow-audit`. The Postgres-wrapped `wyrd-auth` lib suite passed 119/119 and the `vala-bifrost-redux` codec/peer/signal unit tests passed 14/14. Each named test was run with its exact `mise exec -- cargo nextest run --locked` selector, and each selected exactly one test.

Non-goals stayed excluded: no new public contract, configuration, dependency, resolver trait, audit path, or route. Limit: `scripts/checks/no-legacy-server-vocab.sh` has the same non-final `! rg` defect. It is outside the accepted findings, and repairing it would flag legitimate OTLP `Scope` identifiers, so retiring or rescoping it needs a separate decision.
