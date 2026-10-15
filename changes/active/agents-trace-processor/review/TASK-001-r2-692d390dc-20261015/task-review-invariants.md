# TASK-001 invariant implementation review

Result: **FAIL**

## Immutable subject and scope

- Base: `dac4c340f835676adc29f3dcc01f98e1d3d51465`.
- Candidate: `692d390dc40e43c6e1d7e25c7850d5c826d26a9e`.
- Authority: approved `changes/active/agents-trace-processor/spec.md` revision 2 and original `tasks/TASK-001-framework-agnostic-telemetry.md`, under AGENTS.md, architecture/agent-rules.md, architecture/wyrd-design.md (authentication/export and observation identity), architecture/wyrd-doctrine.mdx, reference router, spec-driven-development, maintainer-style, telemetry-observations, testing-workflows, PyO3 boundaries, error projection, docs/CONTRIBUTING.md and docs/PRODUCT.md.
- Independent review of the complete cumulative change; no other reviewer reports read. CodeGraph was consulted before source navigation. No source edits or tests/builds performed.

## Values, owners, and workflow comparison

| Producer → authority → consumers | Investigation and conclusion |
|---|---|
| Ambient credentials → shared WyrdClient → exporter/session and stock gateway client | Python `_session` obtains `client.access_token()` on each request; TS OTLP exporter headers obtain `client.accessToken()` per export. Runtime adapters do not derive identity from Run/Card. Gateway journeys use existing `http_auth`/`httpAuth`, and check that the local upstream receives only the configured provider credential. No new authentication owner or token cache. |
| Run scope → OTEL context → correlation processor → OTLP → existing authorized ingest/Bifrost readback | Python uses the existing context stack and installer; TS exposes its existing processor via a stateless construction-time factory. Nested/concurrent scope and captured-context tests cover consumers. Native instrumentation remains unmodified; IDs/resources/events/links stay on the existing OTLP path. |
| OpenAI callbacks → bridge live-span map → global OTEL provider → existing exporter | Both bridges own only callback-lifetime mapping, preserve enclosing context and known framework parents, ignore live duplicate starts and missing/duplicate ends, and end outstanding spans with a fixed interruption status on shutdown. They do not introduce transport, a retry queue, or a provider lifecycle. Generation and Responses data are sibling metadata producers; the Python Responses consumer omits a supplied metadata source (INV-REV-001). |
| Managed provider → runtime flush result → native drain → shared TelemetryFlushFailure → public WyrdError | Python/TS attempt provider flush before native drain; application-owned providers are not retained by state. Shared Rust settlement preserves native-only errors and adds native code as secondary detail when telemetry fails. Managed shutdown never invokes provider shutdown. Local injection proves error classification/drain attempts; successful live flush and continued export lack the required real-IO proof (INV-REV-003). |
| User recipe → framework construction/configuration → actual execution → query | Framework journey builders demonstrate the necessary construction using existing framework APIs. The docsite omits this construction and prerequisites for the new recipes (INV-REV-002). Reuse these demonstrated workflows; no new tutorial framework or API is needed. |
| Optional framework/package dependencies → installed SDK → normal integration collection/capability leaves | Optional runtime dependencies remain optional; framework development dependencies now collect normally and both Bifrost leaves include the new journeys. Python floor/abi3 change is recorded as owner-directed task evidence, not treated as unauthorized drift. |

## Review Findings

### Important

#### INV-REV-001 — INCORRECT: Python Responses usage disappears when upstream content capture is disabled

- Governing obligation: REQ-010 maps available input/output usage at completion; REQ-014 preserves upstream content controls; AC-007 and AC-011.
- Changed location: `sdks/wyrd-sdk-python/python/wyrd/otel.py:430-435` (`_gen_ai`, response branch).
- Evidence: this branch reads only `data.response.usage`. In the installed tested OpenAI Agents 0.23 dependency, `agents/models/openai_responses.py:635-640` always writes payload-free counts to `span_response.span_data.usage`, but writes `span_data.response` only when `tracing.include_data()` is true. The streamed completion path does the same at lines 735-745. `agents/tracing/span_data.py:212-230` defines this separate `usage` field, and `agents/usage.py:436-447` serializes `input_tokens` and `output_tokens` into it. Thus these counts are supplied even while `response` is `None`. `_gen_ai` maps both counts to None and removes them from exported attributes. The current synthetic response test supplies a response object and misses this producer.
- Reachable consequence: ordinary OpenAI Responses execution configured with `RunConfig(trace_include_sensitive_data=False)` loses available token usage in Wyrd even though the framework deliberately retains that safe telemetry. No need to guess model identity or re-enable payload capture.
- Sibling trace: generation spans already use their direct `data.usage`; Responses spans must consume their own safe usage field as well. TS 0.20 does not expose the same independent Responses usage producer, so do not invent a TS field or require metadata absent upstream.
- Smallest testable correction: extend the existing Python response mapping to use supplied `ResponseSpanData.usage` for counts (with existing response usage as fallback where appropriate), leaving missing model absent and content untouched. Add a focused callback case with `response=None`, nonzero direct usage and content capture disabled; assert exact token attributes and absence of content. The existing real Responses producer identifies the correctness boundary; no new adapter or transport.

#### INV-REV-002 — MISSING: new docsite framework recipes cannot be followed from their prerequisites

- Governing obligation: REQ-015 explicitly requires actual install prerequisites and framework construction or a link to a complete prerequisite; undefined agent/provider placeholders cannot be the only usable procedure. AC-012; docs/PRODUCT.md complete executable journeys.
- Changed location: `docs/src/content/docs/build-and-verify/observe/record-a-run-and-observations.md:161-216`.
- Evidence: the Pydantic recipe imports `pydantic_ai` but only installs `wyrd-ai[otel,agents]`, never constructs its `agent` or model/provider, and calls undefined `question`. The ADK recipe has no installation or construction/session/message prerequisite and calls undefined `runner`, `user_id`, `session_id`, and `message`. Python and TS OpenAI snippets likewise receive an undefined `agent`; TS additionally imports `@openai/agents` without listing that package in the install prerequisites. Earlier page sections create a WyrdState and hydrated Cards, not framework agents. The stock-client link at line 226 explains optional gateway model routing, not construction for these three framework journeys. There is no complete prerequisite link in the new recipe section.
- Reachable consequence: a reader following the new page cannot execute the advertised native-framework or bridge workflow without reconstructing it from tests or external knowledge, which is the specific prohibited documentation outcome.
- Smallest testable correction: provide the missing framework install instructions and minimal actual model/agent/runner/session construction, or link each recipe to a complete prerequisite that supplies exactly those objects. Reuse the framework API shapes already exercised by `tests/integration/otel/frameworks/app.py` and the TS journey; keep ambient Wyrd authentication and optional external provider credentials distinct. Verify the resulting procedures against those same executed builders and run the existing docs checks. Preserve draft flags.

#### INV-REV-003 — MISSING: managed flush while alive and continued provider export are not proved through real IO

- Governing obligation: REQ-013; REQ-016 explicitly requires successful flush journeys; AC-009; task scenarios 3 and 5 require live flush/readback and continued provider usability through real IO. AGENTS.md §11 says local injection cannot substitute for the required user journey.
- Changed locations: `sdks/wyrd-sdk-python/tests/integration/otel/frameworks/app.py:338-342`; `sdks/wyrd-sdk-ts/wyrd/tests/integration/agents-trace-processor.test.ts:202-211` and `:253-254`; lifecycle unit cases in `tests/unit/state/test_observe_surface.py:796-828` and TS `tests/unit/telemetry.test.ts:126-180`.
- Evidence: every new managed framework journey terminates via `state.shutdown()` before parent/test readback. Python prints its only report after shutdown; TS performs readback only after shutdown. Neither starts Bifrost or calls state flush in these journeys. Owned cases shut down their provider before readback. The unit lifecycle cases replace force-flush and intentionally never start Bifrost; they prove native errors and that an existing active span remains recording, but never deliver a second span through the provider after state shutdown. Existing local-development workflows also read back after shutdown; observation-surface flush calls cover native rows without managed telemetry. The implementation evidence calls J1/J2/K1/K2 "read back after flush", but the source exercises shutdown.
- Reachable consequence: the change's newly added public state-flush behavior and required provider-survival behavior can regress while all asserted framework journeys remain green. A shutdown-only journey cannot establish availability of completed spans while the application continues, nor that the global export pipeline still works after the state writer closes.
- Smallest testable correction: extend existing Python/TS real-server journeys with one bounded live lifecycle scenario: start the state writer where the existing flush contract requires it, complete framework activity, state-flush, read it back before shutdown, then complete/export further activity using the same installed provider after state shutdown and read that back. Keep an active span alive across the barrier if asserting that invariant. Reuse current server/query fixtures and existing provider flush bounds; no new harness, timeout, or broad suite requirement.

### Critical

None.

### Non-blocking notes

None. No optional refactor or speculative hardening requested.

## Complete acceptance matrix

PASS below denotes source-supported behavior plus existing recorded verification where applicable, not an independent rerun. FAIL includes a required evidence gap, as defined in the approved spec.

| Obligation | Implementation evidence | Verification evidence reviewed | Result |
|---|---|---|---|
| REQ-010 explicit optional OpenAI bridges; ordinary IDs, hierarchy, timing, late metadata, safe lifecycle, no provider ownership | Python otel.py bridge and TS index.ts bridge use global tracers and live maps; Python Responses direct usage omitted | Python callback lifecycle/thread cases; TS callback lifecycle; real model/tool/stream journeys; INV-REV-001 | FAIL |
| REQ-011 generic managed/owned paths, optional Run/Card and external model calls | `_start_telemetry`, `span_exporter`; TS startTelemetry/spanExporter; native Pydantic/ADK builders | Managed and owned framework journeys; uncorrelated/direct local-provider execution | PASS |
| REQ-012 explicit provider ownership, idempotent setup, conflict resource cleanup, retryable registration, sampling preservation | Python installer lock/marker rollback and failed global-install shutdown; TS refused provider shutdown and public correlation factory | Setup/owned-provider tests; always-off journeys; import/package tests | PASS |
| REQ-013 bounded managed flush before native drain, error precedence, active/global provider survival, owned provider untouched, Run owns no lifecycle | Python state binding and TS WyrdState; shared TelemetryFlushFailure; no provider shutdown from state | Runtime-local injection covers failure branches; required successful live flush/provider continuation journey absent (INV-REV-003) | FAIL |
| REQ-014 native OTEL fidelity, scope restoration/concurrency, sampling and content controls | Existing OTLP path unchanged; correlation processors only add pair; bridge excludes payload; Python direct Responses usage overlooked | Existing correlation/fidelity tests plus framework sampling/privacy cases; INV-REV-001 | FAIL |
| REQ-015 executable docsite recipes, correct prerequisites, owned/managed setup, readback, conventions/lasting authority | Observe page and links/architecture updated; recipe construction/prerequisites incomplete | Docs check recorded; source procedure review fails INV-REV-002 | FAIL |
| REQ-016 actual frameworks/local endpoints/real server, edge/negative matrix and lifecycle proof in owning lanes | New Python subprocess app and TS framework execution; both Bifrost leaves include journeys; normal Python collection | Framework positive/edge/auth cases reviewed; live flush proof missing, INV-REV-003 | FAIL |
| REQ-017 merged ambient auth, stock-client current tokens, no credentials sent upstream, isolated configuration | Existing per-export access token hooks and stock `http_auth`/`httpAuth`; app env isolates config | Gateway journeys assert upstream header boundary, no-credential refusal; two-export token freshness tests | PASS |
| INV-001 tenant/publisher from auth; no Run/Card credential selection | No new server/auth owner; existing authorized OTLP ingest retained | Viewer/foreign-tenant empty readback and upstream credential assertions | PASS |
| INV-002 no new durable protocol/runtime/registry/patch/export queue/Pydantic or ADK adapter; existing Rust and other signals preserved | New bridge uses stock tracer; other signals/shared Rust OTLP untouched; narrow runtime boundaries | Diff/manifests and existing signal test selection inspected | PASS |
| INV-003 optional enrichment cannot change results/suppress exceptions or copy payload/secrets | Framework callback path and bridge metadata allowlist; fixed interruption status; flush cause uses type/stage | Success/failure/secret sentinel tests and denied-export app result assertions | PASS |
| INV-004 provider ownership explicit, setup never adopts, Run owns no lifecycle | Global conflict refusal, state retains only its managed provider, Run scope remains context only | Setup/owned-provider/local lifecycle tests | PASS |
| AC-001 real three Python frameworks and TS OpenAI export/readback | Actual framework/provider clients in app.py and TS agent helper | J1/J2/K1/K2 recorded; source assertions include model and tool activity | PASS |
| AC-003 relevant checks/lane closure and scoped verification | mise leaves, manifests, generated error/N-API outputs | Task records format/lint/type/codegen/boundaries/docs and positive integration counts; no independent rerun | PASS |
| AC-007 bridge metadata/start-time scope/callback edges/optional imports | Bridges plus optional imports; Python safe Responses usage missing | Callback tests cover only response object with usage; INV-REV-001 | FAIL |
| AC-008 managed idempotency and owned providers preserve configuration/export/correlation | Existing owners extended without new provider registry | Setup and owned-provider tests plus real owned journeys | PASS |
| AC-009 live flush/error/drain/provider survival | New state flush sequencing and error projection | Only failure injection and shutdown readback; INV-REV-003 | FAIL |
| AC-010 uncorrelated/nested/concurrent/ancestry/error/stream identities and no duplicates | Start-time OTEL context plus mapped parent spans | Actual framework edges, ancestry and duplicate checks; callback late-end cross-scope assertions | PASS |
| AC-011 sampling/content/auth isolation | Stock sampling; content allowlist/controls; signed auth unchanged | Always-off/content sentinels/viewer/other-tenant journeys; Python safe metadata content-control interaction fails INV-REV-001 | FAIL |
| AC-012 usable recipes/current authority/generated docs | Architecture and links updated; incomplete framework procedures | Recorded docs check does not prove executable recipes; INV-REV-002 | FAIL |
| AC-013 ambient gateway/refusal/exporter freshness | Existing selected-client token path, existing stock transport | Actual gateway journeys, no-credential cases, per-export freshness tests | PASS |

## Verification notes and limits

- Inspected complete cumulative changed-file inventory/diffs, runtime owners, direct/sibling metadata producers, lifecycle consumers, all new framework journey source and unit callback/lifecycle assertions, existing local-development/observation workflows, manifests/lane changes, error declarations and docsite procedures.
- Installed dependency source establishes the version-specific Python Responses producer: no network request or live model was needed to validate that path.
- Task evidence records broad/focused successful checks at its stated commits. It also explicitly records that the unit scenarios did not capture separate RED runs; this review does not substitute summaries for the source behavior or claim those runs were independently observed.
- Static `git diff --check` passed. No tests, builds, lints, or server processes run because verification uses a shared target and the orchestrator owns coordination.
- No prior independent finding ledger exists, so there are no prior FIND-* closure claims.
- Final HEAD recheck equals the pinned candidate; tracked-source diff is empty. Only the assigned report was written by this reviewer.
- The orchestrator separately reports fresh Python 14, TS 5, and the exact shared Rust failure-projection test passing in verification.md; these were not run by this reviewer and do not exercise the three gaps above.

Skipped independent runtime verification; the remaining risks are missing Responses usage under content suppression and unproved live lifecycle behavior.
