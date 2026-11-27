# CI memory diagnosis implementation plan

Status: ready

One implementation task. Authority is the maintainer-approved
[design](design/notes.md) and explicit instruction to omit a spec and TDD.
Implement the steps sequentially; record observations and commands here.
Do not turn each step into another task or require RED/GREEN cycles.

## TASK-001 — Diagnose the RunsOn failure and restore efficient CI

Status: in progress (step 3 passed on RunsOn; step 5 implemented locally; cloud validation of step 5 and step 6 pending)
Dependencies: none

### Outcome

Explain the measured RunsOn build failure, verify the smallest sufficient fix,
and restore CI with bounded build memory and reuse of workspace artifacts.
Successful local compilation alone does not establish a fix. RunsOn verification
is required before normal CI is restored.

### Execution

1. **Preserve evidence and stop obsolete runs.** Work on `ci-debug` from the
   approved main baseline `57665045e356f6b684020dead6af12b14ae61a2c`; record the
   actual implementation SHA. Archive the old diagnostic branch in a local git
   bundle and retain available logs and evidence-branch references. Disable
   `lints-test`, `nightly`, and `diagnose-rust-ci` through workflow controls;
   cancel any obsolete active experiment. Delete local and remote
   `diag/runner-loss` after verifying the archive. Preserve unrelated work.
   Do not change required-check protection or synthesize passing results.

2. **Measure the whole build locally.** Add one hidden mise task
   `ci:diagnose-build`, backed by `scripts/ci/diagnose-build.sh`, owning this
   known failing reproducer:

   ```sh
   cargo build --locked --workspace --tests \
     --exclude wyrd-sdk-python --exclude wyrd-sdk-ts --exclude wyrd-sdk-ts-testing
   ```

   Use the pinned toolchain, main's profile, `CARGO_INCREMENTAL=0`, native
   platform linker (main's BFD settings on Linux), and uncapped Cargo
   concurrency for the baseline. Record actual CPU count and settings.
   Use one dedicated initially empty target directory; reuse downloaded Cargo
   sources and preserve the developer's normal target directory.

   Sample every two seconds: build descendants including linker children,
   PID/parent PID, command/crate/target, RSS, CPU and elapsed time; system
   available memory, swap and available native CPU/I/O observations. Retain
   full timestamped output and the command's exit status. Record source SHA,
   host/RAM/toolchain, flags, target path and initial cache/artifact state.
   Do not treat summed RSS as exact system consumption or phase-boundary RSS
   as a continuous peak. Record kernel OOM details on Linux.

   On this 18 GiB Mac, abort only the build's own process tree after two
   consecutive critical-memory-pressure samples; label it an aborted
   measurement. Prefer an already accessible local Linux host for matching
   RunsOn; lack of access must not lead to installing a VM or assuming the Mac
   clears Linux. First inspect prior measurements; reuse only evidence that
   actually covers the whole build.

3. **Use one RunsOn workflow to diagnose or verify.** Add
   `.github/workflows/ci-debug.yml`: one job, no matrix, push trigger restricted
   to `ci-debug`, one active run with superseded runs cancelled. Use the same
   8 CPU / 32 GiB x86_64 RunsOn configuration and 100 GB root disk, S3 cache,
   `runs-on/action@v2`, one checkout and main's existing setup. Retain necessary
   cleanup and swap; do not import old cow-root setup modifications.

   If local evidence is insufficient, run the unchanged reproducer with the
   recorder to obtain the missing Linux evidence. If local evidence identifies
   a correction, use the workflow to verify that correction. Restore the
   dependency cache once, record its identity and hit status, and always invoke
   Cargo; never skip compilation based only on an exact cache key. Keep the
   workspace across steps. Use a 60-minute job timeout, mirror measurements to
   live logs, retain kernel diagnostics and upload artifacts with `always()`.
   Propagate the build exit code through logging, cleanup and artifact steps.
   Do not push or dispatch until explicitly asked; prepare the local diff and
   local evidence for that review. Do not claim cloud verification until run.

4. **Select and verify one correction from the evidence.** If overlapping
   compilers/linkers exhaust memory, test only `CARGO_BUILD_JOBS=2` first,
   retaining the linker/profile. If a single compiler dominates, profile its
   exact test invocation/features before editing code. If a single linker
   dominates, test lld with the same workload/concurrency and confirm the
   selected linker. Do not change multiple build variables together.

   Non-reproduction is an explicit result: inspect cache and process differences
   before another experiment. An unexplained VM loss remains unexplained; do
   not substitute a resource theory. A failed approach or material redesign
   returns to the maintainer with evidence and options before further changes.
   Crate splitting, toolchain changes and product refactors are outside scope.

   Compare equivalent source/configuration and initial artifact state; do not
   attribute improvement to a change if compilation was simply reused. Retain
   local artifacts for investigation; use matched starts when quantitatively
   comparing cold-build memory. Once the reproducer passes, run
   `mise run test:wyrd`, followed by `mise run gate:rust:families` on the same
   RunsOn checkout and target directory with the correction. Install the normal
   lane prerequisites and let existing mise tasks provision real services.
   The first run is focused validation; the aggregate is integrated proof.

5. **Apply the verified fix and remove duplicate builds.** Remove the separate
   `deps` warm-up from `lints-test.yml` and `nightly.yml`, including their
   `needs` references and required-job result arguments. Superseded by the
   maintainer's accepted topology (design notes): do not serialize the Rust
   shards; co-locate only `gate:typescript` then `gate:rust:journeys` in one
   RunsOn job/workspace, preserving selected lane membership and order, and
   restore once and save once there. Every other shard keeps its own
   GitHub-hosted job. Remove the forced BFD linker from shared setup and bump
   the cache namespace; no concurrency cap is set, because the lld run needed
   none. Preserve distinct Clippy/napi configurations and existing
   platform/language coverage. Keep `build:server` feature alignment and
   `line-tables-only`.

   Update `select-ci.py`, its existing tests, required-job consumers and the CI
   description in `TESTING.md` as necessary. Do not introduce another warmer,
   artifact-distribution system, cache service or sticky-disk mechanism.

6. **Restore and close out.** Validate the production CI configuration on
   RunsOn before re-enabling normal automatic workflows. Restore the real
   required-check path, then confirm the affected normal CI run passes.
   Remove the temporary experiment YAML/task/recorder after verification.
   Preserve the evidence and measured explanation in this task's execution
   record. Do not silently leave suspended CI or a diagnostic-only green check
   as the completed state.

### Verification and completion evidence

- Recorder: a small child-process command proves descendant capture; a deliberate
  nonzero command proves exit propagation. No synthetic load or large test suite
  for this helper. Record the exact self-check commands and outcomes.
- Local diagnosis: retain invocation/configuration, process/system samples and
  result. Distinguish reproduced failure, safety abort and non-reproduction.
- RunsOn: retain job URLs, actual exit codes, kernel evidence, baseline/candidate
  configuration and cache state. Explain the causal finding and why the chosen
  change addresses it; report observational limits explicitly.
- Reuse: Cargo output/fingerprint evidence shows unchanged dependencies reused
  between sequential lane steps. Cache-hit output alone is insufficient.
- Checks: validate changed YAML with actionlint through mise; run
  `mise run check:ci-selection` after selection/aggregation changes. Existing
  test expectations may change for routing, never to remove verification lanes.
- Real path: reproducer and family aggregate pass under the correction on
  RunsOn, followed by affected normal CI. Verify cleanup, workflow states and
  deletion of the obsolete branch. Record any pending remote authorization;
  preparation alone cannot mark this task complete.

### Execution record

Implementation SHA: uncommitted on `ci-debug` at
`57665045e356f6b684020dead6af12b14ae61a2c` (main baseline). Steps 1–2 done; step 3 prepared locally (not
committed, pushed or run); steps 4–6 not started.

#### Step 1 — retire the old experiment (2026-11-26)

- Archive (gitignored, local): `.dev/ci-memory-diagnosis/archive/`.
  - `diag-runner-loss.bundle` (116 MB): `refs/heads/diag/runner-loss`
    (`7dee9a95adf1b5a8e72b3eaaba9ac6360d2011c4`) plus the 12
    `origin/diag-samples/*` refs; `git bundle verify` OK; heads listed in
    `bundle-heads.txt`.
  - `runs/<run-id>/`: `run.json`, `jobs.json` and every still-downloadable job
    log for all 15 `diag/runner-loss` runs (`gh api .../actions/jobs/<id>/logs`).
    Jobs with no log (never started, or expired) have a `.log.missing` marker,
    including one of the two jobs of OOM run 45744069536.
- `gh workflow disable` for `lints-test`, `nightly`, `diagnose-rust-ci`; all
  three report `disabled_manually`. No run was in progress or queued, so none
  was cancelled (45831998406 was already cancelled).
- Deleted `diag/runner-loss` locally and on origin; `git ls-remote` confirms it
  is gone. `diag-samples/*` remote branches retained (archived, not deleted).
  Branch protection and required checks untouched.

#### Step 2 — local whole-build measurement

Implementation: hidden mise task `ci:diagnose-build` →
`scripts/ci/diagnose-build.sh`. Starts the command in its own process group and
samples every ~2 s every process in that group (pid, ppid, RSS, CPU, elapsed,
crate name and `[test]`), the rustc and linker (`ld`, `cc`, `collect2`, …)
totals, and system memory: on macOS `kern.memorystatus_vm_pressure_level`, free
percentage, `vm.swapusage`, `vm_stat` available pages and swapouts, and load; on
Linux `free`, PSI memory and I/O, load, and `dmesg` OOM lines. Exports
`CARGO_INCREMENTAL=0` and `CARGO_PROFILE_DEV_DEBUG=line-tables-only`, because
the developer's `~/.cargo/config.toml` sets `profile.dev.debug = 1`, which would
otherwise override main's profile. Default target:
`.dev/ci-memory-diagnosis/target`. On macOS it aborts the process group after
two consecutive critical samples (`aborted-memory-pressure`, exit 3).
Otherwise it exits with the command's status. Arguments replace the reproducer;
this is used only for the self-check.

Recorder self-check (output in `.dev/ci-memory-diagnosis/selfcheck/`):
- `DIAG_OUT_DIR=…/descendants mise run ci:diagnose-build -- bash -c 'bash -c "sleep 5" & sleep 5; wait; echo child-done'`
  → exit 0. Every sample lists both `sleep` grandchildren (ppid = the
  command's bash, whose ppid = the recorder).
- `DIAG_OUT_DIR=…/nonzero mise run ci:diagnose-build -- bash -c 'echo failing; exit 7'`
  → task exit 7, `result.txt` `exit=7`.

Prior evidence: none covers the whole build with per-process memory. The old
probe sampled only the `wyrd-server` compiler, and the RunsOn OOM job has no
process breakdown. No local Linux host is accessible, so this Mac measurement
cannot clear Linux.

Baseline run: `DIAG_OUT_DIR=.dev/ci-memory-diagnosis/runs/mac-baseline mise run ci:diagnose-build`.
- Artifacts: `.dev/ci-memory-diagnosis/runs/mac-baseline/{meta.txt,build.log,samples.log,result.txt}`.
- Host: MacBook arm64 (T6030), 11 CPUs, 18 GiB RAM, macOS 15.7.3, ld64
  `ld-1115.7.3`, rustc/cargo 1.99.0. Uncapped jobs, `RUSTFLAGS` unset.
  Target was empty at start; the registry and git sources were already
  downloaded. 3 dirty files: this packet, the script and `mise.toml`.
- The host was already loaded before the build: about 5.7 GB available, with
  2.6 of 3 GB swap in use from other workloads.
- **Result: completed, exit 0, `Finished … in 7m 28s`. Not aborted.** Pressure
  was normal in 207 samples, warning in 5 and critical in 2. The 2 critical
  samples (14:15:02, 14:15:23) were not consecutive.
- System: minimum available 2.1 GB (14:15:02). Swap rose 2.6 → 5.1 GB during
  14:14:57–14:15:23, then stayed flat.
- Build RSS sum (approximate, shared pages counted twice) peaked at 11.6 GB at
  14:15:11: 8.4 GB in linkers (4 `ld` processes of 1.6–2.7 GB plus their `cc`
  wrappers) and 3.2 GB across 11 rustc processes.
- Every pressure event came during a ~30 s burst (14:14:57–14:15:30) in which
  Cargo linked many test binaries at once. 32 separate `ld` processes ran over
  the build. Per-link peaks were 2.3–4.5 GB.
- Largest single processes (peak RSS):

  | Process | Peak RSS |
  |---|---|
  | `wyrd_server` lib-test rustc | 6.3 GB |
  | `wyrd_server` lib rustc | 5.5 GB |
  | `wyrd_server` test link | 4.5 GB |
  | `otlp` test link | 4.2 GB |
  | `mcp` test link | 3.6 GB |
  | `vala_bifrost_redux` test rustc | 2.8 GB |
  | `wyrd_spec` test rustc | 2.7 GB |

- From 14:16:15 to 14:17:52 the `wyrd_server` lib-test compiler ran alone and
  grew 1.6 → 6.2 GB. Its link followed (4.5 GB), while the parent rustc stayed
  resident (2.3 GB).
- Target size after the build: 35 GB.

What this establishes:
- The recorder captures the whole tree and real exit status.
- On macOS, the workspace build's memory peak is concurrent test-binary
  linking overlapping still-running compilers. No single process approaches
  the peak.
- The largest single-process load is the serial `wyrd_server` lib-test
  compile, followed by its link.
- It does not establish the Linux failure. BFD on x86_64 measured about 5 GB
  per `wyrd_server` link, and the server test compile measured 8.5 GB in the
  earlier single-target probe; both exceed their ld64 and macOS counterparts.
  The scaling to RunsOn (8 CPUs, 32 GiB, BFD) is unmeasured.
- Per plan step 3, the Linux evidence still requires the single RunsOn run of
  the unchanged reproducer. It is not yet authorized.

Limitations:
- RSS sums overcount shared pages.
- The 2 s samples miss sub-second spikes.
- The host's pre-existing load (other apps holding swap) shifts the absolute
  system figures.
- `[test]` tagging is by `--test` in the rustc arguments.
- Linkers are attributed to crates via the ppid chain ld → cc → rustc.

#### Step 3 — RunsOn lld candidate workflow (prepared, not run)

Evidence for choosing lld:
`design/archived-run-audit.md`. In run 45831998406, the BFD build had eight
simultaneous links summing about 27.5 GiB RSS, with swap above 10 GiB for more
than four minutes. The matched lld build finished in 9m 41s with at least
20 GiB available. That makes lld the single candidate. Concurrency and
profile stay unchanged. The historical OOM in 45744069536 is not yet fully
explained.

`.github/workflows/ci-debug.yml`:

- **Trigger and isolation:** push to `ci-debug` only. One job, no matrix.
  Concurrency group `ci-debug`, which cancels superseded runs.
- **Runner:** RunsOn `8cpu-linux-x64/ram=32/volume=100gb/extras=s3-cache`,
  with `runs-on/action@v2` and one checkout.
- **Timeout:** 150 minutes.
- **Setup:** the existing `setup-rust-ci`, with `docker-images: "false"`
  because the lanes need Postgres. It keeps free-disk-space and the 16 GB
  swapfile.
  - `bfd-linker: "false"`, so no linker environment is set at any point:
    neither before nor after the cache restore. rustc 1.99's x86_64 default
    is the bundled rust-lld.
  - `cache-key: test-lld` gives a lld-only cache identity. It never restores a
    BFD-built cache. The first run is expected to miss.
  - Cache is restored once and never saved (`save-cache` default false).
- **Configuration record:** logs `cache-hit`, `RUSTFLAGS`, the x86_64 linker
  variable, `CARGO_BUILD_JOBS` (expected unset, so uncapped), CPU count,
  memory and swap.
- **Build:** `mise run ci:diagnose-build` with
  `DIAG_TARGET_DIR=$GITHUB_WORKSPACE/target`. That is the directory
  rust-cache restores and the family lanes use. Cargo always runs; nothing is
  conditioned on cache-hit. A failing build fails the step, which skips the
  remaining non-`always()` steps and fails the job.
- **lld check:** runs `readelf -p .comment` on every executable in
  `target/debug/deps`. Fails unless there is at least one, and every one
  carries the `LLD` linker comment. Prints the `wyrd_server` binaries'
  comments.
- **Family prerequisites:** identical to lints-test's `ci` job: uv/Python
  3.13, rustfmt and clippy, ripgrep, libcurl, pkg-config, and the wasm32
  target.
- **Family lanes:** `mise run ci:diagnose-build -- mise run gate:rust:families`,
  in the same checkout and target. It does not run `test:wyrd` separately,
  because the aggregate includes it. `WYRD_TEST_PACKAGES` is unset, so every
  family package runs. It uses lints-test's `RUST_LOG`.
- **Always steps:** full `dmesg -T` plus filtered OOM lines, then final
  memory and swap. The `ci-memory-diagnosis` artifact contains
  `.dev/ci-memory-diagnosis/`:
  - `runs/build/`: meta, build log, samples, result and oom.
  - `runs/families/`: the same files for the family run.
  - `dmesg.txt`.
  Per-sample `sys` lines also mirror to the live log.

Validation:
- `mise x actionlint@latest shellcheck@latest -- actionlint -config-file <runs-on label config> -shellcheck shellcheck .github/workflows/ci-debug.yml`
  is clean on actionlint 1.7.12, after one SC2024 fix to the dmesg redirect.
- `mise x shellcheck@latest -- shellcheck scripts/ci/diagnose-build.sh` is
  clean.
- The label config marks `runs-on=*` as a self-hosted label.

Limitations:
- The recorder's Linux branch (`free`, PSI and `dmesg`) runs for the first
  time on this job.
- Docker containers started by the lanes sit outside the recorder's process
  group. Only the system-level readings include them.
- A cold `test-lld` cache means this run measures a cold lld build, not the
  archived run's warm-cache start.

Recorder corrections before the first push (maintainer review):
- `rust-lld` was missing from the linker classification, so the lld process
  this experiment measures would have been left out of the `link=` totals.
  The classification now matches
  `ld|ld.bfd|ld.lld|rust-lld|ld64|collect2|cc|clang`.
  - Check:
    `ln -s /bin/sleep <scratch>/rust-lld; DIAG_TARGET_DIR=<scratch>/target DIAG_OUT_DIR=<scratch>/out mise run ci:diagnose-build -- bash -c "<scratch>/rust-lld 5"`
    exits 0.
  - All 3 samples show `link=1/1mb`, with a `proc … rust-lld` line each.
- On Linux, `meta.txt` labelled the output of `ld.bfd --version` as
  `linker=`, which is wrong for the lld run. It now records `installed_ld_bfd=`
  and `installed_rust_lld=`, the latter from the toolchain's `rust-lld`.
  macOS now records `installed_ld=`.
  - The actual linker is confirmed only by the workflow's readelf check of
    the built binaries.
  - The `rust-lld --version` path resolves on macOS but cannot run there,
    because the dylib is missing; this is irrelevant to the Linux branch.
    Its Linux output is unverified until the run.
- `shellcheck` is clean after the fix.

Pending authorization:
- Commit and push of `ci-debug`, which triggers the run.
- `lints-test`, `nightly` and `diagnose-rust-ci` remain disabled.

#### Step 3 — RunsOn lld run 45843864434 (commit 5ce0a1c97)

Evidence (local, gitignored): `.dev/ci-memory-diagnosis/remote/45843864434/`
holds `job.log` (job 139992303642) and `artifact/`. The artifact contains
`runs/build`, `runs/families` and `dmesg.txt`.

Conclusion: job `failure`. The build step passed; the family step failed with
exit 100.

Host:
- RunsOn x86_64, 8 CPUs, 31.5 GiB RAM, 16 GiB swapfile.
- Kernel 7.0.0-1018-aws.
- `RUSTFLAGS`, the linker variable and `CARGO_BUILD_JOBS` all unset.
- Cache `test-lld` missed, as expected; the target directory was empty at
  start.
- Installed linkers: rust-lld 23.1.1 and GNU ld 2.42.

Reproducer:
- Exit 0, `Finished … in 12m 00s`, from a cold target.
- 364 samples.
- Lowest available memory 21620 MiB (15:05:02).
- Highest swap 601 MiB.
- Highest memory PSI avg10 2.86; highest I/O PSI avg10 53.92.
- No kernel OOM lines in `oom.txt` or `dmesg.txt`.
- Largest processes:

  | Process | Peak RSS |
  |---|---|
  | `wyrd_server` lib-test rustc | 7.9 GB |
  | `wyrd_server` lib rustc | 7.7 GB |
  | one `rust-lld` link | 5.3 GB |

- Up to 15 linker-class processes (drivers plus `rust-lld`) and 8 rustc ran at
  once. The summed RSS of 21 GB overcounts shared and mapped pages, and is
  contradicted by the 21.6 GB still available at the same sample.
- Compared with the archived BFD run: lowest available 21.6 GB here against
  2.2 GB there; swap 0.6 GB here against more than 12 GB there.

lld confirmation: every executable in `target/debug/deps` carries the `LLD`
`.comment`, and the check step passed.

Family aggregate:
- `test:wyrd` ran 3033 tests in 597 s: 3032 passed, 1 failed, 365 skipped.
- The failure:
  `wyrd-server verification::hosted::workspace::pg_tests::workspace_cleans_up_on_drop`
  panicked at `crates/wyrd/wyrd-server/src/verification/hosted/workspace/pg_tests.rs:102`.
  - Message: "the emptied directory is reused".
  - left `/tmp/.tmpTYmkVI/.tmpjZIm8J/inputs`; right
    `/tmp/.tmpTYmkVI/.tmp8Ekl1V/inputs`.
  - The reopened workspace got a new directory instead of reusing the
    emptied one.
- mise stopped at that failure, so `test:skald`, `test:vala` and
  `test:shared` did not run.
- Family memory: lowest available 26278 MiB; highest swap 305 MiB; no OOM.
- Reuse: the family step compiled only 9 workspace crates and no third-party
  crate: `wyrd-semver`, `wyrd-spec` (twice), `wyrd-runtime`, `wyrd-sql`,
  `vala-sql` (twice), `wyrd-dev-fixtures`, `wyrd-cli`.

Limitations:
- nextest starts test processes in their own process groups, so test-process
  RSS is outside the recorder's per-process lines. System availability covers
  them.
- The test failure has no memory, OOM or linker signal. Whether lld affects it
  is unverified.

Status:
- The lld candidate removes the observed memory exhaustion in the reproducer.
- It is not verified as a fix until the family aggregate passes.
- No setting changed after this run.

#### Workspace recycling race fix

Cause (maintainer analysis, confirmed):
- In `Workspace::drop` (`crates/wyrd/wyrd-server/src/verification/hosted/workspace.rs`),
  `scratch.release()` ran before the emptied root was pushed onto the free
  list.
- `workspace_cleans_up_on_drop` wakes on that release, so it could reopen
  before the push and get a fresh directory.

Fix:
- The free-list push now happens before `scratch.release()`, and the drop
  rustdoc says so.
- The emptied-before-release invariant is unchanged: `empty_root` still
  succeeds before both.
- A reopen that runs between the push and the release only briefly
  overcounts scratch, which is conservative.

Local reproduction and verification:
- macOS, test Postgres from `scripts/postgres/with-test-postgres.sh`, target
  `.dev/ci-memory-diagnosis/target`.
- Pre-fix binary `wyrd_server-f4bf70d1c982b169 --exact …workspace_cleans_up_on_drop`,
  40 sequential runs: 39 passed, 1 failed with "the emptied directory is
  reused".
- Fixed build:
  `cargo nextest run --locked -p wyrd-server --lib -E 'test(=verification::hosted::workspace::pg_tests::workspace_cleans_up_on_drop)' --stress-count 300 --no-fail-fast`.
  - 163 passed and none had a reuse failure. At the pre-fix 1/40 rate, 163
    clean passes would happen by chance about 1.6% of the time.
  - The other 137 iterations all failed inside the shared Postgres fixture
    start (`could not write init file` / `No space left on device`), because
    the local Docker disk is full; they never reached the assertion.
  - Log: `.dev/ci-memory-diagnosis/local-stress-fixed.log`.
  - The late 11 s failures in the earlier pre-fix nextest stress run show the
    same disk-exhaustion pattern.
- No sleeps, retries or assertion changes. `cargo fmt --all --check` is
  clean.

#### Step 3 — RunsOn lld run 45859766688 (commit 5085fe95b): reproducer and families pass

Evidence: `.dev/ci-memory-diagnosis/remote/45859766688/` holds `job.log`
(job 140047415573) and `artifact/`.

Job conclusion: `failure`, solely from the `Post Install uv and Python` step.
Its error: "Cache path /home/runner/.cache/uv does not exist on disk". The
family lanes never ran uv, and this workflow omitted lints-test's
`ignore-nothing-to-cache: true`. That line is now restored in `ci-debug.yml`,
uncommitted and not rerun. Every build and test step succeeded.

- Runner: RunsOn m8 (8 CPUs, 32 GiB). The cache `test-lld` was not found, so
  the build was cold. The cache is never saved by design.
- Reproducer:
  - Exit 0, `Finished … in 10m 55s`.
  - Lowest available memory 19092 MiB; highest swap 699 MiB.
  - No kernel OOM lines.
- lld check passed.
- `gate:rust:families` exit 0 in 847 s; no OOM.
  - Lowest available memory 25849 MiB; highest swap 83 MiB.
  - 26 workspace-crate `Compiling` lines.
  - Per lane:

    | Lane | Tests run | Result | Time |
    |---|---|---|---|
    | `test:wyrd` | 3033 | all pass, 365 skipped | 589 s, 586 s of it test execution |
    | `test:skald` | 365 | all pass | 1 s |
    | `test:vala` | 1359 | all pass, 23 skipped | 78 s of tests |
    | `test:shared` | 879 | all pass, 231 skipped | 29 s of tests |

  - `workspace_cleans_up_on_drop` passed.
- Whole job: 16:33:13–16:59:36 (26 min).

Result:
- The lld candidate meets the experiment's success criteria: the reproducer
  and the family aggregate pass on RunsOn with lld confirmed and no OOM.
- The historical BFD OOM is attributed to the eight concurrent BFD links
  measured in the archived run. The earlier GitHub-hosted runner losses
  remain unexplained.
- Steps 4–6 not started.

#### Step 5 — normal CI topology (implemented locally, not run in the cloud)

Base: uncommitted on top of `6ccb8d0941da551d8ee064f7378b5bdc7a788697`
(`ci-debug`). Step 4 needed no further correction: lld alone passed.

Topology:
- `select-ci.py::plan_jobs` builds every matrix (full gate, partial,
  Bifrost-only, `--gate`). The `RUNS_ON` shards (`gate:typescript`, then
  `gate:rust:journeys`) merge into one entry with `runs_on: true` and cache
  `journeys-typescript`; on a partial change it holds only their selected
  lanes, and with neither selected it does not exist. Full gate: 7 `ci` jobs
  (checks, families, gateway, Bifrost, storage, Python hosted; one RunsOn).
- `lints-test.yml::ci` and `nightly.yml::gate` pick the runner from
  `matrix.job.runs_on` (the step-3 RunsOn label, else `ubuntu-24.04`) and run
  `runs-on/action@v2` only there. Both run lanes one by one with `mise run`
  in the same checkout; a nonzero lane fails the step. Nightly previously ran
  `mise run <lanes>` once; it now loops, since the combined job has two lanes.
- `deps` removed from both workflows, `ci-complete` needs and result
  arguments. Image, SDK platform, docs and UI jobs are unchanged apart from
  the UI job's cache save.

Shared setup:
- BFD input, step and its `RUSTFLAGS`/linker exports removed; `ci-debug.yml`
  no longer passes `bfd-linker`. No job sets a linker; `scripts/build.sh`
  still adds lld for arm64 images. binutils untouched.
- `prefix-key: v1-rust` retires every BFD-linked entry (default `v0-rust`).
  Nothing changes `CARGO*`/`RUST*` after the cache step, so the key hashes
  the effective environment.
- `save-cache: always` and `cache-on-failure` removed: saves happen on main
  after success only.

Cache identities and writers (main only, after success):

| Backend / key | Writer |
|---|---|
| GitHub / `test` / Linux | first selected `test` `ci` job (families on the full gate; `bifrost` when Bifrost-only) |
| GitHub / `clippy` / Linux | `checks` |
| GitHub / `napi` / Linux | `ui` (runs whenever the addon or UI changes, and on the full gate) |
| GitHub / `test`, `napi` / macOS | unchanged: `rust-client`, `typescript` macOS legs |
| GitHub / `image-<os>` | unchanged: each image leg |
| RunsOn S3 / `journeys-typescript` | the RunsOn job |

- napi always passes `--target <host triple>` (`@napi-rs/cli` 3.4.1
  `cli.js:1496`), so addons land in `target/x86_64-unknown-linux-gnu/`.
  rust-cache v2 caches the whole `target/` and treats triple directories as
  profiles (`src/cleanup.ts::cleanTargetDir`), so the default `. -> target`
  mapping already covers both the host test tree and the napi tree, and the
  image builder's `target/ubuntu-22.04/release`. Files under the builder's
  `target/ubuntu-22.04/home` are removed by the same cleanup. No `workspaces`
  input was added. Whether the image cache actually avoids recompilation is
  unmeasured.

Local verification (macOS, this worktree):
- `mise run check:ci-selection`: 70 + 7 passed, 0 failed. New cases: full
  gate and unknown-path fallback and `--gate` each run every shard exactly
  once, with exactly one RunsOn job (`gate:typescript gate:rust:journeys`)
  and one writer per cache; journey-only, TypeScript-plus-identity,
  TypeScript-only (`plan_jobs` directly), Rust-leaf, server-change,
  Bifrost-only, docs-only and planning-only matrices; workflow assertions for
  the runner expression, `runs-on/action` guard, lane loop, no `deps`, and the
  `ci-complete` needs. Reversing `RUNS_ON` made 5 of them fail.
- `mise x actionlint@latest shellcheck@latest -- actionlint -config-file <runs-on=* label config> -shellcheck shellcheck .github/workflows/{lints-test,nightly,ci-debug}.yml`:
  clean (1.7.12).
- `mise x shellcheck@latest -- shellcheck .github/scripts/*.sh .github/scripts/tests/*.sh`:
  clean, after fixing three existing findings and one new one in
  `test-detect-changes.sh` (SC2016 ×3, SC2043).
- `ruff format --diff` shows no change in the touched `select-ci.py` hunks;
  the file's other pre-existing differences were left alone.
- `git diff --check` clean. No product code, migration, dependency, feature or
  test exclusion changed.

Pending cloud validation (not run):
- A PR whose change takes the full gate: all seven `ci` jobs and the
  existing image/SDK jobs pass; the RunsOn job starts, restores
  (cold first) and runs both shards; `ci complete` reflects real results.
- Then a warm run at identical source to record restore/save size, Cargo
  third-party rebuilds, RunsOn job price and the critical path against the
  ~60-minute target.
- Step 6: re-enable `lints-test` and `nightly`, then remove `ci-debug.yml`,
  `ci:diagnose-build` and `scripts/ci/diagnose-build.sh`.

#### Test-setup corrections after run 45875513959 attempt 2

Two failures in the run, neither reproduced locally as the CI interleaving:
- `test_failed_verification_pages_on_call_across_restart[rust]` failed with
  `no failed window was judged`. The test invoked `cargo run` after recording
  samples. Compilation can delay program startup beyond that window; the
  resulting query exclusion was reproduced locally, but the exact CI compile
  timestamps are unavailable. Now
  `test:local-dev:journey` builds `failed_verification_dispatches` with the
  other binaries, and the test runs `target/debug/failed_verification_dispatches`
  itself. The query, three-minute deadline, webhook and restart assertions
  are unchanged. The guide is unchanged: `docs:check:commands` refuses
  `cargo build`. Public example code and guide commands are unchanged.
- `pg_verification_routes::staging_refusal_recovers_the_same_enqueue_identity`
  hung for 900 s. The test now accepts and settles both fills, holds
  `scribe_outbox().reserve(2)` (the whole reserved share at capacity 3)
  across the keyed request that must return 429, then drops it. The Scribe
  stall and reserve-probe polling are gone; every assertion and count is
  unchanged. Production code is unchanged.

Local verification (macOS, scratch target, `CARGO_INCREMENTAL=0`,
`CARGO_PROFILE_DEV_DEBUG=line-tables-only`):
- `scripts/postgres/with-test-postgres.sh -- cargo nextest run --locked -p wyrd-server --test integration --run-ignored=all -E 'test(=pg_verification_routes::staging_refusal_recovers_the_same_enqueue_identity)'`:
  5 of 5 passed, about 4 s each.
- `cargo build --locked -p wyrd-rust-examples --bin failed_verification_dispatches`
  succeeded, and so did the mise step's final form with
  `-p workspace-hack -p wyrd-testing` appended. The pytest run used the binary
  from the first form. Then the following passed in 146 s, covering
  the first page, the restart page and both webhook bodies:
  `scripts/postgres/with-test-postgres.sh -- python -m pytest -o pythonpath= -q -m local_dev 'tests/integration/test_local_development.py::test_failed_verification_pages_on_call_across_restart[rust]'`.
  It used the root checkout's venv and UI build through temporary symlinks,
  which were removed afterwards.
- `check:ci-selection` (70 + 7), `rustfmt --check`, `ruff format --check`,
  `ruff check`, `docs:check:commands` and `git diff --check` all passed.
- Not run: the whole `test:local-dev:journey` lane, the `[python]` and
  `[typescript]` parameters, and any cloud run.


#### Gateway refresh synchronization and smaller RunsOn runner

Run 45908689141 at `bac00de1e` passed the combined RunsOn TypeScript and
Rust journey job in 61m10s. Reported peak used RAM was 9,319.87 MB; disk
usage reached 86.95% of the 100 GB volume. The job cost estimate was $0.2062.
The only failing gate was GitHub-hosted `gate:rust:families`:
`components::gateway::pg_invocation_tests::usage_cache_bounds_staleness_restarts_and_policy_changes`
received `GatewayLimitExceeded` where it expected the raised concurrency
limit to admit a second call. The earlier corrected tests passed.

Source diagnosis, independently reviewed before editing: `configure` makes
separate deployment, fallback and governance writes. The test waited for any
snapshot replacement, so an intermediate snapshot could satisfy that wait
while retaining the old concurrency limit of 1. The failing CI snapshot was
not logged; that exact interleaving is inferred. The insufficient readiness
condition is visible in the source. The test now waits for the served snapshot
to contain concurrency 2 before making the original invocation and assertion.
The existing reload helper accepts the readiness predicate; its two credential
test callers retain their pointer-replacement condition. No production path,
notification behavior, admission rule, request retry or timeout changed.

With maintainer approval, reduce the single combined RunsOn job to 16 GiB,
retaining 8 CPUs and 100 GB disk. Update both normal workflow expressions,
the selection assertion and current topology documentation. Historical 32 GiB
diagnostic configuration and evidence are unchanged. This is a sizing trial;
the next cloud run must verify capacity and cost on the smaller instance.


Local verification (macOS, scratch target, `CARGO_INCREMENTAL=0`,
`CARGO_PROFILE_DEV_DEBUG=line-tables-only`, `CARGO_BUILD_JOBS=2`):
- `cargo nextest run --locked -p wyrd-server --lib --no-run`: passed.
- Through `scripts/postgres/with-test-postgres.sh`, exact nextest selection
  `test(=components::gateway::pg_invocation_tests::usage_cache_bounds_staleness_restarts_and_policy_changes)`
  with `--stress-count 5 --no-fail-fast`: 5/5 passed, 1.8–2.1 s each.
- Through the same wrapper, exact selection
  `test(=components::gateway::pg_invocation_tests::gateway_managed_credentials_resolve_per_tenant_across_restart_and_rotation)`:
  passed in 1.85 s, covering both other helper calls.
- `mise run check:ci-selection`: 70 + 7 passed.
- `mise exec -- cargo fmt --all --check`, actionlint/shellcheck on both
  changed workflows and `git diff --check`: passed.
- Build emitted a macOS linker warning about large compact unwind metadata;
  compilation and both test commands exited 0.

Next: push this revision for normal PR CI. Full Linux families and the 16 GiB
RunsOn configuration remain pending cloud verification. Nightly stays disabled
while normal CI is being verified.


#### Test readiness and failure-output remediation after run 46007930354

Run 46007930354 passed families (including the gateway refresh correction).
The 16 GiB RunsOn job completed normally: reported peak used RAM 8,385.96 MB,
disk 71.47%, estimated cost $0.0995. TypeScript passed. Rust SDK journeys
reported timeline and repetition failures; the GitHub Bifrost job reported a
memory-refusal test receiving a query timeout instead of resource exhaustion.

Independent read-only diagnosis by `ci_failure_diagnostician`, followed by a
source audit, established these test precondition/workload gaps:
- Gateway answers precede background capture completion; `act_and_read` only
  flushed rows already staged. It now waits up to ten seconds for gateway
  tasks to finish before the existing flush/read and unchanged assertions.
- DataFusion folds constant `REPEAT('x', 100000000)` during planning. CI spent
  approximately 28.5 of its 30 seconds before graph registration. The fixture
  now repeats the row's two-digit id, retaining 100 MB execution-time sort
  keys. Both query forms must still refuse with the typed resource code.
  Dependency allocations during planning are outside the cooperative query
  pool's declared boundary (`architecture/bifrost-design.md:1325,1495`).
- The repetition test observed a provider request without proving the expected
  recovery state persisted. It now uses existing `Desk::accepted` to wait for
  the draft's iteration-0/attempt-1 publication and unsettled iteration-1/
  attempt-1 invocation before takeover. Its exact publication, overlap and
  outcome assertions remain. Unaccepted progress may be lost on interruption
  (`architecture/wyrd-design.md:2278`). This repairs a test precondition; it
  does not establish the cause of the original repetition failure.

The repetition panic was lost when nextest 0.9.143 failed writing its captured
output and exited 110. Individual job logs confirm the mid-line interruption;
the underlying I/O cause is not retained. On GitHub Actions the existing SDK
journey task now directs nextest itself to a regular log file, preserving its
exit status. Both normal workflows always print/upload an existing log with
seven-day retention. Local task output, profiles, test selection, production
code, timeouts, runner shape and cache behavior are unchanged.

TDD: not applicable to this approved test/CI remediation; the maintainer waived
TDD for this CI investigation. No original CI interleaving was manufactured.


Local verification used `mise exec -- bash scripts/postgres/with-test-postgres.sh --`
with the existing scratch `CARGO_TARGET_DIR`, `CARGO_INCREMENTAL=0`,
`CARGO_PROFILE_DEV_DEBUG=line-tables-only`, `CARGO_BUILD_JOBS=2` and tracing on:
- `cargo nextest run --locked -p wyrd-ai --test integration -P journey --run-ignored=all -E 'test(=action_timeline::service_action_timeline_is_one_connected_tree) | test(=action_timeline::service_action_timeline_survives_a_quiet_operator_policy) | test(=verifier_orchestration::repeat_stops_or_exhausts_without_overlapping)' --stress-count 5 --no-fail-fast`:
  all three tests passed in every iteration (15 executions, 53.98 s total).
- `cargo nextest run --locked -p wyrd-testing --test oracle -P journey --run-ignored=all -E 'test(=capacity::memory_refusal_preserves_oracle_health_and_next_query)'`:
  passed in 4.31 s; assertions cover both query forms, release, health and
  the subsequent query. Builds emitted the same macOS compact-unwind warning.
- A temporary shell self-check ran the parsed SDK task body with a command
  returning 7: on GitHub Actions it retained stdout and stderr in the file
  and returned 7; outside CI it printed both streams and returned 7.
- `mise run check:ci-selection` (70 + 7), `mise exec -- cargo fmt --all --check`,
  actionlint/shellcheck for both workflows, shellcheck for the extracted task
  body and `git diff --check`: passed.
- Not run: full suites or another cloud run. GitHub artifact upload remains
  cloud-unverified. Commit requested; no push requested for this remediation.
