---
id: SPEC-sdk-client-instantiation
revision: 6
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

## Requirements

### The common form

- **REQ-001** Every public server-facing handle in the Rust, Python, and
  TypeScript SDKs is instantiated with a function named `connect`. The handles
  in scope are listed in [Scope](#scope-and-non-goals).
- **REQ-002** `connect` accepts an optional `WyrdClient`. When the caller
  provides none, the handle resolves its endpoint and credential through the
  same ambient resolution `WyrdClient` uses (saved configuration, then
  environment, then saved login), identically in all three languages.
- **REQ-003** `connect` is eager. Before it returns, it resolves the endpoint
  and credential and establishes any persistent transport the handle owns.
  A missing endpoint, missing or unusable credential, or unreachable transport
  fails at `connect` with the existing stable error for that condition, not at
  a later call.
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
  the client created from the ambient configuration. The handle never
  re-resolves the ambient configuration per call, and every call on it reuses
  that client, including its credential exchange and token cache.
- **REQ-015** For a handle without a persistent transport (`Cards`, `Gateway`,
  `Principals`, `OperatorConnections`), `connect` sets the client and completes
  the credential exchange the handle needs, so a rejected credential fails at
  `connect` (REQ-003) without a separate health call. A handle with a
  persistent transport (`Bifrost`) also establishes that transport.

### Python's lazy constructor

- **REQ-006** Every Python handle that has `connect` also keeps a public
  constructor with the same optional arguments, for example `Bifrost()` and
  `Cards(client=None)`.
- **REQ-007** The Python constructor performs no IO. It holds its arguments,
  including the optional client. On the first operation that needs the server,
  the handle resolves the ambient configuration, creates the client, connects,
  and then runs the operation; this happens once per handle. Laziness never
  causes a failure of its own. The only errors that call can raise beyond its
  own are the configuration errors any call would hit without a usable
  configuration (no resolvable endpoint or credential, a rejected credential,
  an unreachable server), with the same stable error `connect` would raise.
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
  `wyrd.principals` becomes `wyrd.auth`, and Rust `wyrd_sdk::principals`
  becomes `wyrd_sdk::auth`, with no alias at the old path. The class keeps the
  name `Principals`. TypeScript exports from its package root and is
  unchanged.

### Documentation and agents

- **REQ-012** Every docs-site page, README, example, and generated SDK reference
  instantiates handles with `connect`. Python tabs may use the constructor;
  setup and smoke-test examples use `connect`, where failing fast is the point.
  Python examples import each handle from its owning submodule (D3).
- **REQ-013** `architecture/wyrd-design.md` (Client model) and the language
  references state the rule: one instantiation verb across first-class SDKs,
  optional client with ambient resolution, eager `connect`, and Python's lazy
  constructor as the single sanctioned addition.

## Invariants

- **INV-001** No handle gains or loses capability, permission, or durable
  behavior. Only how it is constructed changes.
- **INV-002** Ambient resolution stays in `wyrd-client`. No SDK implements its
  own endpoint or credential lookup; all three call the shared resolver, so the
  same configuration yields the same endpoint and credential in every language.
- **INV-003** A provided `WyrdClient` is used as given, including a delegated
  client from `on_behalf_of` / `onBehalfOf`. A handle never replaces it with the
  ambient client.
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

## Expensive-to-reverse decisions

- Public SDK construction API in three languages. Wyrd has not shipped, so the
  old forms are removed rather than deprecated.
- `connect` is eager and the Python constructor is lazy. Reversing either
  changes when every existing caller sees configuration errors.
- The architecture authority changes from "surface ergonomics may differ" to a
  fixed instantiation verb.

## Acceptance criteria

- **AC-001** In each SDK, a public-surface test enumerates the in-scope handles
  and fails if any lacks `connect` with an optional client, or if a removed form
  (REQ-010) is still public.
- **AC-002** A user journey per language opens each in-scope handle with
  `connect()` and no client against `WyrdTestServer`, using saved configuration
  only, and completes one server call.
- **AC-003** A user journey per language opens a handle with `connect` and a
  delegated client and shows the server attributes the call to the delegated
  principal (INV-003).
- **AC-004** A Python journey constructs each in-scope handle with no client
  and no IO, then completes a first server call that resolves the ambient
  configuration and creates the client. A negative journey per language shows
  `connect()` with no resolvable credential fails at `connect` with the stable
  error, and the Python constructor's first server call raises that same
  error.
- **AC-005** A Python test shows constructing `Bifrost()` and `AsyncBifrost()`
  with no server reachable performs no network IO and returns.
- **AC-006** `mise run docs:check` passes with every docs-site handle
  instantiation in the common form, and `mise run codegen:check` passes with
  regenerated Python stubs and TypeScript declarations.
- **AC-007** A test per language shows a handle created without a client
  resolves the ambient configuration once: changing the configuration after the
  first call does not change the endpoint or principal later calls use, and
  repeated calls perform one credential exchange.

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
  (REQ-014). `connect` sets it and completes the credential exchange
  (REQ-015). Rejected: resolving locally only, which would make `connect` no
  more eager than the lazy constructor for these handles.

- **D3. Python import location.** Each Python handle is imported from the
  submodule that owns it: `wyrd.bifrost`, `wyrd.cards`, `wyrd.gateway`,
  `wyrd.auth`, `wyrd.operators`. This change adds no root exports, and
  docs import from the submodule (REQ-012). The existing root re-exports of
  `Cards` and `WyrdState` are out of scope. Rejected: exporting every handle
  from the `wyrd` root, which flattens the logical grouping.

- **D4. `Platform`.** It gets `Platform::connect()` and
  `Platform::connect_with(&client)` like every other Rust handle, resolving the
  platform credential through the same ambient path, and stays Rust-only.
  Projecting it to Python and TypeScript is a separate change. Rejected:
  excluding it, which would leave the one Rust handle on `with_client`.

## Open material decisions

None. D1 through D4 are resolved; see *Decided*.

## Revision history

| Revision | Status | Change |
| --- | --- | --- |
| 1 | draft | Initial draft: `connect` everywhere with an optional client, eager; Python constructor kept and made lazy. |
| 2 | draft | D1 decided: Rust `connect()` / `connect_with(&client)`. REQ-007 and AC-004 state that the lazy constructor resolves and creates the client on first use and adds no failure of its own. |
| 3 | draft | D2 decided: every server-facing handle holds one `WyrdClient` set once (REQ-014); `connect` completes the credential exchange (REQ-015). |
| 4 | draft | D3 decided: Python handles stay imported from their owning submodule; no new root exports. |
| 5 | draft | REQ-016: the module owning `Principals` is renamed `auth` in Python and Rust. |
| 6 | approved | D4 decided: `Platform` gets `connect` / `connect_with` and stays Rust-only. No open decisions remain. |

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
