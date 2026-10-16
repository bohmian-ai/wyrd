# SDK/native contracts domain review

**Overall: FAIL** — one bounded application-template regression proposed for independent validation.

## Subject and boundary

Repository `/home/thorrester/Documents/GitHub/wyrd-worktrees/agent-runtime-composition`; cumulative base `06a368906187be20364a2d959da311151964b617` to candidate `497dd3acc8fd06567a0e0ff01baf599f9af1711b`. HEAD was still the candidate at inspection. Reviewed cross-language public/native runtime removal, companion exclusivity, retained Card/Prompt projections, and migrated Prompt-consuming application calls. No implementation files changed.

## Material finding

### SDK-01 — REGRESSION / Important: TypeScript question binding interprets replacement metacharacters

- **Location:** `examples/support-desk/typescript/support-desk.ts:140`, `answer`.
- **Violated obligation:** REQ-013/016 and AC-009 require migrated consumers to preserve their useful application behavior. The existing Prompt template must receive the actual question; removing the SDK runtime does not authorize altering application input.
- **Producer and reachable path:** The exported `answer(client, state, run, asked)` accepts arbitrary question text. Its hydrated Prompt is the registered `examples/support-desk/service/support-prompt.yaml`, whose message is `Answer the customer: {{question}}`. `serve` calls this function at line 177; the real-server journey imports and calls the same function directly at `sdks/wyrd-sdk-ts/wyrd/tests/integration/support-desk.test.ts:86`. The new application binder passes `asked` as JavaScript's replacement-string argument to `replaceAll`.
- **Evidence:** JavaScript replacement strings interpret `$&`, `$\``, `$'`, and `$$`. For example, `asked = "What does $& mean?"` sends `Answer the customer: What does {{question}} mean?` to the gateway instead of `Answer the customer: What does $& mean?`. This is source-level language behavior, not an upstream-model assumption. The former `Run.invoke` path called Skald `Prompt::render`; `crates/skald/skald-spec/src/prompt.rs:601-606` performs literal Rust string replacement, so the same input was preserved. The new Rust support-desk and Python support-desk consumers still use native Prompt rendering; this faulty replacement is local to the TypeScript migration.
- **Consequence:** A valid customer's question is silently changed before model inference while observations can retain the original question. This can produce an answer and verification evidence for a different request.
- **Smallest correction:** Keep binding application-owned and use a replacement callback returning `asked`, so input is inserted literally. No SDK runtime, render API, new abstraction, or provider compiler is needed.
- **Focused closure proof:** Extend the existing TypeScript support-desk real-server/mock-upstream journey to call `answer` with a question containing replacement tokens (at least `$&` and `$$`) and assert the upstream message contains the exact original text after the `Answer the customer: ` prefix. The existing fixture/provider records upstream requests. Retain the existing permission/correlation checks. No tests were executed by this reviewer.

## Authority and source coverage

| Boundary | Governing authority inspected | Source and conclusion |
|---|---|---|
| Public execution deletion | Approved spec revision 3 REQ-012/013/015; original task at base; AGENTS SDK/PyO3 ownership; agent rules; spec-driven-development | Reviewed full cumulative changed-path map and relevant removal diffs: shared `observe/invoke.rs`, `gateway_caller.rs`, client exports/dependencies, Python root exports/stubs, native aggregator and observe pymethod, TypeScript public/native Run removal and both generated declarations. No remaining SDK Agent constructor or Run invocation found in these projections. |
| Native companion exclusivity | REQ-013/017; Python API/stubs and PyO3 references | Inspected removed `wyrd.agent` exports/session protocol and native registration lists, removed Skald Python module owners/features, and searched remaining SDK/framework consumers. Deleted callback/session/local-tool/mock-provider wrappers serve the retired runtime. Shared Agent/Session/Tool error classes and observation identities remain. No surviving external-framework dependency on those wrappers found. |
| Retained Prompt/Card contracts | REQ-014, INV-008; wyrd-design Prompt/Agent sections; doctrine runtime/public surfaces | Python retains `skald-prompt/python`, Prompt registration, public exports/stubs, Card registration and hydrated navigation. TS `hydrated_agent`, `hydrated_prompt`, `promptCard`, and AgentCard projection remain; Rust retains shared-client/Card surfaces. Authored Prompt forms, schemas, model settings, and their compiler were not modified. Existing Prompt model-settings/output-schema unit coverage remains. |
| Internal Vala/server semantics | REQ-014; INV-005/006; AGENTS Skald/Vala ownership | Changes to Agent/context/run/session/loop owners remove Python-only wrappers or Python parsed-result fields. Inspected Vala `AgentExecutor::invoke`, simulator `ServerSimulatedUser::next_turn`, and hosted `HostedTools::open`: run configuration, routes, tool-session opening, refusal handling, drop/cleanup, and structured results remain with current owners. No moved SDK execution owner or host-tool rewrite. |
| Migrated consumers | REQ-016; AC-009; maintainer/testing references | Inspected Python/Rust/TS support-desk calls, Rust support-desk-app answer, gateway inference journeys, Python installed-CLI local-development changes and fixtures. Rust uses rendered full native body; Python local-development does likewise. SDK-01 is the concrete current input-fidelity regression. Correlation/auth remain via existing headers/adapters. |
| Public/generated absence | AC-007/011; Python API/stubs and TypeScript references | Native registration/public/stub removals agree in inspected source; `.d.ts` and `.d.cts` remove native invocation alongside Rust source. Package import/regeneration claims are recorded task evidence, not independently reproduced here. |

CodeGraph was used first to navigate Prompt/rendering and Vala/hosted-tool owners; bounded source/rg fallback expanded incomplete results. Authority read included `AGENTS.md`, `architecture/agent-rules.md`, reference router, specification/original task, spec-driven development, maintainer style, PyO3 boundaries, Python API/stubs, TypeScript and testing guidance; relevant design/doctrine sections establish the retained declarative/runtime boundary. Review does not infer the older reference examples listing native `agent`/`tool`/`providers` override this explicitly approved deletion.

## Verification and limits

- Static source review only. No Cargo, build, codegen, package, Python/Node runtime, or test commands were run, as coordinated by the orchestrator.
- Candidate task evidence claims green SDK journeys, unit/lint/type/package/dependency checks. Their source coverage was inspected, but execution results were not independently reproduced.
- TS `gateway-inference.test.ts:402` now constructs a hard-coded `hi` message for both `agent` and native-body `body-agent`; it does not exercise the stored Prompt body/template. That reduces what this particular test proves. It is recorded as a proof limitation, not a separate production defect or a request for a new SDK Prompt runner.
- The deleted live Prompt-provider smoke module covered surviving compiler behavior as well as removed execution. Remaining authored-Prompt/unit tests preserve meaningful lowering coverage, but live provider acceptance is no longer checked by those removed cases. No new live-provider gate is requested here.
- Support-desk checked-in prompts contain simple portable OpenAI messages. Generalized settings/schema/native-form handling in its new Python/TS application helpers was not demanded as a speculative new product capability; the retained compiler/schema/settings contracts themselves are unchanged.
- Security/gateway authorization and operational reliability are independently reviewed domains. This report checks their SDK seams without claiming a new full security or deployment audit.

Skipped independent runtime verification; SDK-01 still needs its focused upstream-message regression proof.
