# TASK-006 R5 peer domain review

**Subject:** immutable cumulative `f8811ac5035c3aa165d34c38992f9889b3c9081f..6ce9f9bb7e2dea288a1b78081346551d896cb7a8`. **Result: PASS.**

## Boundary and authority coverage

| Boundary | Authority | Source and verification inspected | Result |
|---|---|---|---|
| Peer-mode boot, listener readiness, fenced membership, discovery, and departure | Approved spec revision 43 REQ-159/160/162/163, AC-036; original TASK-006; R3 FIND-22; `AGENTS.md` §§2, 6, 9, 11; `architecture/agent-rules.md`; `architecture/bifrost-design.md`; spec-driven-development reference | `wyrd-server` config, boot, `app/server.rs`, `app/peer_plane.rs`, `state.rs`, Redux cluster registry, `tail_discovery.rs`, process `peer_join_and_remote_query`; supplied `test:server:peer` 9/9 | PASS |
| Peer identity and receiver-side authorization | Spec REQ-160/161, AC-036; Bifrost tenant and terminal-result rules | `oracle/peer.rs`, Scribe tail RPC, mounted private gRPC admission and peer security journey; supplied peer lane and kind no-certificate refusal | PASS |
| Oracle scale-out and remote live Scribe read | Revision 43 AC-037 and R4 FIND-26 | Existing kind script and manifests: one fixed `all` anchor, Oracle Deployment/HPA, Downward-API address, per-pod Oracle success metric, post-scale read and tail-fence checks; supplied kind pass | PASS |
| Public gRPC versus private peer TLS | Revision 43 REQ-153/160/165 | `config.rs` removes public gRPC certificate inputs and production peer-mode requirement; `app/server.rs` composes the internal public gRPC router without TLS while the private router loads the peer bundle; production config regression test | PASS |

## Judgment

Peer-mode roles are reserved unready. The peer socket is bound before serving, and `BoundServer::run` polls the private serving future before activating the Scribe and Oracle fences. Failed activation withdraws readiness; shutdown deactivates exact fences. Discovery reads ready, fresh role leases from the existing registry. The process join journey exercises a held peer socket, live join, remote Oracle lease, cross-pod Scribe tail, and abrupt departure without a partial result. The kind journey now checks that HPA scales the Oracle workload, the new Oracle advertises its own pod IP, executes the successful read, and acquires the anchor's tail over the peer plane. Public gRPC certificate removal affects the loopback public listener, not the private mTLS router or certificate checks.

## Findings and verification limits

No material peer-domain findings. I inspected the cumulative source and R4 delta but did not rerun live Kubernetes or Postgres lanes. The supplied evidence reports `test:server:peer` 9/9 and `test:server:kind` passing. The kind proof detects a certificate-less peer refusal and a positive connection with the cluster leaf; the separate peer security journey covers wrong identity, unrelated CA, stale and cross-tenant contexts. The kind read proves remote tail acquisition through the anchor's counter but does not prove the tail rows were unpublished at that instant; this limit was already recorded and is not a new R4 defect.
