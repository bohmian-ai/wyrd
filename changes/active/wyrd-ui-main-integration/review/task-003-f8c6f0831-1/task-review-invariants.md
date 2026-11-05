# TASK-003 independent invariant review

Overall result: **PASS**. Proposed findings: **none**.

## Immutable subject and scope

Repository `/Users/stevenforrester/Documents/GitHub/wyrd-ui-main-integration`, branch `wyrd-ui-main-integration`. Cumulative base `c62ebce8c4de0d43a2b9d50d360de628c49fd533`, candidate `f8c6f083103d8f55007510a4fbcc75e36b4d9c55`. Authority is approved specification revision 13 and the original `tasks/TASK-003-shared-ui-foundation.md`, reconciled with root and SDK AGENTS, `changes/spec-driven-development.md`, maintainer guidance, the UI skill, Oracle query authority, and first-party browser-session security authority.

Inspected the cumulative changed production surfaces, configuration, tests and evidence, using the R7 diff only to locate the latest changed owners. Prior findings were hypotheses to check against current source, not acceptance authority. No CodeGraph index exists. The flat Card identity inherited from PR #205 is current authority, not a regression. TASK-027 layout mechanics are separately accepted and excluded; WyrdView's foundation validation, scheduling, cancellation and action seams remain reviewed. Peer reports were not read.

UI paths below are relative to `crates/wyrd/wyrd-server/wyrd-ui/` unless repository-relative paths are given.

## Producer-to-consumer coverage and existing owners

| Invariant and producer | Consumers and failure paths inspected | Existing alternative and ownership assessment |
|---|---|---|
| `schemas.ts`: untrusted view/panel/action input -> schema admission | `validateView`, `validatePanel`, `loadPanel`, WyrdView root/panel error rendering; both POST load handlers; custom example | Existing Zod schemas remain the single portable admission owner. No host-specific range arithmetic, second identity resolver or public configuration was added. |
| `bounds` -> `resolveRange` -> inherited absolute bounds | WyrdView `start`; explicit Chart range in `loadChart`; `remoteLoader`; BFF/custom POST `AbsoluteRangeSchema`; Chart brush action | Native Date performs arithmetic; the existing AbsoluteRangeSchema establishes its contract. The R7 extension is necessary because native Date alone admits signed expanded years. No broader retention policy or schema widening is needed. |
| Authored SQL -> `highestBind`/`chartParams` -> exact `executed` values | `loadChart`, public `loadPanel`, WyrdView local/MCP and remote loads, direct custom app, Chart `chart.open_in_query` | No portable tokenizer exists in these package/app owners. Narrow lexical recognition stays in the existing bind planner; Oracle/DataFusion remain SQL parsing, table, permission and binding authority. No parser service, dependency or SQL rewrite was introduced. Compared quote/comment/token handling against the installed sqlparser 0.63 GenericDialect implementation used by the query owner. |
| Exact flat `CardRef` -> registry response -> paged versions | Node `Cards.get/list`, MCP `cards.get/list` envelope adaptation, `loadCardHeader`, Header/Chip/formatter, version and Observe intents | Current registry identity and generated SDK contracts are reused. The version list is dependent on the resolved Card name/kind; denied prerequisite stops it. New version intents discard a UID belonging to the old version. |
| Ordered Arrow schema/batches -> shared `JsonResultBuilder` | Server `ResultCollector::consume`, native `NativeBifrostQueryStream::collect`, SDK `BifrostQueryStream.collect`, Node/MCP adapters -> shape/plot/table | The former server projection is extracted into one shared owner, rather than duplicated in JavaScript. Existing stream owners retain schema/protocol/EOS/terminal validation, settlement and cancellation. |
| Original JavaScript numeric ceiling -> lossless parse -> `admit_ceilings` | Native collect takes/releases stream on refusal; server admission; integration refusal and subsequent-query tests | Shared range policy and encoder are reused. Fractional/wrapping values are refused before rows, rather than narrowed/clamped. No second ceiling policy. |
| Active load controller -> result settlement and dependent reads | WyrdView replacement/removal/destroy, `loadCardHeader` paging, Node acquisition/collect, native notification before lock acquisition, MCP AbortSignal forwarding | Existing AbortController/stream close mechanisms suffice. Native collect owns its stream; cancellation releases it rather than returning retained rows. Controller membership and abort checks fence late results. |
| Rust session -> request-scoped SDK transport -> domain handles | `BrowserSession.client`, BFF load, shared client `connected/is_browser_session`, Bifrost `connect_with/read_only`, query-only sink | Reuses the existing HTTP/session boundary and existing query-only construction. It adds no token exchange, credential store, ingest authentication mode or tenant selector. Normal credential connections still use the existing assembly path. |
| Normalized result -> positional bindings -> plotted identity/captions | `shapeChart`, `columnIndex`, `timeSeries`, `barRows`, Chart geometry, legend, tooltip, stat/table, degraded/error states | First-position duplicate-name resolution is shared. Typed `(series,y)` keys precede aggregation; final captions distinguish authored and aggregate identities. Native SVG/platform dialog and existing tokens are used, without a chart or modal framework. |
| Package source -> generated catalog/theme -> package/AppBridge artifact | Workspace/build scripts, six public functionality entry points and HTML artifact, catalog generator/check, gallery, custom app, integration AppBridge | `svelte-package`, Vite and the existing authoritative build script are extended. No production MCP resource registration, saved-view store or Ask service is bundled into this task. |

## Reconciled acceptance matrix

| Requirement, criterion, constraint or non-goal | Implementation evidence | Verification evidence | Result |
|---|---|---|---|
| REQ-030 reusable package, portable planning and approved host boundaries | `packages/ui/package.json`, `index.ts`, `contract.ts`, `loaders.ts`, `node.ts`, `mcp.ts`, `view.ts`, App main and host handlers | Supplied UI check/test/catalog results; source import separation; retained package/import/build evidence | PASS |
| Generated wire domains and catalog are authoritative; browser entries load no native SDK | Type-only domain imports; schema-derived `catalog.ts`; canonical problem JSON import with standard attribute; SDK declarations | Browser import regression, catalog drift proof, supplied check/catalog green | PASS |
| Relative bounds resolve once per view and explicit range takes precedence | `schemas.ts:bounds/resolveRange`; WyrdView `start`; `loadChart` | Shared six-hour binds, brush/reset/explicit precedence tests; actual source schema probe below | PASS |
| Invalid root/panel ranges are canonical field errors before IO and never invalid AbsoluteRanges | `bounds` checks AbsoluteRangeSchema; RelativeRangeSchema rejects unresolvable input; existing root/panel/endpoint admission precedes loads | Fixed-clock year-zero and former overflow cases; initial/replaced local/remote root and isolated explicit-panel assertions; endpoint no-read assertions; supplied tests green | PASS |
| Authored SQL is unchanged; only actual range placeholders bind and Open in Query receives exact values | `highestBind`, `chartParams`, `loadChart.executed`, Chart action input | Recording executor cases for strings/doubled quotes/E escapes/identifiers/dollar strings/line+nested comments, real/inert mix, no-range refusal, exact action values | PASS |
| REQ-025 all Chart types, schema mismatch, first duplicate-name binding, signed geometry and twenty-series threshold | `shapeChart`, `plot.ts`, Chart | Mounted signed/flat/zero, duplicate-column, 5/20/21 series, authored-caption collision, dense bar, stat/table, empty/denied/error/loading tests | PASS |
| Exact CardHeader identity, versions, lifecycle and interactions | `loadCardHeader`, `compareVersions`, CardHeader, `fmtCardRef` | Exact get/list request assertions, paging/truncation and failed listing, kind-sensitive copy, UID behavior, version/tab pending and outcomes | PASS |
| Failed prerequisites stop dependent reads; superseded results cannot replace current state | `loadCardHeader` prerequisite return and per-page abort; WyrdView controllers/start/settle | Denied prerequisite, document replacement, removed panel/late result and acquisition/collection cancellation assertions | PASS |
| Node/MCP normalized query parity preserves positional names/nulls and terminal evidence, with no usable partial failure | Shared Rust builder; server consume and native collection; SDK result projection; adapter error mapping | Real-server parity integration for populated/empty/duplicate/null/failed-bind/limits/denial; source establishes shared degraded projection; supplied integration 2/2 | PASS |
| Bounded collection admits lossless original ceilings and releases refused streams | `parse_ceiling`, `admit_ceilings`, shared byte accounting and native ownership | Integration fractional/wrapping/out-of-range/maxima/refused-stream/subsequent-query assertions; historical focused Rust proof | PASS |
| Typed intents, pending deduplication, canonical failure and truthful unavailable outcome | Schema action declarations; WyrdView `act`; ActionButton/ActionNote; CardHeader controls | Direct component and WyrdView action regressions, canonical problem tests | PASS |
| INV-001/002 current contracts, Rust durable/session/query ownership, tenant/credential isolation | Closed generated read union; session-bound BFF; shared client query-only path; existing Oracle/registry boundary | Real Rust session BFF integration: cross-tenant Card refusal, no secret in props, unauthenticated/malformed refusal, direct SDK/MCP denial | PASS |
| INV-003/004 small integration and authoritative build | Existing build script builds/packs SDK + native addon and UI; shared constructor/projection owners | Historical darwin-arm64 production closure/import proof; source confirms required runtime dependencies | PASS |
| T1/T2/T3 foundation rendering/loading/view validation and primitives | Chart/Header plus EmptyState, CardRefChip, VerdictPill, StatusPill, Sparkline, DurationBar, native Drawer; fixture/gallery | Mounted unit source and supplied 40/40; historical human desktop/mobile light/dark approval | PASS |
| T4 complete built browser artifact; T5 real SDK/server authorization reads and custom app | App main/inline build, integration `appBridgeView`, custom app routes and executor | Supplied real-server integration 2/2; historical artifact/custom-app and gallery checkpoint | PASS |
| REQ-020/AC-010 provenance and existing visual baseline retained | Canvas/reference evidence remains preserved; gallery/task evidence identifies comparison and approved reinspection | Existing preservation/human checkpoint record; no R7 reference or visual change | PASS |
| INV-005/006/007 exclusions, narrow proof, no speculative platform/workflow | R7 only changes existing schema/loader/tests and evidence; no new runtime owner | Actual cumulative scope separated from PR #205 and TASK-027; supplied narrow R7 proof | PASS |
| No per-component route, generic write dispatcher, A2UI adapter, saved-view store, Ask, production MCP registration or publishing | Read-only shared union, single host endpoint, package source and example | Cumulative source and export/build inspection | PASS |

## Prior findings: source-backed disposition

| Prior ID | Current producer/consumer evidence | Disposition |
|---|---|---|
| FIND-003-1 | `loadChart` selects input.range before context.range; WyrdView resolves root once; executed params drive action handoff | CLOSED |
| FIND-003-2 | Shared `columnIndex` is first-position and feeds validation, lines, bars, stat; tables retain positional cells | CLOSED |
| FIND-003-3 | Chart signed domains include zero; line domain increases; bars span signed value to zero with nonnegative height | CLOSED |
| FIND-003-4 | `MAX_SERIES=20`; only greater counts fold to nineteen plus aggregate; style indices remain defined and shared by legend and plot | CLOSED |
| FIND-003-5 | Header version/tab consume named host action state; pending/unavailable disable controls; notes and authoritative selection remain visible | CLOSED |
| FIND-003-6 | Node handles already/post-acquisition/in-collect abort; native close notifies before waiting for the held lock; renderer fences stale controllers and listing checks abort | CLOSED |
| FIND-003-7 | `wyrdProblem` derives local complete problems from generated Rust examples; errors/denials preserve code/title; both endpoint refusal helpers use canonical problem+json | CLOSED |
| FIND-003-8 | `contract.ts` uses `with {type:'json'}`; browser entries have only type imports from native SDK; public package/import proof remains retained | CLOSED |
| FIND-003-9 | Aggregate caption is chosen against retained authored captions; legend/tooltip/plot consume the same final distinct names | CLOSED |
| FIND-003-10 | Typed JSON pair keys distinguish series/y before aggregation; collision qualification distinguishes null, text-null and multi-y captions | CLOSED |
| FIND-003-11 | N-API accepts original f64; decimal integer parse precedes shared ceiling admission after taking stream; refusal releases stream; shared builder accounts complete compact JSON | CLOSED |
| FIND-003-12 | Shared formatter includes kind/name/version and optional supplied UID; Header/Chip reuse it; version intents omit old UID | CLOSED |
| FIND-003-13 | R7 `bounds` returns only ranges accepted by AbsoluteRangeSchema; schema admission rejects prior native-valid expanded years and native-invalid spans before host dispatch; all existing callers reuse admission | CLOSED |
| FIND-003-14 | Changed client regression has its named `# Panics` documentation; assertion intent remains intact | CLOSED |
| FIND-003-15 | R7 `highestBind` consumes inert tokens before real `$1..$3`; `chartParams` still refuses real placeholders without range; loader preserves SQL and exact executed params | CLOSED |

No prior finding is reopened and no distinct bounded defect was established. No new product, public API, architecture, security, concurrency or persistent-data decision is required.

## Independent probe and verification limits

Ran an artifact-free Node import of the actual candidate `schemas.ts` through `mise exec`, fixed Date.now to `2026-10-22T00:00:00Z`, and checked RangeSchema then AbsoluteRangeSchema on every successful resolution. Results: `6h` and `24h` admitted with valid absolute output; `100000000d` and `1000000000000d` refused. This directly closes the formerly valid-native/invalid-contract producer condition. The mounted renderer/endpoint and bind-scanner assertions were inspected in the actual test source; supplied green runs are reported executions, not rerun claims.

Supplied candidate verification: UI check, 40/40 tests, catalog --check, fmt:check, real-server integration 2/2. No Rust changed in R7. Historical platform/bundle and human inspection evidence remains historical; source continuity and narrow R7 scope do not imply fresh visual or release qualification. This audit did not rerun builds or tests that could write outside the packet, launch a browser/server, or perform image/Linux/native platform qualification. No source/test modification, commit or push occurred.

Skipped: build/test/deployment reruns under the packet-only restriction. Risk: platform and visual results retain their recorded historical limits; no new blocking invariant defect was found.
