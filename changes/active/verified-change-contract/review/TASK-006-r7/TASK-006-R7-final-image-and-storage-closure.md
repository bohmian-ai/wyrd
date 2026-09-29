---
id: TASK-006-R7
kind: remediation
status: ready
spec: SPEC-verified-change-contract
spec_revision: 44
requirements: [REQ-153, REQ-155, AC-034, AC-037]
depends_on: [TASK-006-R6]
parent_task: TASK-006
remediates: [FIND-TASK-006-13, FIND-TASK-006-33, FIND-TASK-006-34]
---

# TASK-006 R7 — Final image proof and storage test closure

## Authority and reviewed subject

- Approved [specification revision 44](../../spec.md) and [original TASK-006](../../tasks/TASK-006-continuous-eval-verifier.md).
- Prior [R6 remediation](../TASK-006-r6/TASK-006-R6-startup-proof-and-credentials.md), this review's [verdict](verdict.md) and [validated findings](findings-validation.md).
- Immutable cumulative range: `f8811ac5035c3aa165d34c38992f9889b3c9081f..f3c65147e0fd828fe5c657d2871ff80f9b3d5543` on `vcc/task-006`.

Preserve the accepted continuous Eval behavior, owner-only migration, separate serving roles, verified database TLS, peer mTLS, production policy order, password-safe startup guides, and **one** Oracle autoscaling kind journey. R6 `FIND-TASK-006-27` and `FIND-TASK-006-31` are closed.

## FIND-TASK-006-33 — Gate the socket regression test

**Diagnosis.** `factory/mod.rs:261–298,343–369` places the idle-gap test and its HTTP socket server in the ordinary `#[cfg(test)] mod tests`. That test binds `127.0.0.1`, accepts connections and sends HTTP from a `--lib` fast lane, contrary to `AGENTS.md` §11's IO-free unit tier and the agent rules' live-server test placement. Its behavior is useful; its tier is wrong.

**Correction.** Move only the socket helper and its test into an inlined gated `pg_tests` module, following the repository's existing test convention. Keep the pure timeout-selection test in the ordinary module. Do not add an external test binary or new harness.

**Acceptance.** The ordinary library lane does not select a socket test, and the moved test passes under its exact focused `mise exec -- cargo nextest run --locked -p wyrd-storage --features cloud --lib -E 'test(=factory::pg_tests::idle_connection_is_not_reused_after_the_pool_timeout)'` invocation. Confirm the final path from source before running.

## FIND-TASK-006-34 — Follow Rust source rules in the touched tests

**Diagnosis.** The new functions at `factory/mod.rs:261–265,305–306,343–344` contain function-local imports, the socket helper uses a fully qualified type in its return signature, and its rustdoc and the test rustdoc omit panic conditions despite `expect` and assertions. `architecture/agent-rules.md` requires module-top imports and bare imported signature types; `AGENTS.md` §16 requires panic documentation for touched Rust items.

**Correction.** While relocating the socket test, put imports at each owning test-module header, use bare imported types in the helper signature, and document panic conditions for the helper and both tests. Do not add lint suppressions or unrelated documentation.

**Acceptance.** Inspect the touched items for module-top imports, bare signature types and `# Panics` sections; `mise run fmt` and `mise run lints` pass.

## FIND-TASK-006-13 — Prove the final official image

**Diagnosis.** R6 recorded passing `test:server:startup` and `test:server:kind` on official images built from `abf963ccf`. Candidate `f3c65147e` later changed `wyrd-storage`, which `wyrd-server` compiles with `cloud`. The recorded images therefore do not contain the final storage behavior and cannot establish AC-034/037 for the reviewed source. This is a missing proof, not an observed startup failure.

**Correction.** After all image inputs are final, rerun the **existing** startup and kind lanes. Record each source commit, immutable local image ID and the scripts' check that every Wyrd container ran that image. Any later evidence-only commit must be shown not to change an image input. The approved pre-release rule requires no published image, second kind test, or new pinning mechanism.

**Acceptance.** Both lanes pass from final image inputs. The kind lane still scales Oracle from one to two on successful Oracle-executed reads and proves the new Oracle executes a read using the anchor's Scribe tail over mTLS.

## Verification and constraints

Run the focused moved storage test, the owning storage library and emulator/rustfs lanes, `mise run fmt`, `mise run lints`, `git diff --check`, then `mise run test:server:startup` and `mise run test:server:kind` after the final code change. Record exact commands, results and image provenance in this task. Preserve the reviewed candidate's per-backend idle limits and no-hidden-retry write rule, the production security and credential corrections, owner-only migration, and peer mTLS. `test:server:peer` is needed only if peer code or admission changes. No spec, public contract, provider-specific setting, additional kind journey, or release publication is requested.

Route the complete cumulative candidate to `$wyrd-task-review` after implementation.

## Implementation evidence

The candidate is `vcc/task-006` at `214d39c13`, with this evidence commit on top. The R7 commits are:
- `e485c98f7` follows the original R7 text. It gates the socket test and, per FIND-32, collapses the idle limits to 2 s.
- `214d39c13` applies the revised task, which withdraws FIND-32. It restores the reviewed per-backend limits and the pure selection test exactly as in `f3c65147e`.

Net against the reviewed candidate, `git diff f3c65147e 214d39c13 -- crates/` touches only the test modules of `crates/wyrd/wyrd-storage/src/factory/mod.rs`. No production line changed. `214d39c13` is the last change to any image input. The evidence commit touches only this file.

| Acceptance criterion | Implementation evidence | Verification evidence | Result |
|---|---|---|---|
| FIND-33: the ordinary library lane selects no socket test; the moved test passes under its focused command | `factory/mod.rs` has a new `#[cfg(all(test, feature = "cloud"))] mod pg_tests` holding `closing_keep_alive_server` and `idle_connection_is_not_reused_after_the_pool_timeout`. `mod tests` keeps the fs tests and `pool_idle_timeout_matches_the_storage_server` | Focused `factory::pg_tests::idle_connection_is_not_reused_after_the_pool_timeout` PASS (1 run, 61 skipped). `--lib -- --skip pg_tests` runs 61, passes 61, skips 1 (the socket test). `nextest list` shows the only socket test under `factory::pg_tests::` | PASS |
| FIND-34: module-top imports, bare signature types, `# Panics` on the helper and both tests | `pg_tests` imports `Arc`, `AtomicUsize`, `Ordering`, `ErrorKind`, the tokio IO traits, `TcpListener` and `S3Config` at its header. `mod tests` imports the settings types at its header under `cloud`. The helper returns `(String, Arc<AtomicUsize>)`. The helper, the selection test and the socket test each document `# Panics` | Source inspection. `mise run fmt` PASS; `mise run lints` PASS (no warnings from the touched crate) | PASS |
| FIND-13: startup and kind journeys pass on images built from the final image inputs, and every Wyrd container ran that image | No new mechanism. The existing lanes ran from clean `214d39c13`: the scripts mark a dirty tree `+dirty`, and neither provenance line carries it | `test:server:startup` PASS on `sha256:d4e8f4818d297280683b4dd07d7abb2e11750848e7b4c1fc02a05e169ce2dd50` from `214d39c13` (write, migration refusal and retry, verify). The script's `docker inspect` app-image check held. `test:server:kind` PASS on `sha256:265270450946c771faeadae67a4be71d9bd319de9b087b15fd1fdc45086c374b` (config `sha256:8561d066d28d782fe32ee55c86e30c4f754bc6668ccabd604d725f7f6f78e4c2`) from `214d39c13`. The script's pod `imageID` pin check held. The Oracle HPA scaled 1→2 at 12827m Oracle-executed reads/s; the new Oracle joined `https://10.244.0.16:50052`; a certificate-less client was refused and the cluster leaf admitted; the new Oracle executed a read over the anchor's remote Scribe tail over mTLS | PASS |
| Storage behavior preserved: reviewed per-backend idle limits, no write retry | Production code is byte-identical to `f3c65147e`: per-operator `HttpClientLayer`, no `RetryLayer` | `factory::tests::pool_idle_timeout_matches_the_storage_server` PASS; `test:storage:rustfs` 4/4; `test:storage:handle:s3-emu`, `:gcs-emu`, `:azure-emu` each PASS | PASS |

The non-goals stayed excluded: no spec, public contract, provider setting, second kind journey, release publication, or peer or admission change, so `test:server:peer` was not needed. Nothing changed outside `factory/mod.rs` and this review packet. `git diff --check` is clean.

### Diagnosis — first kind run failed before any Wyrd pod

- **Symptom:** `test:server:kind` exited 1 with `deployment "prometheus" exceeded its progress deadline`. The rustfs and Prometheus fixtures were crash-looping, and no Wyrd pod had been created.
- **Evidence:** on a live rerun, rustfs logged `[FATAL] … Io error: No space left on device (os error 28)` and Prometheus died with `SIGBUS` on an mmap write. Inside the kind node, `df` showed `/dev/vdb1 98G 94G 0 100% /var`, because the Colima Docker VM disk was full. Host disk and inotify limits were not the cause.
- **Cause:** 18 untagged images, mostly superseded ~3.95 GB official-image builds from earlier lane runs, plus build cache filled the 98 GB Docker VM disk.
- **Fix site:** host Docker state, not repository code. `docker image prune -f` (untagged only) and `docker builder prune -f` freed the VM disk to 33 GB free. Volumes, tagged images and other projects' containers were untouched. The rerun passed unchanged.

### Commands

```bash
mise exec -- cargo nextest run --locked -p wyrd-storage --features cloud --lib \
  -E 'test(=factory::pg_tests::idle_connection_is_not_reused_after_the_pool_timeout)'  # PASS
mise exec -- cargo nextest run --locked -p wyrd-storage --features cloud --lib \
  -E 'test(=factory::tests::pool_idle_timeout_matches_the_storage_server)'            # PASS
mise exec -- cargo nextest run --locked -p wyrd-storage --features cloud --lib -- --skip pg_tests  # 61/61
mise run test:storage:rustfs            # 4/4
mise run test:storage:handle:s3-emu     # PASS
mise run test:storage:handle:gcs-emu    # PASS
mise run test:storage:handle:azure-emu  # PASS
mise run fmt && mise run lints          # PASS
git diff --check                        # clean
WYRD_LOG=info mise run test:server:startup  # PASS, image sha256:d4e8f481… from 214d39c13
WYRD_LOG=info mise run test:server:kind     # PASS, image sha256:26527045… from 214d39c13
```
