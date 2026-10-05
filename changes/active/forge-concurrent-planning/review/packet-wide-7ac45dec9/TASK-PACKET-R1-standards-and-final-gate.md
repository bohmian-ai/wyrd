---
id: TASK-PACKET-R1
kind: remediation
status: ready
spec: SPEC-forge-concurrent-planning
spec_revision: 11
parent_task: packet-wide
remediates: [FIND-PACKET-1, FIND-PACKET-2, FIND-PACKET-3, FIND-PACKET-4]
---

# Close packet-wide repository standards and final verification

## Contract and candidates

- Approved spec: `changes/active/forge-concurrent-planning/spec.md`
- Task index: `changes/active/forge-concurrent-planning/tasks/README.md`
- Review verdict: `review/packet-wide-7ac45dec9/verdict.md`
- Reviewed candidate/base: `7ac45dec99535c881b7a936c66623044f15d8823` / `c1508b375`

This remediation runs after the task-specific source corrections so its audit
and final gate cover the complete corrected candidate.

## Diagnosis

The range contains new/materially changed Rust items without required rustdoc,
`# Errors`, `# Panics`, or async cancellation/partial-progress documentation.
Changed modules also contain function-local imports and fully qualified types
where module-scope imports and bare names are mandatory. Two packet Markdown
files contain extra EOF blank lines. Because the range crosses SQL, Vala,
server, contracts, three SDKs, generated artifacts, docs, test infrastructure,
and `mise.toml`, AGENTS.md requires `mise run gate`; no result is recorded.

## Intended correction outcome

Every changed Rust symbol complies with documentation and import/signature
rules, the cumulative diff is clean, and one final aggregate gate proves the
corrected broad candidate.

## Decision-complete recommendation

Audit the full changed-symbol set, document existing behavior in place, move
misattached prose to its owner, and include failure, panic, cancellation, and
partial-progress contracts where applicable. Move local imports to module or
test-module import blocks and use bare names in changed signatures, fields, and
impl heads, retaining only the explicit narrow trait-as-underscore exception.
Remove only the two extra EOF blank lines. After every task remediation lands,
run one `mise run gate`; do not repeat its component lanes as final commands,
and separately retain only specialized proof genuinely outside the aggregate.

## Preserved behavior and non-goals

- Do not change runtime behavior while repairing documentation/import shape.
- Do not add lint suppressions, wrapper types, aliases, documentation helpers,
  or weaken checks.
- Do not use this task to absorb task-specific functional findings.

## Acceptance criteria

| Finding | Acceptance criterion |
|---|---|
| `FIND-PACKET-1` | Complete changed-symbol audit finds no missing substantive rustdoc or required errors/panics/cancellation/partial-progress section. |
| `FIND-PACKET-2` | Complete changed-range scan finds no prohibited local import or qualified signature/field/impl type. |
| `FIND-PACKET-3` | Cumulative `git diff --check` exits zero. |
| `FIND-PACKET-4` | One final `mise run gate` passes on the corrected immutable candidate and its result is recorded. |

## Focused proof and broader verification

Run the repository's changed-symbol documentation and import audits (or the
existing checks that cover them), format, lints, and cumulative diff check.
Then run the single final `mise run gate`. Record any specialized lane outside
that aggregate explicitly; do not duplicate aggregate components.
