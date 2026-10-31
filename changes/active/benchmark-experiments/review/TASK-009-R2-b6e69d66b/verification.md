# Verification and provenance

Immutable subject: prior candidate `f0ea3b4023e544d0f95e9b5e38d398105dbe2e33`, candidate `b6e69d66b5657881f7d605a0e308b2985464c420`. Initial HEAD equals candidate; original untracked diagram is preserved. Prior review reports are recorded by `a8590dcc8`; the remediation is `b6e69d66b`.

- `mise tasks ls --hidden`: exit 0; existing build/server journey/migration owners located.
- `mise exec -- git diff f0ea3b402 b6e69d66b --check`: exit 0.
- CodeGraph exploration used before code navigation; relevant symbols and downstream owners are in scope.md.

Fresh focused proof attempted:

```bash
mise exec -- scripts/postgres/with-test-postgres.sh -- bash -lc 'mise run db:migrate:all:inner && mise run build:server && mise exec -- cargo nextest run --locked -p wyrd-testing --test server -P journey --run-ignored=all --test-threads=1 -E "test(=verification_runtime::queued_origins_share_ack_gated_admission)"'
```

Exit 1 before migrations/build/tests. `/tmp/task-009-r2-v18.log` records Docker API permission denial at `/home/thorrester/.config/colima/default/docker.sock`. The canonical wrapper attempted its existing single rebuild and teardown, also denied. This is an environment refusal, not a test failure. No elevated execution or alternate database lifecycle was attempted.

Available implementation proof inspected directly, outside review output:
`/tmp/claude-1000/-home-thorrester-Documents-GitHub-wyrd-worktrees-benchmark-rev19/cdd11ca1-bd0f-4d66-883d-a1d0f87d1205/scratchpad/v18.log`.
This log shows the canonical migration owner (one SQL and one Vala migration test), build:server, recompilation of wyrd-server/wyrd-testing and the exact journey with nonzero selection. Nextest run `b379e204-1d3a-4f1e-9018-d48fbcc5a7f3`: 1 run, 1 passed, 55 skipped, 33.473 s. File timestamp is Oct 31 07:46, before candidate commit Oct 31 07:47:39 -0400. This corroborates the remediation's recorded 33.5 s result; the log itself has no source SHA and is not claimed as a fresh reviewer run or an independently cryptographically pinned final-candidate execution. Prior `v18.txt` is older proof and is not substituted for remediation execution.

No full gate, benchmarks, publication, commits or pushes ran. V13 settled-source integrated proof and V14–V16 final-source closeout remain pending under the user's explicit sequence. Older five L1 records retain their maintainer-accepted validity and original SHA. No new broad suite, lints or format rewrite was run during review; implementation evidence records those checks. Runtime change in this remediation is zero: production method documentation only; the changed test and its actual assertions receive independent source review.
