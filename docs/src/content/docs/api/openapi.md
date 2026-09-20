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
bodies, `application/problem+json` error media type, and the stable Wyrd
error codes it can return.
