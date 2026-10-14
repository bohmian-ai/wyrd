---
id: responses-input-forms-R2-schema-rustdoc
status: ready
parent: responses-input-forms-R1-contract-and-journey
findings:
  - FIND-responses-input-forms-4
---

# Complete the schema remediation's rustdoc gate

Approved spec and original task: `changes/active/responses-input-forms/spec.md`, revision 1 (the spec is the task). Original base `807c4fe68ddcf43f325e970ed15f8a7c9918e0d4`; reviewed cumulative candidate `100adbc04a39ca22022fb013b3c0e93bfe903c3a`, tree `3bb993d4c32769cda67650eb02b991f981ebc3c2`, a snapshot of uncommitted remediation atop branch HEAD `6294bf6bbc0acb9abc37e4e4d617e2638f9dc3f7`. Parent remediation and execution evidence: `remediation-evidence.md`; independent diagnosis: `findings-validation.md`; verdict: `verdict.md`. Route directly to `$wyrd-implement`.

## Diagnosis and correction

FIND-responses-input-forms-4 identifies the new `const EASY` at `crates/skald/skald-spec/src/wire/openai_responses.rs:315` inside `input_items_schema`. Its value identifies the named easy-message schema registered in the existing Schemars generator and the reference to that same definition. It has no rustdoc. AGENTS §16 explicitly includes constants regardless of visibility and hard-blocks missing rustdoc; the task-review skill preserves that exception. The enclosing helper's documentation does not meet the separate per-item requirement.

This is only a documentation gate failure. The input schema, raw provider forwarding, real SDK journey and tenant boundary are correct, and all three prior findings remain closed. The independent validator reconciled the no-style-only-round instruction with the stronger explicit hard rustdoc requirement; this task is solely minimal completion of the existing schema remediation, not a broader style pass.

Add one substantive rustdoc line on this constant explaining its shared definition/reference identity. Reuse the existing schema owner unchanged. Do not delete the schema hook, duplicate its name literals, change registration or references, add helpers, regenerate outputs or alter tests. Those changes do not close a further diagnosed gap.

## Acceptance and proof

The constant has meaningful rustdoc describing its role; the tracked implementation difference for this correction contains only documentation; source schema registration/reference behavior and prior finding closures remain unchanged.

Inspect the corrected source and run:

```sh
mise run fmt:check
git diff --check
```

No new runtime test, codegen, lint/typecheck rebuild, journey suite or full aggregate is required to prove this documentation-only correction. Preserve the existing independent and supplied passing evidence. Do not expand scope to other documentation, remove the schema unit, change dependencies or reopen findings 1–3. Report the source location and focused proof results; a later acceptance review retains the complete cumulative subject.

## Implementation evidence

Status: `IMPLEMENTED` (uncommitted, atop the R1 working tree on `6294bf6bb`).

| Acceptance criterion | Implementation evidence | Verification evidence | Result |
|---|---|---|---|
| `const EASY` has meaningful rustdoc on its shared definition/reference role | `crates/skald/skald-spec/src/wire/openai_responses.rs:315-316`: a two-line `///` doc above `const EASY` inside `input_items_schema` | Source inspected | PASS |
| The correction is documentation-only, and registration, references and earlier closures are unchanged | Only the two `///` lines were added. No code, test, dependency or generated output was touched | `mise run fmt:check` exit 0; `git diff --check` exit 0 | PASS |

Non-goals stayed excluded: no codegen, test, lint or journey rerun beyond what was required, and no other documentation changed.
