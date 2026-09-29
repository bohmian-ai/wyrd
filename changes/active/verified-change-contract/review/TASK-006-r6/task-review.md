# TASK-006 R6 task implementation review

## Immutable subject and method

- Repository: `/home/thorrester/Documents/GitHub/wyrd-vcc-t006`.
- Cumulative base: `f8811ac5035c3aa165d34c38992f9889b3c9081f`.
- Candidate: `3f93886489a1d95be1a3eb2382fe059bb9856988` (evidence parent `895477da1ced790ded96e21a83e2cf31c3185ca1`).
- Authority: approved `changes/active/verified-change-contract/spec.md` revision 44; original TASK-006, R1–R5 verdicts and remediation tasks; `AGENTS.md`, `architecture/agent-rules.md`, spec-driven-development and applicable Eval, Bifrost, deployment, security, and testing authorities. `.codegraph/` is absent.
- Inspected the cumulative diff and R5 increment, current source and recorded verification. This is a static review; I did not rerun Docker, kind, Postgres, or language journeys. `git diff --check` for the cumulative range passed.

## Acceptance matrix

| Requirement, criterion, constraint, or non-goal | Implementation evidence | Verification evidence | Result |
| --- | --- | --- | --- |
| Original continuous Eval: one existing engine/judge path; post-ACK best-effort activation; stable sampling, trace, error/result distinction, gate mapping, canonical outcomes, native media and authorization (REQ-077/083–085/111/130/152, AC-014/016/020/027) | `crates/wyrd/wyrd-server/src/verification/{observations,eval,runner}.rs`, `crates/vala/vala-eval`, `crates/wyrd/wyrd-testing/tests/bifrost/server/eval_verification.rs`; R5 does not alter these paths | Original and R1–R4 recorded terminal-matrix, provider, server, SQL and language journeys; prior independent Eval reviews | PASS within recorded limits |
| Eval non-goals: no outbox, second engine, Bifrost queue, synthetic failed assertions, offline execution, private URI in provider text | Cumulative source and deletion inventory; native media test checks provider body | Prior journey and source reviews | PASS |
| Official image, single-process standalone, setup, public HTTP/gRPC, durable restart, removed policy hook (REQ-153–155/159/165/166, AC-034/038) | `docker/official`, startup script and journey, config/routes and SDK URL resolution | Recorded startup and language journeys; **current source-image proof gap below** | FAIL for AC-034 image provenance; otherwise PASS within recorded limits |
| External Postgres, two serving roles and owner-only migration; no redundant `SqlStore`; bounded migration lease; fail-closed schema readiness (REQ-156–158, AC-035, reopened FIND-20) | `SqlStore` and its raw-pool conversion deleted; `OperatorPool::migration_lease` in `wyrd-sql/src/lib.rs`; `wyrd-server/src/main.rs` builds an owner-backed `OperatorPool` only in `migrate`; schema check retained | R5 records focused competing-migrator test, `test:sql`, startup, and pool allowlist passing | PASS |
| Local calls, peer mTLS and receiver checks, discovery and ready membership, shared storage (REQ-160–164, AC-036) | Cumulative peer authority, listener and cluster-membership paths; R5 does not edit them | Prior peer 9/9 and kind journeys; R5 explicitly did not rerun peer lane | PASS within recorded limits |
| One bounded kind journey scales the Oracle workload on successful Oracle-executed reads, then proves new Oracle execution over remote Scribe data (AC-037, FIND-26/30) | `deploy/kubernetes/kind/{wyrd,load}.yaml`, `scripts/server/test-kind-autoscale.sh`; R5 load shell counts parent-shell jobs before spawning reads | R5 records held-read cap of eight and Oracle HPA 1→2 kind pass; **current source-image proof gap below** | FAIL for AC-037 image provenance; otherwise PASS within recorded limits |
| Three self-hosting startup journeys, required variables, role and credential setup, peer certificates, production TLS and autoscaling (R3 documentation task; FIND-27–29) | `docs/.../self-hosting/{local-development,kubernetes-development,kubernetes-production}.svx`; production guide has strict mesh mTLS, edge-only public access, verify-full DSNs with CA, and Oracle HPA; development guide uses CLI tenant creation | R5 records local kind guide walk through HTTP and gRPC edge, mTLS/ordinary-pod denial, three TLS database logins plus wrong-CA/host refusal; `docs:check` | PASS within kind-only guide proof limits |
| R5 secondary fixes: catalog SQLx TLS, public HTTPS gRPC roots, same-hostname gRPC routing, readiness error log, unused TLS config removal and obsolete manifests deletion | Catalog TLS feature/test, `ClientTlsConfig::with_native_roots`, production HTTPRoutes, health logging, transport config removal and approved adjacent Bifrost spec revision | R5 records focused catalog/client tests, codegen, client-tier, workspace-hack, docs, format and lints | PASS within recorded limits |
| Revision 44 published `dist` profile; no premature published-image claim | Workspace profile and `.github/workflows/release.yml`; spec defers published-image journey to first release | Source inspection; no image yet published | PASS |

## Proposed material finding

### TASKREV-R6-01 — INCORRECT: the image journeys do not exercise the final server source

**Violated obligation.** The pre-release paragraph before AC-034 and AC-037 requires the official recipe image built from the reviewed commit, pinned by immutable local image ID, for the startup and kind journeys. Prior `FIND-TASK-006-13` made that provenance explicit. R5 also requires `test:server:startup` and `test:server:kind` after remediation.

**Exact location and evidence.** The R5 remediation evidence records both passing image journeys against `sha256:4f1b645befa38935a1bfac34dad51da5fd51199c59e93a14d0c7afb112d0f94a` built from `dcf1321aa`. The scripts label builds with `git rev-parse HEAD` (`scripts/server/test-startup.sh:103-115`; `scripts/server/test-kind-autoscale.sh:110-123`). Commit `1303c20a8`, *after* `dcf1321aa`, changes `crates/wyrd/wyrd-server/src/components/health/mod.rs`, which is compiled into the official server binary and affects readiness diagnostics. Later `b87993bfb` changes SDK transport code and `895477da1` removes deployment files; those do not themselves prove a different image binary, but they remain unexercised by the cited full journeys. The final code parent is `895477da1`; `3f9388648` only records evidence. The evidence's statement that all later commits are documentation is false.

**Observable consequence.** The pinned image and recorded journeys establish behavior for an earlier binary, not for the final server source under review. In particular, the new readiness logging was checked through a focused health test, but not in the required official-image startup/kind journeys. This is a proof gap, not evidence that the final binary fails to boot.

**Required testable correction.** Rerun the existing startup and kind image journeys from the final code commit, record each immutable image ID and source commit, and verify every application container uses the corresponding pinned image. Preserve the approved release-only published-digest check; add no second kind test or new image mechanism.

## Result and limits

**FAIL.** The R5 changes appear to close FIND-20 and FIND-27–30 on the recorded paths, and prior Eval/peer findings remain closed within their earlier evidence. The source-image mismatch prevents task acceptance under AC-034/037. This static review did not rerun infrastructure lanes; it relies on the exact command/results in the task packet and inspected source. No other material task finding is proposed.
