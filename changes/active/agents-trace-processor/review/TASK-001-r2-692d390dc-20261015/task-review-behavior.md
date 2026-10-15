# TASK-001 independent behavior acceptance review

Result: **FAIL**. Proposed findings: **BEH-001**, **BEH-002**, **BEH-003**.

## Immutable subject and scope

- Root: `/home/thorrester/Documents/GitHub/wyrd-worktrees/split/agents-trace-processor`.
- Base: `dac4c340f835676adc29f3dcc01f98e1d3d51465`.
- Candidate: `692d390dc40e43c6e1d7e25c7850d5c826d26a9e`.
- Approved authority: `changes/active/agents-trace-processor/spec.md`, revision 2, and original `tasks/TASK-001-framework-agnostic-telemetry.md`.
- Reviewed cumulative changed surfaces, their caller paths, repository agent rules, maintainer/spec-development references, telemetry-observation authority, applicable design/doctrine sections, and docsite procedure conventions. Used CodeGraph before source navigation.
- This is the behavior acceptance role only. No other discovery reports were read. Source was not edited. No Cargo work, framework tests, integrations, dependency installation, or package build was run. The orchestrator authorized one isolated existing-toolchain TypeScript noEmit reproduction.

## Caller-to-result paths inspected

1. Python application: ambient `WyrdState` → `start_telemetry` native binding → `_start_telemetry` → stock provider, correlation processor and stock authenticated exporter → existing OTLP server → `Bifrost.trace_timeline`. Native Pydantic/ADK builders and the OpenAI callback bridge emit actual model/tool activity.
2. TypeScript application: ambient `WyrdState.fromPath` → `startTelemetry` → construction-time processors → bridge callbacks → stock OTLP exporter → server → `Bifrost.traceTimeline`. Application-owned providers use the same correlation processor factory and exporter.
3. Runtime lifecycle: managed provider `force_flush`/`forceFlush` → native state drain → shared `TelemetryFlushFailure` catalog projection. Application-owned providers are absent from state lifecycle ownership. Both-stage failure preserves native code as secondary detail.
4. Framework bridge: framework root/parent IDs select open OTEL parent; active context supplies enclosing ancestry and start-time Run scope; final data supplies model/usage/status; shutdown clears unfinished spans with a fixed error description. Python state mutation is lock-guarded.
5. Authentication: exporters query the selected client's existing access-token path per request. Gateway journeys use existing `http_auth`/`httpAuth`; local upstream receives only the operator provider credential.
6. Documentation consumer: Observe recipe prerequisites → framework construction/execution → shutdown/readback. This path currently stops at undefined framework objects (BEH-002).
7. Package consumer: ordinary TypeScript application imports only `WyrdClient` → published root declaration → optional Agents peer import. This path currently fails compilation without the optional framework dependency (BEH-001).

## Acceptance matrix

PASS denotes inspected source and credible available evidence for the scoped obligation; it does not claim this reviewer reran the recorded lane.

| Requirement / acceptance / constraint | Implementation evidence | Verification evidence inspected | Result |
|---|---|---|---|
| REQ-010: explicit bridge registration, no automatic framework patching | Python `otel.py:293-389`; TS `index.ts:2517-2594`; setup registers only OTEL processors | Real framework app explicitly sets processors; TS `agent()` does likewise | PASS |
| REQ-010: optional framework dependency across public consumers | TS root `index.ts:1` leaks optional Agents types into emitted root declaration | Isolated noEmit consumer fails TS2307; pack check tests runtime only | FAIL — BEH-001 |
| REQ-010: valid OTEL IDs, known parent/root fallback, enclosing ancestry | Both bridges call the OTEL tracer; `_start`/`#start` resolve open framework parent/root into active context | U4/T4 inspect exact parent IDs; J1/K1 assert connected enclosing ancestry | PASS |
| REQ-010: valid/missing/invalid timing, late model/usage/status, unknown span type | Python `_nanos`, `_gen_ai`; TS `agentsTime`, `agentsGenAi`; final attributes read on end | U4/T4 supplied timestamp and late-generation/response/custom cases | PASS |
| REQ-010: start-time attribution, duplicates/missing callbacks, unfinished shutdown, late callbacks | Both bridges retain created span per live ID; completion does not reread scope; shutdown clears state | U4/T4 duplicate, orphan-end, scope change and interrupted cases; Python thread-safety test | PASS |
| REQ-010: adapter flush/shutdown do not own provider, no default payload capture | Bridge `force_flush`/`forceFlush` no-op; shutdown ends spans only; attribute allowlist | U4/T4 sentinel content and status checks | PASS |
| REQ-011 / AC-001: real Python Pydantic AI, ADK, OpenAI; TS OpenAI; no manual internal spans | Python app builders `app.py:150-229`; TS stock `OpenAIChatCompletionsModel` at integration test `:93-111` | J1/J2 parameterize all 3 frameworks; K1/K2 model/tool count, identity and model assertions | PASS |
| REQ-011: generic telemetry without Run and authenticated publisher retained | No server identity changes; exporters use client authentication and omit correlation outside scope | J3/K3 uncorrelated real executions, existing OTLP identity contracts | PASS |
| REQ-012 / AC-008: managed idempotency, existing provider refusal, failed-attempt cleanup | Python `_start_telemetry:230-240`, binding OnceLock; TS `otel.ts:96-107` and state provider field | U1/T1 repeat, other-state/application and cleanup assertions | PASS |
| REQ-012: retryable/idempotent Python owned installation; TS construction-time factory | `install_run_correlation:107-137`; `runCorrelationProcessor:83` returns same stateless processor | U2/T2 retry/settings/sampling/correlation; J2/K2 real owned export | PASS |
| REQ-012 / INV-004: preserve owned lifecycle, sampler/resource/context/exporters | Owned setup is explicit composition; state flush only accesses its retained managed provider | U2/T2 resource/sampler preservation; Python application-provider no-flush test | PASS |
| REQ-013: OTEL first, native drain even after failure, stable primary/secondary errors | Python binding `flush:278-281`, `shutdown:299-302`, helper `:509-531`; TS `flush:2924-2927`, `shutdown:2945-2948`; shared `state.rs:2098-2163` | U3/T3 deterministic failure injections and native codes; shared catalog projection tests | PASS |
| REQ-013: flush active-span/global-provider compatibility, no added timeout option/queue/retry | State invokes configured provider flush, never provider shutdown; run scope unchanged | U3/T3 assert active span still recording and provider identity retained | PASS |
| REQ-013 / REQ-016 / AC-009: successful state flush visible while application lives; continued real export | Production sequencing exists, but new Python app ends with shutdown and TS real framework tests likewise use shutdown | No managed state.flush successful readback journey; U3/T3 mock forceFlush; older Python observation journey explicitly flushes its own provider | FAIL — BEH-003 |
| REQ-014: native IDs/links/events/attributes/resources/scope preserve existing OTLP representation | No server ingest/storage/query or stock serialization changes; Wyrd processor adds only correlation | Existing OTLP regression ownership retained; J1/K1 ancestry/model data; final integrated Rust OTLP verification remains change-review work | PASS |
| REQ-014 / AC-010: nested/concurrent, uncorrelated, streaming, error identity | Existing Run context owners retained; bridges use current context at span start | J3/K3 real framework edge executions plus U2/U4/T2/T4 | PASS |
| REQ-014 / AC-011: upstream sampling/content controls, no universal-redaction claim | Pydantic include_content=False; ADK env capture switch; bridge payload allowlist | J4/K4 real sampling and content-sentinel readback; docs disclose residual native metadata | PASS |
| REQ-015 / AC-012: complete construction/prerequisites for each documented framework recipe | Observe `:165-216` consumes undefined `agent`, `runner`, session/message variables; framework package prerequisites omitted for Pydantic/ADK | Real app fixtures contain construction, but docs neither include nor link these complete prerequisites | FAIL — BEH-002 |
| REQ-015: ambient setup, both provider modes, readback, versions, conflicts, sampling/content, flush errors, draft state | Observe added sections plus Connect/Understand links; architecture `:672-685`; draft preserved | Recorded docs:generate/docs:check; static source review | PASS |
| REQ-016 / AC-003: owner/capability lane collection with required dependencies | Python dev dependencies include all frameworks; default integration no longer excludes MCP folder; Bifrost leaves add framework directories/TS file | Recorded positive J/K counts and 266 Python integration passes; mise command inspection confirms selection | PASS |
| AC-003: usable public types and affected static proof | Existing workspace static checks see installed Agents peer and therefore miss clean consumer | Reproduction in BEH-001 contradicts complete public type closure | FAIL — BEH-001 |
| REQ-017 / AC-013: ambient client selection and existing shared refresh, no copied token/cache | Python session auth invokes client.access_token per request; TS config headers async invoke client.accessToken | U5/T5 per-export freshness; J1–J5/K1–K5 ambient setup | PASS |
| REQ-017: stock gateway transports; no Wyrd credential forwarded externally; no credential refusal | Python gateway builder `app.py:232-251`; TS K5 `:325-350` | J5/K5 assert local upstream provider bearer and absence of x-wyrd headers; credential-negative case; R1/R2 retained | PASS |
| INV-001 / AC-011: tenant/publisher comes from authentication; foreign Card denied; reads tenant-scoped | No server auth changes; framework attributes do not select client identity | J4 denies viewer and foreign-tenant Card; K4 checks foreign tenant only sees its uncorrelated root | PASS |
| INV-002: no durable protocol, registry/runtime/discovery/patching, adapter beyond approved OpenAI bridge | Cumulative production diff confined to SDK bridge/correlation/lifecycle and shared error projection | Changed-file/diff inspection; recorded check:deps | PASS |
| INV-002: preserve Rust OTLP/log/metric behavior | No Rust exporter/provider ownership changes; existing Python/TS log/metric factories retained | Existing signal tests still selected in Bifrost leaves; final integrated regression run not performed here | PASS |
| INV-003: enrichment cannot suppress application exceptions, no new secret payload metadata | Existing run hooks swallow enrichment errors; bridge payload allowlist; framework callback dispatcher catches processor exceptions | Python unit run exception/failing-enrichment regressions; U4/T4; inspected installed Python framework dispatcher | PASS |
| INV-004: state does not adopt existing provider; Run owns no provider lifecycle | Provider refusal and retained managed ownership; Run only updates context | U1/T1, U3/T3; existing Run tests | PASS |
| Non-goal: no server routes/tables/migrations/Card kinds, durable payload, auth subsystem, new queue/retry, Rust framework adapter | Cumulative changed-file list and implementation diff | Static diff audit | PASS |
| Non-goal: framework production deps remain optional; no new unrelated production dependency | Python `agents` extra and TS optional peer; test deps are dev only | Runtime clean-package import exists, but declaration leakage remains BEH-001 | FAIL — BEH-001 |
| Approved later owner direction: framework dependencies move to dev; Python >=3.11 | pyproject/Cargo abi3-py311 and lane changes | Task evidence explicitly records owner direction; not treated as unapproved scope drift | PASS |

## Proposed findings

### BEH-001 — REGRESSION / Important — optional Agents peer is required to compile an ordinary TS consumer

- **Obligation:** REQ-010 says framework dependencies remain optional; task Scenario 1 requires generic imports without framework packages; AC-003 includes public typing closure.
- **Exact changed location:** `sdks/wyrd-sdk-ts/wyrd/src/index.ts:1`. Emitted public consequence: `sdks/wyrd-sdk-ts/wyrd/dist/index.d.ts:1`, and bridge declaration at `:1287-1296`.
- **Producer → consumer / reachability:** new root import uses Agents SDK types in the exported class's implemented interface and callback signatures. TypeScript retains that import in the root declaration. `package.json:47` and `:57-59` declare the peer optional. A normal application that installs only wyrd-ai and imports `WyrdClient` still resolves the full root declaration, and cannot resolve that peer. The root package's ordinary state/Cards/Bifrost consumers are sibling affected callers; this is not restricted to bridge users. `scripts/check_ts_package.mjs:145-152` checks runtime import only, where type erasure hides the problem.
- **Validated consequence:** existing TypeScript compiler returns one TS2307 for `@openai/agents-core` in root dist/index.d.ts, even though the consumer does not use telemetry. Users must install an unrelated framework or weaken declaration checking to use the generic SDK.
- **Smallest testable correction:** keep optional framework types out of the generic root declaration's required dependency graph. Use the narrow structural callback contract actually consumed by this local bridge, preserving compatibility with the real SDK and its real-framework tests, rather than making the optional framework mandatory. Do not move transport/auth/provider ownership or add another package. Extend the existing clean-package verification with an ordinary noEmit consumer that imports a generic root API without optional peers and with declaration checking enabled.
- **Proof performed:** copied built `dist`, package.json and root authored/generated declaration files into `/tmp/wyrd-optional-types.ap09jh/node_modules/wyrd-ai`; symlinked only installed mandatory apache-arrow and existing @types/node; consumer.ts contains `import { WyrdClient } from "wyrd-ai"; WyrdClient.connect();`. No install/build/source mutation.

```sh
mise exec -- node sdks/wyrd-sdk-ts/node_modules/typescript/bin/tsc --noEmit --skipLibCheck false --module NodeNext --target ES2022 /tmp/wyrd-optional-types.ap09jh/consumer.ts
```

Observed exit 2:

```text
../../../../../../../tmp/wyrd-optional-types.ap09jh/node_modules/wyrd-ai/dist/index.d.ts(1,76): error TS2307: Cannot find module '@openai/agents-core' or its corresponding type declarations.
```

Initial command used a nonexistent wyrd/node_modules/typescript path and failed before compilation; the corrected command above uses the existing workspace compiler. No test/build failure was diagnosed from that setup error.

### BEH-002 — MISSING / Important — framework procedures still omit their required construction

- **Obligation:** REQ-015 explicitly requires actual framework construction or a link to a complete prerequisite and says undefined agent/provider placeholders cannot be the only usable procedure; AC-012 requires following the recipes to success.
- **Exact changed location:** `docs/src/content/docs/build-and-verify/observe/record-a-run-and-observations.md:165-216`, especially ADK `:182-187`, Python OpenAI `:195-203`, and TS OpenAI `:209-216`; install instructions at `:159`.
- **Producer → consumer / reachability:** a reader follows the page's valid bundle/state prerequisites, then each added framework section. Pydantic imports Agent but never constructs agent; ADK supplies no LlmAgent, model, runner, session, message or install prerequisites; OpenAI Python/TS likewise call an undefined agent. The only nearby stock-client link targets its OpenAI section, which constructs an HTTP client and says to pass it to a model 'as usual', without constructing an Agents SDK agent or an ADK runner. It cannot complete these procedures.
- **Consequence:** the explicitly requested onboarding recipes cannot execute from the documented prerequisites. Tests construct these objects separately, so successful framework journeys do not establish that a user can follow the published procedure.
- **Smallest testable correction:** complete the existing recipe sections (or link directly to complete framework-specific construction prerequisites) with package install commands, model/provider client, agent/tools and ADK runner/session/message as needed. Reuse the working actual-framework construction in `tests/integration/otel/frameworks/app.py:150-251` and the TS journey's `agent()`; retain ambient Wyrd export auth and keep external model credentials separate. No new integration architecture or full tutorial subsystem is needed. Walk the updated recipe through the corresponding existing real journey and docs checks.

### BEH-003 — MISSING / Important — no real managed state.flush readback despite explicit acceptance

- **Obligation:** REQ-016 requires flush while alive; AC-009 requires flush visibility while alive and active/global-provider survival; original task Scenario 3 names real readback and continued provider usability, and Scenario 5 explicitly requires real model/tool execution followed by flush while alive. Runtime-local failure injection is permitted, but successful flush still requires a journey.
- **Exact changed locations:** `sdks/wyrd-sdk-python/tests/integration/otel/frameworks/app.py:338-342`; `sdks/wyrd-sdk-ts/wyrd/tests/integration/agents-trace-processor.test.ts:203-211` (same shutdown-only pattern in edges/gateway). Unit locations: Python `test_observe_surface.py:768-850`, TS `telemetry.test.ts` managed-lifecycle case.
- **Producer → consumer / reachability:** every Python real-framework subprocess completes and invokes state.shutdown (owned path invokes provider.shutdown), then parent process reads. TS managed real-framework tests similarly call state.shutdown before reading. Neither exercises successful state.flush with a live managed provider and native writer. Existing Python observation integration explicitly invokes its application-owned provider.force_flush before state.flush (`observe/test_observation_surface.py:234-235`), so it cannot catch omission of managed provider flushing. U3/T3 replace forceFlush with a stub and leave Bifrost unstarted, so successful state.flush only reports BIFROST_NOT_STARTED. Existing broader local-development/action journeys also shut down before readback.
- **Consequence:** a regression that removes the new managed-provider flush from state.flush, or breaks export after a state lifecycle boundary, still passes the added real-framework suite; this is the exact cross-boundary behavior the approved evidence was required to establish. This is a coverage/acceptance gap, not a claim that inspected production ordering is currently wrong.
- **Smallest testable correction:** extend the existing Python/TS real-framework journey owners to start native Bifrost where a successful state.flush requires it, execute the actual framework, state.flush, and query/read back before shutdown while an active span remains active. End that span / execute a second turn and query after another drain to prove continued provider usability. Include the documented state.shutdown compatibility by exporting after state shutdown and application-owned provider flush if needed. Reuse existing subprocess/server/provider fixtures and normal lane selection; do not add a new harness or broad suite. Existing error-injection tests remain appropriate for deterministic failure branches.

## Verification assessment and immutability

Task evidence reports U1–U5/T1–T5, J1–J5/K1–K5, existing auth regressions, Python 266 integration cases, and format/lint/type/codegen/docs/boundary checks. Source confirms real stock framework execution against local models, positive framework parametrization, real server/query APIs, and lane inclusion. These support substantial acceptance, but do not cover the three findings above. The recorded claim that J1/J2/K1/K2 prove managed state.flush readback is contradicted by the actual shutdown-only call sites.

No unrelated refactor is requested. The existing exporter, correlation owner, native drain, error catalog and auth owner are sufficient; no replacement orchestration is warranted. Existing Python 3.11 floor change is recorded as owner-directed, not independently treated as a defect.

HEAD remained the pinned candidate during inspection; tracked source remained unchanged. Only this assigned review report was written by this reviewer. Full integration/build lanes and real provider/network failures were not rerun; current review combines source, the committed task evidence and the authorized isolated declaration reproduction. Final integrated capability/shared-infrastructure proof remains the orchestrator/change-review responsibility.
