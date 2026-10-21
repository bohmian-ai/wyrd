---
title: Cards, versions, and relationships
description: A Service declaration is a set of versioned Cards, each one a declared component of an AI system.
pillar: wyrd
section: Understand
order: 1
---

A Card is a versioned declaration of one AI system component, or of one Verifier that checks components. Every observation and verdict points back to an exact Card version.

## The shape of a Card

These are the two Cards from [Run your first verification](/get-started/quickstart/):

<Tabs>
<Tab label="support-reply.yaml">

<CodeFromFile file="examples/quickstart/support-reply.yaml" lang="yaml" title="support-reply.yaml" />

</Tab>
<Tab label="no-refund-promise.yaml">

<CodeFromFile file="examples/quickstart/no-refund-promise.yaml" lang="yaml" title="no-refund-promise.yaml" />

</Tab>
</Tabs>

An author writes `apiVersion`, `kind`, `metadata`, and `spec`. The server adds `relationships` and `status`.

A Card is identified by `space`, `kind`, `name`, and `version`. A space groups related Cards. An omitted space comes from `wyrd.toml`, or is `default`.

## Versions are exact

A reference names either a file, relative to the file it is in, or a declared Card at an exact version:

```yaml
verifier: ./no-refund-promise.yaml
verifier: {kind: Verifier, name: no-refund-promise, version: 1.0.0}
```

So a declared Service records exactly which Cards it was built from, and a verdict points back to exactly what ran. A Card moves to a new version of a dependency only when its reference changes and it is declared again.

`metadata.version` is optional. Without it, Wyrd records `0.1.0`, then the next patch version each time the content changes. Declaring unchanged content keeps the current version. Re-declaring an exact version with different content fails with `WYRD_REGISTRY_409_SPEC_DRIFT`.

## Relationships come from references

The server reads every reference in a Card's `spec` and records it in both directions:

```text
Service support-reply
└── Verifier no-refund-promise
```

Every Card therefore shows what it depends on and what depends on it. A running service may record observations only about Cards in its own graph.

## Next

[How verification works](/understand/how-verification-works/).
