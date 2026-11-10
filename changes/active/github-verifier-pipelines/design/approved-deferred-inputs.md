# Approved deferred inputs and existing storage owners

Status: approved design direction; not an implementation-ready specification.

The user approved points 2 and 3 of the concrete revised proposal. Report
selection and the proposed binding-level github.report field were rejected at
that point. The later [report-delivery agreement](approved-report-delivery.md)
selects the single-envelope convention without either selector.

## Deferred evidence inputs

CI providers upload evidence to Wyrd. No GitHub Actions artifact dependency is
required. Extend declared input slots with deferred: true, for example:

```yaml
inputs:
  ci_results:
    type: media
    formats: [json]
    deferred: true
```

- A PR starts its queued execution. Independent steps run; steps consuming the
  deferred slot remain pending until its value is accepted.
- CI publishes evidence through Wyrd's CLI or SDK. Wyrd matches it to declared
  subscriptions, supplies matching deferred inputs and queues continuation.
  CI does not query pending executions or address execution IDs. This explicit
  correction supersedes execution.provide_input and execution discovery.
- Reuse Wyrd upload/storage for bytes and existing canonical evidence and queue
  owners for acceptance/resumption. Pub/sub describes this evidence flow; no new
  broker, general event bus or PostgreSQL table is selected. The approved matching
  and subscription lifecycle below own publication behavior. Exact wire schemas,
  SDK signatures, storage representation and recovery still require specification
  and source validation.
- Accept inputs only for explicitly deferred slots. A value may arrive before
  its consuming step starts; no arbitrary external signal buffer is introduced.
- Validate tenant permission, slot type and exact reviewed commit. Accept once:
  identical retries succeed, conflicting replacement fails.
- Cancelled, superseded or terminal executions reject new input. Missing evidence
  expires under the original execution deadline; absence never means a pass.
- Durably retain the accepted input and resume through the existing queue, with
  completed steps preserved. A fully blocked execution releases capacity.
- Preserve a separate human-approval action. Do not add generic timer/external
  wait actions. Approval and deferred inputs reuse park/resume behavior.

This supersedes the initial scope of three generic durable wait modes. Previously
approved budget/deadline preservation, checkpoint-before-release, queue-based
resumption, independent branches and human approval security still apply.
The earlier rule rejecting early external wait signals does not prohibit supplying
a declared deferred slot before its consumer starts.

## Approved publication matching and subscription lifecycle

The user explicitly approved the corrected proposal: do not repurpose group_id,
require a publisher idempotency key or retain unmatched publications for future
subscriptions. These supersede those earlier assistant proposals.

### Authoring and dependency

deferred: true subscribes the named input. Ordinary references establish which
steps need it; no extra depends_on, wait action or subscription object is needed.
For example, the slot above can feed:

```yaml
steps:
  - id: read-ci
    action:
      kind: extract
      from: inputs.ci_results
```

read-ci remains pending until its input exists; downstream steps can consume
steps.read-ci.output.value for JSON. Independent steps continue normally.
Declaration and activation of the binding establish the subscription; an active
execution is a consumer, not the subscription's existence condition.

### Matching and publisher arguments

Within the authenticated tenant, match stable repository identity, commit tag
and input name. The input name distinguishes different evidence for the same source.
The server derives a PR execution's commit from its pinned PR head. CLI/SDK infer
the repository from the checkout's Git remote and commit from Git; allow an
explicit override when CI checks out a different revision. Outside a checkout,
the publisher must supply repository and commit identity. Evidence is about
source, not the Repository Card version; policy changes do not require CI to
select a different Card version. Executions keep their exact policy pins. This is proposed new
inference behavior, not a claim that today's publisher implements it.

Approved normal CLI shape:

```shell
wyrd evidence publish ci_results --file results.json
```

The input name selects the subscription and the file supplies evidence. Do not
require CI to query Wyrd, identify a Verifier/execution, choose group_id or invent
a retry key. Do not introduce arbitrary subscription-filter expressions.

For the same matching identity, identical canonical JSON or completed File digest
acknowledges the original acceptance; different content cannot replace an accepted
input. Server-derived deduplication needs no CI job ID or caller retry key.

### Delivery cases

| Situation | Approved behavior |
|---|---|
| Matching execution waiting | Accept input and queue continuation of the same execution |
| Matching execution running or queued | Accept deferred input; normal scheduling releases its consumers |
| Active subscription, no execution started | Retain evidence for that subscription; a later matching execution may consume it; publication alone never starts a run |
| No active matching subscription | Explicit no-matching-subscription result; do not retain evidence for hypothetical future subscribers or silently report delivery success |
| Completed, cancelled or superseded execution | Do not reopen or supply new input to that execution |
| Subscription created after an unmatched publication | No historical replay; publisher must publish again |

Check retained matching evidence when initializing an execution, as well as
responding to new acceptance notifications. Notifications alone are not durable
recovery. Subscription evidence follows normal evidence retention, not indefinite
storage. Retention for existing subscriptions while consumers are offline is
distinct from retaining evidence before any subscription exists.

Primary references consulted: [Google Pub/Sub subscription semantics](https://docs.cloud.google.com/pubsub/docs/subscription-overview)
and [RabbitMQ publisher routing](https://www.rabbitmq.com/docs/publishers).
These support the distinction between durable subscriptions and online consumers;
they do not imply adopting either broker. Current execution_inputs requires an
execution and Verifier identity; the exact representation of accepted evidence
for an existing subscription without a started execution remains owner-level
design work. Do not invent fake executions or claim existing storage suffices
without verification. No new PostgreSQL tables are authorized.

## No new PostgreSQL tables

This is an explicit user constraint, not merely a preference.

| Concern | Existing owner |
|---|---|
| Uploaded bytes | Wyrd storage |
| Accepted inputs, approval decisions, checkpoints, reports | Bifrost verification records |
| Admission, parked/runnable coordination, claims, cancellation | Verification queue tables |
| Writes to Bifrost | Scribe outbox |
| Queue admission and resumption | SubmissionSink/outbox |

Extend these owners for deferred-input acceptance, approval, park/resume and
GitHub delivery tracking. No additional outbox, event bus or payload store.
Existing terminal delivery records outcomes in Bifrost; GitHub publishing needs
provider integration and recoverable progress, not a replacement result path.
Exact extensions and restart-safe handoffs remain to specify and verify.

## Approval and deadlines

The user approved a dedicated approval action with message and expiry, returning
approved/rejected/expired with server-derived actor and time. Reuse existing
authentication plus a separate approval permission; no quorum, approval-links
service or generic signal API. The user's later correction removes the earlier
recent-interactive-login restriction: normal authentication and authorization
are sufficient. No authentication-age threshold or special approval proof.

Use the original execution deadline for missing evidence and approval; neither
extends it. Expose the execution ceiling through normal operator configuration
so intended human approval periods are usable. Exact nested authoring/API schemas
remain to specify, not another decision about adding generic waits.

## Subsequent simplification and audit

The user approved repository + commit + input matching without publisher-facing
Card-version selection. Existing subscribers can receive evidence; absence of a
subscriber is explicit; newly added subscribers do not automatically receive
historical evidence. Do not add public rollback/reactivation modes or a separate
subscription-management protocol. Preserve exact Card/Verifier versions in
execution lineage, not the evidence routing key.

The user requested a full spec complexity audit. Its corrections are recorded in
[research](../research.md#complexity-audit--revision-3); the specification remains
revision 3 draft, not approved or implementation-tested.
