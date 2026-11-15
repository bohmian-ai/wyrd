---
title: Real-time verification
description: Real-time verification for AI applications - get an evidence-backed verdict before your application sends a response or takes an action, and how Wyrd also runs the same checks queued or on a schedule.
pillar: wyrd
section: Understand
order: 2
---

Real-time verification returns a judgment while the application can still act on it: before it sends a response or commits an action. Wyrd is a verification engine built for that request path, and it runs the same Verifiers asynchronously when no request is waiting.

[Foundations of verification](/understand/foundations-of-verification/) explains verification without Wyrd. This page shows how Wyrd places a verdict inside an application's decision.

## A verdict before the response

A finance API receives a request to send a customer report to an outside party. Before it sends anything, it verifies the release:

<CodeFromFile file="examples/finance-release/api.py" lang="python" title="api.py" region="handler" />

The request moves through six stages:

1. **Request.** `POST /reports/send` arrives with the report, its purpose, the recipient, and the destination.
2. **Requirements and evidence.** The `release-check` Verifier declares what must be true and which evidence it reads. The request body is the evidence; the policy is a file the Verifier version names.
3. **Verification.** `run.observe.verify` sends the evidence to Wyrd, which runs every requirement's check.
4. **Verdict.** Wyrd returns `passed`, `failed`, or `inconclusive`, with an outcome and reason for each requirement.
5. **Decision.** The application releases only a `passed` report. Everything else is held for a person.
6. **Response.** The caller learns whether the report was released or held.

Because the verdict arrives before step 6, the application can release, hold, or route the request to review while nothing has left. A check that runs after the report is sent can only explain what went wrong.

Verification during execution, sometimes called live verification, uses this same call: a workflow calls `verify` before its next step, as the finance API does before it sends.

## The contract the application calls

`release-check` is a declared, versioned Verifier. This is its YAML form; the [Quickstart](/get-started/quickstart/) declares the same Verifier in Python, Rust, or TypeScript:

<CodeFromFile file="examples/finance-release/release-check.yaml" lang="yaml" title="release-check.yaml" />

The declaration fixes what the application can rely on:

| Part | What it fixes |
| --- | --- |
| `inputs` | The named evidence slots the caller must fill, each with a type and optional schema |
| `requirements` | Each `id` and `statement`, and the one `check` that decides it |
| `decide` | How requirement outcomes combine. Omitted here, so the default `all` applies: any failed requirement fails the verdict, otherwise any inconclusive one makes it inconclusive |
| Version | When `metadata.version` is omitted, the server assigns `0.1.0`, then the next patch version each time the content changes. Each result names the exact Verifier, policy file, and subject versions |

When the policy changes, declare the Verifier again with the new file: Wyrd registers a new Verifier version (in the Quickstart, `release-check@0.1.1`). Results judged by the old version still name the old file, so a past decision can be traced to the policy it applied. [How verification works](/understand/how-verification-works/) explains each part, and the [Verifier Card reference](/reference/verifier-card/) lists every field.

The contract is Wyrd's own public format. It makes requirements explicit and attributable; it does not make them complete, and a passing verdict is a judgment on the evidence supplied, not a proof.

## Where the time goes

Whether a verdict fits a request depends on the application's latency budget and on which checks the Verifier runs. Time in a `verify` call falls into three parts.

**Engine and coordination.** The server resolves the exact Verifier and subject, checks the caller's `verifier:run` permission, and admits the work.

Resolved targets are cached in each server process, per tenant, Verifier, and subject. A first call, an evicted entry, or a change to the Verifier reads PostgreSQL to reload; while the process cannot hear change notifications, every call reads PostgreSQL.

Admission can refuse work instead of queueing it: `WYRD_VERIFICATION_429_STAGING_FULL` when staging is full, `WYRD_SERVER_503_SERVICE_UNAVAILABLE` when no Python worker or workspace is free.

**Evidence.** JSON evidence arrives in the request. A `File` is hashed by the SDK, which uploads only digests the server lacks, so a repeated file uploads once; each execution resolves its file and Artifact metadata once and reuses it. A step such as PDF extraction, or a `trace` check that reads spans from Bifrost, adds its own read time.

**Checks.** An `assert` or `trace` check runs in the check engine without a model. A `code` check runs in a sandboxed Python worker. An `agent` check calls a model through the LLM gateway, possibly several times with tool calls between them, and usually dominates the total. In the finance example, the two assertions add little; the policy judge (`policy-judge.yaml` in the Quickstart) adds up to three model calls (`max_iterations: 3`), shared between tool use and answer corrections.

Returning the verdict does not wait for the evidence to be published. The server stages the result and its pinned inputs on its in-process outbox and answers; Bifrost then stores them, and they become queryable shortly after. The verdict the application received stands even if an abrupt server stop loses the staged copy before it is written; then no record of that verdict is retained, and the loss is counted. [Runs and evidence](/understand/runs-observations-and-evidence/) and [Bifrost](/understand/bifrost/) explain retention.

Real-time here means a verdict available for the current decision. It is not hard real-time scheduling and not a latency guarantee. The whole call is bounded by `WYRD_VERIFICATION_TIMEOUT_MS`, 60 seconds by default ([Runtime limits](/reference/verification-runtime-limits/)). A synchronous call is not proof a Verifier fits a budget: measure the Verifier you deploy, on your own evidence and model routes.

## Queued and scheduled verification

The same engine runs two more paths. They change when the verdict arrives, and so which decision it can inform.

| Path | Called with | Verdict arrives | Use it when |
| --- | --- | --- | --- |
| Request | `verify` | In the call's response | The application must decide before it responds or acts |
| Queued | `submit` | Later; any process holding the execution can wait for it | The decision can wait, a model judge is too slow for the request, or evidence such as trace spans lands after the call |
| Scheduled | A binding with a Trigger in `runs_on` | When the server runs the schedule | A time window of recorded samples needs judging, as with drift. Only drift Verifiers are scheduled, and they refuse `verify` |

`submit` returns once its inputs are staged, and the server then queues an execution. A queued execution has a status (`pending`, `running`, `retrying`, `completed`, `cancelled`, `timed_out`, `errored`) separate from its verdict. Only executions a binding's Trigger creates run Operators; `verify` and `submit` leave the response to the application. [verify and submit](/understand/how-verification-works/#verify-and-submit) compares the calls in detail.

## Missing evidence, timeouts, and errors

Each non-pass outcome has a defined meaning, and the application decides its response to each:

| Situation | `verify` returns | Queued execution |
| --- | --- | --- |
| An input slot is omitted | The requirements that need it are `inconclusive` | The same |
| A trace has no visible spans yet | The `trace` check is `inconclusive` at once | Tries 3 times, then `inconclusive` |
| A model judge cannot decide, or passes without a valid citation | That requirement is `inconclusive` | The same |
| The call exceeds the timeout | `WYRD_VERIFICATION_504_EXECUTION_TIMED_OUT`, no verdict | Status `timed_out`, no verdict |
| The input does not match the Verifier | `WYRD_VERIFICATION_400_INPUT_INVALID`; nothing ran | No verdict: `submit` is refused or the execution ends `errored` |

`inconclusive` is never a pass and never a claim that the subject failed. An error or timeout is no verdict at all. The finance API releases only `passed` reports, so every other outcome holds the report. The [technical questions](/understand/technical-questions/) cover the remaining adoption questions.

## Next

Run the finance example in the [Quickstart](/get-started/quickstart/), or read [How verification works](/understand/how-verification-works/) for each part of a Verifier.
