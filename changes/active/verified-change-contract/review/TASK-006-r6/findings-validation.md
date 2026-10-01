# TASK-006 R6 independent findings validation

## Subject and method

Reviewed the immutable cumulative range `f8811ac5035c3aa165d34c38992f9889b3c9081f..3f93886489a1d95be1a3eb2382fe059bb9856988` against approved spec revision 44, the original TASK-006, R1–R5 verdicts and remediation, `AGENTS.md`, `architecture/agent-rules.md`, the applicable security and deployment authorities, the complete cumulative diff, R5 evidence, and all eight R6 Wave 1 reports. `.codegraph/` is absent. The candidate remained `HEAD` during validation. This is a source and evidence review; no product source or infrastructure lane was changed or run.

The R5 production walkthrough and required image journeys have real callers and are part of the approved TASK-006 startup work. I traced the concrete startup/kind build scripts, readiness publisher and its Postgres/storage probes, production manifest application order, development-to-production bootstrap reference, `roles.sql` password input, and the credential-consuming `psql`/`kubectl` commands. The corrections below reuse existing tests and Kubernetes policies; none needs a new API, transport, abstraction, or specification decision.

## Disposition of every Wave 1 proposal

| Source | Disposition | Independent validation |
| --- | --- | --- |
| `TASKREV-R6-01` | **REVISED**; reopen `FIND-TASK-006-13` | The startup and kind scripts compile the current checkout and label the image with `git rev-parse HEAD`; the recorded image was built at `dcf1321aa`. Later `1303c20a8` changes production readiness probe code, which `readiness_loop` calls through `compute_snapshot` on every serving process. The recorded runs therefore do not prove the final server binary. Rerun the existing two lanes; no new test or published image is needed. |
| `SR-R6-1`, `SEC-R6-01`, `DEPLOY-R6-1` | **REVISED, merged**; reopen `FIND-TASK-006-27` | The production guide applies and waits for both serving workloads before creating its already authored `NetworkPolicy`, STRICT `PeerAuthentication`, and edge-only `AuthorizationPolicy`. The steady-state walk cannot prove the initial rollout. Applying those existing policies before workloads is the complete correction. |
| `SR-R6-2`, `SEC-R6-02` | **REVISED, merged**; new `FIND-TASK-006-31` | Both Kubernetes guides put database passwords in `kubectl create secret --from-literal` argv. The production guide also gives password-bearing URLs to `psql "$url"`; the development bootstrap passes two role passwords through `psql --set`. The production guide links to that same bootstrap, and `roles.sql` consumes those variables. The secret-handling rule applies to all of these reachable commands, not only the three examples cited by Wave 1. |
| `domain-review-data.md`, `domain-review-peer.md`, `domain-review-eval.md`, `domain-review-client.md` | **Validated empty** | Their inspected data, peer, Eval, and client boundaries supply no competing finding or correction. The unrerun peer lane alone is not a finding because the R5 increment did not change peer admission. |

## Deduplicated retained finding ledger

### FIND-TASK-006-13 — REVISED / INCORRECT — final source lacks required image-journey proof

- **Sources:** `TASKREV-R6-01`; reopens the R3 image-provenance finding under approved revision 41.
- **Obligation:** Spec revision 44's pre-release paragraph before AC-034 requires the official image built from the **reviewed commit**, pinned by its immutable local image ID, for AC-034 and AC-037. R5's remediation also requires `test:server:startup` and `test:server:kind`.
- **Location and evidence:** `scripts/server/test-startup.sh:101-112` and `scripts/server/test-kind-autoscale.sh:110-145` build, label, record, and pin images. R5's evidence records image `sha256:4f1b645befa38935a1bfac34dad51da5fd51199c59e93a14d0c7afb112d0f94a` from `dcf1321aa`. Commit `1303c20a8` later changes `crates/wyrd/wyrd-server/src/components/health/mod.rs:328-438`, part of the compiled binary; `readiness_loop` calls the changed probes through `compute_snapshot`. The candidate's code parent is `895477da1`. The statement that every later commit changed only documentation is false.
- **Observable consequence:** The passing image journeys establish startup and Oracle autoscaling for an earlier server binary. They do not establish those required journeys for the final candidate; they do not show that the candidate fails.
- **Decision-complete correction:** Rerun the **existing** startup and kind lanes on the final source commit, and record each lane's source commit, immutable image ID, and existing checks that every Wyrd container uses the pinned image. Do not publish an image or add a second kind journey. The first-release published-digest obligation remains at release.
- **Focused closure proof:** `mise run test:server:startup` and `mise run test:server:kind` pass from the final source; their recorded provenance matches that source and the running containers.

### FIND-TASK-006-27 — REVISED / INCORRECT — production public ports are briefly unprotected

- **Sources:** `SR-R6-1`, `SEC-R6-01`, `DEPLOY-R6-1`; reopens R5's production gateway-hop finding at a distinct rollout step.
- **Obligation:** `architecture/operations/deployment-and-release.md:28-32,44` and R5 FIND-27 require authenticated, encrypted edge-to-pod transport and restricted public ingress throughout the production startup journey.
- **Location and evidence:** `docs/src/content/docs/self-hosting/kubernetes-production.svx:234-242,421-426` tells operators to apply and wait for `wyrd-core` and `wyrd-oracle` before applying `wyrd-netpol.yaml` and `wyrd-mesh.yaml`. The workloads expose nginx's plaintext `8080` and `50051` ports. The necessary NetworkPolicy, STRICT `PeerAuthentication`, and edge-only `AuthorizationPolicy` are already specified at lines 433-479 but do not exist during the waits. No pre-existing equivalent policy is required by the prerequisites.
- **Observable consequence:** An ordinary in-cluster workload can reach the pods' plaintext public listeners during first rollout, before the guide's edge-only identity and network restrictions exist. The R5 walk tested only the protected final state.
- **Decision-complete correction:** In the existing production guide, apply the already authored network and mesh policies **before** creating either serving workload; then roll out core and Oracle and apply the edge route. Their label selectors work before pods exist. Keep their policy contents, public TLS edge, separate peer mTLS, and Oracle topology. No new mesh component or Wyrd TLS setting is needed.
- **Focused closure proof:** Follow the guide from a fresh namespace: while a Wyrd pod is starting, an ordinary pod cannot reach either public port; after rollout, HTTP and gRPC work through the edge. Run `mise run docs:check`.

### FIND-TASK-006-31 — REVISED / VIOLATION — documented database passwords enter command arguments

- **Sources:** `SR-R6-2`, `SEC-R6-02`; new finding, distinct from R5 FIND-29's root credential in a `kubectl exec` request.
- **Obligation:** `architecture/wyrd-security-posture.md:374-375` forbids credentials in command arguments and requires restrictive permissions for file-mounted secrets. The R3 documentation task requires a usable credential setup for development and production Kubernetes.
- **Location and evidence:** `docs/src/content/docs/self-hosting/kubernetes-development.svx:71-74,83-87` passes role passwords via `psql --set` and three database URLs via `kubectl --from-literal`; the owner URL in its `psql` invocation also contains a password placeholder. `kubernetes-production.svx:105-119,133-136` constructs password-bearing verified DSNs, gives them to `psql "$url"`, and supplies them via `kubectl --from-literal`. Production line 112 reuses the development bootstrap. `crates/wyrd/wyrd-sql/bootstrap/roles.sql:4-7,11-15` documents and consumes the password variables; it need not change its SQL behavior.
- **Observable consequence:** Process inspection or operator-machine command auditing can retain owner, app, or platform-admin passwords during normal setup, even though the resulting Kubernetes Secrets are correct. R5's FIND-29 proof covers only the Wyrd platform credential in `kubectl exec`.
- **Decision-complete correction:** Keep the same three roles, `verify-full` production DSNs, CA, and Kubernetes Secret keys, but remove password values from executable arguments in **both** guides. Use libpq's existing environment/credential-file channel for `psql` connection passwords; feed the bootstrap's two role-password variables through `psql` input or environment instead of `--set`; and create Kubernetes Secrets through the already mentioned secret-provider integration or restrictive local input files passed with `kubectl --from-file`. If using local files, prescribe restrictive permissions and removal after Secret creation. Update the stale `roles.sql` invocation comment if the documented method changes. No new Wyrd credential API is needed.
- **Focused closure proof:** Follow both examples and inspect the spawned `psql` and `kubectl` argv: no database password appears. Verify the three production logins still use certificate-verified TLS and the migration Job and serving pods receive their intended Secret values. Run `mise run docs:check`.

## Result

Three bounded findings remain: reopened `FIND-TASK-006-13` and `FIND-TASK-006-27`, and new `FIND-TASK-006-31`. The other R5 finding closures stand on the inspected paths. Each correction is inside approved revision 44, so the validated ledger supports `FIX_REQUIRED`, without a spec revision.
