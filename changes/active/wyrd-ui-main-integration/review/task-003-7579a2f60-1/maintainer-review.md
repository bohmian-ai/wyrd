# TASK-003 independent maintainer review

Overall result: **FAIL**. One material proposal, **MNT-003-11**, requires independent validation. FIND-003-10's authored-series correction is complete in the inspected source; the proposal concerns the cumulative bounded-result boundary, not that remediation.

## Immutable subject and method

Repository `/Users/stevenforrester/Documents/GitHub/wyrd-ui-main-integration`, branch `wyrd-ui-main-integration`; cumulative base `c62ebce8c4de0d43a2b9d50d360de628c49fd533` through candidate `7579a2f603c023daae0d15b9d5cc3013eec6f00d`. HEAD matched that candidate at the end of source inspection. Read the cumulative diff directly, original TASK-003, approved spec revision 13, prior FIND-003-1..10 ledger and remediation instructions. No peer discovery report was read. There is no `.codegraph/` directory. Source and tests were not modified; only this report was written.

Applied root AGENTS.md, the TypeScript SDK's nested AGENTS.md, `changes/spec-driven-development.md`, `agent_docs/{index,maintainer-style,rust,errors,agent-surfaces}.md`, TESTING.md, architecture authority map, Wyrd doctrine/client model, Bifrost query terminal authority, browser-session security boundary, deployment/release boundary, and the repo UI skill/brand guidance. The lens was owner discoverability, complete workflow reuse, typed contracts, lifecycle clarity, declarations and outcome-focused proof, not optional redesign.

`git range-diff 695ca914d^..wyrd-ui-pre-205 ff4145ac6^..7579a2f60` independently confirms equal foundation implementation/remediation patches through the old reviewed candidate. The SDK bundle patch differs only in context (`image` to `bundle_in`); packet-authority link/history resolution differs as recorded. Flat-identity and authored-series remediations are subsequent explicit additions. Removed upstream commits and TASK-027 layout commits are excluded as findings; WyrdView seams were inspected for foundation regressions.

## Changed-surface coverage

UI paths below are relative to `crates/wyrd/wyrd-server/wyrd-ui/`.

| Surface inspected | Owners, callers, proof and maintenance assessment |
|---|---|
| Portable package and schemas | `packages/ui/{package.json,svelte.config.js,tsconfig.json}`, `src/lib/{index,contract,schemas,loaders,format,plot,view}.ts`, catalog/problem JSON, generator. Generated SDK domain types remain type-only at browser entry points; public Node import is isolated. Closed schemas and component map are the shared dispatch owner. No extra client or registry workflow is introduced. |
| Chart and all primitives | Full Chart module and `ActionButton`, `ActionNote`, `CardHeader`, `CardRefChip`, `Drawer`, `DurationBar`, `EmptyState`, `Sparkline`, `StatusPill`, `VerdictPill`. Chart's producer is shared by direct components, WyrdView, gallery, example and AppBridge. Native dialog owns focus/Escape; action controls share feedback instead of implementing separate refusal paths. Plot/format helpers are pure beside their consumers. |
| Panel orchestration | WyrdView module, view validation, remoteLoader and mountView. Current controllers fence late results and abort superseded work; catalog validation precedes dispatch; host actions return named visible outcomes. TASK-027 geometry/edit ownership is separate; it does not replace the loader or render another Chart. |
| Host adapters and credentials | Full `node.ts`, `mcp.ts`, tenant `ui/load/+server.ts`, browser-session client projection, example executor and both page/load routes, App entry and single-file build configuration. Six read branches call existing SDK/MCP capabilities. Card envelope projection uses current `cards.get`; flat Card requests use exact generated fields. No parallel OAuth, tenancy or persistent authority. |
| Shared result conversion | Full `wyrd-queue/src/json_result.rs`, server `query/collect.rs` cumulative extraction and collector loop, native `NativeBifrostQueryStream::{new,next,collect,close,schema_ipc,terminal_json}`, TS `BifrostQueryStream` constructor/iteration/collection/return. One positional encoder replaces the former server-local implementation; stream framing and settlement remain with their existing owners. Ceiling admission has the material gap below. |
| Shared client construction | Changed `Bifrost::{connect_with,with_sink,query_only,read_only,from_sink}` and client browser-session predicate/callers/test. Existing QueryOnlySink and one assembly path are reused. This prevents a bearer-only ingest dial for the Rust-owned HTTP-only session without another transport or credential store. |
| Build and generated parity | `scripts/build.sh`, relevant `mise.toml` changes, UI workspace/manifests/lock, theme generator/output, native Cargo dependency adjustment, both generated `index.d.ts`/`index.d.cts` changes. Native collect signature/documentation changes match declarations; the SDK exports the bounded wire projection. Production bundle extends the established build owner with packed SDK/addon rather than adding another release pipeline. |
| Proof and examples | Shared `ui-fixtures.ts`, setup, relevant `ui-library.test.ts` foundation scenarios, complete integration owner and AppBridge helper; examples app metadata/config/README and direct/view pages; gallery's dev-only route and shared-fixture composition; formatter migration consumers. Assertions observe plot attribution, selected versions, typed calls, state and cancellation. The latest iframe addition supplies a normal platform API to jsdom; it does not weaken an assertion. TASK-027-only scenario bodies are outside this task's acceptance ownership. |

## Existing workflow alternatives

The shared `JsonResultBuilder` is justified: prior server `ResultCollector` already encoded positional results but could not be consumed by the SDK without a server-tier dependency. Extraction preserves one conversion owner and leaves framing/settlement with server and SDK streams. It should also provide or feed the shared bounds used by the added bounded entry point, rather than leaving only the MCP adapter to impose them.

Cards.get/list, Bifrost.stream/listTables/describeTable and Execution.status are existing complete read owners. UI loaders add expressly approved portable planning and prop shaping, not a second registry/query service. The browser session reuses `WyrdClient` and QueryOnlySink; the host adapters are required projections. Native `<dialog>`, Svelte lifecycle, Map and JSON serialization cover the local requirements without a new framework. Catalog/theme additions extend their current generators. No other material duplication or unearned machinery was found.

## Prior-finding closure in current source

| IDs | Current source evidence |
|---|---|
| FIND-003-1 | `loaders.ts::loadChart` chooses input.range before context.range; the executed payload and request share params. Existing mixed-panel brush/reset and relative-clock assertions exercise it. |
| FIND-003-2 | `shapeChart`, timeSeries, barRows and explicit stat selection use the same `columnIndex` first match; table remains positional. Duplicate-name tests cover conflicting descriptor orders. |
| FIND-003-3 | Chart signed line and bar domains include zero; negative bars have positive extent and label clearance. Mounted signed/flat/zero tests remain. |
| FIND-003-4 | MAX_SERIES is twenty, overflow keeps nineteen plus one, and Chart pairs four markers with five dashes for twenty styles. Existing 5/20/21 assertions remain. |
| FIND-003-5 | CardHeader's version/tab named states feed shared ActionNote and disabled controls; version selection resets to host input; direct and WyrdView refusal tests remain. |
| FIND-003-6 | Node adapter closes a stream acquired after abort, listens during collect, rejects late post-abort results and removes its listener. WyrdView separately owns generation suppression. Existing acquisition/active/pre-abort checks remain. |
| FIND-003-7 | Canonical generated problem examples feed local producers and both endpoint responses. Existing projection/refusal assertions cover full canonical output and retained upstream identity. |
| FIND-003-8 | `contract.ts` uses the JSON import attribute; root test script performs native Node public-loader/MCP imports after package build. |
| FIND-003-9 | `timeSeries` chooses a synthetic aggregate caption against final authored captions; existing real `other`, qualification-like authored text, exact totals and rerender checks remain. |
| FIND-003-10 | `plot.ts:43` keys on JSON `[raw series,y]` before accumulation, retaining raw group/y for `captions` at lines 79–93. JSON distinguishes null from text and escapes arbitrary tuple text. Qualification is deterministic and only unqualified collision participants can change, so it terminates within the number of groups; qualified captions are injective for admitted string-or-null series. Chart consumes the final unique labels in every keyed/accessible/tooltip path. Tests at `ui-library.test.ts:150–174` assert four distinct tuple values 1/10/2/20, null versus text null plus qualification-like text, and rerendered attribution. No downstream guard or public identity extension is needed. |

Flat identity adaptation also closes through strict CardRefSchema (rejects `space`), resolved kind/name version-list planning, CardHeader/ref formatting, fixtures, example and real BFF tenant proof. This is human-directed scope, not a retrospective finding.

## Material proposed finding

### MNT-003-11 — the new bounded Node collector has no admission of the shared result ceilings

**Classification:** INCORRECT. **Proposed stable ID:** next unused FIND-003-11, subject to independent validation.

**Governing obligation and maintenance cost:** REQ-030's Query results clause says to keep current row/JSON-byte ceilings on both paths; TASK-003 Scenario 2 promises identical six reads with honest limits. The maintainer guide requires types and their owning workflow to carry a contract, and root rules require one shared root correction. Extracting the size constants into the shared builder module while retaining their only input admission in server QueryArguments leaves two public bounded collection paths with different contracts. A maintainer changing the shared limits cannot rely on the Node entry point respecting them.

**Exact locations:** `sdks/wyrd-sdk-ts/native/src/lib.rs:1377–1391`, specifically construction at 1386–1390; `crates/shared/wyrd-queue/src/json_result.rs:69–76`. Reachability is `packages/ui/src/lib/contract.ts::WireRequest` (`max_rows?: number`, `max_bytes?: number`) → `node.ts:30–37` → SDK `BifrostQueryStream.collect` at `wyrd/src/index.ts:545–553` → native collect. Compare server `query/collect.rs:171–181`, which refuses zero or values beyond shared MAX_ROWS_CEILING=10,000 and MAX_BYTES_CEILING=16 MiB before `BoundedQuery::run`; MCP `src/mcp/bifrost.rs:236–244` enforces this validation.

**Source evidence and consequence:** native collect unwraps caller u32 values and passes them directly into `JsonResultBuilder::new`; that constructor stores them unchanged. Its later batch/byte checks compare only against those caller values. Neither the TS method nor Node executor validates their range. Therefore a public Node executor request `{op:'bifrost.query', sql:'SELECT 1', max_rows:10001}` can return its one-row success, while the identical MCP request is refused before execution. `max_bytes:16777217` has the same discrepancy. A larger permitted query can retain beyond the maximum bounded size promised for this surface; the ordinary server stream's own larger ceiling is not the missing bounded-result ceiling. This is source-demonstrated reachability, not a claimed live reproduction.

**Source/writers/siblings:** JsonResultBuilder is the sole writer of both caller ceilings and the result budget, used by server ResultCollector and NativeBifrostQueryStream.collect. Server QueryArguments is the existing admission mechanism; its schema/validation shares constants already moved to json_result. Bifrost's general streaming iterator is a sibling with different semantics and should remain streaming. All direct SDK collect users and Node executors are affected; ordinary Chart load defaults do remain within bounds. MCP stays bounded today and must retain its existing refusal behavior. No CSS, panel schema, renderer or BFF-specific guard fixes the public collector.

**Smallest testable correction:** admit the bounded collection options at the shared Rust bounded-result boundary, reusing the existing shared ceiling constants and current MCP validation rules, and project the result through existing native/SDK Wyrd error handling. Use that admitted policy for Node collection while preserving existing MCP schema/refusal behavior. Refuse invalid bounds; do not silently expand or clamp them, add UI-only guards, alter general stream limits, or add another query transport. A focused existing Node/MCP test should cover zero, ceiling+1, exact maximum and omitted defaults for both options; an above-maximum option with a tiny result directly exercises admission without creating load. Retain existing actual-result-over-budget and cancellation tests. No new architecture or product decision is needed.

Existing live proof tests positive small ceilings (2 rows, 64 bytes), defaults and parity, so their green results do not cover this option-domain discrepancy.

## Verification limits

Candidate UI check (zero package/app errors), 40/40 tests, build and 2/2 managed Postgres integration results are supplied by the user, not freshly executed here. Historical native/generated gates, darwin bundle and human checkpoint remain historical evidence. No source-generating check, build, live server test, browser inspection or Linux/image job was run by this reviewer. No missing cross-platform execution was converted into a source defect.

Skipped: implementation and duplicate verification runs. Risk: the proposed bounded-option mismatch remains pending independent validation; Linux/image and fresh browser proof were not repeated.
