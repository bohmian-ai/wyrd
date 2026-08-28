# Wyrd Architecture

This directory is the repository-level architecture and engineering authority
for Wyrd. It describes the required system directly. Implementation that
conflicts with these documents is drift to correct; implementation state,
delivery sequencing, and decision history belong outside normative
architecture.

## Authority map

| Document | Normative responsibility |
|---|---|
| [`wyrd-design.md`](wyrd-design.md) | Wyrd protocol, doctrine, nouns, Card contracts, service boundaries, identity, and public surfaces |
| [`bifrost-design.md`](bifrost-design.md) | Bifrost table, Scribe, Oracle, Forge, storage, resource, and serving architecture |
| [`wyrd-security-posture.md`](wyrd-security-posture.md) | Trust boundaries, credentials, authorization, tenant security, audit integrity, and security operations |
| [`agent-rules.md`](agent-rules.md) | Mandatory repository implementation constraints |
| [`wyrd-doctrine.mdx`](wyrd-doctrine.mdx) | Public rationale for the protocol and product boundaries |
| [`operations/`](operations/README.md) | Supported deployment, release, reliability, backup, recovery, SLO, and incident contracts |
| [`references/`](references/README.md) | Reusable engineering expertise for applying the architecture |
| [`v1/`](v1/) | Detailed v1 foundation, service, surface, crate, and journey specifications |

## Conflict resolution

Apply the narrowest owning authority:

1. `wyrd-design.md` wins for Wyrd doctrine, Cards, wire contracts, and
   cross-product boundaries.
2. `bifrost-design.md` wins for Bifrost internals.
3. `wyrd-security-posture.md` wins for security controls without changing the
   protocol nouns owned by `wyrd-design.md`.
4. `operations/` wins for deployment and operational behavior without
   weakening security or protocol guarantees.
5. `agent-rules.md` and repository `AGENTS.md` govern implementation shape and
   contributor behavior.
6. `references/` explain how to apply those decisions. A reference cannot
   override an owning architecture document.

Generated schemas, OpenAPI, source code, examples, tests, and old design
artifacts do not override these authorities. Resolve an actual contradiction
in the owning architecture document instead of introducing aliases or parallel
contracts.

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
