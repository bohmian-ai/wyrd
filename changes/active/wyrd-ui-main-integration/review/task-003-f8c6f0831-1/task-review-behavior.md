# TASK-003 independent behavior review

Overall result: **FAIL**. One proposed new finding, **BEH-1**, remains. Prior FIND-003-1..15 are closed against the candidate, including both R7 corrections.

## Immutable subject and coverage

Repository `/Users/stevenforrester/Documents/GitHub/wyrd-ui-main-integration`, branch `wyrd-ui-main-integration`; cumulative base `c62ebce8c4de0d43a2b9d50d360de628c49fd533`, candidate `f8c6f083103d8f55007510a4fbcc75e36b4d9c55`. HEAD was checked and matches. Reviewed the cumulative Git diff independently, using the R7 `21dab5d11...f8c6f0831` diff only to locate remediation. Read original TASK-003, revision-13 REQ-020/025/030/033, applicable AC-006/008/010/011 and invariants, prior verdict/validated findings and R7, root and TypeScript SDK AGENTS, spec-driven workflow, maintainer guide, UI guidance and relevant testing boundaries. There is no CodeGraph index. PR #205/main content and TASK-027 layout are excluded; shared loading/action seams remain reviewed.

Inspected actual producers and consumers across package schemas/catalog, typed contracts, every foundation component/primitive, plot shaping, loaders, Node/MCP executors, view validation/mount/remote loading, BFF and example hosts, generated theme/build/package closure, shared JSON collection, server collector, native collection/admission/cancellation, SDK projection, gallery and focused unit/integration proof. Changed planning/review documents are evidence, not stronger authority. No peer discovery report was read. No source or test file was changed.

## Acceptance matrix

UI-relative paths below begin at `crates/wyrd/wyrd-server/wyrd-ui/`.

| Requirement, acceptance criterion, constraint or non-goal | Implementation evidence | Verification evidence | Result |
|---|---|---|---|
| Approved portable package surfaces; no runtime native SDK in browser exports | `packages/ui/package.json`, `index.ts`, `view.ts`, `loaders.ts`, `mcp.ts`; SDK domain imports outside `node.ts` are type-only | Existing import smoke in root package test command; supplied UI check, catalog check; historical package/build proof | PASS |
| One schema authority for panel input/actions and generated drift-checked catalog | `schemas.ts` component declarations; `scripts/catalog.ts`; `validatePanel`, `validateIntent`, `ViewDocumentSchema`; existing codegen task | Unit catalog/unknown component/failed-field cases; supplied catalog `--check` | PASS |
| Root input failure and individual invalid panel isolation | View schema precedes `WyrdView.start`; panel validation precedes `run`; BFF/example validate context and use shared `loadPanel` | Root replacement, valid sibling, malformed endpoints and original render cases in unit/live files | PASS |
| Ordinary relative/absolute bounds, one inherited clock, explicit panel precedence and exact authored SQL/range binds | `schemas.ts::bounds` validates resulting bounds with `AbsoluteRangeSchema`; `resolveRange`; `WyrdView.start`; `loadChart`; `chartParams/highestBind` | R7 fixed-clock year-zero/overflow examples, local/remote root no-read cases, explicit panel isolation, inert quote/comment/identifier cases, real-placeholder missing-range refusal | PASS |
| Complete Chart timeseries/bar/stat/table, truthful schema/error/empty/degraded states, exact Open in Query and safe brush interaction | `shapeChart`, `plot.ts`, `Chart.svelte`; signed domains, positional lookup, group identity/captions, executed binds and action handoff | Existing type/schema/state/signed/caption/20-series/brush tests; actual-source probe below exposes an uncovered successful-null result | **FAIL: BEH-1** |
| Exact Card identity/version reads and version/page failures, copied ref, declared action completion | `loadCardHeader`, shared `fmtCardRef`, `CardHeader.svelte`, `CardRefChip`, `ActionButton/ActionNote`; UID is omitted when selecting another version | Exact list request, five-page truncation/refusal, UID copy and pending/denied/error/unavailable/success tests; real exact Card loading | PASS |
| Closed six-read union; shared SDK/MCP positional projection, stable refusal, terminal-only success and bounded collection | `contract.ts`, `node.ts`, `mcp.ts`; shared `JsonResultBuilder`; server/native collectors retain their own protocol and terminal validation; shared ceiling admission | Supplied live integration 2/2; source exercises SDK/MCP read parity, duplicate names/nulls, bind errors, denial and row/byte limits; shared terminal code preserves degraded evidence | PASS |
| Failed prerequisites stop dependent reads; replacement/removal/unmount fences stale results and propagates supported cancellation | `loadCardHeader` returns before listing and checks signal each page; `WyrdView.abortAll/start/settle`; Node post-acquisition/collection abort handling; native close notification before lock | Existing prerequisite, cancellation, removal and stale-result tests; native acquisition/collection refusal source; supplied current unit green | PASS |
| Typed host actions, validation, duplicate prevention and visible refusal | Catalog action schemas; `WyrdView.act`; Card controls, `ActionButton/ActionNote`; no generic mutation executor | Existing duplicate pending/navigation/error cases; brush conversion has the isolated failure recorded below | PASS except BEH-1 |
| Session and tenant boundary stays Rust-owned; credentials absent from component props/documents/browser output | Request-scoped `BrowserSession.client`; existing Rust `for_browser_session`; BFF selected session; browser-session Bifrost query-only branch; example credential server module | Supplied live integration includes real second-tenant session, foreign Card refusal, malformed input and credential text exclusions | PASS |
| Required gallery, primitives, example direct/view pages, non-Svelte mount and actual self-contained AppBridge artifact | Primitives and `/_gallery`; example server loaders/routes; `mountView`; `src/app/main.ts`; existing package/build owner produces self-contained HTML | Historical individual human checkpoint approved 2026-10-20; current actual-artifact integration supplied green; no fresh visual/platform qualification claimed | PASS within retained evidence |
| One build owner and real Node/native dependency production closure | `scripts/build.sh` extends existing bundle with SDK build/pack, production install, host addon and actual import smoke; manifests/exports/declarations align | Historical darwin-arm64 bundle/import proof; current R7 changes no native/build owner; Linux/image proof remains release-owned | PASS within retained evidence |
| Preserve source provenance and scoped non-goals; no new per-component route, generic writes, A2UI adapter, saved store, Ask service, production MCP registration or publishing | Cumulative diff extends approved owners; shared foundation package is expressly required; unrelated layout/rebase content is separately scoped | Static cumulative diff inspection and retained task evidence | PASS |

The supplied current green checks corroborate their selected paths; they cannot establish the unexercised null-result interaction below. Historical visual and platform limitations are retained rather than relabeled fresh proof.

## Existing owner/reuse assessment

| Changed machinery | Existing alternative and missing behavior | Assessment |
|---|---|---|
| Shared portable loaders/view renderer/catalog | SDK Cards/Bifrost and server query owners already perform authorized reads; they do not shape UI props or render view documents | Required portable package adds planning/rendering only and delegates IO. No replacement registry/query authority found. |
| SQL range-bind recognition | Oracle `parse_select_tables`/DataFusion own actual SQL admission; no browser-portable repository tokenizer exists in the package; former raw regex could not distinguish inert SQL | R7 narrowly recognizes range tokens without rewriting SQL or deciding SQL safety. Installed sqlparser/DataFusion sources corroborate relevant quote/comment/identifier forms; no second query validator added. |
| Result collection/admission | MCP previously owned the positional Arrow JSON projection and ceilings | Extraction to `wyrd_queue::json_result` is reused by SDK/native and server, while lifecycle/protocol/terminal settlement remain with their owners. |
| Browser SDK transport/query-only construction | Existing Rust session transport and existing query-only Bifrost shape | Node forwards selected session through the SDK; shared `from_sink/read_only` reuses the existing shape and avoids opening bearer-only ingest. |
| Chart empty geometry and brushing | `timeSeries` already skips null observations, and `Chart` already owns the geometry-to-action boundary | Nulls are legitimate query values; adding transport/schema rejection would weaken existing behavior. The local brush owner must not manufacture a range when there is no drawable x domain. |

## Proposed finding BEH-1 — A successful all-null time series throws on brush release

**Classification:** INCORRECT. **Obligation:** REQ-030 complete typed Chart rendering/interactions with preserved supported null values and safe validated range actions; original TASK-003 scenario 1 complete states/interactions and scenario 3 safe actions. This is new foundation behavior in the cumulative diff, not PR #205 debt or TASK-027 layout.

**Exact locations:** `packages/ui/src/lib/components/Chart.svelte:45–51,99–105,219–241`; producer `packages/ui/src/lib/plot.ts:35–47`; admission `packages/ui/src/lib/loaders.ts:171–199`. The action consumer is `WyrdView.svelte::act`; public direct Chart callers, view renderer and AppBridge all reuse this component.

**Reachable caller-to-result path:** An authorized successful query can return a valid Timestamp x column and nullable Float64 y column with rows whose y values are all null. For example, a typed null projection of `vala.traces.spans` bounded by real `$1/$2` has this shape:

```sql
SELECT start_time_unix_nano AS at, CAST(NULL AS DOUBLE) AS value
FROM vala.traces.spans
WHERE start_time_unix_nano >= $1 AND start_time_unix_nano < $2
```

This respects Oracle's actual Bifrost-table floor and uses supported timestamp/numeric/null data. Both collectors preserve nulls and both executors pass the same successful shape to `loadChart`. `shapeChart` checks column types and only marks zero-row results empty, so a row `["2026-10-22T00:00:00Z", null]` is populated. `timeSeries` correctly skips the missing numeric value; all points disappear. Chart derives `xs=[]` and `bounds.x0/bounds.x1=undefined` but still renders its pointer-operable time-series SVG. `WyrdView` always supplies its `act` callback, so this is reachable even without a navigation host handler. Pointer down and a movement of at least six SVG units set a valid brush selection. `release()` converts undefined bounds into NaN and calls `new Date(NaN).toISOString()`, throwing `RangeError: Invalid time value` before an intent reaches host validation. The displayed result has no data line and the user's range interaction fails through an uncaught event error.

**Source proof:** An artifact-free Node probe used the candidate's actual TypeScript-transpiled `shapeChart`, `timeSeries`, and complete extracted `Chart.release` body, with the one-row typed result above, a normal 560px plot box and brush `{from:100,to:200}`. It returned:

```json
{"state":"populated","lines":[],"xs":[],"releaseError":"RangeError: Invalid time value","intent":null}
```

This is source-body execution, not a mounted-browser claim. Existing unit tests cover null table cells and null series identities, but their numeric timeseries values remain populated; existing brush assertions use non-null numeric series. No current assertion exercises this path.

**Smallest testable correction:** Keep null projection and normal loader admission intact. Correct the existing Chart geometry-to-range action boundary so a plot without a valid increasing finite x domain cannot attempt a range conversion or emit an invalid intent. A local guard belongs here because Chart owns pixel-to-time conversion and must also work when a custom app supplies typed props directly. Reuse its current bounds/action mechanism; no new parser, loader state model, transport, runtime setting or framework is needed. Preserve valid brushing, SQL/executed values, ordinary null gaps, signed domains, group/caption behavior and host validation. Do not rewrite a successful query into a denial or reject nullable numeric schemas merely to avoid the geometry case.

**Focused closure proof:** Extend the existing Chart rendering/brush cases with a populated successful Timestamp+nullable numeric result having only null y values. A meaningful drag must not throw or emit a malformed range. Rerender with valid drawable points and prove ordinary brushing emits finite increasing RFC 3339 bounds, retaining the current shared-view reload test. Use the existing `TASK-003: Validated portable rendering` and `TASK-003: Dependent loads and safe actions` selectors; no live-server or broad journey rerun is needed for this local correction.

## Prior finding closure

| Finding | Candidate source-backed closure |
|---|---|
| FIND-003-1 | `loadChart` uses its own range ahead of inherited context; one root range resolves in `WyrdView.start`; exact executed binds feed Open in Query. |
| FIND-003-2 | Shared first-position `columnIndex` drives shape checking, line/bar/stat lookups; table remains positional. |
| FIND-003-3 | Signed y domains include zero; negative bar geometry spans to zero with positive SVG height. BEH-1 concerns absent geometry, not signed-value regression. |
| FIND-003-4 | `MAX_SERIES=20`; only counts above twenty fold to nineteen plus aggregate; marker/dash pair stays unique through twenty. |
| FIND-003-5 | Version/tab controls consume pending/unavailable state and render `ActionNote`; selection returns to host-provided version; `WyrdView.act` deduplicates. |
| FIND-003-6 | Node closes after an aborted acquisition, registers abort during collection and rechecks after collection; native close notifies before awaiting stream lock; view fences by controller identity/abort and listing checks before dependent reads. |
| FIND-003-7 | Generated canonical problems feed `wyrdProblem`; `failed` retains denied classification; both POST refusal helpers serialize complete canonical problems. |
| FIND-003-8 | JSON import has standard `with { type: 'json' }`; public loader/MCP package smoke remains in package test command; browser SDK imports are type-only. |
| FIND-003-9 | Aggregate caption is chosen against retained authored captions; legend/plot/tooltip share one labeled result. |
| FIND-003-10 | Typed `[group,y]` keys separate summation identities; iterative JSON caption qualification distinguishes null/text and multi-y collisions. |
| FIND-003-11 | Original JavaScript numeric ceilings enter native `Option<f64>`, exact decimal integer parsing and shared `admit_ceilings` after taking the stream; refusal drops it; server uses same policy and JSON byte accounting. |
| FIND-003-12 | Shared `fmtCardRef` includes kind/name/version and supplied UID; Header and Chip reuse it; a version action omits old version UID. |
| FIND-003-13 | R7 `bounds` checks the produced range through existing `AbsoluteRangeSchema`; RelativeRange admission rejects Date-valid expanded-year spans as well as Date-invalid spans before local/remote reads. Existing initial/replacement/root/explicit-panel tests exercise the original failure families. |
| FIND-003-14 | New client regression has the required `# Panics` section naming its invariant expectations. |
| FIND-003-15 | R7 `highestBind` skips single/double/backtick quotes, escaped E strings, dollar quotes, identifiers and nested/line comments before counting actual `$1..$3`; `chartParams` leaves authored SQL intact and preserves real-placeholder/no-range refusal. Relevant new loader assertions exercise dispatch and executed values. |

## Verification and limits

Supplied green at candidate: UI check, 40/40 unit tests, catalog `--check`, `mise run fmt:check`, live UI real-server integration 2/2. R7 changes no Rust. Historical bundle/native/integration and human visual evidence retain their recorded scope. This review ran static source/Git reads and artifact-free in-memory Node probes only; it did not rerun source-writing build/check/test lanes, mount a browser, start a server, or qualify images/platforms. The probe demonstrates the failed source conversion; a mounted regression remains the focused closure proof.

Skipped: fresh build/test/visual/platform reruns under the packet-only restriction. Risk: a valid successful all-null timeseries can still throw from a user brush before action validation.
