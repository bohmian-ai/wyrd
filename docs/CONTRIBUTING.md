# Docs contributing

The docs site is an Astro Starlight app under `docs/`. Public pages should explain the Wyrd surface as it exists or as the current phase explicitly plans it. Avoid migration-only shorthand in user-facing pages.

Use repository tasks for normal work:

```sh
mise run docs:dev
mise run docs:generate
mise run docs:check
```

Contributor-only commands are acceptable when they are labelled for maintainers and are not presented as the public path.

```sh contributor
pnpm astro check
pnpm astro build
```

Generated pages live in `src/content/docs/cards/` and `src/content/docs/api/`. Update the source schema or API metadata first, then refresh generated docs with `mise run docs:generate`.
