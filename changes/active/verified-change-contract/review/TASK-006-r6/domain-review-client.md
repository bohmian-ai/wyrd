# TASK-006 R6 client transport domain review

## Subject and boundary

- Cumulative candidate: `f8811ac5035c3aa165d34c38992f9889b3c9081f..3f93886489a1d95be1a3eb2382fe059bb9856988`.
- Authority: approved `spec.md` revision 44, especially REQ-153, REQ-160, REQ-165 and AC-034; original TASK-006 and R1–R5 remediation; `AGENTS.md`, `architecture/agent-rules.md`, `architecture/wyrd-design.md`, and `architecture/bifrost-design.md`.
- Reviewed the shared client endpoint resolution, HTTP and gRPC TLS builders, Rust/Python/TypeScript constructor projections, transport config/schema/example deletions, and production guide's public gateway routes. The peer transport uses a separate, explicitly pinned CA and client identity.

## Coverage and result

| Boundary | Source and diff evidence | Verification evidence | Result |
| --- | --- | --- | --- |
| One server URL selects HTTP and public gRPC for all three SDKs; override survives | `wyrd-client/src/config.rs` derives gRPC from the effective server URL; Python `src/client.rs` and TypeScript `native/src/client.rs` call the shared Rust constructor | R5 startup image journey and SDK constructor tests; source inspection | PASS |
| HTTPS gRPC verifies a public edge certificate | `wyrd-client/src/transport/grpc.rs` enables tonic native roots with `ClientTlsConfig::new().with_native_roots()`; `wyrd-tonic` enables `tls-native-roots`; no insecure verifier or client certificate is introduced | Focused `https_endpoint_trusts_the_platform_roots` test recorded passing; production guide walkthrough recorded successful SDK gRPC through TLS edge | PASS |
| HTTPS HTTP and relative upload/download use shared transport | `wyrd-client/src/transport/http.rs` builds reqwest's platform-verifying client; public SDKs compose this Rust client | R5 guide walkthrough and recorded startup journey; source inspection | PASS |
| Ignored public transport certificate fields are removed without changing peer mTLS | `GrpcConfig.tls` and `HttpConfig.tls` removed from shared config and golden schemas; misleading `transport_secrets` examples deleted; peer transport retains its pinned CA and mutual TLS path | Recorded `codegen:check`, `wyrd-client --test transport` 68/68, Rust examples, Python example smoke 7/7 | PASS |
| Production public routing matches client URL convention | Guide places HTTP on `443` and HTTPS gRPC on `50051` for both anchor and Oracle hostnames; Istio HTTPRoutes route gRPC to named backend gRPC ports | R5 kind guide walk recorded successful HTTP and gRPC through the edge | PASS |

## Verification limits

This is a read-only review. I did not rerun the image, kind, SDK, or TLS tests. The committed guide walkthrough used local kind substitutions rather than a production cluster. The Python `transport_grpc.py` and `transport_http.py` examples still serialize historical queue/auth fields that are not accepted by the current transport configs; that behavior predates R5, and those examples are not used by the reviewed startup guide or required client construction path, so it is not a task finding.

## Material proposed findings

None.

## Overall result

**PASS** for the client transport and public TLS boundary reviewed here.
