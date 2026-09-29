# TASK-006 R3 task implementation review

## Subject and result

- Repository: `/home/thorrester/Documents/GitHub/wyrd-vcc-t006`
- Original base: `f8811ac5035c3aa165d34c38992f9889b3c9081f`
- R2 remediation base: `f500ea38bc749f36b3ee8d88893dcf7c0161435c`
- Candidate: `2fbe90cd880936d8e4f25173839e4cc4fa18f0b4`
- Approved authority: `changes/active/verified-change-contract/spec.md`, revision 40
- Original task: `changes/active/verified-change-contract/tasks/TASK-006-continuous-eval-verifier.md`
- Remediation: `changes/active/verified-change-contract/review/TASK-006-r1/TASK-006-R1-continuous-eval-closure.md` and `review/TASK-006-r2/TASK-006-R2-continuous-eval-closure.md`
- Prior findings: R1 `FIND-TASK-006-1` through `-10`; R2 `-11` and `-12`

**Result: FAIL.** One required deployment proof uses a locally built checkout image rather than a pinned published image. The remaining inspected task obligations pass. This is a proposed Wave 1 finding for independent validation.

The task reviewer inspected the complete base-to-candidate changed-file inventory and diff, the remediation range, approved obligations, prior verdicts and ledgers, current source at the named boundaries, and the recorded verification evidence. No reviewed source was changed.

## Acceptance matrix

| Requirement, acceptance criterion, constraint, or non-goal | Implementation evidence | Verification evidence | Result |
|---|---|---|---|
| REQ-077/079; first committed Eval observation alone starts best-effort enqueue with exact managed event day | `scribe/ingress.rs`, `gate/mod.rs`, `verification/observations.rs`; committed disposition gates activation | `eval_verification::sealed_replay_on_a_later_day_activates_once`; server journey | PASS |
| REQ-083/084/085/111/130/152; one Eval engine and judge path, stable sampling, complete trace input, attesting result/error split, canonical items and summary | `verification/eval.rs`, `vala-eval` executor/results, immutable `observation_ordinal` in `verifier_runs.rs` | terminal-matrix journey; trace, storage, SQL, and engine regressions recorded in original/R1 task evidence | PASS |
| AC-014/016/020/027; real server terminal matrix, cross-day input, fail-open enqueue, native media, negative media without false verdict or dispatch | `verification/eval.rs`; `eval_verification.rs:694-930` includes unbound, foreign, unsupported MIME, and oversized records | server journey 22/22, Oracle journey 28/28; focused terminal-matrix command recorded passing | PASS |
| Prior R1 findings 1–10 and R2 findings 11–12 remain closed | `WyrdTestServer::superuser_pool` removed; `eval_verification.rs:744-837` adds three negative media records; shared bounds, errors, trace projection, sampling and documentation retained | R2 task's exact focused tests and current complete server/SQL/storage lanes | PASS |
| REQ-153/155/163; official image contains Rust, Node BFF, nginx; setup, public HTTP/gRPC, relative local artifact URLs and persisted data | `docker/official/Dockerfile`, `entrypoint.sh`, `nginx.conf.template`, `startup_image_journey.rs`, shared client configuration, storage URL service | `test:server:startup` recorded passing; Python/TypeScript client and artifact journeys, storage matrix | PASS except image provenance below |
| AC-034 and R2 Scenario 2: local journey starts from a **pinned published image** | `scripts/server/test-startup.sh:109-112` compiles the server and runs `docker build -t "$image"` from this checkout; `docker/official/Dockerfile` is the intended release image recipe | Local image test passed, but neither a published registry artifact nor its digest is selected or exercised | **FAIL — TASKREV-006-R3-001** |
| REQ-154/156/157/158, AC-035; external Postgres, two serving roles, owner-only migrate, schema refusal/retry | `wyrd-server/main.rs:188-219`, `postgres.rs:19-32`, SQL bootstrap and schema checks | `test:server:startup` migration refusal/retry; `test:sql` and role checks recorded passing | PASS |
| REQ-159/160/161, INV-016; local calls without peer credentials; remote mTLS and receiver context checks | `boot/mod.rs`, `oracle/peer_authority.rs`, Scribe tail RPC, private proto and peer transport | `test:server:peer` 9/9 and local server journey; negative TLS/context cases | PASS |
| REQ-162/163, AC-036; runtime address registration, fresh peer discovery, remote Oracle, Scribe tail, no partial result | `vala.cluster_nodes` membership, `peer_network/join.rs:73-150`, peer configuration and Kubernetes Downward API manifests | two-process peer journey and Oracle journey recorded passing | PASS |
| AC-037; HPA on measured successful read rate scales one to two, registers address, executes remote read through mTLS | `deploy/kubernetes/kind/{wyrd,infra,load}.yaml`; `scripts/server/test-kind-autoscale.sh` | one recorded `mise run test:server:kind` pass, HPA desired 2 at 13.625/s, member row and Oracle metric change | PASS for scaling mechanism; same local-build image provenance caveat |
| REQ-164/165/166, AC-038; no ticket compatibility or PolicyHook route, preserved delegated exchange and API denial | deleted route/hook/client surfaces; `pg_router_smoke.rs`, exchange code, synchronized architecture/docs | served OpenAPI and authenticated 404 in startup lane; delegated Rust/Python/TypeScript journeys | PASS |
| Task non-goals: no outbox, second Eval engine/judge, offline dataset execution, provider file lifecycle, private URI prompt text, replacement policy gate | Cumulative diff and current owners | Terminal, provider, and router proofs above | PASS |

## Proposed material finding

### TASKREV-006-R3-001 — MISSING

- **Obligation:** AC-034 requires starting the local deployment from a pinned published image. AC-037 and the R2 task likewise require the kind rehearsal to use the published image.
- **Location:** `scripts/server/test-startup.sh:109-112`; `scripts/server/test-kind-autoscale.sh:104-108`; the release publisher is `.github/workflows/release.yml:408-460`.
- **Evidence:** Both required proof scripts compile `wyrd-server` from the checkout and build `docker/official/Dockerfile` into ephemeral local tags. Neither resolves a release tag to an immutable registry digest, pulls that digest, or runs a published artifact. The release workflow publishes architecture-specific images only on a release event, separate from either journey.
- **Observable consequence:** A successful lane can certify a locally built image while the actual published image is missing, differs in contents, or fails to boot. The approved image delivery obligation has no artifact-level proof.
- **Required testable correction:** Run the startup and kind journeys against one selected published official image by immutable digest, recording that digest with the run. Keep the existing assertions and source-image recipe; do not add another image or test harness. If publication cannot yet occur, record this acceptance proof as incomplete rather than treating the local build as equivalent.

## Verification limits

This was a static acceptance review. The available lane results are the exact commands and outcomes recorded in the R2 task; this reviewer did not rerun the expensive Docker/kind/whole-repository lanes. The kind journey's tail rows may already be published, as recorded in the task; AC-037 requires a successful cross-pod read, which the journey establishes, rather than specifically proving unpublished tail rows.
