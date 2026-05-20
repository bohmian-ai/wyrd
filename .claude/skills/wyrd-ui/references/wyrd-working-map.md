# Wyrd UI Working Map

Use this file to find the right feature area quickly before editing.

## App root

- UI root: `crates/wyrd/wyrd-server/wyrd-ui`
- Global CSS: `src/app.css`
- App shell: `src/routes/+layout.svelte`
- Home route: `src/routes/+page.svelte`
- Brand assets and tokens: `brand/`

## Suggested feature boundaries

Create these directories only when the feature needs them:

- Shared visual components: `src/lib/components/`
- Route-local components: keep beside the route when they are not reused.
- Server-only helpers: `src/lib/server/`
- Internal API endpoints: `src/routes/api/...`
- Shared client state: `src/lib/state/` or `src/lib/components/settings/`, depending on local convention.
- Shared types: keep with the feature first; move to `src/lib/types/` only when reused across features.

## Editing order

1. Inspect the route entrypoint.
2. Inspect the nearest feature component.
3. Inspect server helpers or API endpoints.
4. Inspect theme and brand files.
5. Inspect shared state, schemas, or route constants.

Avoid broad refactors while adding a feature. Wyrd's UI is still small, so local clarity is usually better than early abstraction.
