# TASK-003 behavior review

Overall: **FAIL**.

Subject: base `695ca914df61a8683059f6f32279ec2148424e06`, candidate `c2bbd8e47e4752b3772ad75acfaf23f3400ac704`; HEAD independently checked at that candidate. Original task `tasks/TASK-003-shared-ui-foundation.md`, approved revision-12 specification, prior verdict/validated ledger, and R1 remediation with appended evidence were reviewed. No peer reports from this attempt were read. Only this report is authored; source remains unchanged.

Paths below are relative to `crates/wyrd/wyrd-server/wyrd-ui/` unless otherwise qualified. Reviewed the cumulative range and latest remediation range with the subject's scope attribution: separately approved TASK-027 layout work and unrelated upstream commits are not foundation drift. No CodeGraph index exists.

## Authority and complete caller audit

Applied AGENTS.md, agent-rules, reference router, spec-driven-development, maintainer-style, applicable design/doctrine, errors/testing and UI authority, original task, REQ-025/030/033, INV-001–007 and mapped acceptance criteria. The task cannot override canonical errors, durable Rust ownership, tenant authorization or native/browser separation.

Traced the complete portable read path: catalog/schema validation → `loadPanel` → `loadChart`/`loadCardHeader` → closed Executor union → Node SDK handles or MCP `App.callServerTool` → normalized response → `shapeChart`/plot/component. Traced WyrdView document validation, per-panel loading, controller cleanup, stale-result refusal, action validation/pending/outcome delivery and range brush/reset. Read the complete changed loader, plot, Node/MCP adapter, renderer, Chart, CardHeader, ActionButton/ActionNote, every exported primitive, public exports/manifests, catalog generator, built artifact entry, gallery, both custom pages and both panel endpoints. Inspected shared-client browser-session/query-only construction and build.sh SDK packaging closure. SDK BifrostQueryStream/native close and server query collection remain existing normalization/cancellation owners; browser-session authority remains Rust-owned. No alternate durable read, auth or query owner is necessary.

Existing alternatives assessed: Cards.get/list already resolve exact identity and paginate; the portable header adds only UI projection. Bifrost.stream/collect already own terminal/limit/Arrow normalization; adapters reuse them. `QueryOnlySink`/`from_sink` reuse the existing read-only client shape. `wyrd-spec` generated problem examples already own canonical metadata; reusing their projection is justified. Native ESM already supplies the JSON import mechanism needed by that projection (see B-003-9). The existing `timeSeries` owner already owns series aggregation; collision correction belongs there (B-003-8), without a second renderer normalization path.

## Acceptance matrix

Verification notation: **fresh** = reviewer-executed; **recorded** = supplied original/remediation execution evidence, assessed against actual assertions/source. A PASS here means the mapped implementation behavior passes, with explicitly retained platform/live-proof limits below.

| Requirement, acceptance criterion, constraint or non-goal | Implementation evidence | Verification evidence | Result |
|---|---|---|---|
| Approved package and six public entry points work from package contents | package.json exports/dist; loaders transitively imports contract.ts:19 | Fresh plain Node import fails; B-003-9 | FAIL |
| Browser entry points never acquire native SDK/credentials | index.ts/loaders.ts/mcp.ts/view.ts use SDK types only; node.ts sole runtime SDK importer; built App entry uses MCP | Fresh package artifact build; recorded UI/example build and T4 | PASS |
| Catalog generated from sole component input/action schemas; drift checked | schemas.ts, scripts/catalog.ts, catalog.json, mise codegen:check | Fresh package generation/build; recorded codegen check | PASS |
| Chart supports timeseries/bar/stat/table and honest loading/empty/denied/error | shapeChart, Chart.svelte, EmptyState | Fresh ten-test file; recorded live populated/empty/denied | PASS |
| Exact SQL/typed binds returned for Open in Query; panel range beats inherited range | loadChart:155–164; Chart executed action; WyrdView range propagation | Fresh assertion groups for brush/reset/two-panel binds and relative clock | PASS |
| Duplicate positional columns remain intact and named bindings validate the rendered first occurrence | shapeChart need/columnIndex; table cells remain positional | Fresh conflicting-type/order tests across chart roles/table | PASS |
| Numeric signed domains and zero baselines produce usable plots | Chart bounds/sy and barDomain/barY/barCells | Fresh mounted negative/mixed/flat/zero assertions; recorded browser geometry/label spot-check | PASS |
| Up to twenty series remain distinct; overflow folds safely into other | plot.ts:32–58; Chart keyed lines and legend | Fresh threshold tests pass but independent literal-other probe fails identity uniqueness; B-003-8 | FAIL |
| Exact Card version/space/identity; denied prerequisite issues no dependent read | loadCardHeader exact read then resolved-space listing; CardHeader ref and copy | Fresh dependency/exact identity assertions; recorded live read | PASS |
| Paging, truncation and version-list failure stay honest without hiding Card | loadCardHeader bounded pages/cursor/sort; CardHeader versions state | Fresh version paging/failure assertions | PASS |
| Declared version/tab/Observe action results remain visible, pending suppresses duplicate interaction | CardHeader versionState/tabState and disabled/aria-busy; ActionNote; WyrdView act | Fresh direct-state and renderer outcome assertions | PASS |
| Invalid panel alone fails; non-Svelte DOM mount and current-generation results | validateView/validatePanel, WyrdView start/settle, mountView | Fresh valid/invalid sibling and mounted stale-load assertions; recorded T4 | PASS |
| Input/range/removal/unmount supersedes loads; dependent work stops | WyrdView controller cleanup; loadCardHeader signal check; executor signal forwarding | Fresh supersession/removal and cancellation assertions | PASS |
| Query acquisition cancellation closes acquired stream and cannot collect/succeed | node.ts:29–46; SDK stream.return owns close | Fresh deferred-start/during-collect/pre-abort assertions | PASS |
| All six reads adapt existing authorized SDK/MCP surfaces | node.ts switch; mcp.ts tool mapping to actual cards.get envelope | Recorded live integration tests; source confirms unchanged owners | PASS |
| Positional normalized rows/terminal/limits, upstream codes and denied mapping preserved | Node collect, MCP structuredContent, contract.failed, shared JsonResultBuilder owner | Recorded MCP/SDK duplicate/null/ceiling/denial checks; source traces; degraded live proof remains outstanding | PASS |
| Canonical UI error identities and full safe HTTP validation metadata | contract.wyrdProblem, generated problems.json; inputProblem; both refuse responses | Fresh canonical errors/HTTP assertions; native consumption regression separately B-003-9 | PASS |
| Session-bound BFF cannot choose caller authority/tenant and leaks no credential | ui/load uses locals.browserSession.client(), validated component/input/range; existing auth hook owns tenant selection | Recorded real session/foreign Card/malformed input proof; no new credential mechanism | PASS |
| Direct/view custom app and built AppBridge HTML; useful gallery/primitives | examples/ui-app, app/main.ts/vite.app config, _gallery and every primitive | Fresh artifact build; recorded real custom/T4 journeys and human component inspection | PASS |
| Production native SDK packaging uses sole build owner | scripts/build.sh builds/packs SDK/addon, installs runtime deps and imports handler/SDK | Recorded macOS arm64 bundle smoke; Linux/images remain unverified | PASS |
| Preserve immutable reference assets; no new persistence/Ask/A2UI/production MCP registration/publication | Scope-attributed cumulative diff and original task exclusions | Diff inspection; source contains no such owners | PASS |
| Preserve TASK-027 shared renderer behavior | Separate layout owner, content/source derived reload keys; action/range logic remains foundation-owned | Fresh ten-test file includes successor layout assertions; separate review owns detailed layout verdict | PASS |

## Prior finding closure

| Stable finding | Independent candidate evidence | Assessment |
|---|---|---|
| FIND-003-1 | loadChart explicitly chooses/resolves input.range before context.range; both BFFs forward context; WyrdView brush/reset changes inherited bounds only | Original cause CLOSED; fresh tests exercise exact bind precedence |
| FIND-003-2 | need() uses existing first-match columnIndex; stat/timeSeries/barRows use the same selector; table untouched | CLOSED; both conflicting type orders tested |
| FIND-003-3 | bounds includes zero and rounds signed sides outward; bar geometry spans min/max against zero, with reserved negative-label floor | CLOSED; fresh mounted assertions plus recorded browser evidence; prior reviewer RED is substantive proof, not permission to weaken tests |
| FIND-003-4 | MAX_SERIES is 20; style(index) covers all twenty combinations; 5/20/21 threshold assertions corrected | Original early-fold cause CLOSED; independent aggregate-name collision is B-003-8 |
| FIND-003-5 | version/tab states now reach disabled/aria-busy and shared ActionNote; select resets to authoritative input version | CLOSED; fresh direct and WyrdView assertions |
| FIND-003-6 | abort checked after acquisition and collection; acquired cancelled stream reaches return before collect | CLOSED; fresh deterministic deferred-start and collection assertions |
| FIND-003-7 | local producers use generated validation/internal/upstream metadata; HTTP responses retain type/details/remediation; upstream identities remain intact | Canonical-projection cause CLOSED; added import breaks native package consumption, B-003-9 |

## Material proposals

### B-003-8 — legitimate `other` series collides with overflow aggregate

Classification: **INCORRECT**. Obligation: REQ-025/030 complete timeseries rendering, distinct series through the threshold and truthful overflow folding. Location: `packages/ui/src/lib/plot.ts:48–54`, consumed by `components/Chart.svelte` keyed `each lines ... (line.label)` in plot, tooltip and legend.

Producer-to-result: normalized queries permit arbitrary text series values. `shapeChart` correctly accepts them. `timeSeries` groups a literal `other` as normal data, sorts it with every other group and retains the top nineteen. It then appends a second entry named `other` for overflow. The Chart uses those labels as keyed identity in three sibling consumers. A top-ranked literal `other` plus twenty other series produces twenty outputs with only nineteen unique identities. Svelte's existing keyed-each implementation rejects duplicate keys (installed runtime `src/internal/client/dom/blocks/each.js`, validation and reconciliation), so a valid query can break the Chart instead of rendering its data. All direct, BFF/view and MCP hosts share this producer; the problem is not host-specific.

Independent in-memory probe, with no source/fixture changes: imported built `plot.js`, supplied twenty-one one-point text series at one valid timestamp, literal `other` having highest numeric total, then inspected `timeSeries(result, 'at', ['v'], 'series')`. Output labels start with `other` and end with `other`; count=20, unique=19. Existing numeric-label cardinality tests never exercise this valid value.

Smallest testable correction: retain the single timeSeries folding owner and ensure its synthetic aggregate has a distinct identity from retained real series. The correction must preserve every input value and deterministic ranking/fold totals, including when a real series is called `other`; do not merely change a renderer key, hide one entry, reject a text value or add a second cap. A collision-safe aggregate identity/display choice within the existing Series/Chart boundary is sufficient; no query/public protocol or dependency is needed. Focused proof should mount the existing Chart with twenty-one series including a retained literal `other`, verify no keyed-render failure, distinguish real and aggregate series and check their values. Preserve ordinary 5/20/21 behavior.

### B-003-9 — canonical JSON projection breaks plain Node consumption of public loaders

Classification: **REGRESSION**. Obligation: TASK-003's first acceptance criterion (exports work from package contents), REQ-030 portable loaders usable on the user's server, and preservation of Node host support. Location: `packages/ui/src/lib/contract.ts:19`; emitted `packages/ui/dist/contract.js` preserves the same import.

Producer-to-result: R1 adds `import problems from './problems.json'` to the shared runtime contract. Both loaders.ts and mcp.ts import runtime values from contract.ts. The package emits ordinary ESM with svelte-package and exports `dist/loaders.js`; package.json type is module. Plain Node requires an import attribute for JSON modules. A public loader import therefore fails during linking before any loader or SDK read can run. The first-party and example Vite builds transform JSON and mask the defect. The nearby first-party server problem.ts import is not sufficient precedent: that module runs through Vite, while these public ESM exports expressly serve independent Node consumers.

Independent fresh probe, executed from the UI workspace under `mise exec -- node --input-type=module`: an import of `./packages/ui/dist/loaders.js` failed with `TypeError [ERR_IMPORT_ATTRIBUTE_MISSING]: Module .../dist/problems.json needs an import attribute of type: json` on Node v26.2.0. The same command also imported plot.js but module linking failed before its body; a separate plot-only probe succeeded. This is unrelated to running the example from its own cwd or resolving workspace dependencies: the failure is inside the built package's valid relative JSON import.

Smallest correction: use standard ESM JSON import attributes at this existing metadata consumer, preserving the source-generated browser-safe file and the package's native-free boundary. No new loader, dependency, bundling framework or handwritten catalog is needed. Add a plain Node smoke import of public `wyrd-ai-ui/loaders` (and `wyrd-ai-ui/mcp`, which shares contract runtime) from built package contents; do not rely solely on Vitest/Vite transformation. Keep canonical metadata and existing browser/AppBridge build proof green.

## Verification and limits

Fresh command: `mise exec -- pnpm --dir crates/wyrd/wyrd-server/wyrd-ui test src/lib/ui-library.test.ts` — package/catalog/App HTML built, 10 tests passed. No package builds remained active after completion; the orchestrator was notified before running its own sequential checks. The independent read-only native Node probes above produced the two material results. HEAD stayed `c2bbd8e47e4752b3772ad75acfaf23f3400ac704`.

Recorded original live SDK/MCP/BFF/custom-app and Rust evidence is credible for its tested cases, and appended remediation commands address the original seven causes. Green transformer-backed tests do not prove native ESM linking; numeric series names do not prove collision-safe folding. Human visual acceptance dated 2026-10-20 is supplied evidence, not a substitute for either reachable behavior defect.

Skipped: fresh infrastructure/Rust/release runs, new human inspection, Linux native/image execution and live degraded-terminal parity. Risk: two reproducible package/chart defects block acceptance; Linux and live degraded-result proof remain explicitly unverified at the owning verification stage.
