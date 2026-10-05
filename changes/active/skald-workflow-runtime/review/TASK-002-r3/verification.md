# Candidate-bound verification and exclusions

All checks below run against committed snapshot /tmp/wyrd-task002-r3-8a8282331 or explicit base/candidate Git objects, never the dirty live source. Pending follow-up builds/tests are excluded.

- `mise run fmt:check`: started; completion will be recorded below. Log /tmp/wyrd-task002-r3-fmt.log.
- PASS `mise run check:skills-sync` in snapshot; log /tmp/wyrd-task002-r3-skills.log.
- FAIL `mise run check:workspace-hack` in snapshot, exit 1; log /tmp/wyrd-task002-r3-hakari.log. Read log for diagnosis; no baseline checkout or authorship investigation performed. Review does not implement.
- FAIL `git diff --check 0569b79702218600c4f9790f45cc03100d5c6f1c 8a8282331042c3cc7610478b3d46583dbbf121f8`, exit 2: changes/active/skald-workflow-runtime/review/TASK-002-r2/maintainer-review.md:74: new blank line at EOF. This is a cumulative patch whitespace failure, not the dirty tree check claimed in task evidence; do not silently treat the two commands as equivalent.

Committed R2 Implementation Evidence reports all focused SDK/server/SQL commands and broad task lanes PASS. Those are supplied execution claims; no raw logs or independent runtime reruns yet. Source/actual assertions must establish sufficiency; excludes pending follow-up results. Candidate is fixed to commit 8a8282331042c3cc7610478b3d46583dbbf121f8 regardless of subsequent live branch activity.

`mise run fmt:check` completed PASS (exit 0) in committed snapshot. Independent runtime tests were not launched against the live tree because its source includes the explicitly excluded follow-up; no candidate-bound runtime rerun is claimed.

During discovery the live branch advanced to 375d97e67f3affe0d5c59727ef3135b22a459140 (`fix(workflow): refuse malformed Workflow load references with a Workflow error code`). The reviewed candidate was NOT replaced: every reviewer reads the immutable 8a8282331 archive. Newer follow-up source/test results are outside this verdict. Snapshot contents independently match a fresh archive of the same candidate (`diff -qr`, exit 0, log /tmp/wyrd-task002-r3-integrity.log); no source content changed. Tar UID/GID extraction metadata is not a code difference.
