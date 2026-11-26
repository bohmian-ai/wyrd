# CI build memory diagnosis

Status: accepted — gate:typescript and gate:rust:journeys share one RunsOn job; implemented locally, cloud validation pending

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

### Current proposal and scope

The maintainer rejected combining the four Rust gate shards. Preserve all eight
parallel gate jobs and all existing SDK OS/Python, UI, docs and image matrices.
Adopt lld by deleting shared setup's BFD input/step/exports; retain intentional
arm64 lld selection in scripts/build.sh. Do not uninstall binutils.

Accepted topology (maintainer, 2026-11-26): the two GitHub lost-communication
workloads are `ci (gate:rust:journeys)` and `ci (gate:typescript)`, both Linux
x64 and neither an image job. Lost communication alone is not proven OOM. They
are co-located as ONE RunsOn job (8cpu-linux-x64/ram=32/volume=100gb/s3-cache):
selected TypeScript lanes first, then selected journey lanes, in one checkout
and target directory, with one `journeys-typescript` cache. A full gate is
seven concurrent `ci` jobs: six GitHub-hosted shards plus that RunsOn job.
Every other job keeps its runner, selection and parallelism. This supersedes
the "eight independent shards" and "up to two paid jobs" text below.

Original note: RunsOn is reserved for demonstrated memory failures, not every
Rust job. Reference run 40936327078 reports five workload failures (image x64, image
arm64, Rust journeys, gate TypeScript, macOS TypeScript) and passes families,
gateway and Bifrost. Its logs return HTTP 410; annotations give only exit 1/2.
Do not equate those five failures with five memory failures. The run is at
1b06b00b070c212713c8d7bd50bc004adaeff3b8, October 29.

Current code tracing distinguishes image release builds from dev test builds:
image -> startup scripts -> scripts/build.sh image -> bundle_in -> container
release/cloud server, native SDK addon and UI. target/ubuntu-22.04 is separate
from host target/debug; host RUSTFLAGS is forwarded. Rust journeys includes
identity providers, SDK and UI work, not solely Rust nextest.

### Proposed cache policy: separate backends, explicit compatibility

Use existing GitHub cache for GitHub-hosted jobs. Use RunsOn S3 Magic Cache
only on jobs actually assigned to RunsOn. Do not synchronize/copy caches across
providers. Matching names never imply shared entries. Native artifacts are
separated by OS, architecture, toolchain, build environment and build family.
Retain current test/Clippy/napi distinctions; retain separate image caches by
architecture and Ubuntu 22.04 builder environment. Do not promise dev test
artifacts satisfy release/cloud image builds.

Cache families and writer rules:

| Backend / family | Contents and consumers | Writer |
| --- | --- | --- |
| GitHub / test / OS+arch | Cargo sources and compatible dev/test third-party target artifacts; GitHub Rust test/Python/native-client consumers | Linux: one selected test-cache matrix job, preferring families; macOS: existing Rust-client writer |
| GitHub / clippy / OS+arch | Check-only artifacts used by checks | Selected checks job |
| GitHub / napi / OS+arch | Native TypeScript addon artifacts; existing TypeScript/UI consumers | Linux selected TypeScript gate; macOS existing TypeScript writer |
| GitHub / image / arch+builder | Cargo release dependencies in target/ubuntu-22.04 | Image matrix leg, if it remains GitHub-hosted |
| RunsOn S3 / matching build family / OS+arch+environment | Only the dependency artifacts used by migrated jobs; no GitHub restore fallback or copy | One selected job per compatible family |
| Existing mise, uv, pnpm and image-layer caches | Existing tool-specific contents | Preserve current owners; avoid enabling unused uv cache in Rust-only jobs |

Normal writers save on main after success. Reads are not restricted to main;
PRs restore eligible base/default branch entries. Preserve per-family hashes
of toolchain, flags, manifests/lockfiles and platform. Bump the Rust cache
namespace once for removal of BFD; never use a raw hit flag to skip Cargo.
For partial selections, elect the first eligible selected writer for its family;
that avoids a writer existing only on full-gate runs. Different backends need
separate writer elections. No single writer can publish to both automatically.

Retain dependency-only caching initially. Do not enable workspace-crate caching
everywhere: the GitHub cache footprint is already roughly 10.6 GB, and broad
workspace snapshots add expensive transfers and eviction risk. Reuse complete
workspace artifacts directly between steps within each job. Workspace crates
may still compile in parallel jobs; eliminating all duplication would require
serializing jobs or distributing build artifacts, neither justified here.
Do not promise no repeated compilation across isolated runners.

Image jobs must configure the actual builder target directory explicitly with
the existing rust-cache `workspaces` mapping (`. -> target/ubuntu-22.04`), rather
than relying on the shared setup's default `. -> target`. Expose that mapping
as an action input only for consumers that need it; keep the default unchanged
for other jobs. This caches dependency artifacts, not all of target/ (which
also contains temporary HOME, logs and provenance). Measure restore/save size
and recompilation before claiming the image cache is effective.

### Remove warmer without losing parallelism

Remove `deps` from lints-test/nightly and their needs/result arguments. Gate
jobs depend only on selection, start in parallel and restore their appropriate
main cache independently. The normal lane commands build missing artifacts;
there is no replacement preliminary whole-workspace compile. Update selector
cache-save metadata to replace deps as the Linux test-cache writer.

Tradeoff: first runs after a toolchain/lockfile/key change may compile common
dependencies independently on several runners. Warm runs avoid the global
warm-up barrier. Do not hide that cold-start cost. Retain feature alignment and
existing profile. SDK/UI/image jobs keep their existing prerequisites.

### Structure and observable success

PR/main: changes -> eight independent selected gate shards, matrix-plan ->
existing SDK matrices, and independent selected docs/UI/image jobs -> existing
ci-complete. Each confirmed affected job changes runner only; it does not join
another shard. Up to two paid runner jobs are proposed if both selected jobs
still require RunsOn under lld; the exact routes are unresolved. Nightly keeps
its eight independent gate shards plus isolated Postgres; it only pays for
migrated jobs that it actually contains. Images are not part of nightly today.

Retain selection, branch triggers, superseded-PR cancellation and actual
required-job results. Do not alter cadence/coverage to fit an assumed budget.
Do not promise a one-hour result by increasing timeout: target warm full PR
completion at or below 60 minutes and show the actual critical path.

Before implementation, identify the two memory-failing jobs. Then validate
YAML and selector tests, run the normal selected path with lld, and compare one
cold/new-cache run and one warm run at identical source/configuration. Record
all lane results, critical path, Cargo third-party rebuilds, restore/save times
and bytes, kernel OOM and paid job estimates. Investigate functional failures
separately from runner resource failures. Re-enable normal workflows and
remove diagnostic tooling only after the actual path passes. No extra paid
run merely to fix the diagnostic uv post-step is needed.

### Cost boundary

User target: less than $5/month on RunsOn, while keeping warm PRs within about
an hour. Existing diagnostic job estimates $0.0940 for 26m58s billed (EC2
$0.0823, EBS $0.0117), source remote/45859766688/job.log:10784–10796.
This is families plus cold build, not the price of either unidentified affected
job. Two such jobs would be $0.188/run; twenty such workflow executions would
be $3.76 before infrastructure, caches or licensing. These are arithmetic
illustrations, not forecasts. Do not apply x64 prices to arm64 jobs.

Total cost is license + baseline infrastructure/cache charges + sum of paid
job costs including retries. Confirm existing license/baseline and monthly
qualifying PR/main/nightly volume, then use actual warm job prices. Free OSS
licensing exists; paid licensing alone exceeds this budget. No custom paid-run
scheduler, automatic runner fallback, persistent warm pool or global queue.

### Current cache audit

GitHub API on 2026-11-26 reports roughly 10.6 GB cached. The listed Linux test
entries belong to refs/pull/244/merge and refs/heads/diag/runner-loss; no listed
main Linux test cache. Main macOS test/napi entries exist. Do not assert these
branch-scoped Linux entries seed unrelated PRs, or claim cache eviction as their
absence's cause. Disabled CI and failed warmers are other possible causes.
No cache deletion or workflow mutation was performed.

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

## Consolidation feasibility verified from source and run labels

| Failed workload in reference run | Architecture / context |
| --- | --- |
| image (ubuntu-24.04) | Linux x64; Ubuntu 22.04 container release/cloud build |
| image (ubuntu-24.04-arm) | Linux arm64; Ubuntu 22.04 container release/cloud build |
| ci (gate:rust:journeys) | Linux x64; mixed Rust/SDK/UI dev-test work |
| ci (gate:typescript) | Linux x64; native napi/dev-test work |
| typescript (macos-15) | macOS arm64; native platform SDK tests |

Run API labels confirm these architectures. Two Linux x64 workloads could
share a job, subject to measured serial duration and feature/target differences.
The two image legs require different native architectures; keep them parallel
on separate native hosts. Combining image build and startup execution on one
host requires emulating the other architecture, not just cross-compiling Rust.
Docker documents emulation can be much slower for compilation:
https://docs.docker.com/build/building/multi-platform/ . macOS SDK coverage
cannot be replaced by Linux RunsOn. Exact memory-failing pair remains unproven
because logs return 410 and check summaries/text are null. No identity inferred.

## Open questions

- Resolved: the two routed jobs are `ci (gate:rust:journeys)` and
  `ci (gate:typescript)` (lost communication, not proven OOM).
- Does lld make either job viable on free GitHub runners? Families on RunsOn
  does not establish image or complete journey behavior on GitHub.
- Actual paid-job duration/cost, monthly run volume, existing license and
  baseline infrastructure costs; needed to establish the $5/month target.

The selective routing above is approved; no spec or TDD is required per the
maintainer's instruction. The previous single-RunsOn/all-Rust design is rejected.

## Handoff

Workflow changes are implemented locally; see the plan's "Step 5" execution
record. Validate the real normal path and warm-cache performance on a PR before
removing diagnostics and restoring CI.

## Latest experiment and primary documentation

Run 45859766688 completed the cold lld build in 10m55s, confirmed lld-linked
binaries, and passed families (wyrd 3033, skald 365, vala 1359, shared 879).
No kernel OOM. Only the unused uv cache post-step failed. Journeys/image paths
were not verified by that run. See plan execution record for retained evidence.

- [rust-cache](https://github.com/Swatinem/rust-cache): target mappings,
  dependency-only default, workspace opt-in and compatibility keys.
- [RunsOn caching](https://runs-on.com/docs/performance/caching/how-it-works/):
  S3 and GitHub cache backends are distinct.
- [RunsOn pricing](https://runs-on.com/pricing/): AWS charges are separate;
  free OSS/non-commercial licenses and paid plans exist.

## Archived Linux comparison audited on 2026-11-26

See [archived-run audit](archived-run-audit.md) for exact sources, log lines,
comparison limits and the reason the next experiment selects lld. This audit
supersedes the earlier proposal to collect another uncapped BFD baseline.
