# Security domain review — TASK-001

**Subject:** `d1ec13200d332745af2fed8069a21d5b5c39cb47..f9115fbbf6b6f116cf5ec5fe5582a9543107955a` (candidate inspected at `f9115fbbf6b6f116cf5ec5fe5582a9543107955a`). **Result: FAIL.**

## Boundary and authority coverage

| Boundary | Authority | Source traced |
| --- | --- | --- |
| Public query admission and tenant-bound table selection | `AGENTS.md` §§2, 9, 11; `architecture/wyrd-security-posture.md` security principles and tenant isolation; spec INV-002 | `query/routes.rs`, `oracle/mod.rs`, `oracle/tail_discovery.rs` |
| Authenticated Scribe discovery | Security posture peer and credential boundaries; spec REQ-002, REQ-004, INV-002; task streaming handoff | `grpc/scribe_tail.rs`, `oracle/tail_authority.rs`, `scribe/tail_rpc.rs`, `oracle/tail_discovery.rs`, `oracle/mod.rs` |
| Signed live fragment execution and tenant tripwire | Security posture peer and tenant isolation; spec REQ-003, REQ-004, INV-002 | `oracle/live.rs`, `oracle/dispatcher.rs`, `oracle/peer_service.rs`, `oracle/follower.rs`, `scribe/tail_rpc.rs` |
| Client terminal acceptance | Spec REQ-001, REQ-004, AC-005; `AGENTS.md` §11 | `oracle/mod.rs`, `wyrd-client/src/bifrost/query.rs`, distributed Oracle journey |

Also read the approved rev-3 spec, original task, `architecture/agent-rules.md`, `architecture/wyrd-design.md`, `architecture/bifrost-design.md`, and `architecture/references/languages/spec-driven-development.md`. The cumulative diff was inspected for changed security and contract surfaces. No manifest or lockfile change introduced a dependency risk. The changed skill commit is outside the task's product behavior.

## Security Audit

### Critical

None.

### High

None.

### Medium

- **SEC-001 — Discovery authorization failures become an accepted Degraded result.** **Violated obligation:** spec REQ-002 and REQ-004 require a security or tenant fault during ready-Scribe listing to fail; INV-002 and the security posture require fail-closed peer authority. **Producer:** `grpc/scribe_tail.rs:66-76,103-132` returns `Unauthenticated` or `PermissionDenied` for an invalid workload credential, tenant mismatch, or signed ticket. **Propagation:** `scribe/tail_rpc.rs:328-332` converts every tonic status into `TailReadError::State`; `oracle/tail_discovery.rs:161-177` converts the resulting error into another `State`; `oracle/mod.rs:3521-3533` treats *every* non-deadline listing error as `listing_lost`, which is emitted as a Degraded terminal. A mismatched tenant or rejected ticket can therefore produce a query whose published rows are accepted as a best-effort result when the required outcome is Failed. In a scheduled verifier this can permit a judgment on a security-failed observation read. **Testable correction:** keep the authenticated RPC's closed status class through discovery and have the Oracle query owner degrade only genuine listing availability loss. Authentication, authorization, binding/schema/protocol, audit, and tenant failures must terminate Failed. Add one real-server listing rejection journey asserting Failed and no accepted partial rows; the current journey at `wyrd-testing/tests/bifrost/oracle/distributed.rs:1674-1677` injects only generic listing unavailability and does not exercise this branch.

### Low / Defense In Depth

None within this task.

### Positive Controls

- The Scribe discovery handler derives tenant from the verified workload token, checks the request tenant, and verifies a signed, expiring, replay-protected ticket before listing.
- Scribe fragment execution checks the peer identity, exact signed assignment digest, tenant binding, node incarnation and fence before provider IO; live frame validation requires a matching footer.
- Live fragment peer-ticket rejection and tenant-invariant errors are typed as terminal failures rather than eligible pre-row availability loss.

## Verification limits

The task records passing format, lint, codegen, Bifrost, journey, and full-gate results; this review did not rerun those lanes. Existing Oracle journey coverage injects a generic listing outage and fragment ticket rejection, but no discovery authentication, authorization, tenant, or signed-ticket rejection. This report identifies the reachable status-to-terminal path in source; an end-to-end proof of the correction is still required.
