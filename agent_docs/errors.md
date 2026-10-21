# Errors

Load when adding or changing an error that a caller can see. Wyrd errors are
clear for humans, structured for agents, and identical across Rust, HTTP,
Python, TypeScript, MCP, and CLI.

## Codes

Stable codes have the form `WYRD_<DOMAIN>_<STATUS>_<SLUG>`, for example
`WYRD_REGISTRY_404_CARD_NOT_FOUND`. The catalog is generated from the
`WyrdError` derive; never keep a hand-written code list.

## Public errors

Errors that cross HTTP, Python, TypeScript, MCP, CLI, or generated docs live in
`wyrd_spec::error::WyrdError` or a Wyrd-coded enum using the same derive
(`crates/wyrd-spec/src/error.rs`):

```rust
#[derive(Debug, Clone, thiserror::Error, Serialize, Deserialize, schemars::JsonSchema, WyrdError)]
#[cfg_attr(feature = "server", derive(utoipa::ToSchema))]
#[serde(tag = "variant", content = "data", rename_all = "snake_case")]
pub enum WyrdStorageError {
    #[error("artifact size {actual} exceeds backend limit {limit}")]
    #[wyrd_error(
        code = "WYRD_STORAGE_400_ARTIFACT_TOO_LARGE",
        status = 400,
        title = "Artifact size exceeds backend capacity",
        remediation = "Split the artifact into multiple cards or use a backend with higher per-object limits."
    )]
    ArtifactTooLarge { actual: u64, limit: u64 },
}
```

The derive generates `code()`, `status()`, `title()`, `remediation()`,
`detail()`, `details()`, `as_problem_json()`, `catalog()`, and
`catalog_entry()`. Never hand-write them. A new code is new variant metadata.

Top-level `WyrdError` boundary variants carry a canonical message and
JSON-safe `details`. Subordinate Wyrd-coded enums use typed fields instead;
the derive maps them into `detail` and `details`.

## Crate-local errors

Plain `thiserror` enums with typed fields that name the operation, resource,
or field. Avoid `InvalidInput(String)` when a typed variant fits.

At a public boundary, convert to `WyrdError`. Never leak raw transport,
database, provider, filesystem, or cryptography strings into public payloads;
log the source with `tracing` and return a typed or `Internal` error.

## SQL family

`wyrd-sql` owns the `WYRD_SQL_*` catalog; `vala-sql` re-exports
`wyrd_sql::error::SqlError`, so callers see one SQL family. Constraint
failures map by SQLSTATE (`23505` unique, `23503` foreign key, `23514`
check), never by message text. The one exception: SQLSTATE `42501` maps to
`WYRD_SQL_403_RLS_DENIED` only when the message names row-level security,
because Postgres uses the same state for ordinary permission denials.

## Per surface

- **HTTP:** problem payloads come only from `WyrdError::as_problem_json()`.
  Public write handlers flow through a single `IntoResponse` mapper.
- **Python:** one `WyrdError` exception root, converted at the PyO3 edge. See
  `sdks/wyrd-sdk-python/AGENTS.md`.
- **TypeScript:** one `WyrdError` class whose fields project the catalog
  exactly, `code` typed by the generated `WyrdErrorCode` union. See
  `sdks/wyrd-sdk-ts/AGENTS.md`.
- **MCP:** tool errors carry the catalog `code`.
- **CLI:** human-readable and actionable, still carrying the stable code when
  one applies. No stack trace unless debug output is requested.

## Validation failures

A validation error an agent can repair carries `code`, `field`, the invalid
value when safe to show, the expected format or range, and remediation. Put
the rule where the durable contract lives, in Rust, and project it to every
surface.
