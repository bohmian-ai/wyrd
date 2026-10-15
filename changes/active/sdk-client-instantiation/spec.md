---
id: SPEC-sdk-client-instantiation
revision: 10
status: approved
---

# One way to instantiate every SDK handle

## Objective and user value

A developer who knows how to open one Wyrd handle in one language knows how to
open every handle in every first-class language. Today the same handle has a
different construction verb in each SDK:

| Handle | Rust | Python | TypeScript |
| --- | --- | --- | --- |
| `Bifrost` | `from_env()`, `connect(&client)` | `Bifrost(client=None)` | `Bifrost.connect({ client? })` |
| `Cards`, `Gateway`, `Principals`, `OperatorConnections` | `from_env()`, `with_client(client)` | `X(client=None)` | `X.connect({ client? })` |
| `Platform` | `with_client(&client)` only | absent | absent |

Documentation for one task shows three different shapes, readers assume the
SDKs disagree on behavior, and agents generating code for one language guess
wrong for another. This change makes `connect` the one instantiation form for
every server-facing handle in Rust, Python, and TypeScript, with the client
optional everywhere. Python additionally keeps its constructor, made lazy,
because it is what most Python users write.

The common usage supplies no client: handles instantiate from the environment
or saved configuration through Wyrd's shared ambient resolver. An explicit
`WyrdClient` is always optional and is primarily for testing, advanced
configuration, or delegated identity. Supplying handle-specific options never
makes a client mandatory.

The same Bifrost handle also exposes compact, authorized table discovery in
all three SDKs. Callers list the tables they can query, then request a table's
schema only when they need it. This supports both programmatic exploration and
the query workbench without inventing a second catalog model.

## Requirements

### The common form

- **REQ-001** Every public server-facing handle in the Rust, Python, and
  TypeScript SDKs is instantiated with a function named `connect`. The handles
  in scope are listed in [Scope](#scope-and-non-goals).
- **REQ-002** `connect` accepts an optional `WyrdClient`. When the caller
  provides none, the handle resolves its endpoint and credential through the
  same ambient resolution `WyrdClient` uses (saved configuration, then
  environment, then saved login), identically in all three languages.
  Omitting the client is the normal usage for every in-scope handle. Callers
  do not need to construct a `WyrdClient` or read environment variables
  themselves. An explicit client is an optional testing, advanced-configuration,
  or delegation form, including when other handle options are supplied.
  This does not change the shared resolver's existing precedence.
- **REQ-003** `connect` is eager. Before it returns, it resolves the endpoint
  and credential, completes any exchange or refresh required by the existing
  credential path, and establishes any persistent transport the handle owns.
  Configuration failures, required exchange failures, and unreachable required
  transports fail at `connect` with their existing stable errors. A supplied
  bearer token or fresh cached token is reused without an extra validation
  request; rejection may therefore occur on the first protected operation.
  Successful `connect` does not guarantee permission for a later operation.
- **REQ-004** Handle-specific construction options (for example Bifrost's
  table configuration and client byte limit) are accepted by `connect`
  alongside the optional client, using each language's idiom for optional
  arguments: Python keyword arguments, a TypeScript options object, and in
  Rust the D1 forms with an options argument where a handle has options.
- **REQ-005** Sync and async may differ by language and handle. A `connect`
  that performs network IO is awaitable in Rust and TypeScript and in Python's
  async handles (`AsyncBifrost.connect`); Python's synchronous handles connect
  synchronously. The name, the optional client, and the eager semantics do not
  differ.
- **REQ-014** Every server-facing handle holds exactly one `WyrdClient` in its
  state, set once for the handle's lifetime: the client the caller passed, or
  the client created from the ambient configuration. `Platform` retains the
  session-bound client produced by its existing platform-session exchange from
  that selected client (D4). The handle never re-resolves the ambient
  configuration per call, and every call on it reuses
  that client, including its credential exchange and token cache. Existing
  refresh, expiry, and refusal-triggered re-exchange behavior is preserved;
  once-only initialization does not prohibit later token renewal.
- **REQ-015** For a handle without a persistent transport (`Cards`, `Gateway`,
  `Principals`, `OperatorConnections`), `connect` sets the client and completes
  any credential exchange or refresh the existing client path requires. A
  credential rejected during that exchange fails at `connect` (REQ-003),
  without a separate health or token-validation call. Supplied bearer and
  fresh cached tokens need no exchange. A handle with a persistent transport
  (`Bifrost`) also establishes that transport; it adds no authentication probe.

### Python's lazy constructor

- **REQ-006** Every Python handle that has `connect` also keeps a public
  constructor with the same optional arguments. Both the eager factory and
  lazy constructor normally omit the client: `Cards.connect()` / `Cards()`,
  `Principals.connect()` / `Principals()`, and `Bifrost.connect()` / `Bifrost()`.
  `AsyncBifrost()` and `await AsyncBifrost.connect()` also omit it by default.
  Every form additionally accepts `client=client`; none requires it.
- **REQ-007** The Python constructor performs no IO. It holds its arguments,
  including the optional client. On the first operation that needs the server,
  the handle resolves the ambient configuration, creates the client, connects,
  and then runs the operation; this happens once per handle. Laziness never
  causes a failure of its own. The only errors that call can raise beyond its
  own are the configuration errors any call would hit without a usable
  configuration or connection path under REQ-003, with the same stable errors.
  Rejection of a reused bearer by the operation remains that operation's error.
- **REQ-008** A handle made by the constructor and a handle made by `connect`
  behave identically once the first server operation succeeds. They differ only
  in when a configuration or connection error is raised.
- **REQ-009** `AsyncBifrost()` no longer blocks the event loop during
  construction; it is lazy like every other Python constructor, and
  `await AsyncBifrost.connect()` is its eager form.

### Removal of the other forms

- **REQ-010** The forms the common form replaces are removed, with no aliases:
  Rust `from_env()` and `with_client(..)` on handles in scope, Rust
  `connect(&client)` with a required client, and Rust
  `connect_with_table` / `connect_with_config`, whose options move into
  `connect` per REQ-004.
- **REQ-011** TypeScript keeps private constructors; `connect` is its only
  public instantiation form. Rust has no lazy constructor; `connect` is its
  only public instantiation form.

- **REQ-016** The module that owns `Principals` is named `auth`: Python
  `wyrd.principals` becomes `wyrd.auth`, and Rust `wyrd_ai::principals`
  becomes `wyrd_ai::auth`, with no alias at the old path. The class keeps the
  name `Principals`. TypeScript exports from its package root and is
  unchanged.

### Bifrost table discovery

- **REQ-017** Every first-class Bifrost surface projects the existing table
  listing operation on the connected handle: Rust async and blocking
  `Bifrost::list_tables()`, Python `Bifrost.list_tables()` and
  `AsyncBifrost.list_tables()`, and TypeScript `Bifrost.listTables()`. The
  operation takes no catalog or filter argument and uses the handle's retained
  client, authentication, tenant, and error path.
- **REQ-018** A listing returns the server's existing authorized
  `BifrostTableEntry` values unchanged and without schema fields. Callers use
  the existing `describe_table` / `describeTable` operation to fetch one
  table's schema and layout on demand. Listing never eagerly describes each
  result and adds no client cache, aggregation, or durable behavior.
- **REQ-019** Discovery is typed in every SDK: Rust returns the existing wire
  struct, Python exposes `BifrostTableEntry` as a `TypedDict`, and TypeScript
  exposes it as an interface. Their public fields match the existing wire
  contract; no new server route or wire type is introduced.

### Documentation and agents

- **REQ-012** Every docs-site page, README, example, and generated SDK reference
  instantiates handles with `connect`. Python tabs may use the constructor;
  setup and smoke-test examples use `connect`, where failing fast is the point.
  Python examples import each handle from its owning submodule (D3).
  Lead with no-client usage in all three languages and state that the shared
  resolver reads environment and saved configuration automatically. Present
  explicit-client forms separately as optional testing, advanced-configuration,
  or delegation examples. Paired examples must not imply that one handle
  requires a client while another does not. The docsite setup guide and root
  README explain this default and link to focused guidance where appropriate;
  update other relevant pages and READMEs without duplicating a full guide.
- **REQ-013** `architecture/wyrd-design.md` (Client model) and the language
  references state the rule: one instantiation verb across first-class SDKs,
  optional client with ambient resolution, eager `connect`, and Python's lazy
  constructor as the single sanctioned addition. No-client ambient
  instantiation is the documented default; explicit clients remain optional
  for every in-scope handle and every supported construction form.

## Invariants

- **INV-001** No handle gains or loses capability except that Bifrost projects
  the server's existing table-list operation. No permission, server behavior,
  or durable behavior changes.
- **INV-002** Ambient resolution stays in `wyrd-client`. No SDK implements its
  own endpoint or credential lookup; all three call the shared resolver, so the
  same configuration yields the same endpoint and credential in every language.
- **INV-003** A provided `WyrdClient` is used as given, including a delegated
  client from `on_behalf_of` / `onBehalfOf`. A handle never replaces it with the
  ambient client. Platform's existing derivation of a session-bound client
  from the selected platform credential is preserved under D4.
- **INV-004** Credentials never appear in error messages, `repr`, or `Debug`
  output produced during construction or connection.
- **INV-005** Language-specific code stays limited to what a foreign-runtime
  boundary earns. The lazy Python constructor wraps the shared `connect`; it
  does not reimplement transport setup.

## Scope and non-goals

In scope, every public handle that calls the server:

- `Bifrost` (Rust async and `bifrost::blocking`, Python `Bifrost` and
  `AsyncBifrost`, TypeScript)
- `Cards`
- `Gateway`
- `Principals`
- `OperatorConnections`
- `Platform` (Rust only; see D4)
- Bifrost table discovery through the existing compact list and lazy describe
  operations in Rust, Python, and TypeScript

Out of scope:

- **Local loaders.** `WyrdState.from_path` and `Workflow.from_path` /
  `from_yaml` load a local bundle or document and make no connection at load
  time. They keep `from_path` / `from_yaml` as their common form, with the same
  optional client in every language. Rust's
  `from_path_with_client` becomes `from_path_with`, per D1.
- **`WyrdClient` construction.** How a `WyrdClient` itself is built
  (`from_global`, `with_config`, Python's constructor, TypeScript's
  `WyrdClient.connect`) is a related inconsistency but a separate decision.
  This change only requires that omitting the client resolves the same way
  `WyrdClient`'s ambient path does.
- **Derived handles.** `Run`, `Observe`, and `WorkflowCards`, which are reached
  from another handle and never constructed directly.
- **`TableConfig`.** It is a value, not a connected handle. Its `describe`
  read already takes an optional client in Python and TypeScript; Rust's
  `describe_from_env` and `describe(&client, ..)` become `describe` and
  `describe_with`, per D1.
- **New handles.** Verification reads through the SDKs are a separate change.
- TypeScript's public `fromNative` and `fromOpen` statics marked `@internal`.
- A new catalog object, `list_catalog`, `list_schemas`, filtering, pagination,
  client caching, or eager per-table schema fetches.
- New Bifrost routes, wire models, permissions, audit behavior, or server-side
  discovery behavior.
- Query workbench UI changes. The UI may consume these SDK primitives, but its
  implementation remains in its own change.

## Expensive-to-reverse decisions

- Public SDK construction API in three languages. Wyrd has not shipped, so the
  old forms are removed rather than deprecated.
- `connect` is eager and the Python constructor is lazy. Reversing either
  changes when every existing caller sees configuration errors.
- The architecture authority changes from "surface ergonomics may differ" to a
  fixed instantiation verb.
- The discovery vocabulary is a flat `list_tables` / `listTables` operation
  returning compact entries, paired with the existing lazy table description.
  Replacing it later with a nested catalog model would change every SDK.

## Acceptance criteria

- **AC-001** In each SDK, a public-surface test enumerates the in-scope handles
  and fails if any lacks `connect` with an optional client, or if a removed form
  (REQ-010) is still public.
- **AC-002** A user journey per language opens each in-scope handle with
  `connect()` and no client against `WyrdTestServer`, in both environment-only
  and saved-configuration-only cases, and completes one server call. Supplying
  handle-specific options does not require supplying a client. Python's lazy
  constructors also support both no-client cases on first server use.
  The saved-configuration-only case applies to handles whose credential has a
  saved source: the tenant handles. `Platform` accepts only a platform bearer,
  which has no saved source (D4), so its journey covers the environment-only
  case and the no-credential refusal through `Platform::connect()`.
- **AC-003** A user journey per language opens a handle with `connect` and a
  delegated client and shows the server attributes the call to the delegated
  principal (INV-003).
- **AC-004** A Python journey constructs each in-scope handle with no client
  and no IO, then completes a first server call that resolves the ambient
  configuration and creates the client. A negative journey per language shows
  `connect()` with no resolvable credential fails at `connect` with the stable
  error, and the Python constructor's first server call raises that same
  error. Journeys also distinguish a credential rejected during a required
  exchange, which fails at `connect`, from a supplied invalid bearer on an
  HTTP-only handle, which connects without a validation request and is refused
  on its first protected operation. Python's lazy path preserves the same
  error codes. Reusing a fresh cached token causes no extra exchange.
- **AC-005** A Python test shows constructing `Bifrost()` and `AsyncBifrost()`
  with no server reachable performs no network IO and returns.
- **AC-006** `mise run docs:check` passes with every docs-site handle
  instantiation in the common form. Authored package-local Python `.pyi`
  stubs are updated directly and pass `mise run py:lints` and
  `mise run py:typecheck`. Generated TypeScript wire types and N-API
  declarations pass `mise run codegen:check` and `mise run ts:napi:check`,
  respectively. Static review of the docsite, root README, and relevant other
  READMEs confirms that primary handle examples omit the client, environment
  and saved configuration are resolved automatically, explicit-client examples
  are clearly optional, and Python's eager/lazy examples preserve the same
  optionality. Existing precedence, eager-failure qualifications, and local
  loader behavior are described accurately.
- **AC-007** A test per language shows a handle created without a client
  resolves the ambient configuration once: changing the configuration after the
  first call does not change the endpoint or principal later calls use, and
  repeated calls reuse the initial exchange while its cached token is fresh
  (or perform no exchange for a supplied bearer or already-fresh cache).
  Existing renewal and refusal-triggered re-exchange remain available through
  that retained client; the test also proves renewal across token expiry for
  renewable credential paths. Platform keeps its existing session lifetime.
- **AC-008** A user journey per language registers a user table, lists it and
  the visible built-in tables through Bifrost, excludes a table belonging only
  to another tenant, and receives the existing stable authorization error when
  the principal lacks `bifrost_table:read`. The journey then describes the
  user table separately and receives its declared fields.
- **AC-009** Public-surface and generated-type tests prove the exact discovery
  method names and typed `BifrostTableEntry` projections in Rust, Python, and
  TypeScript. Compact list entries contain no schema, and no `list_catalog` or
  `list_schemas` compatibility surface exists.

## Decided

- **D1. How Rust spells the optional client.** `X::connect()` resolves the
  ambient configuration; `X::connect_with(&client)` uses the given client. One
  verb, two forms, because Rust has no optional arguments. The same pair
  applies to `from_path` / `from_path_with` and `TableConfig::describe` /
  `describe_with`. Rejected: `connect(None)` / `connect(Some(&client))`, which
  puts a `None` on every everyday call, and a builder, which adds the most
  ceremony to the common case.

- **D2. What "eager" means for HTTP-only handles, and where the client
  lives.** Every server-facing handle holds one `WyrdClient`, set once
  (REQ-014). `connect` sets it and completes any exchange or refresh required
  by the existing credential path (REQ-015). It reuses supplied bearer and
  fresh cached tokens without probing the server. Rejected: deferring a
  required exchange until the first operation, or adding a health or
  token-validation request to force network IO for every credential kind.

- **D3. Python import location.** Each Python handle is imported from the
  submodule that owns it: `wyrd.bifrost`, `wyrd.cards`, `wyrd.gateway`,
  `wyrd.auth`, `wyrd.operators`. This change adds no root exports, and
  docs import from the submodule (REQ-012). The existing root re-exports of
  `Cards` and `WyrdState` are out of scope. Rejected: exporting every handle
  from the `wyrd` root, which flattens the logical grouping.

- **D4. `Platform`.** It gets `Platform::connect()` and
  `Platform::connect_with(&client)` like every other Rust handle, resolving the
  platform credential through the same ambient path, and stays Rust-only.
  Its existing platform-session exchange and session-bound client are
  preserved; it does not exchange platform credentials on the tenant path.
  Projecting it to Python and TypeScript is a separate change. Rejected:
  excluding it, which would leave the one Rust handle on `with_client`.

- **D5. Bifrost discovery shape.** Discovery is `list_tables` in Rust and
  Python and `listTables` in TypeScript, directly on the existing Bifrost
  handle. It returns the existing compact `BifrostTableEntry`; callers use the
  existing describe operation when they need schema. Rejected: `list_catalog`
  or a nested catalog/schema abstraction, which would duplicate server nouns
  and encourage an eager N+1 describe workflow.

## Open material decisions

None. D1 through D5 are resolved; see *Decided*.

## Revision history

| Revision | Status | Change |
| --- | --- | --- |
| 1 | draft | Initial draft: `connect` everywhere with an optional client, eager; Python constructor kept and made lazy. |
| 2 | draft | D1 decided: Rust `connect()` / `connect_with(&client)`. REQ-007 and AC-004 state that the lazy constructor resolves and creates the client on first use and adds no failure of its own. |
| 3 | draft | D2 decided: every server-facing handle holds one `WyrdClient` set once (REQ-014); `connect` completes the credential exchange (REQ-015). |
| 4 | draft | D3 decided: Python handles stay imported from their owning submodule; no new root exports. |
| 5 | draft | REQ-016: the module owning `Principals` is renamed `auth` in Python and Rust. |
| 6 | approved | D4 decided: `Platform` gets `connect` / `connect_with` and stays Rust-only. No open decisions remain. |
| 7 | approved | Added the agreed typed Bifrost table-list projection in all three SDKs, preserving the existing compact list plus lazy describe contract and rejecting a new catalog abstraction. |
| 8 | approved | User-requested repository-alignment revisions: authored Python stubs and their actual checks, current `wyrd_ai` module paths, eager exchange qualified by credential kind, retained-cache renewal, and preservation of Platform's existing session exchange. Public construction and discovery scope unchanged. |
| 9 | approved | User clarification: client is always optional, no-client environment/saved-configuration instantiation is the common usage, and explicit clients are for testing, advanced configuration, or delegation. Require environment-only and saved-only coverage and focused docsite/README guidance. Resolver precedence and API scope unchanged. |
| 10 | approved | User decision on review finding FIND-TASK-001-6: AC-002's saved-configuration-only case applies to handles with a saved credential source. `Platform` proves environment-only and no-credential `connect()`; no saved platform credential source is added. |

## Authority

- [AGENTS.md](../../../AGENTS.md) §2 (first-class SDKs over `wyrd-client`) and
  §9 (no compatibility aliases).
- [Wyrd design, Client model](../../../architecture/wyrd-design.md) (amended by
  REQ-013).
- [Python API and stubs](../../../architecture/references/languages/python-api-and-stubs.md)
  ("Avoid hidden IO in constructors", satisfied by REQ-007).
- [TypeScript guide](../../../architecture/references/languages/typescript-guide.md)
  (`Bifrost.connect({ client })`).
- [Patterns](../../../architecture/references/architecture/patterns.md)
  (`Cards` as the canonical handle).
