---
title: Bifrost
description: Wyrd's analytical warehouse, the two ways to write to it, and when a written row becomes queryable.
pillar: wyrd
section: Understand
order: 5
---

Bifrost is the analytical warehouse inside every Wyrd server. Observations, verdicts, traces, audit records, and caller-owned rows all land in it as tables that read-only SQL can query:

```sql
SELECT verdict, count(*) AS executions
FROM bifrost.verification.results
GROUP BY verdict
```

## How it is put together

<WyrdDiagram name="bifrost-parts" />

| Part | Does |
| --- | --- |
| Gate | Checks each request's permission and routes it |
| Scribe | Accepts writes and publishes them into tables |
| Oracle | Runs SQL queries |
| Forge | Compacts files and applies retention in the background |

One server runs all four.

Tables are Apache Iceberg tables of Parquet files in the deployment's object store, with the catalog in Postgres. The `vala.datasets` namespace holds caller-owned tables. Wyrd writes every other namespace.

## Where each part runs

Every pod runs the same `wyrd-server` binary, and `WYRD_TARGET` picks which parts it runs. The default, `all`, runs every part, so a single pod is a complete deployment. A larger team splits writes, queries, and compaction so each scales on its own load.

<WyrdDiagram name="bifrost-pods" />

## Two ways in

Inside a running service, `run.observe.*` writes rows tied to the run and the exact Card version. For batch jobs, backfills, and notebooks, the `Bifrost` client registers tables, writes rows, and queries them. SQL is read-only.

## Written versus queryable

<WyrdTimeline name="bifrost-write-read" />

A row is stored once a Scribe acknowledges it, which is what `flush` and `shutdown` wait for. It becomes queryable when it is published, shortly after. Queries also read the recent unpublished rows each Scribe still holds, and a row published while the query runs is returned exactly once.

Every query ends with a status: `success`; `degraded` when a Scribe could not be reached as the query listed its sources, so that Scribe's recent rows may be missing; or `failed`, which includes losing a Scribe after the query has read its recent rows. The SDKs raise an error on `failed`. Query errors are listed in [Errors](/api/errors/).

## Next

[Identity and authorization](/understand/identity-and-authorization/).
