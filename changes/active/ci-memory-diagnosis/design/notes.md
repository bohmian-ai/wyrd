# CI build memory diagnosis

Status: exploring — lld build and families verified; normal CI routing and cost proposal resumed

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

The diagnostic build and all four family lanes now pass with lld. Move to the
normal workflow revision; do not repeat the diagnostic merely to fix its uv
post-job cache error. Do not describe journeys as validated: the diagnostic
ran families, not gateway, Bifrost or the other journeys.

Proposed bounded revision, following the maintainer's instruction to use
RunsOn only where needed:

1. Remove the forced BFD selection from shared `setup-rust-ci`. Keep the pinned
   Rust toolchain, existing profile, cleanup, swap and feature alignment.
   Establish effective build flags before cache restoration and use an lld
   cache namespace so old BFD entries cannot masquerade as compatible hits.
2. In PR/main and nightly workflows, run selected heavy Rust lanes (families,
   gateway, Bifrost and journeys) sequentially in one RunsOn job with the
   verified 8 CPU / 32 GiB / 100 GB configuration and S3 cache. Preserve existing
   change selection: run only the selected lanes, exactly once. Families is
   verified on this machine; the remaining three lanes still need validation.
   Keep checks, SDK platform jobs, storage and other jobs on their current
   GitHub-hosted runners unless actual failures show a need to move them.
3. Remove the separate `deps` warmer. Invoke existing selected mise lane
   commands directly, in a single checkout and target directory. Do not add
   another preliminary whole-workspace build. Cargo builds the artifacts each
   lane requires and reuses compatible artifacts from prior lanes.
4. Restore once before those lanes and save once afterward, including workspace
   crates. Use the existing cache action, with one successful main writer for
   the RunsOn Rust-test cache. PRs restore main's cache; misses build normally.
   Keep Clippy/napi caches distinct. Always invoke Cargo regardless of a hit.
   GitHub and RunsOn S3 caches are separate backends; do not build cross-provider
   synchronization or introduce a new cache service.
5. Update selector/aggregation consumers and their tests, required-check
   references and TESTING.md for the consolidated Rust job. Preserve coverage
   and failure propagation. Retain cancellation of superseded PR runs.
6. Validate YAML and CI-selection checks locally, then one normal full Rust
   path and one warm-cache run. Record lane results, OOM evidence, Cargo rebuild
   behavior, cache transfer time, total duration and estimated EC2/EBS cost.
   Re-enable normal workflows and remove temporary diagnostic workflow/task/
   recorder only after the normal path passes. Leave evidence archived.

This is design, not authorization to edit workflow code, push, enable workflows,
or launch paid runs. The already-existing plan's later steps must be reconciled
with this current proposal before implementation; preserve its execution record.

### Exact topology and cache changes (proposal; not accepted)

The maintainer rejected the earlier summary as insufficiently specific. The
following makes the proposed concurrency change explicit; it is not approval.

| Current PR/main or nightly gate path | Proposed path |
| --- | --- |
| Separate `deps` job blocks all eight gate shards | Delete `deps`; downstream jobs depend only on their selector |
| Four parallel Rust shards: families, gateway, Bifrost, journeys | One RunsOn job, selected lanes sequentially, same target directory |
| `gate:checks`, `gate:storage`, `gate:python`, `gate:typescript` | Four independent GitHub-hosted jobs; remain parallel with RunsOn |
| SDK OS/Python matrices, docs, UI, images | Existing runners, selection and parallelism retained |
| Nightly isolated Postgres job | Remains independent on GitHub-hosted runner |

A full eight-shard gate therefore becomes five concurrent jobs, not one serial
pipeline. Only the four Rust shards are consolidated. It sacrifices their
parallel wall-clock execution to avoid repeating workspace compilation and
paying for four RunsOn machines. Complete Rust duration remains unmeasured.
No other job waits for RunsOn merely to obtain a cache.

`select-ci.py` must partition by owning gate, not a loose name prefix:
`gate:rust:{families,gateway,bifrost,journeys}` and their partial lane members
route to RunsOn; the special `verify:bifrost` selection also routes there.
Preserve `WYRD_TEST_PACKAGES`, lane order, selected coverage, empty selections,
and the conservative full-gate fallback. Emit the RunsOn lane list separately
from the remaining GitHub matrix. Do not change local mise gate definitions
merely to implement runner routing.

`ci-complete` drops `deps` from needs and from its result arguments, includes
the consolidated Rust job, and continues reporting every selected job's actual
failure. Intentionally empty job groups may skip; a failed/cancelled selected
job cannot become success. Update existing selector tests for full/partial/
Bifrost-only/docs-only selections and exactly-once coverage.

Cache ownership must reflect separate backends, not pretend `test` is shared
across S3 and GitHub:

| Backend/cache | Restore consumers | Main writer |
| --- | --- | --- |
| RunsOn S3 / lld Rust-test cache | Consolidated Rust job | That job after success |
| GitHub / lld test cache | Existing GitHub storage/Python/native Rust jobs | One selected eligible Linux test-cache matrix job |
| GitHub / lld Clippy cache | Checks | Checks |
| GitHub / lld napi cache | Existing TypeScript consumers | Existing designated TypeScript writer |
| Other OS/image/tool caches | Existing consumers | Existing writers |

On GitHub, mark one eligible test-cache matrix writer now that `deps` is gone;
retain existing main-only save policy for normal jobs. RunsOn cache includes
workspace crates. Retain the existing GitHub cache contents initially: extending
workspace caching everywhere is not required for this change. PR jobs restore
main caches but do not publish them. First runs on a new identity are cold;
parallel cold jobs can compile common dependencies independently. Caches speed
later runs and are not a same-run artifact handoff. No replacement warmer or
cross-provider copy step is proposed.

Shared setup removes the `bfd-linker` input, BFD step and BFD-specific env
exports/descriptions. Do not uninstall binutils or delete `ld.bfd` from images.
Remove the obsolete diagnostic input when removing that workflow. Bump the
Rust cache namespace once; preserve toolchain/environment/manifest hashing.
Keep default lld for the verified Linux x64 toolchain; do not claim macOS or
other targets use lld, or force Linux-specific flags onto them.

### Cost boundary

The maintainer's target is less than $5/month on RunsOn. Actual run
45859766688 estimated $0.0940 for 26m58s billed time: $0.0823 EC2 and $0.0117
EBS. Source: `.dev/ci-memory-diagnosis/remote/45859766688/job.log`, lines
10784–10796. This is an estimate for a cold build plus families, not a full
Rust gate or the total AWS bill. Roughly 53 such jobs consume $5 before other
costs. A daily job at that measured price would consume about $2.82 in a
30-day month before PR/main runs or other costs.

Budget model: license + infrastructure/cache costs + sum of billed job costs.
Do not promise the target from cache hits or sample rates. Verify the existing
license and baseline AWS charges, then measure the complete cold/warm Rust job
and compare expected monthly PR/main/nightly executions against the remaining
budget. Preserve current triggers while discussing cost; changing coverage or
nightly frequency needs a maintainer decision. No custom budget scheduler,
automatic runner fallback or persistent runner pool is proposed.

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

- Confirm the current RunsOn license and whether the $5 target covers all AWS
  and licensing charges or runner compute alone. Paid licensing exceeds this
  target by itself; do not infer eligibility from repository visibility.
- Expected monthly qualifying PR/main executions and nightly usage, needed to
  assess budget feasibility after the complete Rust path is measured.
- The maintainer has requested lld adoption and selective RunsOn routing; the
  exact grouping and cost proposal above remains proposed until agreed.

No spec or TDD is required, per the maintainer's instruction.

## Handoff

Resume at the production CI revision, preserving completed diagnostic evidence
and the independent workspace recycling fix. The experiment's remaining uv
post-job cache error does not invalidate the passing build and test evidence.
The next proof is the normal Rust path including journeys, followed by actual
warm-cache reuse. No workflow or production code was changed during this design.

## Latest experiment and external cost evidence

Run 45859766688 (source 5085fe95b) completed its cold lld build in 10m55s,
confirmed lld-linked binaries and passed families (wyrd 3033, skald 365, vala
1359, shared 879 tests). No kernel OOM; build minimum available RAM 19092 MiB,
maximum swap 699 MiB. Only the unused uv cache post-step failed. See the plan's
execution record for retained logs and full limitations.

- [RunsOn pricing](https://runs-on.com/pricing/): free OSS/non-commercial
  licensing is offered; paid plans start at EUR 300/year. AWS charges are
  separate. Actual license and installation overhead are not inspected.
- [RunsOn cache mechanism](https://runs-on.com/docs/performance/caching/how-it-works/):
  S3 and GitHub-hosted cache backends remain distinct.
- [RunsOn cost controls](https://runs-on.com/docs/costs/cost-control/): spot
  costs vary and per-job estimates do not establish a monthly total.

## Archived Linux comparison audited on 2026-11-26

See [archived-run audit](archived-run-audit.md) for exact sources, log lines,
comparison limits and the reason the next experiment selects lld. This audit
supersedes the earlier proposal to collect another uncapped BFD baseline.
