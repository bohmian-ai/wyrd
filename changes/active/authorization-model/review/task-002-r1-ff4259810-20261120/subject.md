# TASK-002 R1 immutable review subject

Repository: `/home/thorrester/Documents/GitHub/wyrd`.
Branch: `task/authorization-model/TASK-002`.
Prior candidate / remediation base: `29a9d276f5e668df56f5f16e898a73f7aefc46a0`.
Candidate: `ff42598100b0edabad5cad2272a3c7735f2eef14`.
Remediation commits: `568b89b9a`, `c8fdad70d`, `ff4259810`.
Cumulative original base: `300542877cd2a58cbe5f061d5075616e4cc4397b`.
Review reads `git diff 29a9d276f ff4259810`; cumulative range remains available.
No rebase or ancestry change is claimed.

Approved contract: SPEC-authorization-model revision 2, supplied working-tree
`changes/active/authorization-model/spec.md`, SHA256
`c27e81cab3368de9a27d93f37fddf620155f107768c80a263a7f937d043f2d9a`.
Original task: `tasks/TASK-002-access-ui.md`, SHA256
`00479aabdaf43a4212dd0e2f66c8d2ea8fe953d9f01f4521c02cac0341c1a127`.
Both hashes match the prior review subject. The intentional dirty packet is
maintainer context, not candidate implementation.

Prior verdict, validated findings and untracked remediation with implementer
acceptance/evidence are under `../task-002-29a9d276f-20261120/`.
Review scope: closure of FIND-TASK-002-1 and FIND-TASK-002-2 against REQ-006,
REQ-008 and affected paths; carry forward unchanged source-backed TASK-002 and
TASK-001 evidence where still valid. Maintainer expressly authorized suspended
Service readability consistent with User; do not relabel that scope as a finding.

## Navigation and independent assignments

- ponytail-rev: remediation closure, Alice's UI-created identity/CLI credential
  across browser/CLI/TS/Python/MCP, Access BFF/control, requirement coverage,
  assertions, TDD and remaining repository rules. `task-review-remediation.md`.
- oracle-rev: `restrict_verification_rows`, field-typed mask/Variant planning,
  optimizer/storage/peer consumers, null masking and external-wait inventory.
  `task-review-oracle.md`.
- domain-rev: `service_account_by_id`, `require_principal`, issuance/revoke/access
  consumers, suspended credential behavior, tenant/SYSTEM/auth boundaries.
  `task-review-domain.md`.
- A fresh independent validator follows these reports, checks all proposals and
  coverage and writes `findings-validation.md`. New validator-originated blockers
  require a separate focused source validation.

Authority: root and SQL nested AGENTS; invoked task-review skill; UI skill;
changes/spec-driven-development.md; agent_docs/maintainer-style.md; TESTING.md;
architecture/README.md and relevant security, public-surface and Bifrost sections.
CodeGraph precedes source exploration. No source edits, implementation, commits,
push, deploy or automatic next review. Reports only in this new directory.

Use supplied exact execution records and inspect actual assertions. No broad
suite reruns or concurrent Cargo work. Prior broad Bifrost result remains 64/66,
not green. The prior subject and remediation expressly record maintainer
exclusion of benchmark qualification, reflected in the supplied approved spec;
preserve that specific decision and do not claim measured performance.
Shredded-leaf/peer proof and empty results/evidence/events are claims/limits to
assess, not automatic defects or automatically accepted coverage.
