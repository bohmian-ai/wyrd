# Independent structured Ponytail validation

One material finding remains: **FIND-003-11**, incomplete admission of the original JavaScript collection budget. **FIND-003-12 is closed**, as are FIND-003-1 through FIND-003-10. The remaining correction is bounded implementation work under the approved specification; no specification revision is required.

## Immutable subject and independent method

Repository `/Users/stevenforrester/Documents/GitHub/wyrd-ui-main-integration`; branch `wyrd-ui-main-integration`; cumulative base `c62ebce8c4de0d43a2b9d50d360de628c49fd533` (parent of `ff4145ac6`); candidate `39c15176955edca785cc578168c67e1efcb80ea5`. HEAD independently matched at inspection and completion. Tracked source remained unchanged. This validator writes only this report; the concurrent TASK-027 packet is preserved.

Read the task-review skill, original TASK-003, approved specification revision 13, all six discovery reports, claims comparison, prior ledgers/verdict evidence and the R4 remediation. Independently inspected the cumulative diff and resulting owners, using the latest fixes only for navigation. Applied root and TS SDK AGENTS, spec-driven development, maintainer/Rust/error guidance, applicable UI guidance, canonical Card reference authority, query/terminal ownership and first-party browser-session boundaries. No CodeGraph index exists. Neither peer agreement nor supplied green tests substitutes for source validation.

Expanded the traces through WireRequest, Node/MCP adapters, public SDK collector, native next/collect/close, actual locked N-API dependency, shared admission/builder, MCP argument/schema/tool admission, existing streaming/collection owners and current integration assertions. Checked previous closure at the chart/range/plot/action/error/identity producers and sibling consumers. PR #205/main content and TASK-027 ordering/layout mechanics are excluded as findings; their shared loading/rendering seams remain inspected. No new finding is assigned to the human-directed flat identity adaptation itself.

## Proposal adjudication

| Discovery source | Independent disposition | Final cause |
|---|---|---|
| B-NUMERIC | CONFIRMED | FIND-003-11 |
| INV-003-11 | CONFIRMED; same original-value admission failure | FIND-003-11 |
| STD-003-1 | CONFIRMED for boundary validation and behavioral consequence | FIND-003-11 |
| MAINT-1 | CONFIRMED for public refusal/parity consequence, rather than style | FIND-003-11 |
| SEC-1 | CONFIRMED; no tenant escape or post-conversion cap bypass established | FIND-003-11 |
| System report's empty ledger | No additional proposal; its explicit scope is effective Rust integer budgets, not original JavaScript numbers | None |

No proposal is discarded because it has fewer votes, and none is retained solely because reviewers agree. These proposals all identify one native conversion boundary; they do not justify one correction per option, host or downstream caller. The system PASS does not contradict that diagnosis. Claims comparison reasonably declines a focused follow-up: the conversion, sibling admission and release paths are traced rather than left unresolved.

One supporting statement in MAINT-1 needs precision: an oversized positive byte budget can deserialize into MCP's `usize` on the supported 64-bit platforms, then fails shared range admission. It need not fail deserialization. Oversized row budgets beyond u32 and fractional JSON budgets fail integer deserialization. This distinction does not change the retained proposal or correction.

## Final deduplicated active ledger

### FIND-003-11 — Native conversion changes forbidden collection budgets before admission

**Status:** CONFIRMED, continuing the existing ID. **Classification:** INCORRECT (also violates native boundary-validation rules). **Discovery sources:** B-NUMERIC, INV-003-11, STD-003-1, MAINT-1, SEC-1.

**Violated obligation.** REQ-030 requires the existing bounded collection/row/JSON-byte ceilings on Node and MCP; original scenario 2 requires equivalent authorized reads with honest limits. R4 explicitly requires refusing inadmissible supplied budgets rather than clamping them. TS SDK AGENTS requires extraction and validation at the N-API boundary. Public SDK collect documentation promises `WYRD_SPEC_400_VALIDATION` for a ceiling outside its range.

**Exact changed location.** `sdks/wyrd-sdk-ts/native/src/lib.rs:1382–1403`, specifically the `Option<u32>` arguments at 1384–1385, before shared admission at 1396. Producer/callers are `sdks/wyrd-sdk-ts/wyrd/src/index.ts:545–547` and UI `packages/ui/src/lib/node.ts:30–40`; shared policy is `crates/shared/wyrd-queue/src/json_result.rs:67–82`. UI-relative paths in this report start at `crates/wyrd/wyrd-server/wyrd-ui/`.

**Producer to consumer, complete correction owner.** `contract.ts:49–55` exposes the two optional budgets as JavaScript numbers. `nodeExecutor` starts an existing SDK query stream, then forwards both options unchanged into `BifrostQueryStream.collect`. That wrapper sets `#done=true`, calls native collect without validation and projects its returned error/result. The native declaration likewise accepts JS numbers. N-API extracts each `Option<u32>` before entering the Rust body. The body takes the stream, resolves defaults, calls the correct shared range rule and then collects using `JsonResultBuilder`.

The locked dependency is `napi` 3.12.3. Its installed `src/bindgen_runtime/js_values/number.rs:45–58,68` implements u32 extraction using `napi_get_value_uint32`; the subsequent `try_into` converts u32 to u32 and therefore cannot validate the original Number. Its primitive `ValidateNapiValue` adds no integer/range check. Node's implementation calls V8 `Uint32Value` for a Number that is not already uint32, losing high bits or a fractional part before this method runs. This is independently checked against the [versioned Node primary source](https://github.com/nodejs/node/blob/v26.5.0/src/js_native_api_v8.cc#L2168-L2188), rather than inferred from the Rust argument spelling alone. The [Node-API conversion documentation](https://nodejs.org/api/n-api.html#napi_get_value_uint32) establishes that this API extracts a uint32 Number representation; it is not the application's admitted-range policy.

**Reachable counterexamples.** For the existing small authorized query, all these original finite values fit exactly enough in JS Number to demonstrate the issue:

| Original option | Value native admission sees | Required disposition of original input |
|---|---:|---|
| `max_rows: 4_294_977_296` (`2^32 + 10_000`) | 10,000 | Refuse above maximum |
| `max_bytes: 4_299_161_600` (`2^32 + 4_194_304`) | 4,194,304 | Refuse above maximum |
| `max_rows: 10_000.5` | 10,000 | Refuse fractional integer budget |
| `max_bytes: 4_194_304.5` | 4,194,304 | Refuse fractional integer budget |

Each transformed value is admitted, so an ordinary tiny query can successfully return rows. These are source-established counterexamples, **not newly executed live reproductions**. The current three-row integration query fits the resulting budgets; no artificial load is needed.

The sibling MCP adapter JSON-serializes these finite originals unchanged, calls existing `bifrost.query`, then server `QueryArguments` deserializes `max_rows:u32` / `max_bytes:usize` (`query/collect.rs:109–132`). `query_tool` parses/validates before `BoundedQuery::run`, and `QueryArguments::validate` calls shared `admit_ceilings` (`collect.rs:171`). Fractions/overflowing row values fail integer deserialization; the large byte value deserializes and fails the shared maximum. MCP therefore refuses originals that Node changes into admitted values.

Repository callers of shared admission/builder construction are native collect and server ResultCollector/QueryArguments. General SDK `next`/`collect_bounded` is adjacent Arrow streaming with different encoded-byte budgets; it is not a repair writer for the local whole-JSON options. Node sends only SQL, params and deadline into server streaming, so the server never receives these local collect options. Chart loaders currently omit budget options, but the exported executor and direct public SDK collector are expressly required reachable task surfaces. BFF/custom-app loading, direct server consumers and MCP all converge on the existing owners; there is no downstream mechanism that recovers the discarded original Number.

**Observable consequence.** Forbidden requested budgets can produce successful Node results under a silently changed budget, contrary to documented refusal and Node/MCP input-domain parity. R4 correctly caps the effective integer budget and closes the old ordinary-u32 over-allocation path. This retained finding **does not claim** that these examples exceed the effective 10,000-row/16-MiB retained-output maxima, expose another tenant or corrupt durable data.

**Selected smallest safe correction.** Correct original-value admission once at the existing native collect boundary. Preserve the incoming JavaScript Number until finite whole-number and lossless representability checks have completed; only then narrow to the integer form and apply the existing shared `admit_ceilings` policy. The installed N-API ordinary floating-point Number extraction and standard numeric checks suffice; no custom numeric abstraction or new dependency is needed. Keep the public JavaScript `number` surface and existing default/range policy. Leave the precise private signature/control flow to implementation.

Use existing `WyrdError::Validation` → `NativeQueryStep::failure` → SDK `projectedError` → Node problem projection for refusal. Retain native stream release on every refusal: the wrapper already marks itself done, so validation must not strand the owned stream behind its mutex or rely on JS garbage collection. Existing take/drop provides the release mechanism. The number guard belongs here because this boundary owns the lossy conversion and all direct/public SDK consumers; component, BFF, Node adapter or TS-wrapper-only guards would repeat or bypass it.

Preserve omitted defaults (1000 rows/4 MiB), valid smaller budgets/exact maxima, shared ceiling constants/admission, positional values/nulls/duplicates, exact JSON accounting, actual-result overflow as `QueryResultTooLarge`, terminal/clean-EOF/degraded evidence, close notification and cancellation, server settlement, ordinary streaming and MCP's existing integer contract/protocol-specific invalid-params errors. Do not clamp, duplicate range policy across hosts, lower general stream limits or introduce a transport, error family, setting, lifecycle owner or durable decision.

**Ponytail ladder.** Deleting bounded collection would remove required behavior. Current shared admission already solves integer policy; extending it into every host is unnecessary. Conventional lossless Number extraction plus native boundary validation is sufficient before reusing that policy. Standard/native mechanisms and the installed binding cover the entire correction. No new product/API/security/architecture decision is needed.

**Focused closure proof.** Extend the existing `TASK-003: Identical authorized reads` scenario at `src/lib/ui-library.integration.test.ts:186–201`. Use the same tiny authorized query, with wrapping and fractional original budgets for both fields. Assert each executor refuses independently with no usable result; SDK failures retain stable validation metadata, MCP retains its current refusal envelope. Do not require equal error wording/envelopes for deserialization versus range admission. Exercise direct native-backed SDK refusal, followed by a second collect on the same stream failing closed and a fresh ordinary query succeeding. Retain current zero/max+1, defaults/exact-max, valid-budget actual-overflow and cancellation assertions. A pure Rust u32 policy test alone cannot prove this foreign-number boundary.

The native owner must be rebuilt before that focused real-server case. Use the existing commands, not a new harness or broad aggregate:

```sh
mise run ts:build
mise exec -- scripts/postgres/with-test-postgres.sh -- bash -lc 'mise run db:migrate:inner && WYRD_UI_INTEGRATION=1 pnpm --dir crates/wyrd/wyrd-server/wyrd-ui test src/lib/ui-library.integration.test.ts -t "^TASK-003: Identical authorized reads$"'
mise exec -- pnpm --dir crates/wyrd/wyrd-server/wyrd-ui test src/lib/ui-library.test.ts -t '^Node query acquisition cancellation$'
mise exec -- pnpm --dir crates/wyrd/wyrd-server/wyrd-ui check
mise exec -- cargo clippy --locked -p wyrd-sdk-ts --lib -- -D warnings
mise run ts:napi:check
mise run fmt:check
mise exec -- git diff --check
```

Regenerate declarations through their native build owner if its output changes; never hand-edit them. No queue/server change is required by this selected correction. If source evidence makes their shared owner change necessary, also retain the exact existing regression command `mise exec -- cargo nextest run --locked -p wyrd-server --lib -E 'test(=query::collect::tests::query_result_is_positional_and_counts_exact_structured_json_bytes)'`. Do not add whole journey/release aggregates to this remediation. Record meaningful RED/GREEN crossing the real native boundary, actual selected counts and prerequisites.

## Prior finding closure from current source

| Stable ID | Independently inspected source and sibling consumers | Disposition |
|---|---|---|
| FIND-003-1 | `loaders.ts::loadChart` resolves explicit input range first, inherited context second; request and Open in Query share executed SQL/params. WyrdView resolves inherited range once; both endpoint hosts call the same loader. Existing two-panel brush/reset and fixed-clock assertions remain. | CLOSED |
| FIND-003-2 | `shapeChart.need`, timeSeries, barRows and explicit stat extraction all use first-match `columnIndex`; table iterates positional columns/cells. Conflicting duplicate descriptors and x/series/stat/table assertions remain. | CLOSED |
| FIND-003-3 | Chart bounds include zero and round signed sides outward; bar domain/endpoint geometry derives nonnegative heights and reserves negative-label room. Direct and view hosts share Chart. Mixed/all-negative/flat/zero cases remain. | CLOSED for original signed-domain diagnosis |
| FIND-003-4 | `MAX_SERIES=20`; fold only above twenty keeps nineteen plus aggregate. Four markers × five dashes produce twenty distinct pairs shared by plot/legend. Existing 5/20/21 assertions remain. | CLOSED |
| FIND-003-5 | CardHeader version/tab controls consume named ActionState, disable pending/unavailable, reset selection to host-authoritative input and show ActionNote. WyrdView validates/deduplicates actions and delivers outcomes. Direct and host-delivered refusal assertions remain. | CLOSED |
| FIND-003-6 | Node checks abort immediately after acquisition and after collection, closes acquired/active streams through SDK return, and removes the listener. Native close notifies before locking; WyrdView fences stale generation results, both endpoint hosts forward signal. Focused acquisition/active/pre-aborted assertions remain. | CLOSED |
| FIND-003-7 | Rust-generated problems flow through catalog generator and `wyrdProblem`; local loader/view/MCP/WyrdView producers reuse them. Both endpoints serialize complete canonical problems; upstream code/title and denied mapping remain. Canonical-problem assertions remain. | CLOSED |
| FIND-003-8 | Contract uses standard JSON import attribute; public loaders/MCP import it. Existing root test script runs actual native public-entry imports before transformed Vitest. | CLOSED by source and supplied checks, no renewed import run claimed |
| FIND-003-9 | `timeSeries:60–66` chooses the synthetic aggregate label against final kept authored captions. Plot/legend/tooltip consume that same unique producer output; authored other/qualification/rerender assertions remain. | CLOSED |
| FIND-003-10 | `timeSeries:44` keys typed `[raw group,y]` before summation; `captions:81–92` qualifies collisions. Null/text-null and multi-y tuples remain distinct; bar/stat/table use their own positional non-grouping projections. Existing four-group, nullable, qualification-like and rerender assertions remain. | CLOSED |
| FIND-003-11 | Shared admission now covers effective integers, both native/server owners call it and native takes/drops on refusal. Original JS Number is still lost first at native arguments. | ACTIVE, as validated above; not a new ID |
| FIND-003-12 | `format.ts:51–53` emits `Kind/name@version[#uid]`, preserving supplied optional UID and omitting null/absent UID; CardRefChip text/title and CardHeader metadata/copy reuse it. Header builds changed-version refs without stale UID. Copy tests distinguish Model/Data and retain Copy uid/failure and version-action checks. | CLOSED |

The header need not synthesize a UID into every reference: canonical identity permits its absence. Retaining that behavior is consistent with R4 and the owning CardRef contract. Formatting remains one producer, with no new lookup/parser/alias.

## Cumulative evidence reconciliation and limits

The portable package/schema/catalog/loaders/renderer, six authorized reads, shared positional JSON projection, request-selected Rust session, BFF/custom-app/AppBridge consumers and production build owner remain the expressly approved mechanisms. The new pure builder replaces the prior MCP JSON encoder/accounting rather than duplicating a query service. Existing Arrow `collect_bounded` retains batches and transport-byte budgets, so it does not supply the required positional JSON-byte projection. Browser-session Bifrost reuses the existing query-only sink/constructor shape; ordinary credential/ingest ownership remains with Rust. No additional retained discovery finding, new authority conflict or unresolved source path was established.

The caller supplies green candidate fmt/lints/codegen/N-API/TS-unit/queue checks, UI check (package/app zero errors), test 40/40, build and real-server integration 2/2. Current source shows those budget assertions only use exactly represented ordinary u32 inputs; that green result cannot disprove the original-number counterexamples. Source tracing, dependency reads and versioned primary-source verification are this validator's fresh evidence. No build, generator, test or live-server execution was performed by this validator. Historical Darwin bundle/import/human approval evidence stays historical; Linux/images, live degraded parity and renewed deployed-browser/human inspection are not newly proven.

Skipped: implementation, build/test reruns and fresh platform/browser/live qualification under the packet-only write constraint. Risk: forbidden original numeric budgets can still succeed after conversion; effective retained-output caps remain enforced.
