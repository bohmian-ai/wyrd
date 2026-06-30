---
title: Model
description: Generated reference for the Wyrd Model card spec.
pillar: wyrd
group: cards
order: 2
---

# Model

Describe a model artifact, its interface, and the context needed to use it safely.

<dl class="wyrd-defs"><dt data-kind="model">Model</dt><dd>Describe a model artifact, its interface, and the context needed to use it safely.</dd><dt>Required</dt><dd><code>interface</code>, <code>signature</code>, <code>task_type</code></dd><dt>Optional</dt><dd>2 additional spec fields — see table below.</dd></dl>

This page is generated from the checked-in JSON Schema. Edit the Rust spec, run the schema generator, then run `mise run docs:generate` to refresh this page.

## Source

- Schema: `crates/wyrd-spec/schemas/model_spec.json`

## Fields

| Field | Type | Required |
| --- | --- | --- |
| `card_refs` | `array` | no |
| `interface` | `object` | yes |
| `sample_input` | `object` | no |
| `signature` | `object` | yes |
| `task_type` | `object` | yes |

## Shape

The `card.json` envelope wraps this spec under `kind: Model`. For a runnable authoring walkthrough, see the [Python SDK](/python/). The JSON Schema at `crates/wyrd-spec/schemas/model_spec.json` is the field-level source of truth.

## Lifecycle

`ModelCard` is a shipped holder: author it locally, then register it to a
server. Registration is idempotent on identity and rejects a changed spec at
the same version. See [Register cards](/server/register-cards/).

## Authoring notes

- Keep card names stable; downstream runs, policies, and audits refer to them by identity.
- Put operational rules in the spec instead of burying them in free-form notes.
- Prefer explicit references to other cards when a dependency matters at runtime.

## Related

- [How it connects](/start-here/how-it-connects/) — where Model fits in the card model.
- [Card reference index](/cards/) — every kind in one place.
