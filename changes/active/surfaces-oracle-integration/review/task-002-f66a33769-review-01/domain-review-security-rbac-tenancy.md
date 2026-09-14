# Security, RBAC, and Tenancy Domain Review

## Reviewed Boundary

- Immutable comparison: base `861f8d86cc3f9d7e70fb59489e80f8be62afddbf` to candidate `f66a337698940920dca20b126c1c6c28a6390191`.
- Approved authority: `changes/active/surfaces-oracle-integration/spec.md`, revision 7.
- Task authority: `changes/active/surfaces-oracle-integration/tasks/TASK-002-converge-client-and-sdks.md`.
- Material security paths reviewed end to end: shared credential resolution; Rust, Python, and TypeScript error projection; Cards register/get/list/download/delete and local `WyrdState` loading; Bifrost register/list/describe routing, authentication, authorization, audit, and tenant selection; authenticated versus presigned HTTP transport; test-server public URL derivation; generated OpenAPI publication.
- Review posture: practical security and contract audit only. No source implementation was changed.

## Authority and Source Coverage

| Authority | Obligation examined | Principal source coverage |
|---|---|---|
| `AGENTS.md` §§2, 4, 9 | One SDK-facing Rust client, catalog-derived public errors, server-owned authz/tenancy, no client-derived tenant | `crates/shared/wyrd-client`, `sdks/wyrd-sdk-python`, `sdks/wyrd-sdk-ts`, server router and Bifrost service |
| `architecture/agent-rules.md` | Authenticate public requests, authorize before access, fail closed, preserve tenant isolation and safe credentials | `/v1` router middleware, Bifrost service, shared transports, Cards and `WyrdState` paths |
| `architecture/wyrd-security-posture.md` | Tenant from verified credentials, typed permissions, canonical decision audit, protected credentials, resolved-object authorization | server auth/router/service path and SDK credential/error surfaces |
| Spec REQ-019A, REQ-022 through REQ-025, REQ-059, INV-007, AC-002, AC-006, AC-021 | Sole shared credential, stable cross-language failures, generated Bifrost structured errors, no untrusted tenant field | client config/error mappings, PyO3/N-API projections, OpenAPI annotations and artifact |
| TASK-002 acceptance | Thin SDKs over `wyrd-client`, stable permission failures including 403, Cards/State journey behavior | cumulative diff plus referenced focused/integration tests |

## End-to-End Boundary Result

| Boundary | Result | Evidence |
|---|---|---|
| Bifrost runtime authentication, RBAC, audit, and tenant isolation | PASS | The whole `/v1` nest, including Bifrost, is default-deny authenticated at `crates/wyrd/wyrd-server/src/http/router.rs:41-60`. Register requires `bifrost_table:write`; list/describe require `bifrost_table:read`; every path uses canonical audit and `caller.data_tenant_id` at `crates/wyrd/wyrd-server/src/bifrost/service.rs:85-187,205-224,245-268`. Describe authorizes before namespace validation, preventing a namespace-validity oracle. |
| Credential forwarding | PASS | Authenticated requests reject cross-origin URLs, while presigned external transfers omit Wyrd auth headers at `crates/shared/wyrd-client/src/transport/http.rs:18-24`. The test-server bound-URL change aligns advertised storage origin with this check and introduces no production trust-boundary change. |
| Cards credential failures | FAIL | SEC-001. |
| TypeScript construction-time failures | FAIL | SEC-002. |
| Local `WyrdState` trust boundary | PASS | Bundle files are confined/canonicalized and artifact size/digest is verified before publication at `crates/shared/wyrd-client/src/state.rs:849-880,1043-1092,1346-1467`; symlink-escape and digest regressions are covered at `crates/shared/wyrd-client/src/state.rs:3218-3308`. No network credential or tenant context is introduced by offline loading. |
| Generated Bifrost failure contract | FAIL | SEC-003. |

## Verification Limits

- This was an independent static review of the immutable cumulative candidate. I did not rerun the reported verification matrix.
- The task artifact reports Rust, Python, TypeScript, codegen, boundary, journey, MCP, and focused Cards/State lanes passing. It explicitly excludes the Bifrost aggregate lane as required by the task.
- Those green lanes do not exercise the gaps below: Cards construction with an empty credential chain, TypeScript constructor/connect failure metadata, or Bifrost OpenAPI 401/403 problem responses.
- I did not treat the absence of a full Bifrost aggregate run as a finding because the task explicitly excludes it.

## Material Proposed Findings

### SEC-001 — INCORRECT — Cards collapses credential/config failures into unrelated catalog errors

- Security impact: Medium.
- Violated obligation: REQ-022 through REQ-024A, AC-002, AC-006, and the catalog-derived public-error rule require every stable failure reachable through a public SDK operation to preserve its safe code, status, title, remediation, and details rather than degrade to `WYRD_INTERNAL_500` or another subsystem identity.
- Exact location: `crates/shared/wyrd-client/src/cards/error.rs:31-49`; reachable through `crates/shared/wyrd-client/src/cards/handle.rs:155-160` and the public Python constructor at `sdks/wyrd-sdk-python/src/state/mod.rs:1464-1492`.
- Evidence: `RegistryEngineError::Client(WyrdClientError::Config)` becomes `WyrdError::Internal`; `NoCredentials` also becomes `Internal`; and `TransportDown` becomes `RegistryUnavailable`. The authoritative catalog already defines `ClientConfigInvalid`, `ClientNoCredentials` (401), and `ClientTransportDown` at `crates/wyrd-spec/src/error.rs:3182-3208,3252-3264`. Bifrost preserves those variants in a separate private mapper at `crates/shared/wyrd-client/src/bifrost/query.rs:147-160`, proving the divergent Cards projection is not imposed by the shared client type.
- Reachable consequence: a Rust or Python caller that constructs `Cards` without credentials receives `WYRD_INTERNAL_500` instead of `WYRD_CLIENT_401_NO_CREDENTIALS`. Client remediation, retry policy, authentication telemetry, and alert classification therefore treat an actionable authentication setup failure as an internal server fault. The same credential-chain failure has a different identity depending on which converged SDK capability the caller selects.
- Testable correction: give `WyrdClientError` one shared catalog projection owned by `wyrd-client`, and use it from Cards and Bifrost. Map configuration, absent credentials, and transport failures to the existing `ClientConfigInvalid`, `ClientNoCredentials`, and `ClientTransportDown` variants. Add focused Rust and public Python Cards-construction tests that remove credential sources and assert the complete safe metadata, including `WYRD_CLIENT_401_NO_CREDENTIALS` and status 401; cover representative invalid-config and transport mappings without message parsing.

### SEC-002 — INCORRECT — TypeScript connect paths discard structured Wyrd failure identity

- Security impact: Medium.
- Violated obligation: REQ-022, REQ-023, REQ-025, AC-002, AC-006, and TASK-002's TypeScript structured-error acceptance require runtime failures to surface as `WyrdError` with generated stable code and safe catalog metadata.
- Exact location: `sdks/wyrd-sdk-ts/native/src/cards.rs:34-47`, `sdks/wyrd-sdk-ts/native/src/lib.rs:338-353,403-423,990-1001`, and `sdks/wyrd-sdk-ts/wyrd/src/index.ts:965-981`.
- Evidence: `connect_cards`, `describe_table_config`, and `connect_bifrost` convert failures with `napi_error`, which constructs a generic `napi::Error` from `Display` text only. `Cards.connect` returns the native call directly and performs no projection. The implemented structured path (`projectedError`/`lifecycleValue` at `sdks/wyrd-sdk-ts/wyrd/src/index.ts:227-271`) is used for operation result envelopes, not connection failures. Thus even a catalog-preserving Rust `BifrostClientError` loses code, status, title, remediation, and details before JavaScript receives it.
- Reachable consequence: `Cards.connect`, `Bifrost.connect`, and `TableConfig.describe` callers cannot distinguish missing authentication credentials from invalid configuration or transport outage without parsing an unstable message. Security-sensitive setup and retry logic cannot branch on `WYRD_CLIENT_401_NO_CREDENTIALS`, while equivalent Python and Rust paths are intended to expose that identity.
- Testable correction: project construction/connection failures through the same catalog metadata boundary used by operation failures, and make the public TypeScript facade throw `WyrdError` without parsing display strings. Add public TypeScript tests with all credential sources absent for both Cards and Bifrost and assert `instanceof WyrdError`, `code === "WYRD_CLIENT_401_NO_CREDENTIALS"`, status 401, and nonempty title/remediation; add one representative invalid-config or transport assertion to pin the general path.

### SEC-003 — MISSING — Published Bifrost table routes omit their structured refusal responses

- Security impact: Low (contract-driven authorization handling; the runtime itself remains protected).
- Violated obligation: REQ-059 and AC-002 require the generated HTTP authority for the Bifrost table routes to include structured errors, including permission failures.
- Exact location: `crates/wyrd/wyrd-server/src/bifrost/routes.rs:22-77`, reflected in `openapi.yaml:12-82`; the route-publication test at `crates/wyrd/wyrd-server/src/http/openapi.rs:56-72` checks only path presence.
- Evidence: register and list publish only a 200 response. Describe publishes 200 plus an untyped 404 description. None publishes the runtime-reachable 401 authentication failure, 403 permission failure, or a `WyrdProblem` body for its documented validation, conflict, or availability failures. Nearby Card annotations demonstrate the repository's established typed form, for example `crates/wyrd/wyrd-server/src/components/cards/routes.rs:218-221,275-279`.
- Reachable consequence: generators and agent tooling consuming the authoritative OpenAPI document cannot model authentication/authorization refusals or reconstruct their stable Wyrd metadata. A real under-privileged request still fails closed at runtime, but generated callers see an undocumented response and may collapse it into a generic transport/internal error, breaking the cross-surface 403 contract.
- Testable correction: annotate each Bifrost table operation with the actually reachable `WyrdProblem` responses, including 401 and the appropriate read/write 403, plus its route-specific validation/conflict/not-found/availability statuses. Regenerate `openapi.yaml` and extend the OpenAPI test to assert the 401/403 response objects reference `WyrdProblem`, not merely that the paths exist.

## Security Audit

### Critical

- None.

### High

- None.

### Medium

- SEC-001: authentication/configuration failures lose their stable security identity at the Cards boundary.
- SEC-002: TypeScript connection failures lose structured Wyrd metadata at the N-API boundary.

### Low / Defense In Depth

- SEC-003: the generated Bifrost route authority omits typed authorization failures despite enforcing them at runtime.

### Positive Controls

- `/v1` authentication is applied at the group boundary, including the fallback, preventing route-existence probing before authentication.
- Bifrost authorization uses typed read/write permissions, records canonical decision audits, fails closed on audit failure, and derives the storage/catalog tenant exclusively from the authenticated caller.
- Describe authorization precedes namespace validation, avoiding an unauthorized namespace-validity oracle.
- Shared HTTP transport has an explicit same-origin credential fence and a separate credential-free path for presigned object-store transfers.
- Offline `WyrdState` loading validates bundle structure, graph closure, canonical path confinement, symlink escapes, sizes, and SHA-256 digests before exposing state.
- Secret values continue to enter the shared Rust boundary as `SecretString`; no new secret-bearing logging or debug exposure was found in the reviewed diff.

## Overall Verdict

**FAIL** — runtime Bifrost RBAC and tenant isolation remain sound, but three task-authority obligations are not met: Cards misclassifies shared credential failures, TypeScript construction paths discard structured error identity, and the generated Bifrost route contract omits typed authorization failures. Each is reachable through a first-class public surface and has a bounded, testable correction.
