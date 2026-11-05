# Independent structured Ponytail validation

Recommendation: **FIX_REQUIRED**. The validated active ledger contains **FIND-003-16** and **FIND-003-17**. FIND-003-1..15 are closed at this candidate. Both corrections are bounded implementation work under approved specification revision 13; neither requires a new product or architecture decision.

## Subject and independence

Repository `/Users/stevenforrester/Documents/GitHub/wyrd-ui-main-integration`, branch `wyrd-ui-main-integration`; cumulative base `c62ebce8c4de0d43a2b9d50d360de628c49fd533`, candidate and checked HEAD `f8c6f083103d8f55007510a4fbcc75e36b4d9c55`. Original task: `changes/active/wyrd-ui-main-integration/tasks/TASK-003-shared-ui-foundation.md`. Authority: approved `changes/active/wyrd-ui-main-integration/spec.md`, revision 13, root/TypeScript SDK contributor rules, spec-driven development, maintainer guidance and the existing client/query/session ownership boundaries. No CodeGraph index exists.

This fresh validation read the cumulative diff and source owners, the original task and applicable REQ-025/030 contracts, all six discovery reports, `claims-comparison.md` and `followup-review.md`. Discovery agreement was not used as source proof. The latest R7 diff located its corrections; it did not replace the cumulative subject. Prior findings were checked against their current producers and consumers. PR #205 identity changes and TASK-027 layout are excluded as instructed; foundation loading and action seams remain in scope.

The full bodies of the proposed correction owners were read: `Chart.svelte` including geometry, brush creation and release; `nodeExecutor`, `read`, `run` and both getters. Tracing also covered `shapeChart`, `loadChart/loadPanel`, plotting/formatting, schema/action validation, renderer scheduling/action dispatch, remote loading and both HTTP hosts, SDK/native connect wrappers, Rust Cards connection, credential exchange and Bifrost assembly. UI paths below start at `crates/wyrd/wyrd-server/wyrd-ui/`.

## Claim dispositions

| Proposed claim | Disposition | Independent source result |
|---|---|---|
| BEH-1; FUP-BEH-1: brushing a successful null-only timeseries throws | **CONFIRMED**, FIND-003-16 | Typed null values are legal and retained. Plotting removes their points, but the component still creates a brush and serializes undefined-domain arithmetic before any host validation. |
| SYS-1; FUP-SYS-1: failed lazy initialization poisons a retained Node executor | **REVISED**, FIND-003-17 | The state defect is confirmed. Classify as INCORRECT new adapter behavior, and restrict consequence to a retained public executor. Shipped request-local BFF/example instances recover through recreation on the next request; they are not permanently disabled. |
| Invariant/standards/maintainer/domain reports propose no additional finding | No additional finding retained | Their PASS results do not refute the independently traced failures. No style-only or hypothetical hardening request is added. |

## FIND-003-16 — A successful null-only chart can throw when brushed

- Discovery sources: **BEH-1**, **FUP-BEH-1**.
- Validation: **CONFIRMED**.
- Classification: **INCORRECT**.
- Obligation: REQ-030 retains query nulls, requires typed JSON-safe intents and shared view brushing; original TASK-003 owns complete Chart interactions and portable rendering. REQ-025 permits timestamp/numeric result shapes without requiring every numeric observation to be non-null.
- Exact changed location: `packages/ui/src/lib/components/Chart.svelte:99–105` (`release`), with domain production at `:42–51` and brush admission at `:228–235`.

### Reachable producer-to-consumer evidence

The query result owner `crates/shared/wyrd-queue/src/json_result.rs::accept_batch:149–181` preserves a null Arrow slot as JSON null, including when the entire numeric column is null. Such a column remains Float64, rather than becoming an incompatible schema. A bounded authored query over the existing trace table can select `start_time_unix_nano AS t, CAST(NULL AS DOUBLE) AS v`, apply `$1/$2` to that timestamp and `ORDER BY 1`. This is within the required read-only SQL shape and table admission boundary. It needs ordinary nonempty trace data, not a malformed executor response. The existing example already uses this timestamp/table and bound range.

`loaders.ts::shapeChart:171–199` checks timestamp/numeric column types and returns populated for nonempty rows. `loadChart` preserves it, as do `loadPanel`, local/MCP view loading and the BFF/custom endpoint. In `plot.ts::timeSeries:35–47`, each null y is correctly skipped; with all y values null the resulting series array is empty. `Chart.svelte` therefore derives `xs=[]` and undefined `x0/x1`. Its populated-timeseries branch still renders the interactive SVG. Pointer down creates a brush whenever `onaction` exists. A drag exceeding six SVG units passes `release` admission, computes NaN endpoints and throws `RangeError` in `toISOString` before invoking `onaction`.

This reaches required hosts: `WyrdView.svelte:330–332` always supplies a Chart callback, and `act:269–279` handles `chart.range` internally. `mountView`, the AppBridge entry and the example view use that renderer. The direct component API also permits a host callback. Direct examples with no callback cannot create a brush; that limits those consumers, not the shared path. Host `validateIntent` cannot catch a conversion error that happens before delivery. Loading/error/empty states do not render this interactive branch, and bar/stat/table siblings do not use this date conversion.

Observable consequence: a valid successful query causes an uncaught pointer-event conversion error and failed range interaction. No process crash, tenant exposure or data loss is asserted.

### Minimum correction and closure proof

Keep the correction in the existing Chart geometry-to-action owner: do not create or complete a range conversion without a usable finite increasing drawable x domain, and emit only an increasing valid absolute range. A local admission guard belongs here because this component produces the arithmetic and also accepts direct typed props independently of loaders. Reuse native numeric checks and the existing absolute-range contract if needed; do not add a range/parser framework. Keep valid nullable query data and null-skipping plotting. Do not reject nullable schemas, manufacture zero observations, turn query success into denial, or add host-specific guards or TASK-027 layout work. Empty or degenerate geometry cannot produce a meaningful interval; withholding that gesture preserves the required safe-intent contract.

Use the existing mounted UI test harness. Feed a successful timestamp/nullable-numeric result whose y values are all null through shared loading into a rendered Chart or WyrdView; perform a meaningful drag and assert no throw and no invalid range action/reload. Retain a normal nondegenerate result check proving a valid brush still updates/reloads the view. A single-instant result can cover the same absent-increasing-domain invariant without a new finding or harness. Existing normal-brush tests at `src/lib/ui-library.test.ts:507–520` and typed-series identity tests do not exercise null y removing all geometry. No new live-server journey is necessary for this local conversion gap.

The ladder rejects deleting brushing (required), changing the query owner (it correctly preserves nulls), replacing the plotter, or adding a dependency. The smallest safe boundary is the current component's conversion.

## FIND-003-17 — Failed handle initialization is retained across later reads

- Discovery sources: **SYS-1**, **FUP-SYS-1**.
- Validation: **REVISED** only to precise classification and blast radius; source failure is confirmed.
- Classification: **INCORRECT**.
- Obligation: REQ-030 exports a Node executor over the existing SDK for custom user servers, with repeatable read execution and existing SDK transport/credential ownership. The public `Executor.run` contract is not one-shot. One failed read must not install adapter state that prevents a later caller-initiated read from reaching a recovered existing SDK owner.
- Exact changed location: `packages/ui/src/lib/node.ts:16–19`, both promise caches/getters; affected consumers `:23–30` and `:45–48`.

### Reachable producer-to-consumer evidence

Both getters assign a connect promise with `??=`. A rejected promise remains non-nullish. Every later relevant read therefore awaits the same rejection; there is no path that invokes connect again.

This is a real fallible boundary. TypeScript `Cards.connect` (`sdks/wyrd-sdk-ts/wyrd/src/index.ts:1670`) awaits native `cards()` and projects the returned error. Native `src/cards.rs:68–84` invokes Rust `Cards::connect_with`; the full connection body in `crates/shared/wyrd-client/src/cards/handle.rs:193–197` awaits `client.connected()`. For a custom-server API key, `client.rs::connected` requests the existing access-token owner. `auth.rs::bearer:851–883` holds the credential cache lock and exchanges when needed; `exchange_and_store:956–965` performs the fallible exchange before storing. A transport failure leaves no terminal failed cache entry, so the same underlying client can connect on a later call once the auth service recovers.

TypeScript `Bifrost.connect` (`index.ts:1098–1106`) similarly projects native `src/client.rs:200–220`, which calls Rust `Bifrost::connect_with`. Its full `facade.rs:148–160` body connects credentials then uses `assemble` for ordinary custom-server clients; `assemble:248–266` dials the existing gRPC transport. Initial auth/network/ingest-channel failure can therefore reject. A subsequent fresh SDK connection is permitted; the UI promise cache prevents that attempt. The browser-session query-only branch remains a separate already-correct path.

All cache siblings are accounted for: Cards get/list share one poisoned promise; Bifrost query/list-tables/describe-table share the other. Verification status connects independently and is not disabled. `run` maps an SDK error into the original structured failed result; later calls replay that failure before issuing the requested operation. This is not a returned partial success or auth bypass.

The required public path is a user's server retaining `nodeExecutor(client)` for successive `loadChart`, `loadCardHeader`, `loadPanel` or direct `run` calls. Its reusable read contract permits this ordinary lifetime; the real integration file itself retains `sdk = nodeExecutor(client)` at `:139` for multiple operations. Repository production callers at the shipped BFF endpoint and the example factory create request-local executors, so later requests abandon failed instances. No permanent BFF outage across requests is alleged.

Observable consequence: one transient first-connect failure disables all Card reads or all Bifrost reads for the lifetime of that retained executor, even after the same client/service can connect again. Recovery requires recreating the executor or its retaining process unnecessarily. No cross-tenant leak or persistent-state damage is asserted.

### Minimum correction and closure proof

Correct failed-state retention at the two existing initialization-cache producers: clear failed initialization so a later explicit `run` can call the existing SDK connect again, retaining successful/in-flight sharing. This is local cache admission, not an automatic retry policy. Keep the initial error visible and let the next caller decide when to read. No new connection manager, SDK credential change, backoff/configuration, consumer retry guard, BFF lifetime change or transport replacement belongs in this correction. Clearing on failure is smaller than deleting all handle reuse and redialing the ingest transport for every successful read; no broader cache abstraction is earned.

Extend the current Node-executor mock test owner. For each of Cards and Bifrost, make first initialization reject with a structured SDK error and the next initialization succeed on the same executor/client. Assert first `run` preserves that problem, later `run` reaches connect again and returns the actual read, and sibling operations reuse the recovered handle. Preserve the existing pre-start, acquisition and collection cancellation assertions at `ui-library.test.ts:641–679`. Both caches must satisfy the same invariant; a one-operation fix is incomplete. No live outage harness or full journey suite is needed to prove this promise lifetime transition.

## Independent executable source check

Ran one artifact-free `mise exec -- node --input-type=module` probe. It read candidate source, used Node's `stripTypeScriptTypes` in memory, and evaluated the actual `format.ts`/`plot.ts`, actual `Chart.release` body and actual full `node.ts` adapter with supplied SDK doubles. No source copy, generated output or test file was written.

- Actual `timeSeries` returned `[]` for two typed timestamp/null-y rows. The actual `release` body threw `RangeError` for a meaningful brush and undefined x bounds, without reaching the action callback.
- For each getter owner, first connect rejected and a second connect would succeed. Two calls through the actual executor both returned failure and connect was called only once. This proves failed-state retention in both caches independently of discovery agreement.

These probes establish the local defects; the production caller and SDK traces above establish required reachability. They do not claim a fresh browser or outage journey.

## Prior-finding closure

| Prior ID | Current source evidence | Disposition |
|---|---|---|
| FIND-003-1 | `loadChart` chooses explicit input range before inherited context; WyrdView resolves the root once; `executed` keeps exact resulting params | CLOSED |
| FIND-003-2 | `columnIndex` selects first position; validation, line/bar and explicit stat binding share it; tables retain positional cells | CLOSED |
| FIND-003-3 | Chart signed domains include zero and increase; bars span value to zero with nonnegative height; constant/zero domains are defined | CLOSED |
| FIND-003-4 | `MAX_SERIES=20`; folding applies only above twenty; marker/dash products provide twenty distinct combinations | CLOSED |
| FIND-003-5 | CardHeader consumes named version/tab state, blocks pending/unavailable, resets selection to host state and renders ActionNote; WyrdView validates/deduplicates dispatch | CLOSED |
| FIND-003-6 | Node closes post-acquisition aborted streams and checks during/after collection; native close notifies before lock wait; renderer fences stale controllers and dependent pages check abort | CLOSED; FIND-003-17 concerns initialization failure, not cancellation |
| FIND-003-7 | `wyrdProblem` uses Rust-generated examples; `failed` preserves denial; both load endpoints serialize canonical refusal envelopes | CLOSED |
| FIND-003-8 | `contract.ts` uses the standard JSON import attribute; browser SDK domain references remain type-only | CLOSED |
| FIND-003-9 | Aggregate caption is selected against final retained authored captions; plot/legend/tooltip share the same final series output | CLOSED |
| FIND-003-10 | Typed JSON `(group,y)` identity precedes summation; caption collision qualification distinguishes null/text-null and multi-y groups | CLOSED |
| FIND-003-11 | Native accepts original f64 and losslessly parses before shared ceiling admission after taking stream ownership; errors release it; shared builder retains exact positional/null and byte accounting while callers retain terminal/settlement ownership | CLOSED |
| FIND-003-12 | Shared `fmtCardRef` includes kind/name/version and supplied optional UID; header/chip reuse it; version intent excludes old UID | CLOSED |
| FIND-003-13 | R7 `bounds` accepts computed output only through existing `AbsoluteRangeSchema`; admission and resolution share it; frozen-clock tests cover year-zero, expanded/native-invalid spans, local/remote root no-read, isolated explicit panels and both endpoints | CLOSED; FIND-003-16 is chart geometry, not relative-range admission |
| FIND-003-14 | `client.rs:760–765` documents named Panics conditions of the browser-session Bifrost regression and keeps its assertions | CLOSED |
| FIND-003-15 | `highestBind` consumes literal/identifier/dollar-string/comment tokens before true binds; `chartParams` preserves SQL and missing-range refusal; R7 tests cover inert/mixed tokens through public loading | CLOSED |

The new findings have separate producers and correction boundaries. Neither is another caller instance of the R7 range/SQL defects. No shared root justifies merging Chart geometry with the Node initialization cache, and no rejected proposal is retained as optional advice.

## Final ledger and proof limits

| Stable ID | Status | One-line consequence | Required correction boundary |
|---|---|---|---|
| FIND-003-16 | CONFIRMED | Brushing a legal null-only timeseries throws before host validation | Existing Chart geometry-to-range production; withhold invalid conversions and retain valid brushing |
| FIND-003-17 | REVISED | One transient first-connect failure permanently poisons a retained public Node executor | Both existing initialization caches; do not retain rejection across later caller-initiated reads |

Supplied green evidence is UI check, 40/40 unit tests, catalog check, fmt check and 2/2 real-server integration tests. R7 changes no Rust. Those healthy-input checks do not cover the two reproduced negative transitions. Historical platform/build/human evidence remains historical. No additional blocker or authority conflict was established.

Skipped: writing/running build or test suites, browser/outage execution and fresh platform qualification under packet-only restrictions. Risk: the two retained failures remain in candidate source until remediation; this report changes only the packet.
