---
title: How verification works
description: Inputs, steps, requirements, and decide; verify versus submit; executions, verdicts, results, and Operators.
pillar: wyrd
section: Understand
order: 2
---

A Verifier checks declared requirements against evidence and returns a verdict: `passed`, `failed`, or `inconclusive`. Every Verifier is built from the same four parts, whether it checks one request, reviews a PDF, inspects an agent trace, or watches a feature distribution drift:

```text
inputs → optional preparation steps → requirements → verdict
```

Your code runs a Verifier now with `verify` and acts on the result, or hands it to the server's queue with `submit`. Scheduled drift runs on its own. Every path uses the same engine and writes the same result, tied to the exact Card versions that ran.

<WyrdSequence name="verification-paths" />

The examples use the `support-desk` service from the [quickstart](/get-started/quickstart/) and its three Verifiers.

## Four parts of a Verifier

A Verifier is a Card with a normal `wyrd/v1` envelope. It never names what it checks; a binding supplies the subject, so one Verifier can check any number of components.

The support desk has three, and between them they use every common building block. Switch tabs to follow each reference: `follows-policy` names `./policy-judge.yaml`, and that Agent carries its prompt, including the refund policy, inline.

<Tabs>
<Tab label="follows-policy.yaml">

<CodeFromFile file="examples/support-desk-app/service/follows-policy.yaml" lang="yaml" title="service/follows-policy.yaml" />

</Tab>
<Tab label="policy-judge.yaml">

<CodeFromFile file="examples/support-desk-app/service/policy-judge.yaml" lang="yaml" title="service/policy-judge.yaml" />

</Tab>
<Tab label="no-refund-promise.yaml">

<CodeFromFile file="examples/support-desk-app/service/no-refund-promise.yaml" lang="yaml" title="service/no-refund-promise.yaml" />

</Tab>
<Tab label="ticket-complete.yaml">

<CodeFromFile file="examples/support-desk-app/service/ticket-complete.yaml" lang="yaml" title="service/ticket-complete.yaml" />

</Tab>
</Tabs>

| Verifier | Shows |
| --- | --- |
| `no-refund-promise` | The smallest Verifier: one `json` input and one `assert` check |
| `ticket-complete` | A `code` check: inline Python that returns a verdict |
| `follows-policy` | A `media` input, an `extract` step, and an `agent` check that judges the step's output |

In `follows-policy`, the caller uploads one file: the refund decision letter the agent wrote. The `read-letter` step extracts its text. The check then hands `policy-judge` two values through its prompt placeholders: the requirement's own statement, and the extracted letter as `steps.read-letter.output`. The policy itself is in the judge's prompt, so it is fixed by the judge's version instead of being sent with every request. The judge has no tools, and its `response_format` is the closed Verdict schema, so it can only answer with an outcome, a reason, and citations to lines of the letter.

| Part | Holds |
| --- | --- |
| `inputs` | Named evidence slots. Each has a `type` and is required unless `required: false`; `samples` slots are always required |
| `steps` | Optional preparation. Each step runs one action and produces data |
| `requirements` | Each has a unique `id`, a `statement`, and one `check`, and produces one verdict. Attach them to a step or to the whole Verifier |
| `decide` | How requirement verdicts combine: `all` (the default) or `{ pass_rate: { threshold } }` |

A Verifier may have no steps. With no requirements, every result is `inconclusive`, never a pass. Every step and requirement ID is unique across the whole Verifier; registration refuses duplicates.

### Input slots

| `type` | Evidence | Caller supplies |
| --- | --- | --- |
| `json` | JSON, optionally constrained by `schema` (JSON Schema) | An ordinary value |
| `media` | A pinned file in one of `formats`, with an optional lower `max_bytes` | A `File` (local path or bytes) or an `ArtifactRef` to a registered Artifact |
| `trace` | A trace, read through tenant-scoped trace reads | A 32-hex-digit trace ID |
| `samples` | Feature measurements or metric samples for a scheduled window | A flat object of the declared `features`, or one value when the slot declares one |

Unknown slot names, wrong value types, invalid trace IDs, and values outside a declared schema or size are refused before anything runs. A missing required slot is not refused: it is missing evidence, and every check that needs it is `inconclusive`. An omitted optional slot is `null`.

Files are evidence, not Cards. Constructing a `File` reads nothing. During `verify` or `submit`, the SDK hashes the file, asks the server which digests it lacks, and uploads only those, straight to tenant object storage. Files are pinned by SHA-256 and never pass through the verification request. The server checks length, digest, and format when it reads them; a mismatch cannot produce a pass. An `ArtifactRef` selects a file of an existing registered Artifact, with that Artifact's normal authorization.

### Steps and actions

| Action `kind` | Does |
| --- | --- |
| `extract` | Reads a `media` slot (`from: inputs.<slot>`) and returns a fixed JSON document per format. See [Extraction output](/reference/extraction-output/) |
| `agent` | Invokes an inline, file-authored, or registered Agent. Output follows its Prompt's response schema, or is text |
| `code` | Runs immutable Python (`runtime: python`) from `source.inline` or an exact Artifact, calling `entrypoint` with explicitly bound `inputs`. Returns JSON |

Actions read and compute. They never write to external systems, register Cards, or dispatch Operators. A step can set `timeout_seconds` and `retries`; every attempt stays visible, and retries never hide the final failure.

An Agent step has two input channels. `inputs` values are injected into the model request through the Prompt's placeholders, and the keys must match those placeholders exactly. `workspace` maps virtual `/inputs/...` paths to media, JSON, or step outputs; the Agent reads them with the built-in tools named on its Agent Card. `/inputs` is read-only, `/work` is empty scratch for that call, and both are removed when the call ends. [Agent tools and authorization](/reference/agent-tools-and-authorization/) lists the tools.

### Requirements and checks

| Check `kind` | Judges |
| --- | --- |
| `assert` | A typed comparison over one value (`path`) or every match of a `[*]` selector (`each`), against `expected` |
| `trace` | The spans in a `trace` slot that a JSONPath `span_selector` matches, compared with `operator` and `expected` (`on` names the slot) |
| `agent` | A tool-less Agent returning the Verdict schema: `outcome`, `reason`, `citations` |
| `code` | Sandboxed Python returning the same Verdict schema |
| `drift` | PSI, SPC, or Custom over a `samples` slot (`on`), with full statistics |

Prefer `assert` whenever the check is deterministic. It is free and gives the same answer every time. Use an `agent` check for what an assertion cannot express, and pass the requirement text explicitly with `claim: requirement.statement`. A check-position Agent has no workspace and may declare only the `bifrost.query` tool, for example to read the spans of trace IDs it was given; registration refuses any other tool. That query reads live data under the ordinary tool and query permissions, so its results are not part of the pinned evidence.

A citation names an original input `slot` and a locator: `page`, `slide`, `lines`, `json_pointer`, or `region`. A step can return citations in its ordinary output, and a downstream judge can reuse them through its input bindings. The server checks that every cited location exists in the pinned evidence. An `agent` check that says `passed` without a valid citation becomes `inconclusive`.

### References and order

| Reference | Means |
| --- | --- |
| `inputs.<slot>…` | What the caller sent for that slot |
| `steps.<id>.output…` | What that step returned |
| `steps.<id>.status` | `succeeded`, `failed`, or `skipped` |
| `steps.<id>.requirements.<rid>.outcome` / `.reason` | A step requirement's verdict. Readable by actions and `when`, never by checks |
| `requirement.id`, `requirement.statement` | The requirement being checked, inside its check |

Child fields use dots; array indexes are zero-based (`slides[0]`). Wildcards (`[*]`) work only in an assertion's `each`. There is no `$.` prefix.

A step's optional `when` is a condition with the same shape as an assertion, `{path, operator, expected}`, not an expression language; when it is false, the step is skipped. A reference in an action or `when` makes a dependency; `depends_on` adds order without data. Independent steps run concurrently. A step runs its action, then its own requirements, before anything that depends on it starts. Whole-Verifier requirements run after every step. Requirements never feed each other, and each verdict reaches `decide` exactly once. Registration refuses cycles, unknown references, and references a position cannot read.

### Skips, missing evidence, and decide

| Situation | Requirement outcome |
| --- | --- |
| Its step was skipped by a false `when`, or only by an intentionally skipped dependency | `not_applicable` |
| Its step failed or timed out, or a dependency was blocked by such a failure | `inconclusive` |
| It needs output from an intentionally skipped step | `not_applicable` |
| It needs failed output or any other missing required value | `inconclusive`; the check is not called |

When both apply, failure wins. Provider errors, malformed output, and infrastructure failures inside one check are `inconclusive` with a reason, never a judgment that the subject failed.

`decide` ignores only `not_applicable` requirements. With none left, the verdict is `inconclusive`.

- `all`: any `failed` → failed; otherwise any `inconclusive` → inconclusive; otherwise passed.
- `pass_rate`: passed divided by every applicable requirement, inconclusive ones included. At or above `threshold` (greater than 0, at most 1) passes. Below it, the verdict is `inconclusive` if any applicable result is inconclusive, otherwise `failed`. One pass and nine inconclusive results is 10%, not 100%.

## Bindings attach Verifiers to what they check

A binding lives in the declaration under `verified_by`. It names the Verifier, optionally the schedule that runs it, and the Operators that react to a failed background verdict. The support desk binds `no-refund-promise` to the whole Service and the other two to its `agent` component:

<Tabs>
<Tab label="support-desk.yaml">

<CodeFromFile file="examples/support-desk-app/service/support-desk.yaml" lang="yaml" title="service/support-desk.yaml" />

</Tab>
<Tab label="support-agent.yaml">

<CodeFromFile file="examples/support-desk-app/service/support-agent.yaml" lang="yaml" title="service/support-agent.yaml" />

</Tab>
<Tab label="support-prompt.yaml">

<CodeFromFile file="examples/support-desk-app/service/support-prompt.yaml" lang="yaml" title="service/support-prompt.yaml" />

</Tab>
</Tabs>

The subject is the component the binding sits under; you can also bind on the Service itself or on a standalone Agent. Changing a binding creates a new Service version, and every result records the version it checked.

| Field | Holds | If you leave it out |
| --- | --- | --- |
| `verifier` | A Verifier Card, by path or exact reference | Required |
| `runs_on` | A `schedule` or `group_end` Trigger | The server never runs this binding on its own |
| `on_failure` | Operators, inline or as Operator Cards | A failed verdict is stored, and nobody is notified |

[Bind a Verifier](/build-and-verify/declare/bind-a-verifier/#bind-a-verifier-to-a-schedule) shows a scheduled binding with its Trigger and Operator.

`group_end` judges a whole caller group, such as one conversation or one event, once its evidence goes quiet. The bound Verifier reads source slots such as `verifiers.turn-check.inputs.turn`, and callers pass the same `group_id` on every `verify` or `submit` of that group:

```yaml
runs_on: {kind: group_end, after: 30m}
```

`after` is at most one year (`365d`, `8760h`, `525600m` or `31536000s`); registration refuses a longer duration, standalone or inline, before anything is stored.

Each time Scribe acknowledges a grouped source input or result, the group becomes due `after` from then; new evidence moves that time later. Once it is due, one group execution judges every source input and result visible at that moment, listed in arrival order, with `null` for an outcome not yet visible. Later evidence makes the group due again and produces a new execution; earlier executions keep the evidence they judged. `on_verdict_change` Operators run when a group's verdict differs from its previous one.

Three limits apply. Steady activity postpones the judgment for as long as it continues. A judgment uses what is visible when it reads, and evidence that becomes visible later without new activity does not schedule another. If the server dies after Scribe acknowledges evidence but before it records the due time, that activity schedules nothing; the next grouped evidence does.

When a group comes due while its binding owner is inactive or retired, or the owner's authority is refused, the server records one `errored` group execution with the reason in `error` and drops the due group. Nothing is evaluated, no verdict or result is written and no Operator runs. The binding status's `last_execution_id` names that execution, but only until a later execution replaces it; there is no list of earlier failures. Reactivating the owner does not bring the dropped group back; the group's next evidence schedules it again. A transient server, database or query failure instead keeps the group due and retries it on a later pass.

## Run evidence and window evidence

The check kinds decide how evidence arrives. Registration refuses a Verifier that mixes the two, or mixes `samples` with other slot types.

| Evidence | `verify` | `submit` |
| --- | --- | --- |
| Run (`json`, `media`, `trace`; assert, trace, agent, code checks) | Runs now and returns the result | Queues one execution and returns its handle |
| Window (`samples`; drift checks) | Refused: use `submit` | Records the samples and returns `Recorded` |

### verify: run now

```python
result = run.observe.verify("no-refund-promise", answer=answer)
if result.verdict != "passed":
    ...  # your service decides what to send instead
```

`verify` returns within its 60-second deadline. A `failed` verdict is an ordinary value, not an exception. The server stores the inputs and result after it answers, but these inputs never start queued work and never run Operators. [Verify a request in real time](/build-and-verify/verify/verify-a-request-in-real-time/) covers it end to end.

### submit: queue it

`submit` uploads missing files, stages the evidence, and returns a `verify.Execution` with a server-issued execution ID:

```python
execution = run.for_card("agent").observe.submit("follows-policy", letter=verify.File("letter.md"))
result = execution.wait(60)  # here, or later from any process with execution.id
```

The same engine runs it later. [Run a verification and inspect its evidence](/build-and-verify/verify/run-a-verification-and-inspect-its-evidence/) follows one from submission to result, including from another process.

`202` means the server staged the evidence in memory, not that the queue has durably accepted it. The SDK sends one `Idempotency-Key` and reuses it on every transport retry. A conflicting reuse of a key is refused when the queue writer accepts the request, and you see it as an `errored` execution with `WYRD_REGISTRY_409_IDEMPOTENCY_CONFLICT`. Evidence lost before the queue accepts it, such as on an abrupt restart, has no outcome; `wait` ends with a timeout rather than an invented result.

## Executions and verdicts

An execution's status says whether the check ran; its verdict says what the check found.

| Status | Means |
| --- | --- |
| `pending`, `running`, `retrying` | Not finished. An execution gets 3 attempts, each with a 5-minute limit |
| `completed` | The Verifier produced a verdict. The result may become visible shortly after |
| `timed_out`, `errored`, `cancelled` | No verdict exists. The execution records the error |

<WyrdDiagram name="run-lifecycle" />

| Verdict | Means | Runs Operators |
| --- | --- | --- |
| `passed` | The evidence met the standard | No |
| `failed` | The evidence violated the standard | Yes, for background work of a binding |
| `inconclusive` | No judgment was possible | No |

Each requirement in the breakdown is `passed`, `failed`, `inconclusive`, or `not_applicable`, with its reason, citations, permitted actual value, check-specific detail such as drift statistics, and the exact check, Agent, Prompt, or code that judged it. Each step reports status, attempts, duration, failure reason, and tool calls.

## Results are evidence

Every result carries the execution ID, the application `run_id` when there is one, the exact Verifier and subject, the verdict, the requirement and step breakdown, and the digest of every input. It is written to `vala.verification.results`; pinned input declarations go to `vala.verification.execution_inputs`. Code hashes, extraction versions, Agent and Prompt versions, effective routes, and drift baselines and windows are recorded with the result.

Agent output is attributable evidence, not a promise that the same call would say the same thing again. Deterministic checks always give the same answer.

`run_id` is your application's run; one run can have many executions. `execution_id` names one Verifier execution. Scheduled work has no `run_id`.

## Operators react to failure

An Operator runs when background work of a binding fails. A direct `verify`, an `inconclusive` verdict, and an execution error never dispatch.

| Operator `kind` | Does |
| --- | --- |
| `notify` | Sends a Slack message or a PagerDuty event |
| `http` | Calls a URL you choose: `get`, `post`, `put`, `patch`, or `delete`, with optional bearer, basic, or header auth |

```yaml title="page-support-oncall.yaml"
apiVersion: wyrd/v1
kind: Operator
metadata:
  space: default
  name: page-support-oncall
  version: 1.0.0
spec:
  kind: notify
  channel:
    kind: pager_duty
    connection: support-pagerduty
    route: support
    severity: error
    summary: "{{verifier_ref}} failed for {{subject_ref}}"
```

Secrets never go on the Card. An Operator names an operator connection, which a tenant administrator creates once. Each Operator is delivered on its own, at least once: up to 3 attempts within 5 minutes. A replayed queue request never causes a second logical dispatch, and one failing Operator does not stop the others or change the verdict.

## What can go wrong at registration

| Error code | Cause |
| --- | --- |
| `WYRD_SPEC_400_INVALID_VERIFIER_REF_KIND` | `verifier:` points at a Card that is not a Verifier |
| `WYRD_SPEC_400_INVALID_BINDING_REF_KIND` | `runs_on:` is not a Trigger, or an `on_failure` entry is not an Operator |
| `WYRD_SPEC_400_DUPLICATE_VERIFICATION_BINDING` | The same Verifier version is bound twice to one subject |
| `WYRD_SPEC_400_OPERATOR_CONNECTION_UNAVAILABLE` | The named operator connection is missing or disabled |

Registration also refuses duplicate block IDs, unknown or cyclic references, a check-position Agent with tools, and a Verifier that mixes run and window evidence, each with a structured error naming the offending field.

Verifiers judge recorded evidence, so the next page covers what a run records: [Runs, observations, and retained evidence](/understand/runs-observations-and-evidence/).
