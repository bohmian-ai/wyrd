# Immutable inputs and verification log

Base: `f4e79ea00523af5cd9212fdacde64f16c97ad554` (`f36150646^`).
Candidate: `365bfc94bfef1a153f213462f7ef104afa823741` (HEAD at start).
Review range: base...candidate, all 15 implementation commits.
Worktree initially had only untracked `changes/active/verifier-review-pipelines/`; tracked source was clean.

Untracked authority SHA-256 at start:

| Input | SHA-256 |
|---|---|
| spec.md | b79e09e88a701a18c6e18b686aff9c98056f4d765b6862fba4dff6fb9b77e596 |
| public-contracts.md | faa219855227b614a41b89bc9054ef3afbcf13a0e9b2ce7e8ead568ece3519a6 |
| output-publication.md | 77076975cb28ac37e620034270312b47374b2f7a91f91c92927675dfaa190cad |
| tasks/TASK-001-durable-agent-work.md | ea95ef980ec479fee143f3eacad8ec4ff0cf1b4a3a8467e4be550966d62eae4d |

Paths above are relative to `changes/active/verifier-review-pipelines/`.

Independent review checks:

- `git diff --check f4e79ea00523af5cd9212fdacde64f16c97ad554...365bfc94b`: passed (exit 0, no output).
- Initial source and candidate identity check: passed.

Implementation runtime/check results are available as recorded evidence in the original task. They have not thereby been independently rerun. No broad aggregate required for this static acceptance audit. Any targeted reruns will be added here with actual results.

## Live checkout integration during validation

The live checkout moved to `fe7a41071dafdeb7939090151654db2d2fe9de99` with conflicts during an independently authorized integration from main. Validation paused immediately. The human clarified that the integration was approved and did not change this review's subject, and explicitly directed continuation.

The reviewed candidate remains the original immutable `365bfc94bfef1a153f213462f7ef104afa823741`. Remaining source validation moved to detached checkout `/tmp/wyrd-task-001-review-365bfc94b`, verified at that exact candidate with clean tracked source. The four untracked authority inputs were copied there and their SHA-256 values match the start-of-review table exactly. Reports remain in the original review directory. No rebased or main-integration source is substituted into this attempt, so no patch correspondence or acceptance carryover to that integration is claimed. The final verdict covers only the original range; the integration needs its own later verification/review.

Final checks: detached candidate identity and clean tracked source reconfirmed; all four authority hashes still match. All eight discovery reports, focused follow-up, independent validation, verdict and self-contained remediation exist and map the same 15 retained IDs. `git diff --check` on the original range passed again. No runtime proof was rerun. The independent validator additionally confirmed the remediation's selected shared execution-row serialization/fresh-snapshot approach is within its validated correction boundaries, with upload IO outside transactions.
