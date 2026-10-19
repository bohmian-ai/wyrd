# UI migration packet

This packet is being rebuilt from committed main. It contains planning only.
The [specification](spec.md) revision 9 is approved by the caller. It incorporates
main `8651b96fd`, desktop/mobile completion, current
grouped verification, and the caller's loader/action/SQL/read-parity decisions.
Its independent [spec readiness re-review](review/plan-readiness-revision-9/review.md)
returned `Approve`, architecture PASS, no blocking findings. This is a spec
gate.

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

No implementation task in the archive may be started. No implementation,
staging, commit, build, or runtime test is part of this planning pass.

The approved spec and reference assets remain. Every component still requires
individual desktop/mobile visual acceptance in light and dark, alongside
functional/accessibility and applicable live proof before shipment.
