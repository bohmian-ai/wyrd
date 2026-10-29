# TASK-003 verdict: FIX_REQUIRED

The implementation's runtime acceptance evidence is credible. One mandatory changed-item rustdoc correction remains. The maintainer removed
benchmark remediation after this review; see [maintainer-decision.md](maintainer-decision.md). No performance regression was
demonstrated, and no product or architecture revision is required.

## Immutable subject and scope

- Repository: `/Users/stevenforrester/Documents/GitHub/wyrd`.
- Base: `3a99eb05fd5ea78a21fd732212bf2c5d3672f27f`.
- Candidate: `f1f6a9c833061ec6f0a781247b762ec3c0de4bea`.
- Approved authority: `changes/active/storage-upload-consolidation/spec.md`, revision 2.
- Original task: `changes/active/storage-upload-consolidation/tasks/TASK-003-canonical-evidence.md`.
- First review: complete base-to-candidate range, no prior TASK-003 acceptance or finding closure carried forward.

Source remained immutable. HEAD was checked again after verification and
independent validation; only this new review-report directory was untracked.
No source was implemented, committed, merged, pushed or deployed. No prior
remediation round exists, so a convergence assessment is not applicable.

## Reviewer selection and results

All reviewers were fresh, separate agents. The same starting navigation map
and immutable subject were supplied without another discovery reviewer's
conclusions. [scope.md](scope.md) records selection, ownership and input rules.

| Role | Coverage | Result and report |
|---|---|---|
| behavior-rev | Full task acceptance, owners/reuse, maintainability, invariants and proof | FAIL: BEH-001, [task-review-behavior.md](task-review-behavior.md) |
| repo-rev | All 13 changed paths; Rust/Python/TS/MCP rules, authority, SQL capabilities, rustdoc and gates | FAIL: REPO-003-1, [standards-review.md](standards-review.md) |
| system-rev | Admission deadlines, request/attempt failure, bounded retries, exhaustion, recovery and shared dependency effects | PASS, [system-review.md](system-review.md) |
| domain-rev-security | Executor identity producers/consumers, tenant/table scope, exact Card authorization and per-use auditing | PASS, [domain-review-security.md](domain-review-security.md) |
| ponytail-rev | Independent source/materiality/correction validation and acceptance reconciliation | FIX_REQUIRED recommendation, [findings-validation.md](findings-validation.md) |

No separate invariant reviewer was selected: execution-local state and affected
lifecycle were covered by behavior/system scopes. No substantial ownership
hierarchy or abstraction change required a separate maintainer reviewer;
behavior review performed the existing-owner and complexity audit. All selected
reports are complete. No follow-up reviewer was needed: the proposals concern
distinct obligations with no material conflict or untraced shared runtime cause.
[claim-comparison.md](claim-comparison.md) records this decision. The fresh
validator found no new blocker needing separate discovery validation.

## Reconciled acceptance matrix

Detailed source and test locations are preserved in the independent reports
and validator's matrix; this table records their reconciled outcomes.

| Obligation | Implementation evidence | Verification evidence | Result |
|---|---|---|---|
| REQ-010–011 / AC-004: one tenant/origin-scoped metadata resolution reused for admission, reads and workspace | Existing execution-owned TenantEvidence and hydrated Card inventory reuse | Fresh V1 passed; repeated bytes, selection, citations/media and workspace, two inventory reads | PASS |
| REQ-014–015 / INV-002 / AC-007: no new File application SQL; exact Card and authority/audit preserved | Server-resolved executor reused; scoped inventory owner; first/cached Artifact uses authorize | Source producer/consumer tracing, V1 negatives and existing hostile Artifact evidence | PASS |
| REQ-013 / AC-006: bounded missing/unreadable inventory readiness and recovery, no guessed path/fallback | Admission error mapping, existing engine deadline/service/runner/queue owners | Fresh V2 passed missing record, three-attempt exhaustion, completion recovery and direct fault/recovery | PASS |
| INV-004: existing length/digest/format and deletion behavior | Existing bounded byte reader and terminal read validation retained | Recorded hostile File/Artifact regressions and fresh V1 byte/workspace use | PASS |
| REQ-009 / REQ-012 / INV-001/005 / AC-005: canonical direct/submitted/group/scheduled assembly; no new byte IO | Unchanged InputResolver, canonical input/output/summary joins and selectors | Fresh V3 passed with bytes removed; inspected recorded group/schedule canonical-owner proof | PASS |
| REQ-016: public readback consistency | Existing Rust/Python/TS/MCP projections, no wire/binding change | Recorded focused V4–V7 journeys inspected independently | PASS |
| Narrow owner reuse, architecture and non-goals | Existing reader, registry/inventory and assembler extended or reused; no new persistence, global cache, queue design or ACK dependency | Full actual range and affected caller review | PASS |
| Applicable focused format/lint/boundary/projection checks | No generated contract changes | Recorded implementation checks; fresh range diff check and ts:napi:check passed | PASS within recorded scope |
| Cost observations and baseline comparison | No affected-path timing measurements | Maintainer explicitly removed benchmark remediation after review | EXCLUDED by current maintainer instruction |
| Mandatory changed-item rustdoc | New MCP local DOCUMENT constant lacks documentation | Source confirmed independently | FAIL: FIND-TASK-003-2 |

The V1/V2 selector moves reflect existing fixture/runtime owners and do not
weaken proof. Queued File exhaustion uses the existing errored outcome;
the trace-only final inconclusive rule remains intact. Missing inventory does
not prove deletion. The inherited missing-File readiness classification and
absence of a production File deletion path establish no new regression here.

## Validated finding ledger

The complete ledger, producer-to-consumer evidence, materiality, correction
burden and focused closure proof are in [findings-validation.md](findings-validation.md).
The discovery and independent-validation reports preserve their original conclusions.
The following ledger incorporates the subsequent maintainer instruction:

| Stable ID | Discovery ID / status | Classification and location | Required correction |
|---|---|---|---|
| FIND-TASK-003-1 | BEH-001 / WITHDRAWN by maintainer instruction | Original missing measurement evidence remains factual | No benchmark extension, run or comparison remediation. This finding is not deferred to change review. |
| FIND-TASK-003-2 | REPO-003-1 / CONFIRMED | VIOLATION; `crates/wyrd/wyrd-mcp/tests/bifrost/mcp/verification.rs:1316` | Add meaningful rustdoc to the local uploaded JSON document constant, preserving its bytes and consumers. |

Standards' minor observations impose no remediation. The missing TS declaration
check evidence was closed by an independent successful run. The remaining rustdoc finding requires no behavior or boundary change.

## Verification limits and handoff

[verification.md](verification.md) preserves exact commands and observations.
Independent review reran V1–V3 against repository Postgres: three selected
tests passed, plus both migration checks. `ts:napi:check` and the range diff
check passed with no generated or tracked source difference. Full language
suites and the integrated storage/backend journey were not repeated; those
remain change-review obligations. Recorded targeted Python ruff/typing covers
the test-only write set; no full py:lints/stub check result is claimed, and
no native/stub contract changed.

No benchmark was rerun during review. The retained candidate record confirms
host.busy and zero admitted work. A quiet-host rerun of the unchanged inline
workload would still omit the specifically required affected costs. Timing remains unmeasured; the maintainer explicitly accepts that limitation
for this task and removed benchmark remediation. No timing claim is made.

Verdict: **FIX_REQUIRED**. Route the single self-contained
[TASK-003-R1-document-mcp-fixture.md](TASK-003-R1-document-mcp-fixture.md)
directly to `$wyrd-implement`. Its review should check the documentation closure and its small diff, carrying forward the
unaffected source-backed acceptance above.
