---
title: Verify a request in real time
description: Call a bound Verifier from your service while a request is in flight, with JSON and file evidence, and act on the requirement breakdown.
pillar: wyrd
section: Build and verify
topic: Verify
order: 1
draft: true
status: runnable
---

Call `observe.verify` inside a run, get the result back in the same request, and decide what your service does next. Here the support desk checks every answer against its refund-policy PDF before it goes out, and hands the ticket to a person when the answer fails.

## Prerequisites

- A running service that loads its bundle, as in [Record a run and observations](/build-and-verify/observe/record-a-run-and-observations/).
- The `policy-check` Verifier from [How verification works](/understand/how-verification-works/#four-parts-of-a-verifier), bound to the component you call it on:

  ```yaml title="service/support-desk.yaml (excerpt)"
  - alias: agent
    ref: ./support-agent.yaml
    verified_by:
      - verifier: ./policy-check.yaml
  ```

- `verifier:run` on that Verifier. The `workload` role a registered Service gets includes it. The Verifier's Agent steps also need their own permissions; see [Agent tools and authorization](/reference/agent-tools-and-authorization/).

## 1. Call the Verifier and act on the verdict

Pass one named value per input slot. JSON is ordinary data; a file is a `File` from a path or bytes.

<LangTabs langs={['python', 'rust', 'typescript']}>
<LangTab lang="python">

```python
import wyrd

with state.run("agent") as run:
    answer = run.invoke({"question": question})
    result = run.observe.verify(
        "policy-check",
        answer={"text": answer},
        policy=wyrd.File("refund-policy.pdf"),
    )
    if result.outcome.verdict != "passed":
        answer = hand_to_person(ticket_id)  # your fallback
```

</LangTab>
<LangTab lang="rust">

```rust
use wyrd_ai::observe::{File, Input, Inputs};
use wyrd_ai::VerificationVerdict;

let run = state.run_for_card("agent")?;
let mut answer = run.invoke(&[("question", question)]).await?;
let result = run
    .observe()
    .verify(
        "policy-check",
        Inputs::named([
            ("answer", Input::from(json!({ "text": answer }))),
            ("policy", File::path("refund-policy.pdf").into()),
        ]),
    )
    .await?;
if result.outcome.verdict != VerificationVerdict::Passed {
    answer = hand_to_person(ticket_id); // your fallback
}
```

</LangTab>
<LangTab lang="typescript">

```typescript
import { File } from "wyrd-ai";

const run = state.run("agent");
let answer = await run.invoke({ question });
const result = await run.observe.verify("policy-check", {
  answer: { text: answer },
  policy: File.path("refund-policy.pdf"),
});
if (result.outcome.verdict !== "passed") {
  answer = await handToPerson(ticketId); // your fallback
}
```

</LangTab>
</LangTabs>

The name is the Verifier's Card name. The SDK looks it up among the Verifiers bound to `agent` in your loaded bundle, so a Verifier bound only to another component is not callable from this view. A Verifier with exactly one slot also takes one positional value instead of names.

Python blocks until the result arrives; Rust and TypeScript await it. The server allows 60 seconds. A `failed` verdict is an ordinary return value, so read the verdict rather than catching an exception.

### Files

`File` reads nothing when you create it. During the call the SDK hashes it, asks the server which digests it is missing, uploads only those directly to your tenant's object storage, and sends the digest in the request. The same policy file uploads once and is reused by every later call. Format comes from the file extension, from the slot when it accepts one format, or from what you declare:

| Language | From a path | From bytes |
| --- | --- | --- |
| Python | `wyrd.File("refund-policy.pdf")` | `wyrd.File(data, format="pdf", filename="policy.pdf")` |
| Rust | `File::path("refund-policy.pdf")` | `File::bytes(data).with_format(MediaFormat::Pdf)` |
| TypeScript | `File.path("refund-policy.pdf")` | `File.bytes(data, { format: "pdf" })` |

To use a file already registered as an Artifact, pass an `ArtifactRef` instead: `wyrd.ArtifactRef(card_ref, path="refund-policy.pdf")`, `ArtifactRef::new(card_ref, Some(path))`, or `new ArtifactRef(cardRef, { path })`. It is read with the Artifact's own authorization.

## 2. Read the breakdown

| Field | Holds |
| --- | --- |
| `execution_id` | The server-issued ID of this execution |
| `run_id` | Your run |
| `verifier`, `subject` | The exact Verifier and Card versions |
| `outcome.verdict` | `passed`, `failed`, or `inconclusive` |
| `outcome.requirements` | One entry per requirement: `id`, `step`, `statement`, `outcome`, `reason`, `actual`, `citations`, `detail`, and the check's provenance |
| `outcome.steps` | One entry per step: `id`, `status`, `output`, `attempts`, `duration_ms`, `reason`, `tool_calls` |
| `outcome.evidence` | The pinned input declarations, by slot, with file digests |

<LangTabs langs={['python', 'rust', 'typescript']}>
<LangTab lang="python">

```python
for requirement in result.outcome.requirements:
    print(requirement.id, requirement.outcome, requirement.reason)
# no-guaranteed-refund  failed  "found 'guaranteed refund'"
# follows-policy        failed  "policy allows refunds within 30 days only"  policy p.2
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

Each citation names an original input slot and a location in it, such as `{ "slot": "policy", "locator": { "page": 2 } }`. A judge that passes without a valid citation is reported as `inconclusive`.

## 3. Confirm the result was stored

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

A Verifier with `samples` slots and `drift` checks judges a scheduled window, so `verify` refuses it; record its samples with `submit` as in [Monitor drift](/build-and-verify/verify/monitor-drift/). A slow review, such as an Agent reading a large document, can be queued with `submit` instead of holding the request; see [Run a verification and inspect its evidence](/build-and-verify/verify/run-a-verification-and-inspect-its-evidence/).

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
