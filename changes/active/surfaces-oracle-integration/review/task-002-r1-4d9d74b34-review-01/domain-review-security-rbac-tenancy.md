# Security, RBAC, and Tenancy Domain Review

## Reviewed Boundary

- Immutable comparison: base `861f8d86cc3f9d7e70fb59489e80f8be62afddbf` to candidate `4d9d74b34803b3f27d55f740a5b40ffbe968b306` in `/tmp/wyrd-task-002-r1-review-4d9d74b34`.
- Approved authority: `changes/active/surfaces-oracle-integration/spec.md`, revision 7; original `TASK-002-converge-client-and-sdks.md`; prior verdict and validated findings; and `TASK-002-R1-close-review-findings.md`.
- Reviewed security boundary: credential resolution and projection; Rust, Python, and TypeScript construction failures; authenticated HTTP and presigned transfer separation; Bifrost HTTP/MCP/gRPC input bounds; public authentication and authorization ordering; tenant propagation into catalog/query owners; structured refusal publication; and preservation of canonical authorization audit behavior.
- Review posture: practical, review-only security audit. No implementation source was modified.

## Authority and Source Coverage

| Authority | Security obligation examined | Principal source coverage | Result |
|---|---|---|---|
| `AGENTS.md` §§2, 4, 7-9, 11-12; `architecture/agent-rules.md` | One credential/error authority, protected public boundaries, server-derived tenant, fail-closed authz/audit, safe secret handling | `crates/shared/wyrd-client/src/{error,auth,config}.rs`, `transport/{credential,http,grpc}.rs`, `cards/{config,error,handle}.rs`, server router/services | PASS |
| `architecture/wyrd-security-posture.md` | Verified credentials select identity and tenant; typed permissions; no secret exposure; canonical decision audit; query object authorization before IO | `/v1` and `/mcp` middleware, Bifrost catalog service, query authority/service, Oracle forwarding/context | PASS |
| `architecture/wyrd-design.md`; `architecture/bifrost-design.md`; `architecture/wyrd-doctrine.mdx` | Server-owned durable behavior, shared Bifrost facade, tenant-qualified query/catalog access, bounded public inputs | shared facade/transports, Bifrost/query routes, MCP adapter, protobuf conversion, forwarding | PASS |
| `architecture/references/{architecture/patterns,doctrine/architecture-constraints,languages/errors,languages/agent-harness,domain/olap-serving}.md` | Cross-surface stable errors, no SDK authz bypass, typed agent inputs, same tenant and permission contract on HTTP/MCP/SDK | Rust/Python/TypeScript error projections, OpenAPI annotations, MCP request conversion, server services | PASS |
| Spec REQ-019A, REQ-021-REQ-025, REQ-054, REQ-059, INV-007, AC-002, AC-006; original and remediation task acceptance | Single explicit credential, catalog-backed cross-language failures, typed permission/refusal contracts, no caller-selected tenancy | cumulative base-to-candidate source plus focused closure tests and implementation evidence | PASS |

## Prior-Finding Closure

| Prior finding | Closure evidence | Result |
|---|---|---|
| `FIND-TASK-002-1` / `SEC-001` | `WyrdClientError` has one `From` projection at `crates/shared/wyrd-client/src/error.rs:89-112`; Cards delegates to it at `cards/error.rs:31-59`; Bifrost delegates to it from its public error conversion. Missing credentials remain `WYRD_CLIENT_401_NO_CREDENTIALS`, invalid config remains 400, and transport unavailability remains 503. | PASS |
| `FIND-TASK-002-2` / `SEC-002` | Closed N-API connection results carry `NativeWyrdError` at `sdks/wyrd-sdk-ts/native/src/cards.rs:24-61` and `native/src/lib.rs:173-189,390-427,461-500`; `nativeHandle` projects them to public `WyrdError` at `wyrd/src/index.ts:274-292`, used by Cards, Bifrost, and table description. No display-string parsing is used. | PASS |
| `FIND-TASK-002-3` / `SEC-003` | All seven public Bifrost table/query/lifecycle operations publish typed 401, 403, route-specific, and default `WyrdProblem` responses in `crates/wyrd/wyrd-server/src/{bifrost,query}/routes.rs`; `http/openapi.rs:74-116` verifies the complete operation set. Generated `openapi.yaml` contains the matching schemas. | PASS |

## End-to-End Security Result

| Boundary | Result | Evidence |
|---|---|---|
| Credential handling and exposure | PASS | Explicit credentials enter Rust as `SecretString`; credential enums and client config redact `Debug`; token exchange drops refresh tokens; the shared HTTP transport attaches Wyrd credentials only after its same-origin check and uses a credential-free client for presigned external transfers. No credential, bearer, API key, or private key was added to production logs, responses, examples, or generated artifacts. |
| Public authentication | PASS | `crates/wyrd/wyrd-server/src/http/router.rs:41-73` applies default-deny authentication to the complete `/v1` group, its fallback, and `/mcp` before handlers run. Health and token exchange remain the intentional unauthenticated surfaces. |
| Bifrost catalog RBAC and tenancy | PASS | `crates/wyrd/wyrd-server/src/bifrost/service.rs:85-187,205-268` authorizes typed write/read permissions before catalog access and consistently supplies `caller.data_tenant_id`; describe authorizes before namespace validation. Allowed and denied decisions use the canonical audit path and fail closed on audit failure. |
| Query RBAC, object scope, and lifecycle IDOR resistance | PASS | `crates/wyrd/wyrd-server/src/query/service.rs:74-180,221-347` admits only callers with the read capability, derives `AuthorizedQueryContext` from the verified caller, delegates resolved-table authorization to Oracle before execution, audits object denials, and scopes list/get/cancel calls by both `caller.data_tenant_id` and typed `RequestId`. A request ID alone cannot cross tenants. |
| External input validation | PASS | HTTP and SDK queries share `BifrostQueryRequest::validate` with deadline `1..=u32::MAX`; protobuf over-range values remain invalid rather than wrapping; TypeScript rejects non-integral/non-finite values into the catalog path; MCP uses a closed schema with bounded SQL, rows, bytes, and deadline. Path identities are parsed into typed request/table identities before owner dispatch. |
| Structured refusal metadata | PASS | Rust owns the catalog conversion; Python and TypeScript project that metadata without an alternate error catalog; OpenAPI now documents runtime-reachable authentication, authorization, validation, conflict, not-found, availability, and default pre-stream failures. Error projection contains safe catalog fields and no credential material. |
| Injection and unsafe deserialization | PASS | The reviewed changes add no command construction, SQL interpolation into an execution owner, template execution, or unsafe deserializer. Query SQL remains an explicitly bounded SELECT-only server contract with resolved-object authorization; JSON/protobuf/Arrow inputs cross bounded typed decoders. |
| Dependencies and deployment configuration | PASS | No new dependency or credential/deployment fallback was introduced by remediation. PyO3 activation was narrowed behind the explicit Python feature; Rust and TypeScript client cones remain outside that foreign-runtime boundary. |

## Verification Limits

- Static inspection covered the complete cumulative range and the remediation delta, including every caller of the corrected credential and TypeScript connection projections and the server owners behind the seven OpenAPI operations.
- The remediation record reports the focused Rust credential test, TypeScript connect-error test, OpenAPI refusal test, Python public Cards test, MCP deadline tests, language integrations, codegen, client-tier, PyO3-scope, and wheel checks passing.
- An independent rerun of the focused Rust credential test could not start because another process held the shared Cargo build-directory lock; it was cancelled without changing the candidate. This does not block the review because the test source directly exercises all three mappings, recorded verification is available, and the correction is independently established by source tracing.
- Candidate `4d9d74b34803b3f27d55f740a5b40ffbe968b306` remained unchanged throughout review; the immutable worktree was clean before and after inspection.

## Material Proposed Findings

None. No reachable security, RBAC, tenancy, credential-exposure, injection, or trust-boundary defect remained within the approved task.

## Security Audit

### Critical

- None.

### High

- None.

### Medium

- None.

### Low / Defense In Depth

- None required for task acceptance.

### Positive Controls

- Default-deny authentication encloses both `/v1` and `/mcp`, including unknown-route fallbacks.
- Effective tenant identity originates from the verified `Caller` and is repeated at catalog, query-context, and lifecycle-owner boundaries.
- Typed read/write permissions are checked before resource access; query object authorization occurs after logical resolution but before admission and source IO.
- Authorization decisions use the canonical audit path, with denials and lifecycle allows failing closed when audit cannot commit; Oracle query-read acceptance preserves its approved WAL-first exception.
- Shared HTTP transport separates same-origin authenticated requests from credential-free presigned object-store requests.
- Secret-bearing Rust values are redacted, refresh tokens are not cached, and remediation did not add secret-bearing telemetry or generated data.
- Deadline and agent result bounds are validated at HTTP, protobuf, MCP, Python, and TypeScript boundaries without narrowing wraparound.

## Overall Verdict

**PASS** — the remediation closes all three prior security-domain findings. Authentication, RBAC, tenant derivation and propagation, authorization audit, credential protection, external input bounds, and structured refusal behavior remain fail-closed and aligned across the reviewed public surfaces. The validated security finding ledger is empty.
