# TASK-003 invariant review

Overall result: **FAIL**. Proposed active ledger: **FIND-003-13**, incomplete closure of the existing range-resolution invariant. FIND-003-1..12 and FIND-003-14 are closed at their diagnosed causes. No new finding ID is proposed.

## Subject, authority and coverage

Reviewed immutable candidate `21dab5d11e484ffceaa4718fc7370c7d6c67dc75` against cumulative base `c62ebce8c4de0d43a2b9d50d360de628c49fd533` (parent of `ff4145ac6`), including the original TASK-003, spec revision 13, prior verdict/validation/remediation history through R6, and the R6 locating diff `6aace6e07...21dab5d11`. HEAD remained the candidate when checked. Unrelated PR #205/main changes and separately accepted TASK-027 layout mechanics are excluded as findings; shared loader/renderer seams are included.

Applied root AGENTS, TS SDK AGENTS (the only nested rules applicable to the changed production/test paths), task-review instructions, spec-driven development, maintainer style, errors/agent-surfaces guidance, TESTING, the UI skill, architecture authority map, canonical CardRef section of wyrd-design, security principles/credential ownership, and Bifrost query/source/terminal ownership. No CodeGraph index exists. No peer discovery conclusions were read.

Source coverage includes the cumulative package contract, schemas/catalog generator, loaders, plot/format owners, renderer and action handoff, Node/MCP adapters, every owned component/primitive, app artifact entry/build, gallery/custom app/BFF consumers, session-to-SDK projection, shared-client query-only construction, shared JSON collection and server/native lifecycle consumers, SDK wrapper/declarations, production packaging and their relevant unit/live proofs. Generated catalog/problem outputs were compared with their source owners rather than treated as independent authority. TASK-027's layout owner remains outside this task's acceptance attribution.

## Acceptance and invariant matrix

| Requirement, acceptance criterion, constraint or non-goal | Implementation evidence | Verification evidence | Result |
|---|---|---|---|
| Publishable package, approved component/loaders/view/catalog/node/mcp entry points; browser-safe exports | package.json exports, index.ts/view.ts, type-only SDK imports, JSON import attribute in contract.ts | Supplied UI check, retained native public-import smoke and browser-source assertions | PASS |
| Generated catalog and shared input/action dispatch authority | scripts/catalog.ts uses schemas.ts; validatePanel/validateIntent and ViewDocumentSchema share definitions | Supplied catalog --check; retained catalog/schema/action assertions | PASS except resolved-range invariant below |
| Root and explicit ranges resolve to the same valid RFC 3339 absolute contract on all hosts; validation errors remain local and visible | RelativeRangeSchema checks only native Date capacity; resolveRange can emit signed six-digit years that AbsoluteRangeSchema refuses at both HTTP hosts | Fresh artifact-free source reproduction below; new test explicitly asserts acceptance of the counterexample span without testing its resolved absolute range | FAIL: FIND-003-13 |
| Explicit panel range wins; inherited relative range resolves once; SQL and typed binds preserved | loadChart:145 input-first selection; chartParams; WyrdView.start resolves inherited range once and passes one context; executed feeds Open in Query | Retained fixed-clock, mixed explicit/shared, brush/reset and exact-bind assertions; supplied 40/40 | PASS for representable contract-valid ranges |
| Complete timeseries/bar/stat/table shape validation and truthful empty/error/degraded states | shapeChart, shared columnIndex, Chart branches, terminal outcome notice and source footer | Retained shape/state/signed/duplicate-column tests, live query proof supplied | PASS |
| Twenty distinguishable series; overflow grouping preserves authored identity | typed [group,y] grouping before summation, captions collision qualification, MAX_SERIES=20, nineteen plus aggregate above limit; four markers/five dashes | Retained 5/20/21, authored collision, null/text-null and multi-y assertions | PASS |
| Exact flat Card identity, bounded version paging, independent listing failures, canonical copied ref | CardRefSchema; loadCardHeader exact read then returned kind/name listing, five pages, compareVersions; fmtCardRef reused by header/chip | Retained exact/paged/denied/truncated/copy/version-action tests; supplied live registry/BFF proof | PASS |
| Typed host actions, pending duplication prevention, visible completion, no frontend mutation authority | components.emits, validateIntent, WyrdView.act, ActionButton/ActionNote, CardHeader named states and authoritative selection | Retained direct/view action tests; no arbitrary dispatch or credential-bearing document | PASS |
| Supersession/cancellation prevents dependencies and stale results | WyrdView abortAll/settle generation fencing; CardHeader signal before each page; Node acquisition/active/post-collect checks; SDK return/native close notification | Retained cancellation tests; supplied native-backed/live checks; no fresh runtime execution claimed | PASS |
| All six reads reuse server/SDK/MCP owners; no alternate domain service | closed WireRequest; Node SDK handles; MCP tool mapping, cards.get envelope projection; existing QueryArguments params | Supplied real-server file 2/2; retained six-read and malformed/denied proof | PASS |
| Positional normalized results preserve duplicate names/nulls/encoding and accepted terminal evidence; no partial success | JsonResultBuilder shared by server/native; caller retains schema/IPC/terminal/EOF and failure ownership; finish projects success/degraded evidence | Retained exact JSON-byte/positional regression and supplied live SDK/MCP proof; live degraded parity remains historical limitation | PASS |
| Original numeric ceilings refused losslessly, common allowed range, cancellation/release on refusal | native Option<f64> -> parse_ceiling decimal parse -> admit_ceilings; stream take precedes refusal; server QueryArguments uses same admission | R5 supplied native-backed refusal/closed-stream/fresh-query evidence and retained assertions | PASS |
| Rust-owned authorization, tenancy, browser session and query-only transport | BrowserSession.client passes selected private cookie/trusted origin to existing Rust client; Bifrost::connect_with uses existing read-only sink shape; BFF cannot select credentials from panel input | Supplied focused wyrd-client test and BFF cross-tenant/live negative evidence | PASS |
| Primitives and components share brand, direct props, accessible controls and individual human acceptance | shared token theme and component markup; Drawer native dialog; Chart focus/keyboard; fixture gallery | Recorded 2026-10-20 individual human approval; supplied checks, no renewed browser/human inspection | PASS within recorded proof limits |
| Custom direct/view app, built AppBridge renderer, bounded BFF and production native SDK packaging | examples/ui-app, main.ts, vite app artifact, scripts/build.sh packs SDK/native addon under existing owner | Supplied real-server 2/2 and historical Darwin bundle/host artifact proof | PASS within recorded platform limits, except range parity finding |
| Mandatory changed-item rustdoc, including panic test documentation | client.rs:760 ff now documents # Panics; assertion body unchanged by R6 | Source inspection; supplied fmt:check/lints/focused nextest | PASS: FIND-003-14 |
| INV-001..007 and explicit non-goals: no replacement durable owner, generic write dispatcher, Ask, saved views, A2UI, production MCP registration or publishing | Existing owners retained; scoped new pure UI/package contracts and shared query projection; deferred work absent | Source/diff inspection; supplied format/lint/catalog checks | PASS except bounded behavior finding |

## Proposed finding

### INV-RANGE-ABSOLUTE / FIND-003-13 — Date-valid relative input can still produce a refused absolute range

Classification: **INCORRECT**. Retain the existing ID because the source invariant and correction boundary are the same: accepted relative input must resolve into the existing absolute-range contract or produce the existing field-specific validation error. This is not a new consumer-specific guard request.

Obligation: spec REQ-030 requires portable loaders and identical host behavior, range binds as RFC 3339 strings, and visible field-specific validation; original task scenario 1 and Chart error acceptance apply. R6 explicitly requires resolution to valid absolute bounds using the existing absolute-range contract, with no per-host arithmetic validation or arbitrary retention cap.

Exact producer locations: `packages/ui/src/lib/schemas.ts:43–53` defines the existing RFC 3339 absolute contract; `:67–74` accepts any relative span whose negative epoch fits native Date; `:198–200` returns the resulting ISO strings as AbsoluteRange without checking that contract. `src/lib/ui-library.test.ts:396` explicitly asserts that `100000000d` remains accepted, but checks only document admission.

All UI paths in this report are rooted at `crates/wyrd/wyrd-server/wyrd-ui/` unless otherwise identified.

Fresh read-only reproduction against the candidate source, without building or writing artifacts:

```sh
mise exec -- node --input-type=module -e 'import { RangeSchema, AbsoluteRangeSchema, resolveRange } from "./crates/wyrd/wyrd-server/wyrd-ui/packages/ui/src/lib/schemas.ts"; const relative = {last:"100000000d"}; const accepted=RangeSchema.safeParse(relative); const absolute=resolveRange(relative, Date.parse("2026-10-22T00:00:00Z")); console.log(JSON.stringify({relativeAccepted:accepted.success,absolute,absoluteAccepted:AbsoluteRangeSchema.safeParse(absolute).success}));'
```

Observed output:

```json
{"relativeAccepted":true,"absolute":{"from":"-271764-02-09T00:00:00.000Z","to":"2026-10-22T00:00:00.000Z"},"absoluteAccepted":false}
```

The installed Zod datetime source (`node_modules/zod/src/v4/core/regexes.ts`, dateSource and datetime) permits four-digit date years; native Date/toISOString additionally represent signed expanded years. Therefore native Date validity alone does not prove this repository's RFC 3339 contract. No external standard change is needed: the existing schema and the observed safeParse refusal establish the mismatch.

Producer-to-consumer and sibling trace:

1. `ViewDocumentSchema.range` and every `ChartInputSchema.range` consume the same RelativeRangeSchema. `validateView` and `validatePanel` therefore accept the example.
2. `WyrdView.start:240–256` resolves a root range once, then invokes each valid panel's selected loader with the signed-year output. With an executor, the selected adapter calls `loadPanel` locally. With `load={remoteLoader(...)}`, `view.ts:51–65` serializes that same range in the HTTP request.
3. Shipped POST `src/routes/t/[tenantKey]/ui/load/+server.ts:19–23` and custom POST `examples/ui-app/src/routes/view/load/+server.ts:8–10` both refuse the generated range through AbsoluteRangeSchema before loading any component. This affects every remote panel, including CardHeader whose exact Card read does not consume time bounds.
4. The local executor path has no absolute context revalidation, so the same accepted document can load its CardHeader and can issue Chart SQL with signed expanded-year binds through `loadChart:145–151`; MCP App uses this local path too. An explicit Chart relative range likewise produces these binds after `validatePanel` succeeds, including when the endpoint inherited range is valid.
5. The direct example page's `resolveRange(range)` is another public resolver consumer; its authored `24h` input remains valid and must stay working. Brush action input already uses AbsoluteRangeSchema and is not the source of this defect. No other production resolver callers were found in package, app and example searches.

Observable consequence: a schema-accepted view (and now explicitly asserted accepted span) behaves differently between local/MCP and remote HTTP hosts. Remote views show per-panel validation errors and issue no reads, including unrelated CardHeader reads; local/MCP views issue their reads. Explicit Chart inputs may also forward a value outside the declared RFC 3339 bind contract. The original huge-span RangeError is repaired, but its stated replacement guarantee is not complete. The reproduction establishes the producer/consumer mismatch; no fresh BFF/browser/live query execution is claimed.

Smallest correction outcome: finish the same shared range admission/resolution owner so its successful result satisfies the existing AbsoluteRangeSchema, and route inability to do so through existing inputProblem/canonical validation at root or explicit panel scope. Reuse the existing absolute contract rather than equating Date validity with it. Preserve a single inherited resolution clock, explicit precedence, ordinary relative/absolute ranges and exact SQL/binds. Do not widen RFC 3339 to native expanded-year syntax, add an arbitrary retention limit, duplicate checks at endpoints/components, or replace transport/query ownership. Private signatures remain implementation-owned.

Focused closure proof: extend the existing range assertions with this Date-valid but contract-invalid counterexample. Prove an accepted resolved range round-trips through AbsoluteRangeSchema; otherwise prove canonical field-specific rejection at the shared boundary. Cover root local and remote paths (including a CardHeader or valid sibling so host parity is observable), explicit Chart range with valid inherited bounds, and both existing POST harnesses. Assert no executor calls for rejected inputs, valid sibling behavior for a rejected explicit panel, and retained ordinary fixed-clock/precedence assertions. Existing unit/endpoint owners suffice; no new dependency, harness or broad live suite is required.

## Prior-finding closure against current source

| Prior ID | Current-source evidence | Disposition |
|---|---|---|
| FIND-003-1 | loadChart chooses its own range before context.range; executed params feed Chart action; WyrdView shares inherited bounds | CLOSED for precedence; representability remains ID13 |
| FIND-003-2 | shapeChart, timeSeries, barRows and explicit stat lookup all use first-position columnIndex; table remains positional | CLOSED |
| FIND-003-3 | Chart signed bounds include zero and round outward; signed bar endpoints produce nonnegative heights and separate negative labels | CLOSED |
| FIND-003-4 | MAX_SERIES=20; folding starts above twenty; four markers/five dashes distinguish all twenty outputs | CLOSED |
| FIND-003-5 | Header named version/tab state blocks pending/unavailable input, ActionNote renders outcomes; select resets to host version | CLOSED |
| FIND-003-6 | Node closes acquired-after-abort streams, listens during collection and rejects late result; native close notifies before locking; renderer fences aborted generations and Card listing checks before pages | CLOSED |
| FIND-003-7 | Generated Rust problem projection feeds local producers; both endpoint refusal helpers return complete canonical problem; failed preserves denial | CLOSED at prior error-projection cause |
| FIND-003-8 | contract runtime JSON import has standard `with { type: 'json' }`; browser SDK imports are type-only; retained native smoke | CLOSED |
| FIND-003-9 | Aggregate label checks final retained authored captions; plot, tooltip and legend consume same output | CLOSED |
| FIND-003-10 | Typed JSON [group,y] identity precedes summing; caption collision qualification separates null/text-null and multi-y authored pairs | CLOSED |
| FIND-003-11 | Native receives original Number as f64, lossless integer parsing precedes common range admission, stream taken before any refusal; sibling server uses same admit_ceilings | CLOSED |
| FIND-003-12 | fmtCardRef emits kind/name/version plus optional supplied UID; header/chip copy/text share owner; changing version omits stale UID | CLOSED |
| FIND-003-13 | R6 rejects Date-invalid huge spans at schema, but accepts Date-valid spans that resolve outside AbsoluteRangeSchema; source reproduction above | ACTIVE, same shared resolution invariant |
| FIND-003-14 | Changed browser-session connection test now includes # Panics with assertion conditions; no test body weakening | CLOSED |

## Existing-owner and complexity audit

The required portable loader is not a duplicate registry/query/session service: existing Cards.get/list and Bifrost/verification handles retain durable behavior. Loaders add only UI planning, bounded dependent paging and presentation shaping. Node and App.callServerTool adapters are necessary because native Node SDK cannot run inside MCP iframes; neither adds a replacement network protocol. Session projection reuses the existing Rust client and existing query-only sink, preventing an unnecessary bearer-only ingest dial without creating browser ingest auth.

JsonResultBuilder extracts the existing MCP positional projection into the narrow shared Arrow owner and is consumed by both native and server paths. General Arrow iteration would not supply its normalized whole-result encoding/byte accounting. It does not replace the server ResultCollector's cancellation/settlement/clean-EOF ownership or QueryResultStream's protocol validation. Shared admit_ceilings similarly avoids diverging per-surface policy. Native parse_ceiling follows the existing f64 deadline-validation pattern and preserves the original Number until validation.

View validation, mountView, per-panel controllers and typed action handoff are explicitly required outcomes; no saved-view store or generic mutation dispatcher is added. Native dialog and existing brand generation are reused for primitives/themes. The range finding needs extension of an already-present owner, not a new layer. No independent unearned abstraction/dependency finding is proposed.

## Verification limits

Candidate UI check, 40/40 tests, catalog --check, fmt:check, lints, focused wyrd-client nextest and real-server integration 2/2 are supplied evidence. Previous task and R1–R6 records provide historical T1–T5, native refusal and human/Darwin evidence; they do not establish renewed cross-platform, images, deployed-browser/human or live degraded-terminal parity. This reviewer executed only read-only source/Git inspection and the artifact-free source-schema reproduction, and wrote only this report. No source/test changes, builds, test reruns, commits or pushes occurred.

Skipped: build/test/browser and cross-platform reruns under the packet-only restriction. Risk: accepted Date-valid ranges can still violate the shared RFC 3339 output contract and diverge across hosts.
