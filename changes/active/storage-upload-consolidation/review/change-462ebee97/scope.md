# Integrated storage-upload-consolidation review

Root: `/Users/stevenforrester/Documents/GitHub/wyrd`.
Base: `3899b0a35ec2eede7bf294db0cf66b27defa1498`.
Target: `462ebee97c81772103054f7d090053ac7054bafa`.
The worktree was clean at review start. The complete active packet, including
the latest task reviews and maintainer decision, is committed in the target.

Authority: `changes/active/storage-upload-consolidation/spec.md`, approved
revision 2; original TASK-001/002/003 in its tasks directory; repository and
narrow architecture authority. Current explicit user direction excludes
benchmark remediation and asks full integrated change review. Preserve that
direction; do not recreate benchmark extension/run/comparison work as a finding.
Timing remains unmeasured and must not be claimed measured.

Final task verdicts present at target:
- TASK-001: `review/TASK-001-R2/verdict.md`, accepted at `5e7dec697`.
- TASK-002: `review/TASK-002-R3/verdict.md`, accepted at `793912475`.
- TASK-003: `review/task-003-r1-57219dee3/verdict.md`, accepted at `57219dee3`.

Review the actual complete `git diff <base>...<target>`, all obligations and
cross-task seams. Task PASS is context, not a substitute for integrated audit.
The range includes a merge from `change/benchmark-rev19` at `15f939fc9`; inspect
ancestry/delivery correspondence and its integration effects rather than
silently omitting those source changes or calling them task drift without
checking their separate packet and delivery context.

## Starting navigation

- Shared upload owner: `wyrd-storage/src/service.rs`, upload sessions and
  sweeper SQL/server projections, client transfer/RunEvidence::pin, shared
  HTTP/MCP completion routes.
- Sole object inventory: server `components/storage/inventory.rs`, Card
  hydration/list/download/reconciliation/deletion, Bifrost storage objects
  built-in and peer capture allowlist, removed Postgres inventory.
- ACK boundary: process `scribe_outbox.rs`, existing local/peer submission,
  upload completion and all sibling producers.
- Evidence: TenantEvidence admission/read/workspace, InputResolver canonical
  inputs/outputs/results, direct/queued/group/scheduled callers.
- Public projections: Rust/Python/TS verifier building blocks, MCP verification,
  CLI card lifecycle, docs reference and generated contracts.
- Proof: task evidence and review reports; fresh integrated gate results will
  be recorded in verification.md. Root owns all execution, sequentially.

No `.codegraph/` exists; use ordinary repository navigation. Read all applicable
nested AGENTS, narrow architecture owners and testing rules.

## Independence and execution

Root participated in the prior one-line source commit, so one fresh independent
change reviewer owns the complete audit, requirement matrix, source findings
and recommendation. Root orchestrates required full journey/aggregate checks
and writes reports from its result. The reviewer must not run Cargo, tests or
benchmarks concurrently with root or modify source.

Root will run the existing broad `mise run -c gate` aggregate, which owns
non-credentialed checks and full surface journeys once, avoiding duplicate
component-lane execution. Logs live outside this reports-only directory.
Credentialed storage proof and any checks outside gate will be assessed against
task evidence and actual availability without exposing secrets. No synthetic
host load or benchmark is authorized.

Write the independent audit to `change-review.md` here with a complete
REQ/INV/AC/constraints/non-goals matrix, task ancestry/patch closure, cross-task
seams, material findings only, verification limits and completion payload if
passing. Do not choose an intended outcome. Inspect final verification evidence
before final recommendation. No source edit, commit, merge, push or deploy
during review. PASS automatically hands off to wyrd-complete.
