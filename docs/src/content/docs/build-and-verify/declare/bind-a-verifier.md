---
title: Bindings
description: Attach a Verifier to a Service or one of its components with verified_by, and choose whether your code or a schedule runs it.
pillar: wyrd
section: Build and verify
topic: Declare
order: 2
draft: true
status: runnable
---

Attach each Verifier to what it checks, and choose who runs it: your code, or a schedule.

This page uses the `support-desk` Service from [Services](/build-and-verify/declare/define-a-service/). The Verifiers it binds are written in [Write a Verifier](/build-and-verify/verify/author-a-verifier/).

## 1. Bind Verifiers your code calls

Add a `verified_by` list to the Service, or to one of its components. Each entry names a Verifier file:

<CodeFromFile file="examples/support-desk-app/service/support-desk.yaml" lang="yaml" title="support-desk.yaml" />

Where you bind a Verifier decides where your code calls it:

| Bound on | Call it as |
| --- | --- |
| The Service | `run.observe.verify("no-refund-promise", ...)` |
| The `agent` component | `run.for_card("agent").observe.verify("ticket-complete", ...)` |

Check the bindings:

```bash
wyrd declare --check support-desk.yaml
```

It lists the Service, its components, and each bound Verifier. A Verifier with no `runs_on` runs only when your code calls `verify` or `submit`, as in [Runs and observations](/build-and-verify/observe/record-a-run-and-observations/).

## 2. Run a Verifier on a schedule

A schedule judges samples your service recorded over time, such as request latency, instead of one request. Add `runs_on` with a Trigger:

```yaml title="latency-watch.yaml"
apiVersion: wyrd/v1
kind: Service
metadata:
  name: latency-watch
  version: 1.0.0
  space: default
spec:
  verified_by:
    - verifier: ./latency-shift.yaml
      runs_on: {kind: schedule, cron: "0 0 * * *"}  # midnight; add tz to use another zone than UTC
```

Only a Verifier whose inputs are `samples` can run on a schedule. [Drift](/build-and-verify/verify/monitor-drift/) shows how to write one and record its samples.

## 3. React to a failure

Add `on_failure` with the Operators to run after a `failed` verdict:

<Tabs>
<Tab label="latency-watch.yaml">

<CodeFromFile file="fixtures/cards/scheduled_drift_alerts_operator/latency-watch.yaml" lang="yaml" title="latency-watch.yaml" />

</Tab>
<Tab label="daily.yaml">

<CodeFromFile file="fixtures/cards/scheduled_drift_alerts_operator/daily.yaml" lang="yaml" title="daily.yaml" />

</Tab>
<Tab label="alert-on-call.yaml">

<CodeFromFile file="fixtures/cards/scheduled_drift_alerts_operator/alert-on-call.yaml" lang="yaml" title="alert-on-call.yaml" />

</Tab>
</Tabs>

The Trigger now lives in its own file, so several bindings can share it. Each day at midnight UTC, the server judges the day's `latency` samples and, on a `failed` verdict, posts to the on-call hook. Declaring an `on_failure` binding needs the `operators:invoke` permission.

<Aside type="note" title="Judge a whole conversation">

A `group_end` Trigger judges every turn sharing a `group_id` once the conversation goes quiet, and `on_verdict_change` runs Operators when that verdict changes. See the [grouped recipe](/build-and-verify/verify/verifier-recipes/#grouped-session).

</Aside>

## Binding fields

| Field | Required | Holds |
| --- | --- | --- |
| `verifier` | Yes | A path to the Verifier's file, or a declared Verifier |
| `runs_on` | No | A `schedule` or `group_end` Trigger, as a path, a declared Trigger, or inline. Absent, only your code runs the Verifier |
| `on_failure` | No | Operators to run after a `failed` verdict. Not allowed with `group_end` |
| `on_verdict_change` | No | Operators to run when a `group_end` verdict changes |

Any other field is rejected.

## If it fails

| You see | Fix |
| --- | --- |
| `WYRD_SDK_404_UNKNOWN_VERIFIER` from `verify` or `submit` | Call it on the Service or component that binds it, or add the binding |
| `WYRD_SPEC_400_DUPLICATE_VERIFICATION_BINDING` | The same Verifier is bound twice on one subject: remove one |
| `WYRD_SPEC_400_TRIGGER_ACTIVATION_MISMATCH` | A schedule is set on a Verifier that doesn't judge `samples`: remove `runs_on` and call it from code |
| `WYRD_PERMISSION_403_DENIED_RBAC` when you declare | Declare `on_failure` with a credential that has `operators:invoke` |

## Next

[Declare the Service](/build-and-verify/declare/define-a-service/#4-declare-it), then [verify inside a Run](/build-and-verify/observe/record-a-run-and-observations/).
