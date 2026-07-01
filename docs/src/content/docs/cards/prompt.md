---
title: Prompt
description: Generated reference for the Wyrd Prompt card spec.
pillar: wyrd
group: Reference
order: 13
---

# Prompt

Version prompt content and the contract around its inputs and outputs.

<CardSummary kind={"prompt"} title={"Prompt"} purpose={"Version prompt content and the contract around its inputs and outputs."} required={["model", "request"]} optionalCount={4} />

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

The `card.json` envelope wraps this spec under `kind: Prompt`. For a runnable authoring walkthrough, see [Your first card](/tutorials/first-card/). The JSON Schema at `crates/wyrd-spec/schemas/prompt_spec.json` is the field-level source of truth.

## Lifecycle

`PromptCard` is a shipped holder: author it locally, then register it to a
server. Registration is idempotent on identity and rejects a changed spec at
the same version. See [Declare a card](/how-to/declare/).

## Authoring notes

- Keep card names stable; downstream runs, policies, and audits refer to them by identity.
- Put operational rules in the spec instead of burying them in free-form notes.
- Prefer explicit references to other cards when a dependency matters at runtime.

## Related

- [Card](/concepts/card/) — where Prompt fits in the card model.
- [Card reference index](/cards/) — every kind in one place.
