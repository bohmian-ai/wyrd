---
title: Evaluate agent behavior
description: Write an eval Verifier, bind it so the server judges every eval observation, and read the verdict and per-task results.
pillar: wyrd
section: Build and verify
topic: Verify
order: 2
draft: true
status: runnable
---

Bind an eval Verifier with `observations_ready`, and the server judges every eval observation your agent records, without your code waiting on it. Here an LLM judge grades every answer the support desk gives.

## Prerequisites

- The `support-desk` declaration from [Cards, versions, and relationships](/understand/cards-versions-and-relationships/), and `editor` to register it.
- A running service that records eval observations, as in [Record a run and observations](/build-and-verify/observe/record-a-run-and-observations/).
- For an `llm_judge` task, a judge Prompt Card whose model your server's AI Gateway can reach. `judge_ref` takes an inline judge like the one below, or a reference to an Agent Card.

## 1. Write the Verifier

```yaml title="service/answer-quality.yaml"
apiVersion: wyrd/v1
kind: Verifier
metadata:
  name: answer-quality
  version: 1.0.0
  space: default
spec:
  description: An LLM judge grades every observed answer.
  implementation:
    kind: eval
    spec:
      pass_gate: {kind: all_pass}
      tasks:
        judge:
          kind: llm_judge
          id: judge
          judge_ref:
            prompt: ./judge-prompt.yaml
            tool_names: []
            run_config: {max_iterations: 1}
          context_path: $.answer
          operator: equals
          expected: {passed: true}
          max_retries: 0
```

`judge-prompt.yaml` asks the model to grade the answer and return `{"passed": true}` or `{"passed": false}` through a JSON schema, so the result compares like any other value.

An LLM call per answer adds up. To skip the judge on answers that already fail a cheap check, add an assertion and make the judge depend on it:

```yaml
      tasks:
        not_empty:
          kind: assertion
          id: not_empty
          context_path: $.answer
          operator: is_non_empty
        judge:
          kind: llm_judge
          id: judge
          depends_on: [not_empty]
          # ...as above
```

The rules that trip people up:

- **Set `pass_gate`.** Without one, every run ends `inconclusive` and no Operator ever fires. `all_pass` requires every task to pass; `overall_pass_rate` and `per_judge_pass_rate` take a `threshold` between 0 and 1.
- **Each task's key must equal its `id`.** Registration rejects a mismatch.
- **`depends_on` orders tasks.** A task runs only after the tasks it depends on pass, so you do not pay for an LLM call on an answer that already failed. A task skipped because of a failed dependency appears as skipped, not passed.
- **`context_path` is a JSONPath into the observation's context.** It must match the field names your code records.
- **A judge's output is compared like any other value.** The judge returns a structured object; `expected: { passed: true }` passes when the judge says so.

To judge only some observations, add `sampling`: `{ kind: ratio, ratio: 0.1 }`, `{ kind: every_nth, n: 10 }`, or `{ kind: deterministic_by_hash, key_path, modulus, bucket }`. A record sampling drops gets an `inconclusive` verdict.

## 2. Bind it and register

```yaml title="service/support-desk.yaml (excerpt)"
- alias: agent
  ref: ./support-agent.yaml
  verified_by:
    - verifier: ./answer-quality.yaml
      runs_on: {kind: observations_ready}
      on_failure:                          # not in the example: page someone on a failed verdict
        - ./page-support-oncall.yaml
```

```bash
wyrd apply examples/support-desk/service/
```

The binding changes the Service's content, so it needs a new Service version. `support-desk.yaml` pins `version: 1.0.0`; bump it, for example to `1.1.0`, or `wyrd apply` fails with `WYRD_REGISTRY_409_SPEC_DRIFT`. Then redeploy your service with a bundle of that version (`wyrd get ... --version <new>`); a running process keeps the bindings of the version it loaded.

## 3. Record eval observations

Nothing new in your service code. Every `observe.eval` on the `agent` run becomes a run of the Verifier once Bifrost commits it:

<LangTabs langs={['python', 'rust', 'typescript']}>
<LangTab lang="python">

```python
with state.run("agent") as run:
    answer = run.invoke({"question": question})
    run.observe.eval({"answer": answer})
```

</LangTab>
<LangTab lang="rust">

```rust
let run = state.run_for_card("agent")?;
let answer = run.invoke(&[("question", question)]).await?;
run.observe().eval(&json!({ "answer": answer }), EvalObservationOptions::default())?;
```

</LangTab>
<LangTab lang="typescript">

```typescript
const run = state.run("agent");
const answer = await run.invoke({ question });
run.observe.eval({ answer });
```

</LangTab>
</LangTabs>

An eval observation about `support-agent` runs the `observations_ready` bindings on that Agent owned by the exact Service version whose credential wrote it. Another Service that also uses `support-agent` gets no run from your observations, and neither does an older `support-desk` version. The verdict is usually ready within seconds. A Verifier with `trace_assertion` tasks waits up to 5 minutes for the run's trace to land, then errors with `eval_trace_unavailable` if it never does.

## 4. Read the verdicts

Join each verdict to the observation it judged through `source_record_id`, then to the per-task rows through `result_id`:

<LangTabs langs={['python', 'rust', 'typescript']}>
<LangTab lang="python">

```python
from wyrd.bifrost import Bifrost

bifrost = Bifrost()
rows = bifrost.sql(
    """
    SELECT r.verdict, i.task_id, i.passed, i.skip_reason, i.message
    FROM vala.eval.observations AS o
    JOIN vala.verification.results AS r ON r.source_record_id = o.record_id
    JOIN vala.eval.result_items AS i ON i.result_id = r.result_id
    WHERE o.run_id = $1
    ORDER BY r.ended_at, i.task_id
    """,
    [run.run_id],
)
print(rows.to_pandas())
```

</LangTab>
<LangTab lang="rust">

```rust
use wyrd_ai::{Bifrost, bifrost::QueryParam};

let bifrost = Bifrost::from_env().await?;
let rows = bifrost
    .sql(
        "SELECT r.verdict, i.task_id, i.passed, i.skip_reason, i.message \
         FROM vala.eval.observations AS o \
         JOIN vala.verification.results AS r ON r.source_record_id = o.record_id \
         JOIN vala.eval.result_items AS i ON i.result_id = r.result_id \
         WHERE o.run_id = $1 \
         ORDER BY r.ended_at, i.task_id",
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
  `SELECT r.verdict, i.task_id, i.passed, i.skip_reason, i.message
     FROM vala.eval.observations AS o
     JOIN vala.verification.results AS r ON r.source_record_id = o.record_id
     JOIN vala.eval.result_items AS i ON i.result_id = r.result_id
    WHERE o.run_id = $1
    ORDER BY r.ended_at, i.task_id`,
  [run.runId],
);
```

</LangTab>
</LangTabs>

One row per task for the judged answer. To read a whole conversation, record a `session_id` with each eval observation and filter on `o.session_id` instead. A failed verdict names the task that failed in `task_id`, with `actual`, `expected`, and `message` columns explaining why. A task with a `skip_reason` did not run; it is not a pass.

If no verdict appears for an observation, the run may still be pending or may have errored without producing one. [Run a verification and inspect its evidence](/build-and-verify/verify/run-a-verification-and-inspect-its-evidence/) shows how to read the binding and run status.

## What can go wrong

| Symptom or code | Cause | Fix |
| --- | --- | --- |
| Every verdict is `inconclusive` | No `pass_gate`, or every task skipped | Add `pass_gate`; check that `context_path` matches the recorded fields |
| `WYRD_SPEC_400_TRIGGER_ACTIVATION_MISMATCH` at `wyrd apply` | `schedule` on an eval Verifier | Use `observations_ready` |
| Registration error naming a task key | A task's key differs from its `id` | Make them match |
| No result rows at all | The service runs an older Service version without the binding, or wrote no eval observations | Deploy the new version; query `vala.eval.observations` for the run |
| `WYRD_VALA_403_BIFROST_CARD_SCOPE` at `flush` or `shutdown` | The service recorded about a Card outside its declaration | Record only on aliases in its own Service |

To check an answer before it goes out instead of after, call the same Verifier in [real time](/build-and-verify/verify/verify-a-request-in-real-time/).
