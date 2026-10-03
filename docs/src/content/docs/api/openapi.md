---
title: OpenAPI
description: Where the live Wyrd OpenAPI contract is served and how to consume it.
pillar: wyrd
group: Reference
order: 22
---

# OpenAPI

Wyrd serves one OpenAPI 3.1 document, and it is generated at runtime from the
server's own route handlers rather than checked into the repository. A running
server publishes it unauthenticated at `GET /openapi.json`, so the contract you
read is always the contract that deployment serves.

## Consume it

```bash
curl -s http://localhost:8080/openapi.json > openapi.json
```

Any Swagger- or OpenAPI-compatible tool takes that URL or file directly:
point Swagger UI, Redoc, or an `openapi-generator` client at it, or load it
into Postman or Bruno to explore the API interactively.

Every operation declares its authentication, typed request and response
bodies, error media types, and the stable Wyrd error codes it can return.
JSON API operations refuse with `application/problem+json`. The four OAuth
2.0 form endpoints (`/auth/token`, `/auth/platform/token`,
`/auth/device_authorization`, and `/auth/revoke`) refuse with the RFC 6749
section 5.2 JSON (`error`, optional `error_description`), which carries no
Wyrd error code. The browser sign-in endpoints (`/auth/authorize`,
`/auth/callback`, and `/auth/device`) answer with redirects and HTML pages
as well; each operation declares its own responses.
