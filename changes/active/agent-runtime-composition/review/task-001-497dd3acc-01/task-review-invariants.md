# TASK-001 invariant review

Overall: **FAIL**. Immutable subject: base `06a368906187be20364a2d959da311151964b617`, candidate `497dd3acc8fd06567a0e0ff01baf599f9af1711b`. HEAD still equals candidate. Source was not edited.

## Proposed findings

### INVREV-001 — INCORRECT: Python migrated gateway workflow drops its Run and Card

- Obligation: REQ-016, AC-009; task Scenario 1 requires preserved Run/Card capture for the migrated gateway callers.
- Location: `sdks/wyrd-sdk-python/tests/integration/test_gateway_inference.py:49–55` (`answer`), consumed at lines 209–214 and 223.
- Producer-to-sink evidence: the original `Run::invoke_on` created `PublicWyrdGatewayCaller::with_subject(Some(&self.run_id), Some(self.subject_uid()))`. The new helper renders the Prompt and authenticates using the same caller, but creates no Run and supplies no `extra_headers`. Server `components/gateway/ingress.rs::requested_subject` expressly leaves calls uncorrelated when both headers are absent. The new assertion selects only `requested_model.deployment`, so four captured rows with null Run/Card pass. The separate lone-run/lone-card test does not drive this helper and cannot prove migration preservation.
- Consequence: the migrated `test_agent_prompt_routes` application path now produces calls that cannot join the Agent Card or its Run, despite being reported as correlated migration proof. Its subject-authorization boundary is also no longer exercised on this path.
- Siblings inspected: Python support-desk `answer` and local-development `call_agent`, Rust `Inference::ask_agent`, and TS `askAgent` all attach both headers. Auth adapters do not infer subject identity; omission must be corrected at the request producer rather than the gateway sink.
- Smallest correction: create/pass the existing Agent Run in this Python workflow, attach its Run ID and exact Agent Card UID using existing headers, and extend this same journey's capture query to assert both alongside deployment and caller. Preserve deployment-denial and no-upstream-IO assertions. No SDK execution helper or server change is needed.
- Focused proof: the existing J-P `test_agent_prompt_routes` must fail on absent headers and pass only when the actual migrated calls retain Run/Card attribution.

### INVREV-002 — MISSING: TS gateway migration no longer exercises either Prompt's content

- Obligation: REQ-013/016 and task Scenario 1 require real consumer migration while preserving useful behavior/assertions; the task explicitly forbids discarding surviving assertions merely because invocation is removed.
- Location: `sdks/wyrd-sdk-ts/wyrd/tests/integration/gateway-inference.test.ts:401–403`.
- Evidence: `askAgent` reads only `state.agent(alias).prompt().spec.model` and hardcodes a fresh user `hi` message. Both `agent` and `body-agent` calls in the existing journey therefore bypass their hydrated Prompt content entirely. `fixtures/cards/gateway_inference/ask-prompt.yaml` holds portable `{{question}}`; `ask-body-prompt.yaml` holds native `body.messages` with `${question}`. The original `run.invoke({question: "hi"})` rendered both through the shared implementation. The migrated upstream assertion checks model only and an echo result; it is insensitive to either Prompt being ignored.
- Consequence: the retained body-agent branch falsely passes as proof that application-owned calls preserve the hydrated native and portable Prompt workflows. A broken/ignored binding is no longer observable in the migration proof.
- Existing owner/reuse: Rust `ask_agent` and Python migration render the actual hydrated Prompt; TypeScript currently exposes authored Prompt values but no renderer. For these concrete fixtures, application code can bind the two existing authored message shapes without adding a general renderer or SDK runtime. The checked-in TS support-desk already demonstrates application-owned binding of its concrete portable message shape.
- Smallest correction: have this test application's request use the selected Card's actual portable messages or native body, binding its declared placeholder; assert upstream message payload as well as model for both aliases. Preserve existing auth/correlation handling and denial checks. Do not introduce a public Prompt execution API.
- Focused proof: existing J-T `agent prompt routes`, with distinguishable fixture content/assertions so replacing the Prompt with hardcoded `hi` fails.

### INVREV-003 — MISSING: runtime-only mock response constructor remains public

- Obligation: REQ-013 and AC-007 remove runtime-exclusive execution-result/mock helpers and their native/public/typing exposure, not just the `agent` module.
- Location: `crates/skald/skald-prompt/src/python/response.rs:40–49`; exposed through surviving `register_prompt_provider_classes` and `sdks/wyrd-sdk-python/python/wyrd/prompt/__init__.py:22`, root `__init__.py`, authored stub `prompt/__init__.pyi:260–278`.
- Evidence: `ProviderResponse.text` still constructs the deterministic mock response. Its Rust and public stub docs explicitly say to return it from `after_model_callback`, an API deleted by this candidate. Repository call search finds no surviving use of `PyProviderResponse::from_arc` outside its definition; removed Agent run/callback glue was the producer/consumer. Existing Prompt authoring/render returns a ProviderRequest and does not consume this response replacement constructor.
- Consequence: shipped package still exposes a runtime-only mock callback companion and guidance referring to a deleted lifecycle, contrary to the promised complete public deletion.
- Smallest correction: remove this mock/callback-only public constructor and its authored documentation/declaration. Inspect its response-only projections for the same exclusivity while retaining request/message projections and Prompt authoring. Do not remove shared Rust provider responses used by internal Skald/Vala. There is no need for replacement machinery.
- Focused proof: public import/native/stub inspection plus existing Python package/export/type checks should prove the mock callback helper absent and Prompt rendering still available.

### INVREV-004 — VIOLATION: deleted smoke file included surviving Prompt contracts

- Obligation: task Verification and Evidence: retire tests only when their sole contract is the intentionally deleted API; preserve meaningful coverage elsewhere. REQ-013 expressly preserves Prompt authoring.
- Location: deleted `sdks/wyrd-sdk-python/tests/smoke/test_prompt_live_providers.py` (base lines 122–251, especially `test_portable_system_and_history`, `test_portable_max_output_tokens`, `test_portable_response_format`, and native-format cases).
- Evidence: the file describes its purpose as proving authored portable/native Prompts lower to requests live models accept; it deliberately bypasses gateway to distinguish Prompt defects from gateway defects. Agent was its execution vehicle. The local-tool loop case is exclusively retired behavior and can be removed. The text/system/history/settings/schema and native-body cases exercise surviving Prompt lowering. Candidate deletes all 251 lines and records 'execution vehicle was runtime Agent; no live-provider replacement'. Surviving `test_prompt_render.py` checks rendering locally; `test_gateway_live_providers.py` uses independent native bodies and does not consume authored Prompt output. Neither preserves this specific proof.
- Consequence: acceptance evidence for surviving provider-native Prompt lowering is removed wholesale, even though a stock client can consume its rendered request. This is explicitly disallowed test loss, not a request for new optional live coverage.
- Smallest correction: retain the surviving authored-Prompt smoke cases using application-owned native requests through already-installed clients; retire the local-tool/Agent-result-only assertions. Preserve opt-in credential gating and keep this out of fast unit lanes. No new SDK executor or harness is necessary.
- Focused proof: static collection and existing smoke lane configuration must retain those Prompt contracts; live execution remains credential-gated, and no credentials were available/used in this review.

## Acceptance matrix

Verification marked “reported” is inspected task evidence, not a rerun or independently captured command result.

| Obligation | Implementation evidence | Verification evidence | Result |
|---|---|---|---|
| REQ-012 executable SDK Agent removed | Python runtime package and native agent registration deleted; Rust/TS export sources have no equivalent constructor | Reported imports, package/export checks; static exports inspected | PASS |
| REQ-013 runtime-only closure, preserve meaningful coverage | Agent/tool/providers submodules and helpers deleted; response mock helper survives; Prompt smoke cases removed | INVREV-003/004 | FAIL |
| REQ-014 Cards/internal Agent preserved | `state.agent`, Card contracts, hydration, Vala executor/server binder remain; Skald diff removes Python-only projections | Reported P-R/P-P/P-T 3 each; test:skald/shared | PASS |
| REQ-015 all Run invocation projections deleted | shared invoke module, Python method/stub, TS native/public/declarations removed | Static diff/consumer search; reported native/type checks | PASS |
| REQ-016 correlated migrated workflows | support-desk and local-development requests carry caller and Run/Card; Python gateway helper omits subjects; TS ignores Prompt content | INVREV-001/002; reported J-R/P/T do not catch gaps | FAIL |
| REQ-017 supported integrations retained | external framework adapters, trace processors, native gateway/MCP remain untouched | Existing framework journeys unchanged; reported owner regressions | PASS |
| REQ-018 accurate boundary docs | evidence guide and example READMEs migrated; runtime-only response docs still prescribe deleted callback | INVREV-003 | FAIL |
| INV-001 server durable ownership | no server durable behavior moved; application producers call native ingress | Static request/server boundary inspection | PASS |
| INV-002 caller token/provider secret ownership | fresh Wyrd access token or existing HTTP auth; no provider secret read added | support-desk upstream-secret/header assertions; reported J tests | PASS |
| INV-003 tool permission not inferred | no client tool binder/tool authorization added; MCP/server ownership unchanged | Static complete change paths | PASS |
| INV-004 delegated tools/domain/audit retained | no hosted binder/MCP/audit changes | Existing preservation tests reported; source owners unchanged | PASS |
| INV-005 server execution refuses native route | unchanged server hosted execution path | Reported preservation regressions | PASS |
| INV-006 Skald independent of Wyrd mechanics | shared client execution dependencies removed; internal runtime not moved into binding | Manifest/diff inspection; reported check:deps | PASS |
| INV-007 portability | deletion/request migration introduces no platform mechanism | Static diff | PASS |
| INV-008 no Cards/persisted contracts/evidence removal | no Card schemas/migrations changed; server verification remains | Reported P-R/P-P/P-T | PASS |
| AC-007 complete public/native/typing/package removal | main runtime removed but mock response companion remains | INVREV-003 | FAIL |
| AC-008 declarative/internal preservation | source preserved as above | Reported P-R/P-P/P-T; test:skald | PASS |
| AC-009 correlated inference and refusals across SDKs | Python migrated gateway route calls lose subject; TS Prompt data skipped | INVREV-001/002 | FAIL |
| AC-010 auth, subject, secret, integration boundaries | actual gateway enforcement remains, existing unrelated integration paths retained | Reported scoped negatives, upstream secret/header assertions; subject proof gap already AC-009 | PASS |
| AC-011 accurate docs/artifacts/checks | primary docs migrated; mock callback documentation remains stale | INVREV-003; reported format/lint/docs/package checks | FAIL |
| Non-goal: no unified lifecycle/Jev runtime refactor | only Python cfg/results glue removed from Skald runtime | Static Skald diff | PASS |
| Non-goal: no new public Agent/Runner/Prompt execution | all new inference functions are application/example or test-owned | Static new symbols | PASS |
| Non-goal: no MCP relocation/new binder/new server execution | none added | Complete changed-file list | PASS |
| Non-goal: preserve Prompt authoring/loading, Cards, observational sessions | public Prompt and Card APIs retained; session observation separate from deleted Agent memory | Existing Prompt unit source and public exports; reported P journeys | PASS |
| Non-goal: no persisted/wire/security/error migration | no schemas, durable migrations, permission/audit or shared catalog removal | Static diff/manifests | PASS |
| Non-goal: no aliases/shims/unrelated cleanup | no compatibility layer or unrelated feature added | Static deletion/new request helpers | PASS |

## Producer, lifecycle, and reuse assessment

Hydration remains the producer of exact Agent/Prompt identities. Run still owns only local correlation/scoping; observation/verification handles retain server transport and authenticated caller context. Removing invocation severs the runtime dependency at that owner while preserving those siblings. Application model calls now terminate at native server gateway handlers, which continue to parse/authorize subject values, select deployment, apply server-owned credentials and capture; no new durable state was added. The Python omission is upstream of those unchanged consumers.

Skald non-Python Agent, sessions, resource/workspace/tool runtime methods remain. Python-only AgentRun parsed/result properties, callback adapters, local tool registry and mock-provider registration are deleted. No evidence of an internal lifecycle or permission rewrite appears in this cumulative diff. The residual public mock response companion is the incomplete closure, rather than justification for new code.

The new application request helpers are bounded concrete flows, not a second SDK workflow owner. HTTP auth/access-token reuse is appropriate. TS gateway test should bind its actual two known authored forms locally; there is no task authorization for a generic renderer. Source-local consumer corrections are sufficient for the identified failures.

## Verification notes and limits

Reviewed the original task at base, approved revision 3 specification, cumulative diff, shared subject/navigation map, governing agent rules and applicable design/doctrine/testing/telemetry references; inspected current caller/sink source via CodeGraph first and bounded source fallback. Did not read other reviewers' reports. Candidate task evidence was treated as claims. `git diff --check` passed; HEAD matches candidate. No builds, Cargo commands, tests, provider calls, source edits, or external writes were performed. No evidence of a test-failure workaround with changed sleeps/retries/ignores was found; the Prompt test retirement issue is independent of a claimed gate failure.

Skipped test reruns under root's coordination rule. Residual risk: reported green commands cannot establish the missing attribution, Prompt-content, and deletion-closure obligations because their assertions/exports currently permit those gaps.
