---
title: Verify a request in real time
description: Call a bound Verifier from your service while a request is in flight, act on the verdict, and read the requirement breakdown.
pillar: wyrd
section: Build and verify
topic: Verify
order: 1
draft: true
status: runnable
---

Call `observe.verify` inside a run, get the result back in the same request, and decide what your service does next. Here the support desk checks every answer with two model-free Verifiers before it replies, and replaces an answer that promises a refund with a handoff to billing.

## Prerequisites

- A running service that loads its bundle, as in [Record a run and observations](/build-and-verify/observe/record-a-run-and-observations/).
- Verifiers bound to the Card you call them on. The support desk binds `no-refund-promise` to the Service and `ticket-complete` to its `agent` component:

<Tabs>
<Tab label="support-desk.yaml">

<CodeFromFile file="examples/support-desk-app/service/support-desk.yaml" lang="yaml" title="service/support-desk.yaml" />

</Tab>
<Tab label="no-refund-promise.yaml">

<CodeFromFile file="examples/support-desk-app/service/no-refund-promise.yaml" lang="yaml" title="service/no-refund-promise.yaml" />

</Tab>
<Tab label="ticket-complete.yaml">

<CodeFromFile file="examples/support-desk-app/service/ticket-complete.yaml" lang="yaml" title="service/ticket-complete.yaml" />

</Tab>
<Tab label="support-agent.yaml">

<CodeFromFile file="examples/support-desk-app/service/support-agent.yaml" lang="yaml" title="service/support-agent.yaml" />

</Tab>
</Tabs>

- `verifier:run` on those Verifiers. The `workload` role a registered Service gets includes it.

## 1. Call the Verifiers

Pass one keyword per input slot the Verifier declares. JSON is ordinary data; a file is a `verify.File`, covered [below](#files).

<LangTabs langs={['python', 'rust', 'typescript']}>
<LangTab lang="python">

<CodeFromFile file="examples/support-desk-app/python/desk.py" lang="python" region="check" title="desk.py" />

</LangTab>
<LangTab lang="rust">

<CodeFromFile file="examples/support-desk-app/rust/lib.rs" lang="rust" region="check" title="lib.rs" />

</LangTab>
<LangTab lang="typescript">

<CodeFromFile file="examples/support-desk-app/typescript/server.ts" lang="typescript" region="check" title="server.ts" />

</LangTab>
</LangTabs>

The name is the Verifier's Card name. The SDK looks it up among the Verifiers bound to the view's Card in your loaded bundle: `no-refund-promise` on the run itself, which is the Service, and `ticket-complete` on `run.for_card("agent")`. A Verifier with exactly one slot also takes one positional value instead of a keyword.

Python blocks until the result arrives; Rust and TypeScript await it. The server allows 60 seconds.

## 2. Act on the verdict

A `failed` verdict is an ordinary return value, so read it rather than catching an exception:

<LangTabs langs={['python', 'rust', 'typescript']}>
<LangTab lang="python">

```python
results = desk.check(run, ticket)
if results["no-refund-promise"].verdict != "passed":
    answer = HANDOFF  # "I've passed your question to our billing team..."
```

</LangTab>
<LangTab lang="rust">

```rust
use wyrd_ai::VerificationVerdict;

if promise.outcome.verdict != VerificationVerdict::Passed {
    answer = HANDOFF.to_owned();
}
```

</LangTab>
<LangTab lang="typescript">

```typescript
const promised = promise.outcome.verdict !== "passed";
return { run_id: wyrdRun.runId, answer: promised ? HANDOFF : text, checks };
```

</LangTab>
</LangTabs>

`inconclusive` means no judgment was possible, such as a missing input. Treat it as not passed.

### Files

`File` reads nothing when you create it. During the call the SDK hashes it, asks the server which digests it is missing, uploads only those directly to your tenant's object storage, and sends the digest in the request. The same file uploads once and is reused by every later call. Format comes from the file extension, from the slot when it accepts one format, or from what you declare:

| Language | From a path | From bytes |
| --- | --- | --- |
| Python | `wyrd.verify.File("letter.md")` | `wyrd.verify.File(data, format="pdf", filename="policy.pdf")` |
| Rust | `wyrd_ai::verify::File::path("letter.md")` | `wyrd_ai::verify::File::bytes(data).with_format(MediaFormat::Pdf)` |
| TypeScript | `verify.File.path("letter.md")` | `verify.File.bytes(data, { format: "pdf" })` |

To use a file already registered as an Artifact, pass an `ArtifactRef` instead: `wyrd.verify.ArtifactRef(card_ref, path="refund-policy.pdf")`, `verify::ArtifactRef::new(card_ref, Some(path))`, or `new verify.ArtifactRef(cardRef, "refund-policy.pdf")`. It is read with the Artifact's own authorization.

## 3. Read the breakdown

| Field | Holds |
| --- | --- |
| `execution_id` | The server-issued ID of this execution |
| `run_id` | Your run |
| `verifier`, `subject` | The exact Verifier and Card versions |
| `outcome.verdict` | `passed`, `failed`, or `inconclusive`. Python also exposes it as `result.verdict` |
| `outcome.requirements` | One entry per requirement: `id`, `step`, `statement`, `outcome`, `reason`, `actual`, `citations`, `detail`, and the check's provenance |
| `outcome.steps` | One entry per step: `id`, `status`, `output`, `attempts`, `duration_ms`, `reason`, `tool_calls` |
| `outcome.evidence` | The pinned input declarations, by slot, with file digests |

<LangTabs langs={['python', 'rust', 'typescript']}>
<LangTab lang="python">

```python
for requirement in result.requirements:
    print(requirement["id"], requirement["outcome"], requirement["reason"])
# ticket-complete failed missing order_id
```

</LangTab>
<LangTab lang="rust">

```rust
for requirement in &result.outcome.requirements {
    println!("{} {:?} {}", requirement.id, requirement.outcome, requirement.reason);
}
```

</LangTab>
<LangTab lang="typescript">

```typescript
for (const requirement of result.outcome.requirements) {
  console.log(requirement.id, requirement.outcome, requirement.reason);
}
```

</LangTab>
</LangTabs>

Each citation names an original input slot and a location in it, such as `{ "slot": "ticket", "locator": { "json_pointer": "" } }` from `ticket-complete`, or `{ "slot": "policy", "locator": { "page": 2 } }` from a judge reading a PDF. A judge that passes without a valid citation is reported as `inconclusive`.

## 4. Confirm the result was stored

After it answers, the server stores the pinned inputs and the result in `vala.verification.results`, tied to your run, with `binding_id` null. A `verify` call never starts queued work and never runs Operators.

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

The row appears shortly after the call returns. The server stores it in the background, after it has already answered you, so the result your code received stands even in the rare case the row is lost, such as an abrupt server restart. An empty query seconds after a call is not proof the row is missing; retry before concluding that.

## Checks that do not belong on the request path

A Verifier with `samples` slots and `drift` checks judges a scheduled window, so `verify` refuses it; record its samples with `submit` as in [Monitor drift](/build-and-verify/verify/monitor-drift/). A slow review, such as the support desk's `follows-policy`, which reads a PDF and calls a model twice, belongs in `submit` instead of holding the request; see [Submit for later](/build-and-verify/observe/record-a-run-and-observations/#submit-for-later).

## What can go wrong

| Error code | Cause | Fix |
| --- | --- | --- |
| `WYRD_SDK_404_UNKNOWN_VERIFIER` | No Verifier with that name is bound to this component in the loaded bundle | Check the name and the component alias; reload the bundle after changing `verified_by` |
| `WYRD_VERIFICATION_400_INPUT_INVALID` | An unknown slot name, a value of the wrong kind for its slot, an invalid trace ID, an unreadable file, or a format the slot does not accept | Match the Verifier's `inputs`. Nothing ran |
| `WYRD_VERIFICATION_413_INPUT_TOO_LARGE` | A file is larger than its slot's `max_bytes` or the deployment limit | Send a smaller file, or raise `max_bytes` up to the deployment limit |
| `WYRD_VERIFICATION_422_INPUT_UNSUPPORTED` | The Verifier judges a scheduled window | Use `submit` |
| `WYRD_PERMISSION_403_DENIED_RBAC` | The credential lacks `verifier:run` on this Verifier, or read access to an Artifact or trace it names | Grant a role that includes it |
| `WYRD_VERIFICATION_504_EXECUTION_TIMED_OUT` | The execution took longer than 60 seconds, usually a slow Agent step | Decide your fallback in code, or use `submit`. The SDK does not retry `verify` |

A missing required input is not an error: the requirements that need it come back `inconclusive`. For when to check in real time versus in the background, see [How verification works](/understand/how-verification-works/).
