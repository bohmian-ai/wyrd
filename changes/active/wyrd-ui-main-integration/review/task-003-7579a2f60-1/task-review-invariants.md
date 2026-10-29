# TASK-003 independent invariant review

Overall result: **FAIL**. Two material proposals require independent validation. FIND-003-1 through FIND-003-10 are closed for their diagnosed causes; the new proposals concern canonical copied Card identity and Node/MCP bounded-read admission parity.

## Immutable subject and method

Repository `/Users/stevenforrester/Documents/GitHub/wyrd-ui-main-integration`, branch `wyrd-ui-main-integration`; base `c62ebce8c4de0d43a2b9d50d360de628c49fd533`, candidate `7579a2f603c023daae0d15b9d5cc3013eec6f00d`. HEAD was checked and matches. Read the cumulative diff, original TASK-003, approved revision 13 (particularly REQ-020/025/030/033), applicable root/SDK instructions, task-review/UI guidance, spec-driven development, maintainer guidance, relevant Card identity and browser-session authority, and prior validation/remediation packets. No CodeGraph directory exists. No peer discovery report informed this review.

Independently ran `git range-diff 695ca914d^..wyrd-ui-pre-205 ff4145ac6^..7579a2f60`: implementation/remediation patches correspond, with the recorded build-function context and packet authority-link changes. New flat-identity and R3 commits are reviewed as current source. PR #205 content and TASK-027 layout implementation are excluded from findings; renderer/load seams were inspected for foundation regression.

Navigation expanded from the shared map through canonical `CardRef` Display/FromStr, shared positional JSON projection and its two collectors, native stream cancellation, both executor adapters, component controls, WyrdView supersession, BFF and custom endpoint, public package/catalog/artifact, example and gallery inputs, and integration assertions. The principal invariant paths are:

- Authorized Arrow schema/batches/terminal → existing query-stream validation → shared `JsonResultBuilder` → Node/MCP adapters → portable loader → Chart/other renderers.
- Exact CardRef → existing Cards owner → resolved Card envelope → version-list pagination → CardHeader actions and copied presentation; CardRefChip shares that presentation producer.
- Document/input/range → catalog schema → isolated loader/controller → result settlement; action intents → catalog validation → existing host action → named action outcome.

Supplied candidate verification is credible for the exercised cases: package/app check, test 40/40, build, real Postgres-managed integration 2/2. These are supplied results, not this reviewer's reruns. A read-only `mise exec -- node --input-type=module` probe independently imported existing package outputs, asserted distinct Model/Agent references both format to `ranker@1.4.2`, and asserted R3's four source tuples preserve four labels and values `[1,10,2,20]`. No build, source/test edit, commit or push was performed.

## Acceptance matrix

Paths abbreviated `ui/` are under `crates/wyrd/wyrd-server/wyrd-ui/`.

| Requirement, acceptance criterion, constraint or non-goal | Implementation evidence | Verification evidence | Result |
|---|---|---|---|
| Approved package components/loaders/view/catalog/node/mcp entries build; browser imports avoid native SDK | `ui/packages/ui/package.json`, index/view exports; domain SDK imports in browser paths are type-only | Supplied package/app check and build; native import smoke retained in root test command | PASS |
| Catalog generated from single input/action declarations and checked for drift | `schemas.ts`, `scripts/catalog.ts`, `mise.toml` codegen extension | Current catalog matches flat schema structurally; historical codegen proof, supplied build | PASS |
| Exact flat Card schema rejects legacy space; no alias or alternate lookup | `CardRefSchema.strict`, `loadCardHeader`, node/MCP exact reads | Strict-input and live SDK/MCP read assertions; supplied 2/2 integration | PASS |
| Canonical copied/reference presentation retains complete kind/name/version identity | `format.ts:51`, CardHeader `Copy ref`, CardRefChip title | Read-only cross-kind probe; tests currently expect incomplete spelling | FAIL — INV-003-FLAT-REF |
| CardHeader preserves exact selected version, dependent listing, bounded pages and separate version failure | `loadCardHeader`, `CardHeader.svelte` authoritative selection, `versions` state | Paged-list/denied-prerequisite/action tests; supplied suite | PASS |
| Chart timeseries/bar/stat/table shape validation is visible and positional | `shapeChart`, `columnIndex`, Chart render branches | Duplicate-name, schema mismatch, table/stat/rendering assertions | PASS |
| Authored SQL unchanged; executed SQL/binds retained for Open in Query | `chartParams`, `loadChart`, Chart action | Existing request/action assertions and live bound SQL proof | PASS |
| Explicit panel range wins; inherited range resolved for the view; brush/reset replaces loads | `loadChart`, WyrdView content/range effects | Explicit/relative and mixed-panel brush/reset assertions | PASS |
| Signed values draw around zero; up to twenty series distinguishable | Chart signed domains, marker/dash pairs; `MAX_SERIES` | Mixed/negative/zero and 5/20/21 assertions retained | PASS |
| Distinct authored `(series,y)` identities survive aggregation; captions stay unique and attributable | `plot.ts::timeSeries`, private `captions`; typed JSON tuple key | R3 mounted four-way/null/qualification/rerender assertions; independent pure grouping probe | PASS |
| Above twenty, nineteen kept groups plus correct per-instant collision-safe other | `timeSeries` total ranking/folding and post-caption aggregate naming | Existing synthetic/authored other tests and exact totals retained | PASS |
| All six approved read operations reuse existing SDK/MCP capabilities | Node/MCP adapters; execution status and Cards/Bifrost handles | Six-operation live assertions include not-found execution result | PASS |
| Typed MCP binds project through existing query owner | Existing `QueryArguments` and request projection, upstream already retained in rebased base | Historical Rust schema proof and live bound/malformed-bind assertions | PASS |
| Node and MCP retain bounded collected-query ceilings and matching refusal behavior | Node passes numeric options directly to native builder; MCP additionally validates bounded-admission range | Live test covers exceeded small output budgets, omits out-of-range requested budgets | FAIL — INV-003-BOUNDS |
| Positional schema/rows/null/value encodings retained; terminal required; failed output never success | Shared `JsonResultBuilder`; server ResultCollector and native collect consume validated streams | Source convergence; positional byte regression and live duplicate/null/limit assertions | PASS |
| Degraded terminal retained with warnings/source evidence | Shared finish projection; Chart partial notice | Source proof and historical shared-builder evidence; no fresh live degraded result | PASS for implementation; live proof remains limited |
| Denied/failed prerequisite emits no dependent reads | `loadCardHeader` early refusal; signal check before each page | Explicit refusal/cancellation assertions | PASS |
| Superseded/removed/unmounted panels reject stale results and supported transport cancels | WyrdView controller/generation fences; request signal forwarding; Node stream.return; MCP signal | Existing stale-load and acquisition/active/pre-abort assertions | PASS |
| Typed actions validated, pending suppresses duplicate, unavailable/denied/error observable | `validateIntent`, WyrdView act, ActionButton/ActionNote, CardHeader controls | Direct and renderer action/refusal tests | PASS |
| Malformed/unknown panels fail individually; canonical local failures and HTTP refusal shape | `validatePanel`, `inputProblem`, `wyrdProblem`, both endpoints | Native import smoke and canonical problem assertions; artifact invalid sibling | PASS |
| Rust owns session and durable authorization; BFF forwards selected tenant session only | Existing BrowserSession.client and shared WyrdClient; query-only Bifrost branch | Supplied real-session/cross-tenant proof; source preserves permission owners | PASS |
| Complete primitives use props, semantic/accessibility behavior and brand tokens | EmptyState, CardRefChip, VerdictPill, StatusPill, Sparkline, DurationBar, native dialog Drawer | Existing component/interaction assertions; recorded human visual approval | PASS except identity issue above |
| Built self-contained AppBridge artifact and custom direct/document app | `src/app/main.ts`, app Vite config, example server-only executor/load endpoint | Supplied build and live artifact/example integration; structuredClone harness fix diagnosed | PASS |
| Production SDK runtime/native addon packaged through established build owner | `scripts/build.sh::bundle`, profile-aware ts:build; packed SDK/runtime dependency closure | Historical darwin-arm64 bundle smoke; supplied current build | PASS for implementation; Linux/image execution unverified |
| No per-component route, generic write dispatcher, saved-view store, Ask service, production MCP registration or publishing | Read-only union and single panel load endpoint; package artifact only | Cumulative attributable diff | PASS |
| TASK-027 and upstream PR #205 changes not misattributed | Commit/range-diff attribution and source seam inspection | Independent range-diff; immutable HEAD | PASS |

## Existing-owner and complexity assessment

The portable UI loader/renderer is expressly approved and was absent before the task. It adds presentation planning over Cards/Bifrost/Execution; it does not replace those complete workflows. A Card definition and its version listing are genuinely dependent and use existing exact get/list capabilities; no new registry/UID resolver is justified or present. The generic panel endpoint is expressly approved for browser-session execution and catalog validation, with the Rust authorization owner retained.

`JsonResultBuilder` moves the existing server's positional projection rather than inventing a second encoding; both terminal-aware callers reuse it. Its omission is shared admission of caller ceilings, described below, rather than a need for another collector. Browser-session Bifrost assembly reuses the existing query-only sink and one construction shape; this avoids an unsupported bearer-only gRPC dial without adding authentication authority. Browser component actions stay intents and do not own mutations. R3's Map plus standard JSON tuple serialization is the smallest suitable grouping correction; its bounded-at-twenty caption disambiguation adds no public identity layer.

## Source-backed prior-finding closure

| Stable finding | Current-source closure and sibling-consumer check | Disposition |
|---|---|---|
| FIND-003-1 — range precedence | `loadChart` uses explicit panel range first; executed binds and actual request use the same params. All hosts converge here. | CLOSED |
| FIND-003-2 — duplicate-column consistency | Shape checks and timeseries/bar/stat use shared first-match `columnIndex`; table remains positional. | CLOSED |
| FIND-003-3 — signed geometry | Chart domains include zero, signed bar endpoints yield nonnegative height and negative label clearance. | CLOSED |
| FIND-003-4 — twenty-series threshold/distinguishability | Producer folds only above twenty; marker/dash combination supplies twenty distinct styles consistently to legend and plot. | CLOSED |
| FIND-003-5 — discarded CardHeader action outcomes | Select/tab controls consume named states, prevent pending/unavailable input, restore authoritative selection, and display ActionNote. WyrdView supplies host outcomes. | CLOSED |
| FIND-003-6 — Node stream-start cancellation | Acquired already-aborted stream is closed before collect; active abort listener closes native; post-collect abort refuses late success. SDK return/native close is existing cancellation owner. | CLOSED |
| FIND-003-7 — uncatalogued local/refusal errors | Generated Rust examples feed `wyrdProblem`; loader/view/renderer/MCP local errors and both endpoints reuse canonical object. | CLOSED |
| FIND-003-8 — native Node JSON linking | Standard JSON import attribute retained; root test native-import smoke retained before transformed tests. | CLOSED |
| FIND-003-9 — synthetic other collision | Aggregate caption chosen against all emitted authored captions, retaining recognizable other qualification. | CLOSED |
| FIND-003-10 — authored tuple identity loss | Map key uses typed serialized `[raw group,y]` before summation, distinct captions emitted by same producer; null/text-null and collision-chain tests retained. `barRows` does not aggregate by caption, and all plot/legend/tooltip consumers share corrected producer. | CLOSED |

The flat-identity remediation correctly removes/refuses `space` in wire requests/schema/catalog and live routes. It did not implement the governing canonical text requirement; that omission is independently diagnosed below. The human-directed adaptation is not itself assigned a finding ID.

## Proposed material findings

### INV-003-FLAT-REF — Copy ref drops the Card kind

Classification: **INCORRECT**. Violated obligation: approved revision 13 explicitly says UI Card identity renders `Kind/name@version`; `architecture/wyrd-design.md:2160–2164` defines canonical text `Kind/name@version[#uid]` and tenant/kind/name/version identity. Original TASK-003 requires copied identity and exact references.

Location: `ui/packages/ui/src/lib/format.ts:50–52`; affected consumers `components/CardHeader.svelte:84,103` and `components/CardRefChip.svelte:9,11`.

Producer-to-consumer evidence: `CardRefSchema` accepts exact kind/name/version. Cards.get returns a typed envelope; CardHeader retains kind in its `ref`, and CardRefChip receives the full reference. Both pass it to `fmtCardRef`, whose narrowed `{name,version}` argument and interpolation discard kind. CardHeader's clipboard writes the resulting string unchanged. The separate kind badge does not reach the clipboard or chip title. No later writer restores it. All direct, gallery, example and WyrdView/AppBridge consumers share these components.

The canonical existing owner is `crates/wyrd-spec/src/reference.rs:558–605`: Display emits `Kind/name@version` plus optional UID and FromStr requires the kind/name separator and known kind. The copied `ranker@1.4.2` is refused by that parser and is identical for Model/ranker@1.4.2 and Agent/ranker@1.4.2. The independent read-only formatter probe confirmed both collapse to the same actual emitted string. This is a real Copy ref workflow defect, not merely caption style. Current tests expecting `Model ranker@1.4.2` establish the weaker rendering and fail to protect canonical copying.

Smallest testable correction: correct the existing shared formatter to accept/retain kind and emit the already-defined flat canonical kind/name/version text, and let existing header/chip consumers reuse it. Retain optional UID where canonical reference text promises it; copying the separately labeled UID remains unchanged. Preserve version-action ref construction (changing version must not retain a prior version's UID), no space compatibility or new identity resolver. Add focused existing component assertions for clipboard text and chip/title with two kinds sharing name/version; verify expected flat spelling and no stale UID in version navigation. Do not prescribe a new formatter layer or public API.

### INV-003-BOUNDS — Node bounded reads accept requested ceilings MCP must refuse

Classification: **INCORRECT**. Violated obligation: REQ-030 shared query results retain current row/JSON-byte ceilings on both paths, and original TASK-003 scenario 2 requires equivalent authorized reads and honest limit/refusal behavior.

Location: `sdks/wyrd-sdk-ts/native/src/lib.rs:1385–1399` (`NativeBifrostQueryStream::collect`), reached from `ui/packages/ui/src/lib/node.ts:37` and `sdks/wyrd-sdk-ts/wyrd/src/index.ts:544–550`.

Producer-to-consumer evidence: public shared `WireRequest` explicitly accepts `max_rows`/`max_bytes` (`contract.ts:53–54`). `nodeExecutor` forwards these options to SDK collect, which forwards them unchanged to native collect. Native uses them directly as builder budgets, importing only defaults; the builder's `new`/`charge_bytes` enforce the supplied budget, without checking the shared maximum. Conversely MCP executor forwards the same fields to `bifrost.query`; existing `QueryArguments::validate` (`query/collect.rs:182–194`) rejects zero or rows greater than shared `MAX_ROWS_CEILING=10000`, or bytes greater than shared `MAX_BYTES_CEILING=16777216`. Its advertised schema uses the same limits. The shared query-result owner exposes these constants but does not enforce requested-budget admission.

Thus an otherwise valid three-row query with `max_rows:10001` or `max_bytes:16777217` can succeed through Node while MCP refuses before query execution. Larger Node caller budgets also permit a complete retained JSON result exceeding the MCP maximum. The HTTP streaming query floor cannot close this: `query/floor.rs:23–26` permits one million rows/256MiB, and `BifrostQueryRequest` has no collected-JSON budget. No nodeExecutor sibling path checks the options. The public six-read executor contract is explicitly required even though current Chart's loader uses defaults; invoking an advertised optional budget is a reachable supported API, not a dormant helper. Existing live parity tests cover output exceeding a small permitted budget, not invalid requested budgets.

Smallest testable correction: reuse the shared bounded-result constants and centralize the allowed-budget invariant at the existing shared collection/admission owner, consumed by MCP and native collection rather than duplicating arbitrary frontend caps. Reject invalid requested budgets with the existing stable validation semantics; preserve valid defaults, caller-smaller budgets, exact byte accounting, valid/degraded terminals and cancellation. The shared SDK remains a streaming API; do not reduce unrelated streaming query limits. Extend the existing live parity scenario with just over-maximum and zero requested row/byte budgets, asserting equivalent refusal and continued ordinary query success, plus boundary-value coverage in the narrow owner. No new endpoint/dependency/harness is required.

This proposal is based on complete source paths, not a new live execution: current supplied green integration covers permitted small ceilings only. Independent validation should confirm the exact error projection and whether allowed-budget admission belongs at shared builder construction or an existing adjacent admission method, without moving server permission/audit ownership.

## Verification limits

Human visual acceptance on 2026-10-20 is recorded evidence, not a new inspection of this candidate. Linux native bundles/images and live degraded-terminal parity were not rerun or inferred from green macOS/UI checks. Revision 11's downstream deployed-browser/conversation journeys remain downstream and are not required anew here. No test was modified, weakened or skipped to obtain this review result. The structuredClone harness correction records a diagnosed jsdom capability gap; source browser support remains unchanged.

Skipped: implementation, builds, broad test reruns, fresh live/browser and Linux/image execution. Risk: the two proposed invariant gaps remain pending independent validation; supplied green tests do not exercise them.
