---
title: Errors
description: Error handling guidance for Wyrd API clients and agents.
---

# Errors

Wyrd errors are structured problem details. Agents and SDK clients must preserve
`code`, `status`, `detail`, `remediation`, `details`, and trace/request ids.

## Client transport codes

| Code | Status | Title | Remediation |
|---|---:|---|---|
| `WYRD_CLIENT_400_CONFIG_INVALID` | 400 | Client transport configuration failed validation | [Transports troubleshooting](/transports/troubleshooting/#config-validation) |
| `WYRD_CLIENT_400_TRANSPORT_FEATURE_DISABLED` | 400 | Selected transport variant is not compiled into this build | [Transports troubleshooting](/transports/troubleshooting/#feature-disabled) |
| `WYRD_CLIENT_413_PAYLOAD_TOO_LARGE` | 413 | Flush body exceeded the server's accepted payload size | [Transports troubleshooting](/transports/troubleshooting/#payload-too-large) |
| `WYRD_CLIENT_503_TRANSPORT_DOWN` | 503 | Client transport is unavailable | [Transports troubleshooting](/transports/troubleshooting/#transport-down) |
| `WYRD_CLIENT_504_FLUSH_TIMEOUT` | 504 | Flush exceeded per-call timeout | [Transports troubleshooting](/transports/troubleshooting/#flush-timeout) |

See [Error Codes](/reference/errors/) for the current catalog.
