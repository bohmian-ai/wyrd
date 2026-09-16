# Peer-forwarding security-domain review

## Subject and boundary

- Cumulative base `861f8d86cc3f9d7e70fb59489e80f8be62afddbf`; immutable candidate `b000af7ab704f077a8a4ba3e29c2b968d47d344d` (tree `f087f7d395cd53386e4e2c6f011b31d8604fd791`). The tested source commit is `f8e887b3038651d2ba82091d642915d6b856d4d6` (tree `eb98062dc854277a248f37d86afe55beb9c3db26`); subsequent candidate changes are review documentation only.
- Scope: R3's `test-support`-gated `SilentForwardPeer`, its insertion in `OraclePeerGrpc::forward_query`, the `Bifrost` test accessor, the role-separated HTTP journey, and the adjacent selected-remote deadline. This is a security/tenancy review of the private peer boundary, not a complete cumulative task review.

## Authority and source coverage

| Boundary | Governing authority | Source inspected | Result |
|---|---|---|---|
| Private peer transport and identity | `AGENTS.md` §§2, 9, 15; `architecture/agent-rules.md` peer/tenant rules; `architecture/wyrd-security-posture.md` trust boundary and peer identity; `architecture/bifrost-design.md` distributed execution | `grpc/mod.rs::build_peer_grpc`, `grpc/peer_auth.rs::admit` and wrapper, `oracle/peer_service.rs::peer_context` and `forward_query` | PASS: mTLS router, peer workload authentication and context check precede the hook. |
| Signed ticket, tenant, fence and query deadline | `architecture/wyrd-security-posture.md` peer-ticket requirements; `architecture/bifrost-design.md` one cut/deadline/attempt; approved spec revision 9 and R3 `FIND-TASK-003-R2-3` | `oracle/forwarding.rs::forward`, `route_remote_once`, `forward_remote`, `accept`, `validate_context`, `validate_claims`; `oracle/peer_service.rs::forward_query` | PASS: hook does not alter ticket bytes or production acceptance. Normal `accept` verifies the signed ticket before planning or IO. Selected delivery uses the captured deadline and starts no successor. |
| Test seam and production isolation | `AGENTS.md` §§11, 12, 16; `architecture/agent-rules.md` testing and gate rules; `architecture/references/languages/testing-workflows.md`; R3 task | `wyrd-server/Cargo.toml`, `oracle/forwarding.rs::SilentForwardPeer`, `oracle/mod.rs`, `state.rs::silent_forward_peer_for_test`, `wyrd-testing/tests/bifrost/server/query.rs::silent_remote_oracle_delivery_yields_typed_query_timeout` | PASS: all hook storage, exports, accessors and handler invocation are `#[cfg(feature = "test-support")]`; switch starts disarmed and only the journey arms it. No public HTTP/gRPC control was introduced. |

## Security Audit

### Critical

None.

### High

None.

### Medium

None.

### Low / Defense In Depth

None required for this task. The hook intentionally parks **before** `ReadyOracleForwarder::accept` verifies the signed ticket, so this journey is not evidence that a parked envelope passed ticket validation. It is evidence for the distinct connected-but-silent transport case. An armed `test-support` build would allow an already-authenticated platform peer to occupy its private handler without a ticket decision; the source has no remote arming path, production default-feature builds omit the hook, and the fixture disarms it after the timed-out call. This does not establish a reachable production vulnerability or a remediation finding.

### Positive Controls

- Private listener composition wraps `OraclePeerGrpc` with mutual TLS, bounded transport admission, and `PeerWorkloadAuthLayer`. The layer verifies the platform Service principal and `bifrost.peer.invoke` before admitting the handler; `peer_context` fails closed if the context is absent.
- The hook's counters store only counts; no bearer, ticket, tenant identifier, query text, or result data is logged or exposed.
- The journey uses a Scribe-only ingress and actual HTTP query request, asserts a typed timeout, one selected envelope, abandonment of the parked handler, and successful subsequent forwarding. It cannot pass by accidentally selecting a local Oracle.
- `route_remote_once` bounds selected delivery (including credential acquisition and the first peer response) with the deadline captured at ingress, maps local expiry to `QueryTimeout`, and has no delivery retry.

## Verification limits

The R3 evidence records the focused forwarding unit test (2 passed), role-separated HTTP tests (2 passed), and server journey lane (12 passed) on source tree `eb98062d`. I inspected source and the recorded evidence; I did not rerun Cargo or the broad aggregate. The test's successful follow-up query is indirect evidence of ingress release, not a direct measurement of its concurrency-slot counter. The test-only pause is before ticket acceptance, while normal ticket verification and tenant binding remain on the unchanged `accept` path.

**Overall: PASS.** No material security, tenant-isolation, or production-build finding in this boundary.
