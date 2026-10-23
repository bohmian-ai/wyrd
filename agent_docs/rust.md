# Rust

Load when writing or reviewing Rust. The root `AGENTS.md` states the short
rules; this guide gives the shape behind them. Generic Rust idiom is assumed.

## Structure: struct-centered, not functional

Stateful capabilities and invariant-bearing behavior live on one owning
concrete type. Pure functions stay free.

| Situation | Default |
|---|---|
| Shared clients, configuration, state, or runtime context | Service or handle struct with inherent methods |
| Domain identity or invariants | Domain struct or newtype with inherent methods |
| Multi-step workflow or lifecycle | Method on the struct that owns the dependencies |
| Closed set of behavioral variants | Enum with methods |
| Stateless deterministic transformation | Free function |
| Several real implementations with stable shared behavior | Small capability trait |
| Optional or phased construction | Builder |

Before adding a module-level function, ask: which type owns the identity or
invariant it changes; do sibling operations share its dependencies; would a
maintainer search for it on a type; is the result determined by its arguments
alone? If the first three name an owner, write a method. Passing every
dependency as an argument does not make orchestration stateless.

A free function may still coordinate multi-step or async work when all state
and lifetime stay caller-owned for that call. Do not invent a struct for that.

Keep three roles distinct:

| Role | Owns |
|---|---|
| Domain value | Construction, validation, invariants, pure transformations |
| Service or handle | Clients, stores, configuration, IO workflows |
| Pure helper | One small deterministic module function |

```rust
/// Coordinates declaration planning and remote declaration IO.
pub struct Registry {
    client: WyrdClient,          // field docs elided here; required in code
    storage: StorageClient,
    policy: RegistrationPolicy,
}

impl Registry {
    /// Produces the declaration plan without IO.
    ///
    /// # Errors
    /// Returns `RegistryError` when the input violates policy or its manifest
    /// cannot be built.
    pub fn plan(&self, input: &DeclarationInput) -> Result<PreparedDeclaration, RegistryError> {
        self.policy.validate(input)?;
        Ok(PreparedDeclaration::new(input, validate_manifest(input)?))
    }

    /// Plans, uploads, and commits one declaration.
    ///
    /// # Errors
    /// Returns `RegistryError` when planning, upload, or commit fails.
    ///
    /// # Cancellation
    /// May stop after upload but before commit; the idempotency key makes
    /// retry safe.
    pub async fn declare(&self, input: &DeclarationInput) -> Result<DeclarationReceipt, RegistryError> {
        let prepared = self.plan(input)?; // pure work stays synchronous
        self.storage.upload(&prepared).await?;
        self.client.commit(&prepared).await
    }
}

/// Free because the result depends only on `input`.
fn validate_manifest(input: &DeclarationInput) -> Result<Manifest, RegistryError> { /* ... */ }
```

Reject the functional alternative, `async fn declare(client, storage, input)`
plus sibling free functions threading the same bundle. When the same
dependencies recur across functions, introduce the owner.

### Module shape

1. The primary domain, service, or handle type.
2. An inherent `impl` with the public workflow and cohesive private stages.
3. Supporting request, result, state, or closed-enum types.
4. Small private pure helpers.
5. In-module tests through the owning type.

No zero-sized `Utils` structs, no god objects, no anemic struct whose behavior
sits in module functions. Split a type when its methods stop sharing
dependencies, invariants, or one responsibility. Prefer precise owners
(`Cards`, `IngestWriter`, `QuerySession`) over `Manager`.

### Canonical owners

- `crates/shared/wyrd-client/src/cards/handle.rs::Cards`: public,
  dependency-owning handle with discoverable declare/resolve/list/load/delete
  methods, composed from a focused engine and narrow private helpers.
- `wyrd_client::Bifrost` owns its client capabilities over the crate's shared
  HTTP, gRPC, and auth transport. Bindings wrap it; they never publish its
  query or transport internals as parallel clients.
- Cards and specs stay declarative: construction, validation, pure
  transformation. Registry, storage, policy, and lifecycle IO lives on the
  handle that owns those dependencies.

## Types

- Durable identifiers are newtypes (`TenantId`, `PrincipalId`, `RunId`,
  `CardUid`, `CardName` in `wyrd_spec::ids` and `wyrd-runtime`).
  Never leak raw primitives across module boundaries.
- Closed sets are enums dispatched by `match`, not `Box<dyn Trait>`. See
  `crates/wyrd/wyrd-storage/src/signer.rs::BackendSigner`. No catch-all `_ =>`
  on an enum you own.
- `#[non_exhaustive]` only on library enums allowed to grow, never on a closed
  wire, persisted, or policy enum.
- A trait needs two real implementations. One implementation means a concrete
  type. Keep traits small and capability-focused; never a broad platform trait
  invented for one caller.
- Generics for hot-path static dispatch. `Box<dyn Trait>` only for intentional
  runtime extensibility.
- `pub(crate)` by default; `pub` only for the intended surface.

## Imports and signatures

- All `use` statements sit at the top of the module, after `//!`, before items.
  No function- or `impl`-scoped `use`. Exceptions: `use super::*;` inside
  `#[cfg(test)] mod tests`, and a rare `use Trait as _;` inside one generic
  function.
- Bring types in with `use` and write bare names in signatures, fields, bounds,
  and `where` clauses: `conn: &mut TenantConn<'_>`, not
  `conn: &mut vala_sql::TenantConn<'_>`.
- Import across tiers through the owning tier's re-exports
  (`use vala_sql::{TenantConn, OperatorPool, SqlError};`), never the underlying
  crate from outside it.

## Ownership and allocation

- `.clone()` is a design question. Acceptable: small identifiers at API
  boundaries, `Arc::clone`/`Bytes::clone` for shared state, owned response
  values, and whatever a crate's `CLONES.md` whitelists. Suspicious: large
  `Vec`, `HashMap`, schemas, serialized payloads, Python-owned objects, clones
  in loops.
- Do not derive `Clone` speculatively. Avoid `Arc<Mutex<T>>`; first try
  ownership, immutable state, a narrower lock, or message passing.
- Keep JSON conversion at API, storage, and Python boundaries; never
  round-trip through serde to move data between Rust layers.
- Map columns and fields by name, not positional index, whenever two schemas
  can diverge. Positional alignment silently corrupts data.

## Async

- Synchronous is the default. `async fn` must directly await IO or compose
  operations that do. Planning, validation, and transformation stay
  synchronous even under an async caller.
- Hold heavy shared dependencies in explicit state structs; never rebuild
  clients, pools, schemas, or runtimes per request. Keep locks off hot paths.
- Never block inside an async request path without an explicit blocking
  strategy.
- Bounded concurrency and timeouts on external calls. `FuturesUnordered` and
  `try_join_all` are not limits; bound the input first.
- No ad hoc Tokio runtimes in library or binding code; use `wyrd-runtime`.

### Recorded exception: Scribe CPU lanes

`vala-bifrost-redux/src/scribe/execution_lanes.rs` runs Scribe's sustained CPU
and WAL work on fixed, named, semaphore-admitted Rayon lanes, because it would
starve Tokio and `spawn_blocking` is unbounded. Do not copy the pattern; a new
lane is a design decision recorded in `architecture/bifrost-design.md`.

## Errors

- `thiserror` for crate-local enums with typed fields naming the operation,
  resource, or field; avoid `InvalidInput(String)`. `anyhow` only in binaries.
- Public errors use the `WyrdError` derive; see `agent_docs/errors.md`.
- No `unwrap()` outside tests. `expect()` only for a true invariant, with a
  message naming it.

## Lints

The workspace `[lints]` table in `Cargo.toml` is the policy: `clippy::pedantic`
with commented exceptions, `unwrap_used` (tests exempt via `clippy.toml`),
`allow_attributes_without_reason`, and `unsafe_code = "deny"`. `mise run lints`
runs it with `-D warnings`.

- Fix the lint. Suppress only as a last resort with
  `#[expect(lint, reason = "...")]` on the narrowest item. Use
  `#[allow(..., reason = "...")]` only when the lint fires in some build
  configurations, wrapped in `cfg_attr` when expressible.
- `cast_possible_wrap`/`cast_possible_truncation`/`cast_sign_loss`: use
  `i64::try_from`, `num_traits::ToPrimitive`, or a range-carrying newtype.
  Never `as`-and-suppress. (`cast_precision_loss` is allowed workspace-wide.)
- `missing_panics_doc`: document the invariant under `# Panics`, or better,
  return `Result`.
- A lint that does not fit the codebase is changed in workspace policy after
  review, never papered over at one site.

## Build and features

- Workspace Clippy and type-check lanes use `--all-features` so every path is
  checked. Test and build tasks declare only the minimal feature set;
  `--all-features` in a test task recompiles the heavy cone at another
  feature union and defeats artifact reuse.
- Cargo features must be earned. A new feature changes the feature union and
  invalidates compilation caches that `mise` tasks are tuned to share.
- `build.rs` writes only to `OUT_DIR`, never the source tree; writing a tracked
  file refingerprints the crate on every build.

## Server handlers

- Errors leave through the single `IntoResponse` mapper; a second impl for the
  same error type does not compile.
- Never construct clients or pools inside a handler; take them from shared
  state. Never clone heavy state per request.

## Providers, evaluation, observability

- Provider-specific request and response details stay inside provider modules.
  Describe capabilities with typed values, not string checks.
- Assertion-defined evaluation is deterministic. Model-based judging is
  isolated from assertion logic and tested against mock provider responses.
- Trace and observation identifiers are typed and validated.
- Archival and query code states its tenant, time range, projection, pruning,
  and retention assumptions explicitly.
- Unit tests use local mock servers and fixtures, never live provider calls or
  credentials.

## Diagnostics and secrets

- `tracing` with structured fields. Write handlers carry
  `#[tracing::instrument(skip(state, body), fields(...))]` with scrubbed args.
- Secrets are `secrecy::SecretString`; a secret-bearing struct has a custom
  `Debug` that prints `<redacted>`.

## Rustdoc

Every new or materially modified item needs rustdoc regardless of visibility,
including fields, variants, private helpers, test helpers, and tests. It says
why the item exists and which workflow it serves, how it works well enough to
change safely, and which invariants and side effects matter. Fallible
functions have `# Errors`; possible panics have `# Panics`; async or durable
operations document cancellation, partial progress, idempotency, and retry.
Placeholder docs that restate the name are a blocker.

## Permanent code only

Never reference plans, tasks, task IDs, reviews, or agents in code, comments,
or test names. Use configuration or runtime parameters. A later maintainer has
none of that context.

## Tests

- Inline tests in the owning `src/` module by default. An external `tests/`
  file must wire two or more non-foundational crates, stand up a real
  server/gRPC/HTTP surface or the compiled binary, or drive a real external
  service.
- Postgres-, Docker-, or server-backed tests live in `mod pg_tests` or a
  `pg_*` file so the fast lane stays credential- and Docker-free.
- Assert Wyrd error codes, not messages; `#[should_panic]` always names its
  `expected` substring.
- A Rust test that needs Python or Node for non-binding behavior means the
  boundary is misplaced.
