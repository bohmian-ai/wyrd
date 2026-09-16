# Peer security domain review

## Subject and boundary

Base `861f8d86cc3f9d7e70fb59489e80f8be62afddbf`; immutable candidate `24b8e78d7420321eb8fe936b3045fcbd3bc1d26b` (tree `6a3ec967ab094a42256b44ce908abb080360e466`); last tested source `d6de890d83f269eab834fa323f3ebe31f1adc573` (tree `d8a727b5846f24bac7b55bbc786bcb391c55cb03`). Reviewed the cumulative private Oracle forwarding boundary, with particular attention to the R3 `SilentForwardPeer` hook and R4 import/doc edits. No CodeGraph index exists.

## Authority and source coverage

| Boundary | Authority | Source and proof inspected | Result |
|---|---|---|---|
| Replica authentication | `architecture/wyrd-security-posture.md` (mutual TLS, verified workload identity); `AGENTS.md` §2/§9; `architecture/bifrost-design.md` Oracle query path | `grpc/mod.rs:build_peer_grpc` wraps `OraclePeerGrpc` in mutual TLS, transport admission, and `PeerWorkloadAuthLayer`; `grpc/peer_auth.rs:admit` verifies the configured control-tenant Service principal and inserts `AuthenticatedPeerContext`; `peer_service.rs:forward_query` refuses absent context before reading its envelope. | PASS |
| Signed, tenant-bound query authority | Spec revision 9 REQ-013, REQ-053A, INV-007, AC-006; security posture's signed peer ticket rule | `forwarding.rs:forward` uses an already authorized context; `ForwardingAttempt::into_claims` signs exact audience, fence, context, request, and ingress deadline; `peer_authority.rs:verify_forward_query` checks signature, audience, fence, expiry and replay before returning claims; `forwarding.rs:accept` executes only the verified claims. No tenant is selected from an unsigned payload. | PASS |
| Test-only silent peer and cancellation | R3/R4 packets and `AGENTS.md` test-support boundary | `forwarding.rs:SilentForwardPeer`, `oracle/mod.rs` export, `state.rs` accessor and the `peer_service.rs` hold call are each `#[cfg(feature = "test-support")]`; no feature or dependency was added by R4. The hook follows workload authentication and precedes ticket acceptance only in that test feature. The role-separated HTTP journey arms it, checks one arrival and abandonment, disarms it, and proves a following query succeeds. | PASS |
| R4 source delta | R4 requirement to preserve auth/tenant/cancellation behavior | `git diff b000af7ab..d6de890d8` changes only `forwarding.rs` and `state.rs` imports and rustdoc; no peer-service, authority, feature-graph, or runtime branch changes. | PASS |

## Security Audit

### Critical

None.

### High

None.

### Medium

None.

### Low / Defense In Depth

None required by this task. The test-support hook can intentionally park an authenticated peer before ticket verification when armed; that is limited to explicitly test-enabled builds and was introduced to prove cancellation. Do not deploy a test-support binary with the switch armed. This is a test-harness operational limit, not a finding against the approved production boundary.

### Positive Controls

- Peer workload identity is established before the private handler and is not reconstructed from request metadata.
- The ticket is purpose-bound, signed, fenced, expiring, and replay-checked; its verified context is the query's tenant authority.
- R4 did not change the authentication, tenant, ticket, timeout, or peer-handler logic.

## Verification limits and result

I inspected source, the cumulative and R4 diffs, the role-separated journey, and the committed same-tree verification record. I did not rerun the broad or live-cloud lanes; those are outside this security boundary. No material security finding is proposed. **Overall: PASS.**
