---
title: Background checks
description: Queue a Verifier with submit, follow its execution from any process, tell a failed verdict from a check that never ran, and read the stored evidence.
pillar: wyrd
section: Build and verify
topic: Verify
order: 3
draft: true
---

`submit` queues a Verifier and returns an execution ID right away. This guide follows that ID to a result: is it still running, did it settle without a verdict, what did each requirement find, and what evidence was it judged on. It also covers what a `202` does and does not promise.

## Prerequisites

- A Verifier bound to the run's Card. Here `my-review` takes one file slot, `document`.
- `verifier:run` to submit, and the same principal or `cards:read` plus `bifrost_query:read` to inspect.

## 1. Submit and keep the ID

<LangTabs langs={['python', 'rust', 'typescript']}>
<LangTab lang="python">

```python
from wyrd import verify

with state.run() as run:
    execution = run.observe.submit("my-review", document=verify.File("report.pdf"))
save(execution.id)
```

</LangTab>
<LangTab lang="rust">

```rust
use wyrd_ai::verify::{File, Input, Inputs, Submission};

let run = state.run();
let submission = run
    .observe()
    .submit("my-review", Inputs::named([("document", Input::from(File::path("report.pdf")))]))
    .await?;
if let Submission::Queued(execution) = submission {
    save(execution.id().to_string());
}
```

</LangTab>
<LangTab lang="typescript">

```typescript
import { verify } from "wyrd-ai";

const run = state.run();
const submitted = await run.observe.submit("my-review", { document: verify.File.path("report.pdf") });
if (submitted.kind === "queued") save(submitted.execution.id);
```

</LangTab>
</LangTabs>

`submit` uploads any missing files, then sends `POST /v1/verification/enqueue` with an `Idempotency-Key`. A transport retry reuses the key, so it returns the same execution ID instead of queuing twice.

### What the 202 means

The `202` and its `execution_id` confirm the evidence was staged in the server's memory, not that the execution is queued:

- The server queues it after its evidence is durably written, in the background. Until then, reading the execution returns not found.
- A second submission with the same key but different content is refused at that point, not at the `202`. It settles as `errored` with `WYRD_REGISTRY_409_IDEMPOTENCY_CONFLICT`, never runs, and leaves the original execution unchanged.
- If the server stops abruptly before the evidence is written, the submission is lost: it never becomes visible, and `wait` times out. Submit again if you need the verdict; nothing is retried for you.
- A completed execution can rarely lack a stored result for the same reason, so `wait` keeps polling and never reports a verdict it has not read.

## 2. Wait for the result

From the same process, or from another one using only the ID:

<LangTabs langs={['python', 'rust', 'typescript']}>
<LangTab lang="python">

```python
import wyrd

execution = wyrd.verify.Execution.connect(execution_id)
result = execution.wait(900)
print(result.verdict)
```

</LangTab>
<LangTab lang="rust">

```rust
use std::time::Duration;
use wyrd_ai::verify::Execution;

let execution = Execution::connect(execution_id.parse()?).await?;
let result = execution.wait(Duration::from_secs(900)).await?;
println!("{:?}", result.outcome.verdict);
```

</LangTab>
<LangTab lang="typescript">

```typescript
import { verify } from "wyrd-ai";

const execution = await verify.Execution.connect(executionId);
const result = await execution.wait({ timeoutMs: 900_000 });
console.log(result.outcome.verdict);
```

</LangTab>
</LangTabs>

`connect` uses the ambient credentials, or the client you pass (`client=` in Python, `{ client }` in TypeScript, `connect_with` in Rust). It reads as that principal, so it sees only executions in its own tenant that it may read.

`wait` polls `GET /v1/verification/executions/{execution_id}`, backing off from 100 ms to 2 s. It returns the same `VerificationResult` that `verify` returns, raises the execution's own error when it settled without a verdict, and raises `WYRD_SDK_504_VERIFICATION_WAIT_TIMEOUT` when the timeout passes first. A timeout does not cancel the execution; wait again later.

To read once without waiting, call `status()`:

| `status` | Means | Next |
| --- | --- | --- |
| not found | Not queued yet, lost before it was, or not visible to you | Read again; after a few minutes, treat it as lost |
| `pending`, `running`, `retrying` | Not finished | Read again |
| `completed` | A verdict exists; `result` holds it once stored | Read `result.outcome` |
| `errored`, `timed_out`, `cancelled` | The Verifier did not reach a verdict | Read `error.code` and `error.message`. No Operator runs |

Status is not the verdict. A completed execution of a passing check and of a failing one look the same until you read the verdict (`result.verdict` in Python, `result.outcome.verdict` in Rust and TypeScript).

## 3. Read the evidence

The result explains itself; you rarely need SQL:

<LangTabs langs={['python', 'rust', 'typescript']}>
<LangTab lang="python">

```python
for step in result.steps:
    print(step["id"], step["status"], step["attempts"], step.get("reason"))
for requirement in result.requirements:
    print(requirement["id"], requirement["outcome"], requirement["reason"], requirement["citations"])
```

</LangTab>
<LangTab lang="rust">

```rust
for step in &result.outcome.steps {
    println!("{} {:?} {} {:?}", step.id, step.status, step.attempts, step.reason);
}
for requirement in &result.outcome.requirements {
    println!("{} {:?} {}", requirement.id, requirement.outcome, requirement.reason);
}
```

</LangTab>
<LangTab lang="typescript">

```typescript
for (const step of result.outcome.steps) {
  console.log(step.id, step.status, step.attempts, step.reason, step.tool_calls);
}
for (const requirement of result.outcome.requirements) {
  console.log(requirement.id, requirement.outcome, requirement.reason, requirement.citations);
}
```

</LangTab>
</LangTabs>

| Where | What it tells you |
| --- | --- |
| `outcome.evidence` | The exact inputs judged, by slot, with file digests |
| `outcome.steps[]` | Whether each step `succeeded`, `failed`, or was `skipped`, how many attempts it took, and every tool it called with argument and output digests |
| `outcome.requirements[]` | Each verdict, its reason, the actual value, citations into the original files, and the check's provenance: code hash, extraction version, Agent and Prompt versions, effective model route, drift window and baseline |
| `outcome.requirements[].data` | The custom data an `agent` or `code` check returned, when it declares a [custom data](/reference/verifier-card/#custom-data) schema; absent otherwise, or when the data failed its schema |

An `inconclusive` requirement whose step `failed` means the evidence could not be prepared; read the step `reason`. One with a citation reason means a judge passed without valid evidence.

The same result is stored in `bifrost.verification.results`, and the pinned inputs in `bifrost.verification.execution_inputs`; query them with `bifrost.query` over MCP or the SDK's `Bifrost`:

```sql
SELECT verdict, verifier_version, run_id, binding_id
FROM bifrost.verification.results
WHERE execution_id = $1
```

## 4. Check a binding

When a scheduled Verifier seems never to run, read its binding with `verification.get_binding` (`GET /v1/verification/bindings/{binding_id}`). The binding IDs are in the Service's Card status, `status.verification.binding_ids`.

| Field | Tells you |
| --- | --- |
| `card_uid`, `verifier_card_uid` | Exactly what is checked, and by which Verifier version |
| `readiness` | `ready`, `baseline_not_ready` (a drift baseline is still fitting or failed), or `verifier_unavailable` |
| `active` | Whether the Service has authenticated in the last 24 hours. An inactive binding does not run on its schedule |
| `next_run_at` | The next scheduled window |

If `active` is false, deploy the Service. If `readiness` is `baseline_not_ready`, see [Monitor drift](/build-and-verify/verify/monitor-drift/#4-wait-for-the-baseline-to-fit).

## Operators

Only a `failed` verdict from a submitted or scheduled execution runs the binding's `on_failure` Operators, once per execution even if the request was replayed. `passed`, `inconclusive`, execution errors, and `verify` calls never do. Each dispatch is delivered and retried on its own; a delivery failure never changes the verdict. To set one up, see [Respond to a failed verification](/build-and-verify/verify/respond-to-a-failed-verification/).

## What can go wrong

| Code or symptom | Cause | Fix |
| --- | --- | --- |
| `WYRD_SDK_504_VERIFICATION_WAIT_TIMEOUT` | The execution has not settled, or the submission was lost before it was queued | Wait again; if `status()` stays not found, submit again |
| `WYRD_REGISTRY_409_IDEMPOTENCY_CONFLICT` from `wait` | The same `Idempotency-Key` was reused with different content | Use a new key for new content; the original execution is unaffected |
| `WYRD_PERMISSION_403_DENIED_RBAC` on submit | No `verifier:run` on the bound Verifier | Use a credential with `workload` or a role that includes it |
| Not found from another process | That principal is in another tenant or may not read the execution | Connect with the submitting principal or one granted read access |
| `completed` but no Operator fired | The verdict was `passed` or `inconclusive` | Read `result.outcome.verdict` |
