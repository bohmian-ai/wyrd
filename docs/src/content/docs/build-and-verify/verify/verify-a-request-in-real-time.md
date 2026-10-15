---
title: Verify a request in real time
description: Call a bound Verifier from your service while a request is in flight, act on the verdict, and find the stored result.
pillar: wyrd
section: Build and verify
topic: Verify
order: 1
draft: true
status: runnable
---

Call `observe.verify` inside a run, get a verdict back in the same request, and decide what your service does next. Here the support desk checks that an answer never promises a refund before it goes out, and hands the ticket to a person if it does.

## Prerequisites

- A running service that loads its bundle and starts Bifrost, as in [Record a run and observations](/build-and-verify/observe/record-a-run-and-observations/).
- The Verifier bound to the component you call it on, in the Service's `verified_by`. A binding with no `runs_on` is only ever called from your code:

  ```yaml title="service/support-desk.yaml (excerpt)"
  - alias: agent
    ref: ./support-agent.yaml
    verified_by:
      - verifier: ./no-refund-promise.yaml
  ```

  ```yaml title="service/no-refund-promise.yaml"
  apiVersion: wyrd/v1
  kind: Verifier
  metadata:
    name: no-refund-promise
    version: 1.0.0
    space: default
  spec:
    description: The agent never promises a refund.
    implementation:
      kind: eval
      spec:
        pass_gate: {kind: all_pass}
        tasks:
          no_refund_promise:
            kind: assertion
            id: no_refund_promise
            context_path: $.answer
            operator: not_contains
            expected: guaranteed refund
  ```

- `verifier:run` on that Verifier. The `workload` role a registered Service gets includes it.

## 1. Call the Verifier and act on the verdict

<LangTabs langs={['python', 'rust', 'typescript']}>
<LangTab lang="python">

```python
with state.run("agent") as run:
    answer = run.invoke({"question": question})
    judgment = run.observe.verify("no-refund-promise", {"answer": answer})
    if not judgment.passed:
        answer = hand_to_person(ticket_id)  # your fallback
```

</LangTab>
<LangTab lang="rust">

```rust
let run = state.run_for_card("agent")?;
let mut answer = run.invoke(&[("question", question)]).await?;
let judgment = run
    .observe()
    .verify("no-refund-promise", &json!({ "answer": answer }))
    .await?;
if !judgment.passed() {
    answer = hand_to_person(ticket_id); // your fallback
}
```

</LangTab>
<LangTab lang="typescript">

```typescript
const run = state.run("agent");
let answer = await run.invoke({ question });
const judgment = await run.observe.verify("no-refund-promise", { answer });
if (!judgment.passed) {
  answer = await handToPerson(ticketId); // your fallback
}
```

</LangTab>
</LangTabs>

The name is the Verifier's Card name. The SDK looks it up among the Verifiers bound to `agent` in your loaded bundle, so a Verifier bound only to another component is not callable from this view.

Python blocks until the verdict arrives; Rust and TypeScript await it. Either way the call waits up to 60 seconds. A `failed` verdict is an ordinary return value, so check `passed` rather than catching an exception.

## 2. Read the judgment

| Field | Holds |
| --- | --- |
| `verdict` | `passed`, `failed`, or `inconclusive` |
| `passed` | `true` only for `passed`. An `inconclusive` verdict is not a pass |
| `kind` | What ran, such as `task_assertion`, `task_llm_judge`, `eval_assertion`, or `drift_psi` |
| `summary` | A one-line explanation you can log |
| `execution_id` | The stored result's `result_id` |

## 3. Confirm the result was stored

A real-time call stores its result in `vala.verification.results`, tied to your run, with `binding_id` null. It creates no observation and never runs Operators.

<LangTabs langs={['python', 'rust', 'typescript']}>
<LangTab lang="python">

```python
from wyrd.bifrost import Bifrost

rows = Bifrost().sql(
    "SELECT result_id, verdict, verifier_version FROM vala.verification.results "
    "WHERE run_id = $1 AND binding_id IS NULL",
    [run.run_id],
)
print(rows.to_pandas())
```

</LangTab>
<LangTab lang="rust">

```rust
use wyrd_ai::{Bifrost, bifrost::{BifrostOptions, QueryParam}};

let rows = Bifrost::connect(BifrostOptions::default())
    .await?
    .sql(
        "SELECT result_id, verdict, verifier_version FROM vala.verification.results \
         WHERE run_id = $1 AND binding_id IS NULL",
        &[QueryParam::String(run.run_id().to_string())],
    )
    .await?;
```

</LangTab>
<LangTab lang="typescript">

```typescript
import { Bifrost } from "wyrd-ai";

const bifrost = await Bifrost.connect();
const rows = await bifrost.sql(
  `SELECT result_id, verdict, verifier_version FROM vala.verification.results
    WHERE run_id = $1 AND binding_id IS NULL`,
  [run.runId],
);
```

</LangTab>
</LangTabs>

The row appears shortly after the call returns. The server stores it in the background, after it has already answered you, so the verdict your code received stands even in the rare case the row is lost, such as an abrupt server restart. Queries also read Bifrost on a best-effort basis: an empty result seconds after a call is not proof the row is missing. Retry before concluding that.

## Other Verifier kinds in real time

The same call works for every Verifier kind bound to the component; only the input changes:

| Kind | Input | Example |
| --- | --- | --- |
| `task` | One object, the context the check reads | `{"tool_call_names": [...]}` |
| `eval` | One object, the record the tasks read | `{"answer": answer}` |
| `drift` | A non-empty list of flat rows, one per sample | `[{"tenure_months": 14, "monthly_spend": 82.5}, ...]` |

An eval Verifier called this way runs its `assertion` and `llm_judge` tasks. `trace_assertion` and `agent_assertion` need a recorded trace and are refused in real time. Every SDK also accepts media references (`media=` in Python, `options.media` in TypeScript, `verify_with_media` in Rust) for eval and task inputs.

## What can go wrong

| Error code | Cause | Fix |
| --- | --- | --- |
| `WYRD_SDK_404_UNKNOWN_VERIFIER` | No Verifier with that name is bound to this component in the loaded bundle | Check the name and the component alias; reload the bundle after changing `verified_by` |
| `WYRD_SDK_400_INVALID_OBSERVATION` | The input has the wrong shape for the Verifier kind, such as an object for drift | Match the table above. Nothing was sent |
| `WYRD_PERMISSION_403_DENIED_RBAC` | The credential lacks `verifier:run` on this Verifier. `evals:run` alone is not enough | Grant a role that includes `verifier:run` |
| `WYRD_VERIFICATION_422_INPUT_UNSUPPORTED` | The eval Verifier has a `trace_assertion` or `agent_assertion` task | Bind it with `observations_ready` and let the [verifier runtime](/build-and-verify/verify/evaluate-agent-behavior/) run it |
| `WYRD_VERIFICATION_504_EXECUTION_TIMED_OUT` | The check took longer than 60 seconds, usually a slow LLM judge | Decide your fallback for this case in code. The SDK does not retry |

For when to check in real time versus letting the server check on its own, see [How verification works](/understand/how-verification-works/).
