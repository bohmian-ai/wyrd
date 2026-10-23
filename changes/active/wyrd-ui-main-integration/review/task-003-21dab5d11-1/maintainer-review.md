# TASK-003 cumulative maintainer review

Result: **PASS**. No material maintainer findings proposed.

Subject: base `c62ebce8c4de0d43a2b9d50d360de628c49fd533` through candidate `21dab5d11e484ffceaa4718fc7370c7d6c67dc75`, original `tasks/TASK-003-shared-ui-foundation.md`, approved `spec.md` revision 13. The cumulative range was inspected independently, with `6aace6e07...21dab5d11` used to locate R6 changes. PR #205/base content and separately reviewed TASK-027 layout are excluded from TASK-003 findings; the shared renderer's loading/action seams were inspected.

## Authority and method

Read the root and TypeScript SDK AGENTS, task-review skill, `agent_docs/maintainer-style.md`, spec-driven workflow, original task, applicable REQ-030 package/loaders/executor/result/view sections and invariants, UI skill, architecture authority map, client-model authority, Oracle query ownership and tenancy authority. No `.codegraph/` exists. Used Git diff, current owning modules, caller searches and tests. No peer discovery conclusions were used. Prior finding disposition and R6 were hypotheses to check against source, not acceptance evidence by themselves.

The maintainer threshold is a concrete behavior/contract consequence, unearned machinery, or mandatory changed-Rust documentation gap. An equally clear private structure is not a defect.

## Changed-surface coverage

| Surface and source | Caller / proof coverage | Assessment |
|---|---|---|
| `packages/ui/src/lib/contract.ts`, `schemas.ts`, `loaders.ts` | WyrdView, both POST endpoints, direct example page, `ui-library.test.ts` | The closed request union projects generated domain types. One component schema declaration drives runtime validation and catalog generation. Loaders own planning and projection; credentials remain in executors. State unions distinguish denied/error/empty. R6 moves resolution beside shared span admission while retaining the loaders export. |
| `plot.ts`, `format.ts`, Chart | Shape validation, first-column binding, typed series/caption/folding, signed geometry, tooltips, table and stat mounted assertions | Pure shaping stays beside its chart consumer. The same column-index helper is used by validation and rendering. Shared formatting replaces the previous app-owned formatter instead of leaving a second owner. Caption disambiguation is earned by distinct authored groups and keyed Svelte consumers; it does not alter their values. |
| CardHeader and CardRefChip | Exact get/list, version paging and failure, copied identity, controlled version/tab state and host-action assertions | UI uses flat generated identity and existing Card operations; no second registry or selector. One formatter owns canonical display. The header keeps resolved Card data visible when listing fails, and cross-version intents avoid retaining the old version UID. |
| EmptyState, StatusPill, VerdictPill, Sparkline, DurationBar, Drawer, ActionButton and ActionNote | Public exports, gallery, per-primitive rendering and interaction assertions | Small presentational components, shared state presentation and native modal dialog. No credentials or durable workflow ownership. Host action state disables pending/unavailable controls and preserves visible outcomes. |
| `view.ts`, foundation portions of `WyrdView.svelte` | mount/unmount, structural and per-panel failures, range changes, removed panels, stale results and validated host intents | Renderer owns controller lifetime and result fencing. Existing Svelte mount/unmount and AbortController suffice; no new orchestration service or persistence layer. TASK-027 edit/placement behavior remains separately owned. |
| `node.ts`, `mcp.ts`, app `main.ts`, BFF `ui/load`, BrowserSession client projection | Six operation mappings, MCP error shapes, query-acquisition cancellation, live SDK/MCP/BFF tests and AppBridge artifact | Adapters delegate full existing domain operations rather than reconstructing transport or authorization. Read adaptation remains separate from declared navigation intents. The BFF validates against the same package schema; it forwards selected session authority through the shared Rust client. |
| Shared-client `Bifrost::connect_with`, `read_only`, `from_sink`, QueryOnlySink and `WyrdClient::is_browser_session` | Existing internal query-only path and new browser-session no-ingest test | Browser-session construction reuses the existing query-only shape and writer pool/sink arrangement. `from_sink` removes repeated assembly across normal/internal/session paths. No replacement Bifrost workflow or duplicate credential owner. R6 completes the new test's panic documentation. |
| `wyrd-queue::json_result`, server query collector, SDK native collect/close and TS collect wrapper | Both builder callers, QueryArguments admission, exact positional-byte test, real query ceilings/refusal/release/fresh-query assertions | The former server JSON projection is shared at an existing Arrow/Variant owner, removing duplication rather than building a competing query service. Builder owns retained rows and byte count; each existing stream owner retains protocol validation and settlement. Native collection takes/releases its stream, parses original JS numbers before admission, and close can signal its drain. |
| SDK generated `.d.ts` and `.d.cts`, exports/manifests, theme and catalog generators | Native `Option<f64>` collect arguments vs generated `number` parameters; runtime TS collect wrapper; package browser import assertions and catalog check | Declaration changes project native methods. Browser entry points use type-only SDK references. Theme projection extends the existing brand generator; catalog problems extend existing catalog generation and codegen drift checks. No parallel palette or manually maintained domain shapes. |
| Workspace/build scripts, custom app, gallery, fixtures and test setup | Package scripts, Vite single-file artifact, release SDK pack/addon import, example direct/view consumers and integration tests | Uses current package/Vite/adapter-node/native SDK owners. The example separates server credential/executor construction from browser composition. Test scaffolding addresses jsdom's missing platform behavior; real-server journeys reuse repository support. No additional framework or durable store. |

## Existing-owner and workflow comparison

The new portable renderer/package has no existing full portable equivalent in the base; its existence is expressly required. Reusing app-local formatters only by import would violate portability, so moving them into the package is sufficient and removes the old copy. The existing Cards get/list and Bifrost stream APIs already own all domain reads; adapters reuse them rather than creating component routes or a domain aggregation layer.

The existing bounded MCP collector already provides the necessary value encoding and result accounting. Extracting that projection into `JsonResultBuilder` permits both MCP and Node to share it while leaving the server's `ResultCollector` and client's `QueryResultStream` responsible for their respective framing/cancellation/settlement. Reimplementing server stream lifecycle in the UI is neither necessary nor present. The browser-session path reuses `Bifrost`'s existing query-only construction rather than adding a Node-specific query client. No material duplicate full workflow was found.

## Prior closure checked in current source

| Prior IDs | Current source evidence / retained proof | Assessment |
|---|---|---|
| FIND-003-1, -2 | `loadChart` explicit range precedence; shared `columnIndex` in shaping/plotting; mixed-range and duplicate-name assertions | Remain closed. |
| FIND-003-3, -4, -9, -10 | Chart signed domains and dash/marker pairs; `timeSeries` typed tuple keys, `captions`, separately named aggregate; mounted counterexamples | Remain closed. |
| FIND-003-5, -12 | CardHeader controlled actions, no stale UID on version intents, shared `fmtCardRef` preserving kind/version/optional UID | Remain closed. |
| FIND-003-6 | Node checks cancellation after acquisition and collection, closes active stream; native close notification; loader/renderer abort fencing | Remains closed. |
| FIND-003-7, -8 | Canonical generated problems and complete endpoint refusal envelopes; standard JSON import attribute; direct package import check | Remain closed. |
| FIND-003-11 | Native `Option<f64>` through `parse_ceiling` before shared admission after stream take; generated declarations and public wrapper align; wrapping/fractional/refused-stream assertions retained | Remains closed. |
| FIND-003-13 | `schemas.ts` RelativeRangeSchema rejects unrepresentable spans with native Date validity; `resolveRange` shares the same span calculation. Root document and panel validation precede resolution; both endpoints enter through `loadPanel`. R6 assertions cover initial/updated invalid document, valid sibling, both endpoints and zero invalid-input reads. | Closed for the required current-clock paths; no per-host guard or duration setting introduced. |
| FIND-003-14 | `client.rs::browser_session_bifrost_connects_without_an_ingest_channel` now has `# Panics` naming session/client/connection assertion conditions; test body unchanged. | Closed. |

## Findings and verification limits

Material proposed ledger: **empty**. No optional style preferences are proposed as blockers.

Reported candidate verification includes UI check, 40/40 UI tests, catalog check, format, lints, focused client nextest and real-server integration 2/2. Source inspection confirms the focused assertions exercise observable outputs and refusal/resource behavior rather than only method existence. These are supplied executions, not reviewer reruns. Historical visual and platform evidence remains historical; this pass does not renew browser/human or cross-platform qualification. No builds/tests were executed that could write outside this packet.

Final static Git check: HEAD remains the immutable candidate. Only this assigned report was written; no source/test edits, commits or pushes.
