# Positioning And Vocabulary

Wyrd is the AI layer for human and agentic work — **the platform agents
and users love to build on**. It makes AI work declarative, inspectable,
reproducible, governed, observable, and operable.

Wyrd is **language-agnostic**. First-class SDKs ship for Python, Rust,
and TypeScript today. Go is planned and becomes first-class only when its
SDK and the same contract/journey gates ship. Language agnosticism is the
doctrine; first-class SDK support is how we deliver ergonomics on top of
it without moving durable behavior out of the server.

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

Sixteen native kinds plus `External`:

`Data`, `Model`, `Artifact`, `Experiment`, `Prompt`, `Agent`, `Workflow`,
`Mcp`, `Service`, `Policy`, `Audit`, `Drift`, `Eval`, `Source`, `Trigger`,
`Operator`, `External`.

Not Card kinds:

- **`Tool`** — a Skald/runtime registry concept.
- **`Bifrost`** — an engine inside Vala for OLAP ingest/query; wired to
  `wyrd-server` as the sole serving surface. Not a Card kind. Not an
  external system.
- **`SubAgent`** — sub-agency is an Agent-to-Agent relationship, not a
  Card kind.
- **`Skill`** — not a v1 Card kind unless a future architecture decision
  adds it.

Current `wyrd-spec` code may still expose stale `Tool`, `Skill`, or
`SubAgent` specs and lack `SourceSpec`. Treat that as implementation drift
to remove, not as contract precedent.

## CardRef Shape

`CardRef` carries `kind`, `name`, one `version` field, optional `space`,
and optional `uid`.

```yaml
ref: { kind: Model, name: churn-rf, version: "~1" }
ref: { kind: Prompt, name: judge, version: "^1", space: shared }
```

Do not introduce a separate `version_req` field.

## Vocabulary Rules

- New durable concepts should first fit `Card`, `Spec`, `Run`, or
  `Observation`.
- Card kinds are not separate top-level ontologies.
- `Policy` is a card kind when users declare a governable rule; policy
  decisions are service behavior.
- `Audit` is a card kind when users declare audit scope or evidence;
  audit history is service-owned accountability.
- `Artifact` is a card kind; storage owns bytes; other cards link to
  artifacts with `CardRef`.
- Predecessor names are allowed only in audit, source-map, or comparison
  context. Do not import legacy names, package names, route prefixes, or
  compatibility shims into implementation code.

## Deleted Concepts (Do Not Reintroduce)

- **Governance tokens.** `WYRD_GOV_TOKEN`,
  `wyrd.auth_governance_tokens`, `GovernanceTokenRow`, and
  `Scope::TokenIssue` are gone. Auth is a single plane. Emit is an
  Auth-plane route, not a third plane. The JWT (`principal.card_ref`)
  plus opaque client-generated `run_id` carry everything.
- **Legacy server vocab** (former project names, `_delta_log`, Delta
  transaction-log logic in new Vala paths). Enforced by
  `check:no-legacy-server-vocab`.
