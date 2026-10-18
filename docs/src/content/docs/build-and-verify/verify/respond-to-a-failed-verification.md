---
title: Respond to a failed verification
description: Page someone when a submitted or scheduled Verifier fails, with an Operator Card, an operator connection, and on_failure, then check that the page was delivered.
pillar: wyrd
section: Build and verify
topic: Verify
order: 5
status: runnable
---

An Operator is what Wyrd does when a Verifier fails: post to a webhook, page PagerDuty, or message Slack. You attach it to a binding with `on_failure`, and the server runs it once per failed execution. This page wires a daily latency check to an on-call webhook.

Operators run only for background work. A `failed` verdict from `submit` or a schedule runs them; `verify`, `passed`, `inconclusive`, and execution errors never do.

## 1. Create the connection

Secrets never go in a Card. A tenant administrator stores the webhook's origin and token once, as an operator connection:

```bash
wyrd operator-connection create --body-file - <<JSON
{"provider": "http", "name": "on-call-hooks", "origin": "https://hooks.example.com", "auth": {"scheme": "bearer", "token": "$ON_CALL_TOKEN"}}
JSON
```

The command prints the connection with its secret redacted. `wyrd operator-connection list` shows every connection in the tenant.

## 2. Write the Operator and bind it

The Operator names the connection and a path on its origin. The Service attaches it to the binding with `on_failure`:

<Tabs>
<Tab label="latency-watch.yaml">

<CodeFromFile file="fixtures/cards/scheduled_drift_alerts_operator/latency-watch.yaml" lang="yaml" title="latency-watch.yaml" />

</Tab>
<Tab label="alert-on-call.yaml">

<CodeFromFile file="fixtures/cards/scheduled_drift_alerts_operator/alert-on-call.yaml" lang="yaml" title="alert-on-call.yaml" />

</Tab>
<Tab label="latency-shift.yaml">

<CodeFromFile file="fixtures/cards/scheduled_drift_alerts_operator/latency-shift.yaml" lang="yaml" title="latency-shift.yaml" />

</Tab>
<Tab label="daily.yaml">

<CodeFromFile file="fixtures/cards/scheduled_drift_alerts_operator/daily.yaml" lang="yaml" title="daily.yaml" />

</Tab>
</Tabs>

When `latency-shift` fails, the server posts the `body` to `https://hooks.example.com/hooks/latency-shift` with `Authorization: Bearer <token>`. The `{{...}}` placeholders are filled from the failed execution:

| Placeholder | Value |
| --- | --- |
| `execution_id`, `result_id`, `dispatch_id`, `binding_id` | IDs to look the failure up by |
| `verifier_ref`, `verifier_card_uid` | The Verifier that failed |
| `subject_ref`, `subject_uid` | The Card it judged |
| `verdict`, `completed_at`, `summary` | What happened and when |
| `drift.drifted_features`, `drift.total_features` | For drift checks, how many features drifted |

The Operator's `auth.scheme` must match the connection's, and `url` is a path on the connection's origin, or a full URL with that same origin. Declare the Service as usual with `wyrd declare latency-watch.yaml`; declaration refuses an Operator whose connection is missing, disabled, or does not match. Declaring a binding with `on_failure` needs the `operators:invoke` scope.

Slack and PagerDuty use `kind: notify` with a `channel` of `{kind: slack, connection, channel_id, text}` or `{kind: pager_duty, connection, ...}`, against a `slack` or `pager_duty` connection.

## 3. Check it was delivered

Each Operator run is a dispatch on the execution. Find the latest execution of the binding:

```sql
SELECT execution_id, verdict, ended_at
FROM vala.verification.results
WHERE binding_id = $1
ORDER BY ended_at DESC
LIMIT 1
```

The binding ID is in the Service's Card status, under `status.verification.binding_ids`. Then read the dispatches:

<LangTabs langs={['python', 'rust', 'typescript']}>
<LangTab lang="python">

```python
from wyrd import verify

for dispatch in verify.Execution.connect(execution_id).status()["dispatches"]:
    print(dispatch["status"], dispatch.get("error"))
```

</LangTab>
<LangTab lang="rust">

```rust
use wyrd_ai::verify::Execution;

let state = Execution::connect(execution_id.parse()?).await?.status().await?;
for dispatch in &state.dispatches {
    println!("{:?} {:?}", dispatch.status, dispatch.error);
}
```

</LangTab>
<LangTab lang="typescript">

```typescript
import { verify } from "wyrd-ai";

const { dispatches } = await (await verify.Execution.connect(executionId)).status();
for (const dispatch of dispatches) console.log(dispatch.status, dispatch.error);
```

</LangTab>
</LangTabs>

A dispatch is `pending`, `running`, `retrying`, `delivered`, or `failed`. Each one is delivered and retried on its own, and a delivery failure never changes the verdict.

## What can go wrong

| Symptom or code | Cause | Fix |
| --- | --- | --- |
| `WYRD_SPEC_400_OPERATOR_CONNECTION_UNAVAILABLE` at declaration | No active connection has that name, or its auth scheme or origin differs from the Operator's | Create the connection first, and match `auth.scheme` and the origin |
| The check failed but `dispatches` is empty | The verdict came from `verify`, or it was `inconclusive` | Only a `failed` verdict from `submit` or a schedule runs Operators |
| A dispatch is `failed` | The webhook refused the request or was unreachable after retries | Read the dispatch's `error`, fix the receiver, and check the connection's token |
| Nothing ran at all | The binding is inactive or its baseline is not ready | [Check the binding](/build-and-verify/verify/run-a-verification-and-inspect-its-evidence/#4-check-a-binding) |
