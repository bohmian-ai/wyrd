---
title: Runs and observations
description: Wrap each request in a Run, call your model through the gateway, record what happened, verify it, and read the whole Run back.
pillar: wyrd
section: Build and verify
topic: Observe
order: 1
status: runnable
---

Wrap each request your app handles in a Run. Everything inside it (the model call, the rows you record, and each verdict) is tied to that Run and to the exact Card version that produced it, so you can read the whole request back with one query.

You need the support-desk Service from [Write a Verifier](/build-and-verify/verify/author-a-verifier/) and a running server from [Set up Wyrd](/get-started/setup/). Install the SDK with its telemetry and gateway extras, and the OpenAI client:

```bash
uv add "wyrd-ai[otel,gateway]==0.3.0" openai
```

## 1. Connect at startup

Once, when your app starts:

```python title="desk.py"
from openai import DefaultHttpxClient, OpenAI
from wyrd import WyrdClient, WyrdState

client = WyrdClient()  # finds the server and credential wyrd init saved
state = WyrdState.declare("service/support-desk.yaml", client)
state.start_bifrost()    # the writer for rows and verdict inputs
state.start_telemetry()  # exports spans started inside a Run

# Your model calls go through Wyrd's gateway. http_auth signs each call and
# ties it to the Run it is made in.
gateway = OpenAI(
    base_url=f"{client.server_url}/v1",
    api_key="wyrd",  # placeholder; http_auth replaces it
    http_client=DefaultHttpxClient(auth=client.http_auth),
)
prompt = state.prompt("prompt").prompt  # the Service's declared Prompt
```

`start_bifrost` is the first server call, so a wrong URL or credential fails here, at startup.

## 2. Answer inside a Run

Open a Run per request, and a view of the agent for the work the agent does:

```python title="desk.py"
def answer(question: str):
    with state.run() as run, run.for_card("agent") as agent:
        # Fill the Prompt's {{question}}; a missing variable raises instead of sending.
        messages = prompt.render(question=question).messages
        # Recorded by the gateway under this Run and the agent.
        reply = gateway.chat.completions.create(model="openai/gpt-4o", messages=messages)
        text = reply.choices[0].message.content

        # A row in the Service's tickets table, tagged with this Run.
        run.observe.record("vala.datasets.tickets", {"question": question, "answer": text})
        return run, agent, text
```

`openai/gpt-4o` is `<deployment>/<model>`: the [LLM gateway](/connect/set-up-the-llm-gateway/) deployment, then the model it sends upstream.

## 3. Verify the answer

Call each Verifier on the view it is bound to. `no-refund-promise` is bound to the Service, so it runs on `run`; `ticket-complete` is bound to the agent, so it runs on `agent`:

```python title="desk.py"
run, agent, text = answer("Can I get my money back?")

checked = run.observe.verify("no-refund-promise", answer=text)
ticket = agent.observe.verify("ticket-complete", ticket={"answer": text, "order_id": "A-1001"})
print(checked.verdict, ticket.verdict)
```

```text
passed passed
```

`verify` waits for the verdict. For a slow check, such as a model judge, `submit` queues it and returns an execution instead:

```python title="desk.py"
from wyrd import verify

execution = agent.observe.submit("follows-policy", letter=verify.File("letter.md"))
print(execution.id)
```

Read it back later, from any process, by that ID:

```python
status = verify.Execution.connect(execution_id, client).status()
print(status["status"])  # pending, running, or completed
```

## 4. Shut down cleanly

Rows and spans are sent in batches. Send what is still queued when the process stops:

```python
state.shutdown()
```

A process killed without it can lose its last rows and spans.

## 5. Read the Run back

Every row the Run produced carries its `run_id`, so one query joins the ticket, the gateway call, and each verdict:

```sql title="run.sql"
SELECT t.question, t.answer, g.call_id, r.verdict
FROM vala.datasets.tickets t
JOIN vala.gateway.calls g ON g.run_id = t.run_id
JOIN vala.verification.results r ON r.run_id = t.run_id
WHERE t.run_id = $1
```

```python
from wyrd.bifrost import Bifrost

rows = Bifrost().sql(open("run.sql").read(), [run.run_id]).to_arrow().to_pylist()
print(rows)
```

Rows become queryable a few seconds after `shutdown`. An empty result right away is not proof they are missing; query again.

<Aside type="note" title="In production">

Pull the exact Service version at deploy time with `wyrd pull`, load it with `WyrdState.from_path`, and give the app a key bound to that Service. [ASGI apps](/connect/integrate-asgi/) shows it.

</Aside>

## If it fails

| You see | Fix |
| --- | --- |
| `WYRD_SDK_400_BIFROST_NOT_STARTED` | Call `state.start_bifrost()` at startup, before any `observe` call |
| `WYRD_SDK_404_UNKNOWN_VERIFIER` | Call the Verifier on the view it is bound to: `run` for the Service, `agent` for the agent |
| `WYRD_VALA_404_BIFROST_TABLE_NOT_FOUND` | Declare the table under the Service's `spec.tables` |
| `WYRD_VALA_403_BIFROST_CARD_SCOPE` at `shutdown` | Use a key issued for this Service version |

## Next

[Runs and evidence](/understand/runs-observations-and-evidence/) explains what a Run records and how long it is kept.
