# TASK-002 first remediation review scope

Repository: `/Users/stevenforrester/Documents/GitHub/wyrd`.
Original base: `15f939fc917b22efa5e5e4d12fd1f97be807a0a7`.
Prior candidate: `5ef2981e66068e8ee5f96d3be15000eea9cbd94f`.
Candidate: `a80afd68e47d992e35f20931984d96524b2e7dbf`.
Incremental review: `git diff 5ef2981e6 a80afd68e`.
Cumulative context: `git diff 15f939fc9...a80afd68e`.

The five requested commits form a direct descendant chain; no rebase correspondence is needed. Initial tracked tree is clean; pre-existing untracked TASK-001-R2 and TASK-002-R1 reports remain untouched. Candidate/source must remain immutable. No `.codegraph/` exists.

Authority and acceptance context: approved spec revision 2, original TASK-002, R1 verdict and all R1 reports, R1 findings-validation, and `../TASK-002-R1/TASK-002-R1-file-session-lifecycle.md` with its appended implementation evidence. Root and SQL AGENTS, spec-driven development, maintainer/Rust/errors/agent-surface guides, TESTING and applicable Wyrd/Bifrost/security authority apply. No spec, original task, AGENTS or Bifrost/security authority change occurs in the incremental range. Wyrd design changes only to explain File session locations/reclamation.

## Scope and carry-forward evidence

Review closure of FIND-TASK-002-1/2/3; all 17 incremental files; and affected path construction/validation, session identity/init/replay/replacement, local/provider operations, inventory selection/hosted byte reads, sweeper projection/abort/status/tenant behavior, tests, docs and mandatory rustdoc. No comprehensive rediscovery of unchanged task areas.

Carry forward R1 source-backed acceptance for shared planner/dispatcher/completion/ACK isolation, limits/encryption/byte checks, operation/exact invocation authorization, tenant/public-write/MCP/audit boundaries, thin SDK/CLI/HTTP projections, removed route/contracts, sole inventory/fresh-install and non-goals unless affected changes invalidate specific evidence. File path and cleanup checks are reopened because fixes change their producers/consumers. TASK-003 REQ-014 and broader metadata/readback/readiness work remain excluded as previously validated.

## Navigation map

| Area | Owners, consumers and focused proof |
|---|---|
| Independent session final path | `tenant_path::evidence/validate_evidence`; `service::file_upload_init/init_upload/insert_initiating_upload/find_and_mark_prior_pending`; unchanged active-path index, local capability, cloud signers/commit/abort and inventory fold; `pg_verification_routes::file_sessions_are_independent` and strict path unit test |
| Recorded locations and sibling Card behavior | `ObjectInventory::file_object/file_fold`, `TenantEvidence::read/stored`; adapted hosted tamper, synthetic tombstones/replay and shared-client mock; existing Card replacement assertions; affected S3/Azure/local/GCS single-PUT e2e cases |
| Expired session recovery | SQL `ExpiredUpload/expired_uploads_batch` and all consumers; `Sweeper::abort_one` and terminal-state mutation; `pg_sweeper` new expiry/orphan/batch/owner-negative proof and completed-race sibling; SQL migration query fixtures |
| Documentation closure | Exact full FIND-3 inventory in R1 validation plus all materially changed remediation items; changed Wyrd design explanation; preserve final cancellation/partial-progress/retry semantics |

Map is a starting point; reviewer must expand against source. Implementation evidence records focused server/storage/SQL/path/client tests, backend transfers and format/lint/boundary/docs checks. Treat evidence claims separately from conclusions. Disclosed limits: no provider-emulator sweeper abort test; no real-GCS multipart. Assess existing unchanged backend abort seam and changed dispatch source before deciding whether that is a material missing proof; do not invent a new cloud cleanup contract.

## Selected topology

One fresh `ponytail-rev` independently checks all prior-finding closure, incremental changes/affected callers and mandatory rules, reconciles carry-forward acceptance and writes `findings-validation.md` in this directory. This is the default bounded remediation topology. No fresh discovery/domain/standards/maintainer/system team is selected initially: fixes use the exact previously validated existing owners and private boundaries, not new public/architecture/lifecycle machinery. Add a focused fresh reviewer only if validation uncovers a new blocker, material uncertainty or changed boundary requiring separate proof. Orchestrator records runtime verification and writes verdict from the independently validated ledger; it does not validate its own findings.

This is the first remediation review, so the repeated-unsuccessful-round convergence threshold does not apply.

## Focused validation follow-up

During FIND-3 closure validation, the validator identified inaccurate new partial-state/reclamation documentation and an omitted changed test's required docs. A fresh followup-rev was selected for these exact existing-finding occurrences and wrote `followup-review.md`. It traced planning-before-transfer, failure/completion states, initializer failures versus the sweeper SELECT, and the mandatory changed-item test gate. This expands only the existing documentation closure boundary; no new runtime obligation or comprehensive discovery was added. The independent validator must reconcile the report before finalizing the ledger.
