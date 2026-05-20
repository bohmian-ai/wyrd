---
title: MCP
description: Generated reference for the Wyrd MCP card spec.
---

# MCP

Describe an MCP surface that tools and agents can discover consistently.

This page is generated from the checked-in JSON Schema. Edit the Rust spec, run the schema generator, then run `mise run docs:generate` to refresh this page.

## Source

- Schema: `crates/wyrd-spec/schemas/mcp_spec.json`

## Fields

| Field | Type | Required |
| --- | --- | --- |
| `description` | `string | null` | no |
| `details` | `object` | no |
| `scopes` | `array` | no |
| `server_name` | `string` | yes |
| `tool_refs` | `array` | no |
| `transport` | `string | null` | no |

## Authoring notes

- Keep card names stable; downstream runs, policies, and audits refer to them by identity.
- Put operational rules in the spec instead of burying them in free-form notes.
- Prefer explicit references to other cards when a dependency matters at runtime.
