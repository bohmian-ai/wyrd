# UI migration packet

The [specification](spec.md) revision 10 records the caller-approved decision
for Rust-owned opaque browser sessions in the existing database, without new
UI deployment secrets, environment variables or session infrastructure. It
supersedes revision 9's confidential-Node-client/encrypted-credential-cookie
premise; the rest of the migration specification remains.

The immutable [TASK-001/002 review](review/tasks-001-002-843484f6c/verdict.md)
reviewed revision 9 at `843484f6c`: TASK-001 requires the approved architecture
correction plus two validated fixes; TASK-002 has no implementation findings
but awaits human inspection. The self-contained
[TASK-001-R1 remediation](review/tasks-001-002-843484f6c/TASK-001-R1-installation-browser-sessions.md)
covers both findings and the separate caller-directed session change.
[Plan](plan.md) records its obligation ownership. No original verdict has been
rewritten into PASS. Revision-9 [spec readiness](review/plan-readiness-revision-9/review.md)
remains historical evidence for its original subject, not a review of revision 10.

- [Evidence and migration inventory](references/evidence.md) distinguishes main,
  historical implementation, and uncommitted visual references.
- [Canvas references](references/canvas/) hold the approved revision-18 canvas
  (`.dc.html` boards plus PNG renders) and, under `archive/`, the 13 original
  HTML canvases and their index. They guide visual inspection, not protocol
  behavior.
- [Historical brand variants](references/historical-brand/) preserve 23 committed
  reference files that differ from main, with source hashes in
  [their manifest](references/historical-brand-manifest.json).
- [Preservation manifest](references/preserved-files.json) records SHA-256 hashes
  of every original packet file retained during this rebuild.
- `archive/invalid-draft/` holds the original spec and ten task files, moved
  without changing their contents. Their approval and task status claims are
  invalid under the caller's instruction. They are not execution authority.

No implementation task in the archive may be started. This revision writes
specification, governing architecture and the remediation assignment only;
it does not implement, stage, commit, deploy or claim runtime verification.

The approved spec and reference assets remain. Every component still requires
individual desktop/mobile visual acceptance in light and dark, alongside
functional/accessibility and applicable live proof before shipment.
