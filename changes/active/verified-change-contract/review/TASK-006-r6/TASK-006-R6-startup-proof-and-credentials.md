---
id: TASK-006-R6
kind: remediation
status: ready
spec: SPEC-verified-change-contract
spec_revision: 44
requirements: [REQ-153, REQ-155, AC-034, AC-037]
depends_on: [TASK-006-R5]
parent_task: TASK-006
remediates: [FIND-TASK-006-13, FIND-TASK-006-27, FIND-TASK-006-31]
---

# TASK-006 R6 — Close final image proof and startup credential exposure

## Authority and subject

- Approved authority: [specification revision 44](../../spec.md), especially its pre-release image proof before AC-034 and AC-037.
- Original task: [TASK-006](../../tasks/TASK-006-continuous-eval-verifier.md).
- Prior remediation: [R5 task](../TASK-006-r5/TASK-006-R5-production-startup-closure.md).
- Independent diagnosis: [R6 verdict](verdict.md) and [validated findings](findings-validation.md).
- Immutable reviewed range: `f8811ac5035c3aa165d34c38992f9889b3c9081f..3f93886489a1d95be1a3eb2382fe059bb9856988`; final code parent `895477da1ced790ded96e21a83e2cf31c3185ca1`.

These are bounded corrections to the accepted R5 behavior. Preserve continuous Eval, the official image recipe, owner-only migration, distinct serving roles, verified database TLS, dedicated peer mTLS, and the single Oracle autoscaling journey.

## FIND-TASK-006-13 — Image journeys ran before the final server code

**Diagnosis.** The startup and kind scripts build the official image from the checkout and record its commit and immutable image ID. R5 records both journeys using `sha256:4f1b645befa38935a1bfac34dad51da5fd51199c59e93a14d0c7afb112d0f94a`, built at `dcf1321aa`. Later commit `1303c20a8` changed compiled server readiness code. The final candidate therefore has no official-image startup or kind run from its final server source. The passing runs prove an earlier binary, not that this one fails.

**Correction and acceptance.** Once all image inputs are final, rerun the **existing** `mise run test:server:startup` and `mise run test:server:kind` lanes. Record each source commit, immutable local image ID and the scripts' check that every Wyrd container runs that image. Record any later commits and show they changed no image input. Keep the release-only published-digest journey deferred to first release; do not add another kind test, test harness or image mechanism.

## FIND-TASK-006-27 — Production protection starts after the serving pods

**Diagnosis.** `kubernetes-production.svx` applies and waits for core and Oracle before applying the guide's `NetworkPolicy`, STRICT `PeerAuthentication` and edge-only `AuthorizationPolicy`. During those waits, nginx's plaintext `8080` and `50051` listeners exist without the guide's restrictions. R5's walk tested blocked direct traffic only after policies were installed, so it missed the first-rollout gap.

**Correction and acceptance.** Apply the guide's **existing** network and mesh policies before creating either serving workload; apply the edge route after workloads are ready. Their selectors do not require existing pods. In a fresh-namespace walkthrough, an ordinary pod cannot reach either public port while Wyrd starts, and HTTP and gRPC work through the edge afterward. Keep public TLS at the edge, separate peer mTLS on `50052`, and the Oracle topology. Run `mise run docs:check`.

## FIND-TASK-006-31 — Database passwords appear in setup arguments

**Diagnosis.** Both Kubernetes guides expand password-bearing DSNs into `kubectl create secret --from-literal` arguments. The production guide also runs `psql "$url"` for its three TLS connection checks; the development role bootstrap passes the app and platform-admin passwords through `psql --set`. The production guide reuses that bootstrap. These commands expose database passwords to process inspection and command auditing on the operator machine, contrary to `architecture/wyrd-security-posture.md`. R5's credential proof covered a different `kubectl exec` command.

**Correction and acceptance.** Keep the three roles, the production `sslmode=verify-full` DSNs and provider CA, and the existing Kubernetes Secret names and keys. In **both** guides, use libpq's existing environment or credential-file channel for `psql` connection passwords and supply the two role-bootstrap passwords through `psql` input or environment instead of `--set`. Create Kubernetes Secrets from the already documented secret provider or temporary local files passed via `kubectl --from-file`; if files are used, require restrictive permissions and deletion after Secret creation. Update the stale invocation comment in `bootstrap/roles.sql` if necessary. A walkthrough must show no database password in spawned `psql` or `kubectl` arguments while all three logins still verify server TLS, migration succeeds, and serving pods receive the intended credentials. Run `mise run docs:check`.

## Constraints and broader verification

Do not change the approved product contract, add public/private TLS variables, weaken network or certificate checks, put the owner login in a serving pod, add a second kind journey, or require a pre-release published image. Use the existing guide, policy, image and credential mechanisms.

After focused proof above, run `mise run test:server:startup`, `mise run test:server:kind`, `mise run docs:check`, `mise run fmt`, `mise run lints`, and `git diff --check`. Run `test:server:peer` only if peer code or admission changes. Any specifically named Rust test added to the evidence needs its exact `mise exec -- cargo nextest run --locked` command through the owning environment wrapper. Record results, image provenance and walkthrough observations in this task, then route the complete cumulative candidate to `$wyrd-task-review`.

## Implementation evidence

The candidate is `vcc/task-006` at `abf963ccf`; this evidence commit sits on top. The R6 commits are:
- `f0f6e6363`: the storage idle-pool fix, directed by the user before R6.
- `e402c93e0`: guides and `roles.sql`.
- `2b6409b84`: this review packet.
- `abf963ccf`: `.dockerignore`.

`abf963ccf` is the last change to any image input, so both image lanes below ran on the final source.

| Acceptance criterion | Implementation evidence | Verification evidence | Result |
|---|---|---|---|
| FIND-13: the official-image startup and kind journeys run from the final server source | No new mechanism; the existing lanes were rerun after the last image input | `test:server:startup` PASS on image `sha256:45eec80fb40bc995cd5e88e8e3612a9db8f60478c9c36f816974af046d05f807` from `abf963ccf` (write, migration refusal and retry, verify). `test:server:kind` PASS on image `sha256:f7fd25687d4fbc93215722de9fdd93d8b1030b71db62bea19a7005fed2ea1be2` from `abf963ccf`: the script's pod `imageID` check held, the Oracle HPA scaled 1→2 at 12883m reads/s, the new pod joined `https://10.244.0.16:50052`, and the remote Scribe tail ran over mTLS. Later commits touch only this task file. | PASS |
| FIND-27: the policies exist before any serving pod; HTTP and gRPC work through the edge afterwards | `kubernetes-production.svx`: the numbered order and the apply block apply `wyrd-netpol.yaml` and `wyrd-mesh.yaml` first and `wyrd-edge.yaml` last | Fresh-namespace walk in the guide's order. A probe pod created before the policies probed every serving pod on both public ports while it was not Ready: 6 of 6 probes returned `000`. Afterwards: readyz 200 on both hostnames, `kind_seed` (gRPC ingest plus HTTP read) PASS through the edge, edge→anchor gRPC 200s over `mutual_tls` = 1 with 0 requests without mTLS, an ordinary pod times out on 8080 and 50051, an unmeshed pod with the gateway label is reset, 40/40 reads pass through the query edge, and the HPA and restart work; `docs:check` PASS | PASS |
| FIND-31: no database password in spawned `psql` or `kubectl` arguments; the logins still verify TLS; migration succeeds; the pods get their credentials | `roles.sql` reads `WYRD_APP_PASSWORD` and `WYRD_PLATFORM_ADMIN_PASSWORD` via `\getenv` when `--set` is absent. Both Kubernetes guides pass the owner's password through `PGPASSWORD` and build Secrets with `--from-file=<(printf …)`, so no temporary file is written. `database.svx` documents the environment channel, and the prerequisites now name psql 15 and a bash or zsh shell | Walk: wrappers recorded the argument list of all 1074 `psql` and `kubectl` processes it spawned (7 of them `psql`), and none contains any of the three passwords. The three logins print `wyrd_owner\|t`, `wyrd_app\|t` and `wyrd_platform_admin\|t`; a wrong CA and a wrong host are refused; migration with the wrong CA fails with `UnknownIssuer` and succeeds with the provider CA; the Secret holds the verify-full app URL, and the anchor has `WYRD_DATABASE_URL`. `test:postgres:roles` (now bootstrapping through the environment channel) PASS; `test:postgres:contract` PASS; `docs:check` PASS | PASS |

### Diagnoses

1. **Readiness storage probe flap (user-directed, before R6)**
   - **Symptom:** in kind, Oracle `/readyz` intermittently returned 503.
   - **Evidence:** with the new error logging, a local reproduction against the lane's rustfs image on the server's 5 s tick failed 8 of 40 probes with `Unexpected (temporary) at stat … connection closed before message completed`.
   - **Cause:** a fresh read-only diagnostician confirmed it. rustfs closes idle keep-alive connections after about 5 s, while OpenDAL's process-global reqwest client keeps them for 90 s. A request written to a connection the server is closing fails, and nothing retries it.
   - **Fix site:** `wyrd_storage::factory::finish_op`. Every operator now carries its own client with a 2 s idle pool. This covers every `StorageHandle` user: Bifrost storage, Iceberg FileIO, Scribe and Forge.
   - **Why not a retry layer:** the diagnostician ruled out OpenDAL's `RetryLayer`, because Bifrost writes and deletes must never be retried behind the caller.
   - **Proof:** after the fix, 40 of 40 probes pass. The regression test `factory::tests::idle_connection_is_not_reused_after_the_pool_timeout` fails at the second `stat` without the fix and passes with it. `wyrd-storage` lib 61/61, `test:storage:rustfs` 4/4 and the handle CRUD tests on S3, GCS, Azure and local storage all pass.
2. **Docker VM out of space during the startup-lane build**
   - **Symptom:** `failed to apply diff … no space left on device` at Dockerfile step 19.
   - **Evidence:** the root build context was 13.69 GB, and the Colima data disk had 11 GB free.
   - **Cause:** `.dockerignore` did not exclude the Python virtualenv (8.2 GB), `_wyrd.abi3.so` (1.6 GB) or the TypeScript `.node` modules (1.7 GB). None of them is copied into any image.
   - **Fix site:** `.dockerignore`. The context is now 32 MB plus the copied server binary. `docker/official/Dockerfile` is the only Dockerfile, and it copies none of these files.

### Commands

```bash
mise exec -- cargo nextest run --locked -p wyrd-storage --features cloud --lib \
  -E 'test(=factory::tests::idle_connection_is_not_reused_after_the_pool_timeout)'   # PASS (FAIL with the layer removed)
mise exec -- cargo nextest run --locked -p wyrd-storage --features cloud --lib       # 61/61
mise run test:storage:rustfs                 # PASS
mise run test:storage:handle:emulators       # PASS
scripts/postgres/with-test-postgres.sh -- mise exec -- cargo nextest run --locked -p wyrd-server --lib \
  -E 'test(/components::health::/)'          # 6/6
mise run test:postgres:roles                 # PASS
mise run test:postgres:contract              # PASS
mise run docs:check                          # PASS
mise run fmt                                 # PASS
mise run lints                               # PASS
mise run test:server:startup                 # PASS, image 45eec80f… from abf963ccf
mise run test:server:kind                    # PASS, image f7fd2568… from abf963ccf
git diff --check main...HEAD                 # clean
```

The walk script is kind-only and not committed (`scratchpad/walk6/walk.sh`). It extracts both guides' YAML on each run, applies the printed kind deviations, and ran on image `f7fd2568…`. `test:server:peer` was not run because no peer code or admission changed.

### Non-goals kept

No product contract, public or private TLS variable, second kind journey, harness or image mechanism was added, and no network or certificate check was weakened. The owner login stays in the migrate Job only. Secret names and keys are unchanged. The published-digest journey remains deferred to the first release.

### Limits

- The startup-window check probed each serving pod once on each public port while it was not Ready (6 probes in total). It shows that the policies were active from the pods' first moments. It does not prove continuous coverage of every millisecond.
- A `000` during startup cannot by itself distinguish "blocked" from "not yet listening". What establishes the protection is that the policies already existed when the pods were created, and the same probes stay blocked after the pods are Ready.
- `roles.sql` still accepts `--set`, which the test-database scripts use with throwaway passwords.

**Status:** IMPLEMENTED. Route the cumulative candidate to `$wyrd-task-review`.
