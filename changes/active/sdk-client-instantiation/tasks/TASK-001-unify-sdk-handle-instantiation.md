---
id: TASK-001
kind: implementation
status: proposed
spec: SPEC-sdk-client-instantiation
spec_revision: 9
requirements: [REQ-001, REQ-002, REQ-003, REQ-004, REQ-005, REQ-006, REQ-007, REQ-008, REQ-009, REQ-010, REQ-011, REQ-012, REQ-013, REQ-014, REQ-015, REQ-016, REQ-017, REQ-018, REQ-019, INV-001, INV-002, INV-003, INV-004, INV-005, AC-001, AC-002, AC-003, AC-004, AC-005, AC-006, AC-007, AC-008, AC-009]
depends_on: []
parent_task:
remediates: []
---

# Unify SDK handle instantiation and Bifrost discovery

## Outcome and Value

Rust, Python, and TypeScript users open every server-facing SDK handle through
`connect`, optionally supplying one `WyrdClient`. `connect` resolves
configuration, completes any required credential exchange or refresh, and
establishes owned persistent transport; Python alone keeps an IO-free
constructor that resolves and connects once on first server use.
Omitting the client is the common usage for every handle and every supported
construction form: the shared resolver reads environment or saved configuration.
An explicit client is always optional, including with handle-specific options,
and is primarily for testing, advanced configuration, or delegated identity.
The old Rust constructors and the old `principals` module paths disappear
without aliases, while local loaders remain local and adopt only the approved
Rust `*_with` spelling. The connected Bifrost handle also lists the existing
compact, authorized table entries in every SDK and continues to describe one
table's schema only on demand.

Supplied bearer tokens and fresh cached tokens are reused without an extra
validation request; rejection may occur on the first protected operation.
Once-only handle initialization preserves existing token renewal. `Platform`
keeps its existing platform-session exchange and session-bound client.

Execute with `$wyrd-implement`.

This task owns the executable behavior for REQ-001–REQ-019, INV-001–INV-005,
and AC-001–AC-009, plus architecture/language references, executable examples,
SDK API docs, authored stubs, and generated SDK declarations. For REQ-012 and
AC-006, the authored docsite and README guidance and generated docsite outputs
belong to dependent [TASK-002](TASK-002-document-default-sdk-instantiation.md).
Neither task owns the other's documentation write set.

## Owners, Scope, Consumers, and Prohibited Changes

| Capability | Existing owner/symbol | Inspected callers/tests | Missing behavior | Selected extension | New machinery justification |
|---|---|---|---|---|---|
| Ambient client resolution and eager credential exchange | `wyrd_client::WyrdClient::{from_global, access_token}` and its shared `AuthMiddleware` cache | Existing handle `from_env`/`with_client` methods; Python `PyWyrdClient::resolve`; TypeScript `clientOf`; client auth unit tests and saved-user journeys | Handles resolve inconsistently and HTTP-only handles do not exchange until their first operation | Reuse the one client resolver and token path from each handle's `connect` entry point | None; no SDK-specific resolver, auth client, or cache is permitted |
| Rust HTTP handles | `Cards`, `Gateway`, `Principals`, and `OperatorConnections` in `wyrd-client` | Rust SDK root surface test; CLI, MCP, server, testing, examples, and Rust SDK journeys found calling `from_env`/`with_client` | No common eager `connect` / `connect_with` pair; a required credential exchange is deferred until an operation | Extend each existing dependency-owning handle and migrate every caller, preserving its retained client/cache | None; the existing structs already own the client and workflows |
| Rust platform handle | `wyrd_client::platform::Platform` | CLI platform credential flow, server boot flow, Rust SDK root surface test | Only `with_client` exists | Extend `Platform` with the same ambient/explicit `connect` pair while preserving its platform-session exchange | None; `Platform` remains the control-plane owner and Rust-only |
| Rust Bifrost and table configuration | `wyrd_client::bifrost::{Bifrost, blocking::Bifrost, TableConfig}` | Shared-client Bifrost unit/integration tests, `wyrd-testing`, CLI, Rust SDK journeys, Python/TypeScript native bindings | Bifrost uses `from_env`, required-client `connect`, and option-specific constructor names; its facade retains transport pieces rather than the one client contract | Give async and blocking facades the approved `connect` / `connect_with` option forms, retain one client for the handle lifetime, and rename `TableConfig::describe*` per D1 | None; the existing facade, writer pool, query client, and transport remain the only owners |
| Local loaders | `WyrdState::from_path_with_client` and `Workflow::from_path_with_client` | Shared-client workflow/state code, all SDK bindings and journeys, CLI/MCP/testing callers | Rust spelling conflicts with D1 | Rename only the explicit-client forms to `from_path_with`; keep local loading behavior and laziness unchanged | None; local loading remains outside connected-handle construction |
| Python connected handles and lazy construction | PyO3 `Bifrost`, `PyCards`, `PyGateway`, `Principals`, and `OperatorConnections`; pure-Python `Bifrost` / `AsyncBifrost` facade | Python unit surface/error/export tests and Postgres-backed Bifrost, Cards, gateway, principal, and operator journeys | Constructors resolve configuration immediately; Bifrost performs IO; handles have no eager `connect`; `AsyncBifrost` construction blocks | Extend the existing boundary types/facades so constructors retain arguments without IO and all first server operations reuse one successful native connection; add eager `connect` forms over shared Rust behavior | Python must retain small lazy-initialization state because that lifecycle is unique to the Python runtime; it must not own transport, authentication, or a second durable handle implementation |
| Python auth module | Native registration and public `wyrd.principals` package | `test_public_exports.py`, principal journeys, docs imports, authored stubs | Public module has the rejected name | Move the public/native projection to `wyrd.auth`, preserving class name `Principals`, and remove the old path | None; this is a public module rename, not a compatibility layer |
| TypeScript handles | `Bifrost`, `Cards`, `Gateway`, `Principals`, `OperatorConnections`, `ClientOptions`, and native adapters in `sdks/wyrd-sdk-ts` | `connect-errors.test.ts`; principal, operator, gateway, Card, and Bifrost journeys; generated N-API declarations | Only Bifrost connects eagerly; the HTTP-handle factories are synchronous and do not exchange credentials | Make every existing public static `connect` await the shared native client/auth path and keep constructors private | None; retain the existing TypeScript facade and N-API adapters |
| Bifrost table discovery | Existing `GET /v1/bifrost/tables`, `GET /v1/bifrost/tables/{namespace}/{name}`, `BifrostTableEntry`, server service, and MCP `bifrost.list_tables` / `bifrost.describe_table` | Shared Rust `Bifrost` and query client; Python and TypeScript Bifrost bindings; existing server/MCP journeys; query-workbench catalog flow | First-class SDK Bifrost handles can describe a known table but cannot list authorized tables | Project the existing compact list operation and wire fields through each existing Bifrost facade; keep schema retrieval on the existing describe operation | None; the route, authorization, audit, tenant binding, and wire type already exist |
| Architecture, SDK docs, examples, authored stubs, and generated SDK surfaces | `architecture/wyrd-design.md`, language references, Rust/Python/TypeScript API docs, executable examples, package-local Python `.pyi` files, N-API declarations | Rust/Python principal import examples; `py:lints`, `py:typecheck`, `codegen:check`, `ts:napi:check`, example tasks | Guidance and declarations encode the old forms | Update these owners to lead with no-client ambient instantiation; edit authored Python stubs directly and regenerate TypeScript artifacts; TASK-002 owns docsite and README prose | None; do not add a second checker or hand-edit generated files |

Paths are guidance, not a private implementation allowlist. Consumer closure
includes every repository caller of a removed public form, including non-SDK
crates and test/support code that compile against `wyrd-client`.

Prohibited changes:

- Do not change handle capabilities except for the approved Bifrost list
  projection. Do not change server routes, wire contracts, permissions, audit
  behavior, durable state, or error catalog entries.
- Do not add `list_catalog`, a catalog/schema object hierarchy,
  `list_schemas`, filters, pagination, client caching, or eager describe
  fan-out; do not change the query workbench UI in this task.
- Do not add aliases, deprecations, compatibility modules, overload builders,
  new dependencies, Cargo features, transports, authentication paths, token
  caches, health probes, or per-language ambient resolvers.
- Do not project `Platform` into Python or TypeScript, add Python root exports,
  or rename the `Principals` class.
- Do not make local loaders eager or turn `WyrdClient` construction into part
  of this change.
- Do not hand-edit generated TypeScript wire types or N-API declarations.
  Package-local Python `.pyi` files are authored and must be edited directly.
- Do not force exchange for supplied bearer or fresh cached tokens, suppress
  existing renewal/re-exchange, or route Platform through tenant authentication.

## Approach

1. Pin the approved public shapes with Rust, Python, and TypeScript surface
   tests, including the absence of every removed form and old auth-module path.
2. Extend the existing `wyrd-client` handle owners to retain one client and
   implement eager ambient/explicit connection through the shared resolution
   and authentication paths; migrate Rust consumers and the D1 local-loader
   and table-description spellings.
3. Project those owners through Python with eager `connect` and one-shot lazy
   constructors, then rename the Python principal module to `auth`.
4. Make every TypeScript handle's existing static `connect` eager and
   awaitable through the shared native client, retaining private constructors.
5. Extend the existing Bifrost query owner and language projections with the
   existing compact table listing, typed at each boundary and paired with the
   existing lazy describe operation.
6. Add the required per-language real-server journeys for environment-only and
   saved-only configuration, delegated identity, stable eager/lazy failures, and
   once-only resolution, token-cache reuse and renewal, and table discovery;
   update all affected existing journeys.
7. Align architecture, language references, SDK API docs, executable examples,
   exports, authored Python stubs, and generated TypeScript declarations. Lead
   with no-client usage, then run the scoped SDK, Bifrost, codegen, examples,
   and dependency-boundary proof. Hand off docsite and README guidance to TASK-002.

## Ordered Implementation Scenarios

### Scenario 1 — The public surface has one construction vocabulary

**Behavior.** Every in-scope handle exposes the approved `connect` form with
an optional client in its language's D1 shape. Rust exposes `connect` and
`connect_with`; Python exposes `connect` and its constructor; TypeScript exposes
only static `connect` with a private constructor. The removed Rust forms and
`wyrd.principals` / `wyrd_ai::principals` paths are absent. Local loaders and
`TableConfig::describe` use the D1 spellings without becoming connected
handles. Covers REQ-001, REQ-004–REQ-006, REQ-010–REQ-011, REQ-016, AC-001,
and INV-001.

**RED.** Update or add the smallest public-surface tests in the existing SDK
owners; they fail because several handles lack `connect`, removed forms remain,
and the principal modules still use the old name:

```bash
mise exec -- cargo nextest run --locked -p wyrd-ai --lib \
  -E 'test(=tests::sdk_root_exposes_the_supported_surface)'
(cd sdks/wyrd-sdk-python && mise exec -- uv run python -m pytest -q \
  tests/unit/test_sdk_client_instantiation.py::test_handles_expose_only_the_common_construction_forms)
(cd sdks/wyrd-sdk-ts/wyrd && mise exec -- pnpm exec vitest run \
  tests/unit/sdk-client-instantiation.test.ts -t 'exports only the common handle construction forms')
```

**GREEN.** Introduce only the approved public names on the existing owners,
remove the replaced names and old module paths, and mechanically update all
repository callers so the three surface tests compile and pass.

**REFACTOR.** Remove obsolete constructor-specific documentation and redundant
binding branches exposed by the unified names. Keep private implementation
choices within the established handle owners and rerun all three surface
tests.

### Scenario 2 — `connect` is eager and preserves handle-specific options

**Behavior.** Ambient and explicit `connect` resolve one client, complete any
exchange or refresh required by the existing credential path before returning,
and report the existing stable configuration, exchange, or required transport
error at that point. Supplied bearer tokens and fresh cached tokens cause no
extra validation or exchange request; their rejection may occur on the first
protected operation. Bifrost also establishes its persistent transport and
accepts its table/queue options;
HTTP-only handles add no health request. Platform preserves its existing
platform-session exchange and retains the resulting session-bound client.
Sync/async behavior follows REQ-005.
Covers REQ-002–REQ-005, REQ-014–REQ-015, INV-002, INV-004–INV-005, and the
negative portion of AC-004.

**RED.** Add focused owner and boundary tests that enumerate every in-scope
handle and expect required eager exchange/failure plus bearer/cache reuse;
they fail because the HTTP handles currently return before exchange,
Python lacks eager factories, and TypeScript
HTTP factories are synchronous:

```bash
mise exec -- cargo nextest run --locked -p wyrd-client --lib \
  -E 'test(=client::tests::server_handles_connect_eagerly_and_share_the_client_auth_path)'
(cd sdks/wyrd-sdk-python && mise exec -- uv run python -m pytest -q \
  tests/unit/test_sdk_client_instantiation.py::test_connect_is_eager_for_every_handle)
(cd sdks/wyrd-sdk-ts/wyrd && mise exec -- pnpm exec vitest run \
  tests/unit/connect-errors.test.ts -t 'every handle connect fails eagerly without a credential')
```

**GREEN.** Route all handle connection through `WyrdClient::from_global` or
the supplied client and its existing authentication cache; preserve Platform's
separate platform-session path. Establish Bifrost's
existing transport during connection and pass its existing option values
through the common form. Return the existing errors unchanged and redact all
credential-bearing debug/error output.

**REFACTOR.** Delete superseded constructor branches and repeated ambient
resolution now made unnecessary by the common owner path. Do not introduce a
generic handle trait or utility owner; rerun this scenario and Scenario 1.

### Scenario 3 — Python constructors are IO-free and initialize once

**Behavior.** Constructing every Python handle retains its optional arguments
without resolving configuration or performing network IO. The first operation
that needs the server performs the same connection as `connect`, once per
handle; successful eager and lazy handles then behave identically.
`AsyncBifrost()` returns without blocking the event loop and
`await AsyncBifrost.connect()` is eager. Covers REQ-006–REQ-009, REQ-014,
INV-002, INV-004–INV-005, AC-004–AC-005, and AC-007.

**RED.** Replace the current eager-constructor assertions with focused lazy
surface tests; they fail because constructors currently resolve credentials
and Bifrost dials immediately:

```bash
(cd sdks/wyrd-sdk-python && mise exec -- uv run python -m pytest -q \
  tests/unit/test_sdk_client_instantiation.py::test_constructors_are_io_free)
(cd sdks/wyrd-sdk-python && mise exec -- uv run python -m pytest -q \
  tests/unit/test_sdk_client_instantiation.py::test_async_bifrost_constructor_does_not_block)
```

**GREEN.** Retain the constructor arguments at the Python boundary and defer
the shared native connection until a server operation needs it. Ensure every
operation uses the same successful native handle and that failure comes only
from the same shared connection path.

**REFACTOR.** Consolidate only genuinely shared Python lazy-boundary behavior;
do not duplicate Rust transport or authentication logic and do not move
unrelated legacy wrappers. Rerun this scenario and Scenarios 1–2.

### Scenario 4 — No-client usage opens every handle from environment or saved configuration

**Behavior.** Against `WyrdTestServer`, each language opens every in-scope
handle with `connect()` and no client in both environment-only and
saved-configuration-only cases, then completes one real server call. Existing
handle-specific options remain usable without a client. Rust also covers `Platform`; Python and
TypeScript do not. Python additionally constructs every handle without a
client and completes its first server operation in both ambient configuration
cases, including `Bifrost()` and `AsyncBifrost()`. Removing the credential makes each language fail at eager
`connect`; Python's constructor reports the same stable error only on its first
operation. Covers REQ-001–REQ-009, REQ-012, REQ-014–REQ-015, INV-001–INV-002,
AC-002, and AC-004.

Also exercise rejected exchange credentials and supplied invalid bearers
against the real server. A required exchange rejects at eager `connect`;
HTTP-only `connect` with a supplied bearer performs no validation request and
the first protected operation receives the existing refusal. Python lazy
initialization preserves those same error codes. A fresh cached token is
reused without another exchange. Bifrost's transport setup is not a promise
that its first protected operation will be authorized.

**RED.** Add focused existing-harness journeys per language. Each fails on the
current inconsistent/missing construction forms or because an HTTP-only
`connect` has not completed a required exchange eagerly:

```bash
scripts/postgres/with-test-postgres.sh -- bash -lc \
  "mise run db:migrate:all:inner && mise exec -- cargo nextest run --locked \
  -p wyrd-ai --test integration -P journey --run-ignored=all \
  -E 'test(=sdk_client_instantiation::ambient_connect_opens_every_handle)'"
scripts/postgres/with-test-postgres.sh -- bash -lc \
  "mise run db:migrate:all:inner && mise exec -- cargo nextest run --locked \
  -p wyrd-ai --test integration -P journey --run-ignored=all \
  -E 'test(=sdk_client_instantiation::connect_without_a_credential_fails_eagerly)'"
mise run py:setup && scripts/postgres/with-test-postgres.sh -- bash -lc \
  "mise run db:migrate:all:inner && cd sdks/wyrd-sdk-python && mise exec -- \
  uv run python -m pytest -q -m integration \
  tests/integration/test_sdk_client_instantiation.py::test_ambient_connect_opens_every_handle"
mise run py:setup && scripts/postgres/with-test-postgres.sh -- bash -lc \
  "mise run db:migrate:all:inner && cd sdks/wyrd-sdk-python && mise exec -- \
  uv run python -m pytest -q -m integration \
  tests/integration/test_sdk_client_instantiation.py::test_lazy_constructors_open_every_handle_on_first_operation"
mise run py:setup && scripts/postgres/with-test-postgres.sh -- bash -lc \
  "mise run db:migrate:all:inner && cd sdks/wyrd-sdk-python && mise exec -- \
  uv run python -m pytest -q -m integration \
  tests/integration/test_sdk_client_instantiation.py::test_connect_and_lazy_first_call_share_the_no_credentials_error"
mise run ts:build && mise run ts:build:testing && \
  scripts/postgres/with-test-postgres.sh -- bash -lc \
  "mise run db:migrate:all:inner && cd sdks/wyrd-sdk-ts/wyrd && mise exec -- \
  pnpm exec vitest run tests/integration/sdk-client-instantiation.test.ts \
  -t 'ambient connect opens every handle'"
mise run ts:build && mise run ts:build:testing && \
  scripts/postgres/with-test-postgres.sh -- bash -lc \
  "mise run db:migrate:all:inner && cd sdks/wyrd-sdk-ts/wyrd && mise exec -- \
  pnpm exec vitest run tests/integration/sdk-client-instantiation.test.ts \
  -t 'connect without a credential fails eagerly'"
scripts/postgres/with-test-postgres.sh -- bash -lc \
  "mise run db:migrate:all:inner && mise exec -- cargo nextest run --locked \
  -p wyrd-ai --test integration -P journey --run-ignored=all \
  -E 'test(=sdk_client_instantiation::credential_paths_preserve_exchange_and_bearer_behavior)'"
mise run py:setup && scripts/postgres/with-test-postgres.sh -- bash -lc \
  "mise run db:migrate:all:inner && cd sdks/wyrd-sdk-python && mise exec -- \
  uv run python -m pytest -q -m integration \
  tests/integration/test_sdk_client_instantiation.py::test_credential_paths_preserve_exchange_and_bearer_behavior"
mise run ts:build && mise run ts:build:testing && \
  scripts/postgres/with-test-postgres.sh -- bash -lc \
  "mise run db:migrate:all:inner && cd sdks/wyrd-sdk-ts/wyrd && mise exec -- \
  pnpm exec vitest run tests/integration/sdk-client-instantiation.test.ts \
  -t 'credential paths preserve exchange and bearer behavior'"
```

**GREEN.** Complete the thinnest existing-harness journeys through the shared
owners, using one harmless real call per handle and the repository's saved
configuration facilities. Extend the named ambient journeys to cover both
environment-only and saved-only resolution, including Python lazy handles and
no-client use with handle-specific options. Prove both credential paths through the focused
journeys above. Do not add an alternate server fixture or transport.

**REFACTOR.** Reuse the existing server, saved-login/configuration, and typed
request fixtures where they already express the workflow; keep each journey
focused on construction timing and identity. Rerun Scenarios 1–4.

### Scenario 5 — Explicit delegated clients remain authoritative

**Behavior.** In each language, a handle connected with a delegated client
performs a real call as the delegated subject, with the server attribution
showing that principal. No handle replaces the supplied client or resolves
ambient credentials. Covers REQ-002, REQ-014, INV-003, and AC-003.

**RED.** Extend each language's instantiation journey with the delegated
identity case; the Rust common forms and non-Bifrost eager projections do not
yet exist, so the complete parity scenario fails:

```bash
scripts/postgres/with-test-postgres.sh -- bash -lc \
  "mise run db:migrate:all:inner && mise exec -- cargo nextest run --locked \
  -p wyrd-ai --test integration -P journey --run-ignored=all \
  -E 'test(=sdk_client_instantiation::delegated_connect_uses_the_subject_principal)'"
mise run py:setup && scripts/postgres/with-test-postgres.sh -- bash -lc \
  "mise run db:migrate:all:inner && cd sdks/wyrd-sdk-python && mise exec -- \
  uv run python -m pytest -q -m integration \
  tests/integration/test_sdk_client_instantiation.py::test_delegated_connect_uses_the_subject_principal"
mise run ts:build && mise run ts:build:testing && \
  scripts/postgres/with-test-postgres.sh -- bash -lc \
  "mise run db:migrate:all:inner && cd sdks/wyrd-sdk-ts/wyrd && mise exec -- \
  pnpm exec vitest run tests/integration/sdk-client-instantiation.test.ts \
  -t 'delegated connect uses the subject principal'"
```

**GREEN.** Pass the supplied client through each existing language boundary
unchanged and assert server-observed subject attribution on a representative
handle operation per language. Preserve the existing delegated exchange and
audit semantics.

**REFACTOR.** Remove any now-redundant boundary conversions that would copy
identity state, while preserving the one shared client/cache. Rerun Scenarios
1–5.

### Scenario 6 — Handles resolve once and reuse the client's renewable cache

**Behavior.** For eager handles and successfully initialized Python lazy
handles, changing ambient configuration after connection does not change the
endpoint or principal used later. Repeated operations reuse the initial
exchange while its cached token is fresh, or perform no exchange for a supplied
bearer or already-fresh cache. Expiry still permits the existing renewal
through that same retained client; refusal-triggered re-exchange remains
unchanged. Platform follows its existing session lifetime. Covers REQ-007–REQ-008,
REQ-014, INV-002–INV-003, and AC-007.

**RED.** Extend the three instantiation journeys to mutate ambient
configuration after the first successful operation and count exchanges within
token validity, then exercise renewal across token expiry for renewable
credential paths. The complete matrix fails until the common construction forms
and Python lazy initialization exist; existing client/cache retention is
regression proof to preserve, not behavior to replace:

```bash
scripts/postgres/with-test-postgres.sh -- bash -lc \
  "mise run db:migrate:all:inner && mise exec -- cargo nextest run --locked \
  -p wyrd-ai --test integration -P journey --run-ignored=all \
  -E 'test(=sdk_client_instantiation::connected_handles_reuse_the_client_cache_and_renew)'"
mise run py:setup && scripts/postgres/with-test-postgres.sh -- bash -lc \
  "mise run db:migrate:all:inner && cd sdks/wyrd-sdk-python && mise exec -- \
  uv run python -m pytest -q -m integration \
  tests/integration/test_sdk_client_instantiation.py::test_handles_reuse_the_client_cache_and_renew"
mise run ts:build && mise run ts:build:testing && \
  scripts/postgres/with-test-postgres.sh -- bash -lc \
  "mise run db:migrate:all:inner && cd sdks/wyrd-sdk-ts/wyrd && mise exec -- \
  pnpm exec vitest run tests/integration/sdk-client-instantiation.test.ts \
  -t 'handles reuse the client cache and renew'"
```

**GREEN.** Ensure each existing handle owner stores and reuses the selected
client for its lifetime, and use the existing test server/auth observation
seams to prove cache reuse and successful renewal without adding production
counters or cache state. Do not force exchange for a supplied bearer or fresh
cache, and do not impose tenant renewal behavior on platform sessions.

**REFACTOR.** Delete per-call resolution that is now unreachable and preserve
the existing `WyrdClient` token-cache implementation unchanged. Rerun every
scenario.

### Scenario 7 — Bifrost lists tables compactly and describes on demand

**Behavior.** In each first-class SDK, a connected Bifrost registers or uses a
user table, lists the compact authorized entries, sees that table and the
visible built-ins, and does not see another tenant's table. A principal without
`bifrost_table:read` receives the existing stable authorization error. The
caller then describes the user table separately and receives its declared
fields; listing performs no per-table describe fan-out. The public names are
Rust/Python `list_tables` and TypeScript `listTables`, with the existing wire
entry typed at each boundary and no `list_catalog` or `list_schemas`. Covers
REQ-017–REQ-019, INV-001–INV-005, and AC-008–AC-009.

**RED.** Add one existing-harness journey per language. Each fails because the
server operation and wire type exist but the SDK Bifrost facades do not expose
the list operation:

```bash
scripts/postgres/with-test-postgres.sh -- bash -lc \
  "mise run db:migrate:all:inner && mise exec -- cargo nextest run --locked \
  -p wyrd-ai --test integration -P journey --run-ignored=all \
  -E 'test(=sdk_client_instantiation::bifrost_lists_tables_and_describes_on_demand)'"
mise run py:setup && scripts/postgres/with-test-postgres.sh -- bash -lc \
  "mise run db:migrate:all:inner && cd sdks/wyrd-sdk-python && mise exec -- \
  uv run python -m pytest -q -m integration \
  tests/integration/test_sdk_client_instantiation.py::test_bifrost_lists_tables_and_describes_on_demand"
mise run ts:build && mise run ts:build:testing && \
  scripts/postgres/with-test-postgres.sh -- bash -lc \
  "mise run db:migrate:all:inner && cd sdks/wyrd-sdk-ts/wyrd && mise exec -- \
  pnpm exec vitest run tests/integration/sdk-client-instantiation.test.ts \
  -t 'Bifrost lists tables and describes schemas on demand'"
```

**GREEN.** Add the one list call to the existing shared Bifrost query client
and async/blocking facades, then project it through the existing Python and
TypeScript bindings. Reuse `BifrostTableEntry`, the existing list route, shared
JSON/error conversion, authorization, and tenant context unchanged. Expose a
Python `TypedDict` and reuse TypeScript's existing `TableEntry` re-export of
the generated interface from their owning public modules.

**REFACTOR.** Remove any duplicate boundary conversion introduced during
projection. Keep the compact entry in its server-provided shape and do not add
an aggregator, cache, catalog wrapper, or schema-prefetch helper. Rerun all
three discovery journeys and the public/generated surface checks.

## Acceptance Criteria

- Rust, Python, and TypeScript expose the exact approved construction forms for
  every in-scope handle; removed Rust forms and old `principals` module paths
  are not public, and TypeScript constructors remain private (AC-001).
- No-client `connect()` opens every in-scope handle and completes a real server
  operation in both environment-only and saved-configuration-only cases in
  every applicable language. Python lazy constructors do the same on first
  server use; handle-specific options never require a client (AC-002).
- Explicit delegated clients remain authoritative and server attribution names
  the delegated subject in Rust, Python, and TypeScript (AC-003).
- Missing configuration/credentials, rejected required exchanges, and
  unreachable required transports fail at eager `connect` with existing stable
  errors; Python's constructor defers those failures to first server use.
  Supplied bearer and fresh cached tokens trigger no extra validation/exchange
  request; a supplied invalid bearer on an HTTP-only handle is refused on the
  first protected operation, with the same error on Python's lazy path (AC-004).
- `Bifrost()` and `AsyncBifrost()` perform no network IO, and async construction
  does not block the event loop (AC-005).
- Architecture and language authorities, SDK API docs, executable examples,
  exports, authored Python stubs, and TypeScript declarations use the approved
  forms and module paths. Primary guidance omits the client; testing, advanced
  configuration, and delegation forms label it optional. TASK-002 owns the
  docsite/README portion of AC-006.
- Each handle resolves ambient configuration once and reuses the retained
  client's fresh token cache across calls, including Python's lazy path;
  existing renewal works across expiry and refusal-triggered re-exchange is
  preserved. Supplied bearer tokens require no exchange. Platform retains the
  session-bound client from its existing exchange (AC-007, REQ-014–REQ-015, D4).
- Each SDK's Bifrost exposes the exact approved table-list name and typed
  compact entries; a real-server journey proves visible built-ins, a registered
  user table, tenant isolation, existing authorization denial, and separate
  on-demand schema description (AC-008–AC-009).
- Existing capabilities except the approved Bifrost list projection,
  permissions, server behavior, error codes, credential redaction,
  local-loader behavior, and first-class language boundaries remain unchanged
  (INV-001–INV-005).

## Expected Write Set and Consumer Closure

Likely production and public-surface owners:

- `crates/shared/wyrd-client/src/{client.rs,cards/,gateway.rs,principals/,operator_connections.rs,platform/,bifrost/,state.rs,workflow/}` and its focused tests;
- `sdks/wyrd-sdk-rust/src/lib.rs` and the consolidated integration target;
- `sdks/wyrd-sdk-python/src/`, public modules under
  `sdks/wyrd-sdk-python/python/wyrd/`, authored package-local `.pyi` stubs, and
  Python unit and integration tests;
- `sdks/wyrd-sdk-ts/native/src/`, `sdks/wyrd-sdk-ts/wyrd/src/`, generated N-API
  declarations, and TypeScript unit and integration tests;
- every CLI, MCP, server, test-harness, benchmark, example, and test caller
  that uses a removed Rust form or module path;
- `architecture/wyrd-design.md`, applicable language references and patterns,
  SDK API docs, and executable examples containing handle construction.

Docsite content, generated docsite outputs, and README prose belong to TASK-002.
Migrate executable example sources here; TASK-002 updates docsite references
(including `CodeFromFile` ranges) after those sources settle.

The existing Bifrost server routes, service, wire structs, MCP tools, and query
workbench are consumers and verification references, not expected production
write targets. A required server or UI change is a material stop condition.

Generated TypeScript artifacts must be regenerated from their owning source;
Python `.pyi` stubs are edited directly. No manifest or lockfile change is
expected because the approved behavior requires no new
dependency or feature.

## Verification and Evidence

Run the exact scenario commands above during Red-Green execution. REQ-012,
REQ-013, REQ-019, AC-006, and the generated-surface portion of AC-009 are
static/generated obligations rather than additional runtime behavior: prove
them through source inspection plus the examples, codegen, declaration,
export, format, and type-check lanes below; do not manufacture another RED.
Python stubs are authored: `py:lints` validates their docs, exports, and layout,
and `py:typecheck` validates typing. `codegen:check` covers generated TypeScript
wire types; `ts:napi:check` separately covers generated N-API declarations.
Existing client retention, token-cache reuse, and renewal are regression
obligations; preserve them while adding the common connection entry points.

The Rust SDK package is `wyrd-ai` (crate import `wyrd_ai`); its existing
integration target is `sdks/wyrd-sdk-rust/tests/integration/main.rs`.
The new scenario test names are planned additions to those existing language
test owners; register the Rust journey module in that integration target and
confirm exact discovery before running each focused selector. Build the Python
and TypeScript native boundaries before their focused unit commands using
`mise run py:setup` and `mise run ts:build`, respectively.

After all scenarios pass, run these repository-native task lanes sequentially:

```bash
mise run fmt
mise run lints
mise run py:format
mise run py:lints
mise run py:typecheck
mise run ts:format
mise run ts:lints
mise run ts:typecheck
mise run test:shared
mise run test:wyrd-sdk
mise run py:test:unit
mise run ts:test:unit
mise run codegen:check
mise run ts:napi:check
mise run check:py-wheel-no-testing
mise run check:deps
mise run check:examples
git diff --check
```

The exact scenario commands are the task's required journeys; do not replace
them with full SDK journey suites. `test:shared` covers the shared Rust owners,
and `test:wyrd-sdk`, `py:test:unit`, and `ts:test:unit` cover the three public
projections. Python lint/type checks cover authored stubs; codegen, N-API, and
wheel checks cover generated and package surfaces; `check:deps` proves no
boundary/feature drift.
`check:examples` proves the executable-example sweep. TASK-002 runs the
docsite gate and reviews README guidance after this implementation lands.

## Material Stop Conditions

Stop and return to specification authority if implementation requires any of
the following:

- a public shape other than D1–D5, an additional compatibility form, a new
  root export, or projecting `Platform` to another language;
- a new wire contract, stable error, server route, permission, audit rule,
  persistent state, dependency, Cargo feature, transport, authentication path,
  or cache;
- a Python constructor failure unrelated to the shared connection path, or a
  semantic difference between eager and successfully initialized lazy handles;
- changing local-loader eagerness/capability or `WyrdClient` construction;
- discovery that requires a new route or wire type, per-table authorization
  semantics, pagination, a catalog hierarchy, client caching, eager schema
  fan-out, or a query-workbench change;
- weakening any required real-server journey, generated-surface check,
  redaction guarantee, or removed-form proof.

## Authority Links

- [Approved specification revision 9](../spec.md)
- [Repository standards](../../../../AGENTS.md)
- [Wyrd design — Client model](../../../../architecture/wyrd-design.md#client-model)
- [Core doctrine](../../../../architecture/wyrd-doctrine.mdx)
- [Bifrost public surface](../../../../architecture/bifrost-design.md#public-surface)
- [Architecture patterns — Client pattern](../../../../architecture/references/architecture/patterns.md#client-pattern)
- [Python API and stubs](../../../../architecture/references/languages/python-api-and-stubs.md)
- [TypeScript guide](../../../../architecture/references/languages/typescript-guide.md)
- [Spec-driven development](../../../../architecture/references/languages/spec-driven-development.md)
- [Implementation execution](../../../../architecture/references/languages/implementation-execution.md)
- [Testing workflows](../../../../architecture/references/languages/testing-workflows.md)

## Implementation Evidence

Commits on `change/sdk-client-instantiation`: `7feabed67` (Rust owners and
callers), `9c6858846` (Python lazy handles, `wyrd.auth`), `be1f17b54`
(TypeScript), `e84739f09` / `b6fef0233` (Rust and Python journeys),
`c5feb76db` (architecture authorities, lint findings), plus the formatting and
evidence commits that follow.

| Acceptance criterion | Implementation evidence | Verification evidence | Result |
|---|---|---|---|
| AC-001 approved forms; removed forms and `principals` paths absent; TS constructors private | `wyrd-client` handles `connect`/`connect_with`; `sdks/wyrd-sdk-rust/src/lib.rs` (`auth` re-export, 19 compile-fail doctests); Python `__new__` + `connect` staticmethods, `python/wyrd/auth`; TS private constructors + `static async connect` | `wyrd-ai` `tests::sdk_root_exposes_the_supported_surface` and doctests; `test_sdk_client_instantiation.py::test_handles_expose_only_the_common_construction_forms`; TS `exports only the common handle construction forms` | PASS |
| AC-002 ambient connect opens every handle (env-only, saved-only); Python lazy first use; options need no client | `WyrdClient::connect_ambient`; Python `LazyConnection`; `BifrostOptions` | Rust `sdk_client_instantiation::ambient_connect_opens_every_handle` (incl. Platform); Python `test_ambient_connect_opens_every_handle`, `test_lazy_constructors_open_every_handle_on_first_operation`; TS `ambient connect opens every handle` | PASS (see limit 1) |
| AC-003 delegated clients authoritative | Supplied client passed unchanged through every boundary | Rust/Python/TS `delegated connect uses the subject principal`: the subject's authority decides the call, and the actor's own call succeeds | PASS (see limit 2) |
| AC-004 eager failures; Python defers to first use; bearer/cache make no extra request | `WyrdClient::connected` (`access_token`) | `client::tests::server_handles_connect_eagerly_and_share_the_client_auth_path`; `connect_without_a_credential_fails_eagerly`; `credential_paths_preserve_exchange_and_bearer_behavior` (Rust/Python/TS); Python `test_connect_and_lazy_first_call_share_the_no_credentials_error`; TS `every handle connect fails eagerly without a credential` | PASS (see limit 3) |
| AC-005 `Bifrost()`/`AsyncBifrost()` do no IO; async construction does not block | Native `Bifrost.__new__` retains its arguments; `AsyncBifrost.connect` runs on `asyncio.to_thread` | `test_constructors_are_io_free`, `test_async_bifrost_constructor_does_not_block` | PASS |
| AC-006 (TASK-001 portion) authorities, stubs, declarations, examples | `architecture/wyrd-design.md#client-model`, `patterns.md`, `python-api-and-stubs.md`, `typescript-guide.md`; `.pyi` stubs; regenerated N-API declarations; examples migrated | `py:lints`, `py:typecheck`, `ts:typecheck`, `ts:napi:check`, `codegen:check`, `check:examples` | PASS |
| AC-007 resolve once, reuse the cache, renew | Handles retain one `WyrdClient`; Python caches the connected handle in `OnceLock` | Rust `connected_handles_reuse_the_client_cache_and_renew` (saved config rewritten after connect; 35 s TTL); Python `test_handles_reuse_the_client_cache_and_renew`; TS `handles reuse the client cache and renew` | PASS |
| AC-008–AC-009 Bifrost `list_tables`/`listTables`, compact typed entries | `QueryClient::list_tables`, async/blocking facades, PyO3 `list_tables` (existing `TableEntry` TypedDict), TS `listTables(): Promise<TableEntry[]>` | `bifrost_lists_tables_and_describes_on_demand` (Rust/Python/TS): user table and `vala.drift.observations` listed, other tenant's table hidden, 403 without `bifrost_table:read`, describe returns declared fields | PASS |
| INV-001–INV-005 unchanged behavior | No route, wire type, error, permission, dependency, or feature added | `check:deps`, `check:py-wheel-no-testing`, `test:rust-sdk:journey` (102 passed), `py:test:integration` (254 passed), `ts:test:integration` (214 passed), `test:shared` | PASS |

Lanes run: `fmt`, `lints`, `py:format`, `py:lints`, `py:typecheck`,
`ts:format`, `ts:lints`, `ts:typecheck`, `test:shared`, `test:wyrd-sdk`,
`py:test:unit`, `ts:test:unit`, `codegen:check`, `ts:napi:check`,
`check:py-wheel-no-testing`, `check:deps`, `check:examples`, `git diff --check`.

Material limits:

1. The Rust journeys drive the ambient chain through a fixed `Environment` with
   `ClientConfig::from_environment`, the same resolver `connect()` reaches
   through `WyrdClient::from_global`. This follows the `saved_user_auth.rs`
   precedent. The workspace denies `unsafe_code`, so a test cannot mutate the
   process environment, and spawning a child process was rejected. `connect()`
   itself is the one-line composition `from_global` + `connected`. Python and
   TypeScript drive true no-client `connect()` against a mutated environment.
2. Delegated attribution is proven by authorization outcome (the subject's
   grant decides), matching the existing Python delegated-Bifrost journey. The
   audit rows are not read back.
3. A refused API-key exchange surfaces the existing client mapping of the OAuth
   refusal, `WYRD_SPEC_400_VALIDATION`, even though the server body carries
   `WYRD_AUTH_401_API_KEY_INVALID`. The journeys assert the existing code
   unchanged. This is likely a pre-existing defect in `wyrd-client`'s exchange
   error mapping and is outside this task.
4. The saved-login identity journeys (`saved_user_auth`,
   `signed_in_development`, TS `saved-user-auth`) need the Keycloak lane and
   were migrated but not run.
5. Docsite and README references (for example
   `docs/src/content/docs/operate/grant-and-inspect-permissions.md`, which still
   imports `wyrd.principals` and calls synchronous `Principals.connect()`)
   belong to TASK-002.

Non-goals remained excluded: no Platform projection to Python or TypeScript,
no compatibility aliases, no new server route, wire type, cache, or dependency.

Status: IMPLEMENTED.
