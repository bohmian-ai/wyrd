# Owner decisions for example-journey-fixes

Decisions the human owner made on 2026-10-14 while the tasks were in flight.
They are binding alongside `spec.md`. Every task review and the final change
review checks the candidate against each one. Where `spec.md` text disagrees
with a decision here, this record wins until `spec.md` is revised to match.

## OD-1 Attribution: actor and origin

Owner: "Anything that kicks off internally should be system", and internal
execution is connected back to its originating state where possible.

- The actor (`requested_by_principal_id`) is:
  - the authenticated caller for manual runs and direct executions;
  - the tenant SYSTEM principal for every internally started run, both
    scheduled and observation-triggered.
- A real principal is never recorded as the actor of internal work. Recording
  the observation writer as the requester of an observation-triggered run is
  false attribution, and the owner classed it as a security violation.
- Origin is recorded as references to the originating state, never as copied
  principal columns:
  - `run_id`
  - subject `card_uid`
  - `source_record_id`
  - binding, owner Card and trigger

  The originating principal is reached through `source_record_id`, because
  the observation row holds its writer, run, and trace.
- An observation-triggered verification adds an OpenTelemetry span link (not
  a parent) to the observation's stored `trace_id`/`span_id`, when present.
- A requester is required for every origin only if every tenant is guaranteed
  an active SYSTEM principal. Otherwise only manual runs require one.
- Open, not yet decided: whether observation-triggered judge model calls run
  as SYSTEM instead of the observation writer (`writer_caller`). The authority
  stays unchanged until the owner decides, based on evidence about
  permissions and spend.

## OD-2 Gateway attribution is optional context

- A gateway call requires only the principal. The gateway is a standalone
  product.
- `wyrd-run-id` and `wyrd-card-uid` are each optional and independent. When
  present they are recorded. When absent they are NULL.
- A judge call records the subject Card the verification was declared
  against, and the Run when one was supplied.
- No binding enforcement on direct verification. Any principal with
  `verifier:run` and Card scope may execute a Verifier against a subject. The
  current rule is correct.

## OD-3 Trace context

- Every client request path carries the caller's W3C trace context:
  - the shared HTTP builder;
  - the MCP client's `authenticated_replay` path;
  - gRPC ingest metadata;
  - Python and TypeScript at their shared native boundary, not only on
    `verify`.
- Trace context is observability linkage only. It never authorizes or
  attributes anything.

## OD-4 Jev (spec r2, approved by the owner: "I approve jev r2")

- Jev works through both routes, like every provider:
  - direct, at `/v1/systemone`;
  - unified, at `/v1/chat/completions`.
- The unified route is a pass-through:
  - The last user message's text is the typed Jev request
    (`{state, questions}`). If it does not deserialize, the call fails with
    one typed bad-request error.
  - The Jev answer is returned as JSON in `choices[0].message.content`, with
    usage mapped.
  - There is no schema mapping and no per-field validation.
- An upstream 401 behaves like the other providers. The status and body are
  relayed with the credential removed, and the call is not retried.
- INV-004 is relaxed to allow this unified route.
- A Jev request reaching the Agent loop fails with a typed error. There is no
  silent OpenAI-chat fallback.

## OD-5 Managed-secret sealing (replaces the original TASK-005)

- Gateway `managed_secret` credentials are sealed with the existing
  deployment sealing keyring. The separate per-tenant gateway keyring and its
  configuration are deleted, with no duplicated configuration lookup.
- `wyrd server dev` ensures that a sealing key exists, with no invented
  restart.
- Stale code from the rejected design does not remain.

## OD-6 Bifrost subquery correctness bar

- Representative analytical queries with several subqueries and joins, over
  seeded data with nulls, ties, duplicates, and orphans.
- Results are checked against a plain DataFusion oracle, with some answers
  computed by hand.
- Coverage spans the published, live, mixed, and remote-Scribe states.

## OD-7 Process

- No bandaid guards. Fix root causes.
- Do not invent complexity the owner did not ask for.
- One independent reviewer runs `wyrd-task-review` over the integrated
  candidate against `spec.md` and this record.
