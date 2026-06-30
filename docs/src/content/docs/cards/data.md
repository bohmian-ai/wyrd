---
title: Data
description: Generated reference for the Wyrd Data card spec.
pillar: wyrd
group: cards
order: 1
---

# Data

Describe a dataset, feature table, document set, or other data dependency.

<dl class="wyrd-defs"><dt data-kind="data">Data</dt><dd>Describe a dataset, feature table, document set, or other data dependency.</dd><dt>Required</dt><dd><code>interface</code>, <code>schema</code>, <code>stats</code></dd><dt>Optional</dt><dd>4 additional spec fields — see table below.</dd></dl>

This page is generated from the checked-in JSON Schema. Edit the Rust spec, run the schema generator, then run `mise run docs:generate` to refresh this page.

## Source

- Schema: `crates/wyrd-spec/schemas/data_spec.json`

## Fields

| Field | Type | Required |
| --- | --- | --- |
| `card_refs` | `array` | no |
| `interface` | `object` | yes |
| `schema` | `object` | yes |
| `splits` | `object` | no |
| `sql` | `object` | no |
| `stats` | `object` | yes |
| `target_columns` | `array` | no |

## Shape

The `card.json` envelope wraps this spec under `kind: Data`. For a runnable authoring walkthrough, see the [Python SDK](/python/). The JSON Schema at `crates/wyrd-spec/schemas/data_spec.json` is the field-level source of truth.

## Lifecycle

`DataCard` is a shipped holder: author it locally, then register it to a
server. Registration is idempotent on identity and rejects a changed spec at
the same version. See [Register cards](/server/register-cards/).

## Authoring notes

- Keep card names stable; downstream runs, policies, and audits refer to them by identity.
- Put operational rules in the spec instead of burying them in free-form notes.
- Prefer explicit references to other cards when a dependency matters at runtime.

## Related

- [How it connects](/start-here/how-it-connects/) — where Data fits in the card model.
- [Card reference index](/cards/) — every kind in one place.
