# Independent maintainer review — PASS

Subject: cumulative TASK-003, base `c62ebce8c4de0d43a2b9d50d360de628c49fd533` through candidate `f8c6f083103d8f55007510a4fbcc75e36b4d9c55`, in `/Users/stevenforrester/Documents/GitHub/wyrd-ui-main-integration`. Approved authority is specification revision 13 and the original `tasks/TASK-003-shared-ui-foundation.md`.

Overall result: **PASS**. No material maintainer finding is proposed. This is an independent discovery report, not the final task verdict.

## Scope and governing rules

Read the cumulative Git range and materially changed owners and their consumers, then used `21dab5d11...f8c6f0831` to locate the R7 corrections. The starting map was expanded through shared result collection, SDK native stream lifetime, package exports, host boundaries, generated catalog/theme owners, primitives, and test consumers. No `.codegraph/` exists. Root `AGENTS.md`, SDK TypeScript `AGENTS.md`, `agent_docs/maintainer-style.md`, the architecture authority map, applicable Card/Bifrost boundaries, spec-driven development, and UI skill guidance govern this review. No nested UI `AGENTS.md` is present.

The threshold is a concrete cost or consequence, not an alternative spelling or preferred decomposition. PR #205's flat Card identity is current authority. TASK-027 layout is not reopened; only its interfaces with foundation load supersession and Chart sizing were traced. Historical review packets were used for prior finding hypotheses, not as proof of current source. No current peer discovery report was consulted.

## Changed-surface coverage

Paths below are repository-relative; `UI` means `crates/wyrd/wyrd-server/wyrd-ui`.

| Surface | Owning source and consumer tracing | Maintainer assessment |
|---|---|---|
| Public input and intent contracts | `UI/packages/ui/src/lib/schemas.ts`, `contract.ts`, `index.ts`; `loaders.ts`, `view.ts`, Chart/CardHeader, BFF/example callers | PASS. Generated SDK types carry domain contracts. Catalog dispatch is derived from the same schema objects as runtime validation. Open `unknown` is narrowed at document/panel/action boundaries. Browser imports of domain types remain type-only. |
| Range admission and resolution | `schemas.ts:45–95,212–217`; `loadChart`, `loadPanel`, `WyrdView::start`, `remoteLoader`, both load endpoints, direct example server load | PASS. R7 makes `AbsoluteRangeSchema` the common postcondition in private `bounds`, reused at admission and resolution. One local helper replaces the inconsistent Date-only criterion; it does not add endpoint-specific guards or widen the public date contract. Root range resolution remains once per load; panel overrides stay with the existing loader. |
| Authored chart SQL planning | `loaders.ts:85–155,203–211`; Chart `executed` action, all local/remote loader entry points; new cases in `ui-library.test.ts:453–484` | PASS. `highestBind` is private beside its only caller, recognizes only chart bind positions and skips lexical containers. Its documentation names the supported token boundaries. It neither rewrites SQL nor duplicates server SQL admission/planning. No installed SQL tokenizer or existing client-side owner in this package replaces this small inspection; the existing Rust parser remains the owner of SQL validity and execution. |
| Chart shaping and plots | `loaders.ts::shapeChart`; `plot.ts::columnIndex,timeSeries,captions,barRows`; `format.ts`; full Chart component, caption/tooltip/action consumers | PASS. Shape validation and geometry have distinct responsibilities. Positional columns resolve through one owner, signed domains include zero, typed series groups remain distinct, and aggregate captions cannot collide with authored captions. Plot styles and legend/tooltip share the same series identity. No new chart framework or speculative renderer is introduced. |
| Exact Card presentation | `loaders.ts::loadCardHeader,compareVersions`; full CardHeader, CardRefChip, `format.ts::fmtCardRef`; gallery/example/test consumers | PASS. Shared exact reads precede dependent paged version listing; failures remain separate. Formatting has one owner. Version selection and host-navigation actions remain host-controlled and never become a registry/identity resolver. |
| Primitives and state feedback | ActionButton, ActionNote, EmptyState, StatusPill, VerdictPill, DurationBar, Sparkline, Drawer; exported entries, shared fixtures and gallery | PASS. Small presentational owners reuse action/state and token mechanisms. Native dialog owns modal focus/Escape mechanics. State words accompany visual status. No extra component framework or data loader is imposed on data-free primitives. |
| Renderer and host adapters | `view.ts`, foundation portions of `WyrdView.svelte`, `node.ts`, `mcp.ts`, `app/main.ts`; BFF `ui/load`, BrowserSession.client, custom-app direct/document pages | PASS. Executor/loader/component ownership is discoverable. Renderer owns generations and per-panel action state; transports own credentials. Node uses existing SDK facades and MCP maps to existing tools. Aborts fence late results and reach the query stream. Host routes perform schema validation and delegate shared loading instead of implementing component-specific workflows. |
| Bounded result and collection lifetime | `crates/shared/wyrd-queue/src/json_result.rs`; `wyrd-server/src/query/collect.rs`; SDK `native/src/lib.rs::NativeBifrostQueryStream::{collect,close}`, `parse_ceiling`; TS BifrostQueryStream.collect and generated declarations | PASS. JsonResultBuilder owns the shared positional projection and exact byte accounting; both consumers reuse it. Existing server settlement and native stream cancellation remain at their lifecycle owners. The native boundary preserves original Number values before shared ceiling admission. Changed Rust helper and method docs state the relevant failures and invariants; the browser-session test includes its Panics section. |
| Session query-only assembly | `wyrd-client/src/client.rs::is_browser_session,connected`; `bifrost/facade.rs::{connect_with,read_only,from_sink,assemble}`, QueryOnlySink; BFF Node consumer | PASS. A single construction path backs existing internal query-only and browser-session clients. The change extends the current handle rather than building a second transport or credential workflow. |
| Package generation and shipping | UI workspace/manifests/lock/configs, `packages/ui/scripts/catalog.ts`, generated catalog/problems/theme, brand generator, `mise.toml`, `scripts/build.sh`, SDK manifest/declarations | PASS. Generation extends existing owners; catalog/problem and token projections are not competing authored contracts. Public exports identify browser-safe and Node surfaces. Production packaging uses the existing SDK pack/addon build rather than a separate native client artifact. |
| Verification consumers and records | `ui-library.test.ts`, `ui-library.integration.test.ts`, format regression, fixtures, gallery, example, task/evidence documents | PASS within stated limits. Tests assert caller-visible request bytes, bounds, panel errors, positional results and action state, rather than solely helper internals. R7 extends existing focused scenarios and fake Date setup rather than introducing a harness or timing sleeps. Layout-only scenarios remain TASK-027. |

## Existing workflow comparison

The cumulative foundation introduces loaders, schema dispatch and rendering because those portable outcomes do not have an existing application owner. It consumes `Cards.get/list`, `Bifrost.stream`, existing verification status, Rust browser-session transport, and existing MCP tools. `JsonResultBuilder` replaces duplicated JSON projection at the common Arrow owner while keeping stream validation/settlement in its existing callers. The query-only branch shares its constructor and refusal sink with the existing capability. Catalog and CSS outputs extend existing generation/check owners.

R7's new lexical scan has one task-specific responsibility: choose range parameters for unchanged authored Chart SQL. Extending the Rust parser into another transport/service would add a new workflow and is unnecessary. Likewise `bounds` centralizes the established absolute-range contract instead of requiring every consumer to recheck it. These choices are justified by actual consumers, not prospective flexibility.

## Prior-findings maintenance closure

| Prior IDs | Current-source evidence relevant to this lens | Assessment |
|---|---|---|
| FIND-003-1, -2 | `loadChart` gives explicit input range precedence; shaping and plot binding call `columnIndex`, resolving the same first positional column | Remain closed. |
| FIND-003-3, -4, -9, -10 | Chart signed domains and twenty marker/dash combinations; typed grouping, collision-safe captions and other-series naming in `plot.ts` | Remain closed. |
| FIND-003-5, -6 | Host action-state ownership in WyrdView/CardHeader/ActionButton; Node acquisition abort and native close/collect cancellation, current-generation settlement | Remain closed. |
| FIND-003-7, -8 | Canonical generated problems and endpoint problem envelopes; standard JSON import attribute in `contract.ts` with public loader/MCP smoke import in UI test command | Remain closed. |
| FIND-003-11 | Native `Option<f64>` ceilings pass through lossless `parse_ceiling`, stream is taken before refusal, shared `admit_ceilings` owns range policy | Remains closed. |
| FIND-003-12 | Shared flat `fmtCardRef`, header and chip consumers; versions/Observe intents contain resolved exact kind/name/version | Remains closed. |
| FIND-003-13 | `bounds` now safely parses its computed range through `AbsoluteRangeSchema`; schema refinement and resolver reuse it. Frozen-clock cases prove year-zero boundary admission and rejection, local/remote root refusal, panel isolation, endpoint no-read behavior | R7 closes the prior source mismatch. |
| FIND-003-14 | `wyrd-client/src/client.rs::tests::browser_session_bifrost_connects_without_an_ingest_channel` documents the precise named panic conditions; assertions are retained | Remains closed. |
| FIND-003-15 | `highestBind` skips quoted strings/identifiers, dollar strings, comments and identifier suffixes before `chartParams`; tests assert unchanged SQL/empty binds and mixed real binds through `loadPanel/loadChart` | R7 closes the diagnosed literal/comment misrecognition. |

## Findings and verification limits

Proposed material findings: **none**. Non-blocking maintainer notes: none.

The supplied candidate checks are UI check, unit tests 40/40, catalog check, format check, and real-server integration file 2/2. They are supplied evidence, not checks rerun by this reviewer. Historical human visual approval and Darwin packaging evidence are retained as historical; this review does not add Linux/image, live degraded-terminal, browser or platform qualification. No source/tests were modified and no build/test commands with external write effects were run. HEAD read during this review was exactly `f8c6f083103d8f55007510a4fbcc75e36b4d9c55`.

Skipped: fresh builds, test execution and visual/platform qualification under the packet-only write restriction. No maintainer-specific material risk was identified; recorded runtime/proof limits remain for the integrated verdict.
