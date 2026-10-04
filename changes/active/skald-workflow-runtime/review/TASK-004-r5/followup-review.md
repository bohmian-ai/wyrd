# Focused follow-up: OpenAI-compatible external-gateway reachability

## Subject and uncertainty

- Repository: `/home/thorrester/Documents/GitHub/wyrd`
- Base: `b90335e0991438e8c28b65ed9c0c4cd80f9b0d56`
- Candidate: `d4d4e2da53abfc677abdb804e71517c3b6849f49`
- Approved specification: `changes/active/skald-workflow-runtime/spec.md`,
  Revision 13
- Original task: `changes/active/skald-workflow-runtime/tasks/TASK-004-accepted-server-jobs.md`
- Remediations: TASK-004 R1 through R4 in their preceding review directories
- Discovery claim under review: `DOMAIN-PROVIDER-1` in
  `domain-review-provider-contract.md`

The focused question was whether
`ProviderRequest::OpenAiChatCompatible` is an intended and reachable request
through `LlmRoute::ExtGateway { protocol: OpenAiChat, ... }`, making its
omission from both `protocol_matches` and `ExternalGatewayClient::send` a
TASK-004 defect, or whether that combination is outside Revision 13 and should
be rejected as unrelated scope.

`HEAD` resolved to the requested candidate before this report was written.
`.codegraph/` is absent, so I used immutable Git objects, repository search,
and direct source inspection. Per the standing direction, I ran no build,
test, Cargo, mise, code-generation, or package-manager command.

## Source path inspected

1. **Producer and durable identity.**
   `crates/skald/skald-spec/src/request.rs:18-37,61-103` defines
   `OpenAiChatCompatible` as a custom provider accepting OpenAI Chat
   Completions request semantics, embeds its dispatch `ProviderName`, and
   returns that embedded identity from `ProviderRequest::provider`. Revision
   13 expressly preserves this variant and its embedded identity in the
   adjacent-tagged body (`spec.md:21-42`). Public Python Prompt construction
   produces the variant for `ProviderName::Custom` at
   `crates/skald/skald-prompt/src/prompt.rs:436-462`; the checked-in native
   Prompt at
   `tests/fixtures/workflow-loading/team/prompts/correctness.yaml:1-23`
   authors the same tagged variant. This is therefore neither dormant nor a
   hypothetical future constructor.

2. **Persistence and hydration.**
   `crates/skald/skald-workflow/src/bodies.rs:66-107,187-242` receives exact
   stored Agent/Prompt bodies and constructs the runtime Prompt with
   `Prompt::from_native`, without rewriting or inferring the provider variant.
   TASK-004's new server host pins the registered graph, creates external
   bindings, and hydrates it through `SkaldWorkflow::from_card_bodies` before
   preparation at
   `crates/wyrd/wyrd-server/src/components/workflow/host.rs:309-400`.
   Consequently a registered custom OpenAI-compatible Prompt reaches the same
   resolved validation used by local execution, with its variant intact.

3. **Agent-loop validation and model dispatch.**
   `crates/skald/skald-agent/src/request_builder.rs:14-51,81-120,222-241`
   classifies both `OpenAiChatCompletion` and `OpenAiChatCompatible` as the
   one `OpenAiChat` conversation dialect, extracts the same message type, and
   preserves the compatible wrapper while rebuilding messages.
   `crates/skald/skald-runtime/src/dispatch.rs:22-39,73-89` dispatches through
   the embedded custom provider name. For an external route,
   `StepRoute::attempt_registry` registers the external adapter under that
   exact Prompt provider and `ExternalGatewayProvider::send` forwards the
   typed request to the external client
   (`crates/skald/skald-workflow/src/route.rs:395-427,610-646`). There is no
   provider-registry or dispatch barrier that makes this combination
   unreachable.

4. **Pre-dispatch route validation.**
   `ResolvedGraph::resolve` calls `protocol_matches` for every resolved
   external route at
   `crates/skald/skald-workflow/src/plan.rs:53-93`.
   `protocol_matches` maps `ExternalGatewayProtocol::OpenAiChat` only to
   `ProviderRequest::OpenAiChatCompletion` at
   `crates/skald/skald-workflow/src/route.rs:430-453`. It omits the compatible
   variant even though the Agent owner classifies both variants as the same
   dialect. Thus the reachable custom-Prompt/external-route combination is
   refused during registration or run preparation as a dialect mismatch.

5. **Downstream send.** If the matcher alone accepted the combination,
   `ExternalGatewayClient::send` would still reject it. Its OpenAI Chat arm
   posts only `OpenAiChatCompletion` to `chat/completions`, while all other
   variants reach the `VariantMismatch` branch
   (`crates/skald/skald-providers/src/clients/external.rs:95-139`). The
   external route adapter forwards the compatible variant unchanged, so this
   is a second reachable rejection of the same dialect.

6. **Sibling working owner.** The in-process Wyrd-gateway projection matches
   `OpenAiChatCompletion` and `OpenAiChatCompatible` in the same arm, emits
   `GatewayOperation::ChatCompletions` with `IngressDialect::OpenAi`, and
   decodes the same OpenAI Chat response
   (`crates/wyrd/wyrd-server/src/components/gateway/workflow.rs:182-205`).
   This confirms that the compatible body already has an established OpenAI
   Chat projection; supporting it does not require translation or a new
   protocol.

7. **Existing proof.** TASK-004 maps `REQ-039` in its front matter
   (`TASK-004-accepted-server-jobs.md:1-8`) and Scenario 5 requires supported
   dialect combinations plus direct server external egress
   (`TASK-004-accepted-server-jobs.md:318-337`). The server journey's external
   fixture always authors `provider: open_ai_chat_completion`
   (`crates/wyrd/wyrd-server/tests/pg_workflow_runs.rs:989-1028,2255-2297`).
   Its provider matrix covers Anthropic, Gemini, OpenAI Responses, and Vertex
   only through `WyrdGateway` (`pg_workflow_runs.rs:2324-2420`). The existing
   local external-route test likewise uses the ordinary OpenAI Chat variant
   (`crates/skald/skald-workflow/src/workflow.rs:2187-2265`). No inspected
   route or external-client proof combines `OpenAiChatCompatible` with
   `openai_chat`, so the recorded green evidence does not exercise either
   omission.

## Authority resolution

The combination is required, not out of scope.

- The durable protocol enum has one `OpenAiChat` member, not separate native
  OpenAI and compatible-provider protocols (`spec.md:665-685`).
- `REQ-039` binds external-route acceptance to the Prompt **request dialect**,
  requires rejection only when the protocol does not match, and prohibits the
  Workflow runtime from translating the external request
  (`spec.md:2077-2086`). `OpenAiChatCompatible` is explicitly an OpenAI Chat
  request-semantic variant and the Agent owner assigns it to that dialect.
  Sending its existing inner `OpenAiChatRequest` to the already-owned
  `chat/completions` path is direct native-dialect dispatch, not translation.
- `REQ-038` requires `ExtGateway` to call that declared protocol directly from
  both local and server runtimes (`spec.md:2067-2076`), and the approved design
  says the Prompt owns provider/request semantics while the route selects only
  the execution boundary (`spec.md:2384-2394`). Restricting `openai_chat` to
  the `OpenAi` provider discriminant would improperly make the boundary select
  provider identity as well as dialect.
- Revision 13 changed only serialization and specifically retained the
  compatible variant; it did not narrow the already-approved REQ-038/039 route
  behavior. TASK-004 carries both `REQ-039` and the new accepted-server
  `ExtGateway` composition, so the omission is task-relevant even though the
  shared matcher/client also serve local execution.

The earlier provider reviews' PASS results and Scenario 5's recorded success
do not override this authority. Those reviews established the adjacent-tagged
wire and registered Vertex path, while the named proofs use the ordinary
OpenAI Chat variant for external egress. They do not establish that the
compatible variant is intentionally excluded.

## Proposed finding

### FOLLOWUP-PROVIDER-1 — confirms `DOMAIN-PROVIDER-1` as one INCORRECT path

- **Violated obligation:** Revision 13 `REQ-038` and `REQ-039`, as mapped by
  TASK-004, require a matching `ExtGateway` protocol to dispatch the Prompt's
  native request dialect directly in local and server execution.
- **Source defect:**
  `crates/skald/skald-workflow/src/route.rs:430-453` rejects
  `OpenAiChatCompatible` as not matching `OpenAiChat`, and
  `crates/skald/skald-providers/src/clients/external.rs:95-139` rejects the
  same reachable request if it gets past validation.
- **Observable consequence:** A persisted, Agent-capable custom
  OpenAI-compatible Prompt cannot be registered or prepared with the only
  external protocol that matches its native body. If validation were changed
  alone, execution would fail at the downstream sender before an upstream
  request.
- **Smallest correction boundary:** Extend the two existing OpenAI Chat match
  arms to treat `OpenAiChatCompatible { request, .. }` as that same dialect
  and post the existing inner request through the existing
  `chat/completions` path, returning the existing OpenAI Chat response shape.
  Prove the existing server external-route journey with one compatible custom
  Prompt, including pre-dispatch acceptance and exact path/body/response.
  No new protocol variant, adapter, translation layer, provider registry,
  compatibility reader, setting, option, checker, dependency, or test harness
  is justified.

This correction follows the standing DRIFT direction: it extends two existing
exhaustive dispatch points and one existing journey using the repository's
ordinary typed-enum and HTTP-client mechanisms. Adding separate machinery or
a special opt-in for a standard OpenAI-compatible endpoint would itself be
drift.

## Resolution

**RESOLVED.** `ProviderRequest::OpenAiChatCompatible` is intended and reachable
through `ExtGateway(protocol = openai_chat)` under Revision 13. The two
omissions are one task-relevant source-to-sink defect, not unrelated
pre-existing debt or an unapproved feature expansion. `DOMAIN-PROVIDER-1`
should proceed to independent Ponytail validation with the narrowed correction
above.
