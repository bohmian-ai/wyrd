# Rust Core Practices

Wyrd's Python, TypeScript, and server APIs are only as reliable as the Rust
core underneath them. Design Rust code first as a clean, testable library,
then expose the right boundary to Python, HTTP, MCP, CLI, or TypeScript.

## API Shape

Prefer APIs that make invalid states difficult to represent:

- Use domain types instead of raw strings for durable identifiers.
- Use enums for closed sets of states, kinds, operations, providers,
  backends, and variants.
- Use structs with named fields for meaningful records.
- Use traits for behavior shared across real implementations.
- Keep public functions explicit about inputs, outputs, validation, and error
  behavior.

Do not shape Rust APIs around what is easiest to extract from Python or JSON.
Convert at the edge, then call Rust-native functions.

## Required Structural Style

Wyrd's required Rust style is a hybrid centered on cohesive, composable
structs. Stateful capabilities and domain behavior live on an owning type.
Pure functions remain appropriate for work that is entirely determined by its
arguments and has no natural owner.

| Situation | Required default |
|---|---|
| Shared clients, configuration, state, or runtime context | Concrete service or handle struct with inherent methods |
| Domain identity or invariants | Domain struct or newtype with inherent methods |
| Multi-step workflow or lifecycle | Method on the struct that owns the dependencies and lifecycle |
| Closed set of behavioral variants | Enum with methods |
| Stateless deterministic transformation | Free function |
| Multiple real implementations with stable shared behavior | Small capability trait |
| Optional or phased construction | Builder struct |

Before adding a module-level function, ask:

1. Which type owns the identity or invariant this behavior changes?
2. Do sibling operations use the same dependencies or configuration?
3. Would a maintainer naturally search for this behavior on a domain type,
   service, or handle?
4. Is the result deterministic from the function arguments alone?

If the first three answers identify an owner, implement the behavior as a
method. Passing every dependency as an argument does not make orchestration
stateless.

### Canonical composition

This example deliberately includes both paths. `plan` is the synchronous
default for deterministic work. `register` earns async because it composes
storage and network IO. Every item is documented, including private fields,
private methods, and the free helper.

```rust
/// Coordinates deterministic registration planning and remote registration IO.
pub struct Registry {
    /// Typed client used to commit prepared registrations to the server.
    client: WyrdClient,
    /// Artifact client used to transfer prepared registration artifacts.
    storage: StorageClient,
    /// Local policy used to validate and normalize registration inputs.
    policy: RegistrationPolicy,
}

impl Registry {
    /// Creates a registry from its explicit transport, storage, and policy dependencies.
    #[must_use]
    pub fn new(
        client: WyrdClient,
        storage: StorageClient,
        policy: RegistrationPolicy,
    ) -> Self {
        Self {
            client,
            storage,
            policy,
        }
    }

    /// Produces the deterministic registration plan without performing IO.
    ///
    /// The method validates the authored input against this registry's policy,
    /// constructs its artifact manifest, and returns the prepared request used
    /// by the asynchronous registration boundary.
    ///
    /// # Errors
    ///
    /// Returns `RegistryError` when the input violates registration policy or
    /// its artifact manifest cannot be constructed.
    pub fn plan(
        &self,
        input: &RegistrationInput,
    ) -> Result<PreparedRegistration, RegistryError> {
        self.policy.validate(input)?;
        let manifest = validate_manifest(input)?;
        Ok(PreparedRegistration::new(input, manifest))
    }

    /// Plans, uploads, and commits one registration through remote IO.
    ///
    /// Planning remains synchronous. After planning succeeds, the method
    /// uploads declared artifacts and awaits the server commit.
    ///
    /// # Errors
    ///
    /// Returns `RegistryError` when local planning fails, an artifact upload
    /// fails, or the server rejects or cannot commit the registration.
    ///
    /// # Cancellation
    ///
    /// Cancellation may stop the client after an upload has completed but
    /// before commit returns. The server-side idempotency key makes retry safe.
    pub async fn register(
        &self,
        input: &RegistrationInput,
    ) -> Result<RegistrationReceipt, RegistryError> {
        let prepared = self.plan(input)?;
        self.upload_and_commit(&prepared).await
    }

    /// Uploads a prepared registration and commits it to the server.
    ///
    /// This method is async because both storage transfer and server commit are
    /// remote IO operations. It does not absorb synchronous planning work.
    ///
    /// # Errors
    ///
    /// Returns `RegistryError` when artifact transfer or server commit fails.
    ///
    /// # Cancellation
    ///
    /// Cancellation can occur between upload and commit. Retrying the prepared
    /// registration is safe because it retains the same idempotency key.
    async fn upload_and_commit(
        &self,
        prepared: &PreparedRegistration,
    ) -> Result<RegistrationReceipt, RegistryError> {
        self.storage.upload(prepared).await?;
        self.client.commit(prepared).await
    }
}

/// Builds the deterministic artifact manifest for one registration input.
///
/// The helper remains free because its result depends only on `input` and it
/// has no service state, dependency, identity, or invariant-bearing owner.
///
/// # Errors
///
/// Returns `RegistryError` when an artifact path or digest is invalid.
fn validate_manifest(input: &RegistrationInput) -> Result<Manifest, RegistryError> {
    Manifest::try_from(input)
}
```

The synchronous method is the default and is directly testable without an
executor. The async methods are limited to the layer that awaits actual IO.
The owning struct exposes the workflow and composes its dependencies. Private
methods break the workflow into cohesive stateful stages. Free functions handle
pure validation or transformation.

Reject functional dependency threading:

```rust
/// Registers one input by threading dependencies through a free workflow.
///
/// # Errors
///
/// Returns `RegistryError` when upload or commit fails.
pub async fn register(
    client: &WyrdClient,
    storage: &StorageClient,
    input: &RegistrationInput,
) -> Result<RegistrationReceipt, RegistryError> {
    upload(client, storage, input).await?;
    commit(client, input).await
}
```

When the same dependency bundle appears across workflow functions, introduce
the owning service or handle instead.

### Module shape

A struct-centered module normally contains:

1. Its primary domain, service, or handle type.
2. An inherent `impl` containing the public workflow and cohesive private
   stages.
3. Supporting request, result, state, or closed-enum types.
4. Small private pure helpers.
5. In-module tests through the owning type, plus direct tests for meaningful
   pure helpers.

Do not force unrelated functions onto a type. A zero-sized `Utils` struct is
functional organization wearing method syntax. Conversely, do not leave a
struct anemic while its natural behavior is scattered across module-level
functions.

Split a service when groups of methods do not share dependencies, invariants,
or one responsibility. Prefer precise owners such as `Cards`, `Registry`,
`IngestWriter`, or `QuerySession` over broad `Manager` objects.

### Wyrd Card boundary

Struct-centered design does not turn Wyrd Cards into active-record objects.
Card envelopes and specs own declarative construction, validation, and pure
transformation. Registry, storage, policy, and lifecycle IO stays on the
service or handle that owns those dependencies. The canonical reference is
`crates/shared/wyrd-registry/src/handle.rs::Cards`.

### Newtype identifiers

```rust
/// Identifies the tenant that owns a durable Wyrd resource.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct TenantId(uuid::Uuid);

impl TenantId {
    /// Wraps a validated UUID in the tenant identity domain type.
    #[must_use]
    pub fn new(id: uuid::Uuid) -> Self {
        Self(id)
    }

    /// Borrows the UUID used for wire and persistence boundaries.
    #[must_use]
    pub fn as_uuid(&self) -> &uuid::Uuid {
        &self.0
    }
}
```

Real examples: `TenantId`, `PrincipalId`, `RunId`, `CardUid`, `CardName`,
`SpaceName` in `wyrd-spec::ids` and `wyrd-runtime`.

### Enum dispatch over a closed set

```rust
/// Selects one of the storage backends supported by this deployment.
pub enum StorageBackend {
    /// Stores objects in Amazon S3.
    S3(S3Client),
    /// Stores objects in Google Cloud Storage.
    Gcs(GcsClient),
    /// Stores objects in Azure Blob Storage.
    Azure(AzureClient),
    /// Stores objects on the local filesystem.
    Local(LocalStore),
}

impl StorageBackend {
    /// Writes one object through the selected backend's remote or filesystem IO.
    ///
    /// # Errors
    ///
    /// Returns `StorageError` when the selected backend cannot persist the
    /// object.
    pub async fn put(&self, key: &str, bytes: Bytes) -> Result<(), StorageError> {
        match self {
            Self::S3(c)    => c.put(key, bytes).await,
            Self::Gcs(c)   => c.put(key, bytes).await,
            Self::Azure(c) => c.put(key, bytes).await,
            Self::Local(c) => c.put(key, bytes).await,
        }
    }
}
```

Enum dispatch is preferred over `Box<dyn StorageBackend>` when the set is
closed and the caller can be monomorphized. See `wyrd-storage` for the
canonical pattern.

### Traits for real polymorphism

Use traits only when multiple real implementations share behavior. Keep them
small.

```rust
#[async_trait::async_trait]
/// Records and flushes observation batches through a concrete observer.
pub trait Observer: Send + Sync + 'static {
    /// Records one observation batch through the implementation's IO boundary.
    ///
    /// # Errors
    ///
    /// Returns `ObserveError` when the batch cannot be accepted or persisted.
    async fn observe(&self, batch: ObservationBatch) -> Result<(), ObserveError>;

    /// Flushes observations buffered by the implementation.
    ///
    /// # Errors
    ///
    /// Returns `ObserveError` when buffered observations cannot be persisted.
    async fn flush(&self) -> Result<(), ObserveError>;
}
```

Real example: `crates/skald/skald-observer/src/lib.rs` (`Observer` trait with
`Otel`, `Composite`, and `Scoped` implementations).

Avoid platform traits invented for a single caller. If there is only one impl
today and no clear second impl on the horizon, use a concrete type.

## Zero-Cost Abstractions

Prefer abstractions that compile down to direct code. Rust rewards
compile-time specialization; pay runtime cost only when you need runtime
extensibility.

- **Generic functions with trait bounds** when the caller can be
  monomorphized. Each concrete instantiation is as fast as a hand-written
  specialization.
- **Enum dispatch** for a closed set of variants (see the `StorageBackend`
  example above). The compiler emits a direct branch; no vtable.
- **Iterators over intermediate `Vec`s.** Iterator chains fuse into a
  single tight loop through LLVM's optimizer.
- **Borrowed data (`&str`, `&Path`, `&[T]`)** instead of owned values.
  Zero allocation is the fastest allocation.
- **`Cow<'_, str>`** only when both borrowed and owned paths are real and
  the complexity pays for itself.
- **`impl Trait` in return position** for closures and iterators — hides
  the concrete type without a boxed indirection.

Reach for `Box<dyn Trait>` only when runtime extensibility is intentional
(plugin surface, heterogeneous collection, object-safe registry). Each
`dyn` call is a vtable dispatch; each `Box` is an allocation.

```rust
/// Parses every valid card name while discarding invalid inputs.
///
/// This zero-cost form is monomorphized for each iterator type.
#[must_use]
pub fn parse_all<I: IntoIterator<Item = &str>>(items: I) -> Vec<CardName> {
    items.into_iter().filter_map(|s| CardName::new(s).ok()).collect()
}

/// Parses valid card names through boxed runtime dispatch.
///
/// This rejected shape incurs one allocation per element and one vtable
/// dispatch per call.
#[must_use]
pub fn parse_all(items: Vec<Box<dyn AsRef<str>>>) -> Vec<CardName> { ... }
```

## Rust Idiom Best Practices

Follow the community and Wyrd-repo idioms the compiler and reviewers
expect. Reject un-Rust-y patterns even when they compile.

### Types

- **Newtype wrappers** for durable identifiers and units
  (`TenantId(Uuid)`, `SchemaFingerprint([u8; 32])`) — never leak raw
  primitives across module boundaries.
- **Exhaustive `match`** on enums; do not use catch-all `_ =>` when the
  compiler could tell you about a missing variant tomorrow.
- **`#[non_exhaustive]`** on public enums that may grow — forces
  downstream `match` arms to keep a fallback and prevents accidental
  breaking changes.
- **`#[must_use]`** on functions whose result must be observed
  (`Result`, iterators, builder terminals).
- **`Default`** only when the default is meaningful; do not derive it to
  silence a lint.

### Ownership

- **Return owned values by default** at API boundaries; take
  references where the callee doesn't need ownership.
- **`&self` methods** unless mutation or ownership transfer is required.
- **`Result<T, E>` over sentinel values** or panics. Errors are
  first-class values.
- **`Option<T>` over nullable pointers.** Absence is a value, not a bug.

### Traits

- **Prefer standard library traits** over custom shapes:
  `From`/`TryFrom` for conversion, `AsRef<T>`/`Borrow<T>` for cheap views,
  `Display` for user-facing text, `Debug` for developer output,
  `IntoIterator` for consumable sequences, `FromStr` for parsing.
- **Blanket impls for conversion.** `impl From<CardName> for String`
  gives you `let s: String = name.into();` for free.
- **Associated types over generics** when the type is one-per-impl:

  ```rust
  /// Produces one provider response for a typed request.
  pub trait Provider {
      /// Response type returned by this provider implementation.
      type Response;

      /// Produces a response using this provider's configured behavior.
      ///
      /// # Errors
      ///
      /// Returns `ProviderError` when the provider cannot satisfy the request.
      fn respond(&self, req: Request) -> Result<Self::Response, ProviderError>;
  }
  ```
- **Extension traits** for adding methods to types you don't own — keep
  the trait scoped and named `-Ext`.

### Iterators

- **Compose over collect.** `iter().filter(...).map(...).sum()` beats
  a loop that pushes into a `Vec<u64>` then sums it.
- **`try_fold` / `try_for_each`** for short-circuit on `Result`.
- **`itertools` crate** for `chunks`, `dedup`, `group_by`, `sorted_by`,
  `cartesian_product` — but only when the stdlib primitive is genuinely
  awkward.
- **`Vec::extend` over repeated `push`** in a loop.

### Pattern matching

- **`if let` / `let else`** for single-branch destructuring:

  ```rust
  let Some(uid) = card.uid() else {
      return Err(WyrdError::MissingUid { /* ... */ });
  };
  ```
- **`matches!` macro** for boolean checks against a pattern.
- **Guards (`if`) in match arms** for finer conditions.

### Modules and visibility

- **`pub(crate)` by default**, `pub` only for the intentional public
  surface. Every unnecessary `pub` widens the API contract you must
  maintain.
- **`mod` files describe the module surface**; keep implementation
  detail in submodules and re-export only the public shape.
- **`use` at the top of the file**, grouped: `std` first, external
  crates next, workspace/local last — `rustfmt` enforces this with
  `group_imports = "StdExternalCrate"` when the config is set.

### Constructors and builders

- **`fn new` for the canonical constructor** on Rust structs (unrelated
  to the PyO3 `fn __new__` rule for `#[pyclass]`).
- **Builder pattern** when a struct has ≥ 4 optional fields or when
  construction has meaningful phases; return `Self` from each setter
  and consume `self` on the terminal `build()`.
- **`Default` + struct-update syntax** (`Foo { field: x, ..Default::default() }`)
  is often simpler than a builder for optional-heavy configs.

### Async

- **Synchronous functions are the default.** An `async fn` must directly await
  IO or intentionally compose operations that do. Keep parsing, validation,
  planning, and transformations synchronous even when an async caller invokes
  them.
- **`async fn` in traits** (Rust 1.75+) for library-owned traits.
- **`Send` + `'static` bounds** on tasks that will cross a spawn
  boundary; the compiler will tell you when you forget.
- **`tokio::select!`** for concurrent branches with cancellation.
- **`FuturesUnordered` / `try_join_all`** for bounded fan-out.
- **`spawn_blocking`** for CPU-bound work inside an async runtime; never
  block the async worker.

### Error handling

- **`?` operator** for propagation; do not `match` a `Result` just to
  return it.
- **`#[from]` in thiserror** to auto-lift wrapped errors, but only when
  the wrapped variant is unique in the enum.
- **`anyhow::Context::context`** in binaries to add hints as errors
  bubble up.
- **Never `.unwrap()` a `Result` in production code** unless the
  invariant is documented and truly cannot fail.

### Testing

- **`assert_eq!` with descriptive types**; the compiler shows both sides
  in the failure message.
- **`#[should_panic(expected = "...")]`** with an explicit expected
  substring when testing panics; naked `#[should_panic]` masks
  regressions.
- **`insta` or `expect_test` snapshots** for asserting complex output
  shapes.
- **`proptest` / `quickcheck`** when the invariant is easier to state
  than the example.

### Anti-patterns to reject

- `Vec<Box<dyn Trait>>` where an enum would fit.
- `Arc<Mutex<HashMap<K, V>>>` as a default cache — reach for
  `dashmap`, `moka`, or a channel-owned actor first.
- `Rc<RefCell<T>>` in async code — it is `!Send` and will fail to
  compile; the presence usually means the wrong runtime shape.
- `String` where `&str` would do; `Vec<T>` where `&[T]` would do;
  `HashMap<K, V>` where `BTreeMap<K, V>` gives ordered iteration you
  actually want.
- Custom `impl Deref` to expose inner fields — reviewers will read it
  as a smart pointer; use accessor methods instead.
- `unsafe` without a `// SAFETY:` comment naming the invariant the
  caller must uphold.
- Rebinding shadowed variables of the same name across long scopes —
  the reader loses track of which one is live.
- Match arms that clone the same value in every branch — hoist the
  clone (or borrow the source) before the match.

## Ownership And Cloning

Treat `.clone()` as a design question, not a reflex:

- Borrow when the callee does not need ownership.
- Move values when the current scope is done with them.
- Use `Arc<T>` only for real shared ownership across tasks or handlers.
- Avoid `Arc<Mutex<T>>` by default; check whether ownership, immutable state,
  a narrower lock, or message passing fits.
- Do not derive `Clone` speculatively.

Acceptable clones: small identifiers at API boundaries, `Arc::clone` /
`Bytes::clone` for shared state, owned response values. Per-crate
`CLONES.md` may whitelist additional cases.

Suspicious clones: large `Vec<T>`, `HashMap`, schemas, serialized payloads,
Python-owned objects, clones in loops.

### Borrow before you clone

```rust
/// Parses a borrowed card name without taking ownership of its source string.
///
/// # Errors
///
/// Returns `VersionError` when `input` is not a valid card name.
pub fn parse(input: &str) -> Result<CardName, VersionError> { ... }

/// Parses a card name after unnecessarily taking ownership of its source.
///
/// This rejected shape forces callers to move or allocate a `String`.
///
/// # Errors
///
/// Returns `VersionError` when `input` is not a valid card name.
pub fn parse(input: String) -> Result<CardName, VersionError> { ... }
```

## Allocation

- Use `&str`, `&Path`, and `&[T]` by default.
- Use `String::with_capacity` and `Vec::with_capacity` when size is known.
- Use `write!` into an existing `String` instead of repeated `format!`.
- Keep JSON conversion at API / storage / Python boundaries only.
- Do not serialize and deserialize just to move data between Rust layers.

Optimize measured bottlenecks. Keep ordinary code clear first.

## Async

Synchronous Rust is the default. Async must earn its state-machine, lifetime,
cancellation, and `Send` complexity by directly awaiting IO or intentionally
composing operations that do. Keep parsing, validation, planning,
transformations, and other pure computation synchronous even when called from
async code.

- Put heavy shared dependencies in explicit state structs.
- Use `Arc` for backend clients and immutable shared config.
- Avoid rebuilding clients, pools, schemas, or runtimes per request.
- Keep locks out of request hot paths.
- Do not spawn ad hoc Tokio runtimes in library or PyO3 code — use the shared
  `wyrd-runtime` bridge.
- Use bounded concurrency and timeouts for external calls.

## Errors

Rust errors should be useful before they become HTTP or Python errors:

- Use `thiserror` for crate-local library error enums.
- Use `anyhow` only in binaries.
- Include operation, field, resource, or invariant context where possible.
- Preserve source errors with `#[source]` / `#[from]`.
- Use the derive-backed `wyrd_spec::error::WyrdError` catalog for public
  errors that cross HTTP, Python, MCP, CLI, or generated-documentation
  boundaries. See `references/errors.md`.

### Crate-local thiserror

```rust
/// Reports failures from registry planning, persistence, and lookup.
#[derive(Debug, thiserror::Error)]
pub enum RegistryError {
    /// Indicates that no card matched the requested durable identity.
    #[error("card not found: {kind}/{space}/{name}@{version}")]
    CardNotFound {
        /// Kind included in the failed lookup.
        kind: CardKind,
        /// Space included in the failed lookup.
        space: SpaceName,
        /// Name included in the failed lookup.
        name: CardName,
        /// Version included in the failed lookup.
        version: VersionBlock,
    },
    /// Wraps a database failure encountered by the registry.
    #[error("database error")]
    Database(#[source] sqlx::Error),
}
```

Do not use `unwrap()` for environment, filesystem, network, parsing, user
input, database, storage, or external-service behavior in non-test code.
Use `expect()` only for true invariants, with a message naming the invariant.

## `tracing`

Use `tracing` with structured fields for diagnostics. Instrument write
handlers with `#[tracing::instrument]` and scrubbed args.

```rust
/// Registers one card for the authenticated tenant.
///
/// The handler delegates durable validation, tenancy enforcement, persistence,
/// relationship derivation, and audit behavior to the owning application
/// service.
///
/// # Errors
///
/// Returns `WyrdError` when authentication, validation, authorization,
/// persistence, relationship derivation, or audit recording fails.
///
/// # Cancellation
///
/// Cancellation may interrupt the response after the server begins durable
/// processing. Retry behavior follows the route's idempotency contract.
#[tracing::instrument(skip(state, body), fields(tenant = %tenant.id(), card_kind = %body.kind))]
pub async fn register_card(
    State(state): State<AppState>,
    tenant: TenantContext,
    Json(body): Json<CardEnvelope>,
) -> Result<Json<CardRecord>, WyrdError> { ... }
```

## Secrets

Use `secrecy::SecretString` for secrets. Wrap secret-bearing structs with
custom `Debug` impls that redact.

```rust
/// Owns the public identifier and redacted secret for one OIDC client.
#[derive(Clone)]
pub struct OidcClient {
    /// Public client identifier sent to the identity provider.
    client_id: String,
    /// Secret credential that must never appear in diagnostics.
    client_secret: SecretString,
}

impl std::fmt::Debug for OidcClient {
    /// Formats the client identity while replacing its secret with a marker.
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("OidcClient")
            .field("client_id", &self.client_id)
            .field("client_secret", &"<redacted>")
            .finish()
    }
}
```

## Documentation

Rustdoc is required for every new or materially modified Rust item regardless
of visibility. This includes modules, structs, fields, enums, variants, traits,
associated types, constants, aliases, functions, methods, private helpers, test
helpers, and test functions.

Documentation must tell a maintainer:

1. why the item exists and which workflow responsibility it owns;
2. how the operation works at the level needed to change it safely;
3. which invariants, state transitions, and side effects matter;
4. which error conditions a fallible operation returns in `# Errors`;
5. which panics remain possible in `# Panics`; and
6. for async or durable operations, what cancellation, partial progress,
   idempotency, and retry behavior applies.

Do not use placeholder rustdoc that merely restates a symbol name or signature.
Missing or incomplete rustdoc is a hard blocker, not optional cleanup.

## Testing Rust Core

New core logic should have Rust tests that do not require Python unless the
behavior is inherently Python-facing.

Good Rust tests:

- Exercise domain behavior through public or crate-visible APIs.
- Cover success, edge cases, and stable failures with explicit assertions on
  Wyrd error codes.
- Use local fixtures and mocks; no credentials, no live external services.
- Use `--test-threads=1` when filesystem, database, runtime, network ports,
  or shared global state can collide.

If a test requires Python for non-Python behavior, the boundary is
misplaced — pure Rust logic belongs in Rust tests.
