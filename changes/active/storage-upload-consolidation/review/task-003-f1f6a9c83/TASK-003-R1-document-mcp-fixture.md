---
id: TASK-003-R1
kind: remediation
status: ready
spec: SPEC-storage-upload-consolidation
spec_revision: 2
requirements: []
depends_on: [TASK-003]
parent_task: TASK-003
remediates: [FIND-TASK-003-2]
---

# Document the MCP readback fixture

Add meaningful rustdoc to the new local `DOCUMENT` constant in
`crates/wyrd/wyrd-mcp/tests/bifrost/mcp/verification.rs:1316`, inside
`stored_evidence_readback_uses_canonical_records`.

## Authority and diagnosis

- Approved spec: `changes/active/storage-upload-consolidation/spec.md`, revision 2.
- Original task: `changes/active/storage-upload-consolidation/tasks/TASK-003-canonical-evidence.md`.
- Review base: `3a99eb05fd5ea78a21fd732212bf2c5d3672f27f`.
- Reviewed candidate: `f1f6a9c833061ec6f0a781247b762ec3c0de4bea`.
- Finding: FIND-TASK-003-2, independently confirmed in [findings-validation.md](findings-validation.md).
- Subsequent scope decision: [maintainer-decision.md](maintainer-decision.md).

The constant's bytes feed the digest, length, upload and File declaration used
by the MCP journey. It has no rustdoc. Root AGENTS and `agent_docs/rust.md`
explicitly require rustdoc on every changed Rust item, including local test
items. The enclosing test's documentation does not document this constant.

Add one meaningful rustdoc line explaining that this is the uploaded JSON
document accepted by the fixture Verifier. Preserve its bytes and consumers.
No new helper, abstraction, fixture extraction or runtime change is needed.

## Acceptance and proof

FIND-TASK-003-2 closes when the constant has meaningful rustdoc and its value
and consumers remain unchanged. Inspect the diff and run `mise run fmt` and
`git diff --check`. This is documentation-only; no behavioral test rerun is
required. Carry forward all unaffected source-backed acceptance from the review.

Benchmark extension, benchmark runs and baseline comparisons are excluded by
the maintainer's instruction. FIND-TASK-003-1 creates no remediation work.
Route this bounded correction directly to `$wyrd-implement`.

## Implementation Evidence

| Acceptance criterion | Implementation evidence | Verification evidence | Result |
|---|---|---|---|
| FIND-TASK-003-2: the constant has meaningful rustdoc; value and consumers unchanged | `crates/wyrd/wyrd-mcp/tests/bifrost/mcp/verification.rs:1316` one rustdoc line on `DOCUMENT` inside `stored_evidence_readback_uses_canonical_records` | Diff inspected: one added doc line, bytes and consumers unchanged; `mise run fmt`; `git diff --check` | PASS |

Documentation only, so no behavioral rerun. All other TASK-003 acceptance
carries forward unchanged. No benchmark work (excluded by the maintainer). No
other source file changed.
