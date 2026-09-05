# TASK-004 cumulative remediation review — APPROVE

Reviewed immutable range:
`027fa942a16626fef2c3a0788bca7c1cf05ee7e8..7a4d76ecd1be75f030ee0ac416b60cd94058d4c4`.
Approved authority: SPEC-wyrd-ui-foundation revision 6, original TASK-004,
R1–R5, AGENTS.md, current architecture and accepted CRW/CRWM visual references.
This is a self-review of the cumulative implementation. It approves this task,
not the broader change, deployment or merge. Original independent findings and
prior failed review/captures remain preserved.

## Finding closure

| Finding | Outcome and evidence |
| --- | --- |
| FIND-TASK-004-1 | Remains closed. Draft-owned metadata, immutable snapshots, revision events and team filtering from R1 remain intact; existing HTTP regression passes. |
| FIND-TASK-004-2 | Closed. Both 390 × 844 themes show two compact Review threads plus Submit review. Submission ends at 798px, with its button above the DEV overlay. The Subject diff panel begins at 211px; discussion and anchored threads follow within the accepted viewport regions. Full context remains available through disclosures. Browser measurements and paired comparisons in R2 prove placement, not merely absence of overflow. Desktop Review/Subjects and narrow creation were also visually inspected in both themes. |
| FIND-TASK-004-3 | Closed. fixtureChange constructs a seeded draft from its own intent, impact, owners, subjects and Claims. The real HTTP journey reads the actual rendered ledger form, posts it unchanged, retries, resumes and checks previous revision preservation. Checkout example remains separate. |
| FIND-TASK-004-4 | Closed. Source coordinates require revision, subject, file, old/new side and line. Typed fixtures, validation, selection, Composer, DOM IDs, thread matching and Review links agree. HTTP checks prove distinct old/new line-1 discussions and retry identity, and reject missing/invalid side, nonexistent row/file, other subject and stale revision without partial mutation. Browser line actions and client-side file navigation were also exercised. |
| FIND-TASK-004-5 | Closed. Verification shows identity/version as text; its unused base prop was removed. HTTP regression asserts the fabricated Card link is absent. No Card kind, route or linking abstraction was added. |

No remaining validated TASK-004 findings in this candidate. The previous review's
positive evidence for list/search, Overview, verification state/run actions and
immutable review transitions remains applicable and is backed by current tests.

## Verification

```bash
mise exec -- pnpm --dir crates/wyrd/wyrd-server/wyrd-ui exec vitest run src/lib/features/changes/ChangesJourney.test.ts
mise exec -- pnpm --dir crates/wyrd/wyrd-server/wyrd-ui exec vitest run src/lib/features/changes/review-actions.test.ts
mise exec -- pnpm --dir crates/wyrd/wyrd-server/wyrd-ui exec vitest run src/lib/features/changes/verification-state.test.ts
mise exec -- pnpm --dir crates/wyrd/wyrd-server/wyrd-ui test
mise exec -- pnpm --dir crates/wyrd/wyrd-server/wyrd-ui check
mise exec -- pnpm --dir crates/wyrd/wyrd-server/wyrd-ui build
mise run check:tokens
```

Results: 10 / 5 / 1 focused tests passed; full suite **111 tests in 23 files**
passed; Svelte check **0 errors / 0 warnings**; build and token checks passed.
Cumulative diff whitespace check passed and the reviewed application tree is clean.

Browser checks passed for keyboard Menu Enter/Escape and focus restoration,
revision disclosures, enhanced review submission, old/new line discussion,
and resetting an unsaved composer through client-side file navigation. The
Vite cache separation was verified after test execution: browser builds resolve
the client MediaQuery implementation, preventing the observed server-module reuse.

The original Rust generator/codegen/format evidence is unchanged and reused.
The existing workspace Clippy warning at wyrd-client/src/transport/http.rs:600
remains an explicitly recorded verification limitation; the supplied independent
review classified it as unrelated, not a TASK-004 defect. No check was weakened.

## Visual evidence

[Region measurements](TASK-004-R2-browser-metrics.json).
[Review light](TASK-004-R2-compare-review-mobile-light.png),
[Review dark](TASK-004-R2-compare-review-mobile-dark.png),
[Subjects light](TASK-004-R2-compare-subjects-mobile-light.png),
[Subjects dark](TASK-004-R2-compare-subjects-mobile-dark.png).
Paired desktop captures and narrow creation regression images share the R2 prefix.
