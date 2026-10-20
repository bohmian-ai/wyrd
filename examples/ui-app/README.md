# Wyrd UI custom app

A SvelteKit app that renders `wyrd-ai-ui` components with data read on its
own server through the `wyrd-ai` SDK. The API key stays on the server; the
browser receives only loaded props.

- `/` uses `CardHeader` and `Chart` directly with props loaded in `+page.server.ts`.
- `/view` renders the same panels as a view document through `WyrdView`,
  loading each panel through `POST /view/load`.

Declare the support desk first (`examples/support-desk`), then from
`crates/wyrd/wyrd-server/wyrd-ui`:

```bash
pnpm install
pnpm package
WYRD_SERVER_URL=http://127.0.0.1:8080 WYRD_API_KEY=... pnpm --filter @wyrd/example-ui-app dev
```
