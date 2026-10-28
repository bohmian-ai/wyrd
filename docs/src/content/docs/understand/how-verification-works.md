---
title: How verification works
description: Inputs, steps, requirements, and decide; Agent workspaces; verify versus submit; executions, verdicts, results, and Operators.
pillar: wyrd
section: Understand
order: 2
---

A Verifier checks declared requirements against evidence and returns a verdict: `passed`, `failed`, or `inconclusive`. A binding attaches the Verifier to the Card it checks, and the server records every verdict with the exact versions it judged.

## The parts of a Verifier

This is the Verifier from [Run your first verification](/get-started/quickstart/):

<CodeFromFile file="examples/quickstart/no-refund-promise.yaml" lang="yaml" title="no-refund-promise.yaml" />

| Part | Holds |
| --- | --- |
| `inputs` | Named slots for the evidence, each with a `type` |
| `steps` | Optional preparation, such as extracting text from a file |
| `requirements` | Each has an `id`, a `statement`, and one `check`, and gets its own verdict |
| `decide` | How requirement verdicts combine: `all` (the default) or a `pass_rate` |

A Verifier never names what it checks. A binding supplies the subject, so one Verifier can check many Cards. [Verifier Card](/reference/verifier-card/) lists every input type, step, and check.

<WyrdDiagram name="verification-parts" />

## Steps and checks

A step prepares evidence, such as the text of a PDF, and its output feeds later steps and checks. A check decides one requirement. Each has a `kind`, and some kinds only make sense in one role:

| Kind | Used as | Produces or decides | Use it when | Runs in |
| --- | --- | --- | --- | --- |
| `assert` | check | Compares one value from an input or step output with an operator | The rule is a field comparison: a status equals `approved`, a reply does not contain "guaranteed" | Check engine |
| `trace` | check | Compares values selected from the spans of a trace | The rule is about what your code did, such as calling the order lookup before replying | Check engine |
| `drift` | check | Compares a window of samples with a baseline | You care whether inputs or outputs shift over time, not whether one answer is right | Check engine, on a schedule |
| `extract` | step | The text and structure of a file | A later check or Agent needs to read a PDF, DOCX, XLSX, or other document | Check engine |
| `code` | step or check | As a step, JSON; as a check, a verdict | The rule is deterministic, but no operator expresses it | Monty pool |
| `agent` | step or check | As a step, the Agent's response; as a check, a verdict | The rule needs judgment: meaning, tone, or policy | LLM gateway |

Start with the cheapest kind that catches the failure. The check engine needs no model and no Python; an `agent` check costs a model call each time it runs.

The Monty pool is sandboxed Python: a fresh worker per call that the server starts from its own binary. An `agent` step or check runs an Agent whose model calls go through the LLM gateway. Step and check Agents follow the same rules: each can call the [built-in tools](/reference/agent-tools-and-authorization/) it declares, and every call still needs its own grants. [Recipes](/build-and-verify/verify/verifier-recipes/) has a working Verifier for each kind.

## Workspaces

An Agent that reads files works in a workspace. The Verifier maps paths under `/inputs` to evidence: an input slot such as `inputs.policy`, or files of an uploaded Artifact Card. The Agent explores them with `fs.list`, `fs.read`, and `fs.search`, and can run `code.run` over them.

An Artifact selection is part of the Verifier version. Uploading a newer version of that Artifact does not change what the Verifier reads, and a caller cannot replace the files at run time. To judge new files, publish a new Verifier version that names them. A selection that matches no file fails before any model call.

Files are mounted unchanged and unparsed. `fs.read` and `fs.search` serve raw text, so they refuse or skip a PDF until the Agent calls `fs.extract`, which writes its extraction JSON under `/work`. `/work` is scratch for one call and is never evidence: a citation names the `/inputs` file's `attachment_<n>` slot, not a `/work` path. [Uploaded files](/build-and-verify/verify/verifier-recipes/#uploaded-files) shows the upload and the mount.

## Bindings

A binding attaches a Verifier to what it checks, under `verified_by`. It can sit on a Service, on one of its components (an Agent, Model, Data, Prompt, or any other Card the Service uses), or on a standalone Agent. `support-reply` binds `no-refund-promise` to the whole Service. A binding can also name a Trigger in `runs_on`, so the server runs it on a schedule, and Operators in `on_failure`.

## verify and submit

`verify` runs the Verifier now and returns the result. A `failed` verdict is an ordinary value, not an exception:

```python
result = run.observe.verify("no-refund-promise", answer="Your refund request is approved.")
```

`submit` queues the work on the server and returns an execution that any process holding it can wait on:

```python
execution = run.observe.submit("no-refund-promise", answer="You have a guaranteed refund.")
result = execution.wait(60)
```

<WyrdSequence name="verification-paths" />

Drift Verifiers judge samples over a time window, so they refuse `verify`. Their `submit` records samples, and a scheduled binding judges the window.

## Verdicts

With `all`, any failed requirement fails the Verifier. Otherwise any inconclusive requirement makes it inconclusive. Otherwise it passes. Requirements that are `not_applicable` do not count, and a Verifier with no applicable requirement is inconclusive.

`inconclusive` means no judgment was possible, such as missing evidence or an error inside a check. It is never a pass, and never a claim that the subject failed.

An `agent` or `code` check can also return a `data` object beside its verdict, such as a score or labels, when its `response_format` declares a schema for it. The server checks the whole response against that schema. A response that does not match makes the requirement `inconclusive` and keeps no data, so a requirement's `data` is either valid or absent. [Custom data](/reference/verifier-card/#custom-data) has the schema rules and size limits.

<WyrdDiagram name="run-lifecycle" />

A queued execution has a status before it has a verdict. `completed` means it produced a result; `timed_out` and `errored` mean no check finished, so there is no verdict and no Operator runs.

## Results

Every result is stored in Bifrost, in `vala.verification.results`, with the exact Verifier and subject versions it judged. The inputs go to `vala.verification.execution_inputs`. Both are queryable with SQL.

## Operators

An Operator reacts when a scheduled execution of a binding returns `failed`, by sending a Slack or PagerDuty notification or calling a URL. `verify` and `submit` never run Operators; your code reads their verdicts.

## Next

[Runs, observations, and retained evidence](/understand/runs-observations-and-evidence/).
