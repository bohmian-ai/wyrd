# Benchmark records

Recorded benchmark runs, one self-contained JSON record per run, at
`<engine>/<experiment>/<UTC timestamp>-<commit>.json`. Each record names the
commit, host and controls it was measured with. Only complete, clean,
unprofiled full runs are kept here.

This branch shares no history with the code. Check it out as a worktree at
`benchmarks/history/` of a code checkout:

    git worktree add benchmarks/history benchmark-records

Git checks a branch out in one worktree at a time; to use it from another
checkout, move it there with `git worktree move`.

After a run worth keeping, commit its new record on this branch and push it.
