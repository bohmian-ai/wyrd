---
title: Architecture
description: What a team deployment of Wyrd runs, what it connects to, and why each part is separate.
pillar: wyrd
section: Understand
order: 1
---

A Wyrd deployment is one server binary, run as one pod or as several pods with different roles, behind one endpoint you own. It stores everything durable in Postgres and object storage, so pods can be replaced, scaled, or split by role without moving data.

<WyrdDiagram name="wyrd-deployment" />

## One binary, several roles

Every pod runs `wyrd-server`. `WYRD_TARGET` chooses what a pod does: `all` runs everything and is a complete deployment on its own; `server`, `scribe`, `oracle`, and `forge-worker` split API, writes, queries, and compaction so each scales on its own load. Splitting changes resource ownership, not the API: clients see one server either way. [Bifrost](/understand/bifrost/#where-each-part-runs) shows which part each target runs.

## One endpoint

Your load balancer owns the public address and TLS. It routes requests but never establishes who the caller is; every request carries a Wyrd token, and the server checks its roles and tenant. Pods talk to each other over mutual TLS.

## Postgres and object storage

Postgres holds declarations and their versions, principals and roles, and the coordination state that lets many pods share work. Object storage holds evidence, artifacts, and Bifrost's Parquet tables. Neither lives on a pod, which is why a pod can be lost without losing a verdict.

## Identity

People sign in through your OIDC identity provider. Apps, CI, and agents use API keys or workload identity. [Identity and authorization](/understand/identity-and-authorization/) explains principals, roles, and tenants.

## Model calls

The server calls model providers with keys you configure, for model-judged checks and for apps that route their calls through the LLM gateway. Wyrd never hosts a model.

## Next

[How verification works](/understand/how-verification-works/) follows one check through the server. [Bifrost](/understand/bifrost/) covers the data plane.
