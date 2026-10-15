# Independent findings validation

Result: **FIX_REQUIRED**. Six deduplicated bounded findings remain. No specification revision or unresolved product/security/ownership decision is required. This is fresh validation, independent of implementation and discovery; agreement counts were not used as proof.

## Subject, inputs, and method

- Repository: `/home/thorrester/Documents/GitHub/wyrd-worktrees/split/agents-trace-processor`.
- Full base: `dac4c340f835676adc29f3dcc01f98e1d3d51465`.
- Full candidate: `692d390dc40e43c6e1d7e25c7850d5c826d26a9e`.
- Authority: approved `changes/active/agents-trace-processor/spec.md` r2; original `tasks/TASK-001-framework-agnostic-telemetry.md`; AGENTS.md, agent rules, active design/provider/observation identity contracts, doctrine, telemetry observations, spec-driven development and maintainer standards. Repository authority wins over task mechanics and execution summaries.
- Inputs: `subject.md`, both implementation reviews, standards, maintainer, system, security, concurrency, OTEL and durability reports, `verification.md`, and `claim-comparison.md`. All nine required discovery reports exist. All 17 source-local material proposals are dispositioned below, including the unique concurrency proposal and both durability proposals omitted from the comparison's initial ID rows.
- CodeGraph was consulted before source navigation. Inspection expanded the changed owners into real framework producers, installed pinned dependency implementations, ordinary package consumers, both public lifecycle callers, correlation hooks, native writer lifecycle, sibling observation/local-development journeys, and framework fixtures. Full bodies of the proposed correction owners were read: Python `_start_telemetry`, `_gen_ai`, bridge callbacks, `flush_telemetry`, state flush/shutdown; TS bridge/type consumers, `startTelemetry`, provider installer, lifecycle methods; framework app main/builders/run and parent launch/readback; TS managed/owned journeys; package checker.
- No prior independent `FIND-*` ledger exists for this candidate. Stable IDs below start at 1. Source/spec/task remained immutable; only this report is owned by this validator.

The comparison's no-follow-up decision is supported. No discovery reports assert opposing reachable facts requiring a fresh discovery role. Apparent PASS rows are broad statements narrowed by concrete counterexamples. The significant correction uncertainty—stock Python concurrent processor substitution—was independently resolved below from its complete implementation, rather than accepted by consensus.

## Proposal-by-proposal disposition

| Discovery proposal(s) | Disposition | Validated boundary / adjustment |
|---|---|---|
| BEH-001; MNT-001; STD-TS-001 | CONFIRMED | FIND-TASK-001-1. Optional framework types unconditionally escape through the generic root declaration. Structural callback inputs and clean declaration proof close it without changing imports or making a framework mandatory. |
| BEH-002; INV-REV-002; MNT-002; OTEL-003 | CONFIRMED | FIND-TASK-001-2. All four framework procedures consume objects not constructed or supplied by a complete prerequisite. Complete these existing procedures using demonstrated framework construction. |
| BEH-003; INV-REV-003; SYS-002; DUR-02 | REVISED | FIND-TASK-001-3. Keep the live-flush and continued-export proof gap. Exercise the existing managed journey owners; do not expand to unrelated suites, adopt application-owned providers, or claim inspected production sequencing itself is wrong. Prove native writer continuation before shutdown and global OTEL continuation after shutdown as distinct obligations. |
| INV-REV-001; OTEL-002 | REVISED | FIND-TASK-001-4. Direct Responses usage is genuinely supplied and discarded. Use that safe field with existing response-object fallback. Actual `ResponseSpanData` plus the real bridge callback provides focused closure of this pure mapping gap; OTEL-002's additional new Responses HTTP endpoint/journey is unnecessary for this bounded correction. Existing real OpenAI journeys remain regression evidence. |
| SYS-001; OTEL-001; DUR-01 | REVISED | FIND-TASK-001-5. Actual synchronous stock force-flush exceeds its bound. Reject a wholesale `ConcurrentMultiSpanProcessor` replacement: it changes callback context and can block application span emission after timeout. Select a bounded wait around only managed lifecycle flushing, preserving stock batch/export and synchronous correlation/callback paths. Details below resolve the safe correction. |
| CONC-001 | CONFIRMED | FIND-TASK-001-6. The initial same-state check precedes an import await; both calls pass, and the second rejects its own state's provider. One post-import same-state recheck closes this without a new startup registry or promise owner. |

No material proposal is rejected in its entirety. The unsafe stock concurrency substitution and unnecessary additional Responses transport proof are rejected portions of proposed corrections, not additional findings. Standards' historical missing RED evidence and test environment notes remain nonblocking observations: neither establishes another reachable code defect or calls for retroactive fabricated evidence. The recorded owner-directed Python 3.11/development-dependency changes are not reclassified as scope drift.

## Validated final ledger

### FIND-TASK-001-1 — CONFIRMED — REGRESSION: optional Agents types break ordinary TS consumers

**Sources:** BEH-001, MNT-001, STD-TS-001.

**Locations:** `sdks/wyrd-sdk-ts/wyrd/src/index.ts:1`, `:2517-2540`; `wyrd/package.json:24-27,47-59`; emitted `wyrd/dist/index.d.ts:1,1287-1296`; `scripts/check_ts_package.mjs:143-155`.

**Obligations:** REQ-010 optional framework dependencies; AC-003 usable typed client/package; task Scenario 1 generic imports without frameworks; AGENTS first-class typed SDK/public contract rules.

**Evidence and consequence:** The exported root class implements imported `TracingProcessor` and exposes imported `Trace`/`AgentsSpan<SpanData>` callback inputs. Declaration emission retains `@openai/agents-core` at the top of the root declaration. Package root `types` selects that file for every caller, including Cards/Bifrost/WyrdClient-only users; the peer is expressly optional. JavaScript erasure does not erase declaration dependencies. Both independent consumer reproductions in discovery returned TS2307 with no Agents package and declaration checking enabled; source independently establishes the same dependency chain. The existing clean-package checker runs JavaScript only, while development typechecking has the peer installed. Ordinary users must install an unrelated framework or weaken declaration checking.

**Decision-complete correction:** Remove the optional framework type dependency from root public declarations. Keep the approved root processor export and precise, self-contained structural callback inputs for the fields actually consumed by the bridge/helpers. Preserve real framework assignability through existing framework-enabled compilation/tests. Regenerate declarations from source. Do not make the peer mandatory, move the public bridge to another import surface, replace the transport, or disable declaration checking. Extend the existing clean-package checker with a noEmit consumer using root generic APIs and the processor, no optional peers, with `skipLibCheck: false`. This extends an existing package property check rather than adding a second package harness.

**Focused closure:** After the existing TS build, run `mise exec -- node scripts/check_ts_package.mjs`; the extended checker must compile its ordinary consumer with the installed TypeScript compiler and no optional framework/OTEL peers. Also run `mise run ts:typecheck` to prove real `setTraceProcessors([new WyrdAgentsTraceProcessor()])` assignability and `mise exec -- pnpm --dir sdks/wyrd-sdk-ts/wyrd exec vitest run tests/unit/telemetry.test.ts -t '^agents callback lifecycle$'`. No new server suite is needed for declaration resolution.

### FIND-TASK-001-2 — CONFIRMED — MISSING: documented framework recipes have no executable construction

**Sources:** BEH-002, INV-REV-002, MNT-002, OTEL-003.

**Locations:** `docs/src/content/docs/build-and-verify/observe/record-a-run-and-observations.md:159-216`; available reusable construction in Python `tests/integration/otel/frameworks/app.py:150-251` and TS `tests/integration/agents-trace-processor.test.ts:93-111`.

**Obligations:** REQ-015 and AC-012 explicitly disallow undefined framework placeholders as the only procedure; docs complete-task conventions.

**Evidence and consequence:** Pydantic imports/enables instrumentation but never creates its agent/model; ADK supplies no agent, runner, session or message; both OpenAI snippets execute an undefined agent. Install prerequisites omit Pydantic/ADK and TS Agents. Earlier sections build Wyrd state, not these framework objects. The stock-client link configures provider HTTP routing and does not supply the missing framework construction. A reader cannot execute any of these advertised recipes from its prerequisites. Draft status does not waive an explicit recipe acceptance criterion.

**Decision-complete correction:** Complete the existing recipe sections using the minimal actual framework construction already exercised in the named builders: package prerequisites, independent model/provider credentials, agent/tool, and ADK runner/session/message where applicable. Define the prompt/input used in execution. Keep managed setup before agent construction; retain ambient Wyrd auth, optional Run scope, upstream content controls, existing readback, supported-version disclosures and publication flags. Use existing sections rather than introducing a separate tutorial framework or adapter. The correction must leave no unexplained execution variable; a complete existing prerequisite link is acceptable only if it actually defines those same objects.

**Focused closure:** Follow each recipe statically from install through construction, invocation and authenticated readback; compare it with its existing real framework builder. Run `mise run docs:generate` and `mise run docs:check`. Framework runtime proof is supplied by the focused managed journeys in FIND-TASK-001-3; no Markdown parser test or additional full journey suite is required merely for prose edits.

### FIND-TASK-001-3 — REVISED — MISSING: live managed flush and continued export lack required journey proof

**Sources:** BEH-003, INV-REV-003, SYS-002, DUR-02.

**Locations:** Python `tests/integration/otel/frameworks/app.py:313-344`, parent `test_framework_telemetry.py:62-82,114-134`; TS `tests/integration/agents-trace-processor.test.ts:203-222`; supporting lifecycle tests Python `test_observe_surface.py:768-850` and TS `telemetry.test.ts:126-180`.

**Obligations:** REQ-013/016, AC-009, task Scenarios 3/5, AGENTS §11. Successful state flush needs real SDK/server/readback while the application remains alive; global export must survive state shutdown.

**Evidence and consequence:** Python `launch` waits for process completion; app main invokes state shutdown before printing its report. TS managed execution shuts down before readback. Owned cases shut down their own provider, correctly, but cannot prove the managed state flush. Existing Python observation journeys call their owned `otlp_tracer.force_flush()` before native state flush; existing TS observation backpressure flush has no managed provider. Unit lifecycle tests replace provider flush and intentionally do not start Bifrost. Thus all delivered checks can remain green if state flush stops exporting, closes the writer, or global OTEL stops exporting after state shutdown. This is an explicit acceptance-proof gap, not a new claim that current successful ordering is defective.

**Decision-complete correction:** Extend the existing Python managed framework app/launch/test and TS managed framework test. Start Bifrost as required by successful native flush. Complete a real turn, call public state flush and read back before shutdown/process completion. Keep an active span across that flush, verify it remains active, then end/export it. Continue framework work and native observation writing with the same state to prove neither owner was closed by flush. Retain shutdown coverage; after state shutdown, emit further framework work through the same installed global provider and explicitly flush that provider to prove continued export without reopening the native writer. Preserve the current application-owned journeys and their separate lifecycle owner. Use existing subprocess/server/query fixtures; do not substitute server publication flush or in-memory export for the application's state barrier.

**Focused closure commands:**

```sh
mise exec -- scripts/postgres/with-test-postgres.sh -- bash -lc 'mise run db:migrate:inner && mise exec -- uv run --directory sdks/wyrd-sdk-python python -m pytest -q -m integration tests/integration/otel/frameworks/test_framework_telemetry.py::test_managed_framework_telemetry'
mise exec -- scripts/postgres/with-test-postgres.sh -- bash -lc 'mise run db:migrate:inner && mise exec -- pnpm --dir sdks/wyrd-sdk-ts/wyrd exec vitest run tests/integration/agents-trace-processor.test.ts -t "^managed framework telemetry$"'
```

The Python selector must collect its three existing parameter cases; TS must collect the one named case. These focused executions also provide the recipe regression proof without rerunning whole framework/Bifrost aggregates.

### FIND-TASK-001-4 — REVISED — INCORRECT: supplied payload-free Responses usage is discarded

**Sources:** INV-REV-001, OTEL-002.

**Locations:** Python `python/wyrd/otel.py:430-435`, reached from `_end:382`; supporting test `tests/unit/state/test_observe_surface.py:974-983`.

**Obligations:** REQ-010 maps available input/output usage at completion; AC-007. Upstream content suppression must not require enabling content to retain already supplied safe counters.

**Producer/consumer evidence:** Pinned Agents 0.23.1 `agents/tracing/span_data.py:212-234` defines `ResponseSpanData.usage` separately from `response`. Its real Responses producer `models/openai_responses.py:635-641` always writes available payload-free counts there, while the full response is gated by `tracing.include_data()`. Stream completion does the same at `:736-745`. Wyrd instead reads only `data.response.usage`; when sensitive content is disabled, response is absent and both counts disappear. Generation spans already consume their own direct usage. The TS pinned producer does not expose the equivalent independent Responses usage field, so this is Python-only. Missing upstream model/provider identity remains correctly absent.

**Independent probe:** With installed actual `ResponseSpanData(response=None, usage={"input_tokens": 7, "output_tokens": 2})`, current `_gen_ai` returned `('chat', {'gen_ai.operation.name': 'chat'})`. No network or simulated producer schema was needed. The current test supplies a synthetic full response and cannot catch this.

**Decision-complete correction:** In the existing Python response branch, map supplied direct `data.usage` counters, retaining the existing full-response usage fallback when direct counts are absent. Preserve zero values, missing model/provider omission, late completion reads, generation/unknown behavior and complete payload exclusion. Do not re-enable sensitive content, invent a provider, add a TS field, or create another mapper/adapter.

**Focused closure:** Extend `test_agents_callback_lifecycle` with actual `ResponseSpanData`, absent response and nonzero direct counts through bridge completion; assert exact attributes and no model/content invention. Also retain response-object fallback and zero-count cases. Run `mise exec -- uv run --directory sdks/wyrd-sdk-python python -m pytest -q tests/unit/state/test_observe_surface.py::test_agents_callback_lifecycle`. This is a runtime-local mapping branch on an existing public bridge, with no changed cross-boundary state; a new Responses HTTP fixture/journey would duplicate the exact supplied-data proof. Existing real OpenAI managed journey execution under FIND-TASK-001-3 guards the shared export path.

### FIND-TASK-001-5 — REVISED — INCORRECT: Python managed force-flush can block past its bound and delay native drain

**Sources:** SYS-001, OTEL-001, DUR-01.

**Locations:** Python `python/wyrd/otel.py:232-234` constructs the managed default synchronous processor owner; `src/state/mod.rs:506-523` directly calls its force-flush; state `flush:277-280` and `shutdown:299-302` cannot reach native drain before that call returns.

**Obligations:** REQ-013/AC-009 require bounded OTEL flushing followed by native drain even on timeout, using existing SDK flush bounds. INV-002/004 forbid a new exporter/retry queue or ownership adoption; REQ-010/014 require start-time context/correlation preservation.

**Evidence and realistic consequence:** Locked/installed OTEL SDK 1.42.1 chooses `SynchronousMultiSpanProcessor`. Its flush checks deadlines only between children; the actual BatchProcessor `sdk/_shared_internal/__init__.py:239-246` ignores timeout and synchronously exports all batches under `_export_lock` (`:167-199`). A slow export, several slow batches or waiting behind an in-progress export can exceed the provider deadline while native observations remain undrained. Stock per-request export timeout is not a total provider flush bound. The added result/error conversion only works once control returns.

**Independent bounded probe:** A real stock TracerProvider + BatchSpanProcessor queued one ended span. A controlled exporter waited on an Event released by a 150 ms timer; `provider.force_flush(timeout_millis=1)` returned **True after 150 ms**, despite its 1 ms requested limit. Provider/timer were then cleaned up. This tiny dependency probe uses no live network, host load, fabricated force-flush result, source mutation or package installation. It establishes the ignored-bound mechanism; the production public lifecycle gap follows from the direct call ordering.

**Rejected tempting correction:** Whole-provider `ConcurrentMultiSpanProcessor` does enforce a wait bound in `force_flush`, but its full `on_start`, `_on_ending` and `on_end` submit work to the same executor and synchronously wait (`sdk/trace/__init__.py:263-295`). Correlation currently resolves `parent_context=None` from caller-local context; moving it to workers loses Run scope. If outstanding timed-out exports occupy the executor, ordinary callbacks can then block behind them. Merely increasing its thread count or placing it around the batch child shifts rather than removes the saturation path. Stock concurrency is not a complete safe substitute.

**Decision-complete minimal correction:** Keep the existing provider, stock BatchSpanProcessor/exporter, synchronous correlation and ordinary span callbacks. Bound only the managed pipeline's lifecycle wait around provider flushing, using Python's standard threading/result coordination at that existing owner. Retain at most one in-flight flush attempt per managed pipeline; overlapping/repeated calls must not create unbounded threads, queued retry attempts or additional export queues. Use the provider force-flush API's existing `timeout_millis` bound/default, with no Wyrd public timeout option. A deadline expiration returns the existing false/timeout outcome so the current catalog settlement and native drain ordering apply while that attempt completes in the background. Preserve type-only error projection and later provider usability. Do not wait for an unfinished worker in a timeout cleanup path, executor context-manager exit, or a newly introduced non-daemon interpreter-join path. The owning retained operation earns its state; there is no reason for a second exporter, durable queue, cancellation protocol or runtime.

This is local enforcement of the already-approved wait/ownership contract, not a new concurrency guarantee: stock export continues best-effort; no promise to cancel an in-flight network call, replay dropped telemetry, drain on process kill, or keep arbitrary broken Python code/GIL starvation responsive is added. Deleting the wait bound violates REQ-013; no inspected existing repository helper or stock processor provides this combination without changing callbacks. Standard-library coordination is therefore the minimum justified new mechanism. Private implementation layout remains implementation-owned.

**Focused closure:** Extend existing Python managed-lifecycle supporting coverage to hold the actual managed batch/exporter behind a controlled Event past a short test-only SDK bound, invoke public state flush/shutdown without replacing force-flush with an immediate result, and observe 504 plus the native result while export is still held. The existing not-started native error can prove the flush drain attempt without a server; shutdown's native outcome must also be checked. Before release, prove a new correlated span can start/end without waiting on that export; after release, prove a fresh real flush exports it and preserves the global provider/active span. Exercise repeated/overlapping timeout attempts to establish bounded in-flight work. Release/join controlled work in test cleanup. Run `mise exec -- uv run --directory sdks/wyrd-sdk-python python -m pytest -q tests/unit/state/test_observe_surface.py::test_managed_telemetry_lifecycle tests/unit/state/test_observe_surface.py::test_owned_provider_correlation`; then use FIND-TASK-001-3's existing managed journey for normal successful export. No live outage test, new transport fixture or broad aggregate is required.

### FIND-TASK-001-6 — CONFIRMED — INCORRECT: overlapping TS setup rejects the same state's provider

**Source:** CONC-001 (unique claim independently validated).

**Locations:** TS `wyrd/src/index.ts:2880-2891`; synchronous installer `wyrd/src/otel.ts:96-107`; existing setup proof `wyrd/tests/unit/telemetry.test.ts:50-75`.

**Obligations:** REQ-012/AC-008 distinguish same-state idempotence from other-owner conflict.

**Reachability/consequence:** `Promise.all([state.startTelemetry(), state.startTelemetry()])` runs both initial `#telemetry` checks before either dynamic import await resumes. The first continuation installs/stores the provider; the second does not recheck, constructs another provider, loses global registration, releases its own loser and throws 409. Even a cached import is asynchronous. This rejects ordinary shared-state startup, although provider identity remains safe; it is not a credential mixup or duplicate-export claim. Existing proof repeats only after awaiting the first installation.

**Decision-complete correction:** Recheck that same state's installed-provider field after the existing import await and before synchronous installation. No await exists between that point, registration and assignment, so this closes the local race. Preserve other-state/application refusal and loser cleanup. No registry, mutex, cached startup promise or new public state is needed.

**Focused closure:** Extend the existing setup unit case to start both same-state calls before awaiting, require both success and one stable global provider, and retain the existing other-state/application refusal and cleanup assertions. Run `mise exec -- pnpm --dir sdks/wyrd-sdk-ts/wyrd exec vitest run tests/unit/telemetry.test.ts -t '^telemetry setup$'`.

## Reconciled complete acceptance matrix

PASS means source-supported within this review and the stated proof limits; it does not imply every historical lane was rerun. FAIL identifies only the retained boundary, not failure of adjacent passing behavior.

| Requirement / constraint / acceptance | Implementation source | Proof and reconciled result |
|---|---|---|
| REQ-010 explicit registration; optional imports/dependencies | Both bridge owners; Python optional imports; TS root declaration | Callback/setup tests pass locally; **FAIL** optional TS declaration (1). |
| REQ-010 IDs, ancestry/fallback, valid/missing timing, duplicate/missing/late callbacks, safe shutdown | Python `otel.py:293-408`; TS `index.ts:2517-2594` | U4/T4 and real ancestry journeys support these individual obligations: PASS. |
| REQ-010 model/provider/operation/tool/usage/status when supplied; final reads and start-time scope | `_gen_ai`, `agentsGenAi`, bridge start/end | **FAIL** safe Responses usage (4); supplied generation/model/tool/status and scope cases otherwise PASS. Absent upstream metadata remains omitted. |
| REQ-010 no bridge provider lifecycle or content capture | Bridge flush/shutdown and allowlisted metadata | U4/T4, privacy assertions: PASS. |
| REQ-011 native Pydantic/ADK, explicit OpenAI; generic managed/owned pipeline, optional Run and independent external provider | Actual Python builders; TS real model; exporter/correlation owners | J1/J2/K1/K2 real execution and J3/K3 uncorrelated cases: PASS. |
| REQ-012 same-state idempotence, refusal/preservation of existing providers, failed-attempt cleanup | Python installer/OnceLock; TS state setup/installer | **FAIL** overlapping same-state TS start (6); sequential/idempotent/refusal/cleanup otherwise PASS. |
| REQ-012 owned composition/retryable Python install/TS construction factory; sampling/resources/context/exporters preserved | Python install lock/marker rollback; TS correlation factory | U2/T2, owned real journeys and always-off sampling: PASS. |
| REQ-013 bounded OTEL first then native drain, scrubbed stable precedence | Python `flush_telemetry`; TS lifecycle; shared `TelemetryFlushFailure`/catalog | **FAIL** Python effective deadline (5). Returned false/exception and native error precedence PASS under fresh U3/T3/Rust settlement proof. |
| REQ-013 active/global provider survival and successful live state flush; native startup/closed refusal retained | Both state lifecycle methods; existing native Bifrost lifecycle | **FAIL** required successful journey/provider continuation proof (3); no provider shutdown in state source, existing native refusals retained. |
| REQ-013 application-owned provider remains application-owned; Run owns no span/lifecycle | State retains only its own managed provider; Run hooks | Owned-provider tests/source: PASS. |
| REQ-014 native IDs/links/events/attributes/resources/scope, nesting/concurrency/explicit context, upstream sampling/content policy and version honesty | Existing OTLP path unchanged; correlation processors, framework configuration and docs | Source plus U2/U4/T2/T4, J3/J4/K3/K4: PASS for these obligations. Bridge missing supplied usage is accounted under REQ-010, not loss of native OTLP fidelity. |
| REQ-015 actual prerequisites/construction for three Python/TS recipes and success readback | Observe page/framework sections | **FAIL** executable construction (2); existing state/query setup alone cannot supply it. |
| REQ-015 provider modes/conflicts, versions, content/sampling, add-vs-replace, failures, links, draft/frontmatter and lasting architecture | Observe/Connect/Understand/Quickstart changes; design 672-685 | Static/source and recorded docs generation/check support these subobligations: PASS. |
| REQ-016 actual frameworks/local endpoints/real server/query, model/tool success/error, stream, correlation/ancestry, both modes, negative tenant/export/content and no duplicates | Real Python app/tests and TS integration owner | Source establishes actual framework execution and selected assertions: PASS except live flush proof (3), hence REQ-016 **FAIL** overall. |
| REQ-016 callback/thread/retry/import supporting coverage and owner/capability collection | Unit owners; dev dependencies; mise Bifrost leaves and normal Python collection | Fresh 14 Python/5 TS plus recorded journey collection counts and source: PASS for existing scenarios; specific missing cases remain 1/3/4/5/6. |
| REQ-017 merged ambient credentials/current-token path, gateway auth, no copied token/foreign provider leak, isolated configuration and no-credential refusal | Python `_session`; TS config; existing stock http_auth/httpAuth; gateway fixtures | U5/T5 fresh-token HTTP exports, J5/K5 gateway and negative cases, security trace: PASS. |
| INV-001 tenant/publisher authentication, authorized Card assertions, scoped reads, shared refresh | Existing ingest/Gate/auth/query owners and exporter client | Security report source paths plus negative journey assertions independently matched to producers: PASS; no new authority owner. |
| INV-002 no new protocol/table/route/kind/runtime/registry/discovery/patching/export retry queue; preserve Rust/log/metrics | Complete cumulative inventory and runtime/manifests | PASS. New metadata bridge is narrow; error value reuses shared catalog; no server changes. |
| INV-003 optional enrichment preserves results/exceptions and excludes payload/secrets | Existing fail-open Run hooks; metadata allowlists; fixed error status/type-only settlement | Callback/privacy/error sentinel and real failure paths: PASS. Deadline implementation defect belongs to explicit lifecycle calls, which may fail. |
| INV-004 explicit provider owner, no adoption, no Run lifecycle | Setup refusal/retained managed field; owned paths | PASS. Same-state startup race remains (6), without silently adopting another identity. |
| AC-001 real three Python plus TS model/tool readback | J1/J2/K1/K2 | PASS for actual successful executions; this does not satisfy live flush AC-009. |
| AC-003 relevant checks/types/generated/lane closure | Manifests/mise/generated errors/declarations and task evidence | **FAIL** clean generic declaration contract (1). Other recorded gates and fresh focused checks have the limits below. |
| AC-007 bridge fidelity/start-time identity/optional imports | Both bridge owners | **FAIL** (1,4); remaining callback/ancestry/timing/scope obligations PASS. |
| AC-008 managed idempotence/owned preservation | Setup and owned paths | **FAIL** overlapping same-state TS setup (6); owned-provider preservation remains PASS. |
| AC-009 live flush/failure/native drain/active/global provider | Lifecycle owners and tests | **FAIL** (3,5); returned-failure conversion and native precedence PASS. |
| AC-010 generic/nested/concurrent/enclosing/error/stream/no-duplicate identities | Real framework edge journeys, unit callback cases | PASS for delivered paths, without promising arbitrary thread/detached propagation. |
| AC-011 sampling/content/auth/tenant isolation | Existing sampling, upstream controls, allowlist and auth owners | PASS. Safe usage omitted under content suppression is finding 4/AC-007; it does not prove content leakage or failed tenant isolation. |
| AC-012 executable recipes/current authority/generated docs | Docsite/design | **FAIL** (2); draft/current authority/generation discipline otherwise PASS. |
| AC-013 ambient gateway/refusal/freshness | Existing auth owners and J5/K5/U5/T5 | PASS. |
| Scope: Python three frameworks, TS OpenAI only, preserve Rust generic behavior and no new server durable boundary | Cumulative changed-file inventory | PASS; no Pydantic/ADK adapter or Rust framework runtime. |
| Scope: optional production framework deps, owner-directed Python floor/dev deps | Python package/ABI/lock, TS optional peer | Production optionality has declaration exception (1); Python floor/development collection changes are authorized, not findings. |

## Reuse, proof limits, and completion

The bridges retain meaningful live callback identity and therefore earn their maps/locks; native Pydantic/ADK integrations correctly reuse native OTEL instead of adding adapters. Exporters reuse stock serialization and shared current-token ownership. The shared failure value removes duplicated error construction without moving durable behavior. Existing server auth/ingest/query and native writer ownership remain authoritative. No graph store, parser/transport replacement, new production dependency or speculative generalized framework interface is justified by the retained corrections.

For each correction deletion/reuse was considered: remove the root optional type edge; reuse demonstrated framework recipes and existing journey/package owners; add the missing safe field in the existing mapper; add a post-await check rather than new startup state. Only the ineffective stock deadline needs minimal additional lifecycle coordination, because neither deletion nor safe stock composition meets the approved bound. That coordination must not become a second batch/export queue, retry engine or callback scheduler.

Fresh orchestrator evidence in `verification.md`: Python 14 passed, TS 5 passed, exact Rust settlement test 1 passed, cumulative diff check clean. Those tests do not cover clean-consumer declarations, true blocked stock flush, direct Responses safe usage, overlapping setup or live state-flush readback. Historical broad integration/docs/type/lint/codegen claims remain recorded evidence, not fresh execution by this validator. I ran only the tiny read-only installed-dependency probe described above; no full tests/builds, installs, live services, credentials, package/source mutations or external research were performed. Probe code ran through the existing Python environment with bytecode disabled; controlled exporter work was released and cleaned up.

All nine discovery reports and every unique material proposal have a disposition; security contributes a validated empty proposal set. No proposal remains unresolved, and no prior finding closure is assumed. The final ledger supports one bounded remediation packet under r2, **FIX_REQUIRED**, with FIND-TASK-001-1 through FIND-TASK-001-6. No additional product, public API, security, durable-data or provider-ownership decision is needed. HEAD was checked as the pinned candidate and tracked source/spec/task diffs were clean at completion; only review artifacts were untracked.

Skipped broad re-verification and source changes. Remaining risks are the six specific implementation/documentation/proof gaps above, including the ineffective Python lifecycle deadline.
