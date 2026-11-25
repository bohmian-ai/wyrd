# Verification workflow design mock

One self-contained preview of the Verify, execution, Verifier, Repository and Settings pages in a single Wyrd shell.
It brings together the original canvas designs (`../../references/original-canvas/`) and the agreed changes in
`../mock-brief.md`. The data is sample data and the server responses are simulated. It is not product code.

## Build and check

From the repository root:

```bash
mise exec -C changes/active/verification-workflow-ui/design/mock -- node build.mjs       # writes dist/
mise exec -C changes/active/verification-workflow-ui/design/mock -- node selfcheck.mjs   # real-browser checks
mise exec -C changes/active/verification-workflow-ui/design/mock -- node selfcheck.mjs --shots   # screenshots only
```

`build.mjs` borrows Vite and Svelte from `crates/wyrd/wyrd-server/wyrd-ui/node_modules`. The only local dependency is
Dagre (`package.json`). The self-check serves the repository root on an ephemeral port and drives `dist/` with the
Playwright that wyrd-ui already installs. It fails on any page error, script dialog, or request outside
`design/mock/`. Screenshots go to a fresh `$TMPDIR/wyrd-verification-mock-*` directory and never enter the source tree.

Preview (served by the existing repository-root server): `http://127.0.0.1:8767/changes/active/verification-workflow-ui/design/mock/dist/`

## Routes

| Hash | Page |
|---|---|
| `#/verify?<facet>=<value>&range&t0&t1&group` | Verify discovery: WHERE, zoom, range, Live, two charts, Group by, facets, list |
| `#/x/<execution>?sel&tab&inst&out&trace&cite&back` | Execution page: header, graph, inspector |
| `#/repositories?q&show` | Repositories: its own navigation item, not under Cards; the breadcrumb returns to the same search and Show filter |
| `#/repositories/<card>?v&tab=toml&reviews=all` | Repository Card detail and verification activity |
| `#/repositories/<card>/pulls/<pr>?set=<review set>` | Pull request review sets; the plain link follows the latest set, `set` pins one |
| `#/repositories/github/<repository_id>` | Repository with access but no Card |
| `#/cards/verifiers/<name@version>?tab=yaml` | Exact Verifier definition, opened from executions and bindings; there is no Verifier list |
| `#/settings/github` | GitHub connection |

Cards, Home, Observe, Query and Changes are inert placeholders in the navigation. Cards already has its own UI, which
this preview does not reproduce.

The **Examples** menu in the preview strip opens every sample. The samples are the session judgment `exe-6c19e5f0`,
the approval wait `exe-4d1e9a07`, the pull request run `exe-7f3a91c2`, the ordinary run `exe-7c1e02a4` and the drift
window `exe-d1f7a00e`.

## Limitations

- The Verify list keeps every original column, including Service, at readable widths (1184px minimum). Narrower
  results panels scroll the table sideways, and the footer says so. At 1440px, Status, Failed and Verdict stay pinned
  on the right, so Service and Repo · PR sit beneath them until you scroll. Below 900px nothing is pinned and the
  whole row scrolls.
- No quick-view drawer. A Verify row opens the execution page, and Back restores the exact Verify state.
- Simulated decisions, cancellations, connection changes and live events last only until the page reloads.
- A wide graph (the pull request run) fits at about 72% on a 1440px screen. Use zoom or 100% to read it closer.
- The user approved this centralized UI. The mock makes no claim about backend behaviour or production implementation approval.
