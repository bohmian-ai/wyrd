---
title: Evaluate agent behavior
description: Check what an agent did, not only what it said, with trace checks, Agent judges, Agent preparation steps, and Python code, and queue the review in the background.
pillar: wyrd
section: Build and verify
topic: Verify
order: 2
draft: true
status: runnable
---

An answer can read well and still come from the wrong process: the agent skipped the policy lookup, or quoted a clause it never read. This guide builds a Verifier that checks the support agent's trace, has an Agent read the policy and cite it, and has a tool-less judge decide whether the answer follows those findings. Because the review takes longer than a request should wait, it is queued with `submit`.

## Prerequisites

- The `support-desk` declaration from [Cards, versions, and relationships](/understand/cards-versions-and-relationships/), and `editor` to register it.
- A running service with tracing on, as in [Record a run and observations](/build-and-verify/observe/record-a-run-and-observations/).
- An AI Gateway deployment the server can call. Server-hosted Agents run through `execution_route: { kind: wyrd_gateway, ... }`.
- The permissions in [Agent tools and authorization](/reference/agent-tools-and-authorization/) for the tools your Agents use.

## 1. Write the Agents

A preparation Agent reads the policy with a built-in tool and returns findings with citations. Its Prompt's `response_format` is the shape downstream checks read.

```yaml title="service/policy-reader.yaml"
apiVersion: wyrd/v1
kind: Agent
metadata: { space: support, name: policy-reader, version: 1.0.0 }
spec:
  execution_route: { kind: wyrd_gateway, deployment: anthropic-primary }
  tool_names: [fs.read, fs.search]
  run_config: { max_iterations: 8, timeout_ms: 120000 }
  prompt:
    provider: anthropic
    model: claude-sonnet-5-5
    system: The refund policy is at /inputs/policy.pdf. Read it with fs.read before answering.
    messages:
      - role: user
        content: "List every policy rule that applies to this answer, with page citations: {{answer}}"
    settings: { max_output_tokens: 2048 }
    response_format:
      type: object
      properties:
        rules:
          type: array
          items:
            type: object
            properties:
              rule: { type: string }
              citations: { type: array, items: { type: object } }
            required: [rule, citations]
      required: [rules]
      additionalProperties: false
```

A judge in a check position has no tools and no workspace, and must return the Verdict schema:

```yaml title="service/policy-judge.yaml"
apiVersion: wyrd/v1
kind: Agent
metadata: { space: support, name: policy-judge, version: 1.0.0 }
spec:
  execution_route: { kind: wyrd_gateway, deployment: anthropic-primary }
  run_config: { max_iterations: 1, timeout_ms: 30000 }
  prompt:
    provider: anthropic
    model: claude-sonnet-5-5
    system: Judge one claim against the evidence. Cite only the citations you were given.
    messages:
      - role: user
        content: |
          Claim: {{claim}}
          Answer: {{answer}}
          Policy findings: {{findings}}
    settings: { max_output_tokens: 1024 }
    response_format:
      type: object
      properties:
        outcome: { type: string, enum: [passed, failed, inconclusive] }
        reason: { type: string }
        citations:
          type: array
          items:
            type: object
            properties:
              slot: { type: string }
              locator: { type: object }   # { page } | { slide } | { lines: { start, end } } | { json_pointer } | { region }
            required: [slot, locator]
      required: [outcome, reason, citations]
      additionalProperties: false
```

## 2. Write the Verifier

```yaml title="service/answer-behavior.yaml"
apiVersion: wyrd/v1
kind: Verifier
metadata: { space: support, name: answer-behavior, version: 1.0.0 }
spec:
  inputs:
    answer: { type: json }
    policy: { type: media, formats: [pdf] }
    trace:  { type: trace }
  steps:
    - id: findings
      action:
        kind: agent
        agent: ./policy-reader.yaml
        inputs: { answer: inputs.answer }
        workspace: { /inputs/policy.pdf: inputs.policy }
      retries: 1
      timeout_seconds: 120
  requirements:
    - id: looked-up-policy
      statement: The agent called the policy lookup tool
      check:
        kind: trace
        on: inputs.trace
        span_selector: "$.spans[?(@.name == 'tool.policy_lookup')]"
        operator: is_non_empty
    - id: cites-a-rule
      statement: At least one policy rule applies
      check:
        kind: code
        runtime: python
        entrypoint: judge
        inputs: { findings: steps.findings.output }
        source:
          inline: |
            def judge(findings):
                rules = findings["rules"]
                if not rules:
                    return {"outcome": "failed", "reason": "no rule applies", "citations": []}
                return {"outcome": "passed", "reason": rules[0]["rule"], "citations": rules[0]["citations"]}
    - id: follows-policy
      statement: The answer is consistent with every applicable policy rule
      check:
        kind: agent
        agent: ./policy-judge.yaml
        inputs:
          claim: requirement.statement
          answer: inputs.answer
          findings: steps.findings.output
```

How it runs:

1. `findings` copies the PDF into a fresh workspace at `/inputs/policy.pdf`. The Agent reads it with `fs.read`, which returns text with page locators, and returns its rules. Every tool name, argument, output digest, and byte count is recorded on the step.
2. `looked-up-policy` reads the stored trace. It needs no step, so it runs while `findings` is still working.
3. `cites-a-rule` and `follows-policy` both read `steps.findings.output`, so they wait for it. The code check runs in an isolated Python worker with no network, credentials, or host files. The judge reuses the reader's citations; the server checks each cited page exists in the pinned PDF.
4. `decide: all` (the default) combines the three verdicts.

If `findings` fails after its retry, both requirements that need its output are `inconclusive`, not failed: the evidence is missing, not the subject at fault. A judge or code check that returns an invalid Verdict, or a `passed` without a valid citation, is `inconclusive`.

Built-in tools are resolved by name when the Agent runs; you never register a callable. [Agent tools and authorization](/reference/agent-tools-and-authorization/) lists them and the permissions each call needs. Code limits are in [Verification runtime limits](/reference/verification-runtime-limits/).

## 3. Bind it and register

```yaml title="service/support-desk.yaml (excerpt)"
- alias: agent
  ref: ./support-agent.yaml
  verified_by:
    - verifier: ./answer-behavior.yaml
      on_failure:
        - ./page-support-oncall.yaml
```

```bash
wyrd apply examples/support-desk/service/
```

The binding changes the Service's content, so bump `metadata.version` in `support-desk.yaml` first, or `wyrd apply` fails with `WYRD_REGISTRY_409_SPEC_DRIFT`. Then redeploy with a bundle of that version; a running process keeps the bindings it loaded.

## 4. Queue the review

<LangTabs langs={['python', 'rust', 'typescript']}>
<LangTab lang="python">

```python
import wyrd

with state.run("agent") as run:
    answer = run.invoke({"question": question})
    execution = run.observe.submit(
        "answer-behavior",
        answer={"text": answer},
        policy=wyrd.verify.File("refund-policy.pdf"),
        trace=run.trace_id,
    )
    save_review(ticket_id, execution.id)
```

</LangTab>
<LangTab lang="rust">

```rust
use wyrd_ai::observe::{File, Input, Inputs, Submission};

let run = state.run_for_card("agent")?;
let answer = run.invoke(&[("question", question)]).await?;
let submission = run
    .observe()
    .submit(
        "answer-behavior",
        Inputs::named([
            ("answer", Input::from(json!({ "text": answer }))),
            ("policy", File::path("refund-policy.pdf").into()),
            ("trace", Input::from(json!(trace_id.to_string()))),
        ]),
    )
    .await?;
if let Submission::Queued(execution) = submission {
    save_review(ticket_id, execution.id());
}
```

</LangTab>
<LangTab lang="typescript">

```typescript
import { File } from "wyrd-ai";

const run = state.run("agent");
const answer = await run.invoke({ question });
const execution = await run.observe.submit("answer-behavior", {
  answer: { text: answer },
  policy: File.path("refund-policy.pdf"),
  trace: traceId,
});
await saveReview(ticketId, execution.id);
```

</LangTab>
</LangTabs>

`submit` returns as soon as the evidence is staged. Read the result later, from this process or another, as in [Run a verification and inspect its evidence](/build-and-verify/verify/run-a-verification-and-inspect-its-evidence/). A `failed` verdict pages `page-support-oncall`; `inconclusive` and execution errors do not.

## What can go wrong

| Symptom or code | Cause | Fix |
| --- | --- | --- |
| Registration refuses `policy-judge` in a check | A check-position Agent has `tool_names` or a workspace | Remove them; pass what it needs through `inputs` |
| Registration refuses an Agent call's `inputs` | Its keys differ from the Prompt's placeholders | Make them match exactly, in both directions |
| Registration refuses a reference | An unknown step or slot, a cycle, or a check reading another requirement's verdict | Fix the reference; only actions and `when` read verdicts |
| `follows-policy` is `inconclusive` with a citation reason | The judge passed without a valid citation into the pinned evidence | Give the judge the reader's citations and tell it to cite them |
| `findings` failed with a tool denial | The verification's principal lacks `tool:invoke` for that tool | Grant it; see [Agent tools and authorization](/reference/agent-tools-and-authorization/) |
| Every requirement `inconclusive` | The trace was not yet stored, or `findings` failed | Read the step `reason`; submit after the trace is flushed |

To check an answer before it goes out instead of after, call a Verifier in [real time](/build-and-verify/verify/verify-a-request-in-real-time/).
