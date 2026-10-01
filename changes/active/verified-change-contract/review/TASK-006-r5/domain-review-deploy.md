# Deployment domain review — TASK-006 R5

**Subject:** `f8811ac5035c3aa165d34c38992f9889b3c9081f..6ce9f9bb7e2dea288a1b78081346551d896cb7a8`, approved spec revision 43. **Result: FAIL.**

## Boundary and authority coverage

| Boundary | Authority and source inspected | Result |
| --- | --- | --- |
| Official image, nginx, startup | `AGENTS.md` §§9, 11–12; `architecture/operations/deployment-and-release.md`; spec AC-034/038 and revision 43; `docker/official/Dockerfile`, `extras/entrypoint.sh`, `extras/nginx/nginx.conf.template`, `scripts/server/test-startup.sh` | Image has one Rust, Node and nginx process group; fresh mounted storage directory is created; internal gRPC is loopback. Production public-hop issue below. |
| Kubernetes development and production guides | R3 documentation obligation, R4 FIND-25/26, spec AC-034/035/037 and revisions 41–43; both `docs/src/content/docs/self-hosting/kubernetes-*.svx`, `architecture/operations/deployment-and-release.md` | Migration-only owner URL, serving roles, stable signing key, persistent anchor volume, object-store identity, peer bundle, setup, restart and Oracle HPA are documented. Production transport issue below. |
| Oracle autoscaling and peer join | Spec AC-037; `deploy/kubernetes/kind/{infra,load,migrate,wyrd}.yaml`, `scripts/server/test-kind-autoscale.sh`, `crates/shared/wyrd-client/tests/startup_image_journey.rs` | The HPA scales `WYRD_TARGET=oracle` on the per-pod successful Oracle histogram rate, and the script checks image identity, membership, execution and remote tail fencing. Load concurrency issue below. |
| Verification | R4 task evidence table at `TASK-006-R4-sql-and-deployment-closure.md:84–124`; `mise.toml` test tasks | Recorded startup, kind, peer, SQL, docs, fmt and lints are green. No real production cluster was deployed; walkthrough used kind with stated substitutions. |

## Material proposed findings

### DEP-1 — Production guide omits secure gateway-to-server transport

**Classification:** VIOLATION. **Obligation:** `architecture/operations/deployment-and-release.md:28–32` requires authenticated, encrypted gateway-to-server transport and confines plaintext listeners to loopback development or a mutually authenticated, policy-enforced local boundary. The R3 documentation obligation requires a complete production Kubernetes startup journey.

**Location and evidence:** `docs/src/content/docs/self-hosting/kubernetes-production.svx:19–30,363–371` asks for public ingress TLS certificates and routes ingress directly to Services on `8080` and `50051`; `docker/official/extras/nginx/nginx.conf.template:40–76` listens on those ports in plaintext. The guide specifies no authenticated, encrypted ingress-to-nginx transport. Its NetworkPolicy at lines 347–360 admits those ports from any source. Spec revision 43 permits public TLS termination at the edge, which is compatible with authenticated transport on the internal hop but does not provide it.

**Consequence:** Following the production guide as written leaves Wyrd tokens, API bodies and gRPC traffic on an unencrypted, unauthenticated pod-network hop from ingress to the Wyrd pod, contrary to the production network authority. The kind walkthrough does not exercise that hop.

**Testable correction:** In the existing production guide, require and show the operator's authenticated, encrypted ingress-to-pod transport for both HTTP and gRPC (for example, an enforced mesh mTLS policy), with ingress-only routing on those two ports. Keep edge public TLS and the separate `WYRD_PEER_TLS_DIR` peer mTLS. Verify the rendered manifests or walkthrough configure that hop; no new Wyrd public certificate variable or server listener is needed.

### DEP-2 — Kind load Job does not strictly cap concurrent reads

**Classification:** INCORRECT. **Obligation:** Spec AC-037 at `spec.md:1931–1933` requires bounded rate, concurrency, duration and cleanup for the local traffic generator.

**Location and evidence:** `deploy/kubernetes/kind/load.yaml:35–43` uses `[ "$(jobs -p | wc -l)" -lt "$MAX_INFLIGHT" ] && read_once &`. POSIX shell backgrounds the entire `&&` list, so `jobs -p` runs in the child shell, not the parent holding the active request jobs. A two-line `bash` and `sh` reproduction with a parent `sleep 1 &` prints `count:0` inside the backgrounded list. The check therefore sees no active parent jobs and admits every request. The 5-second curl timeout, 240-second loop and Job deadline bound duration but do not enforce maximum concurrency.

**Consequence:** On a slow server, the 14-per-second generator can admit more than eight simultaneous reads, so the local proof can create more load than the approved cap.

**Testable correction:** Keep the existing shell Job and move the `jobs -p` check into the parent shell, then background only `read_once`. A small shell check with deliberately held jobs should show no more than eight requests start; rerun the normal `test:server:kind` lane to retain HPA and peer proof. No new load harness is needed.

## Verification limits

- The passing kind run proves an Oracle HPA event and a remote Scribe tail fence. The append-to-read interval is not independently shown to precede publication, so the recorded counters do not by themselves establish that the returned rows were unpublished; this is a proof limit, not a separate finding against AC-037's wording.
- The production guide walkthrough used a fresh kind database, local RustFS credentials, reduced resources and a test HPA target. It proves startup and metric wiring, not production network encryption or performance sizing.
