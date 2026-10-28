---
title: Write a Verifier
description: Write a Verifier, bind it to your Service, call it from your code, and read its verdict, then add a Python check and a model judge.
pillar: wyrd
section: Build and verify
topic: Verify
order: 0
---

Build the Verifiers for a support desk: one assertion, one Python check, and one model judge. Each step adds a check, binds it, and shows the verdict your code gets back.

You need a running server from [Set up Wyrd](/get-started/setup/). The files live in a `service/` directory next to your program.

## 1. Write an assertion

A Verifier names the evidence it takes and what must be true of it. This one fails any answer that promises a guaranteed refund:

<CodeFromFile file="examples/support-desk-app/service/no-refund-promise.yaml" lang="yaml" title="service/no-refund-promise.yaml" />

An assertion runs no model, so its verdict is deterministic and returns in milliseconds. Every field is listed in the [Verifier Card reference](/reference/verifier-card/).

## 2. Bind it and check it

A Verifier runs only on what it is bound to. Add it under `verified_by` in your Service:

```yaml title="service/support-desk.yaml"
spec:
  verified_by:
    - verifier: ./no-refund-promise.yaml  # runs on the Service itself
```

Check every file without declaring anything:

```bash
wyrd declare --check service/support-desk.yaml
```

A misspelled field or a reference to a slot that does not exist fails here, naming the field.

## 3. Call it and read the verdict

Open a Run for each request your app handles, and verify the answer inside it:

```python title="app.py"
from wyrd import WyrdState

state = WyrdState.declare("service/support-desk.yaml")  # declares only what changed

with state.run() as run:  # one Run per request; everything inside is tied to it
    answer = support_agent("Can I get my money back?")  # your agent's own code
    # answer= fills the Verifier's answer slot; verify waits for the verdict
    result = run.observe.verify("no-refund-promise", answer=answer)

print(result.verdict)
for requirement in result.requirements:  # one outcome and reason per requirement
    print(requirement["id"], requirement["outcome"], requirement["reason"])
```

```bash
uv run python app.py
```

```text
passed
no-refund-promise passed ...
```

A `failed` verdict is an ordinary return, not an exception: the check ran and the answer broke the rule.

## 4. Add a Python check

When one operator cannot express the rule, write it in Python. This Verifier names every field a ticket is missing:

<CodeFromFile file="examples/support-desk-app/service/ticket-complete.yaml" lang="yaml" title="service/ticket-complete.yaml" />

The function must return the requirement's [verdict](/reference/verifier-card/#verdicts-and-citations), with exactly these fields:

| Field | Value |
| --- | --- |
| `outcome` | `passed`, `failed`, or `inconclusive` |
| `reason` | One sentence shown in the result |
| `citations` | Where in the evidence the decision rests, as `{slot, locator}` |

Any other shape, or an exception, makes the requirement `inconclusive`, and so does a `passed` outcome without a citation. The same function used as a step instead of a check may return any JSON. The code runs in Wyrd's sandbox with no files, network, or environment.

Tickets come from the agent, so bind this Verifier to the agent component rather than the whole Service:

```yaml title="service/support-desk.yaml"
spec:
  components:
    - alias: agent
      ref: ./support-agent.yaml
      verified_by:
        - verifier: ./ticket-complete.yaml
```

Verify on the Run's view of that component, inside the same `with` block:

```python
result = run.for_card("agent").observe.verify("ticket-complete", ticket={"answer": answer, "order_id": "A-1001"})
print(result.verdict)  # passed
```

## 5. Add a model judge

Some rules need judgment. `follows-policy` asks a model whether a refund decision letter follows the refund policy, which lives in the judge Agent's prompt:

<Tabs>
<Tab label="follows-policy.yaml">

<CodeFromFile file="examples/support-desk-app/service/follows-policy.yaml" lang="yaml" title="service/follows-policy.yaml" />

</Tab>
<Tab label="policy-judge.yaml">

<CodeFromFile file="examples/support-desk-app/service/policy-judge.yaml" lang="yaml" title="service/policy-judge.yaml" />

</Tab>
</Tabs>

The judge calls its model through your server's [LLM gateway](/connect/set-up-the-llm-gateway/), on the deployment named in `execution_route`. Its Prompt sets `response_format: verdict`, so the model answers with an outcome, a reason, and citations. Declaring a Verifier whose check Agent has no verdict format fails. Bind `follows-policy` beside `ticket-complete` on the agent.

A model call takes seconds, so submit the letter and collect the verdict later instead of waiting in the request:

```python
from wyrd import verify

# inside the same with block
execution = run.for_card("agent").observe.submit("follows-policy", letter=verify.File("letter.md"))
result = execution.wait(timeout=60)  # or later, from any process: verify.Execution.connect(id)
print(result.verdict)  # passed
```

## If it fails

| You see | Fix |
| --- | --- |
| `WYRD_REGISTRY_400_INVALID_CARD_SPEC` naming an unknown field | Fix the spelling against the [Verifier Card reference](/reference/verifier-card/) |
| `WYRD_SPEC_400_VALIDATION` with rule `unknown_reference` | Match the slot or step `id` the reference names |
| `WYRD_SDK_404_UNKNOWN_VERIFIER` | Verify on the view the Verifier is bound to: `run` for the Service, `run.for_card("agent")` for the agent |
| `WYRD_GATEWAY_404_MODEL_UNAVAILABLE` from the judge | Create the deployment the judge's `execution_route` names, as in [LLM gateway](/connect/set-up-the-llm-gateway/) |

## Next

Copy a check for your own case from [Recipes](/build-and-verify/verify/verifier-recipes/).
