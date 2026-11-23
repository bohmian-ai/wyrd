---
title: Failures
description: Page someone when a scheduled Verifier fails, with an Operator Card, an operator connection, and on_failure, then check that the page was delivered.
pillar: wyrd
section: Build and verify
topic: Verify
order: 5
draft: true
---

An Operator is what Wyrd does when a Verifier fails: post to a webhook, page PagerDuty, or message Slack. You attach it to a binding with `on_failure`, and the server runs it once per failed execution the binding creates. This page pages an on-call webhook when a minute of checkout requests is too slow.

The server runs Operators only for executions it starts itself, such as a schedule's. A `failed` verdict from `verify` or `submit` never runs them, and neither do `passed`, `inconclusive`, or execution errors.

Start from a server you set up with `wyrd init`, and an empty directory holding the files below.

## 1. Store the webhook's connection

Secrets never go in a Card. A tenant administrator stores the webhook's origin and token once, as an operator connection:

```bash
wyrd operator-connection create --body-file - <<JSON
{"provider": "http", "name": "on-call-hooks", "origin": "https://hooks.example.com", "auth": {"scheme": "bearer", "token": "$ON_CALL_TOKEN"}}
JSON
```

The command prints the connection with its token redacted. The server encrypts the token before storing it, with the key `wyrd init` keeps.

## 2. Write the Operator and bind it

The Operator names the connection and a path on its origin. The Service binds `latency-budget` to an every-minute schedule and pages on-call when a minute fails:

<Tabs>
<Tab label="checkout-alerts.yaml">

<CodeFromFile file="examples/respond-to-a-failed-verification/checkout-alerts.yaml" lang="yaml" title="checkout-alerts.yaml" />

</Tab>
<Tab label="page-on-call.yaml">

<CodeFromFile file="examples/respond-to-a-failed-verification/page-on-call.yaml" lang="yaml" title="page-on-call.yaml" />

</Tab>
<Tab label="latency-budget.yaml">

<CodeFromFile file="examples/respond-to-a-failed-verification/latency-budget.yaml" lang="yaml" title="latency-budget.yaml" />

</Tab>
<Tab label="every-minute.yaml">

<CodeFromFile file="examples/respond-to-a-failed-verification/every-minute.yaml" lang="yaml" title="every-minute.yaml" />

</Tab>
</Tabs>

When a minute fails, the server posts the `body` to `https://hooks.example.com/hooks/checkout-latency` with `Authorization: Bearer <token>`. The `{{...}}` placeholders are filled from the failed execution:

| Placeholder | Value |
| --- | --- |
| `execution_id`, `result_id`, `dispatch_id`, `binding_id` | IDs to look the failure up by |
| `verifier_ref`, `verifier_card_uid` | The Verifier that failed |
| `subject_ref`, `subject_uid` | The Card it judged |
| `verdict`, `completed_at`, `summary` | What happened and when |
| `drift.drifted_features`, `drift.total_features` | For drift checks, how many features drifted |

The Operator's `auth.scheme` must match the connection's, and `url` is a path on the connection's origin, or a full URL with that same origin. Declaration refuses an Operator whose connection is missing, disabled, or does not match, and declaring a binding with `on_failure` needs the `operators:invoke` scope.

Slack and PagerDuty use `kind: notify` with a `channel` of `{kind: slack, connection, channel_id, text}` or `{kind: pager_duty, connection, ...}`, against a `slack` or `pager_duty` connection.

## 3. Declare the Service and create its account

Declare and pull the Service, then create the account its schedule runs as:

```bash
wyrd declare checkout-alerts.yaml
wyrd pull --kind Service --name checkout-alerts --version 1.0.0 --output-dir .wyrd-bundle
wyrd principal credential create-principal --name checkout-runtime --role workload --workload-name checkout-alerts
export CHECKOUT_KEY=<credential>
```

## 4. Record a slow minute

Save `record` from [Monitor drift](/build-and-verify/verify/monitor-drift/#3-write-the-programs) in the same directory, and `dispatches`, which waits for the failed minute and prints how each Operator run was delivered:

<LangTabs langs={['python', 'rust', 'typescript']}>
<LangTab lang="python">

<CodeFromFile file="examples/respond-to-a-failed-verification/dispatches.py" lang="python" title="dispatches.py" />

```bash
WYRD_API_KEY=$CHECKOUT_KEY uv run record.py 250
uv run dispatches.py
```

</LangTab>
<LangTab lang="rust">

<CodeFromFile file="examples/respond-to-a-failed-verification/dispatches.rs" lang="rust" title="src/bin/dispatches.rs" />

```bash
WYRD_API_KEY=$CHECKOUT_KEY cargo run --bin record 250
cargo run --bin dispatches
```

</LangTab>
<LangTab lang="typescript">

<CodeFromFile file="examples/respond-to-a-failed-verification/dispatches.ts" lang="typescript" title="dispatches.ts" />

```bash
WYRD_API_KEY=$CHECKOUT_KEY node record.ts 250
node dispatches.ts
```

</LangTab>
</LangTabs>

Within about a minute, `dispatches` prints:

```text
verdict: failed
dispatch: delivered
```

and your webhook receives the Operator's body. A dispatch is `pending`, `running`, `retrying`, `delivered`, or `failed`. Each one is delivered and retried on its own, and a delivery failure never changes the verdict. After a restart of `wyrd init`, the same connection keeps delivering.

## If it fails

| You see | Fix |
| --- | --- |
| `WYRD_SPEC_400_OPERATOR_CONNECTION_UNAVAILABLE` at declaration | Create the connection first, and match the Operator's `auth.scheme` and origin |
| `no failed window was judged` | Record with `$CHECKOUT_KEY` at least 10 seconds before the minute ends, then run `dispatches` again |
| `dispatch: failed` | The webhook refused the request or was unreachable after retries; fix the receiver and check the connection's token |

## Next

[Act on verdicts in production](/build-and-verify/verify/act-on-verdicts-in-production/) covers the other ways to react to a verdict.
