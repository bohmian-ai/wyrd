---
title: Verify your first agent
description: Run the support-desk API against your local server, get verdicts from three Verifiers, and query the evidence behind them.
pillar: wyrd
section: Get started
order: 3
status: partial
---

You will run `support-desk`, a small customer-support API, against the local server from [Set up Wyrd](/get-started/setup/). Each question it answers is one Wyrd Run, and three Verifiers judge the answer:

| Verifier | Bound to | Checks | How it runs |
| --- | --- | --- | --- |
| `no-refund-promise` | The Service | The answer never contains `guaranteed refund` | Your code calls it and waits; no model |
| `ticket-complete` | The agent | The ticket has an answer and an order ID | Your code calls it and waits; sandboxed Python, no model |
| `follows-policy` | The agent | A refund decision letter the agent writes follows the refund policy | Your code queues it; the server runs a model judge in the background |

At the end you have a verdict for every check, each tied to the exact Card versions that ran, and one SQL query that joins it to the ticket, the trace, and the model call.

The code is the same app in Python, Rust, and TypeScript. This page runs the Python one; the tabs show the matching Rust and TypeScript code.

## Prerequisites

- A local server running from [Set up Wyrd](/get-started/setup/), in another terminal. The app reads the endpoint and credential that setup saved, so you export nothing.
- An OpenAI API key with access to `gpt-4o` (the agent) and `gpt-6-luna` (the policy judge).
- Python 3.11+ and `git`.

<Aside type="caution" title="Version">This page describes Wyrd 0.2.0, built from this repository. The published packages are 0.1.1; see [Set up Wyrd](/get-started/setup/) before mixing them.</Aside>

## 1. Get the example

```bash
git clone https://github.com/bohmian-ai/wyrd.git
cd wyrd/examples/support-desk-app/python
pip install "wyrd-ai[agents,gateway,otel]" fastapi uvicorn
```

The `agents` extra brings the OpenAI Agents SDK, `gateway` the HTTP client the gateway uses, and `otel` the trace exporter. The Cards the app runs live next door, in `examples/support-desk-app/service/`.

## 2. Connect the model

Wyrd calls OpenAI for you, so your code never holds the OpenAI key. If you accepted the OpenAI key during setup, with the proposed deployment name `openai`, this step is already done.

Otherwise, with `OPENAI_API_KEY` exported, apply one document. It names the variable instead of holding the key; the CLI reads the value and sends it once, and the server encrypts it, so no read ever returns it:

```bash
wyrd gateway deployment put --file - <<'JSON'
{
  "credentials": [{"name": "openai-key", "provider": "openai", "source": {"local_env": "OPENAI_API_KEY"}}],
  "deployments": [{"name": "openai", "adapter": "openai", "auth": {"bearer": {"credential": "openai-key"}}}]
}
JSON
```

The server creates the **credential** `openai-key` and the **deployment** `openai` together, or neither, and prints both with the secret redacted. Agents now reach OpenAI models as `openai/<model>`, and both Agent Cards in the example ask for exactly that with `execution_route: {kind: wyrd_gateway, deployment: openai}`. [Set up the LLM gateway](/connect/set-up-the-llm-gateway/) covers other providers and the separate credential and deployment commands.

## 3. Read what is declared

The example is seven Cards in `service/`. The Service names the others by relative path, such as `ref: ./support-agent.yaml`, so switch tabs to follow each reference:

<Tabs>
<Tab label="support-desk.yaml">

<CodeFromFile file="examples/support-desk-app/service/support-desk.yaml" lang="yaml" title="service/support-desk.yaml" />

</Tab>
<Tab label="support-agent.yaml">

<CodeFromFile file="examples/support-desk-app/service/support-agent.yaml" lang="yaml" title="service/support-agent.yaml" />

</Tab>
<Tab label="support-prompt.yaml">

<CodeFromFile file="examples/support-desk-app/service/support-prompt.yaml" lang="yaml" title="service/support-prompt.yaml" />

</Tab>
<Tab label="no-refund-promise.yaml">

<CodeFromFile file="examples/support-desk-app/service/no-refund-promise.yaml" lang="yaml" title="service/no-refund-promise.yaml" />

</Tab>
<Tab label="ticket-complete.yaml">

<CodeFromFile file="examples/support-desk-app/service/ticket-complete.yaml" lang="yaml" title="service/ticket-complete.yaml" />

</Tab>
<Tab label="follows-policy.yaml">

<CodeFromFile file="examples/support-desk-app/service/follows-policy.yaml" lang="yaml" title="service/follows-policy.yaml" />

</Tab>
<Tab label="policy-judge.yaml">

<CodeFromFile file="examples/support-desk-app/service/policy-judge.yaml" lang="yaml" title="service/policy-judge.yaml" />

</Tab>
</Tabs>

| Card | Role |
| --- | --- |
| `support-desk.yaml` | The Service: its components, its `tickets` table, and which Verifier checks what |
| `support-agent.yaml` | The agent. It runs `support-prompt` through the gateway's `openai` deployment |
| `support-prompt.yaml` | The prompt and model the agent runs |
| `no-refund-promise.yaml` | Bound to the Service. One assertion, no model |
| `ticket-complete.yaml` | Bound to the agent. A sandboxed Python check, no model |
| `follows-policy.yaml` | Bound to the agent. Extracts the text of the agent's letter, then asks `policy-judge` |
| `policy-judge.yaml` | A tool-less Agent with the refund policy in its prompt. Returns a verdict citing lines of the letter |

`verified_by` on the Service binds `no-refund-promise` to every Run of the Service. `verified_by` on the `agent` component binds the other two to the agent alone. Declaring the Service declares every Card it references, so the next step declares all seven.

## 4. Deploy the Cards

Check the declaration first. It validates all seven Cards and contacts no server:

```bash
wyrd declare ../service --check --format json
```

Then deploy:

```bash
python deploy.py
```

This declares all seven Cards, which creates the `vala.datasets.tickets` table. It then downloads the registered versions into `.wyrd-bundle/`, so the app runs exactly what the server holds, and turns on gateway metadata capture, so each model call is recorded with its Run:

<LangTabs langs={['python', 'rust', 'typescript']}>
<LangTab lang="python">

<CodeFromFile file="examples/support-desk-app/python/deploy.py" lang="python" region="deploy" title="deploy.py" />

</LangTab>
<LangTab lang="rust">

<CodeFromFile file="examples/support-desk-app/rust/lib.rs" lang="rust" region="deploy" title="lib.rs" />

</LangTab>
<LangTab lang="typescript">

<CodeFromFile file="examples/support-desk-app/typescript/deploy.ts" lang="typescript" region="deploy" title="deploy.ts" />

</LangTab>
</LangTabs>

It ends with `declared <card-uid> into .wyrd-bundle`. Declaring is idempotent: running it again with unchanged Cards changes nothing. Declaring needs the `editor` role; a `viewer` credential is refused with `WYRD_PERMISSION_403_DENIED_RBAC`.

## 5. Start the app

The app connects once at startup. `WyrdState.from_path` loads the pulled bundle, `start_bifrost` starts the buffered writer for the `tickets` table, and telemetry exports spans, including the Agents SDK's own traces, to Wyrd:

<LangTabs langs={['python', 'rust', 'typescript']}>
<LangTab lang="python">

<CodeFromFile file="examples/support-desk-app/python/desk.py" lang="python" region="init" title="desk.py" />

</LangTab>
<LangTab lang="rust">

<CodeFromFile file="examples/support-desk-app/rust/lib.rs" lang="rust" region="init" title="lib.rs" />

</LangTab>
<LangTab lang="typescript">

<CodeFromFile file="examples/support-desk-app/typescript/server.ts" lang="typescript" region="startup" title="server.ts" />

</LangTab>
</LangTabs>

Rows and spans are buffered, so they reach Wyrd in batches, not on every call. On exit the app flushes them. In Python and TypeScript, `state.shutdown()` flushes the spans of the provider `start_telemetry` installed, then drains the writer. In Rust, `start_telemetry(&state)` returns a `Telemetry` handle that the server keeps while it serves; it flushes that handle before draining the state:

<LangTabs langs={['python', 'rust', 'typescript']}>
<LangTab lang="python">

<CodeFromFile file="examples/support-desk-app/python/app.py" lang="python" region="lifespan" title="app.py" />

</LangTab>
<LangTab lang="rust">

<CodeFromFile file="examples/support-desk-app/rust/lib.rs" lang="rust" region="serve" title="lib.rs" />

</LangTab>
<LangTab lang="typescript">

<CodeFromFile file="examples/support-desk-app/typescript/server.ts" lang="typescript" region="shutdown" title="server.ts" />

</LangTab>
</LangTabs>

A process killed before `shutdown()` loses whatever was still buffered.

Start the API, and leave it running:

```bash
uvicorn app:app
```

## 6. Ask a question

In another terminal, ask a question:

```bash
curl -s localhost:8000/ask -H 'content-type: application/json' \
  -d '{"question": "Where is my order?", "order_id": "A-1001"}'
```

The handler answers in a new Run, records the ticket under it, then asks for both model-free verdicts and waits for them:

<LangTabs langs={['python', 'rust', 'typescript']}>
<LangTab lang="python">

<CodeFromFile file="examples/support-desk-app/python/desk.py" lang="python" region="answer" title="desk.py" />

<CodeFromFile file="examples/support-desk-app/python/desk.py" lang="python" region="check" title="desk.py" />

</LangTab>
<LangTab lang="rust">

<CodeFromFile file="examples/support-desk-app/rust/lib.rs" lang="rust" region="answer" title="lib.rs" />

<CodeFromFile file="examples/support-desk-app/rust/lib.rs" lang="rust" region="check" title="lib.rs" />

</LangTab>
<LangTab lang="typescript">

<CodeFromFile file="examples/support-desk-app/typescript/server.ts" lang="typescript" region="answer" title="server.ts" />

<CodeFromFile file="examples/support-desk-app/typescript/server.ts" lang="typescript" region="check" title="server.ts" />

</LangTab>
</LangTabs>

`run.observe.verify` checks against the Service's Verifiers; `run.for_card("agent").observe.verify` checks against the agent's. Both carry the same `run_id`.

Both calls are synchronous: `verify` returns once the verdict exists. The reply holds the Run ID, the answer, and each Verifier's verdict with one entry per requirement. This reply was captured from the Python SDK's integration run of this app, trimmed of each requirement's `check` and `provenance`; there a local model fixture wrote the answer, so your answer text will differ:

```json
{
  "run_id": "01a1512a-1238-7ea5-b9bb-a4f0ad0dab64",
  "answer": "Your order A-1001 ships tomorrow.",
  "checks": {
    "no-refund-promise": {
      "verdict": "passed",
      "requirements": [{
        "id": "no-refund-promise",
        "statement": "The answer never promises a guaranteed refund",
        "outcome": "passed",
        "reason": "the comparison held",
        "actual": "Your order A-1001 ships tomorrow."
      }]
    },
    "ticket-complete": {
      "verdict": "passed",
      "requirements": [{
        "id": "ticket-complete",
        "statement": "The ticket has an answer and an order id",
        "outcome": "passed",
        "reason": "all fields present",
        "citations": [{"slot": "ticket", "locator": {"json_pointer": ""}}]
      }]
    }
  }
}
```

Now drop the order ID and ask again:

```bash
curl -s localhost:8000/ask -H 'content-type: application/json' \
  -d '{"question": "Where is my order?"}'
```

`ticket-complete` fails with the reason `missing order_id`. That is a verdict, not an error: the request succeeds, and your code decides what to do with a failed check. Keep the `run_id` from either reply for step 8.

A verify call that names a Verifier not bound to the Run's Card fails with `WYRD_SDK_404_UNKNOWN_VERIFIER`; evidence for a slot the Verifier doesn't declare fails with `WYRD_VERIFICATION_400_INPUT_INVALID`. Those are errors, not verdicts.

## 7. Queue a policy review

Some answers are documents. `POST /reviews` asks the agent to write a refund decision letter in Markdown, uploads the letter as the evidence for `follows-policy`, and replies at once with an execution ID. The judge calls a model, so the customer doesn't wait for it. `submit` only queues the check: a returned execution ID means the work is staged, not that a verdict exists:

```bash
curl -s localhost:8000/reviews -H 'content-type: application/json' \
  -d '{"question": "Can I get a refund on a damaged item?"}'
```

<LangTabs langs={['python', 'rust', 'typescript']}>
<LangTab lang="python">

<CodeFromFile file="examples/support-desk-app/python/desk.py" lang="python" region="submit" title="desk.py" />

</LangTab>
<LangTab lang="rust">

<CodeFromFile file="examples/support-desk-app/rust/lib.rs" lang="rust" region="submit" title="lib.rs" />

</LangTab>
<LangTab lang="typescript">

<CodeFromFile file="examples/support-desk-app/typescript/server.ts" lang="typescript" region="submit" title="server.ts" />

</LangTab>
</LangTabs>

The server runs it: an `extract` step turns the letter into text, then the tool-less `policy-judge` Agent, whose prompt holds the refund policy, decides whether the letter follows it and cites the lines it relied on. The policy never travels with the request; changing it means a new judge version, so every verdict names the policy it applied. Read the result back by its ID:

```bash
curl -s localhost:8000/reviews/<execution-id>
```

<LangTabs langs={['python', 'rust', 'typescript']}>
<LangTab lang="python">

<CodeFromFile file="examples/support-desk-app/python/app.py" lang="python" region="status" title="app.py" />

</LangTab>
<LangTab lang="rust">

<CodeFromFile file="examples/support-desk-app/rust/lib.rs" lang="rust" region="status" title="lib.rs" />

</LangTab>
<LangTab lang="typescript">

<CodeFromFile file="examples/support-desk-app/typescript/server.ts" lang="typescript" region="status" title="server.ts" />

</LangTab>
</LangTabs>

The first read can arrive before the queued execution is visible, and answers `404`:

```json
{"code": "WYRD_VERIFICATION_404_EXECUTION_NOT_FOUND", "message": "no verification execution with this ID in the caller's tenant"}
```

Read again. `status` then moves from `pending` to `running` to `completed`. A completed review, captured from the same integration run and trimmed of `check` and `provenance`:

```json
{
  "execution_id": "d4be3e01-88fd-7d99-a6e7-5fbf4b775bef",
  "status": "completed",
  "verdict": "passed",
  "requirements": [{
    "id": "follows-policy",
    "statement": "The letter's refund decision follows the refund policy",
    "outcome": "passed",
    "reason": "a 40 dollar refund within 30 days is approved",
    "citations": [{"slot": "letter", "locator": {"lines": {"start": 4, "end": 5}}}]
  }]
}
```

The citation points at lines 4 and 5 of the letter. Its `provenance` names the exact judge Agent, Prompt version, and gateway route that decided. `errored`, `timed_out`, and `cancelled` are terminal too: they carry an `error` with a stable code instead of a verdict, so stop polling on any of them. `retrying` means read again. [Run a verification and inspect its evidence](/build-and-verify/verify/run-a-verification-and-inspect-its-evidence/) covers every status, and what to do if an execution stays not found.

Nothing about the ID is tied to the process that queued it. Any process with the credential can read it.

## 8. Query the evidence

Every piece of the Run landed in Bifrost, Wyrd's warehouse, keyed by the same `run_id`. One query joins the ticket, the request span, the gateway call, and every verdict:

<CodeFromFile file="examples/support-desk-app/python/evidence.py" lang="python" region="sql" title="evidence.py" />

```bash
python evidence.py <run-id>
```

It prints one row per gateway call and verdict for that Run. For the first `/ask` Run above, after the app had shut down and flushed, the integration run printed two rows, one per verdict:

```text
{'run_id': '01a1512a-1238-7ea5-b9bb-a4f0ad0dab64', 'question': 'Where is my order?', 'answer': 'Your order A-1001 ships tomorrow.', 'trace_id': b'e\xf8\xf7+AS7\x9b>=K\xca?|I\x08', 'call_id': '01a1512a-1322-7780-8ea1-475e61e2de0d', 'execution_id': '01a1512a-1348-7223-9310-e27a7b1f114e', 'verdict': 'passed'}
{'run_id': '01a1512a-1238-7ea5-b9bb-a4f0ad0dab64', 'question': 'Where is my order?', 'answer': 'Your order A-1001 ships tomorrow.', 'trace_id': b'e\xf8\xf7+AS7\x9b>=K\xca?|I\x08', 'call_id': '01a1512a-1322-7780-8ea1-475e61e2de0d', 'execution_id': '01a1512a-1341-7a51-ac0a-b2e5d3c5c3cc', 'verdict': 'passed'}
```

Evidence is not instant. Tickets and spans are buffered in the app, and verdicts are written in batches by the server, so a query right after a request can return fewer rows. Query again, or stop the app first so it flushes. The same SQL runs from the CLI with `wyrd query --sql`.

## If it fails

| You see | Cause | Fix |
| --- | --- | --- |
| A gateway error naming `openai/gpt-4o` or `openai/gpt-6-luna` | No `openai` deployment, or the key lacks that model | Run [step 2](#2-connect-the-model) and check the key's model access |
| `WYRD_CLIENT_401_NO_CREDENTIALS` | The app found no saved credential | Run it with the same `WYRD_CONFIG_HOME` as the server from setup |
| `WYRD_VALA_409_BIFROST_FINGERPRINT_MISMATCH` | A `vala.datasets.tickets` table with different columns already exists in this tenant | Use a fresh tenant in its own config directory, such as `WYRD_CONFIG_HOME=~/.config/wyrd-quickstart wyrd server dev --tenant quickstart` |
| The app can't find `.wyrd-bundle` | The Cards were never deployed | Run `python deploy.py` from the `python/` directory |
| A review stays `pending` | The server is not running background verification | Check the server terminal for errors |
| `WYRD_VERIFICATION_404_EXECUTION_NOT_FOUND` right after `/reviews` | The queued execution is not visible yet | Read it again |
| The evidence query returns no rows | Rows are still buffered in the app or the server | Query again, or stop the app so it flushes |
| `WYRD_CLI_409_SERVER_VERSION_INCOMPATIBLE` | The CLI and server are different minor versions | Use a CLI and server from the same release |

## Adapt it to your own app

The same three moves apply to any application you own:

1. **Declare.** Write a Service Card whose `components` pin each Agent, Prompt, or other part by exact version, whose `tables` hold the rows you record, and whose `verified_by` binds Verifiers. A hosted model your Prompt names, such as `openai/gpt-4o`, is not a Model Card; declare a `Model` Card only for a model artifact you own and version. Run `wyrd declare <dir> --check --format json`, then `wyrd declare <dir> --format json`, then `wyrd pull` the root into a bundle. [Define a Service](/build-and-verify/declare/define-a-service/) covers each field.
2. **Observe.** Load the bundle with `WyrdState.from_path`, start Bifrost and telemetry once at startup, wrap each unit of work in `state.run()`, record rows with `run.observe.record`, and call `state.shutdown()` on exit. [Record a Run and observations](/build-and-verify/observe/record-a-run-and-observations/) shows each call.
3. **Verify.** Call `verify` where your code must act on the verdict, and `submit` where it must not wait, then read the execution by ID until it is terminal. [Run a verification and inspect its evidence](/build-and-verify/verify/run-a-verification-and-inspect-its-evidence/) covers both.

A coding agent connected through MCP can follow the same path: `wyrd.guide {"topic": "start"}` returns it step by step.

## Clean up

Stop the API with Ctrl-C, then remove the deployment and the stored key:

```bash
wyrd gateway deployment delete openai
wyrd gateway credential delete openai-key
```

Your Cards and verdicts stay in the database and `.wyrd/`. Delete those as described in [Set up Wyrd](/get-started/setup/#stop-and-restart) to start over.

## Next

- [Set up the LLM gateway](/connect/set-up-the-llm-gateway/): route other providers and see what each call records.
- The Rust and TypeScript tabs are reference copies, compiled against the public SDKs by this repository's `check:examples` and `ts:typecheck` lanes. The runnable apps, with manifests and tests, are in the `wyrd-example-py`, `wyrd-example-rs`, and [`wyrd-example-ts`](https://github.com/bohmian-ai/wyrd-example-ts) repositories.
