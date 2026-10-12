---
title: Run a verification and inspect its evidence
description: Follow exact Cards to bindings and runs, then inspect persisted verification, Drift, and Eval rows through MCP.
pillar: wyrd
section: Build and verify
topic: Verify
order: 1
draft: true
status: runnable
---

Use MCP to discover an exact Service and its components, read verification state, and inspect retained judgments through Bifrost SQL. A completed failed judgment differs from a run that could not execute.

## Prerequisites

Connect an [agent through MCP](/connect/connect-an-agent-through-mcp/). Your tenant must contain a registered Service and its component Cards. To run a managed Drift verification, the component needs an active binding with a ready baseline. Use `cards:read`, `bifrost_table:read` for discovery/description, table-scoped `bifrost_query:read`, and `verifier:run` when starting or directly executing verification. Existing retained results can be investigated without starting another run.

## 1. Select an exact Service

Call `cards.list` with a registered name; `support-desk` is a repository fixture, not a built-in Service:

```json
{"kind":"Service","name":"support-desk","space":"default","limit":20}
```

Select a version from `items`. If `next_cursor` is present, repeat the same filters with that cursor unchanged. No fuzzy lookup or automatic newest-version selection occurs. Call `cards.get_by_ref` with the selected identity:

```json
{"card_ref":{"kind":"Service","name":"support-desk","version":"1.0.0","space":"default"}}
```

Read `card.spec.components` and `card.relationships.outbound_refs`. Every relationship edge has an exact `ref`; pass it to another `cards.get_by_ref` call. `inbound_refs` identifies Cards that refer to the current Card. Record each component's UID, kind, version, and space. `cards.get` remains available with `kind` and `card_uid` when you already have the UID.

## 2. Read bindings and start a managed run

Read the owner Card's `status.verification.binding_ids`. For each relevant ID, call `verification.get_binding`:

```json
{"binding_id":"<binding_id>"}
```

Use the returned owner, subject, Verifier, readiness, and last-run state. A binding ID belongs to a specific declared expectation, not to any component with the same name.

To enqueue a manual Drift run, call `verification.start_run`. Replace all placeholders with a discovered binding and an appropriate bounded window:

```json
{"target":{"kind":"binding","binding_id":"<binding_id>"},"input":{"kind":"drift_window","start":"2026-10-10T00:00:00Z","end":"2026-10-10T01:00:00Z"}}
```

The result contains the run ID. Poll `verification.get_run`:

```json
{"run_id":"<run_id>"}
```

Read execution state and `result_id`. Pending, timed-out, and errored execution are not completed judgments. Operator dispatch state is separate from the judgment. Use the returned result ID for the next step.

## 3. Inspect retained judgments

Call `bifrost.list_tables`, then `bifrost.describe_table` for `vala.verification.results`, `vala.drift.result_features`, and `vala.eval.result_items`. The [custom-data procedure](/build-and-verify/observe/write-and-query-custom-data-with-bifrost/#3-discover-and-describe) shows those calls and how to interpret positional rows.

Call `bifrost.query` with this argument template:

```json
{"sql":"SELECT result_id, implementation, execution_status, verdict, subject_card_uid FROM vala.verification.results WHERE result_id = '<result_id>' LIMIT 20","deadline_ms":10000,"max_rows":100,"max_bytes":65536}
```

If you started with a component rather than a run, find its retained results by the exact subject UID and a suitable window:

```sql
SELECT result_id, implementation, verdict, subject_card_uid
FROM vala.verification.results
WHERE subject_card_uid = '<subject_card_uid>'
  AND ended_at >= TIMESTAMP '2026-01-01 00:00:00'
  AND ended_at < TIMESTAMP '2027-01-01 00:00:00'
ORDER BY ended_at DESC
LIMIT 20
```

Adjust the calendar-year window. Select the most recent completed result for each implementation you need; ordering recent rows alone does not prove a completed judgment. `card_uid` on a result identifies its Verifier; `subject_card_uid` identifies what was judged. `owner_card_uid` identifies the binding owner where present. Do not use a principal ID as a Card UID.

For a Drift result:

```sql
SELECT result_id, feature, score, threshold, verdict
FROM vala.drift.result_features
WHERE result_id = '<result_id>'
LIMIT 20
```

For an Eval result:

```sql
SELECT result_id, task_id, outcome_kind, passed, skip_reason
FROM vala.eval.result_items
WHERE result_id = '<result_id>'
LIMIT 20
```

An Eval item that was skipped does not establish a passing assertion. Inspect `actual`, `expected`, and `message` when needed and authorized for that table. Those columns follow table query permission; sensitive-column metadata does not add a separate verification payload permission.

## Direct execution

`verification.execute` evaluates supplied input and returns a one-off judgment directly. It requires `verifier:run` on the exact Verifier. Use its advertised input schema: scoring supports current Drift implementations and direct assertion evaluation; unsupported execution forms are refused.

Direct execution enqueues no run, persists no result, and dispatches no Operator. Its returned judgment cannot be retrieved later through the result-history queries above. Use managed bindings and runs for retained evidence.

## Report the supported conclusion

Cite exact Card versions, subject UIDs, result IDs, table names, and returned rows. Distinguish a failed expectation from failed execution. State when a component has no binding or matching retained result. A Service-level result alone does not prove every component passed.

Missing and foreign-tenant Cards produce the same absence behavior. Permission refusals and oversized queries supply no complete evidence; do not infer a passing judgment from either. Require a successful terminal and preserve any returned warnings when explaining a query. See [MCP tools](/reference/mcp-tools/) for error handling and limits.
