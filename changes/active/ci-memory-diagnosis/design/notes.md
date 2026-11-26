# CI build memory diagnosis

Status: exploring — resumed after completed local investigation and archived Linux evidence audit

## Problem

Rust CI fails before it can give maintainers a reliable test verdict. RunsOn run
45744069536 confirms a kernel OOM kill during the workspace test build in the
`deps` job. It does not establish the victim's peak memory, total swap usage,
or the cause of earlier GitHub-hosted runner disconnects. Previous experiments
changed runner, linker, concurrency, swap, and workload across runs without a
stable comparison. Their branch must be retired, not used as the implementation base.

Main also does unnecessary work: the `deps` job compiles workspace test binaries,
but the cache excludes workspace artifacts; downstream jobs compile them again.

## Desired outcome

Identify the processes and build phase responsible for memory pressure, verify
a minimal correction, and return a real pass/fail result from the existing mise
lanes. CI must prefer bounded memory and reuse of an existing workspace over
parallel builds of the same artifacts.

## Constraints and non-goals

User decisions:

- Work on `ci-debug`, starting from main; retire `diag/runner-loss`.
- Suspend normal CI while investigating; use one simple experiment workflow.
- Attempt diagnosis locally before launching a new RunsOn experiment.
- Optimize eventual CI for low memory and reuse of caches and workspace.
- Preserve the committed feature-alignment fix; do not compile the same graph
  in a preliminary job and then rebuild it in test jobs.

No crate split, toolchain upgrade, new cache service, synthetic load, product
behavior change, or new benchmark. Do not assume Postgres, BFD, a compiler
regression, or the old hosted-runner failures explain the RunsOn OOM. Do not
merge the previous diagnostic setup changes. This design authorizes no remote
mutation or expensive run by itself.

## Proposed implementation

Steps 1 and 2 of TASK-001 are complete. Preserve that work and its execution
record. The local Mac baseline passed and established overlapping links as its
pressure source. Do not repeat it or recreate the deleted experimental branch.

### Next experiment: verify lld on the actual Linux test path

The archived Linux BFD/lld comparison supplies the baseline evidence previously
thought missing. Do not launch another unchanged BFD baseline or job-cap matrix.
Prepare one temporary `ci-debug.yml`, with one RunsOn x86_64 job, using the
existing diagnostic recorder. Keep 8 CPUs / 32 GiB RAM / 100 GB disk, cleanup,
swap, toolchain and line-table profile. Change only linker selection for the
candidate: disable forced BFD and verify a linked output identifies lld.
Keep Cargo concurrency uncapped during this experiment so a job cap does not
obscure the effect. Production concurrency is a subsequent measured decision.

Set the effective compiler/linker environment before the cache action. Use a
new lld-specific diagnostic cache identity; restore once, always invoke Cargo,
and preserve the target directory throughout the job. Cache misses must build
normally. Do not reuse an exact BFD cache identity as proof of lld readiness.
The candidate may start cold; record cache and fingerprint evidence explicitly.

First run the existing `mise run ci:diagnose-build` reproducer with whole-build
recording. If it succeeds, provision the normal family-lane prerequisites and
run `mise run gate:rust:families` in the same workspace. The aggregate includes
`test:wyrd`; do not run that leaf separately. Record compilation and linking
memory during the aggregate as well. Preserve real exit codes and full output;
upload evidence on failure as well as success. Stop immediately on a failed
build instead of continuing into tests. Allow 150 minutes for the complete job;
a timeout is a failed experiment, not an OOM diagnosis.

Use a push trigger limited to `ci-debug`, one active run, no matrix or generic
command inputs. No new cloud run is authorized by this design discussion.
Prepare the diff and retain the normal workflows' disabled state pending the
maintainer's implementation/run instruction.

### Result and follow-through

Success requires the reproducer AND all family tests to pass on RunsOn, a
confirmed lld-linked artifact, no kernel OOM, and retained system/process
measurements. This validates the candidate for the affected build path; it
does not prove the exact allocation breakdown of the original OOM or explain
old hosted-runner disconnects. Cache reuse must be shown by actual Cargo
behavior, not the cache-hit flag. Different feature graphs may legitimately
rebuild; identify rather than hide those differences.

If the candidate OOMs, identify its dominant processes using the recorder before
choosing another change. If lld fails to link or tests fail, retain the error
and investigate that failure; do not silently return to BFD or add a job cap.
Report the failed approach and options before redesigning.

After candidate verification, apply the already-agreed CI simplification:
remove the separate `deps` warmer in PR/nightly, execute lanes sharing the
Rust test graph sequentially in one workspace, restore/save once, preserve
Clippy/napi distinctions and all selected coverage. Update selection/aggregation
consumers, required-job references, tests and TESTING.md together. Preserve the
feature-alignment fix. Only after validating the affected normal CI path should
normal workflows be restored and temporary diagnostic code removed.

## Evidence and rationale

Verified repository facts:

- `lints-test.yml::deps` and `nightly.yml::deps` execute the workspace build.
- `setup-rust-ci/action.yml` defaults to excluding workspace crates from cache
  and sets BFD flags after cache restore.
- `mise.toml::build:server` aligns binary and test features; mise task jobs=1
  does not limit Cargo's internal compiler concurrency.
- `Cargo.toml::profile.dev` already specifies `line-tables-only`.

Original evidence:

- [RunsOn OOM job](https://github.com/bohmian-ai/wyrd/actions/runs/45744069536/job/139655761678):
  SIGKILL of server lib-test compiler and explicit RunsOn kernel OOM warning.
  No process breakdown or demonstrated swap exhaustion.
- [Single-target Linux probes](https://github.com/bohmian-ai/wyrd/actions/runs/45818164324):
  approximately 8.5 GB compiler phase-boundary RSS on x86_64 and 8.3 GB on arm64;
  successful completion does not measure simultaneous workspace pressure.
- Earlier local wrapper sampled only the server test compiler, excluding links
  and other compilers. Its workspace record lasted four seconds.

Primary documentation:

- [Cargo jobs](https://doc.rust-lang.org/cargo/reference/config.html#buildjobs):
  concurrency defaults to CPU count, not available memory.
- [Cargo rustc](https://doc.rust-lang.org/cargo/commands/cargo-rustc.html):
  `--profile test` enables test mode; the successful probe did include tests.
- [rust-cache](https://github.com/Swatinem/rust-cache#cache-details): dependency
  caching and environment-based keys do not preserve the live workspace.
- [RunsOn caching](https://runs-on.com/docs/performance/caching/actions/): S3
  accelerates cache transport; it does not itself change selected cache contents.

Inference: overlapping expensive compiler/linker processes are the leading
RunsOn explanation. The allocation breakdown remains unmeasured. Earlier
GitHub-hosted losses need not share that cause.

## Decisions and alternatives

One portable diagnostic task plus one temporary workflow is sufficient. No
custom profiler, durable telemetry service, crate split, cache warmer, sticky
disk system or experiment matrix is justified now. Sequential execution trades
parallel wall-clock speed for bounded memory and direct artifact reuse.

## Standards conflicts

AGENTS.md forbids platform-specific runtime mechanisms in product code. Native
OS resource observations here are external diagnostic tooling only; no product
runtime, isolation or platform behavior changes are proposed. If that rule is
intended to forbid even diagnostic host tools, maintainer disposition is needed
before implementation. Required gates remain suspended, never bypassed.

## Open questions

The revised next experiment awaits maintainer agreement. Steps 1–2 remain complete.
The archived evidence identifies concurrent BFD links as the observed Linux
memory-pressure source; lld is the proposed correction requiring test validation.
No spec or TDD is required, per the maintainer’s instruction.

## Handoff

Continue from completed steps 1–2: verify the lld candidate with the recorded
workspace reproducer and family aggregate on one RunsOn job; apply the bounded
CI reuse changes; re-enable normal workflows and remove the temporary workflow
and task after real CI passes. Include source/command/cache state, process and
system measurements, exit codes, and before/after explanation in the result.

Validate the recorder with a small child-process command and a deliberate
nonzero exit, proving descendant capture and exit propagation without synthetic
load. Validate workflow syntax with actionlint and run `mise run check:ci-selection`
when CI selection changes. No production implementation or consequential
experiment has been performed as part of this design.

## Archived Linux comparison audited on 2026-11-26

See [archived-run audit](archived-run-audit.md) for exact sources, log lines,
comparison limits and the reason the next experiment selects lld. This audit
supersedes the earlier proposal to collect another uncapped BFD baseline.
