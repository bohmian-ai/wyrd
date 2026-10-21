---
title: How verification works
description: Inputs, steps, requirements, and decide; verify versus submit; executions, verdicts, results, and Operators.
pillar: wyrd
section: Understand
order: 2
---

A Verifier checks declared requirements against evidence and returns a verdict: `passed`, `failed`, or `inconclusive`.

## The parts of a Verifier

This is the Verifier from [Run your first verification](/get-started/quickstart/):

<CodeFromFile file="examples/quickstart/refund-limit.yaml" lang="yaml" title="refund-limit.yaml" />

| Part | Holds |
| --- | --- |
| `inputs` | Named slots for the evidence, each with a `type` |
| `steps` | Optional preparation, such as extracting text from a file |
| `requirements` | Each has an `id`, a `statement`, and one `check`, and gets its own verdict |
| `decide` | How requirement verdicts combine: `all` (the default) or a `pass_rate` |

A Verifier never names what it checks. A binding supplies the subject, so one Verifier can check many Cards. [Verifier Card](/reference/verifier-card/) lists every input type, step, and check.

## Bindings

A Service binds a Verifier under `verified_by`, as `support-reply` binds `refund-limit`. A binding can also name a Trigger in `runs_on`, so the server runs it on a schedule, and Operators in `on_failure`.

## verify and submit

`verify` runs the Verifier now and returns the result. A `failed` verdict is an ordinary value, not an exception:

```python
result = run.observe.verify("refund-limit", {"refund": 40})
```

`submit` queues the work on the server and returns an execution you can wait on, here or from another process:

```python
execution = run.observe.submit("refund-limit", {"refund": 250})
result = execution.wait(60)
```

Drift Verifiers take samples over a time window, so they accept only `submit`.

## Verdicts

With `all`, any failed requirement fails the Verifier. Otherwise any inconclusive requirement makes it inconclusive. Otherwise it passes.

`inconclusive` means no judgment was possible, such as missing evidence or an error inside a check. It is never a pass, and never a claim that the subject failed.

## Results

Every result is stored in Bifrost, in `vala.verification.results`, with the exact Verifier and subject versions it judged. The inputs go to `vala.verification.execution_inputs`. You can query both with SQL.

## Operators

An Operator reacts when a binding's background work fails, by sending a Slack or PagerDuty notification or calling a URL. `verify` never runs Operators.

## Next

[Runs, observations, and retained evidence](/understand/runs-observations-and-evidence/).
