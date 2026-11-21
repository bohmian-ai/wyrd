# TASK-002 immutable review subject

Repository: `/home/thorrester/Documents/GitHub/wyrd`.
Base: `300542877cd2a58cbe5f061d5075616e4cc4397b` (accepted TASK-001 R2).
Candidate: `29a9d276f5e668df56f5f16e898a73f7aefc46a0`.
Read `git diff 300542877...29a9d276f` directly; descendant range, no rebase.
Contract: `changes/active/authorization-model/spec.md`, approved revision 2.
Original task/evidence: `changes/active/authorization-model/tasks/TASK-002-access-ui.md`.
Spec SHA256: `c27e81cab3368de9a27d93f37fddf620155f107768c80a263a7f937d043f2d9a`.
Task SHA256: `00479aabdaf43a4212dd0e2f66c8d2ea8fe953d9f01f4521c02cac0341c1a127`.

Existing dirty packet edits and untracked prior reviews are supplied context; preserve them. HEAD and implementation source must remain immutable. Write reports only here. Task frontmatter remains proposed; maintainer reports IMPLEMENTED.

## Navigation and independent assignments

- behavior-rev: assigned requirements REQ-002/003/006/007/008/009; AC-001/002/003; Access actions/forms, safe copy, sessions/device approval, connected journey assertions, documentation, projections and repository rules. Covers all boundaries not assigned below. Report `task-review-behavior.md`.
- domain-rev: BFF authorization/credential and tenant isolation boundaries, server Card concealment, SQL local-user eligibility, provider/local authentication preservation. Report `task-review-domain.md`.
- system-rev: Oracle parse/deadline/CPU-lane lifecycle, local/remote forwarding, parser pin, discovery query equivalence, build-once integration host lifecycle, changed Bifrost assertions. Inventory added/moved external waits. Report `task-review-system.md`.
- Fresh ponytail-rev follows discovery, validates proposals and coverage in `findings-validation.md`. New blockers require separate focused source validation.

Authority: root/nested AGENTS, changes/spec-driven-development.md, agent_docs/maintainer-style.md, TESTING.md, architecture/README.md and owning security, identity, UI and Bifrost sections. Follow CodeGraph first with exact symbols/files. Read UI skill for UI review. Prior TASK-001 reports supply unchanged backend evidence, not authority.

Maintainer evidence: library 17/17, pages 19/19, provider-login pass, parser 3/3, Bifrost 64/66; two capacity assertions await another branch's 5290a7c14. Original RED missing; later tests written after fixes. Python/MCP UI-created-grant journey limitation is explicitly recorded with TASK-001 proof cited. No Oracle benchmark was run by maintainer decision: preserve this explicit exclusion; assess other evidence and waits without silently restoring benchmark qualification. Preserve TDD history separately. Another process can rebuild shared build/ mid-run: assess against supported topology, not hypothetical hardening.

Only behavior-rev may run heavy checks, coordinating first. Inspect recorded evidence and actual assertions; do not run broad integrated suites. No source edits, implementation, commits, push, deployment or automatic next review.
