# TASK-003A remediation review R1

Repository: /home/thorrester/Documents/GitHub/wyrd
Original base: 43c77a219fd10554e8c8663e980788b70f1a6296 (43821f9d6^)
Prior reviewed candidate: c233b75456fad4f45c772ec242a9d115b4d35c6b
New immutable candidate: 5575b41bf53734eb0ade88257e76b88ac1ec857d
Branch supplied: task/wyrd-ui-main-integration/TASK-005
Remediation range: git diff c233b75456fad4f45c772ec242a9d115b4d35c6b 5575b41bf53734eb0ade88257e76b88ac1ec857d
Cumulative range: git diff 43c77a219fd10554e8c8663e980788b70f1a6296 5575b41bf53734eb0ade88257e76b88ac1ec857d
Prior candidate is ancestor; no rebase/patch correspondence issue. Initial working tree clean.

## Authority and retained context

Approved spec: changes/active/verification-workflow-ui/spec.md revision 4, REQ-015 / AC-010.
Original task: changes/active/verification-workflow-ui/tasks/TASK-003A-execution-status-events.md.
Prior verdict, source-backed matrices and final ledger: ../TASK-003A-2026-11-16-01/{verdict,findings-validation}.md and their cited behavior, invariant, repo, system, data, security and focused follow-up reports.
Remediation contract: ../TASK-003A-2026-11-16-01/TASK-003A-R1-deterministic-checkpoint-proof.md, now with implementation Evidence. Review all as acceptance context; implementation assertions are claims, not verdict authority. Preserve historical TDD and unavailable historical diagnostician evidence separately from current behavioral acceptance.

Scope is FIND-TASK-003A-1/2 closure, complete prior-to-new diff and affected consumers/invariants/tests. No repository-wide rediscovery. Candidate commits the previous review artifacts; inspect them as carried-forward evidence, not new runtime behavior. Confirm authority/dependency stability before carrying evidence. Expand only for a material new source-backed defect or a fix that affects accepted behavior, recording why.

## Common navigation map and coverage selection

| Contract/invariant | Owners and paths | Current coverage / carry-forward |
|---|---|---|
| FIND-TASK-003A-1 changed-item panic contracts | Flusher::run -> CapabilityCrash completion flush; SQL test stored_cancellations -> status lookup. Only doc additions in these files. | Fresh ponytail-rev validates sections against full bodies; prior standards report and validated ledger retain all other Rust/SQL rule coverage. No fresh broad repo specialist: no changed signatures, pools, SQL, production logic or new item owner. |
| FIND-TASK-003A-2 checkpoint custody proof | pg_deferred_evidence two hosted tests -> WyrdTestServer occupy_hosted_memory/wait_for_hosted_waiters/settle_bifrost_outbox -> Sandbox retained memory/session envelope -> RunState single frozen revision -> Scribe status/checkpoint frames and native ingest stall -> running/resource/eventual outcome assertions | Fresh ponytail-rev traces changed fixtures and existing controls independently, verifies held-producer/drain happens-before and budget/assertion preservation. Prior system/followup/data and final validation cover unchanged runtime concurrency/acknowledgement boundaries. No new production recovery/shared-state behavior; no new system/invariant/data specialist selected. |
| Lifecycle, cancellation, group fallback, SQL identity/attribution and security | Original runner, claims, Completions, admissions/revocation/GitHub commit owners, Scribe row/frame, checkpoint reader | Carry forward exact source/assertion acceptance from prior reconciled matrix and behavior/invariant/data/security reports after checking changed dependencies/authority. These implementations and tests are untouched except two panic docs. No fresh domain reviewers: no permissions/tenancy/transaction/persistent-state changes. |
| Ownership/reuse, public/UI/schema/deployment/hot paths | Existing fixtures reuse existing controls; no new owner/dependency/transport/config/schema or production instruction | Prior maintainer omission remains applicable. No substantial abstraction change or new public/SDK/UI/migration/deployment surface. Fresh validator independently verifies omissions and no hot-path change. |
| Diagnosis/evidence continuity | Original task diagnosis corrected; R1 Evidence appended; previous review reports newly tracked | Fresh validator assesses credible focused proof, current diagnosis vs unrepairable history, and report continuity. Prior broad owner 126/126 predates final first-round test-only commits and is not claimed current. |

Selection: one fresh ponytail-rev, as remediation default and mandatory independent validator. It is distinct from implementation and all prior reviewers. No separate discovery pass unless material new scope/question warrants it. Any new blocker discovered by validator needs separate focused source validation before retention. No intended verdict supplied. Reports only in this new directory; source, old reports, task evidence and Git untouched.

## Verification plan

Inspect actual assertions and recorded four exact selector results plus five paired repeat passes, fmt/lints/diff results in R1 Evidence. Orchestrator may run narrow check-only format/diff and exact focused Postgres selectors sequentially; no broad owner/public-journey/aggregate rerun required for this test/docs remediation. Runtime availability in this sandbox must be stated accurately; historical passes are not review-run passes. Full hot-path benchmark gate applies only if remediation actually changes a hot path. Current diff shows test sequencing and doc additions, so no benchmark expected. Named selectors and commands are supplied by the remediation contract; do not invent new tests/harnesses.
