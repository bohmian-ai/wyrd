# TASK-001 security and tenancy domain review

**Subject:** `d1ec13200d332745af2fed8069a21d5b5c39cb47..f1f1d5ebd264e8f9ac861ec79c6340da44a7a1a8` (HEAD checked at candidate). Approved `spec.md` revision 3 and original `TASK-001-unified-scribe-live-query.md`; prior R1 findings treated as hypotheses. Review only; no source edits.

**Overall: PASS.** No material security or tenancy finding survived source inspection.

## Boundary and authority coverage

| Boundary | Authority | Source traced | Result |
| --- | --- | --- | --- |
| Client query to tenant and table authority | `AGENTS.md` §§2, 9; `architecture/wyrd-security-posture.md` security principles; spec REQ-001/INV-002 | Oracle query setup and `discover_live_routes` in `vala-bifrost-redux/src/oracle/mod.rs`; existing query gRPC auth path | PASS: discovery receives the already resolved tenant/table cut; no client-selected source mode. |
| Oracle to Scribe active-stream listing | Security posture peer boundary; `architecture/bifrost-design.md` live source/tenant authority; spec REQ-002, REQ-004, INV-002 | `wyrd-server/src/oracle/tail_discovery.rs`, `grpc/scribe_tail.rs`, `oracle/tail_authority.rs`; `vala-bifrost-redux/src/scribe/tail_rpc.rs::TonicTailReadTransport`, `FetchLiveTailService::list_active_streams` | PASS: mutual TLS/workload bearer, service principal, verified tenant binding, signed single-use audience/node/epoch/table/query ticket, and table-scoped local listing precede metadata return. Production boot installs the authority (`wyrd-server/src/boot/mod.rs:980-1003`); router wraps the service in peer auth (`grpc/mod.rs:390-426`). |
| Oracle to Scribe live fragment | Security posture peer boundary; spec REQ-003/REQ-004/INV-002 | `wyrd-server/src/oracle/peer_service.rs::execute_scribe_fragment`; `vala-bifrost-redux/src/oracle/peer.rs::assignment_authority_digest_for`; `oracle/follower.rs::authenticated_preflight`, `validate_assignment`, `ScribeTailResolver::resolve` | PASS: peer ticket, local role fence, tenant/table/permission and plan digests, exact assignment digest, encoded scan set, hidden tenant column, schema fingerprint, projection and predicate closure are checked before live source IO. Local source is tenant/table bound; tenant tripwire remains in the plan. |
| Error to result terminal | Spec REQ-004, AC-005; `architecture/bifrost-design.md` failure semantics; prior FIND-TASK-001-1/-2 | `scribe/tail_rpc.rs::tonic_error`; `oracle/tail_discovery.rs::discover`; `oracle/mod.rs::discover_live_routes`; `peer_service.rs::scribe_start_error`/`scribe_stream_error`; `oracle/dispatcher.rs::live_execution_status_error` | PASS: listing `Unauthenticated`/`PermissionDenied` becomes authorization failure, invalid binding becomes binding failure, and Oracle fails them; only ready-Scribe `Unavailable` or stale identity after retry degrades. Fragment capacity, schema, integrity, tenant, and execution faults fail; only genuine pre-row source loss can degrade. |

I also checked `architecture/agent-rules.md`, `architecture/references/languages/spec-driven-development.md`, the applicable security foundation, cumulative security-sensitive diff, and R1 remediation evidence. The changed manifests/lockfiles do not introduce a dependency in this boundary. No new SQL construction, command execution, path interpretation, redirect, secret material, or configuration privilege surface appeared in the traced live-read changes.

## Prior finding closure and verification limits

- **R1 FIND-TASK-001-1:** Closed. `tonic_error` retains gRPC status classes, discovery forwards them, and Oracle degrades only `Unavailable` or exhausted stale-identity retry. The real-server `live_query_terminal_failure_matrix` drives a missing-ticket refusal through Scribe's verifier and confirms Failed, alongside an outage that confirms Degraded.
- **R1 FIND-TASK-001-2:** Closed in this domain. `FollowerResolutionError` preserves source loss, capacity, and fault; Scribe maps non-source faults to terminal/capacity and in-stream local faults to terminal. The journey checks ticket/capacity refusal and client rejection. Schema mismatch and staged decode remain unit-level proof, as recorded in R1; they use the same Scribe terminal mapping, but no real-server corruption/schema-change journey exists.
- **Open question, not finding:** `ScribeTailGrpc::authenticated_tenant` returns gRPC `Unavailable` if its token verifier is absent; the listing transport would classify that as source loss. Production boot always supplies the verifier and installs the tail authority, so I found no reachable production path for this state. If production composition changes, preserve auth-backend failure as fatal rather than availability loss.

## Security Audit

### Critical

None.

### High

None.

### Medium

None.

### Low / Defense In Depth

None required by this task.

### Positive Controls

- Signed peer assignment and ticket are verified before Scribe source IO.
- Tenant comes from verified credentials and signed assignment; hidden tenant projection and runtime tripwire remain enforced.
- Trust-boundary refusals and local integrity faults fail the query instead of becoming an accepted Degraded result.
- Test-only fault hooks are gated by `test-support`/`cfg(test)` and production boot installs the real authority.
