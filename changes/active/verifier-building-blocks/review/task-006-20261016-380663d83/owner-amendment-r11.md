# Owner amendment — approved revision 11

The owner directed "drop the requirement" after discussing removal of the
permanent vocabulary scan. Approved spec revision 11 is already recorded in
commit d77214d8e; implementation removal is recorded in 10fa9269c. Those shared
workspace changes were preserved, not repeated or overwritten here.

FIND-TASK-006-1 is withdrawn as repository-scan remediation. Its requirement to
maintain or extend the removed scan, pathname matcher and probe matrix no longer
applies. No replacement source-tree checker or repository-policy exception is
required. The existing served OpenAPI assertion remains contract proof under
revision 11, alongside source review and focused behavioral tests; naming and
runtime obligations otherwise remain unchanged.

The original review/validation reports describe revision 10 at immutable
candidate 380663d83 and remain historical evidence. This is an owner scope
amendment, not a claim that the original source audit was wrong or that the
current implementation passed re-review. Findings 2–4 still require independent
closure. No TASK-005 or TASK-006 PASS is granted by this amendment.

The active TASK-006 and R1 packets now point to revision 11 and prominently
supersede their historical scan instructions. Their original implementation
records are preserved. No implementation code or unrelated work was changed by
this amendment, and no commit was made here.
