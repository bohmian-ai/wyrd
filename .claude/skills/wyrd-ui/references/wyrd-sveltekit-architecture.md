# Wyrd SvelteKit Architecture

## Stack

Wyrd UI is a SvelteKit app using Svelte 5, TypeScript, Vite, Tailwind v4, and Skeleton. Keep changes aligned with the app in `crates/wyrd/wyrd-server/wyrd-ui`.

## Server and client split

- Keep backend access server-side by default.
- Use `+page.server.ts` or `+layout.server.ts` for page data that requires secrets, auth, or backend calls.
- Use `src/routes/api/.../+server.ts` for browser-callable internal endpoints.
- Put shared server-only logic under `src/lib/server/`.
- Keep client components focused on rendering, interaction, and local state.

## Route conventions

- Prefer route groups and nested layouts only when they simplify repeated UI structure.
- Keep route parameters explicit and typed.
- Co-locate route-specific parsing and validation near the route.
- Reuse shared path builders or route constants once string duplication becomes error-prone.

## API boundary conventions

Use a consistent response envelope for internal endpoints:

```ts
type ApiResult<T> = { response: T; error: null } | { response: null; error: string };
```

Normalize backend errors at the server boundary. Components should receive typed data and actionable error messages, not raw exception shapes.

## Invalidation

- Use SvelteKit invalidation for data that changes after mutations.
- Keep cache keys, route dependencies, and refresh triggers near the route or server helper that owns the data.
- Avoid global invalidation when a feature-level invalidation is enough.

## Auth and session boundaries

- Validate auth and session information in hooks, server loads, or server helpers.
- Do not expose tokens or backend credentials to browser code.
- Make unauthenticated, unauthorized, empty, and backend-error states visually distinct.
