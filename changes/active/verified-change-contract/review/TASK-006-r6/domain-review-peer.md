# TASK-006 R6 peer and concurrency domain review

**Subject:** cumulative `f8811ac5035c3aa165d34c38992f9889b3c9081f..3f93886489a1d95be1a3eb2382fe059bb9856988` (spec revision 44). **Result: PASS.**

| Boundary | Authority and source inspected | Result |
|---|---|---|
| Peer admission, discovery, and ready membership | Spec REQ-159–163 and AC-036; `AGENTS.md` §§2, 9, 11; `architecture/agent-rules.md`; `architecture/bifrost-design.md`; `wyrd-server/src/app/{server,peer_plane}.rs`, `boot/mod.rs`, Redux `cluster/mod.rs`, and the R4 peer journey. Peer roles stay unready until the private listener is polled into service, then join the fenced registry; readiness falls when that listener stops. R5 changes neither this code nor the private mTLS dial. | PASS |
| Remote Scribe tail and Oracle execution | Spec REQ-160–162, AC-036/037; `scripts/server/test-kind-autoscale.sh:237–283`, `deploy/kubernetes/kind/wyrd.yaml:43–44,114–115`, and prior peer security journey. The kind proof checks the new Oracle's distinct advertised pod address, refusal without a client certificate, admission with the cluster leaf, its own successful read counter, idle other Oracles, and an increased anchor Scribe tail fence. | PASS |
| Oracle-only autoscaling and bounded load | Spec revision 42 AC-037; `deploy/kubernetes/kind/{infra,wyrd,load}.yaml`, `scripts/server/test-kind-autoscale.sh:205–233`. The HPA consumes the per-pod rate of successful Oracle executions and is capped at two. R5 changes the shell load loop so the parent counts its background reads before launching another; `RATE`, `MAX_INFLIGHT`, duration, curl timeout, Job deadline, and Job deletion bound the test. | PASS |
| R5 TLS and readiness changes adjacent to peer mode | Spec REQ-153/160; `wyrd-client/src/transport/grpc.rs`, `wyrd-server/src/components/health/mod.rs`, R5 diff. Native trust roots apply only to the public SDK HTTPS endpoint; private peer transport retains its dedicated CA and identity checks. Readiness adds diagnostic logs without changing its decision. | PASS |

**Findings:** None.

**Verification limit:** This is a source audit; I did not rerun kind or the process peer lane. R5 evidence reports `test:server:kind` passing with Oracle HPA 1→2 and a held-read check peaking at eight concurrent reads. The separate `test:server:peer` lane was not rerun in R5, but its implementation and admission path are unchanged. The kind tail-fence increase proves remote acquisition, not that rows remained unpublished at read time; that preexisting limitation is recorded and does not violate the approved acceptance criterion.
