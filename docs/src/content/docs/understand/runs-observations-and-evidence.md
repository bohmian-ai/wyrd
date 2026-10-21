---
title: Runs, observations, and retained evidence
description: How Wyrd records what a declared service actually does, and ties each observation to the version that ran.
pillar: wyrd
section: Understand
order: 3
---

An observation is a record of what your declared service actually did, tied to the exact Card version and run that produced it.

## A run is one invocation

A run is one execution of your service, such as answering one question. Every observation recorded inside it shares the same `run_id`:

<LangTabs langs={['python', 'rust', 'typescript']}>
<LangTab lang="python">

```python
with state.run("agent") as run:
    answer = answer_question(question)
    run.observe.record("vala.datasets.tickets", {"question": question, "answer": answer})
```

</LangTab>
<LangTab lang="rust">

```rust
let run = state.run_for_card("agent")?;
let answer = answer_question(question).await?;
run.observe()
    .record("vala.datasets.tickets", &json!({ "question": question, "answer": answer }))
    .await?;
```

</LangTab>
<LangTab lang="typescript">

```typescript
const run = state.run("agent");
const answer = await answerQuestion(question);
await run.observe.record("vala.datasets.tickets", { question, answer });
```

</LangTab>
</LangTabs>

`state` holds the exact Card versions your service runs. `state.run("agent")` scopes the run to the Card behind the `agent` alias, and `run.for_card(alias)` scopes the same run to another one. Opening a run makes no network call.

## Where observations land

Observations go to Bifrost, the Wyrd data plane:

| You record | Lands in |
| --- | --- |
| `observe.record(table, row)` | Your own `vala.datasets.<name>` table |
| `observe.verify` and `observe.submit` | `vala.verification.execution_inputs` and `vala.verification.results` |
| Drift samples, with `observe.submit` | `vala.drift.observations` |
| OpenTelemetry spans, metrics, and logs | `vala.traces.spans`, `vala.metrics.points`, and `vala.logs.records` |

Every row carries its `card_uid`, its `run_id`, and the principal that wrote it. A service may write only about Cards in its own graph. Anything else fails with `WYRD_VALA_403_BIFROST_CARD_SCOPE`.

## Recording is buffered

`observe.*` calls queue the row and return at once. `shutdown` sends everything still queued, so call it before your process exits. If the queue fills, the call fails with `WYRD_CLIENT_429_QUEUE_FULL` instead of blocking.

A row the server has stored can take a moment to show up in queries. [Bifrost](/understand/bifrost/) explains why.

## Query the evidence

Observations and verdicts share `run_id`, so one SQL query joins what a run saw to how it was judged:

```sql
SELECT t.question, t.answer, r.verdict, r.verifier_version
FROM vala.datasets.tickets AS t
JOIN vala.verification.results AS r ON r.run_id = t.run_id
```

## Next

[Bifrost](/understand/bifrost/).
