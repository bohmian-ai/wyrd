# Rust Quality Reference

Use this as a compact calibration for code-quality reviews of Rust changes.
Repository rules and nearby local implementations take precedence over generic
guidance.

## Idiomatic shape

- Prefer concrete types and enums when the domain is closed.
- Add a trait only for a real capability boundary, multiple implementations, or
  focused substitution in tests.
- Borrow when ownership is not required. Treat every non-boundary clone or
  allocation as something that needs a concrete ownership or lifetime reason.
- Prefer typed IDs, enums, newtypes, and validated constructors over raw strings
  where the repository already uses them.
- Use `?` and typed errors at fallible boundaries. Do not hide context by
  discarding errors or converting them to opaque strings.
- Use checked conversions for values crossing integer widths or signs.
- Keep pure computation synchronous and keep blocking work out of async paths.
- Do not hold mutex or transaction guards across `.await` unless the local
  pattern explicitly proves it safe.

## Documentation

- Public modules and items need rustdoc that explains purpose and observable
  behavior.
- Add `# Errors`, `# Panics`, `# Safety`, retry, concurrency, lifecycle, or
  idempotency sections when those are part of the contract.
- Document non-trivial private helpers when their invariants or side effects are
  not recoverable from names and types.
- Prefer a type or name change over a comment that restates code.

## Tooling calibration

Use the repository's canonical checks when available:

- `rustfmt --check` for formatting;
- Clippy correctness, suspicious, style, complexity, and performance lints;
- rustdoc missing-documentation and broken-link checks; and
- repository-specific audits for panic sites and lint suppressions.

Useful primary references:

- <https://doc.rust-lang.org/stable/clippy/>
- <https://doc.rust-lang.org/style-guide/>
- <https://doc.rust-lang.org/rustdoc/how-to-write-documentation.html>
- <https://rust-lang.github.io/api-guidelines/>
