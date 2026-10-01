# TASK-006 R7 peer and distributed-read domain review

**Immutable subject:** `f8811ac5035c3aa165d34c38992f9889b3c9081f..f3c65147e0fd828fe5c657d2871ff80f9b3d5543`, approved spec revision 44. **Result: PASS** for the peer domain.

| Boundary | Authority and source coverage | Judgment |
| --- | --- | --- |
| Private identity, admission and readiness | Spec REQ-159–162 and AC-036; `AGENTS.md` §§2, 9, 11; `architecture/agent-rules.md`, `architecture/wyrd-security-posture.md` peer section, and `architecture/bifrost-design.md`; `wyrd-server/src/app/{server,peer_plane}.rs`, `boot/mod.rs`, `grpc/{peer_auth,scribe_tail}.rs`, `oracle/peer_service.rs`, Redux `cluster/mod.rs`. Peer roles remain reserved unready until the private listener serves; listener departure withdraws readiness. The private mTLS router and receiver-side context/fence checks are unchanged since R6. | PASS |
| Discovery and Oracle/Scribe remote read | Spec REQ-161–163 and AC-036/037; Redux Oracle peer and Scribe tail code, `deploy/kubernetes/kind/wyrd.yaml`, `scripts/server/test-kind-autoscale.sh`, and `peer_network/{join,security}.rs`. The kind journey checks the scaled Oracle's injected address, certificate-less refusal, cluster-leaf admission, its own successful Oracle read, and the anchor's increased Scribe tail fence. The separate process journey covers joining and failed remote work. No R6 commit changes those paths. | PASS within recorded proof limits |
| R6 storage change touching remote reads | Spec REQ-163 and Bifrost shared-store contract; `wyrd-storage/src/factory/mod.rs`, including `build_operator` and `finish_op`; R6 storage tests and evidence. Both Oracle and Scribe still use the same configured shared object store. The new per-operator HTTP client changes connection reuse, not peer identity, selection, authorization, or data-cut semantics; it adds no retry behind Bifrost writes. | PASS |

**Material findings:** None in this domain.

**Verification limits:** This is a source review; I did not rerun kind, peer, or cloud storage tests. The R6 evidence records the kind Oracle 1→2 scale and mTLS read on an image from `abf963ccf`; the final `f3c65147e` changes the linked storage code after that image, so those image journeys need a final-source rerun for AC-034/037. The R6 peer lane was not rerun, but no peer or admission code changed. The tail-fence counter proves remote acquisition; it does not establish that the returned rows were unpublished at that moment, a previously recorded limitation.
