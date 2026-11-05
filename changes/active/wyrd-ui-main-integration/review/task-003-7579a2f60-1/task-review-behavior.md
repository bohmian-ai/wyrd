# TASK-003 independent behavior review

Overall result: **FAIL**. Two source-backed proposals require independent validation. FIND-003-1 through FIND-003-10 are closed at their diagnosed causes; the authored-series remediation is present and preserves its sibling behavior.

## Immutable subject and method

Repository `/Users/stevenforrester/Documents/GitHub/wyrd-ui-main-integration`, branch `wyrd-ui-main-integration`; cumulative base `c62ebce8c4de0d43a2b9d50d360de628c49fd533`, candidate `7579a2f603c023daae0d15b9d5cc3013eec6f00d`. HEAD still matches. This reviewer writes only this report and did not read peer discovery reports.

Applied original TASK-003, approved specification revision 13, root AGENTS.md, the applicable TypeScript SDK AGENTS.md, spec-driven-development, maintainer guidance, architecture authority map, CardRef contract in `architecture/wyrd-design.md:2159`, Rust-owned browser-session rules in `architecture/wyrd-security-posture.md:282`, Bifrost query authority, and the repository UI boundary guidance. No CodeGraph directory exists. TASK-027 implementation is separately attributed; only its foundation loading/rendering seams were considered. PR #205 content is not independently assessed as TASK-003 work.

Independently ran `git range-diff 695ca914d^..wyrd-ui-pre-205 ff4145ac6^..7579a2f60`: the foundation implementation and R1/R2 patches match. The native packaging patch differs only in upstream `image()` versus `bundle_in()` context; packet-only differences resolve relocated authorities. New flat-identity and authored-series changes were inspected directly. None of the unrelated pre-205 upstream commits is a proposal here.

Expanded the shared map through: catalog/schema → `validatePanel`/loaders → Node/MCP/remote adapters → SDK/native bounded collection and shared positional JSON; exact Card reads → flat dependent version listing → CardHeader controls/copy; view validation → concurrent panel loading/cancellation → typed host intents; chart bindings → first-match descriptors → typed series groups → geometry/legend/tooltip; package exports, native JSON imports, generated catalog and production build closure; gallery, direct example, view example and built AppBridge consumers. Inspected the original task and preceding findings/remediation as hypotheses, not conclusions.

## Acceptance matrix

UI-relative source paths start at `crates/wyrd/wyrd-server/wyrd-ui/`. Results identify source behavior; supplied execution is distinguished below.

| Obligation | Implementation evidence | Verification evidence | Result |
|---|---|---|---|
| Publishable package and approved components/loaders/view/catalog/node/mcp exports | `packages/ui/package.json`; `index.ts`, `view.ts`, workspace links | Candidate-supplied check/build; native import smoke in root test script | PASS |
| Browser exports contain no native Node SDK runtime import | Type-only wire imports in contract/schemas; runtime SDK import confined to `node.ts`; App entry uses MCP executor | Built AppBridge journey; candidate build | PASS |
| Generated catalog is the shared input/action authority | `scripts/catalog.ts` derives from `schemas.ts`; `validatePanel`/`validateIntent`; `mise.toml` codegen owner | Existing canonical/catalog/schema assertions; historical codegen evidence | PASS |
| No invented registry, query or authorization owner | Loaders call closed domain union; Node wraps Cards/Bifrost/Execution; MCP forwards existing tools; shared Rust JSON projection | Live six-read journey and source tracing | PASS |
| Exact Card identity and no space compatibility | Strict flat CardRefSchema; `loadCardHeader` reads exact ref and lists resolved kind/name; MCP maps to current `cards.get` envelope | Flat-identity negative `space` assertion; real SDK/MCP and BFF flat reads supplied green | PASS |
| Copied identity faithfully represents the exact Card | `fmtCardRef` and CardHeader Copy ref | Current assertion expects name/version without kind; B-COPY below | FAIL |
| Failed/denied prerequisite never issues dependent read | `loadCardHeader` returns before listing; signal checked before each next page | Dependent loads scenario | PASS |
| Header version paging, truncation and list failure remain honest | Five bounded pages, cursor forwarding, separate versions Loaded state | Paged/truncated and denied-version tests | PASS |
| Header actions preserve authoritative selection and visible result | CardHeader disables pending/unavailable, resets chosen version, uses ActionNote; WyrdView stores outcomes | Direct and WyrdView pending/refusal/error/success tests | PASS |
| Authored SQL preserved with typed range/bucket binds and exact Open in Query | `chartParams`, `loadChart.executed`, Chart intent | Existing exact-SQL/binds assertions and AppBridge request observation | PASS |
| Panel range overrides inherited range; inherited relative range resolved once | `loadChart` selects input.range first; WyrdView resolves common range before loads | Two-panel brush/reset and clock-pinned explicit relative range assertions | PASS |
| Invalid view/panel input produces useful isolated error | ViewDocumentSchema, validatePanel/inputProblem, independent panel map | Valid/invalid sibling, version/catalog and malformed input tests; AppBridge invalid panel | PASS |
| Changing/removing/leaving panel supersedes results and stops dependent reads | WyrdView abortAll/settle controller membership; loadCardHeader pre-page abort | Held-load stale result, removed sibling and unmount tests | PASS |
| Supported query cancellation propagates, including stream acquisition race | Node closes acquired already-aborted stream, installs active listener, checks post-collect; native close notifies running collector; MCP forwards signal | Query-acquisition/active/pre-abort assertions; supplied live pre-abort parity | PASS |
| Six approved reads return equivalent normalized success/error results | Node switch and MCP mapping; current Cards tool envelope adapter | Supplied live six-read, malformed bind, denied and limits cases | PASS for exercised valid requests; upper ceiling domain fails below |
| Current row/JSON-byte ceilings retained on both paths | Shared JsonResultBuilder charges rows and exact bytes, but native collect accepts ceilings beyond MCP maximum | Existing tests only exercise smaller valid ceilings; B-LIMIT below | FAIL |
| Positional duplicate names/nulls and existing supported encodings survive | Shared `json_result.rs` Arrow encoder projection; SDK collect parses same JSON | Live duplicate/null case; existing Rust positional/byte regression and encoding evidence | PASS |
| Terminal-only complete result; failed/malformed/denied read yields no usable partial result | Native collect waits next_batch completion and validated terminal; shared builder finish; MCP BoundedQuery/decoder | Shared-owner source, historical focused Rust evidence; live supplied negative flows | PASS |
| Degraded result preserves warnings and source evidence | Shared finish projects outcome/warnings/source_completion; Chart shows partial notice rather than empty success | Existing degraded component fixture; live degraded parity remains unverified | PASS source behavior; limited live proof |
| Complete Chart types and truthful schema mismatch handling | shapeChart first descriptor checking; timeseries/bar/stat/table branches | Validated portable rendering scenario | PASS |
| Signed chart values remain finite and readable | Chart signed zero-including domains and nonnegative bar geometry | Mixed/all-negative/flat/zero/positive cases | PASS |
| Twenty distinct visual series before folding and correct overflow sum | `MAX_SERIES`, nineteen-plus-one fold only above twenty; marker/dash product | Existing 5/20/21 and exact aggregate total assertions | PASS |
| Authored series identity survives grouping and captions remain attributable | JSON-serialized typed `[raw group,y]` identity; captions retain raw group and qualify only collisions; shared render labels | New mounted four-tuple values 1/10/2/20, nullable-vs-string/null-like caption and rerender assertions | PASS |
| Primitives render from props with local platform interactions | EmptyState, CardRefChip, Status/VerdictPill, Sparkline, DurationBar, native-dialog Drawer and shared ActionButton/ActionNote | Existing component scenario; recorded human checkpoint | PASS |
| Session-bound BFF retains Rust authorization and tenant isolation | Tenant route receives selected session; calls shared SDK session client; query-only browser-session Bifrost shape; no browser bearer | Supplied real selected-session, foreign Card, malformed input and secret absence tests | PASS |
| Direct and view-document custom app, gallery and actual AppBridge artifact are usable | Example SSR loaders and server endpoint; gallery fixture owner; App main mountView/MCP executor | Candidate build and real-server/AppBridge 2/2 supplied; human checkpoint recorded | PASS |
| Production package closure stays with scripts/build.sh | Existing bundle function builds native SDK in selected profile, packs/install SDK and copies native addon; import smoke | Historical darwin-arm64 bundle evidence; no fresh Linux/image run | PASS source closure; platform proof limited |
| INV-001–007 and task non-goals preserved | Generated wire types, no space alias, no write dispatcher, Ask/A2UI/persistence owner, replacement query service or production test config | Cumulative attribution/source inspection; historical gates | PASS except the two concrete proposals |
| Human individual visual checkpoint and source/reference provenance | Original task records named human approval 2026-10-20 covering components/gallery/example/AppBridge and applied rework; packet retains reference links | Recorded approval, not a new human inspection | PASS historical checkpoint; no new visual claim |

## Prior-finding closure

| ID | Current source and proof assessed | Disposition |
|---|---|---|
| FIND-003-1 | `loaders.ts::loadChart` chooses explicit panel range; all hosts converge on loader; exact executed params retained | CLOSED |
| FIND-003-2 | `columnIndex` first-match selection shared by schema validation/timeSeries/bar/stat, table stays positional | CLOSED |
| FIND-003-3 | Chart signed domains and bar endpoints preserve values below zero, with reserved negative label space | CLOSED |
| FIND-003-4 | `MAX_SERIES=20`; fold only when >20; marker × dash combinations supply twenty distinct cues | CLOSED |
| FIND-003-5 | CardHeader version/tab pending/unavailable controls, authoritative selection reset and shared ActionNote; WyrdView returned outcome | CLOSED |
| FIND-003-6 | Node pre-acquisition abort closes stream after acquisition, active abort calls return, post-collect abort cannot succeed | CLOSED |
| FIND-003-7 | Canonical generated problems at local producers and full problem serialization at both endpoints | CLOSED |
| FIND-003-8 | `contract.ts` JSON import attribute; native import smoke loads public loaders and MCP before Vitest | CLOSED |
| FIND-003-9 | Synthetic other caption selected against actual authored emitted captions; suffix avoids authored qualification collisions | CLOSED |
| FIND-003-10 | Typed pair key precedes accumulation; group/y data retained for injective qualification; all plot/legend/tooltip/accessibility consumers use common captions | CLOSED |

Flat-identity adaptation is human-directed, not a retroactively assigned prior finding. Strict schema/catalog/list request and fixture adaptation are complete. The AppBridge structuredClone harness adjustment has an explicit recorded symptom, traced cause, independent diagnosis and selected test-realm correction. It does not weaken production behavior or replace the browser engine. The new copied-reference issue below concerns the resulting task-owned formatter against current architecture, rather than PR #205's server implementation.

## Proposed finding B-LIMIT — Node bounded collection bypasses the existing maximum ceilings

Classification: **INCORRECT**. Violates REQ-030 query results: keep existing row/JSON-byte ceilings on both paths, and Scenario 2's equivalent six-read behavior. Proposed next stable finding allocation belongs to independent validation, after FIND-003-10.

Location: `sdks/wyrd-sdk-ts/native/src/lib.rs:1377–1391`, through `wyrd-ui/packages/ui/src/lib/node.ts:39` and SDK `BifrostQueryStream.collect` at `sdks/wyrd-sdk-ts/wyrd/src/index.ts:546`.

Producer-to-result evidence: WireRequest publicly permits `max_rows` and `max_bytes`; nodeExecutor forwards both into SDK collect unchanged. The wrapper forwards them into NativeBifrostQueryStream.collect, which constructs JsonResultBuilder with the requested values directly. JsonResultBuilder::new simply stores any usize and charges against it; neither constructor nor native caller imports/enforces MAX_ROWS_CEILING or MAX_BYTES_CEILING. MCP's sibling path deserializes the same request and runs QueryArguments::validate (`query/collect.rs:168–181`), refusing zero, >10,000 rows and >16 MiB before BoundedQuery. These constants and validation existed at cumulative base and were moved/shared by the task; they are not new PR #205 expectations.

Concrete reachable distinction: an ordinary authorized `bifrost.query` WireRequest with `sql: 'SELECT 1'` and `max_rows: 10001` returns one successful row through Node; the same request through MCP is an invalid-arguments failure. Likewise max_bytes 16 MiB + 1. If the query actually produces more rows/bytes than the existing bounded ceiling but below the chosen larger value, Node retains them; the shared builder only checks the larger selected number. This is a public exported executor and SDK path explicitly required by the task, not an unreachable Chart option. Existing tests use max_rows 2 and max_bytes 64 and therefore do not exercise the missing upper refusal.

Existing owner and sibling tracing: shared JsonResultBuilder is the complete result projection owner used by both native collect and server ResultCollector. MCP's QueryArguments owns protocol parsing. Server Oracle/query service remains the owner of SQL/auth/stream limits and cannot enforce this smaller client-side JSON result ceiling, because the Node request only sends sql/params/deadline. No renderer, loader or BFF can recover a consistent limit after the SDK collector has retained too much.

Smallest correction boundary: enforce the existing shared bounded-result limit contract at SDK/native bounded collection, reusing shared constants and canonical refusal mechanism, while retaining MCP argument refusal and all server query/cancellation/settlement behavior. Do not clamp an invalid request into success, widen the MCP limits, change streaming iteration, or duplicate guards in each component/host. If shared validation is extracted, use this existing bounded-result owner and both actual callers; no new transport or settings. Require equivalent refusal for upper/zero ceiling inputs and unchanged success at accepted ceilings. Focused proof can extend the existing bounded SDK/MCP read proof with the just-above-maximum requests and a direct SDK bound check, preserving current small valid ceiling overflow cases. This was source-traced, not freshly executed against Postgres in this review.

## Proposed finding B-COPY — Copy ref drops the Card kind from canonical identity

Classification: **INCORRECT**. Violates original task's copied exact identity, INV-001 current contracts, and `architecture/wyrd-design.md:2159–2163`, which fixes canonical text as `Kind/name@version[#uid]`.

Location: `wyrd-ui/packages/ui/src/lib/format.ts:51–53`, consumed by `components/CardHeader.svelte:103` (Copy ref), header identity line at line 84 and CardRefChip text/title. This formatter was materially changed by flat-identity remediation.

Producer-to-consumer: CardHeader has the server Card kind and resolved name and constructs its ref with kind/name/version; fmtCardRef's parameter then discards kind by type and returns only name@version. Copy ref sends that reduced text directly to navigator.clipboard. CardRefChip separately displays kind visually, but its title/ref text uses the same reduced formatter. A Model named ranker@1.4.2 and a Data named ranker@1.4.2 therefore produce indistinguishable copied references. The clipboard no longer carries enough identity to select the exact Card; the neighboring kind badge does not enter the clipboard.

Smallest correction boundary: reuse the same formatter for the flat canonical CardRef text, carrying kind/name/version and optional uid according to the existing canonical contract. Keep optional UID semantics, the separate copy-UID action, flat route/schema refusal, version-selection pinning and host action behavior. No parser, compatibility alias, lookup or route is needed. Update existing CardHeader clipboard and CardRefChip text/title assertions to prove two same-name/version different-kind refs remain distinct and canonical. No fresh live journey is necessary for this pure formatter/component correction.

## Evidence and limits

Candidate-green check (package/app zero errors), test (40/40), build, and managed-Postgres integration (2/2) are **user-supplied results**, not this reviewer's executions. Historical task evidence supplies focused Rust normalization/MCP checks, codegen/typing/packing, darwin native bundle and human visual approval. No claim of fresh Linux/image/degraded-live parity or browser/human rerun is made. The proposed issues are outside current asserted input cases, so green tests do not refute their source paths. No source/test/runtime artifact was written or modified by this reviewer.

Skipped: fresh tests/builds, live server and browser execution, Linux/image runs. Risk: until independently adjudicated, the two proposals identify bounded-result host divergence and ambiguous copied Card identity.
