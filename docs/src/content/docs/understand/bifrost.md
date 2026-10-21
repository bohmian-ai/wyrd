---
title: Bifrost
description: Wyrd's analytical warehouse, the two ways to write to it, and when a written row becomes queryable.
pillar: wyrd
section: Understand
order: 4
---

Bifrost is the analytical warehouse inside every Wyrd server. Every observation, verdict, trace, audit record, and custom row lands in it, and all of it is queryable with SQL. It runs inside `wyrd-server`; there is nothing extra to deploy.

## How it is put together

Bifrost has four parts. Their names show up in logs, metrics, and deployment settings, so they are worth learning:

| Part | Does |
| --- | --- |
| Gate | Authenticates every request, checks the permission, and routes it: writes to a Scribe, queries to an Oracle |
| Scribe | Takes writes, acknowledges them once they are in its write-ahead log, serves recent rows as the live tail, and publishes them as Parquet |
| Oracle | Runs SQL over the published tables plus each Scribe's live tail, spreading large queries across peers |
| Forge | Compacts small files, applies retention, and commits new Iceberg snapshots in the background |

<WyrdDiagram name="bifrost-parts" />

## Where each part runs

Every pod runs the same `wyrd-server` binary. `WYRD_TARGET` picks which parts it runs, and the default, `all`, runs everything. A single-pod install never needs to set it. A team that outgrows one pod splits the parts so each scales on its own signal: Scribes with write volume, Oracles with query load, Forge workers with the compaction backlog.

<WyrdDiagram name="bifrost-pods" />

`scribe` and `oracle` pods each run half of the read path, so they only work with peers: pods that reach each other over mutual TLS. Exactly one Forge leader is active at a time, elected through Postgres; a second `all` or `server` pod waits as a standby.

## How it stores data

Tables are Apache Iceberg tables of Parquet files in the object store you configured with `WYRD_STORAGE_URL`. Postgres holds the catalog, scoped per tenant. Queries run on Apache DataFusion.

Each table has a namespace and a name, such as `vala.drift.observations`. Wyrd owns every namespace except `vala.datasets`, which is yours. See [Write and query custom data with Bifrost](/build-and-verify/observe/write-and-query-custom-data-with-bifrost/#4-find-and-describe-tables) for the full list.

Every row carries managed columns Wyrd fills in: `card_uid`, `run_id`, `sub_principal_id` (who the work was for), `act_principal_id` (who performed it), `wyrd_request_id`, and `wyrd_ingested_at`. Each table also has one time column that partitions it, so a query that filters on time reads only the files it needs.

## Two ways in

| Path | Use it from | Rows carry | Writes to |
| --- | --- | --- | --- |
| The observations API, `run.observe.*` | Your running service | The run and Card that produced them | Drift samples, verification inputs and results, and your `vala.datasets` tables |
| The Bifrost client, `Bifrost` | Batch jobs, backfills, notebooks, analysis | No run, unless you pass a correlation | Your `vala.datasets` tables |

They share one writer underneath, and both read with the same SQL. The difference is attribution. The observations API stamps every row with the exact Card version and run from the bundle your service loaded, which is what lets a verdict point back to what ran. The Bifrost client is for data that did not come from a run, and for reading anything you have access to.

[Record a run and observations](/build-and-verify/observe/record-a-run-and-observations/) and [Write and query custom data with Bifrost](/build-and-verify/observe/write-and-query-custom-data-with-bifrost/) show each path.

Writes go to the server over gRPC; queries go over HTTP.

## Written versus queryable

These are two different moments, and the gap between them explains most "where is my row" questions.

<WyrdTimeline name="bifrost-write-read" />

A write is stored when a Scribe acknowledges it. That is what `flush`, `shutdown`, and `write_batch` wait for. From then on the row survives a server restart.

A row is queryable once it is published into the table, which happens shortly after. An Oracle query reads the published data plus the live tail: recent rows still held by the Scribe that received them. A row moving from the live tail into the table while the query runs is returned exactly once. Only an unreachable Scribe can leave its recent rows out, and then the query ends `degraded`.

This applies to everything that reads Bifrost, including Verifiers. A scheduled drift window that closes the instant a row is written may score without it.

## Query results have a terminal

A query streams rows and then ends with a terminal status: `success`, `degraded`, or `failed`.

- `success` means every part of the plan completed.
- `degraded` means a Scribe holding recent rows was unreachable, so its live tail may be missing. Published data is complete.
- `failed` means the query did not finish. Partial rows are never returned as a success.

The SDKs check the terminal for you and raise on `failed`. The `QueryResult` keeps it as `terminal` if you need to tell `degraded` from `success`.

## What Bifrost does not do

- **No SQL writes.** `INSERT`, `UPDATE`, `DELETE`, and `CREATE TABLE AS` are refused. Write through the observations API or the Bifrost client.
- **No schema changes.** A table's columns and layout are fixed at registration. A new schema needs a new table.
- **No dropping tables.** Data is removed by retention, not by you.
- **No writes outside `vala.datasets`.** Drift, verification, telemetry, gateway, and audit tables are written only by Wyrd.

## Permissions

| To | You need |
| --- | --- |
| Query a table | `bifrost_query:read` |
| Read Gateway request and response payloads | `gateway_payload:read` as well |
| List or describe tables | `bifrost_table:read` |
| Register a table | `bifrost_table:write` |
| Write rows | `bifrost_record:write` |

`viewer` can query and describe. `workload` can also register and write. See [Identity and authorization](/understand/identity-and-authorization/#roles-and-permissions).
