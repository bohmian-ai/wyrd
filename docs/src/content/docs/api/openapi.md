---
title: HTTP API
description: Where a Wyrd server serves its OpenAPI document and how to use it.
pillar: wyrd
section: Reference
order: 22
status: runnable
---

Every Wyrd server serves its own OpenAPI 3.1 document at `GET /openapi.json`, without authentication. It is built from the routes that server mounts, so it always matches the version you are calling.

```bash
curl -s $WYRD_SERVER_URL/openapi.json > openapi.json
```

Load the file or the URL into any OpenAPI tool, such as Swagger UI, Redoc, Postman, or a client generator.

Each operation declares its authentication, request and response bodies, and the Wyrd error codes it can return. Errors use `application/problem+json`; see [Error codes](/api/errors/).

The Rust, Python, and TypeScript SDKs call this same API, so you only need it for another language or for raw HTTP.
