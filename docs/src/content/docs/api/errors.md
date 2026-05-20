---
title: Errors
description: Error handling guidance for Wyrd API clients and agents.
---

# Errors

The public error catalog is not emitted as a standalone file yet. Until that generator lands, treat errors as structured data that should be shown to a developer or handed back to an agent with the original operation, card id, and request context.

## Client handling

- Preserve the original error code and message.
- Keep retries bounded and tied to idempotent operations.
- Surface policy and audit failures as decisions, not as generic transport failures.
- Link remediation steps to the card or run that caused the error.
