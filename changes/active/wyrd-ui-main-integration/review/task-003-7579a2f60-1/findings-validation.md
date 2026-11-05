# Independent structured Ponytail validation

Two material findings are retained: **FIND-003-11** and **FIND-003-12**. FIND-003-1 through FIND-003-10 remain closed at their diagnosed causes. Neither correction needs a specification revision.

## Subject and method

Repository `/Users/stevenforrester/Documents/GitHub/wyrd-ui-main-integration`; cumulative base `c62ebce8c4de0d43a2b9d50d360de628c49fd533` (parent of `ff4145ac6`); immutable candidate `7579a2f603c023daae0d15b9d5cc3013eec6f00d`. HEAD independently matched at inspection and completion. Only this report was written; no tests, builds, generators, source/test edits, commits or pushes were performed.

Read the subject, original TASK-003, specification revision 13, all six discovery reports, claims comparison, focused follow-up, prior validated ledgers and remediation instructions. Applied root and TS SDK AGENTS, task-review instructions, spec-driven workflow, maintainer guidance, applicable UI guidance, canonical Card references and existing query/security boundaries. No CodeGraph index exists. Independently inspected cumulative diffs and current source through the proposed producer/caller/sibling correction boundaries, rather than accepting report agreement as proof.

Fresh `git range-diff 695ca914d^..wyrd-ui-pre-205 ff4145ac6^..7579a2f60` confirms matching reviewed foundation/remediation implementation patches. The packaging difference is surrounding `image`/`bundle_in` context; other differing reviewed packet patches concern relocated authority links. Post-rebase flat-identity and authored-series changes were inspected separately. PR #205 content and TASK-027 implementations are excluded as findings; shared renderer/load seams remain relevant. The human-directed identity adaptation is not retroactively assigned an ID: the new finding concerns its independently established implementation omission.

The portable loader/renderer is expressly approved. Cards/Bifrost/Execution, Rust session authorization, query streaming/settlement, shared positional projection, generators and the production bundle remain existing owners. No proposal warrants another registry, query service, transport, credential workflow, lifecycle owner, settings surface or plotting framework.

## Claim adjudication

| Discovery source | Independent disposition | Final cause |
|---|---|---|
| B-LIMIT | CONFIRMED; correction refined below | FIND-003-11 |
| INV-003-BOUNDS | CONFIRMED; same producer invariant | FIND-003-11 |
| MNT-003-11 | CONFIRMED for its behavioral consequence, rather than style | FIND-003-11 |
| SYS-003-1 | CONFIRMED; no service-wide failure policy required | FIND-003-11 |
| B-COPY | CONFIRMED; optional UID correction constrained below | FIND-003-12 |
| INV-003-FLAT-REF | CONFIRMED; same formatter cause | FIND-003-12 |
| Standards/security empty proposed ledgers | No additional finding. Their scoped passing claims do not establish requested-option admission or canonical clipboard spelling. | None |

The focused follow-up resolves the apparent conflict correctly: actual-result accounting differs from allowed requested budgets, and structured read identity differs from copied canonical text. No unresolved claim requires another discovery pass. Recommendations are revised to preserve protocol-specific errors, native stream release and version-specific UID semantics. No proposal is rejected or converted into optional advice.

## Final deduplicated ledger

### FIND-003-11 — Native bounded collection admits budgets above the existing ceilings

**Status:** CONFIRMED. **Classification:** INCORRECT. **Discovery:** B-LIMIT, INV-003-BOUNDS, MNT-003-11, SYS-003-1.

**Obligation:** REQ-030 query results retain the current row/JSON-byte ceilings on both paths; original TASK-003 scenario 2 requires equivalent authorized bounded reads and honest limits. The existing allowed domains are 1..10,000 rows and 1..16,777,216 bytes. These constants and MCP admission existed before the task's extraction, so this is attributable task behavior rather than unrelated PR #205 policy.

**Exact source:** `sdks/wyrd-sdk-ts/native/src/lib.rs:1377–1392`, specifically the unchecked `JsonResultBuilder::new` arguments; `crates/shared/wyrd-queue/src/json_result.rs:69–79` stores them without admission. All UI-relative paths below start at `crates/wyrd/wyrd-server/wyrd-ui/`.

**Producer → consumer and sibling evidence:** public `contract.ts::WireRequest` exposes `max_rows`/`max_bytes`; `node.ts::nodeExecutor` acquires the SDK stream then forwards these to `BifrostQueryStream.collect` (`sdks/wyrd-sdk-ts/wyrd/src/index.ts:544–552`). That wrapper sets `#done` and calls native collect unchanged. Native takes the owned stream out of its mutex slot, resolves omitted defaults and constructs the builder with any supplied u32 values. Builder `accept_batch`/`charge_bytes` compare against those stored values, not `MAX_*_CEILING`. There is no later writer or consumer that restores the required cap.

The complete sibling path is `mcpExecutor` → existing `bifrost.query` → `QueryArguments::validate` (`server/src/query/collect.rs:171–182`) → `BoundedQuery` → `ResultCollector::consume` → the same builder. MCP validates before execution (`server/src/mcp/bifrost.rs:236–243`). Repository search finds exactly two builder-construction callers: this server collector and native collect. General SDK streaming uses its existing `next` path and has different, larger engine limits. Node sends SQL/params/deadline to that service; local whole-JSON options are not part of `BifrostQueryRequest`, so server stream admission cannot enforce this missing collection contract. Current Chart loaders choose defaults, but the exported six-read executor and SDK collector are expressly required reachable surfaces.

**Reachable consequence:** an otherwise valid tiny authorized query with `max_rows:10001` or `max_bytes:16777217` can succeed through Node and is refused by MCP. Both numbers fit u32; no questionable N-API conversion assumption is needed. A larger result can be retained beyond the approved collected maximum while remaining below the caller's larger chosen budget. Zero is also outside the domain: an empty result with zero rows can evade actual row-overflow accounting, and zero bytes produces a result-size error rather than proper input admission. The existing integration cases at `ui-library.integration.test.ts:180` use valid smaller budgets (2 rows/64 bytes); their green result does not exercise this distinction. This validation is source evidence, not a fresh live reproduction.

**Selected correction boundary:** put the common allowed-budget rule beside the existing shared `JsonResultBuilder`/ceiling constants and use that rule for native bounded collection and the server's existing argument admission. Do not leave independently evolving copies of the range policy. Refuse out-of-range inputs; do not clamp them. Reuse `WyrdError::Validation` for native/SDK request refusal and `NativeQueryStep::failure` → existing SDK `projectedError` → Node problem projection. Preserve MCP's current `InvalidQueryArguments` reason/details and JSON-RPC invalid-params mapping, including its advertised schema; matching protocol envelopes is not an additional requirement. The shared pure owner may expose admission without owning streams, SQL, permissions or MCP types. Private method/constructor shape remains implementation-owned.

Native refusal must retain the current ownership guarantee: `collect` closes the stream either way. Its current `take` makes local stream drop release response-body consumption; `close` notifies a running collector before locking. A newly introduced validation return must not leave a stream retained behind the mutex after the wrapper has already set `#done=true`. Keep rejection local to this collection/request. Preserve acquired-after-abort closure, active cancellation, defaults (1000/4 MiB), valid smaller budgets, exact compact-JSON accounting, `QueryResultTooLarge` for actual result overflow, positional values, validated terminal/EOF, degraded evidence, and existing server cancellation/settlement. Do not move or duplicate adjacent authority, add per-component/host guards, lower general streaming limits, or create new error codes/dependencies.

Deletion cannot satisfy the required bounded Node read. Existing shared constants, MCP admission and native error projection supply the mechanism; no new policy decision is needed. Centralizing this small invariant is sufficient; stronger collection machinery or retries are not.

**Focused closure proof:** extend the existing `TASK-003: Identical authorized reads` scenario with a tiny query and independently supplied row/byte options: zero, maximum+1, exact maximum and omitted defaults. For invalid options, assert refusal and absence of usable rows through each executor; inspect its existing protocol-appropriate error rather than forcing exact equality through the current `both()` helper. Also exercise direct SDK collect in this existing Node test owner and verify rejection leaves the stream closed (a subsequent native-backed collection is refused), followed by ordinary successful querying. Preserve valid smaller-budget overflow and existing cancellation cases. No large allocation/load fixture is required. Retain the focused shared positional/byte regression when the shared owner changes.

### FIND-003-12 — Shared Card reference formatting omits kind

**Status:** CONFIRMED. **Classification:** INCORRECT. **Discovery:** B-COPY, INV-003-FLAT-REF.

**Obligation:** original TASK-003 requires preserved copied identity; spec revision 13 explicitly says UI identity renders `Kind/name@version`; INV-001 defers to current contracts. `architecture/wyrd-design.md:2159–2164` and `crates/wyrd-spec/src/reference.rs:556–605` define canonical text `Kind/name@version[#uid]` and its parser.

**Exact source:** `packages/ui/src/lib/format.ts:51–53`; clipboard consumer `components/CardHeader.svelte:103`; sibling header metadata at line 84 and `CardRefChip.svelte:9–11`.

**Producer → consumer and sibling evidence:** `CardRefSchema` and both exact read adapters retain kind/name/version. `loadCardHeader` reads the exact reference, then lists the returned kind/name. A successful CardHeader constructs `ref` with kind/name/selected exact version (`CardHeader.svelte:29–31`), but `fmtCardRef` accepts only name/version and emits only `name@version`. Header `copy` writes precisely that supplied string. CardRefChip receives the full CardRef, shows kind separately and calls the same reduced formatter for text/title. Repository search finds no other formatter caller or clipboard repair. Direct components, gallery, example, WyrdView and AppBridge share these components. The public barrel also exports this formatter.

**Observable consequence:** Model/ranker@1.4.2 and Data/ranker@1.4.2 yield identical `ranker@1.4.2` clipboard strings, which omit a required identity dimension and cannot round-trip through canonical CardRef parsing. A nearby kind badge does not enter the clipboard. Current component tests check reduced text and a failed Copy ref operation, rather than successful canonical reference copying. Authorized wire reads remain correct; the correction is not a tenancy or route redesign.

**Selected correction boundary:** fix the existing shared formatter to project the generated flat CardRef fields in canonical spelling; every current header/chip consumer then reuses it. Preserve an optional UID when one is actually supplied to that formatter, omitting a null/absent UID as canonical Display does. This does **not** require attaching a UID to every CardHeader reference: its current kind/name/version ref is valid. Keep separate Copy uid behavior. In particular, do not retain an old version's UID when emitting a newly selected version reference. No lookup, parser, compatibility spelling, route, new reference type or schema/catalog change is required. The formatter is the invalid text producer, so downstream clipboard-specific guards are unnecessary.

**Focused closure proof:** extend `TASK-003: Validated portable rendering` with successful Copy ref assertions and chip text/title assertions for two kinds sharing name/version. Assert `Kind/name@version`, plus formatter coverage for present versus null/absent UID. Retain current copy-failure feedback and version-action behavior; changing version must not carry a stale UID. This pure component/formatter correction needs no fresh four-host journey or identity service test.

## Prior-finding closure in current source

| Stable ID | Independent source and sibling check | Disposition |
|---|---|---|
| FIND-003-1 | `loadChart` selects explicit input range before inherited context; request/executed binds share params across direct/remote hosts. | CLOSED |
| FIND-003-2 | `shapeChart`, timeseries/bar and explicit stat use first-match `columnIndex`; table stays positional. | CLOSED |
| FIND-003-3 | Chart signed line domains include zero; bar endpoints produce nonnegative extents and reserve negative-label room. | CLOSED |
| FIND-003-4 | `MAX_SERIES=20`; folding keeps nineteen+aggregate only above twenty; four markers × five dashes distinguish returned lines and legend. | CLOSED |
| FIND-003-5 | CardHeader version/tab controls read named states, disable pending/unavailable, restore host selection and show ActionNote; WyrdView returns outcomes. | CLOSED |
| FIND-003-6 | Node closes an acquired already-aborted stream before collect, closes active work and refuses late success; SDK return/native close remain lifecycle owners. | CLOSED |
| FIND-003-7 | Generated canonical problems feed local producers; both panel endpoints serialize the complete problem; upstream denied/error identity remains. | CLOSED |
| FIND-003-8 | Standard JSON import attribute remains in contract; root test script performs native public loader/MCP import smoke before Vitest. | CLOSED |
| FIND-003-9 | Synthetic aggregate caption is selected against final kept authored captions, including qualification-like text; exact fold totals remain. | CLOSED |
| FIND-003-10 | `timeSeries` keys typed JSON `[raw group,y]` before summation; captions qualify collisions without merging typed tuples. Chart plot/legend/tooltip/accessible names consume the same output; barRows does not group by caption. Mounted four-tuple 1/10/2/20, null/text-null, qualification and rerender assertions remain. | CLOSED |

These are source-backed closures of the diagnosed causes, not claims of fresh execution. FIND-003-11 concerns previously untested option admission; FIND-003-12 concerns canonical text after the human-directed rebase adaptation. Neither reopens the prior chart/cancellation/error findings.

## Narrow proof commands and verification limits

After remediation, rebuild the existing native SDK before exercising its changed collector (`mise run ts:build`). Run the selected existing titles, not a broad journey aggregate:

```sh
mise exec -- pnpm --dir crates/wyrd/wyrd-server/wyrd-ui test src/lib/ui-library.test.ts -t '^(TASK-003: Validated portable rendering|Node query acquisition cancellation)$'
mise exec -- scripts/postgres/with-test-postgres.sh -- bash -lc 'mise run db:migrate:inner && WYRD_UI_INTEGRATION=1 pnpm --dir crates/wyrd/wyrd-server/wyrd-ui test src/lib/ui-library.integration.test.ts -t "^TASK-003: Identical authorized reads$"'
mise exec -- cargo nextest run --locked -p wyrd-server --lib -E 'test(=query::collect::tests::query_result_is_positional_and_counts_exact_structured_json_bytes)'
mise exec -- pnpm --dir crates/wyrd/wyrd-server/wyrd-ui check
mise exec -- cargo clippy --locked -p wyrd-queue -p wyrd-sdk-ts -p wyrd-server --lib -- -D warnings
mise run fmt:check
mise exec -- git diff --check
```

The native SDK build regenerates declarations through the existing owner; commit generated changes only if the public signature/docs actually change. No codegen/catalog/dependency/packaging change is required by these recommendations. No new Rust test title is prescribed; if the implementer adds a focused shared admission unit test, record and run its exact nextest selector as well.

User-supplied candidate check (package/app zero errors), test 40/40, build and managed-Postgres integration 2/2 are credible evidence for their asserted cases, not fresh validator runs and not proof of the missing cases. Earlier Darwin bundle/native/generated checks and human visual approval remain historical. Linux/images, a fresh human/browser pass and live degraded-terminal parity were not repeated or inferred. The full cumulative acceptance assessment remains with the orchestrator using the independent reports and this validated ledger.

Skipped: implementation, test/build execution and duplicate broad journeys. Risk: admitted oversized collection budgets and ambiguous copied references remain at this candidate; broader platform/live-proof limits remain explicit.
