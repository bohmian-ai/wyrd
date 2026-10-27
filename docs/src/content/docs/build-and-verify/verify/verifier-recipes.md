---
title: Recipes
description: Copyable Verifiers for common checks, each with its YAML and the Python that calls it - assertions, traces, Python checks, model judges, multi-step, uploaded files, grouped sessions, and drift.
pillar: wyrd
section: Build and verify
topic: Verify
order: 1
status: runnable
---

Pick the check you need, bind its Verifier under `verified_by` to what it checks (a Service, one of its Agents, Models, or Data, or a standalone Agent), and call it from your code. Each recipe's Python declares `support-reply.yaml`, a Service that binds the recipe's Verifier, as in [Run your first verification](/get-started/quickstart/).

| To check | Use |
| --- | --- |
| Fields of an answer | [Assertions](#assertions) |
| That your code took the right steps | [Trace](#trace) |
| Logic an operator cannot express | [Python check](#python-check) |
| Meaning, tone, or policy | [Model judge](#model-judge) |
| An answer against a document | [Multi-step](#multi-step) |
| An answer against files you upload once | [Uploaded files](#uploaded-files) |
| A whole conversation, not one reply | [Grouped session](#grouped-session) |
| Inputs or outputs shifting over time | [Drift](#drift) |

Start with the cheapest check that catches the failure. A model judge costs a model call per requirement; an assertion costs nothing.

## Assertions

Compare fields of a JSON value with an operator.

<Tabs>
<Tab label="Declare (YAML)">

```yaml title="reply-rules.yaml"
apiVersion: wyrd/v1
kind: Verifier
metadata: {name: reply-rules, version: 1.0.0}  # the name your code calls
spec:
  description: A support reply never promises a refund and stays within the refund limit.
  inputs:
    answer: {type: json}  # one named slot for the evidence; json takes any JSON value
  requirements:  # each requirement gets its own verdict
    - id: no-guarantee
      statement: The reply never promises a guaranteed refund  # shown in results
      # path selects a value from the evidence; operator compares it with expected
      check: {kind: assert, path: inputs.answer.text, operator: not_contains, expected: guaranteed refund}
    - id: within-limit
      statement: The refund offered is at most 100 dollars
      # operators that take parameters are written as {kind: ..., ...}
      check: {kind: assert, path: inputs.answer.refund, operator: {kind: in_range, min: 0, max: 100, inclusive: true}}
```

</Tab>
<Tab label="Observe (Python)">

```python
from wyrd import WyrdState

# Declare your Service and the Verifiers it binds, then start a Run.
run = WyrdState.declare("support-reply.yaml").run()

# Pass the evidence by slot name: answer fills inputs.answer. verify waits for the verdict.
result = run.observe.verify("reply-rules", answer={"text": "Your refund is approved.", "refund": 40})
print(result.verdict)  # passed
for requirement in result.requirements:  # one verdict and reason per requirement
    print(requirement["id"], requirement["outcome"], requirement["reason"])
```

</Tab>
</Tabs>

[Comparison operators](/reference/verifier-card/#comparison-operators) lists every operator.

## Trace

Check the spans your code recorded: that a step ran, in order, with the right attributes.

<Tabs>
<Tab label="Declare (YAML)">

```yaml title="looked-up-policy.yaml"
apiVersion: wyrd/v1
kind: Verifier
metadata: {name: looked-up-policy, version: 1.0.0}
spec:
  description: The reply was drafted after looking up the refunds policy.
  inputs:
    trace: {type: trace}  # a trace ID; Wyrd reads the trace's stored spans
  requirements:
    - id: looked-up-first
      statement: The first span is the policy lookup
      # spans are ordered by start time, so spans[0] is the first one
      check: {kind: trace, on: inputs.trace, span_selector: "$.spans[0].name", operator: equals, expected: lookup-policy}
    - id: read-refunds
      statement: The lookup read the refunds policy
      # attributes are the ones your code set on the span
      check: {kind: trace, on: inputs.trace, span_selector: "$.spans[0].attributes.policy_id", operator: equals, expected: refunds}
```

</Tab>
<Tab label="Observe (Python)">

```python
from opentelemetry import trace
from wyrd import WyrdState

state = WyrdState.declare("support-reply.yaml")
state.start_telemetry()  # export OpenTelemetry spans, logs, and metrics to Wyrd
tracer = trace.get_tracer("support")

# Your code's real work, traced as it runs.
with tracer.start_as_current_span("lookup-policy", attributes={"policy_id": "refunds"}) as span:
    with tracer.start_as_current_span("draft-answer"):
        ...
state.flush()  # make sure the spans are stored before they are checked

trace_id = format(span.get_span_context().trace_id, "032x")  # 32 hex characters
result = state.run().observe.verify("looked-up-policy", trace=trace_id)
print(result.verdict)  # passed
```

</Tab>
</Tabs>

## Python check

Run your own function when no operator fits. It returns a verdict with a reason and a citation.

<Tabs>
<Tab label="Declare (YAML)">

```yaml title="names-the-policy.yaml"
apiVersion: wyrd/v1
kind: Verifier
metadata: {name: names-the-policy, version: 1.0.0}
spec:
  description: A support reply points the customer to the refund policy.
  inputs:
    answer: {type: json}
  requirements:
    - id: names-the-policy
      statement: The reply names the refund policy
      check:
        kind: code
        entrypoint: check  # the function to call
        inputs: {answer: inputs.answer}  # passed to it as keyword arguments
        source:
          inline: |
            def check(answer):
                named = "policy" in answer["text"].lower()
                # Return a verdict: outcome, reason, and the evidence it cites.
                return {
                    "outcome": "passed" if named else "failed",
                    "reason": "names the policy" if named else "does not name the policy",
                    # slot names an input; json_pointer points inside it
                    "citations": [{"slot": "answer", "locator": {"json_pointer": "/text"}}],
                }
```

</Tab>
<Tab label="Observe (Python)">

```python
from wyrd import WyrdState

run = WyrdState.declare("support-reply.yaml").run()

# The function runs on the server, not in your process.
result = run.observe.verify("names-the-policy", answer={"text": "Refunds follow our policy."})
print(result.verdict)  # passed
```

</Tab>
</Tabs>

The function runs sandboxed, with no files, network, or environment.

## Model judge

Ask a model whether a claim holds. A pass must cite the evidence it relied on, or it counts as `inconclusive`.

<Tabs>
<Tab label="Declare (YAML)">

```yaml title="follows-policy.yaml"
apiVersion: wyrd/v1
kind: Verifier
metadata: {name: follows-policy, version: 1.0.0}
spec:
  description: A model judges whether a support reply follows the refund policy.
  inputs:
    answer: {type: json}
  requirements:
    - id: follows-policy
      statement: The reply follows the refund policy and never promises more than it allows
      check:
        kind: agent
        agent: ./policy-judge.yaml  # the judge, below
        inputs:  # fill the judge's prompt placeholders
          claim: requirement.statement  # this requirement's statement
          answer: inputs.answer.text
```

```yaml title="policy-judge.yaml"
apiVersion: wyrd/v1
kind: Agent
metadata: {name: policy-judge, version: 1.0.0}
spec:
  execution_route: {kind: wyrd_gateway, deployment: openai}  # a deployment in your LLM gateway
  run_config: {max_iterations: 1}  # one model call, no tools
  prompt:
    provider: openai
    model: gpt-4o
    messages:
      - role: user
        content: |
          Refunds of up to 100 dollars are approved within 30 days; larger refunds need a manager.
          Claim: {{claim}}  # filled from the check's inputs
          Reply: {{answer}}
          Decide whether the claim holds for the reply, and cite the reply.
    # The judge answers with a Verdict: an outcome, a reason, and citations.
    response_format: verdict
```

</Tab>
<Tab label="Observe (Python)">

```python
from wyrd import WyrdState

run = WyrdState.declare("support-reply.yaml").run()

result = run.observe.verify("follows-policy", answer={"text": "Refunds up to 100 dollars follow our policy."})
judged = result.requirements[0]
print(judged["outcome"], judged["reason"])  # the judge's verdict and its explanation
print(judged.get("citations"))  # where in the reply the judge looked
```

</Tab>
</Tabs>

The judge's model calls go through the LLM gateway with your provider key. [Verdicts and citations](/reference/verifier-card/#verdicts-and-citations) explains the locators a citation can use.

### Return scores and labels with the verdict

Declare a schema under `data` and the judge returns that object beside its verdict. The server checks it against the schema; output that does not match is `inconclusive` and keeps no data.

```yaml title="policy-judge.yaml (excerpt)"
    response_format:
      kind: verdict
      data:
        type: object
        properties:
          score: {type: number, minimum: 0, maximum: 1}
          category: {type: string}
        required: [score, category]
        additionalProperties: false
```

A `code` check declares the same `response_format` beside `entrypoint`, and its function returns `data` with its verdict. Each requirement carries its own `data`, which stays absent when its check declares none. Read it as JSON, or ask for your own type:

<LangTabs langs={['python', 'rust', 'typescript']}>
<LangTab lang="python">

```python
from pydantic import BaseModel

class JudgeData(BaseModel):
    score: float
    category: str

result = run.observe.verify_typed("follows-policy", JudgeData, answer={"text": "Refunds up to 100 dollars follow our policy."})
data = result.requirements[0].get("data")  # a JudgeData, or None when the check returned none
```

</LangTab>
<LangTab lang="rust">

```rust
use serde::Deserialize;
use serde_json::json;
use wyrd_ai::verify::{Input, Inputs};

#[derive(Deserialize)]
struct JudgeData {
    score: f64,
    category: String,
}

let answer = json!({ "text": "Refunds up to 100 dollars follow our policy." });
let result = run
    .observe()
    .verify_typed::<JudgeData>("follows-policy", Inputs::named([("answer", Input::from(answer))]))
    .await?;
let data = result.outcome.requirements[0].data.as_ref(); // a JudgeData, or None when the check returned none
```

</LangTab>
<LangTab lang="typescript">

```typescript
import { z } from "zod";

const JudgeData = z.object({ score: z.number(), category: z.string() });

const answer = { text: "Refunds up to 100 dollars follow our policy." };
const result = await run.observe.verifyTyped("follows-policy", { answer }, JudgeData);
const data = result.outcome.requirements[0]?.data; // a JudgeData, or undefined when the check returned none
```

</LangTab>
</LangTabs>

Data that does not fit your type raises `WYRD_SDK_422_VERIFICATION_DATA_INVALID` naming the requirement; the stored result is unchanged and nothing runs again. Its `path` is always `""`, the root of that requirement's data, because a deeper path could name a key the check returned.

## Multi-step

Prepare evidence before checking it. Here a step reads the policy PDF, a second step compares the refund with the limit it states, and a requirement checks the result.

<Tabs>
<Tab label="Declare (YAML)">

```yaml title="within-policy.yaml"
apiVersion: wyrd/v1
kind: Verifier
metadata: {name: within-policy, version: 1.0.0}
spec:
  description: The refund offered is within the limit the policy PDF states.
  inputs:
    answer: {type: json}
    policy: {type: media, formats: [pdf]}  # a file, uploaded by your code
  steps:  # run before the requirements; later steps read earlier outputs
    - id: policy-text
      action: {kind: extract, from: inputs.policy}  # PDF to text, page by page
    - id: compare
      action:
        kind: code
        entrypoint: compare
        # steps.<id>.output reads a step's result; this step waits for policy-text
        inputs: {pages: steps.policy-text.output.pages, answer: inputs.answer}
        source:
          inline: |
            def compare(pages, answer):
                text = " ".join(page["text"] for page in pages)
                # "Refunds of up to 100 dollars are approved ..."
                limit = int(text.split("up to ")[1].split(" ")[0])
                return {"within_limit": answer["refund"] <= limit}
  requirements:
    - id: within-limit
      statement: The refund offered is within the policy limit
      check: {kind: assert, path: steps.compare.output.within_limit, operator: equals, expected: true}  # reads the step's output
```

</Tab>
<Tab label="Observe (Python)">

```python
from wyrd import WyrdState, verify

run = WyrdState.declare("support-reply.yaml").run()

# One keyword per input slot.
result = run.observe.verify(
    "within-policy",
    answer={"text": "Your refund of 40 dollars is approved.", "refund": 40},
    policy=verify.File("refund-policy.pdf"),  # your policy document, with "up to N dollars"
)
print(result.verdict)  # passed
for step in result.steps:  # each step's status, for debugging
    print(step["id"], step["status"])
```

</Tab>
</Tabs>

Steps can also be `agent` actions, which read files with built-in tools. [Actions](/reference/verifier-card/#actions) lists each kind.

## Uploaded files

Upload a file or directory once as an Artifact Card, then mount the exact files a judge needs into its workspace.

```bash
wyrd artifact upload ./handbook --name staff-handbook --version 1.0.0 --extract
```

The command prints the exact Active reference, UID included. Each file keeps its path relative to `./handbook`, byte for byte. Nothing is parsed unless you pass `--extract`; then each supported document `P` is also published as `extracted/P.json` before the Card is declared, and `spec.metadata.extraction` records the report.

The version follows the ordinary rules: uploading the same bytes to a pinned version returns the same Card, changed bytes under it are refused with `WYRD_REGISTRY_409_SPEC_DRIFT`, and `--bump patch` without `--version` publishes the next version. List what a version holds from the server:

```bash
wyrd artifact files --kind Artifact --name staff-handbook --version 1.0.0
```

The SDKs run the same upload:

| Language | Upload | Inventory |
| --- | --- | --- |
| Python | `Cards().upload_artifact(source, {"name": ..., "version": ...}, version_bump=None, spec=None, extract=False)` | `Cards().list_artifacts(card_ref)` |
| Rust | `Cards::upload_artifact(source, metadata, spec, extract)` | `Cards::list_artifacts(selector)` |
| TypeScript | `cards.uploadArtifact(source, { metadata, spec, extract })` | `cards.listArtifacts(ref)` |

`metadata` takes `labels` and `annotations` too, and `spec` defaults to `{artifact_kind: file}`.

Mount the uploaded files by exact reference in an Agent's `workspace`, here the extraction of one handbook:

<Tabs>
<Tab label="handbook-review.yaml">

<CodeFromFile file="fixtures/cards/uploaded_artifacts/handbook-review.yaml" lang="yaml" title="handbook-review.yaml" />

</Tab>
<Tab label="handbook-judge.yaml">

<CodeFromFile file="fixtures/cards/uploaded_artifacts/handbook-judge.yaml" lang="yaml" title="handbook-judge.yaml" />

</Tab>
</Tabs>

Without `path`, every file of the version is mounted. The judge reads with `fs.read` and cites the `attachment_<n>` slot the tool reports. Uploading a new version never changes what this Verifier mounts. [Agent tools and authorization](/reference/agent-tools-and-authorization/) lists the file tools and the grants each call needs.

## Grouped session

Judge a whole conversation after it ends. Each reply is verified on its own; a session Verifier reads every reply's evidence and outcome once the group goes quiet.

<Tabs>
<Tab label="Declare (YAML)">

```yaml title="whole-conversation.yaml"
apiVersion: wyrd/v1
kind: Verifier
metadata: {name: whole-conversation, version: 1.0.0}
spec:
  description: Every reply of a conversation passed reply-rules.
  inputs:
    # Every reply-rules answer in the group, in order.
    replies: verifiers.reply-rules.inputs.answer
    # Every no-guarantee outcome in the group: passed, failed, or inconclusive.
    outcomes: verifiers.reply-rules.requirements.no-guarantee.outcome
  # List order: recorded_at (the default) is the order Wyrd received each reply;
  # inputs.replies.<field> orders by a field of each reply instead.
  order_by: recorded_at
  requirements:
    - id: every-reply-passed
      statement: No reply in the conversation promised a guaranteed refund
      # each checks every item; one failed reply fails the conversation
      check: {kind: assert, each: "inputs.outcomes[*]", operator: equals, expected: passed}
```

Bind it beside `reply-rules` with a `group_end` Trigger:

```yaml
verified_by:
  - verifier: ./reply-rules.yaml
  - verifier: ./whole-conversation.yaml
    runs_on: {kind: group_end, after: 10m}  # judge 10 minutes after the last reply
```


</Tab>
<Tab label="Observe (Python)">

```python
from wyrd import WyrdState

run = WyrdState.declare("support-reply.yaml").run()
conversation = "conv-1234"  # one ID per conversation
replies = [
    {"text": "Let me check your order.", "refund": 0},
    {"text": "Your refund of 40 dollars is approved.", "refund": 40},
]

for reply in replies:
    # Each reply is judged now; group_id also adds it to the conversation.
    run.observe.verify("reply-rules", answer=reply, group_id=conversation)
# whole-conversation runs on its own, 10 minutes after the last reply.
```

</Tab>
</Tabs>

New replies in the group push the session check back. Its verdict is stored in `vala.verification.results` with the `group_id`.

## Drift

Compare recent values with a baseline on a schedule. Your code records samples; the schedule judges the window.

<Tabs>
<Tab label="Declare (YAML)">

```yaml title="latency-drift.yaml"
apiVersion: wyrd/v1
kind: Verifier
metadata: {name: latency-drift, version: 1.0.0}
spec:
  description: Request latency stays distributed like the healthy baseline.
  inputs:
    latency: {type: samples, features: [latency]}  # values your code records over time
  requirements:
    - id: latency-stable
      statement: Request latency is distributed like the healthy baseline
      check:
        kind: drift
        on: inputs.latency
        method: Psi  # Population Stability Index: how far a distribution moved
        signal:
          kind: Distribution
          baseline_ref: {kind: Data, name: latency-baseline, version: 1.0.0}  # what healthy looks like
          features: [latency]
        condition: {kind: Statistical}
        profile:
          kind: Psi
          binning_strategy: {kind: EqualWidth, n_bins: 10}  # compare across 10 equal-width buckets
          threshold: {kind: Fixed, value: 0.25}  # fail when PSI is above 0.25
```

Bind it with a schedule:

```yaml
verified_by:
  - verifier: ./latency-drift.yaml
    runs_on: {kind: schedule, cron: "0 0 * * *"}  # judge the day's samples at midnight
```


</Tab>
<Tab label="Observe (Python)">

```python
import pandas as pd
from wyrd import WyrdState
from wyrd.cards import Cards
from wyrd.data import DataCard, PandasInterface

# Declare the baseline once: latencies from healthy traffic, in milliseconds.
healthy = pd.DataFrame({"latency": [float(ms) for ms in range(100)]})
Cards().data.declare(DataCard(PandasInterface(data=healthy), name="latency-baseline", version="1.0.0"))

run = WyrdState.declare("support-reply.yaml").run()
run.observe.submit("latency-drift", latency={"latency": 12.5})  # one sample per request
```

</Tab>
</Tabs>

A drift Verifier refuses `verify`; only the schedule judges it.

## Next

[How verification works](/understand/how-verification-works/) explains steps, verdicts, and where each check runs.
