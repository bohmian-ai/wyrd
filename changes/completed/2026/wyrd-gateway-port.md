# Gateway V1 on current Wyrd

- Change: `wyrd-gateway-port`
- Specification: `SPEC-wyrd-gateway-port`, approved revision 1
- Completed: 2026-10-01
- Reviewed target: `7d96c30066425e0cde2290842d5801307843283d`
- Delivery reference: candidates `452a028d0`, `f70b30e3c`, and `7c2b7a3aa`

Gateway V1 is available through the current Wyrd server, identity, RBAC, SQL, audit, Bifrost, client, and SDK architecture. One in-process gateway serves OpenAI-compatible, Anthropic Messages, and Gemini GenerateContent ingress plus the supported chat, responses, embeddings, images, audio, and batch operations. There is no second listener, gateway process, virtual-key authority, or caller-selected provider secret.

Tenant administrators manage redacted environment, file, or managed credentials; managed secrets use the current encrypted storage boundary and survive rotation and restart. Invocation keeps bounded SSRF-screened endpoints, capability checks, admission, quotas, accounting, retry and fallback, streaming termination, cancellation, idempotent batch behavior, and canonical authorization audit.

Optional call and attempt capture is asynchronous, tenant-scoped, and limited to its exact Bifrost destination. Rust, Python, TypeScript, HTTP, CLI, and MCP project the same server contracts. Credential mutation remains with the administrative CLI and scoped MCP surface rather than first-class SDKs.

Acceptance closed through the credential-free gateway journey, native protocol and resilience families, managed-secret restart, under-privileged and cross-tenant refusals, audit/capture/accounting settlement, Rust/Python/TypeScript/CLI/MCP journeys, served OpenAPI verification, SQL and Skald tests, code generation, boundary checks, and docs checks. Archive migrations, checked-in OpenAPI copies, and unrelated workflow execution were deliberately omitted in favor of current owners.

Current owners and evidence:

- [Wyrd design](../../../architecture/wyrd-design.md), [security posture](../../../architecture/wyrd-security-posture.md), and [Bifrost design](../../../architecture/bifrost-design.md).
- [Gateway engine](../../../crates/wyrd/wyrd-gateway), [server gateway](../../../crates/wyrd/wyrd-server/src/components/gateway), and [shared client](../../../crates/shared/wyrd-client/src/gateway.rs).
- [Gateway journeys](../../../crates/wyrd/wyrd-testing/tests/gateway).

