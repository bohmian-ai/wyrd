# Wyrd Architecture

This directory is the repository-level design authority
for Wyrd. It describes the required system directly. Implementation that
conflicts with these documents is drift to correct; implementation state,
delivery sequencing, and decision history belong outside normative
architecture.

## Authority map

| Document | Owns |
|---|---|
| [`wyrd-design.md`](wyrd-design.md) | Doctrine, nouns, Card contracts, service boundaries, identity, and public surfaces |
| [`bifrost-design.md`](bifrost-design.md) | Bifrost tables, Scribe, Oracle, Forge, storage, resources, and serving |
| [`wyrd-security-posture.md`](wyrd-security-posture.md) | Trust boundaries, credentials, permissions, tenant isolation, audit integrity, and security operations |
| [`operations/`](operations/README.md) | Deployment, release, reliability, backup, recovery, and incidents |

Contributor rules live outside this directory: the root `AGENTS.md`, nested
`AGENTS.md` files beside the code they govern, and `agent_docs/` topic guides.
The change workflow lives in `changes/`.

## Conflict resolution

The narrowest owner wins:

1. `wyrd-design.md` for doctrine, Cards, wire contracts, and cross-product
   boundaries.
2. `bifrost-design.md` for Bifrost internals.
3. `wyrd-security-posture.md` for security controls, without changing protocol
   nouns.
4. `operations/` for operational behavior, without weakening security or
   protocol guarantees.

Contributor rules govern implementation shape and cannot override these
documents. Generated schemas, OpenAPI, source code, examples, tests, and old
design artifacts do not either. Resolve a real contradiction in the owning
document instead of adding aliases or parallel contracts.

## Normative writing rules

- State one required architecture. Do not preserve superseded alternatives,
  delivery status, historical execution records, or projections in this
  directory.
- Use Wyrd vocabulary and repository ownership names.
- Separate contract, rationale, and operational procedure so that each fact has
  one owner.
- Do not encode unverified throughput, latency, availability, recovery, or
  capacity claims. Production objectives require named owners, measurement,
  evidence, and error-budget behavior.
- Do not put secret values, private topology details, tenant data, or
  credential examples in architecture documents.
