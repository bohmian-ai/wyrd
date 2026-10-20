# <img src="docs/src/assets/wyrd-mark.svg" alt="" width="30" height="30"> Wyrd

## Overview

**Open Source Verification Infrastructure for AI Systems**

Wyrd gives you the primitives to build your own verification factory, so you
can verify, understand, and control your AI systems. Declare your service and
its requirements, observe what happens in real time, and verify its output
against the declared requirements.

💬 **Join the community:** [Wyrd Discord](https://discord.gg/K5bf23yPNb)
— Ask questions, share what you're building, and help shape Wyrd.

**Coming this week:** Wyrd `v0.2.0` is targeted for October 19, 2026, with the
full verification primitive suite. Next release: the UI.

## Declare

One Service with one Agent, two Verifiers bound to that Agent, and one Bifrost
table:

```yaml
# refund-letters/service.yaml
apiVersion: wyrd/v1
kind: Service
metadata: {name: refund-letters, version: 1.0.0, space: default}
spec:
  service_type: agent
  tables:
    - name: letters
      schema:
        type: object
        properties:
          order_id: {type: string}
          letter: {type: string}
        required: [order_id, letter]
  components:
    - alias: agent
      ref: ./agent.yaml
      verified_by:
        - verifier: ./no-refund-promise.yaml
        - verifier: ./names-order.yaml
---
# refund-letters/agent.yaml
apiVersion: wyrd/v1
kind: Agent
metadata: {name: letter-writer, version: 1.0.0, space: default}
spec:
  prompt:
    provider: openai
    model: gpt-4o
    messages:
      - role: user
        content: "Write a refund decision letter for order {{order_id}}: {{question}}"
  execution_route: {kind: native}
  tool_names: []
---
# refund-letters/no-refund-promise.yaml: a simple assertion
apiVersion: wyrd/v1
kind: Verifier
metadata: {name: no-refund-promise, version: 1.0.0, space: default}
spec:
  inputs:
    letter: {type: json}
  requirements:
    - id: no-refund-promise
      statement: The letter never promises a guaranteed refund
      check: {kind: assert, path: inputs.letter, operator: not_contains, expected: guaranteed refund}
---
# refund-letters/names-order.yaml: parse the document, then check it
apiVersion: wyrd/v1
kind: Verifier
metadata: {name: names-order, version: 1.0.0, space: default}
spec:
  inputs:
    letter: {type: media, formats: [markdown]}
  steps:
    - id: read-letter
      action: {kind: extract, from: inputs.letter}
  requirements:
    - id: names-order
      statement: The letter states the order ID
      check: {kind: assert, path: steps.read-letter.output.text, operator: contains, expected: "Order ID:"}
```

Declare it, then download the declared versions for your app:

```bash
wyrd declare refund-letters/service.yaml
wyrd pull --kind Service --space default --name refund-letters --version 1.0.0 --output-dir .wyrd-bundle
```

## Observe

**Wyrd ships python, typescript and rust SDKs**

```python
from wyrd import WyrdState, verify

state = WyrdState.from_path(".wyrd-bundle")
state.start_bifrost()

def letter_writer(order_id: str) -> str:  # mock agent call
    return f"# Refund decision\n\nOrder ID: {order_id}\n\nYour $40 refund is approved.\n"

letter = letter_writer("A-1001")

run = state.run()
agent = run.for_card("agent")

# Verify now and wait for the verdict
result = agent.observe.verify("no-refund-promise", letter=letter)
print(result.verdict)  # passed

# Submit the document check to run in the background
execution = agent.observe.submit("names-order", letter=verify.File(letter.encode(), filename="letter.md"))

# Record the letter to the Service's Bifrost table
run.observe.record("vala.datasets.letters", {"order_id": "A-1001", "letter": letter})

state.shutdown()  # flush everything recorded
```

## Verify

The letter and both verdicts land in Bifrost under the same `run_id`, so one
join shows each verdict next to the letter it judged. The submitted check runs
in the background, so its verdict can arrive a moment after the first one:

```python
from wyrd.bifrost import Bifrost

rows = Bifrost().sql(
    """
    SELECT
      l.order_id,
      r.execution_id,
      r.verdict
    FROM vala.datasets.letters l
    JOIN vala.verification.results r
      ON r.run_id = l.run_id
    WHERE l.run_id = $1
    """,
    [run.run_id],
)
print(rows.to_polars())
```

To run Wyrd locally, start with [Set up Wyrd](https://docs.bohmian.ai/wyrd/get-started/setup/),
then [Verify your first agent](https://docs.bohmian.ai/wyrd/get-started/quickstart/).

## What the heck are you waiting for??????

Start verifying your system with Wyrd now!

## Contributing

Issues and pull requests are welcome! Read [CONTRIBUTING.md](CONTRIBUTING.md)
before starting a substantial change.

Want to discuss ideas, ask questions, or connect with other Wyrd developers?
[Join our Discord community](https://discord.gg/K5bf23yPNb).

Report vulnerabilities privately through [SECURITY.md](SECURITY.md).

## License

Wyrd is licensed under the [Apache License 2.0](LICENSE.md).
