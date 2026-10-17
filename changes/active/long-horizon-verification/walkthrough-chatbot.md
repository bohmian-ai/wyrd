# Walkthrough: customer-facing chatbot

Proposal input for `spec.md`. Shows the complete user side: every Card file,
every producer's code, and how a conversation moves through them. Lines marked
`# NEW` are proposed; everything else works today.

## How it fits the existing model

Nothing about `state.run()` / `run.observe` changes. A session is a group of
runs that share a caller key.

| Today | With sessions |
|---|---|
| `state.run("agent")` opens one local invocation (one `run_id`) scoped to a Card view | `state.run("agent", session=key)` — same local run, also tagged with the session key. Still no network IO. `# NEW` |
| `run.observe.verify(v, **inputs)` runs a bound Verifier now, inline | Unchanged. Use it when the app must act on the verdict before replying. |
| `run.observe.submit(v, **inputs)` queues one execution, or records samples for a scheduled Verifier | Unchanged. |
| `run.observe.record(table, row)` writes a dataset row | Unchanged. |
| — | `run.observe.append(**inputs)` adds the named inputs to the run's session as one contribution. The bindings decide what runs; the caller names no Verifier. `# NEW` |

What runs on an `append` is decided by existing Card scoping, not by event
names:

- Bindings on the **view's Card** with `runs_on: evidence_appended` run on the
  contribution, if it fills the Verifier's required slots. A customer-message
  contribution has no `reply`, so reply checks are not applicable to it.
- Bindings on the **Service** with `runs_on: session_sealed` run once per
  sealed revision over every contribution from every view.
- An input name no bound Verifier declares is refused
  (`WYRD_VERIFICATION_400_INPUT_INVALID`), so a typo cannot silently skip a
  check.

## Card files

```
support-bot/
  support-bot.yaml            Service: components + every binding
  support-agent.yaml          the production Agent (theirs; not shown)
  on-agent-turn.yaml          Trigger: each contribution        # NEW kind
  on-conversation-end.yaml    Trigger: sealed conversation      # NEW kind
  hourly.yaml                 Trigger: schedule
  reply-safe.yaml             Verifier: inline guard before sending
  turn-helpful.yaml           Verifier: LLM judge, one agent turn
  turn-judge.yaml             Agent: judge model for turn-helpful
  turn-output.yaml            Verifier: assertions + trace assertion, one turn
  conversation-resolved.yaml  Verifier: LLM judge, whole conversation  # NEW slot type
  conversation-judge.yaml     Agent: judge model for conversation-resolved
  behaviour-shift.yaml        Verifier: drift across conversations     # NEW data
  page-support-oncall.yaml    Operator: the only page
```

### support-bot.yaml

```yaml
apiVersion: wyrd/v1
kind: Service
metadata: {name: support-bot, version: 3.2.0, space: retail}
spec:
  service_type: agent
  components:
    - alias: agent
      ref: ./support-agent.yaml
      verified_by:
        # Called from code before a reply is sent (no runs_on: today's behaviour)
        - verifier: ./reply-safe.yaml
        # Run on every contribution appended through the agent view
        - verifier: ./turn-helpful.yaml
          runs_on: ./on-agent-turn.yaml
        - verifier: ./turn-output.yaml
          runs_on: ./on-agent-turn.yaml
  verified_by:
    # Whole conversation: every contribution from every view
    - verifier: ./conversation-resolved.yaml
      runs_on: ./on-conversation-end.yaml
    # The chatbot as a whole, over time: the only binding that notifies
    - verifier: ./behaviour-shift.yaml
      runs_on: ./hourly.yaml
      inputs:
        helpful:  {features: [turn-helpful/helpful]}              # NEW: result samples
        resolved: {features: [conversation-resolved/resolved]}    # NEW: result samples
      on_failure:
        - ./page-support-oncall.yaml
```

### on-agent-turn.yaml

```yaml
apiVersion: wyrd/v1
kind: Trigger
metadata: {name: on-agent-turn, version: 1.0.0, space: retail}
spec:
  description: Each contribution appended through the bound Card's view.
  kind: evidence_appended                                         # NEW
```

### on-conversation-end.yaml

```yaml
apiVersion: wyrd/v1
kind: Trigger
metadata: {name: on-conversation-end, version: 1.0.0, space: retail}
spec:
  description: A conversation after 30 quiet minutes, or when someone closes it.
  kind: session_sealed                                            # NEW
  session:                                                        # NEW
    idle: 30m        # seal after 30 minutes with no contribution
    max_age: 24h     # seal regardless after 24 hours
    finality: 30d    # later evidence opens a new revision; after 30 days it is refused
```

### hourly.yaml

```yaml
apiVersion: wyrd/v1
kind: Trigger
metadata: {name: hourly, version: 1.0.0, space: retail}
spec:
  description: Every hour on the hour, UTC.
  kind: schedule
  cron: "0 * * * *"
```

### reply-safe.yaml — inline guard

```yaml
apiVersion: wyrd/v1
kind: Verifier
metadata: {name: reply-safe, version: 1.0.0, space: retail}
spec:
  description: A reply is safe to send to the customer.
  inputs:
    reply: {type: json}
  requirements:
    - id: no-guaranteed-refund
      statement: The reply never promises a guaranteed refund
      check: {kind: assert, path: inputs.reply.text, operator: not_contains, expected: guaranteed refund}
```

### turn-helpful.yaml — LLM judge, one agent action

```yaml
apiVersion: wyrd/v1
kind: Verifier
metadata: {name: turn-helpful, version: 1.0.0, space: retail}
spec:
  description: One agent reply helps the customer without inventing store policy.
  inputs:
    message: {type: json}
    reply:   {type: json}
  requirements:
    - id: helpful
      statement: The reply answers the customer's message and invents no store policy
      check:
        kind: agent
        agent: ./turn-judge.yaml
        inputs:
          claim:   requirement.statement
          message: inputs.message.text
          reply:   inputs.reply.text
```

### turn-judge.yaml

```yaml
apiVersion: wyrd/v1
kind: Agent
metadata: {name: turn-judge, version: 1.0.0, space: retail}
spec:
  execution_route: {kind: wyrd_gateway, deployment: openai}
  run_config: {max_iterations: 1}
  prompt:
    provider: openai
    model: gpt-6-luna
    system: |
      You judge one reply from a retail support agent.
      Store policy: returns within 30 days; refunds over $500 need a manager.
      Fail any reply that states a policy not listed here.
    messages:
      - role: user
        content: "Claim: {{claim}}\nCustomer: {{message}}\nAgent: {{reply}}"
    response_format:
      type: object
      properties:
        outcome: {type: string, enum: [passed, failed, inconclusive]}
        reason:  {type: string}
        citations:
          type: array
          items:
            type: object
            properties:
              slot: {type: string}
              locator: {type: object}
            required: [slot, locator]
      required: [outcome, reason, citations]
      additionalProperties: false
```

### turn-output.yaml — assertions and trace assertion, one agent action

```yaml
apiVersion: wyrd/v1
kind: Verifier
metadata: {name: turn-output, version: 1.0.0, space: retail}
spec:
  description: One agent reply is well formed and was produced the right way.
  inputs:
    reply: {type: json}
    trace: {type: trace, required: false}
  requirements:
    - id: refund-under-limit
      statement: Any refund the agent offers is at most $500
      check: {kind: assert, path: inputs.reply.refund_amount, operator: less_than_or_equals, expected: 500}
    - id: order-looked-up
      statement: The agent looked up the order before answering
      check: {kind: trace, on: inputs.trace, span_selector: "$.spans[0].name", operator: equals, expected: lookup-order}
```

`trace` is optional: a team without tracing still gets `refund-under-limit`;
`order-looked-up` is not applicable.

### conversation-resolved.yaml — LLM judge, whole session

```yaml
apiVersion: wyrd/v1
kind: Verifier
metadata: {name: conversation-resolved, version: 1.0.0, space: retail}
spec:
  description: A whole support conversation resolved the issue and kept its promises.
  inputs:
    conversation:
      type: session                                               # NEW
      records:                     # the input names producers may append; all optional
        message: {type: json}
        reply:   {type: json}
        order:   {type: json}
        trace:   {type: trace}
  requirements:
    - id: resolved
      statement: The customer's issue was resolved and every promise the agent made was kept
      check:
        kind: agent
        agent: ./conversation-judge.yaml
        inputs:
          claim: requirement.statement
          conversation: inputs.conversation
```

At seal, `inputs.conversation` is every contribution of the revision, ordered
by producer time; trace IDs are resolved to their spans:

```json
[
  {"run_id": "0192…a1", "card": "support-bot/agent", "producer": "chat-backend",  "at": "10:00:02",
   "inputs": {"message": {"text": "Where is order 8812?"}}},
  {"run_id": "0192…a1", "card": "support-bot/agent", "producer": "chat-backend",  "at": "10:00:05",
   "inputs": {"message": {"text": "Where is order 8812?"},
              "reply": {"text": "It's delayed. I can send a replacement.", "refund_amount": 0},
              "trace": {"spans": [{"name": "lookup-order"}, {"name": "draft-reply"}]}}},
  {"run_id": "0192…b7", "card": "support-bot",       "producer": "order-service", "at": "10:01:40",
   "inputs": {"order": {"id": "8812", "status": "replacement_shipped"}}},
  {"run_id": "0192…c3", "card": "support-bot",       "producer": "agent-console", "at": "10:20:11",
   "inputs": {"reply": {"text": "Your replacement arrives Friday."}}}
]
```

### conversation-judge.yaml

```yaml
apiVersion: wyrd/v1
kind: Agent
metadata: {name: conversation-judge, version: 1.0.0, space: retail}
spec:
  execution_route: {kind: wyrd_gateway, deployment: openai}
  run_config: {max_iterations: 1}
  prompt:
    provider: openai
    model: gpt-6-luna
    system: |
      You judge a whole retail support conversation: customer messages, agent and
      human replies, order updates and the agent's tool calls. Pass only if the
      issue was resolved and every promise is backed by a later order update.
    messages:
      - role: user
        content: "Claim: {{claim}}\nConversation: {{conversation}}"
    response_format:
      type: object
      properties:
        outcome: {type: string, enum: [passed, failed, inconclusive]}
        reason:  {type: string}
        citations:
          type: array
          items:
            type: object
            properties:
              slot: {type: string}
              locator: {type: object}
            required: [slot, locator]
      required: [outcome, reason, citations]
      additionalProperties: false
```

### behaviour-shift.yaml — the chatbot over time

```yaml
apiVersion: wyrd/v1
kind: Verifier
metadata: {name: behaviour-shift, version: 1.0.0, space: retail}
spec:
  description: Helpful-turn and resolved-conversation rates have not shifted from the healthy baseline.
  inputs:
    helpful:  {type: samples, features: [turn-helpful/helpful]}
    resolved: {type: samples, features: [conversation-resolved/resolved]}
  requirements:
    - id: helpful-unshifted
      statement: Turn helpfulness has not shifted from the baseline
      check:
        kind: drift
        on: inputs.helpful
        method: Psi
        signal:
          kind: Distribution
          baseline_ref: {kind: Data, name: support-bot-baseline, version: 1.0.0, space: retail}
          features: [turn-helpful/helpful]
        condition: {kind: Statistical}
        profile:
          kind: Psi
          binning_strategy: {kind: EqualWidth, n_bins: 2}
          threshold: {kind: Fixed, value: 0.25}
    - id: resolution-unshifted
      statement: Conversation resolution has not shifted from the baseline
      check:
        kind: drift
        on: inputs.resolved
        method: Psi
        signal:
          kind: Distribution
          baseline_ref: {kind: Data, name: support-bot-baseline, version: 1.0.0, space: retail}
          features: [conversation-resolved/resolved]
        condition: {kind: Statistical}
        profile:
          kind: Psi
          binning_strategy: {kind: EqualWidth, n_bins: 2}
          threshold: {kind: Fixed, value: 0.25}
```

`# NEW`: every requirement result is recorded as a sample named
`<verifier>/<requirement>` (passed = 1, failed = 0, inconclusive not
recorded), so today's `features:` syntax reads them. The drift `profile` is
copied from the latency fixture; its fit for 0/1 data is unverified.

### page-support-oncall.yaml

```yaml
apiVersion: wyrd/v1
kind: Operator
metadata: {name: page-support-oncall, version: 1.0.0, space: retail}
spec:
  description: Page support on-call when the bot's behaviour shifts.
  kind: http
  method: post
  url: /hooks/support-bot-shift
  auth: {scheme: bearer, connection: support-oncall-hooks}
  body:
    execution_id: "{{execution_id}}"
    summary: "{{summary}}"
```

## Register (today)

```bash
wyrd apply ./support-bot/support-bot.yaml
wyrd get --space retail --kind Service --name support-bot --version 3.2.0 --output-dir ./support-bot-bundle
```

Each producer gets its own credential, so every contribution is attributed:

```bash
wyrd auth issue-key --kind Service --name support-bot --version 3.2.0 --space retail   # chat-backend
wyrd auth issue-key --kind Service --name support-bot --version 3.2.0 --space retail   # agent-console
wyrd auth issue-key --kind Service --name support-bot --version 3.2.0 --space retail   # order-service
```

## Producer code

### Chat backend (Python): customer messages and agent turns

```python
from wyrd import WyrdState

state = WyrdState.from_path("./support-bot-bundle")
state.start_bifrost()

def handle_message(conversation_id: str, msg) -> str:
    key = f"chat/{conversation_id}"

    # The customer's message joins the session. No agent-view Verifier has all
    # its required slots, so nothing runs per turn yet.
    with state.run("agent", session=key) as run:                     # NEW: session=
        run.observe.append(message={"text": msg.text})               # NEW

    with state.run("agent", session=key) as run:
        reply, trace_id = support_agent.respond(msg)                 # spans correlate to run_id

        # Inline guard, today's verify: block an unsafe reply before sending it.
        if run.observe.verify("reply-safe", reply={"text": reply.text}).verdict == "failed":
            reply = escalate_to_human(msg)

        # The agent turn joins the session; turn-helpful and turn-output run on it.
        run.observe.append(                                          # NEW
            message={"text": msg.text},
            reply={"text": reply.text, "refund_amount": reply.refund_amount},
            trace=trace_id,          # optional: omit without tracing
        )
    return reply.text
```

### Human agent console (TypeScript): human replies and close

```typescript
import { WyrdState } from "wyrd-ai";

const state = WyrdState.fromPath("./support-bot-bundle");
await state.startBifrost();

export async function sendHumanReply(conversationId: string, text: string) {
  const run = state.run(undefined, { session: `chat/${conversationId}` });   // NEW: root Service view
  await run.observe.append({ reply: { text } });                              // NEW
}

export async function resolveConversation(conversationId: string) {
  await state.session(`chat/${conversationId}`).close();                     // NEW: seal now
}
```

The console appends through the root Service view, so the agent-view per-turn
Verifiers do not judge human replies; `conversation-resolved` still sees them.

### Order service (Rust): order facts

```rust
use serde_json::json;
use wyrd_ai::state::WyrdState;
use wyrd_ai::verify::{Input, Inputs};

let state = WyrdState::from_path("./support-bot-bundle")?;
state.start_bifrost().await?;

async fn on_order_event(state: &WyrdState, event: &OrderEvent) -> anyhow::Result<()> {
    let run = state.run_in_session(None, &format!("chat/{}", event.conversation_id)); // NEW
    run.observe()
        .append_with_id(&event.id, Inputs::named([(                                   // NEW
            "order",
            Input::from(json!({"id": event.order_id, "status": event.status})),
        )]))
        .await?;
    Ok(())
}
```

`append_with_id` (Python: `append(event.id, /, **inputs)`; TypeScript:
`append(inputs, { id })`) makes a redelivered webhook idempotent. Without an
ID, the contribution ID is the run's `run_id` plus its append index.

## One conversation, end to end

| Time | Producer | Call | Server result |
|---|---|---|---|
| 10:00:02 | chat backend | `append(message)` on agent view | session `chat/c-7781` opens revision 1, contribution 1. No per-turn check has its required slots. |
| 10:00:05 | chat backend | `verify("reply-safe")` | inline `passed`, returned to the app (today) |
| 10:00:05 | chat backend | `append(message, reply, trace)` on agent view | contribution 2; queues `turn-helpful` and `turn-output` on it |
| 10:01:40 | order service | `append_with_id(order)` on root view | contribution 3; no per-turn check (root view has none) |
| 10:20:11 | console | `append(reply)` on root view | contribution 4 |
| 10:21:00 | console | `close()` | revision 1 sealed (`explicit`); queues `conversation-resolved` over contributions 1–4 |
| 11:00:00 | schedule | — | `behaviour-shift` compares the last hour's `turn-helpful/helpful` and `conversation-resolved/resolved` samples with the baseline; pages only if they shifted |
| day 3 | order service | `append_with_id(order: refund_failed)` | opens revision 2 = revision 1 + this; re-runs `conversation-resolved` on the whole conversation |

## Reading every result without being paged

```python
s = state.session("chat/c-7781").get()                               # NEW
s.revision                                       # 2
s.results["conversation-resolved"].verdict       # "failed" (revision 2)
s.results["conversation-resolved"].replaced      # [revision 1: "passed"]
[r.verdict for r in s.contributions[1].results]  # turn-helpful, turn-output on contribution 2
```

```bash
wyrd verification sessions get chat/c-7781                            # NEW
```

## What is new, user-side

1. `state.run(alias, session=key)` and `run.observe.append(**inputs)`, with an
   optional contribution ID; `state.session(key).close()` and `.get()`.
   Same names in Rust (`run_in_session`, `append`, `append_with_id`) and
   TypeScript.
2. Trigger kinds `evidence_appended` and `session_sealed` (with `session:`
   settings).
3. Verifier input `type: session` with `records:` declaring the names
   producers may append.
4. Requirement results recorded as samples `<verifier>/<requirement>`.

Differences from `spec.md` revision 1 draft: the append operation is reached
through `run.observe` rather than a free-standing call; per-turn filtering
comes from the binding's Card view plus required slots, so no event-kind filter
is added to Triggers; unknown appended names are refused; result samples are
not yet in the spec.
