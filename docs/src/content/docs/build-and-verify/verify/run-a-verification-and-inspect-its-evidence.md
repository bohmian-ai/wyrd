---
title: Run a verification and inspect its evidence
description: Find a binding, start or follow its run, tell a failed verdict from a check that never ran, and read the stored result.
pillar: wyrd
section: Build and verify
topic: Verify
order: 4
draft: true
status: partial
---

When a check fails, or seems to have never run, follow it from the binding to the run to the stored result. Each step answers a different question: is the binding able to run, did the run execute, what did it find, and were the Operators delivered.

The SDKs can find a binding ID and read results. Reading bindings and runs, and starting a run, are MCP and HTTP only for now. This guide shows the MCP tool arguments; each matches the HTTP body for the route named beside it.

## Prerequisites

- A Service with at least one binding, such as `support-desk` from [Monitor drift](/build-and-verify/verify/monitor-drift/) or [Evaluate agent behavior](/build-and-verify/verify/evaluate-agent-behavior/).
- `cards:read` and `bifrost_query:read` to inspect. `verifier:run` to start a run. The `viewer` role covers inspecting.
- An [MCP connection](/connect/connect-an-agent-through-mcp/), or `curl` with a bearer credential.

## 1. Find the binding

The server assigns each `verified_by` entry a binding ID when the Service registers. Read them from the Service's Card status:

<LangTabs langs={['python', 'rust', 'typescript']}>
<LangTab lang="python">

```python
from wyrd.cards import CardRef, Cards

service = Cards().get(CardRef(kind="Service", name="support-desk", version="1.0.0", space="default"))
print(service.status.verification.binding_ids)
```

</LangTab>
<LangTab lang="rust">

```rust
use wyrd_ai::cards::{CardKind, CardSelector, Cards};

let selector = CardSelector::named(CardKind::Service, "default".parse()?, "support-desk".parse()?)
    .with_version("1.0.0".parse()?);
let service = Cards::from_env()?.get(selector).await?;
let binding_ids = service.status.and_then(|status| status.verification).map(|v| v.binding_ids);
```

</LangTab>
<LangTab lang="typescript">

```typescript
import { Cards } from "wyrd-ai";

const service = await Cards.connect().get({
  kind: "Service",
  name: "support-desk",
  version: "1.0.0",
  space: "default",
});
console.log(service.status?.verification?.binding_ids);
```

</LangTab>
</LangTabs>

Over MCP, `cards.get_by_ref` returns the same `status.verification.binding_ids`. A binding belongs to one exact Service version; a new version has new binding IDs.

## 2. Check that the binding can run

Call `verification.get_binding` (`GET /v1/verification/bindings/{binding_id}`):

```json
{ "binding_id": "<binding-id>" }
```

| Field | Tells you |
| --- | --- |
| `card_uid`, `verifier_card_uid` | Exactly what is checked, and by which Verifier version |
| `readiness` | `ready`, `baseline_not_ready` (a drift baseline is still fitting or failed), or `verifier_unavailable` |
| `active` | Whether the Service has authenticated in the last 24 hours. An inactive binding does not run on its Trigger |
| `next_run_at` | The next scheduled window. Always null for an `observations_ready` binding |
| `last_run_id` | The most recent run, for step 4 |

If `active` is false, deploy the Service; nothing on the Verifier side is wrong. If `readiness` is `baseline_not_ready`, see [Monitor drift](/build-and-verify/verify/monitor-drift/#4-wait-for-the-baseline-to-fit).

## 3. Start a run (drift only)

To score a window now instead of waiting for the schedule, call `verification.start_run` (`POST /v1/verification/runs`):

```json
{
  "target": { "kind": "binding", "binding_id": "<binding-id>" },
  "input": { "kind": "drift_window", "start": "2026-10-12T14:00:00Z", "end": "2026-10-12T15:00:00Z" }
}
```

It returns `run_id`. A binding run uses the binding's Operators, so a `failed` verdict pages whoever it would page on schedule. Manual runs are drift only; an eval binding runs once per eval observation.

## 4. Follow the run

Call `verification.get_run` (`GET /v1/verification/runs/{run_id}`) until `status` is final:

| `status` | Means | Next |
| --- | --- | --- |
| `pending`, `running`, `retrying` | Not finished. 3 attempts, 5 minutes each | Wait and read again |
| `completed` | A verdict exists | Read `result_id` and go to step 5 |
| `errored`, `timed_out`, `cancelled` | The check did not run to a verdict | Read `error.code` and `error.message`. No Operator runs |

The run never carries the verdict itself. A `completed` run of a passing check and of a failing one look the same here; the verdict is in the result.

`dispatches` lists each Operator a failed binding run started, with its own `status`: `pending`, `running`, `retrying`, `delivered`, or `failed`. A `failed` dispatch has an `error` saying why, such as an expired connection credential. Delivery failure never changes the verdict.

## 5. Read the result

Query by `result_id`. `vala.verification.results` holds the verdict; the per-feature or per-task rows explain it:

<LangTabs langs={['python', 'rust', 'typescript']}>
<LangTab lang="python">

```python
from wyrd.bifrost import Bifrost

bifrost = Bifrost()
result = bifrost.sql(
    "SELECT verdict, implementation, verifier_version, window_start, window_end "
    "FROM vala.verification.results WHERE result_id = $1",
    [result_id],
)
features = bifrost.sql(
    "SELECT feature, score, threshold, verdict FROM vala.drift.result_features "
    "WHERE result_id = $1 ORDER BY feature",
    [result_id],
)
print(result.to_pandas(), features.to_pandas())
```

</LangTab>
<LangTab lang="rust">

```rust
use wyrd_ai::{Bifrost, bifrost::QueryParam};

let bifrost = Bifrost::from_env().await?;
let id = [QueryParam::String(result_id.to_string())];
let result = bifrost
    .sql(
        "SELECT verdict, implementation, verifier_version, window_start, window_end \
         FROM vala.verification.results WHERE result_id = $1",
        &id,
    )
    .await?;
let features = bifrost
    .sql(
        "SELECT feature, score, threshold, verdict FROM vala.drift.result_features \
         WHERE result_id = $1 ORDER BY feature",
        &id,
    )
    .await?;
```

</LangTab>
<LangTab lang="typescript">

```typescript
import { Bifrost } from "wyrd-ai";

const bifrost = await Bifrost.connect();
const result = await bifrost.sql(
  `SELECT verdict, implementation, verifier_version, window_start, window_end
     FROM vala.verification.results WHERE result_id = $1`,
  [resultId],
);
const features = await bifrost.sql(
  `SELECT feature, score, threshold, verdict FROM vala.drift.result_features
    WHERE result_id = $1 ORDER BY feature`,
  [resultId],
);
```

</LangTab>
</LangTabs>

For an eval result, read `vala.eval.result_items` instead: one row per task, with `passed`, `actual`, `expected`, `message`, and `skip_reason`. Over MCP, pass the same SQL to `bifrost.query`.

A run can report `result_id` a few seconds before the row is queryable. If the query comes back empty right after the run completed, retry before assuming the result is missing.

## What can go wrong

| Code or symptom | Cause | Fix |
| --- | --- | --- |
| `WYRD_VERIFICATION_409_BASELINE_NOT_READY` on `start_run` | The drift baseline has not fitted | Check `readiness`; wait or fix the baseline |
| `WYRD_VERIFICATION_400_INVALID_WINDOW` | Empty, inverted, or longer than 31 days | Send `start` before `end`, at most 31 days apart |
| `WYRD_PERMISSION_403_DENIED_RBAC` on `start_run` | No `verifier:run` on the bound Verifier | Use a credential with `workload` or a role that includes it |
| Run `completed` but no Operator fired | The verdict was `passed` or `inconclusive` | Read the verdict in step 5 |
| Dispatch `failed` | The Operator's connection or target refused delivery | Read the dispatch `error`; fix the operator connection |
