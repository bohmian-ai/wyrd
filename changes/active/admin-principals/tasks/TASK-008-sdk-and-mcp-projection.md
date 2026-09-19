---
task: TASK-008
title: CLI and MCP projection of the administrative contract
spec: SPEC-admin-principals
spec_revision: 7
obligations: [REQ-036, REQ-040, REQ-047, INV-013, INV-014, INV-015, AC-013, AC-014]
depends_on: [TASK-004, TASK-005, TASK-006, TASK-007]
status: implemented
---

## Current Status

This task is **partially delivered**. A fresh implementor resumes here and needs
no prior context beyond this file, the approved spec, and the repository
authorities linked at the bottom.

### Landed

| Outcome | Evidence |
|---|---|
| `wyrd_client::Principals` — tenant principal and credential administration | `4225069` |
| `wyrd_client::Platform` / `PlatformSession` — platform control plane | `7ecd356` |
| Rust SDK admin journeys | **deleted** — never run by any lane, unearned |
| MCP credential observation and revocation, write tools scope-gated | `128eb40` |
| Administrative paths published in the generated contract | `384e167` |

### Remaining

Five items, all consolidation. None adds a capability; each removes a duplicate
implementation the earlier stages accumulated, or publishes a contract fact that
was omitted. Spec revision 7 was approved for exactly this closeout.

| # | Item | Obligation | Proof |
|---|---|---|---|
| 1 | Platform plane moves to `X-Wyrd-Access-Token` and reuses the shared pipeline | `INV-015` | TDD — Scenario 1, 2 |
| 2 | Signing-key resolution exists once | `REQ-036` | Refactor under Scenario 1 |
| 3 | Every `wyrd-cli` caller uses `wyrd-client` | `REQ-047` | TDD — Scenario 3 |
| 4 | Generated contract declares its authentication scheme | `REQ-036`, `AC-014` | Generated-artifact proof |
| 5 | Platform revocation rustdoc states the real invariant | `INV-013` | Documentation proof |

### Scope removed at revision 7

The administrative client surfaces this change builds and proves are the **CLI
and MCP**. Administration is an operator and agent act: the operator reaches it
through the CLI, the agent through MCP.

**No Python or TypeScript administrative binding is added.** Both packages are
workload-runtime surfaces — they exist for workloads already provisioned — while
credential creation is a provisioning-time act. The Python package already
reaches every administrative operation through `wyrd.cli.run_wyrd_cli`, the
in-process CLI it ships. Neither package contains an administrative binding
today; none is to be created.

**The Rust SDK is untouched.** It re-exports `wyrd-client` wholesale and that
re-export stays; the CLI may need it. What is deleted is
`sdks/wyrd-sdk-rust/tests/principals.rs` and `tests/platform.rs`: both were
`#[ignore]`-gated and no `mise` lane ran them — `test:wyrd-sdk` is `--lib` only
— so journeys that were supposed to discharge `AC-014` had never executed.
Unrun tests do not earn their place.

`wyrd-client` **keeps** `Principals` and `Platform`. `REQ-047` requires every
Wyrd-owned caller to go through the shared client, and the CLI is that caller.
Scenario 3 is what gives them a real consumer.

## Outcome and Value

The administrative contract is reachable from the CLI and the agent-facing MCP
surface, both speaking one identity model on one authentication header through
one client implementation. An independent client can implement
the administrative surface from the generated contract alone.

Maps `REQ-036`, `REQ-040`, `REQ-047`, `INV-013`, `INV-014`, `INV-015`,
`AC-013`, `AC-014`.

## Owners, Scope, Consumers, and Prohibited Changes

**Owners.** `crates/shared/wyrd-client` is the sole client implementation.
`crates/wyrd/wyrd-server/src/components/auth` owns extraction.
`crates/shared/wyrd-auth-verify` owns verification. `crates/wyrd/wyrd-cli` and
`crates/wyrd/wyrd-mcp` are the administrative consumers and own no transport.

**Invariants.**

- `INV-015`: `X-Wyrd-Access-Token` is the one authentication header on every
  plane. The caller's own `Authorization` header belongs to the calling
  application and is never read. `architecture/wyrd-design.md` lines 495-507 are
  the authority.
- Plane separation is carried by `verify_platform`'s `PLATFORM_TOKEN_SCOPE`
  check and by the extractor type a route declares — never by header choice,
  which any client can set. Do not restate the header rationale in code or docs.
- `INV-013`: revocation takes effect no later than the next request, by the
  mechanism each plane's own caching demands.
- Credential plaintext crosses a client surface exactly once, in the response
  that created it, and is never persisted by a client.
- MCP read tools are always available; administrative writes require explicit
  scopes.

**Prohibited changes.**

- Do not add a Python or TypeScript administrative binding: no PyO3 wrapper, no
  napi binding, no `.pyi` or `.d.ts` administrative surface. Do not reinstate
  the deleted Rust SDK administrative journeys. The Rust SDK's existing
  `wyrd-client` re-export is not in scope and must not be narrowed here.
- Do not route the platform plane through `AuthenticatedPrincipal`. It wraps
  `Arc<VerifiedToken>`, which is tenant-shaped — non-optional `tenant_id`, roles,
  delegation chain. Threading the platform plane through it would require a
  nullable tenant across `VerifiedToken`, `Principal`, and every downstream
  query, which is the shape that makes `REQ-046`/`AC-017` hard to prove. Keep
  two extractors and two principal stores; share only the plumbing beneath them.
- Do not add a revocation epoch to the platform plane. See **Material Stop
  Conditions**.
- Do not weaken the platform plane's indistinguishable rejection.
- Do not hand-edit generated stubs, schemas, or `openapi.yaml`.
- Do not add a compatibility route, alias, or transitional flag.

**Non-goals.** A UI. Exposing platform-plane operations to tenant-scope clients.
Widening beyond the callers named in the write set.

## Approach

1. Extract one private signing-key resolution on `TokenVerifier` and call it
   from both internal verify paths.
2. Move `PlatformCaller` onto the canonical header and the shared `RequestId`
   extraction, preserving its indistinguishable rejection.
3. Move every `wyrd-cli` command that calls a Wyrd route onto `wyrd-client`,
   deleting the per-command transport and error mapping it replaces.
4. Declare the authentication scheme in the generated contract by changing the
   generator, then regenerate.
5. Correct the platform revocation rustdoc to state the caches-nothing
   invariant.
6. Prove the administrative operations through the CLI and MCP lanes.

## Ordered Implementation Scenarios

### Scenario 1 — A platform session authenticates on the canonical header

**Behavior.** A platform session token presented on `X-Wyrd-Access-Token`
authenticates and yields a `PlatformCaller`. The same token presented on
`Authorization` does not. Maps `INV-015`, `REQ-036`.

**RED.** Extend the existing header unit tests in
`crates/wyrd/wyrd-server/src/components/auth/platform_extractor.rs` so that
extraction is asserted against `x-wyrd-access-token`. The current
`malformed_headers_yield_no_token` and its sibling assert `authorization`;
re-pointed at the canonical header they fail, because line 31 still binds
`HeaderName::from_static("authorization")`. Confirm the exact selector with
`mise exec -- cargo nextest list -p wyrd-server` before running:

```bash
mise exec -- cargo nextest run --locked -p wyrd-server --lib \
  -E 'test(=components::auth::platform_extractor::tests::malformed_headers_yield_no_token)'
```

**GREEN.** Bind the extractor to the canonical header constant already owned by
`components::auth::token_extract`. Keep the indistinguishable rejection rather
than adopting `extract_wyrd_access_token`'s informative one: a legitimate client
never depends on telling "missing" from "malformed" apart, and the
indistinguishable form is the stronger property. Re-run the platform journey,
which exercises the real header path end to end:

```bash
mise run test:platform:journey
```

**REFACTOR.** While green: collapse the duplicated signing-key preamble —
`decode_header` → `kid` → `Kid::new` → `decoding_keys.get` → `Arc::clone`, which
appears verbatim at `crates/shared/wyrd-auth-verify/src/lib.rs:411-420` and
`488-497` — into one private method on `TokenVerifier`, called by both.
Leave `verify_external_against` alone; it resolves against external JWKS and is
genuinely different. Also collapse the duplicated `RequestId` extraction shared
with `caller_extractor.rs`. Re-run:

```bash
mise exec -- cargo nextest run --locked -p wyrd-auth-verify --lib
mise run test:platform:journey
```

### Scenario 2 — An application's own `Authorization` header is never read

**Behavior.** A request carrying an unrelated `Authorization: Bearer <app token>`
alongside a valid `X-Wyrd-Access-Token` authenticates on the Wyrd token and
leaves the application's header untouched; a request carrying *only* an
`Authorization` header is refused with the stable unauthenticated error. Maps
`INV-015`, `AC-013`.

**RED.** Add the both-headers and Authorization-only cases to the platform
extractor's unit tests. Both fail today: the extractor reads `authorization`, so
the first authenticates on the wrong header and the second wrongly succeeds.

```bash
mise exec -- cargo nextest run --locked -p wyrd-server --lib \
  -E 'test(/components::auth::platform_extractor::tests::/)'
```

**GREEN.** Satisfied by Scenario 1's binding; this scenario exists to pin the
property so it cannot silently regress. No further production change is expected
— if one is needed, Scenario 1 was incomplete.

**REFACTOR.** Rewrite the module doc on `platform_extractor.rs` and on
`crates/shared/wyrd-client/src/platform/mod.rs`. Both currently assert that the
distinct header is what keeps the planes from being confused. That reasoning is
false — any client can set any header — and it is what licensed the drift. State
the real mechanism: the scope marker and the extractor type.

### Scenario 3 — A CLI command authenticates against a real server

**Behavior.** `wyrd principal revoke` authenticates and revokes against a real
server. It currently cannot: it sends only `Authorization: Bearer`
(`crates/wyrd/wyrd-cli/src/principal/revoke.rs:60-63`) while the route it calls
takes the tenant `Caller` extractor, which reads `X-Wyrd-Access-Token` only. The
same defect affects `auth issue-key`, `auth trusted-issuer`, and
`auth workload-binding`. Maps `REQ-047`, `INV-015`.

**RED.** Add a real-server CLI test that runs the revoke command against a
provisioned tenant and asserts the principal's tokens stop working. It fails on
the unauthenticated refusal, not on the revocation. Follow the existing
real-server CLI journey shape (`mise run test:cli:journey`) and register the new
test in that lane. This test is the deliverable's centre: the defect existed
precisely because the CLI's transport path had no test.

**GREEN.** Construct a `WyrdClient` from the command's `--server` and `--token`
arguments as a `ResolvedCredential::BearerToken`, which `wyrd-client` already
passes through as-is, and issue the call through it. This routes the command
onto the canonical header and the shared `WyrdError` mapping in one move. Re-run
Scenario 1 and 2, then:

```bash
mise run test:cli:journey
```

**REFACTOR.** Extend the same move to every remaining `wyrd-cli` caller and
delete what it replaces — the per-command `reqwest` clients and the per-command
error variants that become unreachable, `WyrdCliError::RevokeFailed` among them.
`auth login` and `auth refresh` fold onto the `/auth/token` exchange
`crates/shared/wyrd-client/src/auth.rs` already owns rather than re-posting it.
For `eval/server.rs` and `eval/agent.rs`, first ask whether each command earns
its place at all; if it does and the shared client lacks a streaming turn loop,
add that capability narrowly to `wyrd-client`. Do not leave a hand-rolled client
behind. Verify with:

```bash
mise run check:client-tier
mise exec -- cargo clippy --locked -p wyrd-cli -p wyrd-client --all-targets
```

## Proof Strategy For The Non-Behavioral Remainder

Two remaining items change no executable behavior and require no manufactured
RED.

**Generated contract (item 4).** `openapi.yaml` publishes 24 paths, 8 of them
platform or auth routes, and declares no `securitySchemes` at all. A
language-agnostic contract that does not say which header carries the credential
cannot be implemented from the artifact, which is the premise of `REQ-036` and
the whole of the rewritten `AC-014`. Add one `securitySchemes` entry for
`X-Wyrd-Access-Token` (`type: apiKey`, `in: header`) and attach it to every
authenticated path, **by changing the generator**. Proof is generated-artifact
drift: `mise run codegen:check` regenerates cleanly and the committed contract
carries the scheme.

**Revocation rustdoc (item 5).** No mechanism changes. Proof is documentation
review against `INV-013` at revision 7 plus the unchanged platform journey.

## Acceptance Criteria

- No Wyrd-owned surface reads the `Authorization` header; a tree-wide search
  returns nothing.
- A platform session token authenticates on `X-Wyrd-Access-Token`; a tenant
  access token presented to a platform route is still refused with the stable
  contract error, proving the scope marker and not the header separates them.
- An application's own `Authorization` header is carried through untouched and
  never consulted.
- Signing-key resolution exists once and both internal verify paths call it.
- No `reqwest::Client` is constructed anywhere in `crates/wyrd/wyrd-cli/src`.
- Every CLI command that calls a Wyrd route authenticates on the canonical
  header and is proven to do so by a test that would have caught the original
  defect.
- `eval/*` is moved onto the shared client or removed, with the reason recorded;
  it is not left hand-rolled.
- Unreachable per-command CLI error variants are deleted, not left in place.
- The generated contract declares the authentication scheme and regenerates
  cleanly with no hand edits.
- The CLI performs the administrative operations against a real server,
  including receiving a once-returned credential and using it on a subsequent
  call, through a lane that actually runs.
- A tenant-scope caller cannot invoke a platform-plane operation through the CLI
  or an MCP tool; the refusal is the stable contract error.
- No Python or TypeScript administrative binding exists, and no unrun
  administrative journey remains in the tree.
- MCP administrative write tools are unavailable without their explicit scope
  and available with it; read tools remain available.

- Platform revocation behavior is unchanged and its rustdoc states the
  caches-nothing invariant rather than the stale no-tokens premise.
- No document, example, or surface still describes a second identity model, a
  removed bootstrap path, or credential-keyed authorization.

## Expected Write Set and Consumer Closure

Paths are guidance, not an allowlist.

**Production.** `crates/wyrd/wyrd-server/src/components/auth/platform_extractor.rs`;
`crates/shared/wyrd-auth-verify/src/lib.rs`;
`crates/shared/wyrd-client/src/platform/{handle.rs,mod.rs}`;
`crates/wyrd/wyrd-cli/src/{principal,auth,eval}/`;
`crates/wyrd/wyrd-auth/src/platform_sessions.rs` (rustdoc only).

**Consumers.** `crates/wyrd/wyrd-server/src/components/auth/caller_extractor.rs`
(shared `RequestId` extraction); any route or MCP handler taking `PlatformCaller`.

**Contract and generated.** The OpenAPI generator source; `openapi.yaml`.

**Test.** `crates/wyrd/wyrd-server/src/components/auth/platform_extractor.rs`
unit tests; a real-server CLI test in the `test:cli:journey` lane;
`mise.toml`.

## Verification and Evidence

Scope is `VER-001` through `VER-006`. `VER-001` at revision 7 places the
pre-existing CLI authentication and admin commands in scope.

```bash
mise run fmt
mise exec -- cargo clippy --locked -p wyrd-server -p wyrd-auth-verify \
  -p wyrd-client -p wyrd-cli -p wyrd-mcp --all-targets
mise run check:client-tier
mise run check:sdk-client-tier
mise run codegen:check
mise run docs:check
mise run test:platform:journey
mise run test:cli:journey
```

Run the exact focused expression for every named test, confirming its selector
with `mise exec -- cargo nextest list` against the owning package before relying
on it. Journeys run under the repository-managed Postgres wrapper. No Python or
TypeScript lane is in scope.

## Material Stop Conditions

**Do not add a revocation epoch to the platform plane.** `INV-013` requires
revocation to take effect no later than the next request. The platform plane
already satisfies this by a simpler mechanism than the tenant plane:
`confirm_credential_session` re-reads the credential row and checks
`is_usable(now)`; the federated path re-reads the principal and checks
`is_active()`; `resolve_grant` re-reads the grant per request; and
`verify_platform` documents that results are not cached. The tenant epoch exists
*because* the tenant path caches verified tokens. This plane caches nothing, so
there is no cached state that can outlive a revocation, and an epoch here would
be new machinery solving a problem the plane does not have.

What is wrong is the recorded reasoning, not the mechanism. Commit `1bd18354`
justified this with "the platform plane issues no access tokens" — true when
written, made false by `0313a78d`, which introduced platform session tokens. The
conclusion survived; the premise did not. Correct the rustdoc; change no
behavior.

Stop and escalate to planning or specification authority if: routing the CLI
through `wyrd-client` would require a durable client-side capability rather than
a transport one; `eval/*` proves to need a streaming capability whose shape is
not a local decision; or publishing the authentication scheme surfaces a route
whose real auth requirement contradicts `INV-015`.

## Authority Links

- Approved spec: `changes/active/admin-principals/spec.md`, revision 7
- `AGENTS.md`
- `architecture/agent-rules.md`
- `architecture/wyrd-design.md` (lines 495-507 for the canonical header)
- `architecture/references/languages/spec-driven-development.md`
- `architecture/references/languages/implementation-execution.md`
- `architecture/references/languages/testing-workflows.md`

## Implementation Evidence (closeout, revision 7)

All five remaining items are delivered. Commits `289978fcc` (items 1-2),
`3a71e0409` (item 3), `40daeee97` + `074a40e87` (item 4), `289978fcc` +
`21abea8c7` (item 5 and the catalog prose it exposed), `73aaa0616` (Scenario 3
journey), `343cda711`, `fac7888dc`.

| Acceptance criterion | Implementation evidence | Verification evidence | Result |
|---|---|---|---|
| No Wyrd-owned surface reads `Authorization` | `platform_extractor.rs` binds `token_extract::WYRD_ACCESS_TOKEN_HEADER`; tree-wide search leaves only skald provider outbound calls, vala webhook redaction, and tests asserting the header is *not* read | `grep -rn "AUTHORIZATION\|\"authorization\"" crates/`; `mise run lints` | PASS |
| Platform session authenticates on `X-Wyrd-Access-Token`; a tenant token on a platform route is still refused by scope | `platform_extractor.rs`, `verify_platform`'s `PLATFORM_TOKEN_SCOPE` check unchanged | `platform_admin_e2e::the_two_control_planes_cannot_reach_each_other` (17/17 PASS) | PASS |
| An application's own `Authorization` is carried through untouched | `platform_extractor.rs` unit tests for the both-headers and Authorization-only cases | `cargo nextest run -p wyrd-server --lib -E 'test(/components::auth::platform_extractor::tests::/)'` | PASS |
| Signing-key resolution exists once | one private `TokenVerifier` resolver in `crates/shared/wyrd-auth-verify/src/lib.rs`, called by both internal verify paths | `cargo nextest run -p wyrd-auth-verify --lib` | PASS |
| No `reqwest::Client` in `wyrd-cli/src` | single construction point `crates/wyrd/wyrd-cli/src/client.rs` over `wyrd-client` | `grep -rn "reqwest::Client" crates/wyrd/wyrd-cli/src/` → no matches; `mise run check:client-tier` | PASS |
| Every CLI command authenticates on the canonical header, proven by a test that would have caught the defect | `client.rs` + `card.rs`, `query/mod.rs`, `principal/`, `auth/`, `eval/` all on `wyrd-client` | `crates/wyrd/wyrd-cli/tests/principal_journey.rs` in `mise run test:cli:journey` (22 passed) | PASS |
| `eval/*` moved onto the shared client, reason recorded | protocol calls go through the new `wyrd_client::eval::{EvalProtocol, EvalRun}`, which owns the run lease on `x-wyrd-eval-lease`; `eval/agent.rs` targets a third-party endpoint so it carries no Wyrd credential and uses `WyrdClient::request_external_stream` rather than a client of its own | `test:cli:journey::eval_server_protocol::server_protocol_carries_lease_after_open` | PASS |
| Unreachable per-command CLI error variants deleted | `HttpBuild`, `Http`, `UrlJoin`, `RevokeFailed` removed from `crates/wyrd/wyrd-cli/src/error.rs`; `AgentTurnFailed` added for the one remaining external call | `mise run lints` (dead-code and unused-import clean) | PASS |
| Generated contract declares the authentication scheme, no hand edits | `SecurityAddon` modifier in `crates/wyrd/wyrd-server/src/http/openapi.rs`; regenerated `openapi.yaml` | `mise run codegen:check`; `openapi::tests::every_authenticated_path_declares_the_one_wyrd_scheme` | PASS |
| CLI performs administrative operations against a real server, including a once-returned credential used on a later call | `principal_journey.rs` (revoke) and the pre-existing `auth` journeys through the same client | `mise run test:cli:journey` | PASS |
| A tenant-scope caller cannot invoke a platform-plane operation via CLI or MCP | structural: MCP exposes no platform tool and the CLI has no platform command; the HTTP side is proven | `platform_admin_e2e::the_two_control_planes_cannot_reach_each_other`; `principal_revoke_cli_journey_refuses_an_unprivileged_caller` asserts `WYRD_PERMISSION_403_DENIED_RBAC` | PASS |
| No Python or TypeScript administrative binding; no unrun administrative journey | nothing added under `sdks/`; the two `#[ignore]`d Rust SDK journeys stay deleted | `mise run check:sdk-client-tier`; `mise run codegen:check` (no stub drift) | PASS |
| MCP administrative writes scope-gated, reads always available | unchanged from `128eb40` | `mise run lints`; MCP tool-catalog tests unchanged | PASS |
| Platform revocation rustdoc states the caches-nothing invariant | `crates/wyrd/wyrd-auth/src/platform_credentials.rs`, `revocation_takes_effect_on_the_next_request` | `platform_admin_e2e::revoking_a_platform_credential_ends_its_live_sessions` (behavior unchanged) | PASS |
| No surface still describes a second identity model or the wrong header | error catalog remediations for `WYRD_AUTH_401_UNAUTHENTICATED`, `WYRD_AUTH_400_BAD_TOKEN_FORMAT`, `WYRD_PERMISSION_401_UNAUTHENTICATED` now name `X-Wyrd-Access-Token`; the eval docs row names `x-wyrd-eval-lease` | `mise run codegen:check`; `mise run docs:check` | PASS |

### Commands run

```bash
mise run fmt
mise run lints
mise run check:client-tier
mise run check:sdk-client-tier
mise run check:unwrap-audit
mise run codegen:check
mise run docs:check
WYRD_CLI_E2E=1 mise run test:cli:journey
WYRD_AUTH_E2E=1 scripts/postgres/with-test-postgres.sh -- bash -lc \
  'mise run db:migrate:all:inner && cargo nextest run --locked -p wyrd-server --test platform_admin_e2e'
git diff --check
```

### Stale task text corrected while implementing

- Item 5's rustdoc lives in `crates/wyrd/wyrd-auth/src/platform_credentials.rs`,
  not `platform_sessions.rs` as the write set says.
- `auth trusted-issuer` and `auth workload-binding` were already on the canonical
  header; only `principal revoke` and `auth issue-key` carried the defect the
  task describes.
- `mise run test:platform:journey` is broken independently of this task: it
  depends on a `setup:postgres` task that no longer exists. The platform suite
  was run through `scripts/postgres/with-test-postgres.sh` instead, which is what
  that lane wraps. Repairing the lane is not in this write set.

### Deliberately out of scope

- `POST /v1/principals/{principal_id}/revoke` ignores the
  `RevokePrincipalRequest` body that the contract declares and the CLI sends;
  existing server tests post an empty body. Requiring the body is a public
  contract change and belongs to its own task.
- Every non-goal held: no UI, no platform operation exposed to tenant-scope
  clients, no widening beyond the named callers. No Python or TypeScript file
  changed.

### Incidental repair

`fac7888dc` fixes `crates/shared/wyrd-auth-issue/src/lib.rs` unit tests that an
earlier commit on this branch (`3699b3c37`) left calling the pre-`Option<Uuid>`
`issue_platform_access_token` signature. They did not compile, so `mise run
lints` could not pass for any change on this branch.
