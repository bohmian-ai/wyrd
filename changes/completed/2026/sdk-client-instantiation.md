# One way to instantiate every SDK handle

- Change: `sdk-client-instantiation`
- Specification: `SPEC-sdk-client-instantiation`, approved revision 11
- Completed: 2026-10-15
- Reviewed target: `4915f55e4`, approved by the human owner in place of a final change review
- Delivery reference: not supplied

Every server-facing handle in the Rust, Python, and TypeScript SDKs now opens
with `connect`, and the client is optional everywhere. A developer who knows
how to open one handle in one language knows how to open every handle in every
first-class language. Omitting the client is the normal usage: the handle
resolves its endpoint and credential through the shared `wyrd-client` ambient
resolver (saved configuration, environment, saved login). An explicit client
is for testing, advanced configuration, or delegation. Bifrost also gained
compact, authorized table discovery in all three SDKs.

## Shipped behavior

- Handles in scope: `Bifrost` (Rust async and blocking, Python `Bifrost` and
  `AsyncBifrost`, TypeScript), `Cards`, `Gateway`, `Principals`,
  `OperatorConnections`, and Rust-only `Platform`.
- Rust spells the optional client as `X::connect()` / `X::connect_with(&client)`,
  with an options argument where a handle has options (Bifrost's
  `BifrostOptions`). The same pair applies to `WyrdState::from_path` /
  `from_path_with` and `TableConfig::describe` / `describe_with`. Python takes
  `client=` and handle options as keyword arguments; TypeScript takes an
  options object.
- `connect` is eager: it resolves configuration, completes any credential
  exchange the existing path requires, and dials Bifrost's persistent
  transport. Configuration, exchange, and dial failures surface at `connect`
  with their existing stable errors. Supplied bearers and fresh cached tokens
  are reused without a validation request, so their rejection surfaces on the
  first protected call.
- Each handle holds one `WyrdClient` for its lifetime and never re-resolves the
  ambient configuration; token renewal and refusal-triggered re-exchange still
  work through that client. `Platform` keeps its session-bound client.
- Python keeps a public constructor with the same optional arguments. It does
  no IO; the first server operation connects once. Local state (Bifrost's
  table binding, producer counts, typed Cards view selection) never connects.
  On a handle not yet connected, the first `insert` connects synchronously,
  then enqueueing is non-blocking. `AsyncBifrost()` no longer blocks the event
  loop; `await AsyncBifrost.connect()` is its eager form.
- Removed without aliases: Rust `from_env()`, `with_client(..)`, required-client
  `connect(&client)`, `connect_with_table`, `connect_with_config`, and
  `from_path_with_client`; TypeScript constructors stay private. The module
  owning `Principals` is `auth` (`wyrd.auth`, `wyrd_ai::auth`).
- Bifrost discovery: `list_tables()` (Rust async/blocking, Python sync/async)
  and `listTables()` (TypeScript) return the server's existing compact
  `BifrostTableEntry` values; `describe_table` / `describeTable` fetch one
  table's schema on demand. Python types the entry as a `TypedDict`,
  TypeScript as an interface.
- `WyrdClient::from_environment(Environment)` is the single ambient resolver;
  `WyrdClient::from_global()` calls it with the process environment.

## Invariants

- No handle gains or loses capability beyond Bifrost's projection of the
  existing table listing; no permission, server, or durable behavior changed.
- Ambient resolution lives only in `wyrd-client`; no SDK reads endpoints or
  credentials itself.
- A supplied client, including a delegated `on_behalf_of` client, is used as
  given and never replaced by the ambient client.
- Credentials never appear in errors, `repr`, or `Debug` output during
  construction or connection.
- The Python lazy constructor wraps the shared `connect`; it does not
  reimplement transport setup.

## Decisions

- D1: Rust uses `connect()` / `connect_with(&client)` rather than
  `connect(Option)` or a builder, keeping the everyday call free of `None`.
- D2: eager means "complete the exchange the credential path already
  requires", with no health or token-validation probe.
- D3: Python handles are imported from their owning submodule; no new root
  exports.
- D4: `Platform` gets `connect` / `connect_with`, stays Rust-only, and keeps
  its platform-session exchange.
- D5: discovery is a flat `list_tables` plus lazy describe, not a nested
  catalog abstraction.

## Revisions and deviations

- Revisions 1–9 shaped the API, decisions D1–D5, discovery, and the
  no-client default. Revision 10 limits AC-002's saved-configuration-only case
  to handles with a saved credential source: `Platform` accepts only a platform
  bearer, so it is proven environment-only and with the no-credential refusal.
  Revision 11 proves Rust ambient cases through `WyrdClient::from_environment`
  with an injected `Environment`, the resolver `connect()` uses, instead of
  process-environment mutation.
- Delegated-connect attribution is asserted from retained audit rows
  (`sub_principal_id` is the subject, `act_principal_id` the actor), following
  the merged `sub-act-attribution` change.
- **Not delivered:** TASK-002 (docsite and README portion of REQ-012 and
  AC-006) was not implemented. The owner closed the change without it.
  Docsite pages and READMEs may still show the removed construction forms;
  that work needs a follow-up change.

## Acceptance closure

| Criterion | Evidence |
| --- | --- |
| AC-001 public surface, removed forms absent | Rust/Python/TypeScript public-surface tests in the SDK unit lanes |
| AC-002 no-client connect, environment and saved | `sdk_client_instantiation::ambient_connect_opens_every_handle` (Rust), `test_ambient_connect_opens_every_handle` (Python, lazy and eager incl. `AsyncBifrost`), TypeScript `ambient connect opens every handle` |
| AC-003 delegated attribution | `delegated_connect_uses_the_subject_principal` journeys in all three SDKs, asserted against retained audit |
| AC-004 lazy first use, eager refusal, exchange vs bearer | Python unit and integration instantiation tests; per-language `connect without a credential fails eagerly` and `credential paths preserve exchange and bearer behavior`; `saved_user_auth::revoked_login_is_refused` |
| AC-005 Python constructors do no IO | Python unit `test_local_access_never_connects` and constructor tests |
| AC-006 stubs, declarations, docs | Stubs and N-API declarations pass `py:lints`, `py:typecheck`, `codegen:check`, `ts:napi:check`; architecture Client model updated. Docsite/README portion not delivered (TASK-002) |
| AC-007 resolve once, cache reuse, renewal | `connected_handles_reuse_the_client_cache_and_renew` (Rust), `test_handles_reuse_the_client_cache_and_renew` (Python), TypeScript `handles reuse the client cache and renew` |
| AC-008 discovery journey | `bifrost_lists_tables_and_describes_on_demand` (Rust) and Python/TypeScript equivalents |
| AC-009 discovery names and typed projections | Public-surface and generated-type tests in each SDK |

## Links

- Architecture: [Wyrd design — Client model](../../../architecture/wyrd-design.md#client-model)
- Rust client owner: [`crates/shared/wyrd-client`](../../../crates/shared/wyrd-client)
- SDKs: [`sdks/wyrd-sdk-rust`](../../../sdks/wyrd-sdk-rust),
  [`sdks/wyrd-sdk-python`](../../../sdks/wyrd-sdk-python),
  [`sdks/wyrd-sdk-ts`](../../../sdks/wyrd-sdk-ts)
- Journeys:
  [Rust](../../../sdks/wyrd-sdk-rust/tests/integration/sdk_client_instantiation.rs),
  [Python](../../../sdks/wyrd-sdk-python/tests/integration/test_sdk_client_instantiation.py),
  [TypeScript](../../../sdks/wyrd-sdk-ts/wyrd/tests/integration/sdk-client-instantiation.test.ts)
- Related: [sub-act-attribution](sub-act-attribution.md)
