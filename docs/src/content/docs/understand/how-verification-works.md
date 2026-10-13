---
title: How verification works
description: Verifiers, bindings, and Triggers; real-time checks versus the verifier runtime; verdicts, results, and Operators.
pillar: wyrd
section: Understand
order: 2
draft: true
---

A Verifier judges behavior against a standard and returns a verdict: `passed`, `failed`, or `inconclusive`. Wyrd runs a Verifier in one of two ways. Your code can call it while a request is in flight and act on the answer, or the server's verifier runtime can run it for you, on a schedule or as observations arrive, and react to failure without your code. Both write the result to the same Bifrost table, tied to the exact Card versions that ran.

<WyrdSequence name="verification-paths" />

The examples continue the `support-desk` service from [Cards, versions, and relationships](/understand/cards-versions-and-relationships/).

## Three kinds of Verifier

A Verifier is a Card. Its `implementation.kind` decides what it can judge and how it can run:

| Kind | Judges | Real time | Verifier runtime |
| --- | --- | --- | --- |
| `eval` | One observed record against a set of tasks and a pass gate | Yes, pass the record | Yes, with an `observations_ready` Trigger: once per eval observation |
| `drift` | A measured signal, such as a feature distribution, against a baseline | Yes, pass a batch of rows | Yes, with a `schedule` Trigger: once per time window |
| `task` | One assertion or LLM judgment on the context you pass | Yes, only this way | No |

A Verifier never names what it checks. The binding supplies the subject, so one `answer-quality` Verifier can check any number of agents.

Eval and task Verifiers share the same checks. A check is either an `assertion`, which compares a value in the context with an operator such as `equals`, `contains`, `is_subset`, or `matches_regex`, or an `llm_judge`, which asks an Agent for a structured judgment. Eval adds two checks that read recorded execution: `trace_assertion` (which spans ran) and `agent_assertion` (a field of the agent's workflow record). Those two need a recorded trace, so they run only in the verifier runtime.

Prefer an assertion whenever the check is deterministic. It is faster, costs nothing, and gives the same answer every time. Use an LLM judge for what an assertion cannot express, such as whether an answer is grounded in what the tool returned.

[Evaluate agent behavior](/build-and-verify/verify/evaluate-agent-behavior/) and [Monitor drift](/build-and-verify/verify/monitor-drift/) show complete Verifier Cards for each kind.

## Bindings attach Verifiers to what they check

A binding lives in the Service declaration under `verified_by`. It names the Verifier, optionally the Trigger that runs it, and the Operators that run when a triggered check fails:

```yaml title="service/support-desk.yaml (excerpt)"
spec:
  components:
    - alias: agent
      ref: ./support-agent.yaml
      verified_by:
        - verifier: ./answer-quality.yaml
          runs_on: {kind: observations_ready}
        - verifier: ./no-refund-promise.yaml
```

The desk binds two Verifiers to its agent. `answer-quality` is an LLM judge the server runs on every eval observation. `no-refund-promise` has no `runs_on`, so only your code calls it. A Verifier that should page someone adds `on_failure`; this is how [Monitor drift](/build-and-verify/verify/monitor-drift/) binds a drift Verifier to a model it adds to the desk:

```yaml
    - alias: churn_model
      ref: { kind: Model, name: churn-classifier, version: 1.4.2 }
      verified_by:
        - verifier: ./churn-input-drift.yaml
          runs_on: { kind: schedule, cron: "0 * * * *" }
          on_failure:
            - ./page-support-oncall.yaml
```

The subject of a binding is the component it sits under. You can also put `verified_by` on the Service itself, or on a standalone Agent. Because bindings are part of the declaration, changing what gets verified creates a new Service version, and every verdict records the version it was checked under.

| Field | Holds | If you leave it out |
| --- | --- | --- |
| `verifier` | A Verifier Card, by path or exact reference | Required |
| `runs_on` | A Trigger, inline or as a Trigger Card | The verifier runtime never runs this binding; only your code can call the Verifier |
| `on_failure` | Operators, inline or as Operator Cards | A failed verdict is stored, and nobody is notified |

Only two Trigger pairings are valid, and registration rejects anything else with `WYRD_SPEC_400_TRIGGER_ACTIVATION_MISMATCH`:

| Trigger | Runs | Pairs with |
| --- | --- | --- |
| `schedule` (`cron`, optional `tz`, UTC by default) | One run per binding per due window | `drift` |
| `observations_ready` | One run per committed eval observation for the subject | `eval` |

A `task` Verifier takes no Trigger at all. Any Verifier bound without `runs_on`, like `no-refund-promise`, is never run by the server; it is bound to `agent` so your code can call it by name.

## Real time: your code asks, and gets the answer back

Call a bound Verifier from a run view, and the server judges the input inside the same request:

<LangTabs langs={['python', 'rust', 'typescript']}>
<LangTab lang="python">

```python
with state.run("agent") as run:
    answer = run.invoke({"question": question})
    judgment = run.observe.verify("no-refund-promise", {"answer": answer})
    if not judgment.passed:
        answer = hand_to_person(ticket_id)  # your fallback
```

</LangTab>
<LangTab lang="rust">

```rust
let run = state.run_for_card("agent")?;
let mut answer = run.invoke(&[("question", question)]).await?;
let judgment = run.observe().verify("no-refund-promise", &json!({ "answer": answer })).await?;
if !judgment.passed() {
    answer = hand_to_person(ticket_id); // your fallback
}
```

</LangTab>
<LangTab lang="typescript">

```typescript
const run = state.run("agent");
let answer = await run.invoke({ question });
const judgment = await run.observe.verify("no-refund-promise", { answer });
if (!judgment.passed) {
  answer = await handToPerson(ticketId); // your fallback
}
```

</LangTab>
</LangTabs>

The call is synchronous from your code's point of view: it blocks until the verdict comes back, for up to 60 seconds. A `failed` verdict is an ordinary return value, not an exception. The server stores the result in `vala.verification.results`, tied to this run, but it never runs Operators for a real-time call. Your code already has the verdict and decides what to do.

Use real time when the answer should change what your service does next: block a response, fall back to a human, retry with a different prompt. [Verify a request in real time](/build-and-verify/verify/verify-a-request-in-real-time/) has the code in all three SDKs.

## Verifier runtime: the server runs it for you

For a binding with a Trigger, the server does the work. When the Trigger fires, it creates a run, executes the Verifier against the stored observations, writes the result, and on a `failed` verdict starts every Operator in `on_failure`. Your service only records observations; it never waits on a check.

A runtime run has an execution status separate from its verdict. The execution status says whether the check ran; the verdict says what it found.

| Execution status | Means |
| --- | --- |
| `pending`, `running`, `retrying` | Not finished. A run gets 3 attempts, each with a 5-minute limit |
| `completed` | The Verifier ran and produced a verdict, stored as a result |
| `timed_out`, `errored`, `cancelled` | No verdict exists. The run records the error |

Only a `completed` run has a verdict. A run that errored is not a failure of your service; it is a failure to check it, and it never starts Operators.

<WyrdDiagram name="run-lifecycle" />

You can also start a drift run by hand for a specific window, through MCP or HTTP. A manual run of a binding behaves like a scheduled one and runs its Operators on failure; a manual run that names a Verifier and subject directly only analyzes. [Monitor drift](/build-and-verify/verify/monitor-drift/#score-a-past-window-by-hand) shows how.

## Verdicts

| Verdict | Means | Runs Operators |
| --- | --- | --- |
| `passed` | The behavior met the standard | No |
| `failed` | The behavior violated the standard | Yes, for a run of a binding |
| `inconclusive` | No judgment was possible | No |

`inconclusive` is never a pass. The common causes are worth knowing because they look like silence:

- An eval Verifier with no `pass_gate`. Its tasks run, but nothing turns them into a verdict. Always set one.
- Every eval task was skipped, or sampling dropped the record.
- A drift window with too little data to score.

## Results are evidence

Every completed check, real time or runtime, writes one row to `vala.verification.results`. Drift writes one more row per feature to `vala.drift.result_features`; eval writes one row per task to `vala.eval.result_items`.

| Column | Identifies |
| --- | --- |
| `result_id` | This result |
| `card_uid` | The Verifier that judged |
| `verifier_version` | Its exact version |
| `subject_card_uid` | The Card that was judged |
| `owner_card_uid`, `binding_id` | The binding that ran it. Both are null for a real-time call |
| `run_id` | Your application's run: the run that recorded the observation, or the run that called `verify`. Null for a scheduled drift window |
| `source_record_id` | For eval, the `record_id` of the observation it judged |
| `verdict` | `passed`, `failed`, or `inconclusive` |

<WyrdDiagram name="evidence-joins" />

Results are queryable SQL, so "which version was running when this failed, and what did it see?" is one join. [Runs, observations, and retained evidence](/understand/runs-observations-and-evidence/#query-the-evidence) shows the join from observations to verdicts.

## Operators react to failure

An Operator is what happens when a binding's check fails. Each binding's `on_failure` list can hold any number:

| Operator `kind` | Does |
| --- | --- |
| `notify` | Sends a Slack message or a PagerDuty event |
| `http` | Calls a URL you choose: `get`, `post`, `put`, `patch`, or `delete`, with optional bearer, basic, or header auth |

```yaml title="service/page-support-oncall.yaml"
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

Text fields accept `{{...}}` placeholders filled from the result: `verifier_ref`, `subject_ref`, `verdict`, `summary`, `result_id`, `run_id`, `binding_id`, `completed_at`, and `dispatch_id` for any Verifier; `drift.drifted_features` and `drift.total_features` on drift bindings; `eval.passed_tasks`, `eval.failed_tasks`, `eval.total_tasks`, and `eval.pass_rate_percent` on eval bindings. Any other placeholder is rejected at registration.

Secrets never go on the Card. An Operator names an operator connection, which a tenant administrator creates once and which holds the Slack token, PagerDuty key, or HTTP credential. Registration fails if the connection does not exist or is disabled.

Each Operator is delivered on its own, at least once: up to 3 attempts within 5 minutes, retrying network errors, `408`, `429`, and `5xx`. One failing Operator does not stop the others or change the verdict.

## What can go wrong at registration

| Error code | Cause |
| --- | --- |
| `WYRD_SPEC_400_TRIGGER_ACTIVATION_MISMATCH` | The Trigger cannot run that Verifier: `schedule` with `eval`, `observations_ready` with `drift`, or any Trigger on a `task` Verifier |
| `WYRD_SPEC_400_INVALID_VERIFIER_REF_KIND` | `verifier:` points at a Card that is not a Verifier |
| `WYRD_SPEC_400_INVALID_BINDING_REF_KIND` | `runs_on:` is not a Trigger, or an `on_failure` entry is not an Operator |
| `WYRD_SPEC_400_DUPLICATE_VERIFICATION_BINDING` | The same Verifier version is bound twice to one subject |
| `WYRD_SPEC_400_UNSUPPORTED_OPERATOR_ACTION` | An `on_failure` entry is a `workflow` Operator, which the runtime does not run |
| `WYRD_SPEC_400_OPERATOR_CONNECTION_UNAVAILABLE` | The named operator connection is missing or disabled |

Verifiers judge observations, so the next page covers how they get recorded: [Runs, observations, and retained evidence](/understand/runs-observations-and-evidence/).
