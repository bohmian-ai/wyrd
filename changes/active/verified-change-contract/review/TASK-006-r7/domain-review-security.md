# TASK-006 R7 — security and trust domain review

## Subject and boundary

Immutable cumulative range `f8811ac5035c3aa165d34c38992f9889b3c9081f..f3c65147e0fd828fe5c657d2871ff80f9b3d5543`, approved specification revision 44, original TASK-006, R1–R6 remediation and verdicts. `.codegraph/` is absent. The candidate remained HEAD during this review. I inspected source and recorded proof; I did not run a cluster, Postgres, or a peer test. No product source was changed.

Authority: `AGENTS.md` §§2, 9, 11–12; `architecture/agent-rules.md`; `architecture/wyrd-security-posture.md` (trust boundaries, peer identity, credential handling); `architecture/operations/deployment-and-release.md` (authenticated edge hop, role-separated Postgres, external TLS); specification REQ-153, REQ-155, REQ-159–163 and AC-034–037; and R6 FIND-TASK-006-27/31.

| Boundary | Source and evidence | Result |
| --- | --- | --- |
| Database role bootstrap and secret delivery | `crates/wyrd/wyrd-sql/bootstrap/roles.sql:12–28` gets serving passwords from environment when the Kubernetes guides invoke it. `kubernetes-development.svx:69–97` and `kubernetes-production.svx:104–145` use `PGPASSWORD` for `psql` and pipe password-bearing URLs to `kubectl --from-file`; the spawned commands carry no password in argv. R6 records a walkthrough of 1,074 `psql`/`kubectl` argument lists, successful role logins, wrong-CA/host refusals, migration, and mounted serving secrets. The owner URL is mounted only in the migration Job (`kubernetes-production.svx:186–208`); the serving manifests mount only app and platform-admin URLs (`:304–308,385–389`). | PASS |
| Production first-rollout edge isolation | `kubernetes-production.svx:237–245,423–437` now applies its existing policies before creating the core or Oracle workload and applies the edge route last. The policies select the workload label and restrict 8080/50051 to the edge pod plus its mesh principal (`:451–488`); the workload templates carry the selected label (`:285,366`). R6 reports six blocked startup probes, blocked ordinary and label-spoofing pods, successful edge HTTP/gRPC and observed edge-to-pod mTLS. | PASS |
| Dedicated peer identity and receiver authorization | `wyrd-server/src/config.rs:1201–1274` loads the CA/leaf/key only in peer mode; `wyrd-tonic/src/server/mod.rs:35–100` requires a client certificate; `wyrd-server/src/grpc/mod.rs:155–177,251–276` checks the leaf's fixed `wyrd-peer` DNS identity before polling a body; `vala-bifrost-redux/src/oracle/dispatcher.rs:2295–2405` dials with that CA, name and client identity. `wyrd-server/src/oracle/peer_service.rs` and `peer_authority.rs` check receiver-owned context and audit refusals. The existing peer journey covers anonymous, unrelated-CA, expired, wrong-name and forged-context callers. R6 changed no peer admission code. | PASS within the recorded peer-lane limit |
| Public/peer separation and database TLS | The production gateway terminates public TLS and Istio mTLS protects the internal 8080/50051 hop; port 50052 stays on its separate Wyrd peer mTLS listener (`kubernetes-production.svx:450–488,499–556`). Production DSNs retain `sslmode=verify-full` and a mounted CA (`:97–123,198–208,304–308`). R6 evidence records wrong-CA/hostname refusals and a successful correctly configured migration. | PASS |

## Material proposed findings

None. The R6 corrections use the existing password, Kubernetes Secret and policy mechanisms and preserve the distinct public, database and peer trust boundaries. I found no reachable task-bound security regression in the cumulative candidate.

## Verification limits

The policy walkthrough probed each startup pod and public port once, so it establishes the documented policy order and observed denial, not every instant of CNI propagation. I did not independently rerun the 1,074-command argument capture, production walkthrough, or `test:server:peer`; those are recorded implementation evidence. The peer code and transport admission did not change in the R6 increment. The local development page's literal throwaway passwords and test fixture arguments are outside R6's Kubernetes-credential correction and do not expose the production credentials reviewed here.

## Result

**PASS.** No material security or trust finding proposed.
