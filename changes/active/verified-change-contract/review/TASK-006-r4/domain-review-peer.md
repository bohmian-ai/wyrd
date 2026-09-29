# TASK-006 R4 peer and concurrency domain review

**Subject:** cumulative `f8811ac5035c3aa165d34c38992f9889b3c9081f..58cabb529b93da366959db796ac3b596a6c6c1e6`; R3 correction `2fbe90cd880936d8e4f25173839e4cc4fa18f0b4..58cabb529b93da366959db796ac3b596a6c6c1e6`. **Result: PASS.**

## Boundary and authority coverage

| Boundary | Authority | Source and proof inspected | Result |
|---|---|---|---|
| Peer boot, fenced membership, discovery, and shutdown | Approved spec revision 41, REQ-159/160/162/163 and AC-036; original TASK-006 and R3 FIND-22; `AGENTS.md` §§2, 6, 9, 11; `architecture/agent-rules.md`; `architecture/bifrost-design.md`; spec-driven and analytical reliability references | `wyrd-server` boot, app server, state, health and peer plane; Redux cluster registry; `vala-sql` cluster-node registration/heartbeat/list; process join test | PASS |
| Remote Oracle and Scribe tail work, peer loss | Spec REQ-161/162, AC-036; Bifrost query and terminal-result rules | Cumulative peer/dispatcher/tail changes, R3 role activation, process join journey's real remote analytical lease, fused tail read, and departure assertions | PASS within this boundary |

## FIND-22 closure

`vala-sql` registration reserves an exact role fence with `ready=false`; live discovery selects only fresh `ready=true` rows. Peer-mode boot leaves Scribe and Oracle reserved through WAL recovery and Oracle reconciliation (`boot/mod.rs`). `WyrdServer::bind` must acquire the private socket before `BoundServer::run`; bind failure therefore leaves the roles unready. The supervised `BifrostPeer` task polls the prebound tonic listener before activating Scribe and Oracle fences (`app/server.rs`); their heartbeats retain the unready bit until activation (`state.rs`, Redux cluster registry). Activation failure withdraws the bit and ends supervision, while normal shutdown deactivates the exact fences before transport drain. The revised `peer_join_and_remote_query` test holds the second process's peer socket, checks that its failed start never enters the first process's ready membership, then verifies join, remote Oracle work, cross-pod tail, and departure without partial results. This closes the prior reachable reserve-to-listener race.

## Findings and verification limits

No material findings in this domain. The supplied evidence reports `test:server:peer` 9/9, the exact `peer_join_and_remote_query` command, server and Oracle journeys, and the kind scale/join/remote-read proof passing. I inspected source and diff but did not rerun the expensive live lanes. The join journey proves bind failure and steady-state remote work; it does not simulate every possible database failure during activation, which does not leave a successfully serving process after supervisor failure.
