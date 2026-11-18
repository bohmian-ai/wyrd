---
title: How verification works in Wyrd
description: How Wyrd implements verification - Verifiers, bindings, and evidence; one inline verification of a finance report release; verify, submit, and drift schedules; where the time goes; and what Wyrd retains.
pillar: wyrd
section: Understand
order: 3
---

Wyrd is one implementation of the practices in [Foundations of verification](/understand/foundations-of-verification/) and [Real-time verification](/understand/real-time-verification/): a server that runs declared checks against the evidence an application supplies and returns a result while the application can still act on it. It does not choose the requirements or act for the application, and a passing verdict is a judgment on the evidence supplied, not a proof or a certification.

## The concepts in Wyrd

Each idea from the two concept pages has a Wyrd counterpart. These names are Wyrd's own, not general terminology:

| Concept | In Wyrd |
| --- | --- |
| A set of requirements and their checks | A **Verifier**: a versioned declaration |
| A requirement | An entry in the Verifier's `requirements`: an `id`, a `statement`, and one `check` |
| Evidence | The named `inputs` the caller supplies, files the Verifier version mounts, and trace spans Wyrd reads |
| A check | A check `kind`: `assert`, `trace`, `drift`, `code`, or `agent` |
| A result | An outcome per requirement and an overall verdict: `passed`, `failed`, or `inconclusive` |
| Evidence that cannot decide | `inconclusive` |
| A check that did not run | Inside one check, `inconclusive` for that requirement; for the whole call or execution, an error or a timeout with no verdict |
| Provenance | Each result names the exact Verifier and subject versions it judged |

A **Card** is Wyrd's versioned record of a component, such as a Service or an Agent. A **binding** attaches a Verifier to the Card it checks, and that Card is the result's **subject**.

## One verification, end to end

The example is the report release from [Real-time verification](/understand/real-time-verification/#the-same-check-can-arrive-too-late), built as a finance API. Before it sends a report to an outside party, the API verifies the release. The request moves through six stages, and [Act on verdicts in production](/build-and-verify/verify/act-on-verdicts-in-production/#1-release-only-what-passed) shows the handler's code:

1. **Request.** `POST /reports/send` arrives with the report, its purpose, the recipient, and the destination.
2. **Requirements and evidence.** The `release-check` Verifier declares what must be true and which evidence it reads. The request body is the evidence; the sharing policy is a file the Verifier version names.
3. **Verification.** `run.observe.verify` sends the evidence to Wyrd, which runs every requirement's check.
4. **Verdict.** Wyrd returns `passed`, `failed`, or `inconclusive`, with an outcome and a reason for each requirement.
5. **Decision.** The application releases only a `passed` report and holds everything else for a person.
6. **Response.** The caller learns whether the report was released or held.

Because the verdict arrives before step 6, nothing has left when the application decides. This is the inline case; a workflow verifies during execution with the same call before its next step.

`release-check` declares three requirements:

<CodeFromFile file="examples/finance-release/release-check.yaml" lang="yaml" title="release-check.yaml" />

Two are assertions. `recipient-authorized` compares `inputs.recipient.authorized` with `true`, and `destination-restricted` compares `inputs.destination.restricted` with `true`. Both read facts the application supplies in the request. Wyrd does not query an authorization service here: it judges what the application reports that lookup returned, so the origin of that fact is the application's to record. Like the log line in [Foundations](/understand/foundations-of-verification/#what-is-being-checked-and-against-what), the check is no better than that supplied fact; the example keeps it simple, and a production Verifier can instead receive the lookup's own response, or check the lookup call in a trace. `follows-policy` is a model judge. An Agent reads the purpose, the report, the recipient, and the extracted policy, then decides whether the policy permits the release.

The [Quickstart](/get-started/quickstart/) declares the same Verifier in Python, Rust, or TypeScript and runs it.

## How a Verifier declares requirements

A Verifier has four parts:

| Part | Holds |
| --- | --- |
| `inputs` | Named evidence slots the caller fills, each with a `type` and an optional schema |
| `steps` | Optional preparation, such as extracting text from a file |
| `requirements` | Each has an `id`, a `statement`, and one `check`, and gets its own outcome |
| `decide` | How requirement outcomes combine: `all`, the default, or a `pass_rate`. `release-check` omits it, so `all` applies |

Because a binding supplies the subject, one Verifier can check many Cards. [Verifier Card](/reference/verifier-card/) lists every field, input type, step, and check.

When `metadata.version` is omitted, the server assigns `0.1.0`, then the next patch version each time the content changes.

<WyrdDiagram name="verification-parts" />

### Steps and checks

A step prepares evidence, such as the text of a PDF, and its output feeds later steps and checks. A check decides one requirement. Each has a `kind`, and some kinds only make sense in one role:

| Kind | Used as | Produces or decides | Use it when | Runs in |
| --- | --- | --- | --- | --- |
| `assert` | check | Compares one value from an input or step output with an operator | The rule is a field comparison: a status equals `approved`, a reply does not contain "guaranteed" | Check engine |
| `trace` | check | Compares values selected from the spans of a trace | The rule is about what your code did, such as calling the order lookup before replying | Check engine |
| `drift` | check | Compares a window of samples with a baseline | You care whether inputs or outputs shift over time, not whether one answer is right | Check engine, on a schedule |
| `extract` | step | The text and structure of a file | A later check or Agent needs to read a PDF, DOCX, XLSX, or other document | Check engine |
| `code` | step or check | As a step, JSON; as a check, a verdict | The rule is deterministic, but no operator expresses it | Monty pool |
| `agent` | step or check | As a step, the Agent's response; as a check, a verdict | The rule needs judgment: meaning, tone, or policy | LLM gateway |

The cheapest kind that catches the failure is the usual starting point. The check engine needs no model and no Python; an `agent` check costs a model call each time it runs.

The Monty pool is sandboxed Python: a fresh worker per call that the server starts from its own binary. An `agent` step or check runs an Agent whose model calls go through the LLM gateway. Step and check Agents follow the same rules: each can call the [built-in tools](/reference/agent-tools-and-authorization/) it declares, and every call still needs its own grants. [Recipes](/build-and-verify/verify/verifier-recipes/) has a working Verifier for each kind.

### Workspaces and fixed policy files

An Agent that reads files works in a workspace. The Verifier maps paths under `/inputs` to evidence: an input slot such as `inputs.policy`, or files of an uploaded Artifact Card. `release-check` mounts the extracted text of `company-sharing-policy@0.1.0`. The Agent explores its files with `fs.list`, `fs.read`, and `fs.search`, and can run `code.run` over them.

An Artifact selection is part of the Verifier version. Uploading a newer version of that Artifact does not change what the Verifier reads, and a caller cannot replace the files at run time. To judge against a new policy, declare the Verifier again with the new file, which registers a new Verifier version; the Quickstart's [policy update](/get-started/quickstart/#5-update-the-policy) creates `release-check@0.1.1`. A selection that matches no file fails before any model call.

Files are mounted unchanged and unparsed. `fs.read` and `fs.search` serve raw text, so they refuse or skip a PDF until the Agent calls `fs.extract`, which writes its extraction JSON under `/work`. `/work` is scratch for one call and is never evidence: a citation names the `/inputs` file's `attachment_<n>` slot, not a `/work` path. [Uploaded files](/build-and-verify/verify/verifier-recipes/#uploaded-files) shows the upload and the mount.

### Bindings

A binding attaches a Verifier to what it checks, under `verified_by`. It can sit on a Service, on one of its components (an Agent, Model, Data, Prompt, or any other Card the Service uses), or on a standalone Agent. A binding can also name a Trigger in `runs_on`, so the server runs it on a schedule, and Operators in `on_failure`.

## Code or YAML

A Verifier, and every other Card, can be declared in two ways. Both produce the same Cards, and the server can't tell them apart.

- **In code.** The Python, Rust, and TypeScript SDKs build Cards with the same field names as the YAML. This is the quickest way to start: one program declares the Cards and verifies with them. The [Quickstart](/get-started/quickstart/) and [Author in code](/build-and-verify/declare/author-in-code/) use this route.
- **In YAML files.** Each Card is a file that lives in the repository beside the application. This is the recommended route in production. Any language can declare the same files, a change to a requirement is reviewed in a pull request like any other change, and CI declares the files with `wyrd declare` before the application deploys. The [Build and verify](/build-and-verify/declare/define-a-service/) guides and [Deploy a Service from CI](/operate/deploy-a-service-from-ci/) use this route.

A team can start in code and move to YAML later, because both routes declare the same Cards.

## Calls, verdicts, and failures

### verify and submit

Wyrd runs a Verifier on three paths. They differ in when the verdict arrives, and so in which decision it can inform:

| Path | Called with | Verdict arrives | Use it when |
| --- | --- | --- | --- |
| Inline | `verify` | In the call's response | The application must decide before it responds or acts |
| Queued | `submit` | Later; any process holding the execution can wait for it | The decision can wait, a model judge is too slow for the request, or evidence such as trace spans lands after the call |
| Scheduled | A binding with a Trigger in `runs_on` | When the server runs the schedule | A time window of recorded samples needs judging. Only drift Verifiers are scheduled, and they refuse `verify` |

`verify` runs the Verifier now and returns the result. These calls use `no-refund-promise`, the Verifier from the [setup check](/get-started/setup/#3-check-it-end-to-end). A `failed` verdict is an ordinary value, not an exception:

```python
result = run.observe.verify("no-refund-promise", answer="Your refund request is approved.")
```

`submit` returns once its inputs are staged, and the server then queues an execution that any process holding it can wait on:

```python
execution = run.observe.submit("no-refund-promise", answer="You have a guaranteed refund.")
result = execution.wait(60)
```

<WyrdSequence name="verification-paths" />

Drift Verifiers judge samples over a time window, so they refuse `verify`. Their `submit` records samples, and a scheduled binding judges the window. Wyrd schedules only drift. To run any other Verifier on a recurring basis, call `submit` from the application's own scheduler.

### Verdicts

With `all`, any failed requirement fails the Verifier. Otherwise any inconclusive requirement makes it inconclusive. Otherwise it passes. A requirement is `not_applicable` when the step it belongs to is skipped because its `when` condition does not hold; those requirements do not count, and a Verifier with no applicable requirement is inconclusive.

`inconclusive` means no judgment was possible, such as missing evidence or an error inside one check. An error or timeout of the whole call or execution is different: it produces no verdict at all. `inconclusive` is never a pass, and never a claim that the subject failed.

An `agent` or `code` check can also return a `data` object beside its verdict, such as a score or labels, when its `response_format` declares a schema for it. The server checks the whole response against that schema. A response that does not match makes the requirement `inconclusive` and keeps no data, so a requirement's `data` is either valid or absent. [Custom data](/reference/verifier-card/#custom-data) has the schema rules and size limits.

<WyrdDiagram name="run-lifecycle" />

A queued execution has a status before it has a verdict: `pending`, `running`, `retrying`, `completed`, `cancelled`, `timed_out`, or `errored`. `completed` means it produced a result; `timed_out` and `errored` mean no check finished, so there is no verdict and no Operator runs.

### Missing evidence, timeouts, and errors

Each outcome that is not a pass has a defined meaning, and the application decides its response to each:

| Situation | `verify` returns | Queued execution |
| --- | --- | --- |
| An input slot is omitted | The requirements that need it are `inconclusive` | The same |
| A trace has no visible spans yet | The `trace` check is `inconclusive` at once | Tries 3 times, then `inconclusive` |
| A model judge cannot decide, or passes without a valid citation | That requirement is `inconclusive` | The same |
| The call exceeds the timeout | `WYRD_VERIFICATION_504_EXECUTION_TIMED_OUT`, no verdict | Status `timed_out`, no verdict |
| The input does not match the Verifier | `WYRD_VERIFICATION_400_INPUT_INVALID`; nothing ran | No verdict: `submit` is refused or the execution ends `errored` |

An error or a timeout is no verdict at all. The finance API releases only `passed` reports, so every other outcome holds the report. [Error codes](/api/errors/) lists every code.

### Trace evidence

A `trace` check judges the whole trace. It reads every retained span with that trace ID from `bifrost.traces.spans`, whatever Service, Card, or time each span carries, ordered by start time and then span ID. The read is an ordinary query by the account the execution runs as, so the same permissions, tenant boundary, and audit apply. Each attempt reads a trace once, and every check in the Verifier that names that trace judges the same spans.

The check is `inconclusive`, never judged on part of a trace, when the trace has more than 10,000 spans, the query is refused or fails, the read times out, a span cannot be decoded, or the input supplied no trace. A selector that matches no span compares `null`.

Spans can land after the call that verifies them. A trace with no visible span has not landed yet, and the outcome depends on the path:

- `verify` judges the check `inconclusive` at once.
- A queued execution, from `submit` or a schedule, stops the attempt with error code `evidence_unavailable` and status `retrying`, and records no result. It tries 3 times, waiting 30 seconds and then 60 seconds. The last attempt judges the check `inconclusive` and completes under the Verifier's `decide` rule.

A completed result is final. Spans that land after it are judged only by a new execution.

### Operators

An Operator reacts when a scheduled execution of a binding returns `failed`, by sending a Slack or PagerDuty notification or calling a URL. `verify` and `submit` never run Operators; your code reads their verdicts.

## Where the time goes

Whether a verdict fits a request depends on the application's deadline and on which checks the Verifier runs. Time in a `verify` call falls into three parts.

**Engine and coordination.** The server resolves the exact Verifier and subject, checks the caller's `verifier:run` permission, and admits the work. Resolved targets are cached in each server process, per tenant, Verifier, and subject. A first call, an evicted entry, or a change to the Verifier reads PostgreSQL to reload; while the process cannot hear change notifications, every call reads PostgreSQL. Admission can refuse work instead of queueing it: `WYRD_VERIFICATION_429_STAGING_FULL` when staging is full, `WYRD_SERVER_503_SERVICE_UNAVAILABLE` when no Python worker or workspace is free.

**Evidence.** JSON evidence arrives in the request. A `File` is hashed by the SDK, which uploads only digests the server lacks, so a repeated file uploads once; each execution resolves its file and Artifact metadata once and reuses it. A step such as PDF extraction, or a `trace` check that reads spans from Bifrost, adds its own read time.

**Checks.** An `assert` or `trace` check runs in the check engine without a model. A `code` check runs in a sandboxed Python worker. An `agent` check calls a model through the LLM gateway, possibly several times with tool calls between them, and usually dominates the total. In the finance example, the two assertions add little; the policy judge (`policy-judge.yaml` in the Quickstart) adds up to three model calls (`max_iterations: 3`), shared between tool use and answer corrections.

The whole call is bounded by `WYRD_VERIFICATION_TIMEOUT_MS`, 60 seconds by default ([Runtime limits](/reference/verification-runtime-limits/)). That is a bound, not a latency guarantee, and Wyrd publishes no general latency figure. Only a measurement of the deployed Verifier, on its real evidence and model routes, shows whether it fits.

## What Wyrd retains, and how requirements change

Returning the verdict does not wait for its record, so storage never slows the request. The server queues the result and its pinned inputs in memory and answers. It batches queued records for at most 50 ms, then writes them to Bifrost's write-ahead log; once written they survive a crash, and they become queryable shortly after. A clean shutdown writes everything still queued. Only an abrupt server stop before that write loses a record; the verdict the application received still stands, and the loss is counted.

Stored results live in three Bifrost tables, all queryable with SQL: each result's summary in `bifrost.verification.results`, its requirement outcomes and steps in `bifrost.verification.execution_outputs`, and its inputs in `bifrost.verification.execution_inputs`. The result the call returns is compact: the verdict and each requirement's outcome. The checks, provenance, step outputs, and pinned evidence stay in storage for later inspection, as the Quickstart's [stored verdicts](/get-started/quickstart/#4-confirm-the-stored-verdicts) step shows. [Runs and evidence](/understand/runs-observations-and-evidence/) and [Bifrost](/understand/bifrost/) explain retention and query visibility.

Every stored result names the exact Verifier and subject versions it judged, including the policy file the Verifier version mounted. Declaring a new Verifier version for a changed policy does not retarget old results: those judged by `release-check@0.1.0` still name the original policy, so a past decision can be traced to the policy it applied. [Cards and versions](/understand/cards-versions-and-relationships/) explains versioning.

## Next

The [Quickstart](/get-started/quickstart/) runs the finance example. [Write verifiable requirements](/build-and-verify/declare/write-verifiable-requirements/), [Establish sufficient evidence](/build-and-verify/observe/establish-sufficient-evidence/), and [Act on verdicts in production](/build-and-verify/verify/act-on-verdicts-in-production/) apply this to your own system, and the [Verifier Card reference](/reference/verifier-card/) lists every field.
