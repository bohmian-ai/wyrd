# R4 re-review disposition

R4 source validation closes FIND-TASK-001-4 through FIND-TASK-001-11 against
approved revision 4; prior findings 1–3 remain closed. See
[findings-validation.md](findings-validation.md), [verification.md](verification.md)
and [subject.md](subject.md) for immutable subject and focused evidence.

Final acceptance was interrupted by the maintainer's explicit rejection of
duplicate constructor-field edits and their three receiver types. This changed
the approved target rather than revealing another revision-4 implementation bug.
The review disposition at that boundary is SPEC_REVISION_REQUIRED.

The maintainer subsequently directed approval of the deletion and remediation
work. [Revision 5](../../spec.md) now records that approval; the exact reviewed
revision 4 is preserved in [spec-revision-4.md](../../spec-revision-4.md).
[TASK-001-R5](../../tasks/TASK-001-R5-constructor-fields-once.md) owns the pending
implementation. No additional approval is required and no new finding ID is
assigned. Historical FIND-9's duplicate-setter requirement is superseded;
all other R4 fixes remain required. This record does not approve current source
against revision 5 or claim the remediation is implemented.
