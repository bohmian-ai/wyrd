# TASK-004 immutable review subject

Repository: /home/thorrester/Documents/GitHub/wyrd-worktrees/verifier-building-blocks-task-004

- Base: 41b0f01d9b47d9b07b078b2431fcdd4cba56a9b0 (parent of the user's first implementation commit; review includes 52fc831f8).
- Candidate: 07adb7907951f343544e550f6cabc5b7aa3aadea.
- Input packet commit / observed HEAD: aa32c821ef74886526b7367bfd77d51c87e0d521. This adds only packet/evidence files after the candidate. Production source in this clean checkout matches candidate. Use git show candidate:path if uncertain.
- Branch: change/verifier-building-blocks-task-004.
- Authority: approved SPEC-verifier-building-blocks r7, changes/active/verifier-building-blocks/spec.md (snapshot here); original TASK-004-sdks-examples-and-docs.md from packet commit (snapshot here), plus tasks/README.md and r7 TASK-005 handoff. Original task r6 frontmatter is preserved history; r7 approved handoff coordinates task ownership.
- Cumulative diff: cumulative.diff; git diff BASE CANDIDATE. Include complete task, not only latest fix or renames.
- No prior TASK-004 verdict or remediation exists. Task001 reports are historical upstream inputs, not current task findings.
- External demo is in original write set. Its uncommitted candidate was frozen at /tmp/wyrd-task004-external-07adb7907 from wyrd-example-ts HEAD 000ed1f2f587a6629a471c51aadff5e05f0c820b plus tracked/untracked nonignored files. external.diff and external-snapshot.sha256 record it. Inspect snapshot, not mutable external checkout. This external candidate has no implementation commit; state that verification/provenance limit explicitly.

## Shared navigation map (starting point, not coverage or conclusion)

- Shared retained client/state: crates/shared/wyrd-client/src/observe/{mod.rs,lifecycle.rs,drift.rs,tests.rs}, state.rs, verify/{mod.rs,execution.rs}. Target projects named evidence and uses transport; Execution owns status/wait; File and ArtifactRef describe evidence.
- Existing alternatives to compare: cards/handle.rs connection conventions, storage/{mod.rs,upload/mod.rs} byte transfer and cards/saga/hash_artifacts.rs hashing. Follow actual producers/callers before judging new orchestration.
- Foreign runtimes: sdks/wyrd-sdk-python/src/{verify.rs,observe/mod.rs,lib.rs}, python/wyrd/{verify,observe}, exports/stubs; sdks/wyrd-sdk-ts/native/src/{verification.rs,cards.rs,lib.rs}, wyrd/src/index.ts and generated declarations; Rust src/lib.rs.
- Tests: shared observe/tests.rs and verify modules; Python unit/state/test_observe_surface.py and test_public_exports.py; TypeScript tests/unit/observe.test.ts; three-language integration verifier_building_blocks, verify_in_real_time, scheduled_drift_alerts_operator, support_desk, action_timeline, existing sibling journeys. mise.toml owns discovery. Changed server harness/capacity/MCP tests also require review.
- Examples: examples/support-desk/{service,rust,python,typescript,README.md}; fixtures/cards/{verifier_building_blocks,verify_in_real_time,scheduled_drift_alerts_operator,gateway_jev,...}; frozen external demo as above.
- Docs: changed build-and-verify guides, new extraction-output/runtime-limits/agent-tools references, diagrams and landing page; architecture/{wyrd-design.md,bifrost-design.md,wyrd-doctrine.mdx,references/...}; AGENTS edits and schema/TS generators.
- Sensitive domains selected: (1) security/tenant authority and evidence trust boundary, including reconnect/credentials, artifact/upload authorization projections and permission types; (2) client concurrency/recovery semantics: replay key retries, timeout, polling, cancellation, lazy file transfer and shared lifecycle. No production persistent SQL changed; domain reviewers must verify actual scope.

## Available verification evidence

Task-snapshot.md Implementation Evidence records implementation-reported checks: 28 client observe/verify tests, Clippy all features/lib-only, Python 628 and TypeScript 70 unit tests, runtime format/lint/typechecks, ts:napi:check, codegen:check, docs:check, check:examples, fmt, check:deps. These are reported results, not logs independently reproduced by review.
Full SDK/server journeys are authored but deliberately deferred by task/coordination to final merged change after TASK-005. Do not run them or require broad journey aggregates as task-remediation proof. Audit whether authored assertions satisfy assigned coverage.

Review is read-only except each agent's assigned report. Do not edit source, task, authority, run codegen, or run concurrent Cargo-backed commands. Request focused execution through orchestrator if indispensable. Follow CodeGraph first (.codegraph exists); its source is reread on disk. Read AGENTS, agent rules, spec/task/coordination, reference router and applicable authority yourself. No implementation completion narrative is supplied as review truth; verification evidence may be consulted as claims. Reports from other discovery roles must not be read before independent completion.

## Review-run verification

Orchestrator reran `mise exec -- cargo nextest run --locked -p wyrd-client --lib -E 'test(/^observe::|^verify::/)'`: 28 passed, 233 skipped (client-check.log). No full integration/journey suite run, per approved task deferral. `git diff --check` also passed on the review checkout. At this checkpoint HEAD remained aa32c821ef74886526b7367bfd77d51c87e0d521 and only review artifacts were untracked.

The frozen external source is additionally preserved in `external-source.tar.gz` (24 KiB), paired with its SHA-256 file manifest. The temporary extraction path is only a navigation convenience; the archive preserves the review input.
