---
title: Bind a Verifier
description: Attach a Verifier to a Service or one of its components with verified_by, and choose whether your code or a schedule runs it.
pillar: wyrd
section: Build and verify
topic: Declare
order: 2
status: runnable
---

A Verifier on its own checks nothing. You attach it to what it checks with a `verified_by` binding on the Service, on one component, or on an Agent Card. The binding also decides who runs it: your code, or a schedule.

## Bind a Verifier your code calls

This is the support desk's full Service. It binds one Verifier to the whole Service and two to the agent. Switch tabs to see each Card a binding points at:

<Tabs>
<Tab label="support-desk.yaml">

<CodeFromFile file="examples/support-desk-app/service/support-desk.yaml" lang="yaml" title="support-desk.yaml" />

</Tab>
<Tab label="no-refund-promise.yaml">

<CodeFromFile file="examples/support-desk-app/service/no-refund-promise.yaml" lang="yaml" title="no-refund-promise.yaml" />

</Tab>
<Tab label="ticket-complete.yaml">

<CodeFromFile file="examples/support-desk-app/service/ticket-complete.yaml" lang="yaml" title="ticket-complete.yaml" />

</Tab>
<Tab label="follows-policy.yaml">

<CodeFromFile file="examples/support-desk-app/service/follows-policy.yaml" lang="yaml" title="follows-policy.yaml" />

</Tab>
<Tab label="support-agent.yaml">

<CodeFromFile file="examples/support-desk-app/service/support-agent.yaml" lang="yaml" title="support-agent.yaml" />

</Tab>
</Tabs>

| Bound on | Verifier | Applies to |
| --- | --- | --- |
| The Service's `verified_by` | `no-refund-promise` | Every Run of the Service, as `run.observe.verify(...)` |
| The `agent` component's `verified_by` | `ticket-complete`, `follows-policy` | The agent's view of a Run, as `run.for_card("agent").observe.verify(...)` |

None of these bindings has `runs_on`, so nothing runs them on its own. Your code calls `verify` or `submit` with the evidence; [Verify inside the run](/build-and-verify/observe/record-a-run-and-observations/#3-record-verify-and-submit) shows both. That is the right default for anything that judges one request.

## Bind a Verifier to a schedule

A schedule judges samples your service recorded over time, such as request latency, instead of one request. Add `runs_on` with a Trigger, `inputs` to say which recorded features fill the Verifier's `samples` slot, and optionally `on_failure` with the Operators to run when it fails:

<Tabs>
<Tab label="latency-watch.yaml">

<CodeFromFile file="fixtures/cards/scheduled_drift_alerts_operator/latency-watch.yaml" lang="yaml" title="latency-watch.yaml" />

</Tab>
<Tab label="latency-shift.yaml">

<CodeFromFile file="fixtures/cards/scheduled_drift_alerts_operator/latency-shift.yaml" lang="yaml" title="latency-shift.yaml" />

</Tab>
<Tab label="daily.yaml">

<CodeFromFile file="fixtures/cards/scheduled_drift_alerts_operator/daily.yaml" lang="yaml" title="daily.yaml" />

</Tab>
<Tab label="alert-on-call.yaml">

<CodeFromFile file="fixtures/cards/scheduled_drift_alerts_operator/alert-on-call.yaml" lang="yaml" title="alert-on-call.yaml" />

</Tab>
</Tabs>

Every day at midnight UTC, the server compares the day's `latency` samples with the baseline and, on a `failed` verdict, posts to the on-call hook. [Monitor drift](/build-and-verify/verify/monitor-drift/) walks through recording the samples.

## Binding fields

| Field | Required | Holds |
| --- | --- | --- |
| `verifier` | Yes | A path to the Verifier's Card file, or a declared Verifier |
| `runs_on` | No | A Trigger, as a path, a declared Card, or inline. Absent, only your code runs the Verifier |
| `inputs` | No | For a scheduled Verifier, each `samples` slot's recorded `features` |
| `on_failure` | No | Operators to run after a `failed` verdict, as paths, declared Cards, or inline |

A Trigger is `{kind: schedule, cron: "<cron>", tz: <zone>}`, with `tz` defaulting to UTC. Schedules only judge recorded samples: binding `runs_on` to a Verifier with other inputs fails declaration. A binding has no other fields, and an unknown field such as `when` is rejected.

## If it fails

| You see | Cause | Fix |
| --- | --- | --- |
| `WYRD_SDK_404_UNKNOWN_VERIFIER` from `verify` or `submit` | The Verifier isn't bound to the view you called it on | Call it on the view whose Card binds it, or add the binding |
| `WYRD_SPEC_400_DUPLICATE_VERIFICATION_BINDING` | The same Verifier is bound twice on one subject | Remove one |
| `WYRD_SPEC_400_TRIGGER_ACTIVATION_MISMATCH` | `runs_on` is set on a Verifier that doesn't judge `samples` | Remove `runs_on` and call the Verifier from code |
| `WYRD_PERMISSION_403_DENIED_RBAC` at declaration | A binding has `on_failure`, and your credential lacks `operators:invoke` | Declare with a credential that has it |

## Next

[Declare and inspect the Service](/build-and-verify/declare/declare-and-inspect/). To write your own Verifier, see [Author a Verifier](/build-and-verify/verify/author-a-verifier/).
