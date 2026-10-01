# TASK-006-R2 peer and concurrency domain review

**Subject:** `f500ea38bc749f36b3ee8d88893dcf7c0161435c..2fbe90cd880936d8e4f25173839e4cc4fa18f0b4` (cumulative TASK-006 base `f8811ac5035c3aa165d34c38992f9889b3c9081f`). **Result: FAIL.** Source was not changed during this review.

## Boundary and authority coverage

| Boundary | Authority | Source inspected | Result |
|---|---|---|---|
| Runtime address, fenced registration, discovery, and readiness | Approved spec rev 40 REQ-159/160/162/163 and AC-036; `TASK-006-R2-continuous-eval-closure.md` startup ordering; `AGENTS.md` §§2, 6, 9, 11; `architecture/agent-rules.md`; `architecture/bifrost-design.md` cluster and failure invariants | `wyrd-server` config, boot, app/server and peer-plane; `vala-bifrost-redux` cluster; `vala-sql` cluster_nodes; process peer join journey | **FAIL**: PEER-1 |
| Remote Oracle forwarding and terminal stream behavior | Spec REQ-161/162, AC-036; Bifrost design Oracle and explicit non-goals; analytical operations reliability | `wyrd-server` forwarding, peer service and authority; Redux dispatcher, analytical transport, follower; peer network join/transport journeys | PASS within reviewed concurrency boundary |
| Scribe tail discovery and receiver fence | Spec REQ-159/161/162, AC-036; Bifrost design live tail and query visibility | server tail discovery and gRPC service; Redux tail RPC/fence; peer network join/security journeys | PASS within reviewed concurrency boundary |
| Restarted Forge scheduler | Spec startup restart/persistence outcome; Bifrost design lease and recovery; analytical operations reliability | Forge scheduler, server boot scheduler owner, `vala-sql` scheduler acquire/renew | PASS: durable node identity supplies the owner; SQL retains exact fence on same live owner and advances it after expiry. |

`architecture/references/languages/spec-driven-development.md` controls authority order. The full relevant source range and the prior R2 review/remediation task were consulted; the implementation summary was not used as proof.

## Material finding

### PEER-1 — INCORRECT: ready membership precedes private listener binding

**Obligation.** Approved spec REQ-160 requires invalid peer inputs to prevent ready membership; REQ-162 requires a reachable published private address; AC-036 requires live discovery and remote work. The remediation task's startup sequence explicitly says to bind the mTLS peer listener before activating roles.

**Evidence.** `crates/wyrd/wyrd-server/src/boot/mod.rs:881` activates the Scribe row after recovery; `:1891` activates the Oracle row after reconciliation. Both happen during state composition. `crates/wyrd/wyrd-server/src/app/server.rs:376` binds the private TCP listener only later in `WyrdServer::bind`, and `:723` starts serving it only in `BoundServer::run`. `vala-sql/src/queries/cluster_nodes.rs:209-211` selects any fresh `ready=true` row, independent of that process's `/readyz`. `RegistryTailStreamDiscovery::discover` refreshes and dials those rows; `ReadyOracleForwarder::query` also selects from the membership snapshot. The local `peer_plane` readiness gate does not govern these foreign selectors.

**Observable consequence.** During each peer startup, other replicas can discover a ready role whose advertised socket has not yet been bound. Tail discovery fails a strict fused query; Oracle selection can fail or route around the newly advertised peer. If bind then fails, the false ready row persists until shutdown or heartbeat expiry. This is a reachable race on normal second-replica startup, not an optional hardening case.

**Testable correction.** Keep both roles reserved and `ready=false` through recovery and listener bind. Publish their exact fences only after the mTLS listener is bound and serving; on bind/start failure leave or mark those fences unready. Reuse the existing reserved-role lifecycle and `peer_plane` status instead of adding another registry or readiness state. A two-process test should hold or fail the second process before peer bind/serve and assert its role never appears in the first process's ready member snapshot; after serving starts, assert discovery and remote work.

## Verification limits

The supplied run reports `test:server:peer` 9/9, server and Oracle journeys, and the kind autoscaling proof passing. Those demonstrate steady-state join, remote query/tail, loss handling, and eventual membership. The join test waits for ready membership after process start and never observes the reserve-to-bind interval. No focused command was rerun for this read-only review.
