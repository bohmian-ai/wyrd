---
title: Runs and evidence
description: How Wyrd records what a declared service actually does, and ties each observation to the version that ran.
pillar: wyrd
section: Understand
order: 3
---

An observation is a record of what a declared service actually did. Each one is tied to the exact Card version and the run that produced it.

## A run is one invocation

A run is one execution of a service, such as answering one question. Every observation recorded inside it shares the same `run_id`:

<LangTabs langs={['python', 'rust', 'typescript']}>
<LangTab lang="python">

```python
with state.run("agent") as run:
    answer = answer_question(question)
    run.observe.record("bifrost.datasets.tickets", {"question": question, "answer": answer})
```

</LangTab>
<LangTab lang="rust">

```rust
let run = state.run_for_card("agent")?;
let answer = answer_question(question).await?;
run.observe()
    .record("bifrost.datasets.tickets", &json!({ "question": question, "answer": answer }))
    .await?;
```

</LangTab>
<LangTab lang="typescript">

```typescript
const run = state.run("agent");
const answer = await answerQuestion(question);
await run.observe.record("bifrost.datasets.tickets", { question, answer });
```

</LangTab>
</LangTabs>

`state` holds the exact Card versions the service runs. `state.run("agent")` scopes the run to the Card behind the `agent` alias, and `run.for_card(alias)` scopes the same run to another one. Opening a run makes no network call.

## Where observations land

Observations go to Bifrost, the Wyrd data plane:

| Recorded with | Lands in |
| --- | --- |
| `observe.record(table, row)` | A caller-owned `bifrost.datasets.<name>` table |
| `observe.verify` and `observe.submit` | `bifrost.verification.execution_inputs` and `bifrost.verification.results` |
| Drift samples, with `observe.submit` | `bifrost.drift.observations` |
| OpenTelemetry spans, metrics, and logs | `bifrost.traces.spans`, `bifrost.metrics.points`, and `bifrost.logs.records` |

Every row carries its `subject_card_uid`, its `run_id`, and the principal that wrote it. A service may write only about Cards in its own graph. Anything else fails with `WYRD_VALA_403_BIFROST_CARD_SCOPE` ([Errors](/api/errors/)).

## Recording is buffered

`observe.*` calls queue the row; a returned call means the row is queued, not stored. `shutdown` sends everything still queued, so a process that exits without it loses those rows. If the queue fills, the call fails with `WYRD_CLIENT_429_QUEUE_FULL` instead of blocking.

A row the server has stored can take a moment to show up in queries. [Bifrost](/understand/bifrost/) explains why.

## Evidence joins on the run

Observations and verdicts share `run_id`, so one SQL query joins what a run saw to how it was judged:

```sql
SELECT t.question, t.answer, r.verdict, r.verifier_version
FROM bifrost.datasets.tickets AS t
JOIN bifrost.verification.results AS r ON r.run_id = t.run_id
```

## Next

[Bifrost](/understand/bifrost/).
