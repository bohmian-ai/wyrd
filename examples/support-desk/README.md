# Support desk

One Agent-backed Service, the same story in Rust, Python, and TypeScript.

`service/` holds the Cards. `support-desk` declares the
`vala.datasets.tickets` table and binds two Verifiers to its `support-agent`:

- `answer-quality` is an LLM judge that runs continuously on every Eval
  observation.
- `no-refund-promise` is an assertion that `serve` calls in real time.

Each language exports four steps and a runnable main:

| Step | What it does |
|---|---|
| `deploy` | Registers and hydrates the Service, which creates `tickets`. Turns on gateway metadata capture and checks that the Agent's model is deployed. |
| `serve` | Answers 100 questions, each in its own Agent Run inside a `support-desk.request` span. For each one it invokes the Agent through the gateway, records the ticket, observes the answer, and verifies it. Every tenth question asks for a refund. |
| `wait_for_verdicts` | Waits until both Verifiers have judged every answer. |
| `explain` | Joins one Run's observation, ticket, span, gateway call, and both verdicts through MCP `bifrost.query`. |

Run it against a server that has a `gpt-4o` deployment, with `WYRD_SERVER_URL`
and `WYRD_API_KEY` set:

```bash
mise run examples:rust:support-desk
mise run examples:python:support-desk
mise run examples:typescript:support-desk
```

The SDK journeys run this same code against a local test server:

- `sdks/wyrd-sdk-rust/tests/integration/support_desk.rs`
- `sdks/wyrd-sdk-python/tests/integration/test_support_desk.py`
- `sdks/wyrd-sdk-ts/wyrd/tests/integration/support-desk.test.ts`
