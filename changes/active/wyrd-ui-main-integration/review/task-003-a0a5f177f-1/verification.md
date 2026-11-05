# Fresh verification at a0a5f177f

Commands run sequentially by orchestrator. Historical task evidence and reviewer probes remain distinct.

| Command | Exit | Selected result |
|---|---|---|
| `mise exec -- pnpm --dir crates/wyrd/wyrd-server/wyrd-ui test src/lib/ui-library.test.ts -t '^TASK-003: Validated portable rendering$'` | 0 | Tests  1 passed | 9 skipped (10) |
| `mise exec -- pnpm --dir crates/wyrd/wyrd-server/wyrd-ui test src/lib/ui-library.test.ts -t '^TASK-003: Dependent loads and safe actions$'` | 0 | Tests  1 passed | 9 skipped (10) |
| `mise exec -- pnpm --dir crates/wyrd/wyrd-server/wyrd-ui test src/lib/ui-library.test.ts -t '^Node query acquisition cancellation$'` | 0 | Tests  1 passed | 9 skipped (10) |
| `mise exec -- pnpm --dir crates/wyrd/wyrd-server/wyrd-ui test src/lib/ui-library.test.ts -t '^Canonical UI problem projections$'` | 0 | Tests  1 passed | 9 skipped (10) |
| `mise exec -- pnpm --dir crates/wyrd/wyrd-server/wyrd-ui test src/lib/ui-library.test.ts -t '^TASK-027: Layout editing$'` | 0 | Tests  1 passed | 9 skipped (10) |
| `mise exec -- pnpm --dir crates/wyrd/wyrd-server/wyrd-ui test src/lib/ui-library.test.ts -t '^TASK-027: Grid placement$'` | 0 | Tests  1 passed | 9 skipped (10) |
| `mise exec -- pnpm --dir crates/wyrd/wyrd-server/wyrd-ui test src/lib/ui-library.test.ts -t '^TASK-027: Layout document contract$'` | 0 | Tests  1 passed | 9 skipped (10) |
| `mise exec -- pnpm --dir crates/wyrd/wyrd-server/wyrd-ui test src/lib/ui-library.test.ts -t '^TASK-027: Narrow views$'` | 0 | Tests  1 passed | 9 skipped (10) |
| `mise exec -- pnpm --dir crates/wyrd/wyrd-server/wyrd-ui test src/lib/ui-library.test.ts -t '^TASK-027: Pointer edit cancellation$'` | 0 | Tests  1 passed | 9 skipped (10) |

## Workspace verification

```sh
mise exec -- pnpm --dir crates/wyrd/wyrd-server/wyrd-ui check
```

Exit: 0

```text
svelte-check found 0 errors and 0 warnings
Loading svelte-check in workspace: /Users/stevenforrester/Documents/GitHub/wyrd-ui-main-integration/crates/wyrd/wyrd-server/wyrd-ui
Getting Svelte diagnostics...

svelte-check found 0 errors and 0 warnings
$ pnpm package && pnpm --filter wyrd-ai-ui check && svelte-kit sync && svelte-check --tsconfig ./tsconfig.json
$ pnpm --filter wyrd-ai-ui build
$ node scripts/catalog.ts && svelte-package && vite build --config vite.app.config.ts
$ svelte-check --tsconfig ./tsconfig.json
```

## Workspace verification

```sh
mise exec -- pnpm --dir crates/wyrd/wyrd-server/wyrd-ui build
```

Exit: 0

```text
node_modules/.pnpm/zod@4.5.4/node_modules/zod/v4/core/regexes.js (70:0): A comment

"/** Anchors a pattern source. The interpolation lives here rather than at the call site because
 * esbuild will not drop a `@__PURE__` call whose own argument interpolates a variable, but it
 * will drop `anchor(dateSource)`. Keeping it inline pinned `date` into every bundle. */"

in "node_modules/.pnpm/zod@4.5.4/node_modules/zod/v4/core/regexes.js" contains an annotation that Rollup cannot interpret due to the position of the comment. The comment will be removed to avoid issues.
Circular dependency: node_modules/.pnpm/zod@4.5.4/node_modules/zod/v4/core/core.js -> node_modules/.pnpm/zod@4.5.4/node_modules/zod/v4/core/util.js -> node_modules/.pnpm/zod@4.5.4/node_modules/zod/v4/core/core.js
Generated an empty chunk: "chunks/env.js".
```

## Workspace verification

```sh
mise exec -- pnpm --dir crates/wyrd/wyrd-server/wyrd-ui --filter @wyrd/example-ui-app check
```

Exit: 0

```text
Loading svelte-check in workspace: /Users/stevenforrester/Documents/GitHub/wyrd-ui-main-integration/examples/ui-app
Getting Svelte diagnostics...

svelte-check found 0 errors and 0 warnings
$ svelte-kit sync && svelte-check --tsconfig ./tsconfig.json
```

## Workspace verification

```sh
mise exec -- pnpm --dir crates/wyrd/wyrd-server/wyrd-ui --filter @wyrd/example-ui-app build
```

Exit: 0

```text
$ vite build
.svelte-kit/output/server/chunks/loaders.js (989:0): A comment

"/** Anchors a pattern source. The interpolation lives here rather than at the call site because
* esbuild will not drop a `@__PURE__` call whose own argument interpolates a variable, but it
* will drop `anchor(dateSource)`. Keeping it inline pinned `date` into every bundle. */"

in ".svelte-kit/output/server/chunks/loaders.js" contains an annotation that Rollup cannot interpret due to the position of the comment. The comment will be removed to avoid issues.
Generated an empty chunk: "chunks/env.js".
```

## Native Node smoke

Cwd `crates/wyrd/wyrd-server/wyrd-ui`.

```sh
mise exec -- node --input-type=module -e 'import assert from "node:assert/strict"; const loaders = await import("wyrd-ai-ui/loaders"); const mcp = await import("wyrd-ai-ui/mcp"); assert.equal(loaders.chartParams("SELECT $1", undefined).code, "WYRD_SPEC_400_VALIDATION"); assert.equal(typeof mcp.mcpExecutor, "function");'
```

Exit: 0; output: ''.

## Identity and limits

HEAD remains `a0a5f177f23960822222b851716460dec02804b3`; tracked source unchanged; git diff --check exit 0.

No generated schema/catalog input changed in R2, so codegen:check was not repeated; prior recorded green evidence remains historical. No full Rust aggregate, Linux bundles/images, new live Postgres/MCP journey, or live degraded-terminal parity rerun by root. Original and R2 browser evidence is supplied; fresh reviewer probes are reported by their respective roles. Root reran the nine exact focused selectors, not the full ten-test file.
