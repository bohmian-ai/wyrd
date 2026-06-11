---
name: review
description: Repo-local Wyrd doctrine review. Use when reviewing Wyrd code, docs, contracts, internal APIs, external APIs, SDK surfaces, generated schemas, CLI, MCP, UI, or diffs that must align with the core doctrine.
---

# Wyrd Doctrine Review

Use this skill when reviewing changes in the Wyrd code repository for alignment
with the core doctrine.

## First Pass

1. Read `AGENTS.md`.
2. Read `docs/src/content/docs/concepts/core-doctrine.mdx`.
3. If available, compare against the canonical planning source:
   `/Users/stevenforrester/Documents/GitHub/wyrd-plan/architecture/v1/00-foundations/core-doctrine.md`.
4. Read the changed files and nearest contracts, tests, schemas, docs, or
   generated sources that define the same surface.
5. If invoked by `review-and-plan`, use the provided review packet and write
   the report to the provided output path. If no output path is provided, write
   the findings in the normal response.

## Review Criteria

Check whether the change preserves the doctrine across implementation and user
surfaces:

- Wyrd remains the AI layer for human and agentic work, not an application
  runtime, training framework, workflow engine, or cloud platform.
- Wyrd keeps a language-agnostic client/server model. Rust server code owns
  durable behavior and core logic; clients project API-wire contracts instead
  of becoming alternate sources of truth.
- Contracts remain on the wire through typed schemas, HTTP/MCP payloads,
  generated docs, and stable errors so any language can implement a client.
- Rust and Python may receive first-class SDK ergonomics, OTEL hooks, agent
  workflow integrations, local helpers, and tests, but those features must not
  move server-owned durable behavior into client packages or make Wyrd
  language-exclusive.
- Self-hosted and cloud SaaS paths preserve tenant separation for identity,
  authz, registry, storage, policy, audit, observability, evaluation, and
  generated artifacts.
- Wyrd remains agent-first and headless. MCP, CLI, HTTP, schemas, errors, and
  machine-readable docs are primary surfaces; the developer UI is supported but
  must not be the only way to perform a workflow.
- New concepts fit the small ontology first: `Card`, `Spec`, `Run`, or
  `Observation`. New nouns are justified only when those shapes cannot express
  the concept honestly.
- Card kinds specialize `Card`; they do not introduce separate top-level
  ontologies, envelopes, registration paths, tables, schemas, routes, or SDK
  object models.
- Foundations stay shared: envelope, metadata, `CardRef`, relationships,
  status, and version are not redefined by individual crates or surfaces.
- Services operate on foundations instead of one-off shapes only a single
  service understands.
- External and internal surfaces keep the same declarative contract. Python
  SDK, HTTP, CLI, MCP, UI, docs, generated schemas, and tests may add
  ergonomics, but must not rename fields, invent alternate payloads, expose
  server-internal state, or hide durable behavior behind runtime-only objects.
- Durable specs contain serializable declared intent, not live runtime state,
  handles, clients, thread state, hidden registries, or provider response blobs.
- User code runs implementations. Wyrd records, validates, versions, links,
  observes, governs, installs, and exposes declared shape.
- Predecessor names appear only in migration or audit context, never as Wyrd
  public vocabulary, route prefixes, package names, compatibility aliases, or
  user-facing API concepts.

## Finding Format

Report only issues that create doctrine drift or make the next implementation
agent likely to encode the wrong contract. Order by severity.

For each finding include:

- `Severity`: Critical, Major, or Minor.
- `Location`: file and line when available.
- `Issue`: the doctrine violation or ambiguity.
- `Doctrine`: the specific doctrine rule being violated.
- `Fix`: the concrete code, API shape, docs wording, or test correction needed.

If there are no issues, say the reviewed changes align with the core doctrine
and note any remaining uncertainty.
