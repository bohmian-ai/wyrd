# TASK-006 R7 deployment-domain review

**Result: FAIL.** The final-source official-image proof is still missing. Reviewed immutable range `f8811ac5035c3aa165d34c38992f9889b3c9081f..f3c65147e0fd828fe5c657d2871ff80f9b3d5543` at candidate `f3c65147e0fd828fe5c657d2871ff80f9b3d5543`. `.codegraph/` is absent. No source changes or live infrastructure reruns were made.

## Boundary, authority, and source coverage

Reviewed the official image build and startup/kind scripts; the production Kubernetes guide's first rollout; the Oracle HPA metric, load, join and remote-tail assertions; and recorded R6 evidence. Authorities: approved spec revision 44 (REQ-153, REQ-160/162/163, AC-034/037), original TASK-006 and R6 remediation, `AGENTS.md` §§1, 9, 11–12, `architecture/agent-rules.md`, `architecture/operations/deployment-and-release.md`, `architecture/wyrd-design.md`, `architecture/bifrost-design.md`, and `architecture/references/languages/spec-driven-development.md`. Inspected the cumulative diff and final deltas, `docker/official/Dockerfile`, `scripts/server/test-startup.sh`, `scripts/server/test-kind-autoscale.sh`, `deploy/kubernetes/kind/{wyrd,load}.yaml`, `docs/src/content/docs/self-hosting/kubernetes-production.svx`, `crates/wyrd/wyrd-storage/src/factory/mod.rs`, and `crates/wyrd/wyrd-server/Cargo.toml`.

| Obligation | Source and verification | Result |
| --- | --- | --- |
| Official-image startup and Oracle kind journey use the final compiled server source and immutable local image ID (AC-034/037; FIND-13) | Both scripts compile `wyrd-server`, build the official Dockerfile, record source commit and image ID, and check running image identity. The recorded passes use source `abf963ccf`; subsequent `f3c65147e` changes `wyrd-storage`, a compiled `wyrd-server` dependency. | **FAIL: DEPLOY-R7-01** |
| Original kind journey scales Oracle pods on successful Oracle-executed reads, bounded to two, then checks the new Oracle and remote Scribe tail over peer mTLS (AC-037) | `test-kind-autoscale.sh` reads `oracle_query_duration_seconds_count{outcome="success"}`, checks the custom metric and HPA scale event, new pod image/address, new Oracle success count, unchanged other Oracle counts, and increased anchor tail fences. `deploy/kubernetes/kind/wyrd.yaml` caps Oracle at two. R6 records a passing run on the earlier image. | PASS for mechanism; final-source proof covered by DEPLOY-R7-01 |
| Production public ports are protected before first workload rollout (FIND-27) | `kubernetes-production.svx` orders existing NetworkPolicy and STRICT mesh policy before `wyrd-core` or `wyrd-oracle`, and creates the public edge last. Recorded fresh-namespace walk probed all serving pods before readiness, then successful HTTP/gRPC through the edge and denied direct access after readiness. | PASS within recorded walkthrough |
| Production ingress/peer boundaries remain distinct | Guide keeps edge-to-public-port mesh mTLS and separate Wyrd peer TLS on 50052; kind script checks certificate-less peer refusal. | PASS within recorded proof |

## Proposed finding

### DEPLOY-R7-01 — Final compiled server has no pinned official-image journey

- **Classification:** INCORRECT, reopened `FIND-TASK-006-13`.
- **Violated obligation:** Spec pre-release acceptance paragraph before AC-034 requires the official image built from the reviewed source, pinned and recorded by immutable local image ID; AC-034 and AC-037 require passing startup and Oracle kind journeys on it. R6's correction also explicitly requires both lanes after all image inputs are final.
- **Location and evidence:** R6 task evidence at lines 57–63 and 95–96 records startup image `sha256:45eec80f…` and kind image `sha256:f7fd2568…`, both from `abf963ccf`. Candidate commit `f3c65147e` later modifies `crates/wyrd/wyrd-storage/src/factory/mod.rs`'s compiled `pool_idle_timeout`, `build_operator`, and tests. `crates/wyrd/wyrd-server/Cargo.toml` depends directly on `wyrd-storage` with its `cloud` feature; both scripts build `wyrd-server --features cloud` and copy it into the official image. Thus the recorded binary predates the final storage behavior. The summary explicitly says both lanes still need a rerun.
- **Observable consequence:** The recorded green startup/kind results cannot establish AC-034/037 on the reviewed source. This is a proof gap, not evidence that the final binary fails.
- **Testable correction:** Run the **existing** `mise run test:server:startup` and `mise run test:server:kind` at the final compiled source. Record each lane's source commit, immutable local image ID, pass result, and the existing script check that every Wyrd container ran that ID; show any later commit changed no image input. No published image or second kind journey is needed before release.

## Verification limits

This is a static audit of source and recorded results. I did not rerun Docker, kind, a mesh, or the production walkthrough. The first-rollout probe is sampled rather than continuous; the pre-existing policies and selector order establish the boundary more directly. The production guide is an example checked on kind, not a measured production cluster. These limits do not add a separate finding.
