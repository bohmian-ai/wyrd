---
title: Errors
description: Error handling guidance for Wyrd API clients and agents.
---

# Errors

Wyrd returns errors as structured RFC 7807 Problem Details objects. Agents and SDK clients must preserve the full structure and must not collapse errors into prose.

## Error fields

| Field | Description |
| --- | --- |
| `type` | URI identifying the error class |
| `title` | Human-readable summary of the error class (stable across instances) |
| `status` | HTTP status code (integer) |
| `code` | Wyrd-specific machine-readable error code (string, stable) |
| `detail` | Instance-specific description of what went wrong |
| `remediation` | Actionable guidance for the caller or agent |
| `context` / `details` | Structured key/value map with operation-specific fields |
| `request_id` | Present when the server can correlate the request |
| `trace_id` | Present when distributed tracing is active |
| `instance` | URI identifying the specific resource or operation that failed (when applicable) |

## Agent handling

- Preserve all structured fields; do not collapse errors into prose.
- Use `code` for programmatic branching, not `title` or `detail` (those are for humans).
- Surface policy and audit failures as decisions (not generic transport failures).
- Keep retries bounded and tied to idempotent operations.
- Link remediation steps to the card or run that caused the error.

## Catalog

The public error catalog is not emitted as a standalone file yet. Once that generator lands, this page will link to the full catalog with per-code remediation tables.
