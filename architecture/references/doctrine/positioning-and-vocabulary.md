# Positioning And Vocabulary

Wyrd is the AI layer for human and agentic work — **the platform agents
and users love to build on**. It makes AI work declarative, inspectable,
reproducible, governed, observable, and operable.

Wyrd is **language-agnostic**. Python, Rust, and TypeScript are first-class SDK
languages. Language agnosticism is the doctrine; first-class SDK support adds
ergonomics without moving durable behavior out of the server. Every other
language can implement a client from the same HTTP, MCP, schema, documentation,
and stable-error contracts.

Wyrd does not try to become the user's application runtime, training
framework, workflow engine, arbitrary code executor, or cloud platform.

## Doctrine Layers

| Layer | Meaning |
|---|---|
| Core nouns | `Card`, `Spec`, `Run`, `Observation`. |
| Card kinds | Domain specializations of `Card`; the kind changes the spec. |
| Foundations | Envelope, metadata, `CardRef`, relationships, status, versioning. |
| Services | Registry, storage, lineage, policy, install, audit, observability, evaluation, drift, runtime. |
| Surfaces | HTTP, Python SDK, Rust SDK, TypeScript SDK, CLI, UI, MCP, IDE integrations, generated docs. |

Cards declare. Specs define. `CardRef`s connect. Relationships explain.
Status tracks lifecycle. Runs record execution. Observations record
measured facts.

## Canonical Card Envelope

Every registered card uses the same envelope:

```yaml
apiVersion: wyrd/v1
metadata:
  space: prod
  name: churn-model
  version: "1.2.0"
kind: Model
spec:
  ...
relationships: []
status: null
```

Rules:

- `kind` is the card specialization.
- `spec` is the typed payload for that kind.
- Do not use the stale `kind: Card` plus `spec.type` envelope.
- Relationships are server-derived from `CardRef` values inside specs.
- Status is server-managed lifecycle state.

## v1 Card Kinds

Sixteen registrable native kinds:

`Data`, `Model`, `Artifact`, `Experiment`, `Prompt`, `Agent`, `Workflow`,
`Mcp`, `Service`, `Policy`, `Audit`, `Drift`, `Eval`, `Source`, `Trigger`,
`Operator`.

`CardKind::External` is a non-registrable discriminator for foreign schema
descriptors. It has no `ExternalSpec` and is never accepted as a Card payload.

Not Card kinds:

- **`Tool`** — a Skald/runtime registry concept.
- **`Bifrost`** — an engine inside Vala for OLAP ingest/query; wired to
  `wyrd-server` as the sole serving surface. Not a Card kind. Not an
  external system.
- **`SubAgent`** — sub-agency is an Agent-to-Agent relationship, not a
  Card kind.
- **`Skill`** — not a v1 Card kind.

`SourceSpec` is the typed contract for read-only access to external data.
`Tool`, `Skill`, and `SubAgent` specs are not part of the Card contract.

## CardRef Shape

`CardRef` carries `kind`, `name`, one exact `version` field, required `space`,
and optional `uid`. The wire identity is `(kind, name, version, space)`; when
present, `uid` strengthens the pin to the registered identity.

```yaml
ref: { kind: Model, name: churn-rf, version: "1.4.2", space: ml-prod }
ref: { kind: Prompt, name: judge, version: "1.0.0", space: shared }
```

Versions in a `CardRef` are strict `MAJOR.MINOR.PATCH` pins, never ranges,
comparators, or `latest`. An authored reference may omit `space` only where the
loader inherits the enclosing Card's space before transmission. A wire
`CardRef` always carries `space`. Do not introduce a separate `version_req`
field.

## Vocabulary Rules

- New durable concepts should first fit `Card`, `Spec`, `Run`, or
  `Observation`.
- Card kinds are not separate top-level ontologies.
- `Policy` is a card kind when users declare a governable rule; policy
  decisions are service behavior.
- `Audit` is the immutable Card produced when an authorized investigator pins
  a case file; it is not a user-authored audit-scope declaration. Audit-event
  history remains service-owned accountability.
- `Artifact` is a card kind; storage owns bytes; other cards link to
  artifacts with `CardRef`.
- `Source` is a read-only declaration for external data. Wyrd never writes to
  the external system through a Source adapter.
- `Bifrost` is Wyrd-owned analytical infrastructure, not an external Source,
  Card kind, or public warehouse noun.
- Predecessor names are allowed only in audit, source-map, or comparison
  context. Do not import legacy names, package names, route prefixes, or
  compatibility shims into implementation code.

## Prohibited Concepts

- **Governance tokens.** `WYRD_GOV_TOKEN`,
  `wyrd.auth_governance_tokens`, `GovernanceTokenRow`, and
  `Scope::TokenIssue` must not exist. Auth is a single plane. Emit is an
  Auth-plane route, not a third plane. The JWT (`principal.card_ref`)
  plus opaque client-generated `run_id` carry everything.
- **Legacy server vocab** (former project names, `_delta_log`, Delta
  transaction-log logic in new Vala paths). Enforced by
  `check:no-legacy-server-vocab`.
