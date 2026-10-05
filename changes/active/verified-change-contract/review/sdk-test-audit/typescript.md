# TypeScript SDK test audit

Scope: `sdks/wyrd-sdk-ts/wyrd/tests/**` (unit, integration, support, fixtures).
Public surface: `sdks/wyrd-sdk-ts/wyrd/src/index.ts` (package `@wyrd/sdk`) over
`index.d.ts`. Harness surface: `sdks/wyrd-sdk-ts/testing/index.d.ts` (`@wyrd/testing`).
All paths below are relative to `sdks/wyrd-sdk-ts/wyrd/tests/` unless they say otherwise.

## Summary
- Files audited: 20 (18 test files, 1 support module, 1 Parquet fixture); test functions: 63 (35 unit, 28 integration).
- Verdict counts: KEEP 9, TIGHTEN 26, REWRITE 13, MOVE 5, DELETE 10.
- Every test imports `@wyrd/sdk` from the public package, except one private `../../index.cjs` import (`unit/gateway.test.ts:7`). The admin, Gateway, OTel export, and Operator-connection journeys are close to the target. The verification and observation journeys are the main problem. Five tests drive the `Verification` handle that REQ-189 removes. Every Card graph is built in code from YAML template strings, with sha256 digests computed inline. Several tests are 200 to 250 line stories that poll `server.verificationRuns()`, `getRun`, and baseline status, and they assert SPC/PSI engine statistics parsed out of `details` JSON. The unit tier spends most of its effort checking native-ABI argument tuples through fakes. It also has type-only checks that pass at runtime by asserting on literals they just built. Every file re-declares the same `rejection` helper and connection boilerplate. The biggest API gaps: no way to wait until written rows are queryable, no way to wait for baseline readiness, no typed Card UID on a Run view, no token or OTLP-metadata helpers, and no typed read of verification results.

## API gaps revealed by the tests
| Gap | Evidence (file:line) | What a user should be able to write instead |
|---|---|---|
| No way to wait for written rows to become queryable; tests call harness barriers | `integration/bifrost-write.test.ts:111,180,278,343`; `integration/observe-run.test.ts:288,451`; `integration/drift-verification.test.ts:311,333,1002,1013`; `integration/oracle-query.test.ts:34,101,510`; `integration/otel-export.test.ts:60` | `await state.shutdown()` followed by a read that is guaranteed to see acknowledged rows, or `await bifrost.flush({ untilQueryable: true })`. Alternatively, document eventual visibility and give the SDK a bounded `await bifrost.waitForRows(sql, n)` |
| No way to wait for a Drift Verifier baseline to become ready; tests poll `cards.get(...).status.verification.baseline.state` | `integration/drift-verification.test.ts:273-285`; `integration/verification-execute.test.ts:172-178` | `await cards.waitUntilReady(verifierRef, { timeoutMs })`, or `observe.verify` refusing with a typed `BASELINE_NOT_READY` that has a retry hint |
| A Run view has no typed subject identity; tests split the string `cardRef` on `#` | `integration/observe-run.test.ts:209-210`; `integration/drift-verification.test.ts:985-986` | `run.subject.uid` / `run.cardRef` returned as a `CardRef` |
| No CardRef formatter; tests hand-build `space/Kind/name@version` strings | `integration/observe-run.test.ts:186,415`; `integration/drift-verification.test.ts:267,944`; `integration/verification-run.test.ts:346`; `integration/verification-execute.test.ts:303` | `formatCardRef(ref)` / `cards.get({ kind: "Verifier", name, version })` (already typed) used everywhere |
| No access-token accessor; tests `fetch('/auth/token')` by hand | `integration/bifrost-write.test.ts:37-48`; `integration/gateway-inference.test.ts:107-115` | `const token = await client.accessToken()` to hand to the OpenAI SDK or to a downstream service |
| No OTLP exporter auth helper; tests hand-set the `x-wyrd-access-token` gRPC metadata | `integration/otel-export.test.ts:44-48`; `integration/observe-run.test.ts:217-218` | `new OTLPTraceExporter(client.otlpExporterOptions())` |
| No backpressure-aware emit; the test hand-rolls a retry loop on `WYRD_CLIENT_429_QUEUE_FULL` | `integration/observe-run.test.ts:438-448` | `await model.observe.driftWhenAdmitted(features)` or a documented `{ wait: true }` option |
| Verification results can only be read through raw SQL on `vala.verification.results` / `vala.drift.result_features`, then by parsing `details` JSON | `integration/drift-verification.test.ts:328-356,365-404,1014-1031` | A typed `Judgment` from `run.observe.verify(verifier, input)` (REQ-188) for real-time checks, plus a documented, typed result row shape for history |
| After REQ-189 there is no SDK view of binding activation; the test uses `Verification.getBinding(...).active` | `integration/drift-verification.test.ts:949-976` | `(await cards.get(service)).status.verification.bindings[i].active` |
| Stored OTel attributes are opaque `KeyValueList` bytes; tests assert `contains` on decoded UTF-8, or hand-encode protobuf | `integration/otel-export.test.ts:92-94,199-205`; `integration/oracle-query.test.ts:229-281,613-617` | A query function or SDK decoder (`decodeAttributes(blob)` → `Record<string, unknown>`) |
| Card authoring needs a precomputed `sha256`/`size_bytes`, so tests hash in code | `integration/cards-state.test.ts:20,36-37`; `integration/drift-verification.test.ts:113-114,139-142`; `integration/verification-execute.test.ts:127-128,145-149`; `integration/observe-run.test.ts:41,57-58` | A static fixture whose artifact entry omits both values (REQ-191) |
| Provider credentials cannot be created through the SDK by design, so tests PUT/DELETE the admin route with raw `fetch` | `integration/gateway-admin.test.ts:396-410,421-428,456-458,474-478`; `integration/gateway-inference.test.ts:126-158` | Harness gap rather than SDK gap: `startTestServer({ providerCredentials: [...] })` or a documented CLI fixture step, so the journey starts from a provisioned credential |
| `TableConfig.fromJsonSchema` takes positional optionals, so callers pass `undefined` | `unit/bifrost-query.test.ts:212,217-218`; `integration/bifrost-write.test.ts:231` | `TableConfig.fromJsonSchema(fqn, schema, { compactionTargetFileSizeBytes })` |
| Some local refusals have no catalog code; tests match message text | `integration/bifrost-write.test.ts:216` (`/invalid cardRef/`); `unit/bifrost-query.test.ts:217-218` (`/compaction target/`) | `toThrow(expect.objectContaining({ code: "WYRD_SPEC_400_VALIDATION" }))` |

## Cross-cutting problems
1. **Card YAML built in code with template strings, string splicing, and computed digests.** Affects 8 tests. Examples: `integration/cards-state.test.ts:19-136`; `integration/drift-verification.test.ts:734-872`, where the Service graph is composed from nested template functions (`drift()`, `onFailure()`); `integration/verification-execute.test.ts:51-113,110`, which builds YAML by `.replace("max_retries: 0", ...)`; `integration/drift-verification.test.ts:616-622`, which derives a Card by `.replace` on another Card's YAML. Fix: checked-in fixture directories shared by the three SDKs (REQ-192), with artifacts that omit digests (REQ-191). Load them with `cards.registerFromPath(fixture("continuous-service/service.yaml"))`.
2. **The removed `Verification` handle (REQ-189).** Affects 5 tests: `integration/verification-run.test.ts:328`; `integration/verification-execute.test.ts:319,399`; `integration/drift-verification.test.ts:424,916`. Fix: delete the handle's journeys and rebuild the real-time path on `run.observe.verify` (REQ-188). Binding, run start, and run status coverage moves to Rust server HTTP/MCP tests.
3. **Mega-tests that cover many stories in one `it`.** Affects 10 tests. Examples: `integration/drift-verification.test.ts:424-655` (about 15 stories, 230 lines); `:916-1157` (240 lines); `integration/observe-run.test.ts:151-395` (about 12 stories); `integration/cards-state.test.ts:160-241` (about 10 stories); `integration/verification-execute.test.ts:319-397` (11 verdicts plus 14 refusals); `integration/gateway-admin.test.ts:413`; `integration/operator-connections.test.ts:254`; `integration/bifrost-write.test.ts:72,151`. Fix: one story per `it`, with a shared `beforeAll` server per file.
4. **Server-internal state and test-only hooks used to fake or observe production behaviour.** Affects about 14 tests. Examples: polling `server.verificationRuns()` (`integration/drift-verification.test.ts:250-262,875-889`; `integration/verification-execute.test.ts:396,406`); `server.retireFittedFormat` (`integration/verification-execute.test.ts:291`; `integration/drift-verification.test.ts:597`); `server.tableDescribeCount` / `changeTableFingerprint`, which also need `auditPublication: false` (`integration/observe-run.test.ts:156,196,235,283,385-388`); `server.bifrostReadDecisionCount` (`integration/oracle-query.test.ts:163,178`); `server.seedBifrostRows` instead of SDK writes (`integration/oracle-query.test.ts:33,36,65,100,138,162`; `integration/bifrost-write.test.ts:75`). Fix: seed through `Bifrost.insert`, assert only on user-observable results, and move cache, fence, audit, and legacy-format checks to Rust. `makeBindingDue` is the one documented clock control and may stay.
5. **Engine and query-engine semantics asserted from the SDK.** Affects 5 tests. Examples: NIST X-bar/S limits and PSI bin counts parsed from `details` (`integration/drift-verification.test.ts:365-404`); DataFusion GROUP BY/window/join/CTE semantics (`integration/bifrost-write.test.ts:352-436`); IPC EOS bytes (`integration/oracle-query.test.ts:90`); canonical-ledger SQL over hand-built Arrow (`integration/oracle-query.test.ts:483-621`). Fix: MOVE to Rust `vala` drift-engine and Oracle tests. An SDK test asserts one read and a verdict.
6. **Duplicated helpers and connection boilerplate.** `rejection` is redefined in 7 files (`integration/bifrost-write.test.ts:62`, `cards-state.test.ts:139`, `verification-run.test.ts:318`, `drift-verification.test.ts:407`, `observe-run.test.ts:136`, `gateway-admin.test.ts:378`, `operator-connections.test.ts:243`). `credentialRequest` appears twice (`gateway-admin.test.ts:396`, `gateway-inference.test.ts:126`). `JUDGE_SCHEMA`/`JUDGE_COMPLETION` appear twice (`drift-verification.test.ts:697-718`, `verification-execute.test.ts:31-48`). Three mock HTTP servers are built three different ways (`drift-verification.test.ts:668`, `verification-execute.test.ts:227`, `gateway-inference.test.ts:62-93`). `{ serverUrl, credential, grpcUrl }` is restated about 25 times. Fix: vitest's native `await expect(p).rejects.toMatchObject({ code })` replaces `rejection` outright. Put one `support/server.ts` (connection options, server lifecycle) and one `support/http-receiver.ts` in `tests/support/`.
7. **Type tests disguised as runtime tests.** Affects 6 tests: `unit/operator-connection-types.test.ts:10,66` (asserts `[5,5,5]` and `toHaveLength(7)` on arrays it built), `unit/wyrd-error.test.ts:6,15`, `unit/bifrost-query.test.ts:153` (asserts fields of a literal it just wrote), and `unit/gateway.test.ts:60` (asserts a string literal equals itself). Fix: `expectTypeOf` / `@ts-expect-error` in `tests/types/*.test-d.ts` under vitest `typecheck`.
8. **Unit tests pin native-ABI argument tuples through fakes, which needs public native constructors.** Affects 11 tests. Examples: `unit/observe.test.ts:55-113,261-266` (positional `undefined` slots); `unit/bifrost-query.test.ts:25-36` (the same 10-line fake native repeated 5 times). The fakes only work because `Observe` (`src/index.ts:1750`) and `BifrostQueryStream` (`src/index.ts:330`) have public constructors that take native types, while every other handle's constructor is private. Fix: delete the ABI duplicates that integration readback already proves. Keep fault-path and value-refusal unit tests behind one shared fake, and stop exporting native-typed constructors.
9. **Weak or wrong error assertions.** Examples: bare `.rejects.toThrow()` (`integration/bifrost-write.test.ts:486-488`; `unit/bifrost-query.test.ts:143`), message regexes (see the API gaps table), and `try/catch/return` idioms (`unit/observe.test.ts:123-131`; `unit/connect-errors.test.ts:44-50,54-60`). Fix: assert the catalog `code` with `toThrow(expect.objectContaining({ code }))`.
10. **Process environment mutated by hand inside tests.** Examples: `integration/bifrost-write.test.ts:120-139`; `unit/connect-errors.test.ts:14-31`; `integration/gateway-inference.test.ts:86-88,96-100`. Fix: vitest `vi.stubEnv` / `vi.unstubAllEnvs`.
11. **Dead or self-referential assertions.** `integration/oracle-query.test.ts:582` (`expect(HISTOGRAM_SUM).toBe(12.5)` checks a constant against itself); `unit/gateway.test.ts:61-62`; `unit/operator-connection-types.test.ts:63,93`. Fix: delete them.
12. **Unique-name suffixes on a fresh per-test server.** Examples: `Date.now().toString(36)` at `integration/bifrost-write.test.ts:156,226,355,441` and `integration/observe-run.test.ts:160`. Every test already starts its own server, so the suffix only hurts readability. Fix: use fixed, meaningful table names.

## Per-file findings

### unit/bifrost-query.test.ts
Covers the wrapper over the native query stream (batches, terminal, fault paths), `TableDescription` typing, and the `TableConfig` compaction target.

| Test | Verdict | Q1 production behaviour | Q2 user understanding | Fix |
|---|---|---|---|---|
| yields Apache Arrow batches and retains the validated terminal (:15) | TIGHTEN | Wrapper decode; happy path duplicates oracle-query:25 | 20-line fake before the point | Share one `fakeStream(steps)` |
| closes the native response when iteration stops early (:48) | TIGHTEN | Resource cleanup; valid unit | Repeated fake | Shared fake |
| raises a structured incomplete-stream error without parsing messages (:66) | TIGHTEN | Valid; `polls` counter is trivia | OK | Drop the `polls` counter; shared fake |
| preserves failed-terminal step details and status (:96) | TIGHTEN | Valid; no integration analogue | OK | Shared fake |
| closes native ownership before propagating Arrow decode failures (:128) | TIGHTEN | Valid | Bare `rejects.toThrow()` (:143) | Assert the error type; shared fake |
| reaches recursive fields, nested events and links, and page tokens without casts (:153) | REWRITE | Type check only; asserts a literal it built | Misleading as a runtime test | `expectTypeOf` in `tests/types` |
| defaults to the deployment target and carries an explicit one (:208) | KEEP | Public `TableConfig` | Clear | — |
| refuses a target that is not a non-negative integer (:216) | TIGHTEN | Valid local refusal | Regex on message | Assert the catalog code |

### unit/connect-errors.test.ts
Covers construction failures with no credentials and an unreachable server.

| Test | Verdict | Q1 | Q2 | Fix |
|---|---|---|---|---|
| WyrdClient.connect throws the no-credentials WyrdError (:43) | TIGHTEN | Real first-run failure | try/catch boilerplate | `it.each` over constructors; `toThrow(objectContaining)` |
| Cards.connect throws the no-credentials WyrdError (:53) | TIGHTEN | Same | Same | Same |
| Bifrost.connect rejects with the no-credentials WyrdError (:63) | TIGHTEN | Same | Same | Same |
| TableConfig.describe rejects with the no-credentials WyrdError (:71) | TIGHTEN | Same | Same | Same; `vi.stubEnv` instead of hand env save |
| TableConfig.describe rejects an unreachable server with the transport WyrdError (:78) | KEEP | Real failure a user hits | Clear | — |

### unit/gateway.test.ts
Covers the Gateway admin wrapper's shape and its local validation.

| Test | Verdict | Q1 | Q2 | Fix |
|---|---|---|---|---|
| exposes every administration operation (:38) | DELETE | `typeof` method checks; the compiler proves this | Trivia | — |
| offers no provider credential mutation on either surface (:48) | REWRITE | Imports private `../../index.cjs` (:7-9) | Odd casts | Assert only on public `Gateway` (`"putCredential" in gateway`) |
| reads a managed secret and exports no write type (:60) | DELETE | Asserts a literal equals itself | Meaningless | — |
| rejects an invalid name in Rust before any request (:65) | KEEP | Local validation with no network | Clear | Rename to "rejects an invalid credential name locally" |
| rejects a malformed body without echoing it (:76) | KEEP | Secret-redaction guarantee | Clear, commented | — |

### unit/observe.test.ts
Covers `Observe` argument conversion, refusal of non-JSON values, and trace-context capture, all through a fake native run.

| Test | Verdict | Q1 | Q2 | Fix |
|---|---|---|---|---|
| passes the feature map as JSON text with its session (:55) | DELETE | Pins the native ABI tuple; observe-run readback covers it | Opaque tuple | Add `sessionId` to the drift readback journey |
| encodes media descriptors with their canonical field names (:69) | DELETE | Duplicates observe-run:341 `MEDIA_TEXT` readback | Opaque tuple | — |
| omits media entirely when the caller names none (:101) | DELETE | ABI trivia (positional `undefined` slots) | Opaque | — |
| awaits one generic-table row as JSON text (:109) | DELETE | Duplicates observe-run:362 | Opaque | — |
| throws the projected catalog error an emit was refused with (:115) | TIGHTEN | Valid; no journey covers "Bifrost not started" | try/catch/return idiom | `toThrow(objectContaining)`; add a journey negative |
| refuses every value JSON.stringify would drop or coerce before native (:134) | TIGHTEN | Real user protection (NaN, bigint, Date, cycles) | 21 cases × 3 methods in one loop | `it.each` by category |
| reads every accessor once and sends native exactly the first value (:191) | TIGHTEN | Valid TOCTOU guard | Hard to follow | `it.each`; name the property in the title |
| refuses media descriptor keys outside the closed shape before native (:233) | TIGHTEN | Valid | Two stories (refusal plus accessor) | Split |
| calls native exactly once for valid nested JSON (:261) | DELETE | Duplicate happy path; ABI trivia | — | — |
| takes both ids from the active span only when neither is explicit (:268) | TIGHTEN | Real OTel behaviour | Asserts `emit[3]`, `emit[4]` | Small named accessor; split explicit-wins and invalid-span cases |

### unit/operator-connection-types.test.ts
Compile-time shape tests for Operator connection request and view unions.

| Test | Verdict | Q1 | Q2 | Fix |
|---|---|---|---|---|
| accept every provider-tagged create, update, and view shape (:10) | REWRITE | Type check; the runtime assertion is `[5,5,5]` | Misleading | `tests/types/operator-connections.test-d.ts` |
| reject invalid provider combinations at compile time (:66) | REWRITE | Same; `toHaveLength(7)` | Same | Same |

### unit/wyrd-client.test.ts
Covers `WyrdClient` local validation and URL derivation.

| Test | Verdict | Q1 | Q2 | Fix |
|---|---|---|---|---|
| onBehalfOf rejects an unknown audience with the validation WyrdError (:8) | KEEP | Guards JS callers | Clear | — |
| onBehalfOf runs the exchange in Rust (:17) | DELETE | Asserts only "not validation"; bifrost-write:87 covers the real exchange | Vague | — |
| derives the gRPC endpoint from serverUrl unless overridden (:26) | KEEP | User-visible default | Clear | — |

### unit/wyrd-error.test.ts
Covers the `WyrdError` class and the `WyrdErrorCode` union.

| Test | Verdict | Q1 | Q2 | Fix |
|---|---|---|---|---|
| carries a catalog code from the generated WyrdErrorCode union (:6) | REWRITE | Users never construct `WyrdError` | — | One `expectTypeOf<WyrdError["code"]>()` type test |
| rejects codes outside the catalog at compile time (:15) | DELETE | Merged into the type test above | — | — |

### support/otel-context.ts
A minimal `AsyncLocalStorage` context manager. It reimplements `AsyncLocalStorageContextManager` from `@opentelemetry/context-async-hooks`. Fix: use the installed upstream package if it is a dependency; otherwise keep it, since it is tiny.

### fixtures/drift-baseline.parquet
The only checked-in fixture. Move it into the shared cross-SDK fixture tree with its Data Card YAML beside it.

### integration/bifrost-write.test.ts
Covers Bifrost register, write, swap, delegation, compaction target, describe, analytical SQL, and typed reads.

| Test | Verdict | Q1 | Q2 | Fix |
|---|---|---|---|---|
| reads as A but cannot write with B's authority through a delegated client (:72) | REWRITE | Right intent; harness seeding (:75), raw `/auth/token` (:41), Scribe barrier | 5 stories, including env chain and option conflict | Split into delegated read/denied write, env-chain connect, and option conflict; use `vi.stubEnv` |
| registers, writes, flushes, swaps tables, and reads back (:151) | REWRITE | Asserts `producerCount` (internal) | 4 stories | Split register idempotency and swap-drains-rows; drop counters |
| refuses a write with no active table and a bad card reference (:204) | TIGHTEN | Valid negatives | Message regex (:216) | Assert the code; split |
| registers a compaction target, describes it back, and refuses a different one (:223) | TIGHTEN | Valid | Positional `undefined`; closure helpers | Options object; inline the steps |
| describes an existing table without restating its schema (:263) | TIGHTEN | Valid | Mixes `token`/`apiKey` | One credential |
| runs group-by, window, join, and scalar SQL over written tables (:352) | MOVE | DataFusion semantics; belongs in Rust Oracle tests | Long | Keep one aggregate read in the SDK |
| keeps the server's schema on an empty result and parses typed rows (:438) | TIGHTEN | Good typed `sql(select, zodSchema)` | 3 stories; bare `toThrow()` (:488) | Split; assert the error type |

### integration/cards-state.test.ts
Covers Card register, get, list, hydrate, binding ids, denial, and offline `WyrdState`.

| Test | Verdict | Q1 | Q2 | Fix |
|---|---|---|---|---|
| registers, reads, hydrates, and loads offline state (:160) | REWRITE | Right surfaces; UUIDv7 regex on binding ids (:197) is trivia | About 10 stories; YAML built in code (:19-136) | Static fixture graph; split into register/replay, list, denied, hydrate complete vs metadata, and offline state with unknown alias |

### integration/verification-run.test.ts
Covers the manual verification run through the `Verification` handle.

| Test | Verdict | Q1 | Q2 | Fix |
|---|---|---|---|---|
| reads a binding, starts a keyed run, and reads the run back (:328) | DELETE | `getBinding`/`startRun`/`getRun` are removed by REQ-189 | — | Keep idempotency and window coverage in Rust HTTP/MCP tests |

### integration/verification-execute.test.ts
Covers direct execution through `Verification.execute` (being replaced by `observe.verify`).

| Test | Verdict | Q1 | Q2 | Fix |
|---|---|---|---|---|
| judges supplied input inline with exact attribution and stable refusals (:319) | REWRITE | Right intent, removed surface; raw UID request dicts (:200-208); `verificationRuns()` (:396); `retireFittedFormat` (:291) | 25 cases in one test; YAML fragments (:51-113) | `observe.verify` journeys: Drift pass/fail, Eval assertion pass/fail, judge pass, unknown verifier (local), wrong input shape (local), denied, cross-tenant. MOVE size limits, legacy, unfitted, and 502 to Rust |
| refuses a judge that outlives the server deadline (:399) | MOVE | Server deadline; 65 s mock delay | Slow | Rust server integration test |

### integration/drift-verification.test.ts
Covers Drift method edge cases and the full continuous Drift + Eval + Operator journey.

| Test | Verdict | Q1 | Q2 | Fix |
|---|---|---|---|---|
| scores each method's edge cases through the production runtime (:424) | MOVE | SPC/PSI engine statistics (:365-404); retired-kind registration (:473-491); removed handle; run polling | 230 lines, about 15 stories | Rust drift-engine and server verification tests. The SDK keeps one "emit rows → `observe.verify` Drift judgment" journey |
| verifies one Service through Drift, Eval, and an Operator end to end (:916) | REWRITE | The right flagship story, but it uses `getBinding`/`startRun`/`getRun`, polls `verificationRuns()`, and runs raw SQL on result tables | 240 lines; YAML template composition (:734-872) | Static fixture graph. Drift through `makeBindingDue`; failed Eval → assert the request the local hook receives; `observe.verify` for Judgments; cross-tenant as its own test |

### integration/observe-run.test.ts
Covers scoped Drift/Eval/record emits, trace correlation, readback, the stale fingerprint fence, and the burst backpressure test.

| Test | Verdict | Q1 | Q2 | Fix |
|---|---|---|---|---|
| emits Drift, Eval, and generic rows correlated to their subject Cards (:151) | REWRITE | Readback assertions are good. `tableDescribeCount` (:196,235,283) and `changeTableFingerprint` (:386) are internals; `cardRef.split("#")` (:209) | About 12 stories, 245 lines | Split: run scoping/forCard; Drift readback; Eval plus span join; record into two tables; local refusals. MOVE describe caching and the fingerprint fence to Rust `wyrd-client` |
| lands every row of a 1,000 x 9 burst exactly once (:404) | TIGHTEN | A real backpressure journey | Hand retry loop; unsealable-budget refusal mixed in | Split the budget refusal out; a backpressure-aware API would remove the loop |

### integration/oracle-query.test.ts
Covers query streaming, IPC, fault paths, denial, deadline validation, and canonical-signal Arrow writes.

| Test | Verdict | Q1 | Q2 | Fix |
|---|---|---|---|---|
| uses the public SDK against an in-process Wyrd server (:25) | REWRITE | Seeds through the harness; asserts live and published source union (server internals) | Title says nothing | "streams Arrow batches for a query"; seed through `Bifrost.insert` |
| uses schema-once IPC with explicit EOS (:62) | MOVE | Wire bytes `[255,255,255,255,0,0,0,0]` (:90) | Trivia | Rust Oracle wire test |
| converts a collected result to Arrow and to IPC bytes (:96) | TIGHTEN | Valid | Harness seeding | Seed through the SDK |
| fails closed on EOF after %s (:132) | KEEP | Fault injection is legitimate at the integration tier | Clear | — |
| rejects an authenticated token before Oracle work (:159) | TIGHTEN | 403 is right; the audit-count check (:163,178) is server-internal | Special `queryDeniedToken` hook | Use `scopedApiKey`; move the audit assertion to Rust |
| rejects out-of-range deadlines with the shared validation error (:184) | TIGHTEN | Valid, but the code is `QUERY_INVALID_SQL` for a deadline, which misleads | Loop | `it.each`; check whether a dedicated code is warranted |
| canonical signal Arrow write and SQL read round-trip (:483) | MOVE | Hand protobuf encoder (:229-281), `ensureBuiltinTable` hook, engine SQL; dead assertion (:582) | 140 lines of encoding | Rust canonical-ingest test; otel-export already covers the user path |

### integration/otel-export.test.ts
Covers stock OpenTelemetry trace, log, and metric exporters landing in Bifrost and read back through the SDK.

| Test | Verdict | Q1 | Q2 | Fix |
|---|---|---|---|---|
| stock OpenTelemetry tracer exports GenAI span to Bifrost (:97) | TIGHTEN | Exactly the user path | About 20 indexed column lookups | Assert `table.get(row).toJSON()` subsets; metadata helper |
| stock OpenTelemetry logger exports correlated log to Bifrost (:216) | KEEP | User path, well commented | Clear | — |
| stock OpenTelemetry meter exports representative metrics to Bifrost (:279) | KEEP | User path | Clear; explains `ValueType.INT` | — |

### integration/gateway-admin.test.ts
Covers Gateway credentials, deployments, a read-only principal, and capture policy.

| Test | Verdict | Q1 | Q2 | Fix |
|---|---|---|---|---|
| manages credentials, deployments, and capture policy (:413) | REWRITE | Raw `fetch` to the admin route (:396-410); `Object.keys(view).sort()` field-list trivia (:433) | 7 stories | Provision the credential through a harness option; split deployment CRUD, read-only denial, and capture policy |

### integration/gateway-inference.test.ts
Covers the OpenAI SDK through the Wyrd Gateway: buffered and streamed answers, unprivileged refusal, and no token leak upstream.

| Test | Verdict | Q1 | Q2 | Fix |
|---|---|---|---|---|
| relays buffered and streamed answers and refuses an unprivileged caller (:143) | TIGHTEN | Excellent user path (real `openai` client) | Raw `/auth/token` and credential `fetch`; module-level env and `received` state | Split the refusal out; `vi.stubEnv`; token helper |

### integration/operator-connections.test.ts
Covers the Operator connection lifecycle, redaction, read/write separation, and tenant isolation.

| Test | Verdict | Q1 | Q2 | Fix |
|---|---|---|---|---|
| manages redacted connections with read/write separation (:254) | TIGHTEN | Public SDK, typed requests, real negatives | About 6 stories | Split: Slack lifecycle, HTTP auth rotation, reader/outsider, cross-tenant |

## Good examples to keep
- `integration/otel-export.test.ts:216` and `:279`: a stock exporter configured the way a user would, read back through `Bifrost.sql`, with comments that explain why.
- `integration/gateway-inference.test.ts:143`: drives the real `openai` client through Wyrd and checks the upstream sees the provider key, not the Wyrd token.
- `integration/operator-connections.test.ts:254`: typed requests, `expectRedacted` on every response and error, and real denial and cross-tenant negatives. Only needs splitting.
- `integration/bifrost-write.test.ts:298-322,438`: Zod schemas as domain types for both table declaration and typed reads (`reader.sql(select, Row)`).
- `integration/bifrost-write.test.ts:223`: one clear story (register, describe, idempotent, mismatch refused).
- `integration/oracle-query.test.ts:132`: fail-closed stream truncation through `it.each`.
- `unit/gateway.test.ts:65,76`: local validation and the no-echo guarantee, without a server.
- `unit/wyrd-client.test.ts:26`, `unit/connect-errors.test.ts:78`: small, readable, user-visible defaults and failures.
- `unit/observe.test.ts:134`: the right user protection (refuse NaN, bigint, Date, cycles before sending). Only its structure needs `it.each`.

## Proposed target structure
```
tests/fixtures/                      # repo-level, shared by Rust/Python/TS (REQ-192)
  cards/
    service-graph/                   # Service + Model + Agent + Prompt (+ prompt.txt, no sha256)
    continuous-verification/         # Service with Drift/Eval bindings, Operator hook URL via path template (REQ-190)
    drift-baseline/                  # data.yaml + data.parquet
    verifiers/                       # psi.yaml, spc.yaml, custom.yaml, eval-assert.yaml, eval-judge.yaml, judge-prompt.json
sdks/wyrd-sdk-ts/wyrd/tests/
  support/
    server.ts          # useServer(opts) beforeAll/afterAll, connection(server), fixture(path)
    http-receiver.ts   # local endpoint recording requests, awaitRequest(path, timeout)
    otel-context.ts
  types/               # *.test-d.ts: operator connections, WyrdErrorCode, TableDescription
  unit/
    query-stream.test.ts      # fault/close paths, one shared fake
    observe-values.test.ts    # JSON refusals, accessor snapshot, trace-context capture
    local-errors.test.ts      # connect errors, gateway/WyrdClient local validation
    table-config.test.ts
  integration/                # one user story per it
    cards.test.ts             # register/replay, list, denied, hydrate, offline WyrdState
    bifrost-write.test.ts     # register, insert, swap-drain, compaction target, describe
    bifrost-query.test.ts     # sql, stream, typed rows, empty schema, truncation, denial
    bifrost-delegation.test.ts
    observe.test.ts           # Drift/Eval/record readback, scoping, backpressure
    observe-verify.test.ts    # Judgment journeys + local and remote negatives (REQ-188)
    continuous-verification.test.ts  # due schedule / observations_ready -> Operator request received
    otel-export.test.ts
    gateway-admin.test.ts
    gateway-inference.test.ts
    operator-connections.test.ts
```
The MOVE set goes to Rust: the drift edge-case engine matrix, DataFusion SQL semantics, the IPC EOS wire format, canonical-ledger Arrow ingest, the judge deadline, verification size limits and legacy-baseline refusals, describe caching and the fingerprint fence, and the read-decision audit count.
