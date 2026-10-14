# Independent system-resilience review

## Subject and result

Base `807c4fe68`; candidate `6294bf6bbc0acb9abc37e4e4d617e2638f9dc3f7`; specification and original task `changes/active/responses-input-forms/spec.md` revision 1. Reviewed the complete `candidate.diff` and current source. HEAD remained the candidate at completion. Source was not modified.

**Overall: PASS for system-resilience scope. Proposed material findings: none.** This is a source-based assessment of the changed runtime paths, not a claim that AC-002 verification passed or that the missing Agents SDK journey exists.

## Authority and coverage

Applicable authority: AGENTS.md §§2–3, 6, 9–12; architecture/agent-rules.md (blocking authorization, non-blocking audit, tenant isolation, journeys); architecture/wyrd-design.md (client model, Prompt and Agent ownership, doctrine 20); architecture/wyrd-doctrine.mdx (service and runtime boundaries); architecture/references/doctrine/architecture-constraints.md (one logical serving surface may comprise multiple pods); architecture/references/architecture/patterns.md (provider separation, server ownership, audit); architecture/references/languages/testing-workflows.md. The spec-driven-development and maintainer-style references were consulted to bound the review. Navigation started with CodeGraph and expanded to surrounding source.

| Changed surface | Actual deployed path and evidence | Assessment |
|---|---|---|
| `OpenAiResponsesInput::Items` / `easy_items` | `crates/skald/skald-spec/src/wire/openai_responses.rs:291–332`; HTTP Responses route `crates/wyrd/wyrd-server/src/components/gateway/routes.rs:915–928` → `typed_body` at 1723–1738 → `invoke_openai` at 1779–1819 | Synchronous request-local decoding. No retained resources, tasks, caches, locks, retries or persistence added. |
| Tool-output union | `openai_responses.rs:392–395,419–457`; shared typed decode accepts text or existing content parts | Admitted at the existing request boundary, with decode errors remaining request failures. |
| Gateway forwarding | `routes.rs:1726–1738` discards typed projection and returns original JSON; `invocation.rs:424–483` retains that JSON in `AdmittedCall`; `adapter/mod.rs:178–186,531–540` produces native JSON, changing only model; `adapter/http.rs:238–266,339–349` uses raw buffered/streamed send | Both requested forms reach OpenAI-compatible deployment without normalization. Normalized decode cannot strip replay IDs or alter assistant content on this path because its result is discarded. |
| Native Skald tool loop / prompt editing | `crates/skald/skald-agent/src/request_builder.rs:375–390`; `crates/skald/skald-prompt/src/prompt.rs:143–151`; `crates/skald/skald-agent/tests/integration/loop_responses.rs:113–187` | Existing generated string tool results wrap the same string in `Text`; request semantics and loop/resource ownership unchanged. |
| Python projection | `crates/skald/skald-prompt/src/python/openai_responses.rs:573–580`, `OpenAiFunctionCallOutput::text` | Read-only text extraction. No IO, lifetime crossing, background work, or deployed lifecycle changes. |
| Generated schemas, eval fixtures, Python/TypeScript declarations | Complete diff adds output union and updates descriptions/references | No deployment config, SQL schema, durable authority, transport or health protocol modified. Generation parity belongs to standards/verification assessment. |

## Failure and recovery paths

The gateway is a capability within shared `wyrd-server`; multiple replicas remain one logical serving surface. Existing owners continue controlling admission, tenant snapshots, provider credentials, tracked execution, accounting, streaming and drain.

| Credible failure | Reachable source path | What stops and survives | Candidate effect |
|---|---|---|---|
| Malformed easy message/tool parts | `easy_items` returns deserialize error; `typed_body` maps it to stable `GatewayInvalidRequest`; `openai_call`, `routes.rs:1341–1355`, renders it | One request refuses before provider dispatch. Decoder has no panic/exit path. Unrelated capabilities remain available. | Expands valid decoding, preserves request-local rejection. |
| Provider unavailable, invalid credentials, or refusal | `GatewayEngine::execute`, `engine.rs:460–562`; `attempt` at 589–636; `HttpProviderDispatch::dispatch`, `adapter/http.rs:640–700` | Credential resolution fails before dispatch; existing engine chooses retry/fallback by failure class, operation and health. Terminal refusal propagates through existing response owner. | No retry, fallback, credential lifetime or shared health changes. |
| Timeout/caller cancellation | `engine.rs:472–500` selects cancellation/deadline against attempt; `invocation.rs:366–406` child cancellation/drop guard; tracked `CallTask::run` at 1017–1051 | Interrupted attempt becomes existing timed-out/cancelled outcome, not process crash. Accounting belongs to tracked work; streams own end signal after head delivery. | No asynchronous work or cancellation-window changes. |
| PostgreSQL/accounting outage after provider answer | `invocation.rs:1061–1094` buffered settlement; 1105 onward streamed settlement | Buffered answer waits for accounting and fails on accounting error. Already-delivered stream logs failed accounting; leases/reconciliation retain existing recovery role. | Neither settlement branch changed or bypassed. |
| Graceful drain/rolling replacement | `invocation.rs:432–437` refuses admission; `app/server.rs:839–871` drains gateway work before Scribe outbox and Bifrost | Accepted settlement tracked inside existing shutdown deadline. Capability trackers remain separate. | No shutdown dependency or decoder state introduced. |
| Abrupt death | Existing tenant ledger reservations, attempt evidence, in-memory request and Scribe outbox lifecycle | In-flight HTTP work ends with process. A new call on remaining/restarted replica uses existing tenant config and engine. Unacknowledged evidence retains approved outbox loss contract. | No durable schema, recovery lease, idempotency fence or outbox changes; no automatic interrupted-call resumption promised. |

The candidate reuses existing wire decoding and invocation ownership. It adds no second orchestration, transport, tracker, journal or cache. A decoder lifecycle owner would earn no state here. No changed reachable path requires request-level errors to terminate the process or remove unrelated capabilities.

## Proof and limits

New unit test `openai_responses.rs:691–714` exercises mixed easy user/assistant messages, replayed function item and content-part result. Existing Skald loop fixture asserts the same second request and final result with a `Text` value. Existing forwarding evidence at `crates/wyrd/wyrd-server/src/components/gateway/pg_invocation_tests.rs:1849–1880` proves raw Responses preservation for text shorthand and typed messages, but does **not** exercise newly accepted easy-message/tool-parts bodies. Existing engine tests cover deadlines, cancellation and retry; the candidate leaves those owners unchanged.

No Cargo, codegen, live-provider, process-crash or restart experiments were run by this reviewer; verification is coordinated separately to avoid competing Cargo processes. No production recovery protocol changes warrant new fault-injection requirements. The admitted Agents SDK body still lacks a real client → server → provider → client journey, as the spec records; this report does not waive the stronger repository journey requirement. That is an acceptance/standards proof gap, not evidence of a system failure or justification for unrelated resilience machinery.

## Proposed findings

Empty. No material system-resilience defect or changed recovery-path regression established from cumulative source.
