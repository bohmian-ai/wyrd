# Admin console design mock

Clickable preview of the tenant Admin area (People, Service accounts, Roles, Sign-in, Secrets, Integrations), Your
account, and the platform console shell. Design notes: [../admin-console.md](../admin-console.md). Sample data only; every address is
`example.test`. It is not product code. Shapes follow the wire views named in `src/data.js`; fields the server does not
expose yet are listed in the notes' open questions.

## Build and check

From the repository root:

```bash
mise exec -C changes/active/authorization-model/design/admin-mock -- node build.mjs      # writes dist/
mise exec -C changes/active/authorization-model/design/admin-mock -- node selfcheck.mjs  # every state, light and dark, 1280 and 390
```

`build.mjs` borrows Vite, Svelte and the fonts from `crates/wyrd/wyrd-server/wyrd-ui/node_modules`. The self-check
fails on any page error, script dialog, sideways page scroll, or request outside `design/mock/`, and prints the temp
directory holding the screenshots.

Preview: serve the repository root with any static server and open
`/changes/active/authorization-model/design/admin-mock/dist/`. The dashed strip's **State** menu opens every state; the sun/moon
button beside the name switches light and dark (`?mode=light` before the hash starts in light).

## States

| Hash | Shows |
|---|---|
| `#/t/gtest/settings/people` | Status strip, members table; `?filter=invited`, `?drawer=invite`, `?sent=1`, `?drawer=u3` |
| `#/t/gtest/settings/service-accounts` | Accounts and key counts |
| `#/t/gtest/settings/roles` | Plain-language role summaries; `?open=editor` expands operations |
| `#/t/gtest/settings/sign-in` | Method tiles, company SSO setup (`?setup=1`, `?setup=2`), `?sso=active`, `&confirm=require`, `&required=1` |
| `#/t/gtest/settings/secrets` | Inventory with Replace and a ⋯ menu; `?drawer=add`, `?confirm=openai-prod` |
| `#/t/gtest/settings/integrations` | GitHub |
| `#/t/gtest/account` | Your account, from the name block |
| `#/platform/ui` + `/workspaces`, `/providers`, `/mail`, `/admins`, `/secrets` | Platform console |
