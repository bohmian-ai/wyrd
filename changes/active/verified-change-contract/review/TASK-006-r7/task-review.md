# TASK-006 R7 task implementation review

## Subject and result

- Repository: `/home/thorrester/Documents/GitHub/wyrd-vcc-t006`; cumulative base `f8811ac5035c3aa165d34c38992f9889b3c9081f`; immutable candidate `f3c65147e0fd828fe5c657d2871ff80f9b3d5543`.
- Authority: approved `changes/active/verified-change-contract/spec.md` revision 44; original `tasks/TASK-006-continuous-eval-verifier.md`; R1–R6 verdicts, findings, and remediation tasks; `AGENTS.md`, `architecture/agent-rules.md`, and applicable architecture.
- Review is static. I inspected the cumulative diff, the R6 changes and recorded tests; I did not rerun Docker, kind, Postgres, or cloud tests. `.codegraph/` is absent.
- **Result: FAIL.** R6 FIND-TASK-006-13 remains open. I also propose one adjacent storage drift finding for independent validation.

## Acceptance matrix

| Obligation | Implementation evidence | Verification evidence | Result |
| --- | --- | --- | --- |
| Original continuous Eval: first committed observation activates best-effort post-ACK work, stable sample/trace lifecycle, errors stay errors, canonical items and summary, native bounded media, single Vala engine; AC-014/016/020/027 | Cumulative server, Vala, SQL and journey code; prior R1–R6 inspected closures | Recorded server journey 22/22, Oracle 28/28, SQL, provider and negative media proof | PASS within recorded limits |
| No outbox, batch-fence join, Bifrost queue polling, synthetic failed assertions, second Eval engine, offline dataset executor, or private media URI in provider text | Cumulative implementation and prior reviews | Prior task and Eval reviews | PASS |
| External Postgres with owner-only migration, two serving roles, RLS and schema readiness; AC-035 | Cumulative SQL/server code, R5 removal of `SqlStore` | Recorded `test:sql`, role and startup checks | PASS within recorded limits |
| Peer mTLS, discovery, receiver checks, ready membership and peer-local-storage refusal; AC-036 | Cumulative peer/server/kind code | Prior peer 9/9 and kind evidence; no peer code in R6 | PASS within recorded limits |
| Official image startup and single Oracle kind autoscaling journey built from final reviewed source; AC-034/037, R6 FIND-13 | `scripts/server/test-startup.sh` and `test-kind-autoscale.sh` build and pin the image from checkout; `docker/official/Dockerfile` copies the compiled server | Both recorded runs used `abf963ccf`; `f3c65147e` later changed `wyrd-storage/src/factory/mod.rs`, a direct server dependency. No run on `f3c65147e` is recorded | **FAIL — TASK-R7-1** |
| First production rollout applies NetworkPolicy and strict mesh policies before serving pods; R6 FIND-27 | `kubernetes-production.svx:238–247,423–436` puts policies before workloads and edge last | Fresh-namespace walk records blocked direct access during startup and successful edge HTTP/gRPC afterward | PASS within recorded proof limits |
| Developer setup keeps database passwords out of psql/kubectl argv while preserving owner migration, serving credentials and verified database TLS; R6 FIND-31 | `roles.sql` reads bootstrap secrets with `\getenv`; both Kubernetes guides use `PGPASSWORD` and `kubectl --from-file` process substitution | Recorded argument capture, three login checks, wrong-CA/hostname checks, migration and serving walk; roles/contract tests | PASS within recorded proof limits |
| Adjacent storage failure fix leaves Bifrost writes without hidden retries and avoids idle connections closed by rustfs | `wyrd-storage/src/factory/mod.rs` builds per-operator HTTP client; custom endpoints use 2 s idle timeout and no RetryLayer | Recorded 8/40-before and 40/40-after readiness probes, focused regression, storage library and emulator tests | PASS for rustfs; **FAIL — TASK-R7-2** for unproven managed-cloud tuning |
| Build context excludes local virtualenv/native outputs, preserving official image recipe | `.dockerignore` change | Recorded context reduction from 13.7 GB to 32 MB, startup build succeeds | PASS |
| Required checks for touched surface | No final-source startup/kind evidence; R6 records fmt, lints, docs, SQL, storage tests and `git diff --check` | Cumulative `git diff --check` passes; final-source image lanes missing | **FAIL — TASK-R7-1** |

## Proposed findings

### TASK-R7-1 — Reopen FIND-TASK-006-13 (MISSING)

- **Obligation:** Spec AC-034/037 and the R6 task require the existing official-image startup and kind journeys to run from the final reviewed image inputs, with immutable image ID and source commit recorded.
- **Evidence:** `docker/official/Dockerfile:28–31` copies `binary/wyrd-server`; `scripts/server/test-startup.sh:100–112` and `test-kind-autoscale.sh:110–145` compile and image that binary. `wyrd-server/Cargo.toml:97` depends directly on `wyrd-storage`. The R6 task evidence lines 61 and 95–96 records both passing images built from `abf963ccf`. The later candidate commit `f3c65147e` changes `crates/wyrd/wyrd-storage/src/factory/mod.rs:21–64,164–215`, so it changes the server binary. No later run is recorded; the R6 statement that later commits touch only the task file is false for this candidate.
- **Consequence:** The recorded startup, persistence, kind HPA and peer-tail results prove an earlier binary. They cannot accept the final candidate's official image.
- **Testable correction:** Run the existing `mise run test:server:startup` and `mise run test:server:kind` after finalizing image inputs. Record source commit, immutable image ID, and the scripts' per-container pin checks. No new journey or published image is needed.

### TASK-R7-2 — Proposed new adjacent storage finding (DRIFT)

- **Obligation:** The original task and R6 remediation require a reliable startup/Oracle readiness path and preserve Bifrost's no-hidden-retry write rule. Adjacent storage changes must be necessary and safe for that path.
- **Evidence:** The diagnosed rustfs failure is a custom-endpoint connection closing after roughly five seconds; `factory/mod.rs:30,53–64` already uses the 2 s bound for it. Candidate commit `f3c65147e` adds unmeasured 15 s AWS S3 and 60 s managed GCS/Azure bounds at `factory/mod.rs:32–60`. `build_operator` applies those to real production operators at lines 190–207. The new `pool_idle_timeout_matches_the_storage_server` test only asserts the chosen constants; it does not check a managed server's idle-close behavior. The R6 evidence itself says these provider timeouts were not measured and reports a possible AWS close at 3–5 s. A 15 s bound can therefore reuse a connection after its server closes it. No throughput requirement or measurement justifies increasing the common 2 s bound.
- **Consequence:** A production S3 write after a shorter server idle close can fail without an automatic retry; the extra managed-provider policy also expands code and assumptions outside the demonstrated rustfs fix.
- **Testable correction:** Delete the provider-specific timeout selection and use the already-proven 2 s per-operator bound for every OpenDAL backend. Retain the rustfs idle-gap regression and no-retry semantics. Revisit longer managed timeouts only with real-provider evidence showing the needed throughput and safe close margin. Independent validation should decide whether this adjacent change is material enough to retain as a finding.

## Limits

This review does not claim that the cloud timeouts have caused an observed failure. It identifies an unproven production behavior change that weakens the diagnosed connection-close avoidance on a reachable path. Prior finding closures outside the two entries above rely on cumulative source and recorded test evidence; infrastructure lanes were not independently rerun here.
