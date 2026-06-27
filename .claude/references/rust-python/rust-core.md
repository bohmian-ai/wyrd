# Rust Core

Wyrd's Python and server APIs are only as reliable as the Rust core underneath
them. Design Rust code first as a clean, testable library, then expose the right
boundary to Python, HTTP, MCP, or the CLI.

## API Shape

Prefer APIs that make invalid states difficult to represent:

- Use domain types instead of raw strings for durable identifiers.
- Use enums for closed sets of states, card kinds, operations, providers,
  backends, and variants.
- Use structs with named fields for meaningful records.
- Use traits for behavior shared across real implementations.
- Keep public functions explicit about inputs, outputs, validation, and error
  behavior.

Do not shape Rust APIs around what is easiest to extract from Python or JSON.
Convert at the edge, then call Rust-native functions.

## Ownership

Treat `.clone()` as a question:

- Borrow when the callee does not need ownership.
- Move values when the current scope is done with them.
- Use `Arc<T>` only for real shared ownership.
- Avoid `Arc<Mutex<T>>` as a default.
- Do not derive `Clone` speculatively.

Acceptable clones include small identifiers at boundaries, `Arc::clone` for
shared state, and owned response values. Suspicious clones include large
vectors, maps, schemas, serialized payloads, and clones in loops.

## Allocation

Avoid accidental allocation in repeated paths:

- Use `&str`, `&Path`, and `&[T]` by default.
- Use `String::with_capacity` and `Vec::with_capacity` when size is known.
- Use `write!` into an existing `String` instead of repeated `format!`.
- Keep JSON conversion at API, storage, or Python boundaries.
- Do not serialize and deserialize just to move data between Rust layers.

Optimize measured bottlenecks. Keep ordinary code clear first.

## Traits And Dispatch

Choose dispatch deliberately:

- Concrete type: one implementation.
- Enum: closed set, exhaustiveness matters.
- Generic trait bound: hot path with static dispatch.
- Trait object: runtime extensibility is required.

Keep traits small and capability-focused. A trait with one implementation is
usually premature unless it defines a public extension boundary.

## Async

Use async for IO: HTTP, database, storage, queues, provider calls, server
handlers, and long-running background work. Keep pure computation synchronous.

For shared state:

- Put heavy shared dependencies in explicit state structs.
- Use `Arc` for backend clients and immutable shared config where needed.
- Avoid rebuilding clients, pools, schemas, or runtimes per request.
- Keep locks out of request hot paths.

## Errors

Rust errors should be useful before they become HTTP or Python errors:

- Use `thiserror` in libraries.
- Include operation, field, resource, or invariant context where possible.
- Preserve source errors when useful and safe.
- Keep stable Wyrd error codes in the contract layer.
- Use the derive-backed Wyrd error catalog for public boundary errors; do not
  duplicate generated `code`, `status`, `remediation`, or problem-json logic by
  hand.
- Avoid `anyhow` in public library surfaces.

Use `references/errors.md` for boundary conversion.
