# Round 3 scope and independent review selection

Subject: repository HEAD `07209c7e9af1664a30bf548e64be08937c4f88df`; prior candidate `74fc2190e255a3e3cf7515091ff45ab86a288f6b`; original base `16ea9694c34a5ec1867306bd5e34df62f0888611`.
Authority: approved packet spec revision 7, original task 007, R2 remediation task (including appended Obligations/Evidence), root AGENTS and applicable architecture/agent guides. User fixes FIND-007-2 as rejected and accepts INPUT_INVALID for Artifact-read denial. Neither is reopened.

Review range: prior candidate to candidate. Changed runtime owner: `components/verification/service/evidence.rs`, pure receipt status/time extraction and inline regression. Changed connected proof: `tests/integration/pg_deferred_evidence.rs`, Artifact publication/refusal/consumption and shared ci_gate fixture limit. Prior R2 reports added to Git are context only. No ancestry rewrite observed.

| Contract and producer/consumer | Review selection / carry-forward |
|---|---|
| Artifact admission before acceptance; publisher RBAC; waiting consumer extraction | Fresh remediation reviewer traces service subscribers -> TenantEvidence admission -> custody -> existing runner; checks changed connected assertions. Prior R1 security/data and R2 validation cover unchanged semantics. No new authority/persistence protocol, so no separate domain specialist initially required. |
| Canonical losing-offer receipt; HTTP/client consumers | Fresh remediation reviewer checks receipt extraction, production caller and race reconciliation proof. Prior data/concurrency and R2 validation cover unchanged fences and canonical custody. No new synchronization/lifecycle semantics, so no separate invariant/system specialist initially required. |
| Stable refusal codes, changed rustdoc, fixture impact | Fresh remediation reviewer checks both changed Rust owners, affected sibling tests and repository gates. No broad cross-tier or substantial ownership change, so no repo/maintainer specialist initially required. |
| Unchanged deferred classification, matching, queue park/claim/budget/deadline/recovery, SDK/CLI/MCP, SQL named decoding | Carry forward R2 verdict/validation matrices and their cited R1 behavior, invariant, standards, maintainer, system, security and data reports. Expand only for a concrete changed dependency or evidence gap. |

Select separate fresh remediation discovery and independent validation reviewers, both Ponytail-scoped, with exact report paths `task-review-behavior.md` and `findings-validation.md`. Validation follows discovery. Required specialists are omitted for the bounded reasons above, subject to independent validation. A new blocker requires separate focused source validation. The orchestrator also audits implementation ledger quality in `ledger-check.md` without self-validating implementation findings.

Docker is unavailable by user statement: connected execution relies on source assertions plus clearly labelled implementation-recorded evidence. Orchestrator may run the user-supplied exact unit selector and read-only checks. Reports only; no source edits, commits, pushes or Git configuration. CodeGraph consulted before source navigation; shared memory searched for prior context.
