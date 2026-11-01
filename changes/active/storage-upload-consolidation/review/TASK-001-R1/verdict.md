# TASK-001 — FIX_REQUIRED

## Immutable subject and scope

Repository: `/Users/stevenforrester/Documents/GitHub/wyrd`.
Base: `3899b0a35`; candidate: `826fcf9c742a36bcae5e8f68307e76685d88244e`.
First review covers the complete `git diff 3899b0a35...826fcf9c7`, including the two verification-runtime corrections. Approved authority is [spec revision 2](../../spec.md); original acceptance context is [TASK-001](../../tasks/TASK-001-object-inventory.md). No prior verdict, remediation, findings or carried-forward acceptance exists. Implementation source stayed unchanged; only review reports/tasks were written. No `.codegraph/` exists.

Navigation, authorities and specialist selection are recorded in [scope.md](scope.md). Root and nested SQL AGENTS, the spec-driven workflow, maintainer/Rust/error guidance, applicable testing map and owning Card/Bifrost/security architecture sections govern this audit. No source, test or task instruction overrides them.

## Independent review and reconciliation

| Selected reviewer | Reason and coverage | Report result |
|---|---|---|
| behavior-rev | Full acceptance, callers/results and regressions | [FAIL](task-review-behavior.md): BEH-001/002 |
| repo-rev | Cross-tier Rust/SQL/docs/tests and mandatory gates | [FAIL](standards-review.md): REPO-001/002/003 |
| maintainer-rev | New inventory and completion/readback/delivery ownership | [PASS](maintainer-review.md): no material proposals |
| system-rev | ACK, deadlines, recovery and shared-producer failure propagation | [FAIL](system-review.md): SYS-001 |
| domain-data-rev | Persistent inventory, replay/deletion and shared lifecycle invariants | [FAIL](domain-review-data.md): DATA-1/2 |
| domain-security-rev | Privileged reads, public writes, tenant/path binding and peer boundary | [PASS](domain-review-security.md): no proposals |
| fresh ponytail-rev | Independent source/materiality/correction validation and acceptance reconciliation | [FIX_REQUIRED recommendation](findings-validation.md): five retained findings |

Every selected report is complete. Persistent-data discovery covers the shared lifecycle invariants, so no separate invariant reviewer was selected. [Claim comparison](claim-comparison.md) groups proposals by cause. No follow-up discovery was needed: proposed paths and common causes were traced, and the discrepancy about Failed Card reads was resolved directly by independent source validation. The SQL getter excludes only Deleted; listing/download can consume stale completed files on Failed Cards. No validator-originated new blocker was retained without separate validation. Report agreement alone did not establish a finding.

## Reconciled acceptance matrix

| Obligation | Implementation and verification evidence | Result |
|---|---|---|
| REQ-002/003, INV-004: shared completion and preserved validation | Storage entries converge on `finish_upload`; Card binding, HEAD/size/encryption checks remain. V1/V2 and recorded storage/Card regressions. | PASS |
| REQ-004: built-in schema, daily layout, Blooms and logical replay | `ObjectsTable`, existing catalog/staged projection, `ObjectInventory::fold`; V1/V2 and recorded catalog/attribution checks. | PASS for recorded facts |
| REQ-005/013, INV-001: required envelope inventory for hydration | `hydrate_card` ignores returned envelope; empty inventory can still serve URI-backed bytes. Healthy V1 does not test missing envelope. | FAIL: FIND-TASK-001-1 |
| REQ-005/013: file consumers, readiness and removed SQL readers | Shared `card_files`/`card_file` feed listing/download/fitter/hosted consumers; missing required files return typed readiness; no serving SQL callers remain. V1/V3 and recorded fitter/CLI/hosted checks. | PASS apart from invalid producer state in FIND-2 |
| REQ-006/007/008, INV-003: existing delivery, genuine isolated ACK and sibling semantics | Object-only oneshot on successful owning stream; existing local/peer outbox and submission handoff retained; V4 and healthy completion API proof. Diagnosed telemetry/fixture repairs preserve execution transitions and assertions. | PASS with verification limits below |
| REQ-015, INV-002: scoped authority and no public inventory append | Existing tenant SYSTEM table-scoped reader after Card authority, path binding, reserved Gate destination and authenticated peer allowlist; V3, security source trace and recorded narrow-permission hosted regression. | PASS |
| REQ-017: fresh-install removal and retained coordination | New immutable DROP migration, removed SQL query/export and retained sessions/manifests. Provisioned migration and V1 pass, but named test can silently skip. | Implementation PASS; proof gate FAIL: FIND-TASK-001-4 |
| REQ-018: all existing cleanup reflected, replay cannot resurrect deletion | Explicit delete/claimed cleanup record tombstones; initial successful failed-declaration cleanup omits them and clears retry state. V2 only covers explicit deletion. | FAIL: FIND-TASK-001-2 |
| Mandatory changed-item docs and handler diagnostics | Enumerated changed items lack required rustdoc; two changed storage handlers lack scrubbed instrumentation. | FAIL: FIND-TASK-001-3/5 |
| Existing owners and excluded work | Catalog, shared completion, scoped query and Scribe extended without new serving copy, worker, queue, hash/promotion, retention, permission or deployment design. Later File/evidence tasks remain excluded. | PASS |

The validator's full matrix and source-local proof are authoritative review detail; scoped reviewer PASS language does not waive the retained omissions.

## Validated finding ledger

| Stable ID | Status / classification | Discovery IDs | Material gap and smallest correction |
|---|---|---|---|
| FIND-TASK-001-1 | CONFIRMED / INCORRECT | BEH-001, DATA-1 | `cards/service.rs::hydrate_card`: require the already-resolved completed envelope before IO; missing fact returns existing visibility error, retaining pointer/binding validation. |
| FIND-TASK-001-2 | REVISED / MISSING | BEH-002, DATA-2, SYS-001 | Initial reconciliation/abort compensation omits deletion and clears cleanup. Persist Failed/pending cleanup before ACK waiting; reuse current deletion recorder and clear success only after physical cleanup and accepted deletion. Existing retry owner handles error/cancellation. |
| FIND-TASK-001-3 | CONFIRMED / VIOLATION | REPO-001 | Complete the validator-enumerated changed-item rustdoc, including Errors/Panics and durable retry boundaries, in the same bounded remediation. |
| FIND-TASK-001-4 | CONFIRMED / VIOLATION | REPO-002 | Fresh-install SQL proof must fail explicitly without required database setup; local correction, same runner/assertions. |
| FIND-TASK-001-5 | CONFIRMED / VIOLATION | REPO-003 | Add scrubbed handler instrumentation to changed storage `complete` and `download_init`. |

[findings-validation.md](findings-validation.md) preserves exact locations, violated obligations, independently traced producer/consumer and sibling evidence, meaningful consequences, materiality, correction burden, selected safe approaches and focused closure proof for every retained ID. No rejected proposal or optional improvement enters remediation. All five fit approved behavior; no specification revision is needed.

## Verification, limits and next scope

[verification.md](verification.md) records fresh review-time V1–V3 against repository-managed Postgres and V4: all four passed. Migration setup also passed; the immutable diff passes whitespace checking. These tests do not exercise the two source-proven lifecycle omissions. The linker emitted the recorded compact-unwind warning; tests completed successfully.

Broader format/lints/drift/tenant-isolation/dependency/codegen, catalog/schema and producer/fitter/CLI regressions remain task execution evidence, not fresh reruns in this review. Public controlled ACK delay/refusal and queued-handoff isolation are partly source-proven; V4 controls the outbox receiver and public V1 covers the healthy completion path. No cloud/load/full language suites ran here; integrated surface/performance proof stays at subsequent tasks/change review. Deployment migration/backfill is excluded by the approved fresh-install scope.

There are no prior findings to close and no repeated-remediation convergence assessment applies. The next review checks the five findings, incremental remediation diff and affected paths, carrying forward unaffected acceptance rather than restarting the whole task audit.

A separate read-only comprehension pass read only the approved spec and proposed remediation task. It reported no missing explanation, ambiguity or contradiction; this is document comprehension, not another source-validation verdict. Final Git checks confirmed the candidate is unchanged, no tracked source diff exists, and only this new review directory is untracked.

**Verdict: FIX_REQUIRED.** Handoff: [TASK-001-R1-inventory-lifecycle.md](TASK-001-R1-inventory-lifecycle.md), directly to `$wyrd-implement`. This review implements, commits, merges, pushes and deploys nothing.
