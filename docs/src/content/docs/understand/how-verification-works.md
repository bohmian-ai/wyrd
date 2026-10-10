---
title: How verification works
description: How bindings, Triggers, Verifiers, and Operators turn a Service declaration into verdicts.
pillar: wyrd
section: Understand
order: 2
draft: true
---

To check that a component behaves as expected, you bind a Verifier to it with `verified_by`. Each binding names three things:

- the **Verifier**, a registered Verifier Card that judges the behavior;
- the **Trigger**, which decides when the Verifier runs: on a `schedule`, or with `observations_ready` as each observation is recorded. Leave the Trigger out and the Verifier runs only when your code calls it, which is how a Verifier gives real-time feedback while a request is in flight;
- the **Operators**, which run when a triggered check fails.

Bindings live in the Service declaration. Changing what gets verified changes the declaration, so the verification setup is versioned along with the service it checks.

The examples continue the `support-copilot` service from [Cards, versions, and relationships](/understand/cards-versions-and-relationships/).

## A binding in the Service declaration

Here the churn model is checked for input drift every hour, the triage agent is evaluated on every answer it records, and the agent's tool calls can be checked in real time before an answer goes out:

```yaml title="service.yaml"
apiVersion: wyrd/v1
kind: Service
metadata:
  space: retention
  name: support-copilot
spec:
  components:
    - alias: churn_model
      ref: { kind: Model, name: churn-classifier, version: 1.4.2 }
      verified_by:
        - verifier: ./verifiers/churn-input-drift.yaml
          runs_on: { kind: schedule, cron: "0 * * * *" }
          on_failure:
            - kind: notify
              channel:
                kind: slack
                connection: ops-slack
                channel_id: C0123456789
                text: "Churn model input drift on {{subject_ref}}"
    - alias: triage
      ref: ./agents/triage.yaml
      verified_by:
        - verifier: ./verifiers/triage-quality.yaml
          runs_on: { kind: observations_ready }
          on_failure:
            - ./operators/page-support-oncall.yaml
        - verifier: ./verifiers/allowed-tools.yaml
```

The subject of a binding is the component it sits under. The same Verifier, Trigger, and Operator Cards can be reused by any number of bindings, on any number of services. You can also put `verified_by` on the Service itself, or on a standalone Agent.

Every binding resolves to exact versions when the Service is registered, like any other reference. Wyrd rejects a binding that points at the wrong kind of Card, pairs a Trigger with a Verifier it cannot run, or binds the same Verifier version twice to one subject.

## Verifiers

A Verifier judges behavior against a standard and records the verdict with what was checked and which standard applied. A Verifier has one of three implementations:

| Implementation | Runs when | Checks | Built for |
| --- | --- | --- | --- |
| `drift` | On a `schedule` Trigger | A measured signal, such as a feature distribution, against a baseline | Models and data over time |
| `eval` | With `observations_ready`, once for each eval observation | That observation against a set of tasks and a pass gate | Agent behavior |
| `task` | Only when your code calls it, inside the request | One assertion or LLM judgment on the context you pass | Real-time checks before you act on a result |

A Verifier never names its subject. That is why one Verifier can check many components: the binding supplies the subject, and the observations carry it.

### Drift: a signal against a standard

A drift Verifier compares a measured signal against a standard. This one compares the churn model's live inputs against the training set, feature by feature, using the population stability index (PSI):

```yaml title="verifiers/churn-input-drift.yaml"
apiVersion: wyrd/v1
kind: Verifier
metadata:
  space: retention
  name: churn-input-drift
  version: 1.0.0
spec:
  description: Churn model input distribution drift.
  implementation:
    kind: drift
    spec:
      method: Psi
      signal:
        kind: Distribution
        baseline_ref: { kind: Data, name: churn-training, version: 1.4.2 }
        features: [tenure_months, monthly_spend]
      condition: { kind: Statistical }
      profile:
        kind: Psi
        binning_strategy: { kind: Quantile, n_bins: 10 }
        threshold: { kind: Fixed, value: 0.25 }
```

| Field | Options |
| --- | --- |
| `method` | `Psi`, `Spc` (statistical process control), or `Custom` |
| `signal` | `Distribution` (features against a baseline `Data` Card) for `Psi` and `Spc`; `Metric` (one named number, such as `p99_latency_ms`) for `Custom` |
| `condition` | `Statistical`: the method's profile decides what counts as drift |
| `profile` | The settings for `method`, such as PSI bins and threshold |

A metric signal uses the `Custom` method. This one fails when mean 99th-percentile latency moves more than 400 ms from its 800 ms baseline:

```yaml
method: Custom
signal: { kind: Metric, name: p99_latency_ms }
condition: { kind: Statistical }
profile: { kind: Custom, metric_name: p99_latency_ms, baseline_value: 800, alert_threshold: 400 }
```

### Eval: each observation against tasks

An eval Verifier is built mainly for agentic evaluation. It judges each eval observation as it arrives: tasks are checks on the observation's context, and the pass gate turns task results into one verdict.

```yaml title="verifiers/triage-quality.yaml"
apiVersion: wyrd/v1
kind: Verifier
metadata:
  space: retention
  name: triage-quality
  version: 1.0.0
spec:
  implementation:
    kind: eval
    spec:
      pass_gate: { kind: all_pass }
      tasks:
        lookup_called:
          kind: assertion
          id: lookup_called
          context_path: $.tool_call_names
          operator: contains
          expected: customer_lookup
        answer_is_grounded:
          kind: llm_judge
          id: answer_is_grounded
          depends_on: [lookup_called]
          judge_ref: { kind: Agent, name: groundedness-judge, version: 1.0.0 }
          operator: equals
          expected: true
```

| Task kind | Checks |
| --- | --- |
| `assertion` | A value in the observation, compared with an operator such as `contains`, `equals`, `is_subset`, or `is_json` |
| `llm_judge` | A judgment from an Agent that answers in a structured format |
| `trace_assertion` | The agent's recorded trace, such as which spans ran |
| `agent_assertion` | A field of the agent's workflow record |

Each task's key must match its `id`. Use `depends_on` to order tasks. Here the judge runs only after the cheaper assertion. Prefer an assertion whenever the check is deterministic, such as "the agent only used allowed tools"; it is faster, free, and gives the same answer every time.

### Task: one check, in real time

A task Verifier is one check that your code calls directly, while the request is still in flight. Use it when the answer should change what your service does next, such as blocking a response that called a tool it should not have. It holds a single `assertion` or `llm_judge`, with the same fields as an eval task:

```yaml title="verifiers/allowed-tools.yaml"
apiVersion: wyrd/v1
kind: Verifier
metadata:
  space: retention
  name: allowed-tools
  version: 1.0.0
spec:
  implementation:
    kind: task
    spec:
      kind: assertion
      id: allowed_tools
      context_path: $.tool_call_names
      operator: is_subset
      expected: [customer_lookup, refund_status]
```

A task Verifier takes no Trigger; a binding that gives it `runs_on` is rejected. Call it by name from the component's run view, and the server returns the judgment in the same request:

<LangTabs langs={['python', 'rust', 'typescript']}>
<LangTab lang="python">

```python
judgment = run.for_card("triage").observe.verify(
    "allowed-tools", {"tool_call_names": tool_names}
)
if not judgment.passed:
    answer = escalate(ticket)  # your fallback
```

</LangTab>
<LangTab lang="rust">

```rust
let judgment = run
    .for_card("triage")?
    .observe()
    .verify("allowed-tools", &json!({ "tool_call_names": tool_names }))
    .await?;
if !judgment.passed() {
    answer = escalate(&ticket); // your fallback
}
```

</LangTab>
<LangTab lang="typescript">

```typescript
const judgment = await run
  .forCard("triage")
  .observe.verify("allowed-tools", { tool_call_names: toolNames });
if (!judgment.passed) {
  answer = escalate(ticket); // your fallback
}
```

</LangTab>
</LangTabs>

A failed judgment is an ordinary return value, not an error. The caller needs permission to run that Verifier, and the server stops waiting after 60 seconds. You can call an `eval` or `drift` Verifier the same way, passing an eval context or drift samples.

## Verdicts

Every Verifier run ends in one of three verdicts:

| Verdict | Means | Runs Operators |
| --- | --- | --- |
| `passed` | The behavior met the standard | No |
| `failed` | The behavior violated the standard | Yes, every Operator in `on_failure`, when a Trigger started the run |
| `inconclusive` | No judgment could be made, such as no observations in the window | No |

A run your code starts with `verify` never runs Operators; your code already has the verdict and decides what to do with it. An inconclusive verdict is never a pass. If a drift window keeps coming back inconclusive, the service is not recording what the Verifier needs.

Verdicts are stored in Bifrost, in `vala.verification.results`, and you query them like any other table:

<LangTabs langs={['python', 'rust', 'typescript']}>
<LangTab lang="python">

```python
from wyrd.bifrost import Bifrost

bifrost = Bifrost()
failed = bifrost.sql("""
    SELECT verdict, implementation, subject_card_uid, verifier_version, started_at
    FROM vala.verification.results
    WHERE verdict = 'failed'
    ORDER BY started_at DESC
    LIMIT 20
""")
print(failed.to_pandas())
```

</LangTab>
<LangTab lang="rust">

```rust
use wyrd_sdk::{Bifrost, WyrdClient};

let bifrost = Bifrost::connect(&WyrdClient::from_global()?).await?;
let failed = bifrost
    .sql(
        "SELECT verdict, implementation, subject_card_uid, verifier_version, started_at \
         FROM vala.verification.results \
         WHERE verdict = 'failed' \
         ORDER BY started_at DESC \
         LIMIT 20",
        &[],
    )
    .await?;
```

</LangTab>
<LangTab lang="typescript">

```typescript
import { Bifrost } from "@wyrd/sdk";

const bifrost = await Bifrost.connect();
const failed = await bifrost.sql(`
  SELECT verdict, implementation, subject_card_uid, verifier_version, started_at
  FROM vala.verification.results
  WHERE verdict = 'failed'
  ORDER BY started_at DESC
  LIMIT 20
`);
```

</LangTab>
</LangTabs>

## Operators

A failed verdict from a triggered run runs the binding's Operators: a notification or an HTTP call. A passing verdict runs nothing.

| Operator `kind` | Does |
| --- | --- |
| `notify` | Sends a Slack message or a PagerDuty event |
| `http` | Calls a URL you choose |

An Operator can be a registered Card or written inline in `on_failure`; both use the same fields. A reusable Operator Card:

```yaml title="operators/page-support-oncall.yaml"
apiVersion: wyrd/v1
kind: Operator
metadata:
  space: retention
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

Text fields accept `{{...}}` placeholders that Wyrd fills from the failed verdict, such as `verifier_ref`, `subject_ref`, `verdict`, `result_id`, and `summary`. Any other placeholder is rejected when the Card is registered.

Secrets never go on the Card. An Operator names an operator connection, which an administrator creates once for your tenant and which holds the Slack token, PagerDuty key, or HTTP credential. Registration fails if the named connection does not exist.

```yaml
kind: http
method: post
url: https://tickets.example.com/api/incidents
auth: { scheme: bearer, connection: tickets-api }
body:
  title: "{{verifier_ref}} failed"
  result_id: "{{result_id}}"
```

Each Operator runs on its own, with its own retries. One failing Operator does not stop the others or change the verdict.

## What can go wrong

| Error code | Cause |
| --- | --- |
| `WYRD_SPEC_400_TRIGGER_ACTIVATION_MISMATCH` | The Trigger cannot run that Verifier, such as `schedule` with an `eval` Verifier, or any Trigger on a `task` Verifier |
| `WYRD_SPEC_400_INVALID_VERIFIER_REF_KIND` | `verifier:` points at a Card that is not a Verifier |
| `WYRD_SPEC_400_DUPLICATE_VERIFICATION_BINDING` | The same Verifier version is bound twice to one subject |
| `WYRD_SPEC_400_UNSUPPORTED_OPERATOR_ACTION` | An `on_failure` entry uses an action the server cannot run, such as a `workflow` Operator |

Verifiers judge observations, so the next page covers how they get recorded: [Runs, observations, and retained evidence](/understand/runs-observations-and-evidence/).
