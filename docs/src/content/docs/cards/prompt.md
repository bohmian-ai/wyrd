---
title: Prompt
description: Generated reference for the Wyrd Prompt card spec.
pillar: wyrd
group: cards
order: 3
---

# Prompt

Version prompt content and the contract around its inputs and outputs.

<dl class="wyrd-defs"><dt data-kind="prompt">Prompt</dt><dd>Version prompt content and the contract around its inputs and outputs.</dd><dt>Required</dt><dd><code>model</code>, <code>request</code></dd><dt>Optional</dt><dd>4 additional spec fields — see table below.</dd></dl>

This page is generated from the checked-in JSON Schema. Edit the Rust spec, run the schema generator, then run `mise run docs:generate` to refresh this page.

## Source

- Schema: `crates/wyrd-spec/schemas/prompt_spec.json`

## Fields

| Field | Type | Required |
| --- | --- | --- |
| `media_variables` | `array` | no |
| `model` | `string` | yes |
| `request` | `object` | yes |
| `response_type` | `object` | no |
| `variables` | `array` | no |
| `version` | `string \| null` | no |

## Shape

The `card.json` envelope wraps this spec under `kind: Prompt`. For a runnable authoring walkthrough, see the [Python SDK](/python/). The JSON Schema at `crates/wyrd-spec/schemas/prompt_spec.json` is the field-level source of truth.

## Lifecycle

`PromptCard` is a shipped holder: author it locally, then register it to a
server. Registration is idempotent on identity and rejects a changed spec at
the same version. See [Register cards](/server/register-cards/).

## Authoring notes

- Keep card names stable; downstream runs, policies, and audits refer to them by identity.
- Put operational rules in the spec instead of burying them in free-form notes.
- Prefer explicit references to other cards when a dependency matters at runtime.

## Related

- [How it connects](/start-here/how-it-connects/) — where Prompt fits in the card model.
- [Card reference index](/cards/) — every kind in one place.
