# Review stopped by product rejection

On 2026-11-09 the user rejected TASK-004's per-tenant bring-your-own-GitHub-App
model and requested specification and task correction for installing Wyrd's
deployment-owned App. The acceptance audit stopped before independent findings
validation and verdict. These reports are proposals, not a validated findings
ledger or acceptance of any portion of the rejected implementation.

The user requested reversal of the complete TASK-004 range
`8f1bb789e..487ccb0b6`. `git revert --no-commit` reversed all 22 commits locally;
the index was verified identical to `8f1bb789e` for tracked implementation files.
No commit, push, database rollback or implementation replacement was performed.
Pre-existing skill edits and change packets were preserved.

Revision 23 of the active specification records the corrected product intent.
The replacement plan must use the restored repository owners and approved new
contract; old test results do not establish the new user journey. Historical
reports remain available as hypotheses, without FIND IDs or mandated fixes.
