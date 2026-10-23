# TASK-003 system-resilience review

Result: **FAIL**. One proposed finding: **SYS-1**, retaining **FIND-003-13** because relative-range admission still produces bounds outside the existing absolute-range contract.

## Immutable subject and coverage

Repository `/Users/stevenforrester/Documents/GitHub/wyrd-ui-main-integration`; branch `wyrd-ui-main-integration`; cumulative base `c62ebce8c4de0d43a2b9d50d360de628c49fd533`; candidate `21dab5d11e484ffceaa4718fc7370c7d6c67dc75`. HEAD matched the candidate at the end of inspection. The cumulative runtime diff and its surrounding owners were inspected independently; the R6 delta located the latest range correction. Earlier review history was treated as hypotheses, without consuming this round's peer discovery reports. PR #205/main content and separately accepted TASK-027 layout are excluded from findings.

Authorities read: root and TypeScript SDK AGENTS, original TASK-003, specification revision 13 REQ-030, spec-driven development, maintainer style, TESTING, UI skill, deployment-and-release supported topologies/browser-session/local-bundle sections, reliability-and-recovery Oracle/dependency failure boundaries, and Bifrost read-audit/terminal contract. No CodeGraph directory exists.

## Deployed paths and existing owners

| Runtime path | Source coverage and ownership | Failure boundary |
|---|---|---|
| First-party panel | `WyrdView.svelte` -> `view.ts::remoteLoader` -> tenant `ui/load/+server.ts` -> `BrowserSession.client` -> `node.ts::nodeExecutor` -> native SDK/shared client -> Rust service | One panel/request; browser holds no SDK principal. Session cookie/origin remain server-side. |
| Custom app | `examples/ui-app/src/executor.ts`, direct server page and `routes/view/load/+server.ts` reuse portable loaders/Node executor | One request using the app's principal; no added session or durable authority. |
| MCP App | `packages/ui/src/app/main.ts`, `mcp.ts`, `view.ts` and app build configuration | Host owns connection and credential; tool-result replacement destroys the old renderer. No independent network client. |
| SDK query collection | `BifrostQueryStream.collect/return`, native `NativeBifrostQueryStream.collect/close`, shared `bifrost/query.rs::QueryResultStream` and `wyrd_queue::json_result` | Native takes one stream; all admission/collection exits release it. Existing stream validates framing, terminal and EOF. |
| MCP collection | Server `query/collect.rs::BoundedQuery/ResultCollector`, existing query service and controls | Cancellation signals the query, then existing controls prove settlement. No new execution or retry owner. |
| Session Bifrost connection | Shared client `is_browser_session`, facade `connect_with/read_only/from_sink/QueryOnlySink` | Skips unsupported bearer-only ingest; reuses query-only sink and refuses writes terminally. Other principals retain ingest connection. |
| Deployment | `scripts/build.sh::bundle`, UI/SDK package configuration, native declarations and wrapper | Builds/packs host native addon beside SDK and smoke-imports handler plus SDK. No new process supervisor, durable state or required secret. |
| Rendering and actions | Chart, CardHeader, plot/format, primitives, schemas/contract, gallery and example consumers | Display-only components; validated intents delegate to host, pending blocks duplicate submission, no automatic mutation retry. |

The shared JSON builder extends the pre-existing MCP JSON projection for reuse by native collection. The existing Arrow collector measures a different transport/result form; it does not replace the required JSON accounting. Existing QueryResultStream/Oracle controls continue to own protocol and lifecycle. The existing query-only construction supplies browser-session reads without creating a second transport. No further graph, durable store, cache, or orchestration replacement is required.

## Failure and recovery assessment

| Credible interruption | Traced behavior | Availability and recovery |
|---|---|---|
| Invalid collection ceiling | Native `Option<f64>` -> `parse_ceiling` -> shared `admit_ceilings`, after `stream_slot.take()` | Stable validation refusal; stream drops and cannot be reused. Fresh independent queries remain available. No clamping, retry or partial result. |
| Result crosses row/JSON budget | `JsonResultBuilder::accept_schema/accept_batch/finish` -> caller failure paths | SDK drops response; MCP requests cancellation and settles through controls. Entire result fails; unrelated requests remain available. |
| Cancel while SDK query opens | Node executor checks signal after awaited acquisition and closes the just-acquired stream | No collection or usable late rows. Acquisition remains governed by existing client timeout/query deadline. |
| Cancel during collection | Signal -> SDK `return` -> native `close` notifies before awaiting mutex; collect selects notification | Collector releases stream and lock, permitting close. Post-collect signal check fences a completion race. No new server/process crash path. |
| Replace range/input, remove panel, destroy renderer | `WyrdView::abortAll/start/settle`; CardHeader checks signal before each dependent page | Controllers fence stale results even if underlying transport cannot cancel. Failed Card prerequisite issues no listing; failed listing preserves Card. New document reloads normally. |
| Transport failure, malformed stream, failed terminal, missing EOF | SDK `QueryResultStream::next_batch` validates terminal/EOF; MCP `ResultCollector::consume/collect` enforces framing and settlement | No usable partial rows. Fault affects query, not shared process. No replay, synthetic success or new automatic retry. |
| Rust/Node replacement or dependency outage | Request-scoped SDK projection uses existing Rust-owned browser session; service remains durable session owner | No in-memory Node credential authority to recover. Failed database/trust/read operation remains refusal/error; new request after dependency recovery can re-read authoritative state. No changed persistence schema. |
| Relative range beyond native Date | R6 schema rejects `1000000000000d` before effect/load execution | Original RangeError escape is fixed for that input. Field-specific invalid-document/panel state and zero-read proof cover it. |
| Relative range native Date can encode but absolute schema cannot | `100000000d` passes relative schema; resolver emits signed expanded ISO year; both HTTP hosts reject inherited range before any panel load | Remaining cross-host admission inconsistency, detailed below. Restoring an ordinary range reloads; no process or persisted-state corruption. |

## Proposed finding

### SYS-1 / retain FIND-003-13 — Date-valid spans can still produce invalid absolute ranges

Classification: **INCORRECT**. Violated obligation: REQ-030 portable validated documents/loaders and shared RFC 3339 bounds; R6 explicitly requires accepted input to resolve to valid absolute bounds or a canonical field-specific validation problem through the existing producer authority.

Location: `packages/ui/src/lib/schemas.ts:70–75` (`RelativeRangeSchema`) and `:198–201` (`resolveRange`), rooted at `crates/wyrd/wyrd-server/wyrd-ui`. Reachable consumers: `WyrdView.svelte:240–256`, `view.ts:54–64`, tenant `ui/load/+server.ts:19–29`, custom `examples/ui-app/src/routes/view/load/+server.ts:8–12`, and explicit Chart range resolution in `loaders.ts::loadChart`.

The refinement tests only native Date validity of a span measured from epoch zero. Native Date admits signed six-digit years that `AbsoluteRangeSchema` does not accept as RFC 3339. A read-only probe against the actual source, using `mise exec -- node --input-type=module`, produced:

```text
RangeSchema.safeParse({last:"100000000d"}).success = true
resolveRange({last:"100000000d"}, Date.parse("2026-10-22T00:00:00Z"))
  = {from:"-271764-02-09T00:00:00.000Z",to:"2026-10-22T00:00:00.000Z"}
AbsoluteRangeSchema.safeParse(resolved).success = false
```

This is admitted by the public schema and explicitly asserted as admitted by the newly added test at `src/lib/ui-library.test.ts:396`. It is not a hypothetical external clock or a zero-caller helper. A view with that root range and a CardHeader (or range-independent Chart SQL) passes document validation and starts loads. Its remote loader forwards the generated bounds; both HTTP hosts refuse those bounds before dispatching the panel. The same panel can load under the direct executor or MCP executor because CardHeader does not consume the time range. Thus a valid document fails differently across required hosts, and a malformed generated range is reported at downstream `Panel input` rather than being prevented by the shared range producer. For an explicit Chart range, invalid absolute bind values can likewise be produced after panel admission.

This retains the existing source/invariant finding rather than assigning a new ID for another caller. R6 closes the escaped exception for dates beyond Date's representable span, but does not establish its claimed absolute-range postcondition. The service boundary remains request/panel; widening RFC 3339, adding retention policy, or taking the process offline would be unjustified.

Smallest testable correction: extend the existing shared range admission/resolution authority so every admitted relative input either produces bounds satisfying the existing `AbsoluteRangeSchema` at resolution or returns the existing field-specific validation problem before executor reads. Reuse the existing schema/problem/rendering paths, retain one inherited clock, and preserve valid siblings when an explicit panel range fails. Do not weaken downstream absolute validation or widen its public date contract, introduce per-host guards, or choose an arbitrary retention cap. The implementation must reconcile the admission test at line 396 with that postcondition.

Focused closure proof: assert the schema/resolver absolute-range invariant with `100000000d` at the fixed clock above; cover initial and replacement root documents with zero reads, an invalid explicit panel plus valid sibling, and both existing endpoint harnesses. Keep ordinary six-hour/24-hour ranges, absolute ranges, explicit precedence, cancellation and exact authored SQL/binds green. No new live-server framework or release run is needed for this pure validation correction.

## Prior finding closure and proof limits

Source evidence retains closure for FIND-003-1/-2 (input-first range and shared first-position lookup), -3/-4 (signed domains and twenty cue pairs), -5 (control action states), -6 (acquisition/active/late cancellation), -7/-8 (catalog-derived problems and standard JSON import attribute), -9/-10 (typed grouping and collision-safe captions), -11 (lossless original Number admission plus shared ceilings), and -12 (kind-bearing shared canonical reference). FIND-003-14's test now documents its assertion panic conditions; no runtime behavior changed. FIND-003-13 remains proposed active as above.

Supplied candidate checks are treated as reported: UI check, unit tests 40/40, catalog check, fmt, lints, focused client nextest and real-server UI integration file 2/2. Inspected focused tests cover acquisition cancellation, active close and late-result exclusion, stale generation fencing, failed dependencies, query limits, same-stream closure/fresh-query recovery and original R6 counterexample. The new range probe above was executed without writing outside this packet. No builds, test suites, browser/human inspection, crash injection or cross-platform packaging qualification were rerun. Green tests do not establish the missing range postcondition; current tests expressly accept the counterexample.
