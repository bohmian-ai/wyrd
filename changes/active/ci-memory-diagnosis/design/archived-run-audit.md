# Archived RunsOn build comparison

Audited locally on 2026-11-26. No builds or remote jobs launched.

Sources under `.dev/ci-memory-diagnosis/archive/runs/45831998406/`:

- `job-139950881754.log`: BFD.
- `job-139950881129.log`: lld.
- `run.json`: common source SHA and cancelled workflow conclusion.

## Matched configuration

Both checkout `7dee9a95adf1b5a8e72b3eaaba9ac6360d2011c4` (lines 287–305),
use m8ib.2xlarge, 8 CPUs, approximately 31552 MiB RAM, and image
`runs-on-v2.2-ubuntu24-full-x64-20261125104805` (lines 22–29).
Both log the same Rust 1.99.0 host/hash (line 1616), CARGO_INCREMENTAL=0,
and the same workspace-test build and exclusions. Neither sets a job cap.
Same checkout fixes the repository profile; the logged environment does not
show a profile override. Verbose compiler commands were filtered out, so every
individual compiler argument is not recoverable.

Both restore exactly the same dependency cache:
`v0-rust-test-Linux-x64-f9eb4fa3-582dbce6`, 1996664492 bytes (lines 1680–1686).
BFD flags are installed AFTER restore. BFD logs
`-C linker-features=-lld -C link-arg=-fuse-ld=bfd` at line 1803;
lld logs empty RUSTFLAGS at line 1747 and actual rust-lld processes later.

This is a useful operational comparison with matched starting cache bytes,
but not a controlled cold-build comparison: compiler flag differences can
invalidate restored artifacts differently. Do not assign every difference in
timing or process overlap solely to linker implementation. Future candidate
verification must set linker environment before deriving its cache identity.

## Observed behavior

BFD line 1838: 13:51:20, eight linkers, 24390 MiB summed linker RSS,
10646 MiB swap used. Swap stays above 10000 MiB in 24 samples through
13:55:31 (over four minutes).

BFD line 1853: 13:53:59, eight linkers, 27521 MiB summed linker RSS,
1352 MiB summed compiler RSS, 2241 MiB available system memory,
12712 MiB swap used. The largest individual BFD links are 5435 and 5305 MiB.

BFD line 1861: 13:55:31, eight links still active, 14874 MiB swap used.
No completed build or build-status record survives; the job was cancelled.
This establishes sustained large linker residency and swap usage, not an OOM
kill in this particular job.

lld line 1788: minimum recorded available memory is 20332 MiB. The same line's
summed process RSS exceeds system used memory; shared pages and non-atomic
sampling mean those aggregates are not an exact physical allocation budget.
Use system available-memory readings for capacity comparisons.

lld line 1804: maximum sampled swap use is 591 MiB. Lines 1806–1807:
`Finished ... in 9m 41s`, followed by `build status=0 elapsed=590s`.
The build actually completed even though the enclosing workflow was cancelled.
It did not run the family tests.

The earlier run 45744069536, job 139655761678, contains the actual SIGKILL of
`wyrd-server (lib test)` plus RunsOn's explicit kernel OOM warning. It used a
different machine variant/image from this comparison. The BFD pressure here
supports the mechanism but does not reconstruct that earlier kill's exact
allocation breakdown.

## What cannot be recovered

The old sampler retained process names and RSS, not PIDs, parents or command
arguments. Therefore its eight linkers cannot be attributed to specific test
binaries from these logs. The compile log was filtered too. Do not substitute
the Mac's target mapping for the missing Linux process relationships. The new
recorder already provides the needed detail for candidate verification.

## BFD rationale

Commit `53f3eda2639f2a5faa79742256e6300358872f65` introduced forced BFD on
2026-06-11 with message `Update lints-test.yml`. Its diff supplies no failure
example or technical rationale. GitHub's commit-associated PR lookup returned
no PRs. The current setup input calls it a workaround for lld issues but names
no concrete defect. This is an unresolved historical rationale, not evidence
that lld is safe or unsafe.

Local history also contains arm64-specific linker changes; those do not explain
why x86_64 needs BFD. Do not block the bounded x86_64 candidate experiment on an
unrecoverable historical story. Verify current linking AND runtime tests.

Rust documents that `linker-features=-lld` disables trying lld and that actual
linker choice can involve a cc driver and bundled rust-lld:
https://doc.rust-lang.org/rustc/codegen-options/index.html#linker-features

## Recommendation

The local-analysis step is now complete. Eight simultaneous BFD links are the
observed dominant memory consumer on the Linux comparison. A completed lld
workspace build with much healthier sampled system memory already exists.
Select lld as the next candidate, retain uncapped concurrency and verify the
reproducer plus family aggregate in one RunsOn workspace. Do not repeat an
unchanged BFD baseline or change a job cap simultaneously. If this candidate
fails, use its new whole-process evidence to choose the next action.
