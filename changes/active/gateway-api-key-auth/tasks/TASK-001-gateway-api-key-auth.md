---
id: TASK-001
kind: implementation
status: proposed
spec: SPEC-gateway-api-key-auth
spec_revision: 1
requirements: [REQ-001, REQ-002, INV-001, INV-002, INV-003, AC-001, AC-002, AC-003, AC-004]
depends_on: []
---

# TASK-001 Exchange a presented API key on gateway ingress

Implementation skill: `$wyrd-implement`.

User-confirmed scope: gateway API-key fallback only. OTLP is outside this
task: do not change its implementation, authentication behavior, or tests.
REQ-001's “unchanged” clause preserves existing non-gateway behavior.

## Outcome and Value

A caller with only a server URL and static Wyrd API key can use the existing
OpenAI-compatible, Anthropic Messages, and Gemini GenerateContent gateway
routes. The gateway exchanges the key through the existing API-key grant,
verifies the minted token through the normal Wyrd verifier, and invokes with
that principal's permissions. Access tokens remain the preferred path.

This task owns the complete gateway fallback: ingress, process-local token
reuse, protocol refusals, tenant and secret isolation, stock-client journeys,
and documentation. It maps every obligation in spec r1 within the user-confirmed
gateway scope. No other
implementation task is required. Ambient-auth behavior already exists in this
checkout (`WyrdClient.http_auth` / `httpAuth` and the stock-client guide); there
is no implementation dependency on another change packet or merge order.

## Owners, Scope, Consumers, and Prohibited Changes

- `wyrd-server` owns gateway authentication, its process-local state, and reuse
  of the auth component's grant orchestration. `wyrd-auth` remains the owner of
  credential verification and token issuance; `wyrd-sql` remains the durable
  tenant-scoped owner.
- Classify the extracted credential, after the existing exactly-one-header
  check: a value starting with `wyrd_sk_` takes the key exchange path; every
  other value takes the existing access-token verification path. Retain each
  route's accepted headers, Bearer syntax, query restrictions, request IDs, and
  protocol error envelope. A failed key exchange must not fall through to JWT
  verification.
- Reuse the complete grant behind `/auth/token`, including tenant selection,
  transaction lifecycle, error mapping, and non-blocking audit staging. A
  principal-only helper that skips token minting and exchange audit is not a
  substitute for the required exchange.
- Reuse successful exchanged tokens across requests within one server process,
  keyed only by the SHA-256 hash of the complete key. Entries expire 60 seconds
  after admission to the cache; reads must not renew that deadline. Never use
  an expired JWT, cache a failure, retain a raw key as a cache key, or reuse one
  key's authority for another key. Bound retained memory using existing
  dependency and cache patterns where applicable. Capacity and private
  synchronization are implementation decisions; they cannot extend the
  revocation window or block unrelated keys behind network IO.
- A cache hit avoids exchange audit only. Existing gateway authorization and
  its audit still execute on every request. Keep all audit writes on the shared
  Scribe outbox and non-blocking.
- The key must not reach provider headers, gateway call rows, trace/log fields,
  error bodies, or successful responses. Use existing secret wrappers and
  redaction conventions for all retained credentials, including cached tokens.
- Restrict the new fallback to the existing public gateway ingress router.
  Management `/v1` routes, `/mcp`, and Bifrost must not gain it. `/v1` here means
  management routes, not the explicitly included `/v1` inference routes.
- No new public credential header, query credential, grant, SDK authentication
  API, runtime configuration knob, distributed cache, persistence layer,
  migration, provider adapter, route alias, dependency, or Cargo feature. Any
  test timing seam must stay behind existing test-support boundaries; the task
  does not require one or require changing the production TTL to run tests.

### Inspected reuse map

Paths and symbols identify existing owners, not an implementation allowlist.

| Capability | Existing owner/symbol | Inspected callers/tests | Missing behavior | Selected extension | New machinery justification |
|---|---|---|---|---|---|
| Credential extraction and dialect refusals | `components/auth/token_extract.rs::extract_gateway_access_token`, `verify_access_token`; `components/gateway/ingress.rs::GatewayIngress::{authenticate,error}` | `gateway_ingress_router` mounts all three dialects; token-extraction unit test; Rust native ingress journeys | Extracted values are always treated as JWTs | Extend the shared gateway boundary after extraction; retain the shared verifier and envelope rendering | No second extractor, parser, verifier, or per-route auth pipeline |
| Key exchange, tenant selection, audit, and principal | `components/auth/routes.rs::TokenGrants::{api_key,api_key_exchange}`; `wyrd-auth/src/exchange_api_key.rs::ExchangeApiKey::execute` | `/auth/token` dispatch; `verify_access_token` constructs `AuthenticatedPrincipal` | Gateway cannot invoke the existing minting workflow | Expose the smallest internal capability of the existing grant owner and verify its minted token | No second exchange, HTTP self-call, duplicated SQL, or alternate audit path |
| Bounded process-local token reuse | `AppState` owns shared server handles; `verification/cache.rs::VerifierCache::{get,insert}` uses installed `lru`; server already imports `sha2::Sha256` in gateway capture/routes | `GatewayIngress::layer` clones `AppState`; `VerifierCache` has tenant-isolation and eviction tests; `wyrd-client/src/auth.rs::AuthMiddleware::bearer` is a client credential cache | No server gateway cache for many presented keys with a fixed 60-second age | Keep cache ownership on server gateway state and reuse installed `lru` / `sha2` and the existing bounded-cache conventions where applicable | A gateway credential cache is explicitly required. VerifierCache stores immutable Cards, not expiring secrets; AuthMiddleware is a client-side single-credential transport with disk-cache behavior. Neither should be repurposed into a server credential owner. No generic cache framework or new cache dependency |
| Real HTTP security and lifecycle proof | `wyrd-testing/tests/gateway/harness.rs::Journey`, `WyrdTestServer`; `tests/gateway/main.rs` registers `native` | Rust native ingress journeys use real server, mock upstream, telemetry capture, tenant authorization, and ledger assertions | No direct-key, cached revocation, exchange-audit, or cross-key journey | Extend the existing gateway journey target and retained evidence paths | No alternate server fixture or in-process authentication substitute |
| Stock Python client closure | `tests/integration/gateway/test_gateway_openai_client.py::openai_client`; `test_gateway_native_clients.py::{anthropic_client,gemini_client}` | Existing official-client success/stream and native refusal journeys; `gateway_server` in integration `conftest.py` | Clients currently supply access tokens, or use `http_auth` | Add direct-key cases using the same local-provider deployments and real server | No Wyrd client auth helper on the invocation path; fixture setup may use Wyrd APIs |
| Stock TypeScript client closure | `tests/integration/gateway-inference.test.ts`; `tests/support/server.ts::serverTest` | Existing OpenAI and Anthropic `httpAuth` journeys and provider-header assertions | No official OpenAI client invocation using a static Wyrd key alone | Extend the existing file using `server.apiKey` / deployment environment directly | No SDK export, N-API change, or new fixture required |
| Preferred path and fallback documentation | `docs/src/content/docs/connect/call-wyrd-from-a-stock-client.md`, section “Without a transport hook”; `architecture/wyrd-design.md`, “Cross-service delegation” | Existing Python/TypeScript transport-hook examples and access-token helper guidance | No static-key fallback or gateway exception | Append fallback after access-token guidance; narrowly qualify the delegation prose | Documentation only; no new guide or compatibility surface |

## Approach

1. Revalidate the reuse map against the implementation checkout, preserving
   the approved gateway-only scope.
2. Pin credential classification and existing header restrictions with focused
   tests; connect gateway misses to the existing token grant and verifier.
3. Add bounded, shared process-local token reuse using installed dependencies
   and existing cache patterns; prove the fixed expiry and per-key isolation.
4. Drive the real-server gateway path through success, denial, malformed input,
   tenant separation, cache reuse, and revocation. Inspect audit, capture,
   telemetry, and upstream evidence.
5. Add the stock Python and TypeScript client journeys with URL and key alone;
   retain access-token and transport-hook regression proof.
6. Update the guide and architecture exception, then record focused TDD and
   final verification evidence against each acceptance criterion.

## Ordered Implementation Scenarios

Commands U1–T1 and R1–R3 are defined under Verification and Evidence. Names
marked planned are tests to add in the inspected existing targets; after adding
one, confirm its exact collected name before recording RED. Test structure,
private helpers, and fixture design remain implementation-owned.

### Scenario 1 — Classify credentials without widening extraction

**Behavior.** Following the existing header extraction, only `wyrd_sk_` values
select the API-key path, including malformed values with that prefix. Other
values retain JWT verification. Repeated/mixed credential headers, wrong-route
native headers, missing/empty credentials, and URL credentials retain existing
refusals before exchange (REQ-001, INV-002, AC-002, AC-003).

**RED.** Add planned U1 and run its exact command. Expect failure because the
gateway has no API-key branch; a compile/setup failure is not RED. Run R1 as
existing extraction regression proof; do not manufacture a failure there.

**GREEN.** Connect the extracted key to the existing minting grant and verify
the result with the same Wyrd-audience verifier as a supplied access token.
Preserve exchange error identity and render it through the ingress dialect.
Run U1 and R1.

**REFACTOR.** Keep one gateway classification boundary and one grant workflow;
remove any duplicate extraction, tenant parsing, or error translation exposed
by the green tests. Rerun U1 and R1.

### Scenario 2 — Reuse exchanged tokens for a fixed, bounded interval

**Behavior.** Successful results reuse authority only for the identical key
hash, until the fixed 60-second deadline or token expiry. A hit does not slide
expiry. Expired entries and failed exchanges confer no authority; a second key
cannot inherit the first key's principal, even within the same tenant. Retained
entries remain bounded (REQ-001, INV-001, INV-003, AC-003).

**RED.** Add planned U2 and run its exact command with controlled time rather
than sleeping in a unit test. Expect failure for absent cache reuse, incorrect
expiry, cross-key reuse, or missing bounds. Assert both the just-before and
at-expiry boundary, repeated hits, and JWT expiry earlier than cache expiry.

**GREEN.** Implement the minimum gateway-owned cache using existing dependencies
and patterns. Store only successful exchanged tokens, enforce fixed age and
bounds, and preserve verification on every use. Run U2, U1, and R1.

**REFACTOR.** Compare the result with the server's existing bounded-cache
pattern; remove unnecessary wrappers, generic machinery, and configuration.
Keep locks short and pure time/lookup logic synchronous. Rerun U1–U2 and R1.

### Scenario 3 — The real gateway preserves identity, revocation, and secrecy

**Behavior.** A real-server HTTP journey succeeds through all three ingress
dialects with accepted key headers, repeats through cache hits, and retains
ordinary per-call authorization. An underprivileged key fails before provider
dispatch. Two keys with different principals and tenants retain their own
permissions, attribution, and tenant resources. A warmed key, revoked through
the existing credential lifecycle, is refused after the cache expires; repeated
hits cannot postpone that refusal (REQ-001, INV-001–INV-003, AC-002).

**RED.** Add planned J1 in the existing gateway target and run its exact
Postgres-owned command. Expect a valid static key to fail JWT parsing before
provider dispatch. Include malformed-key refusals in all three native error
envelopes with the existing API-key error code, plus missing/multiple/query
credential negatives. Assert zero provider dispatch for refusals. Use the real
production TTL for the revocation journey unless an existing test-only timing
boundary can prove the same behavior without introducing a public setting.

**GREEN.** Complete shared state wiring and grant reuse until the journey passes.
On a miss, assert the existing exchange authorization audit and identity; on a
hit, assert no additional exchange audit while gateway permission decisions
remain audited. Check warmed-cache isolation as well as cold requests. Flush
existing capture/audit owners before inspecting retained evidence. Assert the
raw key is absent from provider requests, responses/refusals, captured call
rows, and captured production-shaped diagnostics. Rerun J1, U1–U2, and R1.

**REFACTOR.** Reuse existing Journey, telemetry, capture, and audit capabilities;
remove redundant orchestration and test-only configuration. Preserve real
client → bound server → provider → client proof. Rerun the same commands.

### Scenario 4 — The exception stays inside public gateway ingress

**Behavior.** The same valid key, including one already cached by gateway
traffic, is refused at `/mcp`, a real `/v1` management route, and Bifrost. The
existing access-token path still works (REQ-001, INV-002, AC-002).

**RED.** Add planned J2 in the existing gateway target and run its exact command.
These exclusions may already be correct: record verification-only proof for
passing cases rather than introducing production changes to create RED. Any
new cross-boundary failure must be demonstrated through the real mounted
surface; a 404 or unrelated malformed request does not prove authentication
refusal. Gateway access-token regressions R2 and R3 must stay green.

**GREEN.** Restrict new classification/cache use to gateway ingress. Preserve
existing management, MCP, and Bifrost verification. Run J2, J1, and R1–R3.

**REFACTOR.** Remove changes to general principal extraction or other surfaces
that the gateway capability does not require. Rerun J1–J2 and R1–R3.

### Scenario 5 — Stock clients need only a URL and Wyrd key

**Behavior.** Official OpenAI clients in Python and TypeScript, the official
Python Anthropic client with `x-api-key`, and the official Python Google GenAI
client with `x-goog-api-key` complete real-server requests without a Wyrd
import, token exchange, or transport hook in client invocation code. Setup may
use the established Wyrd testing harness. Ordinary buffered and streaming
responses remain usable; only the operator's provider credential travels
upstream (REQ-001, AC-001).

**RED.** Add planned P1–P3 and T1, running each exact command separately. Record
RED against the missing gateway fallback before applying its production change
if earlier scenarios would otherwise make these pass. If production behavior
is already green, record these as additional journey/regression proof rather
than manufacturing churn. Expect the static Wyrd key to be rejected before
successful provider dispatch while fallback is absent.

**GREEN.** Complete only the missing invocation closure exposed by those tests;
use the actual key in each stock client's normal key slot and server URL in
its normal base-URL option. Assert native response and streaming behavior and
provider-credential isolation. Run P1–P3, T1, and J1–J2 after the final server
change. Preserve existing access-token and `http_auth` / `httpAuth` tests.

**REFACTOR.** Reuse existing deployments and recording upstreams. Remove client
adapters or redundant auth setup from the direct-key request path. Rerun the
new journeys and the adjacent existing client regressions in touched files.

## Acceptance Criteria

| Obligation | Required result | Owner and credible proof |
|---|---|---|
| REQ-001, AC-003 | Only prefixed credentials select API-key exchange; supplied JWTs retain verification | Gateway ingress; U1, R1, R2–R3 |
| REQ-001, INV-003 | Misses use the existing minting grant, commit its tenant work, and stage its audit; hits skip only exchange audit | Auth grant plus gateway state; J1 with retained audit evidence |
| REQ-001, INV-001, AC-003 | Fixed 60-second hash-keyed reuse, bounded storage, no failure caching, no expired JWT or cross-key authority reuse | Gateway state; U2 and J1 |
| INV-001 | Key parser supplies the tenant and normal grant verification establishes it; warmed keys cannot cross tenants or principals | Existing auth owner and tenant connection; J1 |
| INV-002 | Exactly one accepted header; duplicate, mixed, wrong-native-header, empty, and query credentials remain refused | Existing extraction plus ingress; R1 and J1 |
| REQ-001, AC-002 | Invalid/malformed/revoked keys return the exchange's stable Wyrd error in each dialect; no provider call | Grant and ingress; U1, J1 |
| REQ-001 | Raw keys never enter provider headers, capture, diagnostics, or responses | Ingress plus existing invocation/capture owners; J1 and P1–P3/T1 |
| REQ-001, AC-002 | Cached keys still fail management, MCP, and Bifrost authentication | Existing mounted surface owners; J2 |
| AC-001 | Required official clients use only URL/key on invocation, with no Wyrd import/hook, against WyrdTestServer and managed Postgres | Python P1–P3 and TypeScript T1; Rust J1–J2 cover HTTP/server closure |
| REQ-002 | Guide places static-key fallback after access-token guidance, explains long-lived-secret exposure and the 60-second revocation window, and shows environment-only use; design states the narrow gateway exception | Existing guide and architecture; documentation inspection and `docs:check` |
| AC-004 | Relevant scoped checks pass; exact RED/GREEN and final outcomes are recorded | Implementer; commands below |

## Expected Write Set and Consumer Closure

- `crates/wyrd/wyrd-server/src/components/gateway/ingress.rs` and adjacent
  gateway-owned state; `src/state.rs` for shared lifecycle wiring as needed.
- `crates/wyrd/wyrd-server/src/components/auth/routes.rs` for minimal internal
  access to the existing grant; `token_extract.rs` for accurate credential
  documentation and extraction regression coverage. No broadened general auth
  behavior.
- Inlined Rust unit tests in the owning module; existing
  `crates/wyrd/wyrd-testing/tests/gateway/{native,harness}.rs` for real-server
  proof. Harness changes are limited to evidence needed by this outcome.
- Existing Python gateway OpenAI/native-client tests and TypeScript
  `gateway-inference.test.ts`; their current dependency sets already contain
  the required stock clients. No public SDK exports, bindings, or typing
  changes are expected.
- `docs/src/content/docs/connect/call-wyrd-from-a-stock-client.md` and
  `architecture/wyrd-design.md`. Correct any directly conflicting touched
  ingress rustdoc/OpenAPI descriptions at their source; if served API
  documentation changes, apply the conditional contract verification below.
- No generated contract change is expected. Do not hand-edit generated outputs.
  A discovered need for a public contract, dependency, feature, or durable
  storage change triggers the material stop conditions.

These are likely surfaces, not a private type/file allowlist.

## Verification and Evidence

This is a mixed task: executable gateway behavior follows the scenarios above;
REQ-002 and credential rustdoc use documentation inspection and existing static
checks, with no manufactured RED. Already-correct exclusions and existing
client behavior use regression/verification-only proof.

### Focused Rust commands

All commands run from the repository root. U1–U2 and J1–J2 are **planned** names
under existing inspected module/target owners, not claims that those tests
already exist. Confirm collection after adding them; if local organization
changes, update the exact commands in the task and evidence. Run Cargo-backed
commands sequentially. Do not use `--all-features` for tests.

U1 — planned classification test in the existing ingress module:

```bash
mise exec -- cargo nextest run --locked -p wyrd-server --lib -E 'test(=components::gateway::ingress::tests::gateway_credentials_select_key_exchange_or_token_verification)'
```

U2 — planned cache behavior test in the gateway owner:

```bash
mise exec -- cargo nextest run --locked -p wyrd-server --lib -E 'test(=components::gateway::ingress::tests::gateway_api_key_cache_is_bounded_isolated_and_expires)'
```

J1 — planned lifecycle, authorization, secrecy, and tenant journey:

```bash
mise exec -- scripts/postgres/with-test-postgres.sh -- bash -lc 'mise run db:migrate:all:inner && WYRD_TEST_GATEWAY_PROVIDER_KEY=sk-native-upstream cargo nextest run --locked -p wyrd-testing --test gateway -P journey --run-ignored=all -E "test(=native::gateway_api_key_exchange_is_scoped_cached_and_revocable)"'
```

J2 — planned public-surface exclusion journey:

```bash
mise exec -- scripts/postgres/with-test-postgres.sh -- bash -lc 'mise run db:migrate:all:inner && WYRD_TEST_GATEWAY_PROVIDER_KEY=sk-native-upstream cargo nextest run --locked -p wyrd-testing --test gateway -P journey --run-ignored=all -E "test(=native::gateway_api_keys_remain_rejected_outside_ingress)"'
```

R1 — existing extraction regression:

```bash
mise exec -- cargo nextest run --locked -p wyrd-server --lib -E 'test(=components::auth::token_extract::tests::gateway_access_token_needs_exactly_one_accepted_header)'
```

R2–R3 — existing native access-token journeys, respectively:

```bash
mise exec -- scripts/postgres/with-test-postgres.sh -- bash -lc 'mise run db:migrate:all:inner && WYRD_TEST_GATEWAY_PROVIDER_KEY=sk-native-upstream cargo nextest run --locked -p wyrd-testing --test gateway -P journey --run-ignored=all -E "test(=native::anthropic_messages_ingress_is_native_governed_and_fail_closed)"'
mise exec -- scripts/postgres/with-test-postgres.sh -- bash -lc 'mise run db:migrate:all:inner && WYRD_TEST_GATEWAY_PROVIDER_KEY=sk-native-upstream cargo nextest run --locked -p wyrd-testing --test gateway -P journey --run-ignored=all -E "test(=native::gemini_generate_content_ingress_is_native_governed_and_fail_closed)"'
```

### Focused stock-client commands

After the last Rust production edit, rebuild the existing harnesses sequentially:
`mise run py:setup`, `mise run ts:build`, and `mise run ts:build:testing`.
These are setup, not acceptance tests. Python commands use the current dev
client dependencies; TypeScript uses its installed workspace dependencies.

P1 — planned Python OpenAI direct-key journey:

```bash
mise exec -- scripts/postgres/with-test-postgres.sh -- bash -lc 'mise run db:migrate:all:inner && cd sdks/wyrd-sdk-python && uv run python -m pytest -q -m integration tests/integration/gateway/test_gateway_openai_client.py::test_openai_client_authenticates_with_a_wyrd_api_key'
```

P2 — planned Python Anthropic direct-key journey:

```bash
mise exec -- scripts/postgres/with-test-postgres.sh -- bash -lc 'mise run db:migrate:all:inner && cd sdks/wyrd-sdk-python && uv run python -m pytest -q -m integration tests/integration/gateway/test_gateway_native_clients.py::test_anthropic_client_authenticates_with_a_wyrd_api_key'
```

P3 — planned Python Google GenAI direct-key journey:

```bash
mise exec -- scripts/postgres/with-test-postgres.sh -- bash -lc 'mise run db:migrate:all:inner && cd sdks/wyrd-sdk-python && uv run python -m pytest -q -m integration tests/integration/gateway/test_gateway_native_clients.py::test_gemini_client_authenticates_with_a_wyrd_api_key'
```

T1 — planned TypeScript OpenAI direct-key journey:

```bash
mise exec -- scripts/postgres/with-test-postgres.sh -- bash -lc 'mise run db:migrate:all:inner && cd sdks/wyrd-sdk-ts/wyrd && pnpm exec vitest run tests/integration/gateway-inference.test.ts -t "^stock openai client authenticates with a Wyrd API key$"'
```

The exact commands above are the task's journey proof; do not substitute broad
Python/TypeScript integration suites or the complete gateway journey aggregate.
Run adjacent existing stock-client regressions in the touched files as scoped
owner coverage:

```bash
mise exec -- scripts/postgres/with-test-postgres.sh -- bash -lc 'mise run db:migrate:all:inner && cd sdks/wyrd-sdk-python && uv run python -m pytest -q -m integration tests/integration/gateway/test_gateway_openai_client.py tests/integration/gateway/test_gateway_native_clients.py tests/integration/gateway/test_client_http_auth.py'
mise exec -- scripts/postgres/with-test-postgres.sh -- bash -lc 'mise run db:migrate:all:inner && cd sdks/wyrd-sdk-ts/wyrd && pnpm exec vitest run tests/integration/gateway-inference.test.ts'
```

### Static and boundary proof

For the expected write set:

```bash
mise run fmt
mise run lints
mise run py:format
mise run py:lints
mise run ts:format
mise run ts:lints
mise run ts:typecheck
mise run check:tenant-isolation
mise run check:deps
mise run docs:check
```

`check:deps` protects the server/client and production/test-support boundaries;
`check:tenant-isolation` protects the reused tenant-auth SQL path. If Python
public typing changes, additionally run `mise run py:typecheck`; such changes
are not expected. If contract/schema generation inputs change, run
`mise run codegen:check`. If served OpenAPI documentation changes, additionally
run `mise run test:principals:integration` as required by AGENTS.md. These
conditional lanes do not replace runtime gateway proof.

Record scenario RED cause, GREEN results, and final command outcomes; distinguish
planned tests from collected/executed tests, and passing regressions from new
behavior. For failed tests, rerun with `WYRD_LOG` (or `RUST_LOG`) tracing and
inspect the trace before diagnosis. Preserve evidence linking authorization,
exchange count, retained audit, capture, and upstream dispatch to the same
request/principal. Runtime checks are for implementation; this planning edit
itself requires source/command validation and `git diff --check` only.

## Material Stop Conditions

- The existing grant cannot supply gateway token minting, tenant selection,
  mapped errors, and audit without changing approved auth/issuance semantics.
- Correctness would require a different revocation bound, sliding expiry,
  cross-key/tenant reuse, or bypassing token verification or gateway permission
  checks. Private eviction and synchronization choices remain implementation-owned.
- Implementation requires a new dependency/Cargo feature, public header or
  configuration option, distributed/persistent cache, migration, SDK API, or
  extension of direct-key acceptance beyond approved ingress boundaries.
- Required stock-client journeys cannot exercise the approved behavior through
  existing real-server boundaries without redefining the acceptance contract.

## Authority Links

Repository-relative authorities (the approved spec wins only within the
repository hierarchy described by the workflow reference):

- `changes/active/gateway-api-key-auth/spec.md` — approved r1, with the
  user-confirmed gateway-only scope stated above.
- `AGENTS.md` — ownership, struct-centered Rust, secrets, rustdoc, and testing.
- `architecture/agent-rules.md` — tenant connections, audit, async and test rules.
- `architecture/wyrd-design.md` — protocol, API-key grant, and
  cross-service delegation; the approved gateway exception must be documented.
- `architecture/wyrd-doctrine.mdx` — shared protocol and client/server boundaries.
- `architecture/wyrd-security-posture.md` and
  `architecture/v1/00-foundations/security.md` — identity, tenant isolation,
  credential verification, secrets, and non-blocking audit.
- `architecture/references/languages/spec-driven-development.md` — approved
  specification, task scope, reuse evidence, and TDD discipline.
- `architecture/references/languages/implementation-execution.md` — reversible
  implementation ownership and material-stop rules.
- `architecture/references/languages/testing-workflows.md`, `mise.toml`, and
  `.config/nextest.toml` — focused proof, managed environments, and journey lanes.
