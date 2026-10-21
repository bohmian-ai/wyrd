---
title: Bifrost
description: Wyrd's analytical warehouse, the two ways to write to it, and when a written row becomes queryable.
pillar: wyrd
section: Understand
order: 4
---

Bifrost is the analytical warehouse inside every Wyrd server. Observations, verdicts, traces, audit records, and your own rows all land in it, and you query them with SQL.

## How it is put together

| Part | Does |
| --- | --- |
| Gate | Checks each request's permission and routes it |
| Scribe | Accepts writes and publishes them into tables |
| Oracle | Runs SQL queries |
| Forge | Compacts files and applies retention in the background |

One server runs all four. A larger deployment can split them across pods with `WYRD_TARGET`.

Tables are Apache Iceberg tables of Parquet files in your object store, with the catalog in Postgres. The `vala.datasets` namespace is yours. Wyrd writes every other namespace.

## Two ways in

Inside a running service, `run.observe.*` writes rows tied to the run and the exact Card version. For batch jobs, backfills, and notebooks, the `Bifrost` client registers tables, writes rows, and queries them. SQL is read-only.

## Written versus queryable

A row is stored once a Scribe acknowledges it, which is what `flush` and `shutdown` wait for. It becomes queryable when it is published, shortly after. Queries also read recent unpublished rows on a best-effort basis, so a row written seconds ago may not appear yet.

Every query ends with a status: `success`, `degraded` when recent rows may be missing, or `failed`. The SDKs raise on `failed`.

## Next

[Identity and authorization](/understand/identity-and-authorization/).
