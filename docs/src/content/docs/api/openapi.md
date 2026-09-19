---
title: OpenAPI
description: Generated summary of the Wyrd OpenAPI contract.
pillar: wyrd
group: Reference
order: 22
---

# OpenAPI

The repository OpenAPI document is `openapi.yaml`. Its current title is `Wyrd API` and its version is `0.0.1`.

## Routes

- `/auth/platform/callback`
- `/auth/platform/login`
- `/auth/platform/token`
- `/platform/admins`
- `/platform/admins/{principal_id}/credentials`
- `/platform/admins/{principal_id}/credentials/{credential_id}`
- `/platform/admins/{principal_id}/status`
- `/platform/oidc/connection`
- `/platform/tenants`
- `/platform/tenants/admin/credentials`
- `/platform/tenants/{tenant_id}`
- `/platform/tenants/{tenant_id}/status`
- `/v1/bifrost/tables`
- `/v1/bifrost/tables/{namespace}/{name}`
- `/v1/cards`
- `/v1/cards/by-ref`
- `/v1/cards/by-uid/{kind}/{card_uid}`
- `/v1/cards/download/init`
- `/v1/cards/{card_uid}/artifacts`
- `/v1/cards/{card_uid}/complete`
- `/v1/cards/{kind}/{space}/{name}/latest`
- `/v1/cards/{kind}/{space}/{name}/versions`
- `/v1/principals`
- `/v1/principals/{principal_id}/credentials`
- `/v1/principals/{principal_id}/credentials/{credential_id}`
- `/v1/principals/{principal_id}/revoke`
- `/v1/query`
- `/v1/query/running`
- `/v1/query/{request_id}`

## Refresh

Run `mise run codegen:check` to verify generated API metadata and `mise run docs:generate` to refresh this page.
