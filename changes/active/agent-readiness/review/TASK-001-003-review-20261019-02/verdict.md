# TASK-001–003 remediation re-review

**FIX_REQUIRED — three open code findings and two separate evidence blockers.**

## Immutable subject

- Root: `/home/thorrester/Documents/GitHub/wyrd-worktrees/local-dev-setup`.
- Original base: `1b962bb0052e58c65af8c45be3ee0d1a04d483b6`.
- Prior candidate: `98d8ef46e61ea6aa5dae90d59572467a256a3ef3`.
- Cumulative candidate: `d6b7c75b88db5eb144e6c282263f51692371bff1`.
- Approved spec: `changes/active/agent-readiness/spec.md`, revision 2.
- Original tasks: `changes/active/agent-readiness/tasks/TASK-001-declare-get-pull.md`, `TASK-002-machine-readable-local-tooling.md`, `TASK-003-self-contained-agent-onboarding.md`.
- Prior verdict/ledger/remediation: `changes/active/agent-readiness/review/TASK-001-003-review-20261018-01/{verdict.md,findings-validation.md,TASK-001-003-R1-close-validated-readiness-gaps.md}`; R1 includes implementation evidence.

The review covers the complete original-base-to-candidate range. The latest fix range located remediation owners without replacing cumulative acceptance. TASK-004/live acceptance and final integrated change verification remain excluded. Candidate and tracked source stayed unchanged. Only this fresh review packet was written; the unrelated pre-existing untracked local-dev-setup review was preserved.

## Independent review results

| Role | Report | Result |
|---|---|---|
| Behavior | [task-review-behavior.md](task-review-behavior.md) | FAIL: new Rust shutdown failure path |
| Invariants | [task-review-invariants.md](task-review-invariants.md) | FAIL: failed diagnostic sink and Rust shutdown |
| Standards | [standards-review.md](standards-review.md) | FAIL: bounded changed-item rustdoc |
| Maintainer | [maintainer-review.md](maintainer-review.md) | FAIL: helper docs and Rust shutdown |
| System resilience | [system-review.md](system-review.md) | FAIL: independent drain skipped on telemetry failure |
| Security/RBAC/tenancy | [domain-review-security.md](domain-review-security.md) | PASS within domain |
| Persistent declaration/replay/bundle integrity | [domain-review-data.md](domain-review-data.md) | PASS within domain |
| Fresh focused follow-up | [followup-review.md](followup-review.md) | RESOLVED competing tracing-closure claims |
| Fresh structured Ponytail validation | [findings-validation.md](findings-validation.md) | Three code findings retained, four prior code findings closed, two evidence blockers retained |

All required reports are complete. [claims-comparison.md](claims-comparison.md) explains grouping and the focused follow-up decision. Ordinary file routing was proven, but follow-up found the pinned subscriber's separate raw-stderr fallback on later write failure. Independent validation resolved that conflict from source. Other proposal differences concerned coverage or stable-ID grouping, not conflicting runtime facts. Agreement alone did not validate any finding.

## Reconciled cumulative acceptance

| Original obligation | Implementation and verification evidence | Reconciled result |
|---|---|---|
| TASK-001 local-only check, disk/reference diagnostics and authoritative real declaration; REQ-002/AC-002 | Existing loader/check precedes client creation; original offline/negative/real declaration proof retained | PASS |
| TASK-001 coordinated declaration names across wire, SQL/replay/audit, CLI and three SDKs; REQ-003/AC-003 | Existing owners renamed together; SQL transaction/replay/auth semantics and distinct outcomes preserved; original served/runtime/generated proof | PASS |
| TASK-001 artifact-free get and merged authenticated selectors; REQ-004/AC-004 | Existing read/selector owners; SDK/CLI/MCP get/refusal/catalog proof | PASS |
| TASK-001 verified complete pull/new bundle format, metadata-only and retired-format refusal; REQ-004/AC-005 | Existing graph/download/staging/publication/reader chain retained; R1 hides progress and PTY proof covers real corrupt transfer/prior-bundle retention | PASS |
| TASK-001 derive-owned bundle recovery | Both attributes now prescribe complete pull/destination; actual reader negative tests inspect derived remediation | PASS; FIND-TASK-001-1 CLOSED |
| TASK-002 exclusive machine streams under diagnostics, including failure; REQ-006/AC-007 | Healthy-file R1 tests pass, but subscriber-internal failed-write stderr fallback remains reachable | FAIL; FIND-TASK-002-1 OPEN |
| TASK-002 terminal progress isolation | Graph writer uses hidden target before transfer; PTY success/refusal proof reaches original failure condition | PASS; FIND-TASK-002-2 CLOSED |
| TASK-002 typed results/errors, unattended hosts, readiness/foreground cleanup and saved-state recovery | Existing command/error/installer/DevServer/proxy owners retained; reported original and R1 focused execution | PASS for those paths; machine-stream sink failure above remains |
| Explicit changed-item rustdoc gate | Prior exact items fixed, but new helper insertion displaced decoder docs and new public tracing initializer omits panic contract | FAIL; FIND-TASK-002-3 OPEN |
| TASK-002 exactly three target authorities and portable dependency/source behavior; REQ-010 | AGENTS/installer inventory and supported dependency facilities conform | PASS source conformance |
| TASK-002 supported-host runtime evidence; AC-010 | Only Linux x86_64 execution recorded; missing Linux arm64/macOS arm64 proof | BLOCKED evidence; FIND-TASK-002-4 |
| TASK-003 MCP start/instructions and role boundaries; REQ-007 | Static existing guide owner, actual catalog/proxy proof and unchanged authorization; no new write/check/upload capability | PASS |
| TASK-003 consistent continuation and visible real language initialization; REQ-001/008/009 | Pull syntax/resources/bindings/slots align with actual Service; visible Rust/TS/Python source embeds use existing public owners; fresh docs proof | PASS original cause; FIND-TASK-003-1 CLOSED |
| TASK-003 safe repeated deployment | Python/TS pre-deletes removed; fresh caller preservation tests plus separate shared publication/integrity proof | PASS; FIND-TASK-003-2 CLOSED |
| TASK-003 new Rust graceful shutdown preserves independent row drain; REQ-008/INV-004 | New published serve returns from telemetry failure before calling state shutdown; content/compile proof does not exercise that branch | FAIL; new FIND-TASK-003-4 |
| TASK-003 source-derived full reference, honest local-provider outputs and partial/publication caveats | Generator uses existing embed owner; 44 fresh docs tests pass; reported actual local-provider quickstart retained | PASS within stated source/local-provider limits |
| TASK-003 compatible released-artifact acquisition proof | Recorded dated unavailable-artifact evidence remains; no compatible installed-package run supplied | BLOCKED evidence; FIND-TASK-003-3 |
| INV-001–007 and explicit non-goals | Existing pure contracts, Rust/server/shared client/SQL/RLS/audit/security/publication owners remain; no compatibility/process manager/new SDK runtime/MCP write/sibling-repo/release/live automation added | PASS except validated code gaps above |
| AC-011 task contribution and final change separation | Focused evidence assessed cumulatively; final integrated gate and TASK-004 not claimed | Task scope reviewed; no final change completion |

## Stable finding closure and open ledger

Four prior code IDs are CLOSED: **FIND-TASK-001-1**, **FIND-TASK-002-2**, **FIND-TASK-003-1**, **FIND-TASK-003-2**. The original six therefore have four closed and two still open; R1 adds one distinct code cause, producing three open code findings total.

| Open code ID | Validation | Exact owner and consequence |
|---|---|---|
| FIND-TASK-002-1 | CONFIRMED; INV-R2-001/FOLLOWUP-001 | CLI `src/lib.rs:132–136`: pinned subscriber writes internal sink failures directly to stderr, corrupting JSON |
| FIND-TASK-002-3 | REVISED; REPO-R2-001/MAINT-R2-001 | CLI `tests/card_lifecycle.rs:882–908` and `src/lib.rs:95–136`: misplaced/missing helper docs and missing initializer panic contract fail explicit rustdoc gate |
| FIND-TASK-003-4 | CONFIRMED; BEH-R2-001/INV-R2-002/MAINT-R2-002/SYS-R2-001 | Rust app `lib.rs:121–135`: telemetry error returns before independent buffered-ticket drain |

The next unused TASK-003 number is 4. New serve error sequencing is a different producer/correction/proof from old 003-1's command/resource/visibility mismatch. Old 003-1 stays closed; this issue is recorded only as 003-4, avoiding double counting. Full traces, disposition of every proposal, minimal correction and focused proof are in [findings-validation.md](findings-validation.md). Rejected nonblocking style proposals are excluded.

Separately, **FIND-TASK-002-4** and **FIND-TASK-003-3** remain evidence-only BLOCKED. They request actual missing execution evidence, not platform code or a release. Source portability and honest caveats do not supply those proofs. Current external registry availability was not independently refreshed, so dated packet observations are not represented as new facts.

## Verification limits and handoff

[verification.md](verification.md) records fresh range diff-check, 44 docs tests, one Python and one TS deploy regression passing. Some reviewers repeated those focused tests. The invariant reviewer additionally reproduced failed diagnostic writes using an existing unrebuilt binary; pinned source is the primary proof. No fresh Cargo/CLI full runtime suite, Rust serve negative test, other-host, live-provider/agent or compatible released-artifact execution was performed. Reported R1 checks remain prior evidence, not new review runs.

No new material product/architecture decision is needed. One bounded handoff goes directly to `$wyrd-implement`: [TASK-001-003-R2-machine-stream-and-shutdown-closure.md](TASK-001-003-R2-machine-stream-and-shutdown-closure.md). The review does not execute it, stage, commit, merge, publish or deploy. Once code findings close, unavailable proof prerequisites still require BLOCKED rather than PASS.
