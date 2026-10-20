# TASK-003 maintainer review

Result: **FAIL** (one material proposed finding).

Subject: repository `/Users/stevenforrester/Documents/GitHub/wyrd-ui-main-integration`; base `695ca914df61a8683059f6f32279ec2148424e06`; candidate `ab1d2499f6a08a6b5afe1f6c904e72746d526bda`. Reviewed cumulative range and candidate source. No source edits. Reports from other roles were not consulted.

Authorities: AGENTS.md, architecture/agent-rules.md, reference router, spec-driven-development.md, maintainer-style.md, TypeScript guide, architecture patterns; applicable shared-client doctrine/Bifrost boundary passages; approved spec revision 12 and original TASK-003. The provided navigation map was expanded through component consumers, SDK owners and tests. No `.codegraph` index exists.

## Changed-surface coverage

| Surface | Source and callers inspected | Maintainer assessment |
|---|---|---|
| Package/export/declaration contract | `packages/ui/package.json`, `src/lib/index.ts`, `contract.ts`, `schemas.ts`, package Svelte/TS config; imports in tests, gallery and custom app | Browser imports are type-only for SDK contracts; Node import is isolated to executor. Public read/result union is discoverable and sourced from SDK types. HTML artifact is also exposed as a package asset. No manually forked native declaration introduced. |
| Catalog/schema generation | `scripts/catalog.ts`, schemas and generated catalog, `mise.toml` codegen entry | Schemas own component input/action definitions and generated catalog; no second handwritten catalog or domain contract. Layout belongs to separately approved TASK-027. |
| Loader ownership and dependent reads | Complete `loaders.ts`, `node.ts`, `mcp.ts`; SDK `Cards.get/list`, `BifrostQueryStream.collect`; shared `Cards` handle and server MCP read owner navigation | New UI planning/projection is justified by portable-host requirement. Registry and query execution stay on existing owners. There is no pre-existing complete portable Card-header or chart workflow to extend instead. One duplicated column-resolution rule is materially inconsistent (MNT-003-01 below). |
| View lifecycle and actions | Complete `view.ts`, `WyrdView.svelte`, `ActionButton.svelte`; plain DOM mount test and gallery/custom-app consumers | Per-view abort controllers and per-panel action state have clear lifetimes; shared schemas validate inputs/intents. No generic mutation dispatcher or durable view owner introduced. Layout editing is TASK-027 scope, not additional TASK-003 drift. |
| Chart geometry/rendering | Complete `Chart.svelte`, `plot.ts`, `format.ts`; loader shape checks, fixtures and component tests | Geometry stays beside its renderer and pure transformations are synchronous. First-column binding rule in plot and stat disagrees with loader validation (finding below). Moved formatters retain behavior and test import is updated. |
| CardHeader | Complete component and `loadCardHeader/compareVersions`; exact reference, paginated listing, clipboard, version/tab/navigation consumers | Distinct Card and versions failure states are readable; the loader uses resolved Card identity for the dependent listing. Pure semantic sorting is local presentation work rather than new registry authority. |
| Other primitives | Complete EmptyState, CardRefChip, VerdictPill, StatusPill, Sparkline, DurationBar, Drawer; public exports and rendering test consumers | Small prop-driven components; Drawer uses native dialog lifecycle instead of custom focus trap. No extra abstraction required. |
| Theme and fixtures | `brand/gen-theme.mjs`, generated package theme, `ui-fixtures.ts`, `_gallery` page/load | Existing palette/generator extended rather than a second theme authority. Synthetic fixtures explicitly identified; gallery is dev-gated. |
| BFF integration | `ui/load/+server.ts`, `BrowserSession.client`, browser-session owner context and real-session integration test | Panel endpoint delegates to portable loader and SDK rather than duplicating reads; session credential forwarding stays with existing Rust client. No new session cache or OAuth owner. |
| MCP App artifact | `src/app/main.ts`, `vite.app.config.ts`, AppBridge integration helper and tests | One artifact entry and a short Vite emission plugin; transport remains host-owned. Lifecycle clears the previous mounted view before replacing it. |
| Custom app | Entire `examples/ui-app` package, executor, panels, direct load/page, view page/load endpoint, README/config/app shell | Demonstrates public-package consumers and server credential boundary without a new client. |
| Production packaging | In-scope `7acf4b55e` build/SDK-profile changes, `scripts/build.sh`, UI manifests/workspace/lock changes and Vite SSR external | Extends existing bundle owner; no duplicate bundle command. Linux builder/image changes are explicitly excluded commit `45797e42e`, not attributed to this task. |
| Shared Rust client | Full changed `Bifrost::connect_with`, `read_only`, `from_sink`, assembly and QueryOnlySink bodies; `WyrdClient::connected/is_browser_session`, new regression test | Existing concrete Bifrost owns state; one constructor consolidates assembly and query-only paths. Changed/new Rust symbols have substantive documentation. No alternate client/transport or optional feature added. |
| Verification infrastructure | Component/unit/live tests, exact task evidence, test setup, Vite configuration, UI scripts | Pure rendering, dependent reads, stale result rejection, duplicate prevention, MCP mapping, real SDK/MCP comparison, session and example consumers covered. Findings concern an uncovered typed-column edge, not test style. |

Unrelated merged main changes (SQL dependency/type-checker/gateway/Forge work) and explicitly excluded commits `45797e42e`, `2884da447`, `62966bbdd`, `fc0d28fda` were distinguished from TASK-003. Candidate TASK-027 layout additions remain separately approved scope.

## Material finding

### MNT-003-01 — Validation and plotting resolve duplicate column names differently

Classification: **INCORRECT**. Location: `crates/wyrd/wyrd-server/wyrd-ui/packages/ui/src/lib/loaders.ts:127-131`; consumers `plot.ts:18-19,32-46,63-69` and `components/Chart.svelte:153-158`.

Rule: maintainer style requires typed contracts and behavior to agree, and existing owners/mechanisms to be reused rather than diverging. AGENTS.md field/schema mapping rule and spec REQ-030 require positional duplicate-column preservation and incompatible chart schemas to render an error. `plot.ts` explicitly states the binding rule: first matching column.

Evidence: `shapeChart` builds a Map by name, retaining the **last** duplicate's type. All actual bound-value consumers use `columnIndex`, a `findIndex` selecting the **first** duplicate. Both executors preserve duplicate columns positionally, so the differing resolution rules are reachable with authored SQL. For columns `[v:Utf8, v:Int64]`, row `['abc',2]`, and a stat bound to `y:['v']`, the loader checks the second type, returns populated, and the component reads the first cell, converts it to NaN, and displays it as a successful numeric stat. Reversing the column types refuses a valid first numeric binding. Bar and time-series validation have the same cause for their named bindings.

The public `shapeChart` reproduction was run through the pinned toolchain against the built candidate package:

```sh
mise exec -- node --input-type=module -e 'import { shapeChart } from "./crates/wyrd/wyrd-server/wyrd-ui/packages/ui/dist/loaders.js"; const columns=[{name:"v",data_type:"Utf8",nullable:false},{name:"v",data_type:"Int64",nullable:false}]; const result={columns,rows:[["abc",2]],terminal:{outcome:"success",warnings:[],row_count:1}}; console.log(JSON.stringify(shapeChart({type:"stat",sql:"SELECT ...",y:["v"]}, result)));'
```

Observed state: `populated`. Source independently establishes that the renderer then reads `'abc'`. No production source was modified for this check.

Concrete maintenance cost: chart binding has two authorities with opposite answers. A maintainer adding a bound chart type cannot safely rely on its loader to have checked the cell the renderer consumes. Existing tests verify duplicate-name preservation in tables and executor parity but do not exercise duplicate names with conflicting types in a named chart binding.

Smallest correction: make shape validation use the existing first-match `columnIndex` mechanism for selecting the descriptor whose type it checks. Keep positional results untouched; do not reject all duplicate names, invent disambiguation fields, rewrite SQL or add renderer guards. All affected consumers are `timeSeries`, `barRows`, and stat extraction; table rendering continues positionally. Closure proof: focused pure shape tests in both duplicate-type orders and a renderer assertion that an incompatible first binding produces the existing shape-error state rather than a NaN stat. Exercise the shared selector once across relevant chart shapes; no new harness/dependency or full live suite is necessary for this correction.

## Verification and limits

Recorded candidate checks and human visual approvals were considered evidence, not rerun wholesale. The focused source/reproduction above was independently executed; no full UI build, native compile, live Postgres journey, degraded-terminal production path, Linux bundle/image or browser visual session was rerun by this role. These limits do not prove a new packaging defect. Source stayed at the recorded candidate during this review.

No optional refactor or preference-only finding is proposed. New portable loaders and host adapters are task-required and reuse existing complete backend/SDK owners; the retained finding only consolidates an inconsistent rule already present in the new UI owner.
