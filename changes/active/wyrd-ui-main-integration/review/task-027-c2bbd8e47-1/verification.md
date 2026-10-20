# Fresh focused verification at c2bbd8e47

Commands executed sequentially through mise; package build runs as part of each test command. Historical task evidence is separate.

## TASK-003: Validated portable rendering

```sh
mise exec -- pnpm --dir crates/wyrd/wyrd-server/wyrd-ui test src/lib/ui-library.test.ts -t '^TASK-003: Validated portable rendering$'
```

Exit: 0

```text

 RUN  v5.0.5 /Users/stevenforrester/Documents/GitHub/wyrd-ui-main-integration/crates/wyrd/wyrd-server/wyrd-ui


 Test Files  1 passed (1)
      Tests  1 passed | 9 skipped (10)
   Start at  14:21:22
   Duration  972ms (environment 40%, tests 21%, transform 16%, import 15%, setup 8%)

$ pnpm package && vitest run src/lib/ui-library.test.ts -t '^TASK-003: Validated portable rendering$'
$ pnpm --filter wyrd-ai-ui build
$ node scripts/catalog.ts && svelte-package && vite build --config vite.app.config.ts
```

## TASK-003: Dependent loads and safe actions

```sh
mise exec -- pnpm --dir crates/wyrd/wyrd-server/wyrd-ui test src/lib/ui-library.test.ts -t '^TASK-003: Dependent loads and safe actions$'
```

Exit: 0

```text

 RUN  v5.0.5 /Users/stevenforrester/Documents/GitHub/wyrd-ui-main-integration/crates/wyrd/wyrd-server/wyrd-ui


 Test Files  1 passed (1)
      Tests  1 passed | 9 skipped (10)
   Start at  14:21:26
   Duration  1.01s (environment 29%, transform 24%, tests 21%, import 19%, setup 6%)

$ pnpm package && vitest run src/lib/ui-library.test.ts -t '^TASK-003: Dependent loads and safe actions$'
$ pnpm --filter wyrd-ai-ui build
$ node scripts/catalog.ts && svelte-package && vite build --config vite.app.config.ts
```

## Node query acquisition cancellation

```sh
mise exec -- pnpm --dir crates/wyrd/wyrd-server/wyrd-ui test src/lib/ui-library.test.ts -t '^Node query acquisition cancellation$'
```

Exit: 0

```text

 RUN  v5.0.5 /Users/stevenforrester/Documents/GitHub/wyrd-ui-main-integration/crates/wyrd/wyrd-server/wyrd-ui


 Test Files  1 passed (1)
      Tests  1 passed | 9 skipped (10)
   Start at  14:21:31
   Duration  774ms (environment 36%, tests 22%, transform 19%, import 17%, setup 6%)

$ pnpm package && vitest run src/lib/ui-library.test.ts -t '^Node query acquisition cancellation$'
$ pnpm --filter wyrd-ai-ui build
$ node scripts/catalog.ts && svelte-package && vite build --config vite.app.config.ts
```

## Canonical UI problem projections

```sh
mise exec -- pnpm --dir crates/wyrd/wyrd-server/wyrd-ui test src/lib/ui-library.test.ts -t '^Canonical UI problem projections$'
```

Exit: 0

```text

 RUN  v5.0.5 /Users/stevenforrester/Documents/GitHub/wyrd-ui-main-integration/crates/wyrd/wyrd-server/wyrd-ui


 Test Files  1 passed (1)
      Tests  1 passed | 9 skipped (10)
   Start at  14:21:35
   Duration  700ms (environment 37%, transform 20%, import 18%, tests 17%, setup 7%)

$ pnpm package && vitest run src/lib/ui-library.test.ts -t '^Canonical UI problem projections$'
$ pnpm --filter wyrd-ai-ui build
$ node scripts/catalog.ts && svelte-package && vite build --config vite.app.config.ts
```

## TASK-027: Layout editing

```sh
mise exec -- pnpm --dir crates/wyrd/wyrd-server/wyrd-ui test src/lib/ui-library.test.ts -t '^TASK-027: Layout editing$'
```

Exit: 0

```text

 RUN  v5.0.5 /Users/stevenforrester/Documents/GitHub/wyrd-ui-main-integration/crates/wyrd/wyrd-server/wyrd-ui


 Test Files  1 passed (1)
      Tests  1 passed | 9 skipped (10)
   Start at  14:21:39
   Duration  705ms (environment 37%, transform 20%, import 19%, tests 17%, setup 7%)

$ pnpm package && vitest run src/lib/ui-library.test.ts -t '^TASK-027: Layout editing$'
$ pnpm --filter wyrd-ai-ui build
$ node scripts/catalog.ts && svelte-package && vite build --config vite.app.config.ts
```

## TASK-027: Grid placement

```sh
mise exec -- pnpm --dir crates/wyrd/wyrd-server/wyrd-ui test src/lib/ui-library.test.ts -t '^TASK-027: Grid placement$'
```

Exit: 0

```text

 RUN  v5.0.5 /Users/stevenforrester/Documents/GitHub/wyrd-ui-main-integration/crates/wyrd/wyrd-server/wyrd-ui


 Test Files  1 passed (1)
      Tests  1 passed | 9 skipped (10)
   Start at  14:21:43
   Duration  1.16s (transform 33%, environment 31%, import 24%, tests 6%, setup 6%)

$ pnpm package && vitest run src/lib/ui-library.test.ts -t '^TASK-027: Grid placement$'
$ pnpm --filter wyrd-ai-ui build
$ node scripts/catalog.ts && svelte-package && vite build --config vite.app.config.ts
```

## TASK-027: Layout document contract

```sh
mise exec -- pnpm --dir crates/wyrd/wyrd-server/wyrd-ui test src/lib/ui-library.test.ts -t '^TASK-027: Layout document contract$'
```

Exit: 0

```text

 RUN  v5.0.5 /Users/stevenforrester/Documents/GitHub/wyrd-ui-main-integration/crates/wyrd/wyrd-server/wyrd-ui


 Test Files  1 passed (1)
      Tests  1 passed | 9 skipped (10)
   Start at  14:21:48
   Duration  610ms (environment 46%, transform 23%, import 21%, setup 8%, tests 1%)

$ pnpm package && vitest run src/lib/ui-library.test.ts -t '^TASK-027: Layout document contract$'
$ pnpm --filter wyrd-ai-ui build
$ node scripts/catalog.ts && svelte-package && vite build --config vite.app.config.ts
```

## TASK-027: Narrow views

```sh
mise exec -- pnpm --dir crates/wyrd/wyrd-server/wyrd-ui test src/lib/ui-library.test.ts -t '^TASK-027: Narrow views$'
```

Exit: 0

```text

 RUN  v5.0.5 /Users/stevenforrester/Documents/GitHub/wyrd-ui-main-integration/crates/wyrd/wyrd-server/wyrd-ui


 Test Files  1 passed (1)
      Tests  1 passed | 9 skipped (10)
   Start at  14:21:52
   Duration  692ms (environment 37%, transform 23%, import 21%, tests 11%, setup 7%)

$ pnpm package && vitest run src/lib/ui-library.test.ts -t '^TASK-027: Narrow views$'
$ pnpm --filter wyrd-ai-ui build
$ node scripts/catalog.ts && svelte-package && vite build --config vite.app.config.ts
```

## TASK-027: Pointer edit cancellation

```sh
mise exec -- pnpm --dir crates/wyrd/wyrd-server/wyrd-ui test src/lib/ui-library.test.ts -t '^TASK-027: Pointer edit cancellation$'
```

Exit: 0

```text

 RUN  v5.0.5 /Users/stevenforrester/Documents/GitHub/wyrd-ui-main-integration/crates/wyrd/wyrd-server/wyrd-ui


 Test Files  1 passed (1)
      Tests  1 passed | 9 skipped (10)
   Start at  14:21:55
   Duration  685ms (environment 36%, transform 24%, import 21%, tests 12%, setup 7%, worker 1%)

$ pnpm package && vitest run src/lib/ui-library.test.ts -t '^TASK-027: Pointer edit cancellation$'
$ pnpm --filter wyrd-ai-ui build
$ node scripts/catalog.ts && svelte-package && vite build --config vite.app.config.ts
```

## Workspace check/build

```sh
mise exec -- pnpm --dir crates/wyrd/wyrd-server/wyrd-ui check
```

Exit: 0

```text
dist/wyrd-view.html  488.70 kB │ gzip: 218.21 kB

✓ built in 204ms
Loading svelte-check in workspace: /Users/stevenforrester/Documents/GitHub/wyrd-ui-main-integration/crates/wyrd/wyrd-server/wyrd-ui/packages/ui
Getting Svelte diagnostics...

svelte-check found 0 errors and 0 warnings
Loading svelte-check in workspace: /Users/stevenforrester/Documents/GitHub/wyrd-ui-main-integration/crates/wyrd/wyrd-server/wyrd-ui
Getting Svelte diagnostics...

svelte-check found 0 errors and 0 warnings
$ pnpm package && pnpm --filter wyrd-ai-ui check && svelte-kit sync && svelte-check --tsconfig ./tsconfig.json
$ pnpm --filter wyrd-ai-ui build
$ node scripts/catalog.ts && svelte-package && vite build --config vite.app.config.ts
$ svelte-check --tsconfig ./tsconfig.json
```

## Workspace check/build

```sh
mise exec -- pnpm --dir crates/wyrd/wyrd-server/wyrd-ui build
```

Exit: 0

```text
$ node scripts/catalog.ts && svelte-package && vite build --config vite.app.config.ts
node_modules/.pnpm/zod@4.5.4/node_modules/zod/v4/core/util.js (330:0): A comment

"// Wrapped in a `@__PURE__` IIFE: esbuild never tree-shakes a top-level initializer that contains a member access on `Number`, so the bare object literal survived into every bundle."

in "node_modules/.pnpm/zod@4.5.4/node_modules/zod/v4/core/util.js" contains an annotation that Rollup cannot interpret due to the position of the comment. The comment will be removed to avoid issues.
node_modules/.pnpm/zod@4.5.4/node_modules/zod/v4/core/regexes.js (70:0): A comment

"/** Anchors a pattern source. The interpolation lives here rather than at the call site because
 * esbuild will not drop a `@__PURE__` call whose own argument interpolates a variable, but it
 * will drop `anchor(dateSource)`. Keeping it inline pinned `date` into every bundle. */"

in "node_modules/.pnpm/zod@4.5.4/node_modules/zod/v4/core/regexes.js" contains an annotation that Rollup cannot interpret due to the position of the comment. The comment will be removed to avoid issues.
Circular dependency: node_modules/.pnpm/zod@4.5.4/node_modules/zod/v4/core/core.js -> node_modules/.pnpm/zod@4.5.4/node_modules/zod/v4/core/util.js -> node_modules/.pnpm/zod@4.5.4/node_modules/zod/v4/core/core.js
Generated an empty chunk: "chunks/env.js".
```

## Workspace check/build

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

## Workspace check/build

```sh
mise exec -- pnpm --dir crates/wyrd/wyrd-server/wyrd-ui --filter @wyrd/example-ui-app build
```

Exit: 0

```text
✓ built in 1.14s

Run npm run preview to preview your production build locally.

> Using @sveltejs/adapter-node
  ✔ done
$ vite build
.svelte-kit/output/server/chunks/loaders.js (989:0): A comment

"/** Anchors a pattern source. The interpolation lives here rather than at the call site because
* esbuild will not drop a `@__PURE__` call whose own argument interpolates a variable, but it
* will drop `anchor(dateSource)`. Keeping it inline pinned `date` into every bundle. */"

in ".svelte-kit/output/server/chunks/loaders.js" contains an annotation that Rollup cannot interpret due to the position of the comment. The comment will be removed to avoid issues.
Generated an empty chunk: "chunks/env.js".
```

## Generated contracts

`mise run codegen:check`

Exit: 0

```text
✔ both application projections carry every approved role value in both modes (0.53825ms)
✔ projections emit no retired hard-shadow, arcade, or superseded font contract (0.239791ms)
✔ every Wyrd mark rendering keeps the exact README geometry (0.368667ms)
✔ an undocumented or aliased token is rejected (0.229791ms)
✔ a used pair below its contrast floor fails in the mode that breaks it (0.669792ms)
ℹ tests 5
ℹ suites 0
ℹ pass 5
ℹ fail 0
ℹ cancelled 0
ℹ skipped 0
ℹ todo 0
ℹ duration_ms 57.594166
All targets are in sync with palette.json.
Scope: all 3 workspace projects
Already up to date
Done in 158ms using pnpm v11.4.0
[codegen:check] $ set -eu
[codegen:regen] $ set -eu
    Finished `dev` profile [unoptimized + debuginfo] target(s) in 1.03s
     Running `target/debug/examples/gen_schemas`
Wrote 472 wire types to /Users/stevenforrester/Documents/GitHub/wyrd-ui-main-integration/sdks/wyrd-sdk-ts/wyrd/src/card-types.ts
```

## Identity and limits

HEAD remains `c2bbd8e47e4752b3772ad75acfaf23f3400ac704`; tracked source unchanged after verification. `git diff --check` exit 0.

No Linux bundles/images, live degraded-terminal parity, full Rust aggregates or fresh live Postgres/MCP journeys were rerun by the orchestrator. Browser remediation evidence in prior R1 tasks is historical; any fresh reviewer probes are recorded in their own reports. The full ten-test file was independently rerun by behavior-rev; root reran each of the nine exact selectors separately.
