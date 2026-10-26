# Support desk

One Agent-backed Service, the same story in Rust, Python, and TypeScript.

`service/` holds the Cards. `support-desk` declares the
`bifrost.datasets.tickets` table and binds one Verifier, `policy-check`, to its
`support-agent`. `policy-check` takes the `answer` and the `policy` PDF
(`refund-policy.pdf`):

- the `findings` step runs `policy-reader`, an Agent that extracts the PDF with
  `fs.extract`, reads the result with `fs.read`, and lists its refund rules with their pages;
- `no-refund-promise` asserts the answer never says "guaranteed refund";
- `follows-policy` asks `policy-judge`, a tool-less Agent, whether the answer
  follows those findings, citing the policy page.

Each language exports five steps and a runnable main:

| Step | What it does |
|---|---|
| `deploy` | Declares and pulls the Service, which creates `tickets`. Turns on gateway metadata capture and checks that the Agent's model is deployed. |
| `check` | Verifies one answer immediately with `verify` and returns its result. The main checks "You have a guaranteed refund.", which fails `no-refund-promise`. |
| `serve` | Answers 100 questions, each in its own Agent Run inside a `support-desk.request` span. For each one it invokes the Agent through the gateway, records the ticket, and `submit`s `policy-check` in the background, keeping the execution ID. Every tenth question asks for a refund, and its answer fails. |
| `wait_for_verdicts` | Resumes every execution by its ID and waits for its result. |
| `explain` | Joins one Run's ticket, span, gateway call, and verdict through MCP `bifrost.query`. |

The main also resumes the first failed check by its ID alone, as another
process would (`verify::Execution::connect(id).wait(...)` in Rust), and
prints each requirement's outcome and reason.

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
