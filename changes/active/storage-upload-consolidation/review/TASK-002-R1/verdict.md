# TASK-002 review — FIX_REQUIRED

## Immutable subject and round scope

Repository: `/Users/stevenforrester/Documents/GitHub/wyrd`.
Base: `15f939fc917b22efa5e5e4d12fd1f97be807a0a7`.
Candidate: `5ef2981e66068e8ee5f96d3be15000eea9cbd94f`.

The user named implementation commits `fa0870703` through `5ef2981e6` inclusively. `fa0870703` contains the core implementation, so the complete first-review range is `git diff 15f939fc9...5ef2981e6`, from its parent. HEAD remained the candidate, with no tracked source edits. Initial unrelated untracked TASK-001-R2 reports were preserved. This review writes reports and one remediation task only; no implementation, commit, merge, push or deployment occurred. No CodeGraph directory exists.

Acceptance authority: [approved spec revision 2](../../spec.md), [original TASK-002](../../tasks/TASK-002-shared-file-uploads.md), root and applicable SQL/Python/TypeScript/docs AGENTS, spec-driven development, applicable agent/testing guides and owning architecture sections. [scope.md](scope.md) records common navigation and reviewer selection. This is the first TASK-002 review: no prior task verdict/findings/closure evidence is carried forward. TASK-001-R2 is dependency context; TASK-003 retains explicitly assigned later work. No convergence assessment applies.

## Independent coverage and reconciliation

| Fresh reviewer | Scope and report | Result |
|---|---|---|
| behavior-rev | Full task acceptance, caller-to-result paths, negative flows, invariants and credible recovery; [behavior report](task-review-behavior.md) | FAIL; BEH-001 and BEH-002 |
| repo-rev | All 33 files across Rust/spec/server/SQL/SDKs/CLI/MCP/docs; nested rules, actual SQL types, changed-item rustdoc; [standards report](standards-review.md) | FAIL; RSTD-001, including focused same-cause addendum |
| maintainer-rev | Changed ownership/abstractions, existing alternatives and callers; [maintainer report](maintainer-review.md) | FAIL proposal MAINT-001, subsequently rejected for this task's scope |
| domain-security-rev | Tenant/operation authority, persisted ownership, local bounds, MCP scopes and audit; [security report](domain-review-security.md) | PASS; no material proposals |
| domain-data-rev | File session persistence, inventory/readiness, completion/retry, expiry and recovery; [data report](domain-review-data.md) | FAIL; DATA-001 |
| followup-rev | Focused BEH-001 correction feasibility under active-path uniqueness and provider lifecycle; [follow-up](followup-review.md) | RESOLVED; revised bounded correction |
| ponytail-rev | Fresh independent source/materiality/correction validation of every proposal, coverage and follow-up; [final ledger](findings-validation.md) | FIX_REQUIRED; three retained findings |

All selected reports are complete. Separate invariant/system reviewers were unnecessary: data review covers the materially changed persistence/lifecycle boundary and behavior covers recovery/failure paths. [claim-comparison.md](claim-comparison.md) groups proposals by cause. A focused follow-up was required because naive File-session coexistence conflicts with the active-path unique index, while sharing one pending session couples abort/backend commit. Its existing-upload-ID final-location correction was independently validated across SQL indexes, strict path validation, signers, S3/GCS handles, Azure fixed block IDs, inventory fold and recorded-location reads. No new unvalidated blocker or unresolved authority/correction decision remains.

The added SYSTEM lookup identified by MAINT-001 is real, but original TASK-003 explicitly owns REQ-014 and its SQL-count proof. The independent validator rejects it as a TASK-002 blocker rather than pulling that named task forward. No remediation item is created from the rejected proposal; the approved whole-change obligation remains. Full source/scope reasoning is in `findings-validation.md`. The storage-e2e real-Card fixture change matches the registered Card requirement of inventory ingest and does not weaken production validation; its diagnosis was inspected against the fixture and Scribe boundary.

## Reconciled acceptance matrix

The complete behavior and authority matrices remain in the selected reports; this table reconciles them with independent validation.

| Applicable obligation | Evidence | Result |
|---|---|---|
| REQ-001/016, AC-001: shared File initialization/planner/transfer/completion and existing authoring | Existing shared owner/dispatcher reused; normal local and backend cases present. Two independent same-digest calls revoke one live session; FIND-TASK-002-1. | FAIL |
| Existing interruption recovery and Artifact reclamation preserved | New File rows reach existing sweeper, which rejects File namespace before abort/status mutation; bounded queue can starve Cards; FIND-TASK-002-2. | FAIL |
| REQ-003, INV-004: limits, local digest/size/encryption and read validation | Shared checks retained; V1/V3 fresh Postgres pass; hosted tamper/media/extract/citation tests inspected. | PASS |
| REQ-010: completed undeleted same-tenant File-only digest/size reuse and recorded location | Inventory origin/filter/fold and hosted read source inspected; V2 fresh Postgres passes. Card/envelope/raw object/foreign facts cannot substitute. | PASS |
| REQ-013 task portion: bounded retryable readiness, no path/SQL inventory fallback | Inventory query deadline and hosted retryable mapping inspected; V2 absent-fact proof passes. Broader queued/readback proof remains TASK-003. | PASS |
| REQ-015, INV-002: operation-level staging, exact invocation, persisted ownership, tenancy, audit and MCP scopes | Separate security coverage; V3 fresh Postgres passes; MCP exact-grant test/source inspected. | PASS |
| REQ-007, INV-003, AC-003 File portion: same completion guarantee and isolation | Reuses dependency's row-specific bounded ACK recorder; no new other-producer wait or completion mode. | PASS with recorded proof limits |
| REQ-016: thin Rust/Python/TypeScript projections, affected CLI and Artifact interfaces | Shared-client orchestration and public wrappers/callers inspected; V4–V8 implementation evidence and test assertions inspected; TS format/lints independently pass. | PASS |
| REQ-016/017: removed evidence-only route/contracts, consistent runtime/public docs and fresh-install coordination | Router/OpenAPI/MCP contract source, schema owner, nullable-session migration and docs inspected; migration setup passes; recorded codegen/docs evidence credible. | PASS |
| Mandatory changed-item rustdoc, including fields/helpers/tests/durable partial effects | Exact source-validated omissions recorded in FIND-TASK-002-3. | FAIL |
| Existing owners, dependency/platform/SQL capability rules and non-goals | Consolidation removes separate workflow; actual TenantConn/SystemReadAuthority signatures inspected; no new queue/inventory/hash/promotion/retention/compatibility machinery. | PASS |

## Validated finding ledger

Only independently retained findings are included. Full exact locations, producer-to-consumer/sibling traces, materiality, correction burden and closure proof are in [findings-validation.md](findings-validation.md).

| Stable finding | Source IDs / status | Failure and smallest safe correction |
|---|---|---|
| FIND-TASK-002-1 | BEH-001 + follow-up / REVISED / REGRESSION | Same-digest B planning aborts A's returned session. Use independent final locations distinguished by existing upload IDs inside the digest namespace; preserve active-path uniqueness, Card replacement, shared lifecycle and inventory-only resolution. |
| FIND-TASK-002-2 | BEH-002 + DATA-001 / CONFIRMED / REGRESSION, INCORRECT | Card-only sweeper validation leaves expired File rows and backend resources unreclaimed, potentially starving Card cleanup. Carry persisted nullable owner through existing discovery and apply strict owner-selected validation in the same worker/status pipeline. |
| FIND-TASK-002-3 | RSTD-001 + independently checked addendum / CONFIRMED / VIOLATION | Mandatory rustdoc omissions in changed production items, fields/helpers and tests. Correct the bounded inventory in place, documenting final errors/panics/partial progress/retry alongside the behavioral fixes. |

No specification revision is required: independent final locations use existing upload identity and recorded inventory locations, preserve the namespace, introduce no promotion/shared ownership and require no deployed migration. No prior-finding closure applies. One cohesive, self-contained remediation task packages all three findings: [TASK-002-R1-file-session-lifecycle.md](TASK-002-R1-file-session-lifecycle.md), routed directly to `$wyrd-implement`.

## Verification and limits

[verification.md](verification.md) records review-time exact V1/V2/V3 selectors on fresh repository Postgres, all passing (three tests, 7.385 seconds), plus successful Wyrd/Vala migration-idempotency setup and clean range whitespace check. Standards independently ran `ts:format:check` and `ts:lints`, both passing on 59 files. The linker compact-unwind warning did not prevent execution. The lifecycle failures are source-proven deterministic paths outside those serial tests; no source mutation or prospective remediation test was performed during review.

Implementation-recorded V4–V8, backend and broader unit/lint/codegen/docs/boundary results were inspected as available evidence, not claimed as fresh reruns. No broad aggregate, cloud credential lane, load test or historical RED reproduction was run. Real GCS multipart remains unproven; emulators cover GCS single PUT. TASK-003 canonical readback/metadata reuse/application-SQL elimination and broader readiness/performance proof remain outstanding at their designated boundary. The remediation requests focused overlap/abort, sweeper and affected provider/path/documentation checks only, not full user-journey suites.

**Verdict: FIX_REQUIRED.** TASK-002 has not passed its completion gate. No change-completion action is invoked.
