# Positioning And Vocabulary

Wyrd is the AI layer for human and agentic work. It makes AI work declarative,
inspectable, reproducible, governed, observable, and operable.

Wyrd does not try to become the user's application runtime, training
framework, workflow engine, arbitrary code executor, or cloud platform.

## Doctrine Layers

| Layer | Meaning |
|---|---|
| Core nouns | `Card`, `Spec`, `Run`, and `Observation`. |
| Card kinds | Domain specializations of `Card`; the kind changes the spec. |
| Foundations | Envelope, metadata, `CardRef`, relationship, status, and version. |
| Services | Registry, storage, lineage, policy, install, audit, observability, evaluation, drift, runtime. |
| Surfaces | HTTP, Python SDK, CLI, UI, MCP, IDE integrations, generated docs. |

Cards declare. Specs define. CardRefs connect. Relationships explain. Status
tracks lifecycle. Runs record execution. Observations record measured facts.

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

- `kind` is the card specialization, such as `Model`, `Data`, `Prompt`,
  `Artifact`, `Eval`, or `Policy`.
- `spec` is the typed payload for that kind.
- Do not use the stale `kind: Card` plus `spec.type` envelope.
- Relationships are server-derived from `CardRef` values inside specs.
- Status is server-managed lifecycle state.

## CardRef Shape

`CardRef` carries `kind`, `name`, one `version` field, optional `space`, and
optional `uid`.

```yaml
ref: { kind: Model, name: churn-rf, version: "~1" }
ref: { kind: Prompt, name: judge, version: "^1", space: shared }
```

Do not introduce `version_req`.

## Vocabulary Rules

- New durable concepts should first fit `Card`, `Spec`, `Run`, or
  `Observation`.
- Card kinds are not separate top-level ontologies.
- Policy is a card kind when users declare a governable rule; policy decisions
  are service behavior.
- Audit is a card kind when users declare audit scope or evidence; audit
  history is service-owned accountability.
- Artifact is a card kind; storage owns bytes; other cards link to artifacts
  with `CardRef`.
- Predecessor names are allowed only in audit, source-map, or comparison
  context.
