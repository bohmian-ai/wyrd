# Contributing to Wyrd

## Welcome

Thanks for contributing to Wyrd. This guide covers the local setup, validation
commands, and review expectations for this repository.

## Table of Contents

- [Contributing to Wyrd](#contributing-to-wyrd)
  - [Welcome](#welcome)
  - [Table of Contents](#table-of-contents)
  - [Environment Setup](#environment-setup)
  - [Contributing Changes](#contributing-changes)
  - [Coding Conventions](#coding-conventions)
  - [Git](#git)

## Environment Setup

Wyrd uses a Rust workspace, a SvelteKit frontend, and a future Python package
stub. Tooling is managed through `mise`.

```console
mise install
mise run check
mise run dev:full
```

If `mise install` fails, ensure mise itself is installed:
<https://mise.jdx.dev>.

The local dev stack starts:

- UI: <http://localhost:3000>
- wyrd-server: <http://localhost:8080/healthz>

All developer tasks live in `mise.toml`. There is no `Makefile` and no
`justfile`. Run `mise tasks` to list available commands.

## Contributing Changes

1. Create a branch for your change.
2. Make the smallest coherent change that satisfies the phase or issue.
3. Run the required checks from the repository root:

```console
mise run pre-pr
```

4. If your change touches the UI, also run:

```console
cd crates/wyrd/wyrd-server/wyrd-ui
pnpm run check
pnpm run build
```

5. Open a pull request after local validation passes.

Both CI workflows (`lints-test`, `codegen-check`) must be green before merge.

## Coding Conventions

- Rust edition `2024`. Workspace deps pin exact major.minor minimum; pre-1.0
  deps pin exact `x.y.z`. No wildcards.
- Library errors use `thiserror`. Binary errors use `anyhow`.
- Public Spec enums and structs are `#[non_exhaustive]`.
- Every wire type derives `schemars::JsonSchema`.
- PyO3 lives only in `sdks/wyrd-sdk-python` and an explicit allowlist.
- Client-tier crates may not depend on `sqlx`, `datafusion`, `deltalake`, or
  cloud SDKs.

### Configuration Secrets

Any `Config` type that carries secrets uses `secrecy::SecretString` with a
custom `Debug` impl that redacts the secret. Plain `String` for secrets is
forbidden.

### Error Code Format

All structured errors carry a stable code in the form
`WYRD_<DOMAIN>_<STATUS>_<SLUG>` (for example `WYRD_REGISTRY_404_CARD_NOT_FOUND`).

## Git

- Identity: `Thorrester <sjforrester32@gmail.com>`.
- `Co-Authored-By:` trailers are allowed when an agentic harness requires
  them; the author and committer stay the identity above.
- Branch names: `<short-slug>` for feature branches.
