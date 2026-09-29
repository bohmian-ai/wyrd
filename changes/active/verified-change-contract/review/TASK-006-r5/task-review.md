# TASK-006 R5 task implementation review

## Subject and method

Repository: `/home/thorrester/Documents/GitHub/wyrd-vcc-t006`. Immutable cumulative range: `f8811ac5035c3aa165d34c38992f9889b3c9081f..6ce9f9bb7e2dea288a1b78081346551d896cb7a8`. Approved specification: `changes/active/verified-change-contract/spec.md`, revision 43. Original task: `tasks/TASK-006-continuous-eval-verifier.md`. Read the prior R1–R4 verdicts, finding ledgers and remediation packets, complete cumulative diff inventory, R4 increment, applicable repository and deployment authorities, and recorded verification. `.codegraph/` is absent. This is static review; I did not rerun Docker, kind or Postgres. The candidate remained at the stated commit; `git diff --check` passed.

## Acceptance matrix

| Obligation | Implementation evidence | Verification evidence | Result |
| --- | --- | --- | --- |
| Original Eval execution, sampling, best-effort post-ACK activation, terminal mapping, canonical results, trace, media, and non-goals | `vala-eval`, `wyrd-server/src/verification/eval.rs`, `wyrd-testing/tests/bifrost/server/eval_verification.rs`; prior R1–R4 reviews inspect the cumulative path | Recorded 22/22 server, 28/28 Oracle, SQL and focused tests; prior Eval review | PASS within recorded limits |
| Official single image, local immutable-ID proof, server/Node/nginx startup, public gRPC and durable restart (REQ-153–155, AC-034, prior FIND-13) | `docker/official`, `startup_image_journey.rs`, `test-startup.sh`, SDK URL resolution; revision-43 config removes public gRPC certificate inputs | Recorded startup pass against image `sha256:deeb0879…` from `a4a82dfc5`; prior language journeys | PASS |
| External Postgres, two serving roles, separate owner migration, fail-closed readiness (REQ-156–158, AC-035, FIND-17–20) | `SqlStore::migration_lease`, `schema_check.rs` exact tenant expression, `wyrd-server migrate`, role bootstrap | Recorded exact policy mutation and competing migrator tests, `test:sql`, startup, pool-boundary check | PASS |
| Standalone local calls, peer mTLS, receiver-side validation, registry discovery and readiness (REQ-159–164, AC-036, FIND-14–16/21–22) | Peer listener, admission, authority, membership, and tail paths; no ticket protocol | Recorded peer 9/9 and kind peer join/no-cert checks | PASS |
| Original one-kind proof scales Oracle on successful Oracle-executed reads and proves remote Scribe work (AC-037, FIND-26) | Existing `test-kind-autoscale.sh`, Oracle Deployment/HPA and Prometheus adapter; counters check new Oracle execution and anchor tail fences | Recorded kind pass: Oracle HPA 1→2 at 13.111/s on pinned image; no direct scale | PASS |
| Delete policy hook and retain real permission/audit behavior (AC-038) | Route/hook removed in cumulative diff | Recorded startup route/OpenAPI and auth journeys | PASS |
| Three startup guides, including required inputs, migration, credentials, peer certificates and production autoscaling (R3 documentation task, FIND-25/26) | Local, development-Kubernetes and production-Kubernetes pages; local kind image branch corrected | Recorded local/dev/prod guide walks and `docs:check` | **FAIL: TASKREV-R5-01** |
| R4 style findings 23–24 and explicit non-goals | Top-level peer transport imports and imported activation error type; no second kind journey, new metric, outbox, policy parser or peer ticket | Source and diff inspection; recorded fmt/lints | PASS |

## Proposed finding

### TASKREV-R5-01 — INCORRECT — production guide leaves the gateway hop unprotected

**Obligation.** R3 requires a complete production Kubernetes startup and credentialing guide. `architecture/operations/deployment-and-release.md:27-32` requires every external connection to use TLS and gateway-to-server traffic to be authenticated and encrypted; plaintext listeners are confined to loopback development or a mutually authenticated, policy-enforced local transport boundary.

**Location and evidence.** `docs/src/content/docs/self-hosting/kubernetes-production.svx:340-369` terminates public TLS at ingress/load balancer, then routes traffic to the Wyrd Services on `8080` and `50051`. Those services expose nginx's plaintext HTTP/gRPC listeners; `docker/official/extras/nginx/nginx.conf.template` has no TLS listener. The guide provides no transport encryption or authentication for ingress-to-pod traffic. Its NetworkPolicy also admits both public ports from all sources, rather than establishing the permitted local boundary. Revision 43 correctly makes the internal Rust gRPC listener plaintext and does not change this hop.

**Observable consequence.** Following the production guide can expose client tokens, requests and responses across the cluster network after edge TLS termination; the claimed production topology does not meet its governing deployment authority. The reported kind walkthrough proves pods boot and peer mTLS works, but it does not prove the public gateway transport boundary.

**Required testable correction.** Document a concrete authenticated, encrypted ingress-to-nginx transport arrangement for both HTTP and public gRPC using an existing Kubernetes/ingress or mesh facility, including the matching access restriction. Keep public TLS terminated at the edge and retain the approved plaintext nginx-to-Rust loopback and separate peer mTLS boundaries. Verify the production example's rendered routing/transport configuration and make a real client request through it; `docs:check` alone is insufficient. If satisfying the deployment authority requires changing the approved public edge contract or Wyrd's image/protocol, return this as a spec decision rather than inventing a new mechanism in remediation.

## Result and limits

**FAIL.** The R4 code and Oracle autoscaling corrections close the known bounded findings on the inspected paths; the production guide remains incomplete at its public gateway transport boundary. No other material task finding is proposed. Prior findings 1–26 otherwise appear closed within their recorded verification limits. This review relied on recorded lane results and did not repeat full environment journeys; the R4 guide walk used kind-only substitutes for workload identity, resource sizes and production HPA thresholds.
