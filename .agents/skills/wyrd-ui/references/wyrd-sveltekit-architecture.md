# Wyrd SvelteKit Architecture

Stack: SvelteKit 2 on `adapter-node`, Svelte 5, TypeScript, Vite, Tailwind v4,
`@fontsource` fonts, Zod, and `@xyflow/svelte` for graphs. There is no component
library.

## Server and client split

- The host is a BFF. `+page.server.ts` and `+layout.server.ts` call the server through
  the `wyrd-ai` SDK via `wyrd-ai-ui/node`, as the signed-in session.
- The browser calls only host routes (`+server.ts`), which validate input against the
  catalog schema and forward the session cookie. The browser never holds a token or an
  API key: refresh tokens and API keys live in HttpOnly cookies, and access tokens stay in
  server memory.
- Tenant comes from the `[tenantKey]` route and the session's memberships, never from the
  request body. A route that cannot prove the tenant refuses the request.
- `wyrd-ai` is SSR-external (`vite.config.ts`). Never import it into browser code;
  `contract.ts` imports only its types.

## Errors

- Errors are problems projected from the Rust `WyrdError` catalog: stable `code`,
  `title`, and `remediation`. Use `wyrdProblem` (`packages/ui`) and `problem.ts` (host);
  never invent a response envelope or a code.
- `problem.ts` keeps known canonical metadata only and drops arbitrary upstream
  diagnostics.
- Expired or missing sessions redirect to sign-in from hooks. `+error.svelte` covers
  only errors raised after route resolution.

## Routes and state

- Shareable view state round-trips through the URL (`cards-host.ts`,
  `observe-host.ts`), so reload, Back, and shared links restore the view.
- Use `+page@.svelte` to escape the tenant shell, as the auth pages do; don't add a
  route group for it.
- After a mutation, invalidate the narrowest dependency rather than everything.
- Validate form fields with `src/lib/server/form.ts`.
