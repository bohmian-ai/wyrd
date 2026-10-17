# Provider wire and hosted Agent boundary review

**Result: PASS. Proposed finding ledger: empty.** FIND-TASK-005-12 and FIND-TASK-005-13 are closed within their approved scope.

## Immutable subject and authority

Reviewed only `/tmp/wyrd-task005-review-02848d737`, HEAD `02848d737a621938ec0b5f817e1aa84d5f3df1fd`, against base exclusive `f2a726c90edbf8956c7b91c843379175ee20eac0`. Inspected cumulative provider/Agent diff; used `1fa4526cc...02848d737` to locate R2 repairs. Final HEAD matched and detached checkout status was clean. No CodeGraph directory exists there. No source changes were made.

Authority: frozen AGENTS.md, agent-rules, reference router, spec-driven-development, maintainer-style, architecture patterns, errors and evaluation references; applicable Prompt/Agent/Verifier portions of wyrd-design and doctrine; approved spec r9 (REQ-004/005/007/008); original TASK-005, R1 remediation, R2 remediation and committed evidence; frozen prior R2 validation ledger; S1 supplemental task and findings. Prior findings were closure hypotheses. Main integration was not treated as task drift. Draft agent-runtime-composition creates no requirements.

## Boundary and source coverage

| Surface | Source inspected |
|---|---|
| Google authoring | skald-prompt builder.rs: gemini, vertex, google_prompt, google_settings, effective format and regressions; both public builders converge before native compilation. |
| Portable/native lowering | skald-spec authoring.rs, prompt.rs, request.rs and wire/google_generate.rs: JSON Schema output/tools, optional role, derived shape, schema error attribution, reasoning-effort mapping/refusal. |
| Hosted composition | server verification/hosted/blocks.rs, Vala blocks/agent.rs, server components/gateway/caller.rs: accepted caller, selected route, exact Agent/Prompt resolution, binding/media/tools, response decoding and error classification. |
| Typed continuation | skald-spec adapter.rs; skald-agent loop_runtime.rs and request_builder.rs: call extraction, native assistant history, tool execution/results and next request reconstruction. |
| Affected projections | Every FunctionCall constructor/match in skald-spec convert/lib, skald-tool helpers, skald-prompt Python Google projection, gateway Google adapter; gateway stream consumer. |
| Ingress/translation | Gateway Google adapter/tests, server gateway ingress/caller/invocation, native passthrough. Translated Google-to-OpenAI is distinct from hosted native history. |
| Python/smoke | Prompt authored-JSON conversion and public Python tests; Google parameter-schema getter; S1 conftest/live Prompt/gateway cases, recorded findings and relevant mise ownership. |

## Acceptance and prior-finding closure

| Obligation | Implementation evidence | Verification assessment | Result |
|---|---|---|---|
| FIND-12 effective schema overrides native response_schema for Gemini and Vertex | builder.rs:235–277,409–434: both builders share google_settings; JsonSchema clears response_schema before assigning response_json_schema, preserving cloned unrelated settings. Without override native config remains. output and response_format resolve to the same effective format. | google_output_schema_replaces_native_response_schema at :743 serializes both builders, asserts absence of old field, exact schema and temperature retention. R2 records exact focused GREEN and 10-case builder slice. | PASS / CLOSED |
| FIND-13 both signature spellings and unsigned control survive typed history | google_generate.rs:121–140 stores optional opaque signature, aliases camelCase, and omits absent values. AnswerContent conversion moves complete parts. request_builder.rs:166–177,236–245 copies/replays native candidate content. loop_runtime.rs:464–480 appends native assistant before extracting tools; separate tool results preserve original order. | loop_gemini.rs:155–219 drives actual Agent gateway dispatch/tool/history loop. Double serializes outgoing request and deserializes buffered JSON to the same ProviderResponse type used by ServerWyrdGatewayCaller. Covers both spellings, unsigned, text/call order, second request, one tool execution and matching result. R2 records exact GREEN and neighboring Agent/spec/tool/gateway slices. | PASS / CLOSED |
| Every affected constructor/adapter remains coherent | Foreign-to-Google constructors/helpers initialize None; extractors match with `..`; foreign-dialect conversions fabricate no signatures. Python getter borrows the retained request and still projects name/args. No new language API is required for protocol retention. | Complete constructor/match search and native history trace; supplied all-feature lints/codegen evidence. | PASS |
| Required hosted consumer works without draft composition | HostedBlocks composes AgentExecutor and ServerWyrdGatewayCaller. Executor resolves/binds Prompt, applies whole route override, attaches built-ins and runs existing loop. Server caller serializes native requests and typed-decodes buffered native responses. Native forwarding preserves signature-containing body, bypassing lossy translated response adapter. Google/Vertex share body/history dialect; initial deployment compatibility is constrained by Prompt provider. | Source establishes production reachability independently of native-only smoke. Focused mock exercises the typed state transition that previously lost metadata. No fresh real hosted upstream test claimed. | PASS |
| Cumulative schema/tool/error lowering | Portable output and tool declarations emit JSON Schema via existing owners; response shape prefers new schema then native legacy schema. Gateway translated requests reuse function-declaration owner. Python parameters uses parameter_schema. Malformed authored JSON maps to existing PromptInvalid; provider errors stay sanitized/stable and check-local errors remain inconclusive. | Authoring tests cover shared schema/settings/role; public Python refusal test checks stable code. Supplied smoke covers common portable output/tool cases, not universal provider support. | PASS |
| Ingress and S1 scope | Bare Gemini model route returns existing protocol-specific invalid-model error under Google ingress instead of Wyrd-token auth fallback. S1 is opt-in/credential-gated, records failures without xfail/skip-on-error, and excludes draft composition. | Source plus committed 42-pass/1-fail record. Jev unsupported-loop failure remains explicit and is not a new acceptance requirement. | PASS |

## Validation and limits

No material provider-domain defect remains supported by approved scope and inspected reachable paths. Fixes extend the shared representation/builder already consumed; no duplicate history, loop, transport, validator or provider abstraction was added. Generic conversion and translated OpenAI gateway paths do not carry Google signatures, but required hosted Google execution uses native passthrough and native history. Optional non-function signatures, new Jev dispatch and draft composition are excluded expressly by R2.

Cross-checked primary [Google GenerateContent reference](https://ai.google.dev/api/generate-content): optional role, opaque part-level thoughtSignature, JSON Schema fields and alternate-schema exclusivity support these corrections. No new API family or universal model claim is inferred. The old thought-signatures guide now redirects to broader thinking documentation.

Tests above are supplied committed evidence checked against source/test bodies, not fresh executions. Signature proof uses a typed recording gateway double, not ServerWyrdGatewayCaller plus a real upstream server; source confirms the same serde decode/native passthrough. This is credible focused proof of metadata-loss correction, not new live Google/Vertex conformance. The old full gate predates remediation and is not candidate gate evidence.

Skipped fresh builds/tests, broad gates, live providers/credentials and external example rebuilds. Residual limit: domain PASS does not qualify every provider/model or replace final integrated verification.
