---
id: TASK-002-R1
kind: remediation
status: ready
spec: SPEC-verifier-code-mode
spec_revision: 2
requirements: [REQ-001, REQ-014]
depends_on: [TASK-002]
parent_task: TASK-002
remediates: [FIND-TASK-002-1]
---

# Explain both model-visible code.run output channels

Route directly to `$wyrd-implement`. This is a documentation/comment correction,
not a runtime change.

## Authority and reviewed subject

- Approved specification: `changes/active/verifier-code-mode/spec.md`, revision 2 at candidate `ff78b51b61fea8f215a2139b17840044f65b17e4`. The working revision 3 adds a separate benchmark and does not supersede this task's revision-2 runtime authority.
- Original task: `changes/active/verifier-code-mode/tasks/TASK-002-public-code-mode-workflow.md` at base `e5af2ce5caef2d00303e1c980b12c8a9bca4af0e` (implementation evidence appended at candidate).
- Original cumulative diff: `git diff e5af2ce5caef2d00303e1c980b12c8a9bca4af0e...ff78b51b61fea8f215a2139b17840044f65b17e4`.
- Review: sibling `verdict.md`, `findings-validation.md`; retained finding `FIND-TASK-002-1` (discovery `BEH-002-1`).
- Read applicable root/docs rules and existing generation owner; preserve unrelated working changes and other task packets.

## Diagnosis

The new reference at
`docs/src/content/docs/reference/agent-tools-and-authorization.md:34` says
“The model sees only what the program prints.” The general comment at
`fixtures/cards/verifier_building_blocks/programs/nested-investigation.py:14`
repeats the rule, and `docs/public/llms-full.txt` publishes both copies.

That rule is false for supported execution. `Invocation::code` in
`crates/wyrd/wyrd-server/src/verification/hosted/tools.rs:331–374` returns the
bounded last-expression repr as `value`, along with `value_truncated` and
printed `output`. Its existing CodeRun descriptor at lines 608–615 states both
channels. `Dispatch::call` bounds the returned JSON and
`RecordedTool::invoke` delivers it to the model without stripping `value`.
Both ordinary and bridge-enabled programs and all public clients share this
output owner.

A program may assign an authorized nested Card lookup to `card` and end with
`card`, without printing. A small result fits the existing output cap and its
repr reaches the next model turn. A reader following the print-only guidance
can therefore expose intermediate content to model context while believing it
stays inside Python. This materially misstates the task's central visibility
contract; it is not an authorization bypass. The green journeys all end in
print and cannot prove the stronger guidance. REQ-014 explicitly permits the
bounded outer value/printed output, and REQ-001 preserves its existing shape.

## Correction outcome and recommendation

Correct the existing reference to explain that the model receives bounded
printed output and the repr of the last expression. Intermediate values stay
inside Python unless the program includes them in either outer channel.
Reconcile the shared investigation program's general explanatory comment with
that contract, keeping its operations and printed output unchanged. Use the
existing `docs:generate` owner to refresh the generated machine-readable
reference and embedded comment; do not hand-edit generated output.

Reuse the runtime descriptor's explanation. A few source prose/comment edits
and ordinary generation close the shared cause once. Deleting the required
guidance would leave TASK-002 incomplete; modifying runtime or client output
would be larger and violate preserved response semantics. No consumer guard,
new helper, output option, runtime test, or additional harness is needed.
The program-specific comment in `reduce-notes.py` accurately describes that
program and needs no blanket rewrite.

## Constraints and non-goals

Preserve response fields, limits, recipients, authorization/capture/audit,
citations, tool records, interpreter behavior and all SDK projections. Keep
all fixture operations and output stable. No endpoint, API, schema, dependency,
client engine, runtime visibility restriction, performance promise, benchmark
or unrelated wording cleanup belongs here. Do not edit prior review reports
or the original implementation task to erase the finding.

## Acceptance and focused proof

For `FIND-TASK-002-1`, the source reference and shared example's general comment
must describe both model-visible outer channels and explain when intermediate
values can cross them. Generated docs must match those sources. Compare the
corrected explanation directly with the existing CodeRun descriptor and full
`Invocation::code` output construction; verify the false general print-only
claim is absent from source and generated copies. Record that the Python
program changed only in explanation and no runtime/client file changed.

Run from the repository root through the existing tasks:

```sh
mise run docs:generate
mise run docs:check
mise exec -- git diff --check
```

Follow the existing generated-drift discipline and record actual outcomes.
A gate depending on caller-owned Git authorization remains an explicit
unresolved gate; do not bypass it, hand-edit output or commit merely to clear
it. No new runtime test or full SDK/MCP journey suite is required for this
explanation-only correction. Full suites remain at final integrated change
review. Review remediation against the prior candidate, this diagnosis and
unchanged acceptance evidence; do not restart the entire task audit.

## Remediation Evidence

Status: `IMPLEMENTED`. Commit `0beec53d8`.

The existing owner `HostedTools::code` (`crates/wyrd/wyrd-server/src/verification/hosted/tools.rs`)
returns `{"value": repr, "value_truncated": !whole, "output": output}`.
`repr` is bounded by `tool_result_max_bytes`. The CodeRun descriptor reads "returns printed output and the repr of the last expression, both cut at
the result limit". The gap was source prose and the shared fixture's comment
only; no runtime or client file changed.

| Acceptance criterion | Implementation evidence | Verification evidence | Result |
|---|---|---|---|
| FIND-TASK-002-1: the reference describes both model-visible outer channels and when intermediate values cross them | `reference/agent-tools-and-authorization.md` "Tools from Python": `output` (printed) and `value` (last-expression repr), each cut at `WYRD_VERIFICATION_TOOL_RESULT_MAX_KIB`, `value_truncated`; a printed tool result or one held by the final expression reaches the model. Savings paragraph now says "its `code.run` result" | Compared against `HostedTools::code` and the CodeRun descriptor | PASS |
| The shared program's general comment matches; its operations and output are unchanged | `nested-investigation.py` comment only: the model sees printed output and the last expression's repr, `None` here. Code lines unchanged; `reduce-notes.py` untouched | `git show 0beec53d8 -- fixtures/` shows comment lines only; the journeys loading it still pass | PASS |
| Generated docs match the sources; the false print-only claim is gone | `docs/public/llms-full.txt` regenerated by `mise run docs:generate` | `grep` for "only what the program prints" / "model sees only" returns 0 in the reference, both programs and `llms-full.txt`; `mise run docs:check` passes | PASS |

Commands (all passed):

```bash
mise run docs:generate
mise run docs:check
mise exec -- git diff --check HEAD~1
scripts/postgres/with-test-postgres.sh -- bash -lc "mise run db:migrate:all:inner && cargo nextest run --locked -p wyrd-ai --test integration -P journey --run-ignored=all -E 'test(=verifier_building_blocks::code_mode_reduces_evidence_and_preserves_citations)' && cargo nextest run --locked -p wyrd-mcp --test mcp -P journey --run-ignored=all -E 'test(=verification::pg_tests::code_mode_preserves_authority_and_records)'"
```

The two journeys were re-run only because the fixture's text is loaded as
`code.run` source. Non-goals stayed excluded: no response field, limit,
recipient, authorization, interpreter or SDK projection change, and no new
test, helper or option. Prior review reports and the original task are unedited.
