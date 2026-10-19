# TASK-001–003 verdict

**FIX_REQUIRED**

## Immutable subject

- Repository: `/home/thorrester/Documents/GitHub/wyrd-worktrees/local-dev-setup`.
- Base: `1b962bb0052e58c65af8c45be3ee0d1a04d483b6`.
- Candidate: `98d8ef46e61ea6aa5dae90d59572467a256a3ef3`.
- Approved authority: `changes/active/agent-readiness/spec.md`, revision 2.
- Original tasks: `tasks/TASK-001-declare-get-pull.md`, `tasks/TASK-002-machine-readable-local-tooling.md`, `tasks/TASK-003-self-contained-agent-onboarding.md`, relative to the active change.
- Subject is the complete cumulative base-to-candidate range. TASK-004/live acceptance and final integrated change review are excluded. This is the first agent-readiness task-review round; no prior findings or remediation closure apply.

All required reports are complete. The candidate commit and tracked source remained unchanged throughout review. Only reports and the remediation handoff were written; the pre-existing unrelated local-dev-setup review directory was preserved.

## Independent results and reconciliation

| Role | Report | Discovery result |
|---|---|---|
| Behavior | [task-review-behavior.md](task-review-behavior.md) | FAIL |
| Invariants | [task-review-invariants.md](task-review-invariants.md) | FAIL |
| Repository standards | [standards-review.md](standards-review.md) | FAIL |
| Maintainer | [maintainer-review.md](maintainer-review.md) | FAIL |
| System resilience | [system-review.md](system-review.md) | FAIL |
| Security/RBAC/tenancy | [domain-review-security.md](domain-review-security.md) | PASS within its boundary |
| Persistent declaration/replay/bundle integrity | [domain-review-data.md](domain-review-data.md) | PASS within its boundary |
| Fresh structured Ponytail validation | [findings-validation.md](findings-validation.md) | Six bounded implementation findings and two evidence-only blockers retained |

The final ledger is independently validated; discovery agreement alone is not the basis for inclusion. [claims-comparison.md](claims-comparison.md) records grouping and why no focused follow-up discovery was needed. Source claims did not materially conflict. The data review's intact shared replacement owner and the invariant review's destructive caller are different portions of the same path, reconciled by validation. Static platform conformance and missing platform execution likewise measure different obligations.

## Reconciled acceptance matrix

| Original obligation | Implementation / verification evidence | Result |
|---|---|---|
| TASK-001 local-only check, transitive file validation and authoritative real declaration; REQ-002/AC-002 | Existing loader/check path precedes client creation; offline/invalid-source tests and real declaration evidence in task packet | PASS |
| TASK-001 declaration rename across wire, SQL/replay/audit, CLI and three SDKs; REQ-003/AC-003 | Coordinated in-place owners, enums/fields, migrations, native exports/stubs/types; reported SQL/server/SDK/OpenAPI/codegen proof | PASS for executable contracts |
| TASK-001 artifact-free get, merged selectors and existing authorization; REQ-004/AC-004 | Existing audited read owners and language projections; selector/denial/catalog/get journeys | PASS |
| TASK-001 complete pull, new bundle header/mode, metadata-only and retired-format refusal; REQ-004/AC-005 | Shared staged verified publication and BundleReader remain intact; reported bundle/runtime rejection proof | PASS at shared owner; published caller failure is FIND-TASK-003-2 |
| TASK-001 source-derived graph-download recovery closes across consumers; REQ-004/009 | Error metadata still directs to envelope-only get | FAIL — FIND-TASK-001-1 |
| TASK-002 one JSON result/error without stream logs or progress; REQ-006/AC-007 | Correct local boundary formatter is bypassed by process tracing and terminal progress; source plus small tracing reproductions | FAIL — FIND-TASK-002-1, FIND-TASK-002-2 |
| TASK-002 unattended input/host behavior, readiness and foreground cleanup | Existing DevServer/installer/host/credential owners retained; reported installed-process readiness/recovery/proxy tests | PASS for lifecycle/authority; stream failures above still affect machine use |
| New/materially changed Rust documentation | Independent provenance check limits correction to specified changed items | FAIL — FIND-TASK-002-3 |
| TASK-002 platform authority and unchanged portability restrictions; REQ-010 | Exactly three supported targets; no new prohibited mechanism | PASS for source conformance |
| TASK-002 supported-host runtime proof; AC-010 | Recorded execution is Linux x86_64 only | BLOCKED evidence — FIND-TASK-002-4 |
| TASK-003 MCP start/instructions, read roles and no new write capability; REQ-007 | Existing guide/type/dispatch/proxy owners; reported actual discovery/proxy/catalog tests | PASS |
| TASK-003 consistent linked Observe/Verify continuation and visible multi-language initialization; REQ-001/008/009 | Linked guide uses rejected get/output-dir command, mismatched resources, incomplete visible Rust/TS telemetry startup | FAIL — FIND-TASK-003-1 |
| TASK-003 executable published deploy recipe preserves prior data; INV-005/AC-005 | Python/TS callers delete the bundle before the safe pull owner | FAIL — FIND-TASK-003-2 |
| TASK-003 actual local-provider quickstart, captured outputs, source-expanded full reference | Reported real-server quickstart and retained three-SDK tests; independently rerun docs tests, 42 passed | PASS within recorded local-provider/source-build limits |
| TASK-003 truthful publication/provider qualifications; REQ-009 | Explicit older-artifact caveats and partial status remain | PASS for honesty |
| TASK-003 installed-user acquisition from compatible published artifacts; REQ-001 and material stop condition | Candidate records no compatible published 0.2.0 artifact set; locally built proof is distinct | BLOCKED evidence — FIND-TASK-003-3 |
| INV-001–007 ownership/security/durability/non-goals | Existing Rust/server/client/SQL/audit/load/publication owners preserved; no new process manager, MCP declaration/check/upload, compatibility reader, sibling-repo work or automatic live lane | PASS except validated stream/recipe/data-preservation gaps above |
| AC-011 contribution versus final integrated aggregate | Prior scoped proof assessed against source; final gate and TASK-004 remain explicitly deferred | Task contribution reviewed; no final integrated completion claimed |

## Validated ledger

Full exact locations, source IDs, traces, smallest correction boundaries and focused proof are in [findings-validation.md](findings-validation.md).

| Stable ID | Status | Classification | Required outcome |
|---|---|---|---|
| FIND-TASK-001-1 | CONFIRMED | INCORRECT/public contract | Derive-owned invalid/incomplete bundle errors prescribe complete pull into a destination |
| FIND-TASK-002-1 | CONFIRMED | INCORRECT | Parent tracing honors the JSON diagnostic destination even with RUST_LOG enabled |
| FIND-TASK-002-2 | REVISED | INCORRECT | Complete graph pull emits no implicit terminal progress on machine stdout |
| FIND-TASK-002-3 | REVISED | VIOLATION | Bounded changed Rust items satisfy the explicit rustdoc gate |
| FIND-TASK-002-4 | REVISED | MISSING EVIDENCE | Obtain actual relevant Linux arm64/macOS arm64 execution proof |
| FIND-TASK-003-1 | REVISED | MISSING/INCORRECT | Linked continuation matches declared resources and exposes existing language telemetry lifecycle |
| FIND-TASK-003-2 | CONFIRMED | VIOLATION | Published deploy callers rely on safe replacement without pre-deleting the bundle |
| FIND-TASK-003-3 | REVISED | MISSING EVIDENCE/external prerequisite | Prove installed acquisition once compatible artifacts are separately available |

## Verification limits and handoff

[verification.md](verification.md) records the orchestrator's independent range diff check and 42 passing docs tests. Several independent reviewers reproduced logging contamination using the existing local CLI binary; that binary was not rebuilt, so current source tracing remains primary evidence. Terminal transfer, broad Cargo/SDK suites, real provider/live agents, other hosts and current registry availability were not independently executed. Prior implementation results remain reported evidence, not fresh runs.

No new product or architectural decision is required for the six bounded corrections. Handoff: [TASK-001-003-R1-close-validated-readiness-gaps.md](TASK-001-003-R1-close-validated-readiness-gaps.md), directly to `$wyrd-implement`. This review does not execute it.

Closing code findings does not waive evidence blockers. If implementation corrections are complete while required hosts or compatible published artifacts remain unavailable, the next acceptance result must retain BLOCKED rather than claim PASS. A separate authorized release process owns publication; this review authorizes no release, version bump, tag or deployment. TASK-004's real-provider/live-agent acceptance remains separate.
