# Focused follow-up: FIND-TASK-001-6 closure discrepancy

**RESOLVED:** the conflicting closure claim is resolved from pinned source. **FIND-TASK-001-6 remains partially open.** This is one residual documentation obligation, not a new finding or a runtime defect.

## Subject and scope

- Candidate: `eb1681461897ee1eb9fe1fa4e00cb0b435658559`; HEAD matched this candidate during inspection.
- Original base: `08b38f74255d344e1c5ec9a6d278935a26277622`.
- Scope: only whether the two cited formatting methods establish closure of the existing documentation finding. No broader audit or tests were performed.
- Inputs inspected: this review's `subject.md`, `task-review-behavior.md` closure statement at line 77, `standards-review.md` RS-R1-001, `maintainer-review.md` MNT-R1-001, the prior `findings-validation.md` FIND-TASK-001-6 and `TASK-001-R1-contract-gaps.md`, applicable AGENTS §16 and agent-rules documentation rule, and the task-review skill's finding threshold. CodeGraph was used before source discovery; pinned `git show` confirmed the complete two method bodies and their attached docs. The cumulative base-to-candidate diff confirms both methods are new in this task.

## Source evidence

| Pinned source | Complete relevant documentation and behavior | Result |
|---|---|---|
| `crates/wyrd-spec/src/card/blocks/reference.rs:232-268`, method at line 235 | Attached rustdoc consists only of “Writes the canonical reference text, which [`Reference::parse`] reads back to an equal value.” The body writes the selected root and each path segment using `write!`/`write_str`, propagates their errors with `?`, and ends in `Ok(())`. It returns `fmt::Result`; there is no `# Errors` section. | Required error documentation remains missing. |
| `crates/skald/skald-spec/src/media.rs:302-307`, method at line 304 | Attached rustdoc consists only of “Writes the canonical wire token.” The complete body is `f.write_str(self.as_str())`, directly returning `std::fmt::Result`; there is no `# Errors` section. | Required error documentation remains missing. |

These are actually fallible output operations, rather than an inference from a return-type spelling alone. A formatting destination may reject a write, and each method returns that underlying formatting failure. Neither method introduces its own parsing or validation error. The trait implementation status does not exempt either method from the repository rule.

## Authority and discrepancy resolution

`AGENTS.md:723-737` requires rustdoc on new/materially modified Rust items regardless of visibility, states “Every fallible Rust function or method MUST include a `# Errors` section naming the error conditions,” and makes missing required rustdoc a hard blocker. `architecture/agent-rules.md` reiterates that every fallible function requires Errors documentation and that missing required rustdoc is `BLOCK_BEFORE_MERGE`.

The task-review skill normally makes placement, naming, structure, and wording findings non-blocking, but explicitly states: “Missing rustdoc on a changed item stays blocking per `AGENTS.md`.” Its remediation guidance preserves the rustdoc gate while requiring bounded documentation corrections to be batched with the same remediation rather than creating an unrelated style-only round.

The prior FIND-TASK-001-6 already covers trait methods in both of these files and accurate error sections throughout the affected cumulative write set. Adding summary rustdoc repairs their original complete absence of documentation but does not satisfy the remaining fallibility requirement. The behavior report's statement that this finding is source-closed is therefore too broad; its explicit deferral to the complete standards gate does not supply the missing sections. The standards and maintainer claims are supported by the actual source, independently of their agreement.

## Retained proposed finding

- **Stable ID:** FIND-TASK-001-6; discovery aliases RS-R1-001 and MNT-R1-001.
- **Severity / classification:** Important / VIOLATION, residual prior finding.
- **Locations:** `crates/wyrd-spec/src/card/blocks/reference.rs:235`; `crates/skald/skald-spec/src/media.rs:304`.
- **Consequence:** the explicit changed-item documentation completion gate remains unsatisfied. No runtime, security, or formatting-output defect is asserted.
- **Smallest correction:** add a `# Errors` section to each existing formatting method stating that it returns the underlying formatter/write failure. Preserve both bodies and canonical output; no wrapper, checker, dependency, new test, or unrelated documentation expansion is required.
- **Focused closure proof:** inspect both method docs for the accurate formatter-error contract and confirm normal formatting. Carry this correction under the existing finding and combined remediation boundary.

## Verification limits

No tests or builds were run; they do not establish this documentation requirement. Only this assigned report was written. Source, tasks, specifications, and prior reports were not modified. Other prior-finding closure claims and other documentation surfaces were outside this focused follow-up.
