---
title: Bifrost
description: Wyrd's analytical warehouse, the two ways to write to it, and when a written row becomes queryable.
pillar: wyrd
section: Understand
order: 4
---

Bifrost is the analytical warehouse inside every Wyrd server. Observations, verdicts, traces, audit records, and caller-owned rows all land in it as tables that read-only SQL can query:

```sql
SELECT verdict, count(*) AS executions
FROM vala.verification.results
GROUP BY verdict
```

## How it is put together

| Part | Does |
| --- | --- |
| Gate | Checks each request's permission and routes it |
| Scribe | Accepts writes and publishes them into tables |
| Oracle | Runs SQL queries |
| Forge | Compacts files and applies retention in the background |

One server runs all four. A larger deployment can split them across pods with `WYRD_TARGET`.

Tables are Apache Iceberg tables of Parquet files in the deployment's object store, with the catalog in Postgres. The `vala.datasets` namespace holds caller-owned tables. Wyrd writes every other namespace.

## Two ways in

Inside a running service, `run.observe.*` writes rows tied to the run and the exact Card version. For batch jobs, backfills, and notebooks, the `Bifrost` client registers tables, writes rows, and queries them. SQL is read-only.

## Written versus queryable

A row is stored once a Scribe acknowledges it, which is what `flush` and `shutdown` wait for. It becomes queryable when it is published, shortly after. Queries also read the recent unpublished rows each Scribe still holds, and a row published while the query runs is returned exactly once.

Every query ends with a status: `success`, `degraded` when an unreachable Scribe's recent rows may be missing, or `failed`. The SDKs raise an error on `failed`. Query errors are listed in [Errors](/api/errors/).

## Next

[Identity and authorization](/understand/identity-and-authorization/).
