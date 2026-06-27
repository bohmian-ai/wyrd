# Errors

Wyrd errors must be clear for humans, structured for agents, and stable across
Rust, HTTP, Python, MCP, and CLI surfaces.

## Error Codes

Use stable codes:

```text
WYRD_<DOMAIN>_<STATUS>_<SLUG>
```

Examples:

```text
WYRD_SPEC_400_INVALID_ID
WYRD_REGISTRY_404_CARD_NOT_FOUND
WYRD_PROVIDER_429_RATE_LIMITED
```

Codes belong in the contract layer when they are public or cross a boundary.

## Public Wyrd Errors

Public errors that cross HTTP, Python, MCP, CLI, or generated-documentation
boundaries belong in `wyrd_spec::error::WyrdError` or in a Wyrd-coded enum that
uses the same derive-backed metadata pattern.

Use `#[derive(derive::WyrdError)]` with one `#[wyrd_error(...)]` attribute per
variant:

```rust
#[derive(Debug, thiserror::Error, derive::WyrdError)]
pub enum WyrdError {
    #[wyrd_error(
        code = "WYRD_REGISTRY_404_CARD_NOT_FOUND",
        status = 404,
        title = "Card not found",
        remediation = "Check that the referenced Card exists in the requested space and version."
    )]
    #[error("[WYRD_REGISTRY_404_CARD_NOT_FOUND] {message}")]
    NotFound {
        message: String,
        details: serde_json::Value,
    },
}
```

The derive is the source of truth for:

- `code()`
- `status()`
- `title()`
- `remediation()`
- `detail()`
- `details()`
- `as_problem_json()`
- `catalog()`
- `catalog_entry()`

Do not hand-write parallel implementations of those methods for public Wyrd
errors. If a public error needs a new code, add the variant metadata and let the
derive generate the catalog and problem payload behavior.

Every public variant should carry a `message: String` and
`details: serde_json::Value` unless there is a concrete reason to use a more
specific shape. The generated problem payload uses `message` for `detail` and
`details` for structured context.

## Crate-Local Rust Errors

Use plain `thiserror` for crate-local library errors that do not directly cross
public boundaries:

```rust
#[derive(Debug, thiserror::Error)]
pub enum RegistryError {
    #[error("card not found: {space}/{name}@{version}")]
    CardNotFound {
        space: SpaceName,
        name: CardName,
        version: Version,
    },
}
```

Good errors name the operation, the resource or field, and enough context to
fix the issue. Avoid vague variants such as `InvalidInput(String)` when a typed
variant is possible.

When a crate-local error reaches a public boundary, convert it into
`wyrd_spec::error::WyrdError` at that boundary. Do not leak internal transport,
database, provider, filesystem, or cryptography error strings directly into
public payloads.

## Python Errors

Do not store `PyErr` in reusable Rust errors. Convert Rust errors to typed
Python exceptions at the PyO3 boundary.

Python exceptions should preserve:

- Wyrd error code
- concise message
- relevant field or resource
- remediation when available

## HTTP Errors

HTTP responses should use structured problem payloads. Do not leak raw database,
storage, provider, or filesystem error strings directly to users.

Map internal errors to safe public `WyrdError` variants and preserve detailed
source errors in logs/traces. HTTP problem JSON should come from
`WyrdError::as_problem_json()` or the equivalent derive-generated method.

## CLI Errors

CLI errors can be human-facing, but should still carry stable codes when the
error maps to a Wyrd contract. Use actionable messages and avoid stack traces
unless the user explicitly asks for debug output.
