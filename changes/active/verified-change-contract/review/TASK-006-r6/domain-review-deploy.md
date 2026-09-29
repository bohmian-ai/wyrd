# TASK-006 R6 deployment-domain review

**Result: FAIL.** One production runbook ordering defect remains. Reviewed immutable subject `f8811ac5035c3aa165d34c38992f9889b3c9081f..3f93886489a1d95be1a3eb2382fe059bb9856988` at `3f93886489a1d95be1a3eb2382fe059bb9856988`; no source edits. `.codegraph/` is absent.

## Boundary and authority

Deployment boundary: official image and release profile, one-off database migration, development and production Kubernetes guides, Istio edge/mesh policy, peer port, Oracle autoscaling, kind load Job, and retired Bifrost examples. Authority: approved change spec revision 44, original TASK-006 and R5 remediation, `AGENTS.md` §§1, 9, 11–12, `architecture/agent-rules.md`, `architecture/operations/deployment-and-release.md` (especially network/TLS and database lifecycle), `architecture/wyrd-design.md`, `architecture/bifrost-design.md`, and spec-driven development reference.

Source coverage: `docs/src/content/docs/self-hosting/{kubernetes-development,kubernetes-production,local-development}.svx`; `deploy/kubernetes/kind/{load,infra,migrate,wyrd}.yaml`; `scripts/server/{test-kind-autoscale,test-startup}.sh`; `docker/official/{Dockerfile,extras/entrypoint.sh}`; root `Cargo.toml`, `mise.toml`, and `.github/workflows/release.yml`. I compared the cumulative subject and R5 delta, prior R5 verdict and task, and R5 implementation evidence. This is a static review; I did not rerun Docker, kind, a real mesh, or the guide walk.

## Assessment

| Obligation | Source and recorded proof | Result |
| --- | --- | --- |
| Official image, one-off owner migration, two serving roles, persistence and restart | Image recipe, startup journey, development guide, R5 `test:server:startup` and `test:sql` evidence | PASS within recorded proof |
| Verify Postgres server identity for owner/app/platform connections | Production guide `?sslmode=verify-full`, mounted `PGSSLROOTCERT`; R5 valid/wrong CA and host walk | PASS |
| Protect public ingress-to-pod HTTP and gRPC for the whole production startup journey | Istio `PeerAuthentication`, `AuthorizationPolicy`, and NetworkPolicy are defined; R5 walk proves protected steady state, but guide starts and waits for both workloads before installing those policies | FAIL: DEPLOY-R6-1 |
| Dedicated peer TLS, runtime peer address and separate peer port | Production pod templates and mesh port exclusions; prior peer and kind journeys | PASS within recorded proof |
| Oracle HPA on successful executed reads and bounded kind load | Oracle histogram adapter/HPA; corrected parent-shell in-flight count; recorded HPA 1→2 and held-read cap | PASS within recorded proof |
| Published server uses revision 44 `dist` profile, local image lanes keep `release` | `Cargo.toml` profile and release workflow `target/dist/wyrd-server`; `mise.toml` local build | PASS |
| Retired Bifrost manifests | Deleted `deploy/kubernetes/bifrost/` and old YAML check; current kind proof remains | PASS |

## Proposed finding

### DEPLOY-R6-1 — Production pods run before their ingress protection is installed

- **Classification:** INCORRECT / security boundary.
- **Violated obligation:** `architecture/operations/deployment-and-release.md:28-32,44` requires plaintext listeners to stay within an authenticated, policy-enforced boundary, with authenticated and encrypted gateway-to-server transport and restricted data-plane ingress. R5 FIND-27 requires the production guide to enforce this hop.
- **Location and evidence:** `docs/src/content/docs/self-hosting/kubernetes-production.svx:234-242,421-426` explicitly applies and waits for the core and Oracle workloads first, then applies `wyrd-netpol.yaml` and `wyrd-mesh.yaml`. Their pod templates expose nginx's plaintext `8080` and `50051` ports (`:307-310`, `:388-391`). The enforcing NetworkPolicy, STRICT `PeerAuthentication`, and edge-only `AuthorizationPolicy` appear at `:440-478` but do not exist during those rollout waits. The reported guide walk tested access after all manifests were applied, so it cannot close this interval.
- **Observable consequence:** During a fresh install or replacement into a namespace without a preexisting equivalent policy, an ordinary in-cluster workload can connect to the pod's plaintext public ports before the mesh and network policies are installed. Istio's default permissive posture does not supply the guide's edge-only identity rule.
- **Testable correction:** Apply `wyrd-netpol.yaml` and `wyrd-mesh.yaml` before creating either serving workload; leave the edge route until workloads are ready. Re-run the guide sequence from a fresh namespace and prove a probe pod cannot reach `8080` or `50051` while the first Wyrd pod is starting, then prove public HTTP and gRPC through the edge after rollout. This changes only the runbook order and reuses the existing policies.

## Verification limits

The R5 evidence records a successful local kind mesh walkthrough, wrong-certificate refusals, Oracle scale-up, and an eight-request maximum under held reads. The reviewed deployment guide has no automated test that enforces the initial policy-application order; the recorded mesh test observes only steady state. No new live checks were run for this review.
