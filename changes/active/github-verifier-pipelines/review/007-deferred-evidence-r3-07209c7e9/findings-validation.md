# Independent structured validation

**Recommendation: FIX_REQUIRED.** FIND-007-1 and FIND-007-6 close. FIND-007-2 remains maintainer-rejected. FIND-007-3 remains open solely for the previously required regression proof; no current production defect is alleged.

## Immutable subject and method

Candidate HEAD `07209c7e9af1664a30bf548e64be08937c4f88df`; prior candidate `74fc2190e255a3e3cf7515091ff45ab86a288f6b`; original base `16ea9694c34a5ec1867306bd5e34df62f0888611`. HEAD independently checked exact. Reviewed actual incremental Rust diff, complete publication/receipt/test bodies, subscriber admission, custody acceptance/fence identity, original task and appended R2 Obligations/Evidence, R2 verdict/validation and current discovery, scope, verification and ledger reports. CodeGraph consulted before source navigation; missing function portions then read directly.

Authorities checked: approved revision-7 REQ-009 replay/validation/continuation; architecture Verifier deferred input publication, Bifrost standalone table/row identity and fenced ACK custody, security publisher/consumer authority; root rules, Rust test rules, maintainer and spec-driven task/proof discipline. The R2 validation explicitly requires the focused regression to fail on the prior reconciliation; the R2 task requires actual production receipt reconciliation, not a privately mandated helper. Current user dispositions override the older delegated-test requirement and accept INPUT_INVALID for Card-read admission refusal.

This is independent validation of discovery proposal R3-BEH-1, not self-validation of a new finding. No new blocker was discovered. No source, tests or Git state were mutated; no Cargo/Docker process was run by this validator.

## Specialist applicability and coverage validation

The separate fresh remediation discovery report and this separate validator satisfy the required independent roles. Selection/omission rationale in scope.md is sound for the actual contracts:

- Artifact authority/custody remains the same existing publisher-owned TenantEvidence admission followed by canonical custody, with unchanged consumer authority. The change adds connected assertions, not new trust or tenancy semantics. Prior R1 security/data and R2 validated owner traces carry forward. No new domain specialist is needed.
- Receipt extraction moves the existing calculation beside its caller without changing fences, batches, queue, failure propagation, retries or accepted data. Prior data/invariant/system coverage remains applicable; the missing targeted proof is reviewed here. No new state/lifecycle specialist is needed.
- Changed Rust docs, connected fixture, pure synchronous helper and stable-code assertions are covered directly. No broad cross-tier or substantial ownership change warrants a new repo or maintainer specialist. No generated, SDK, SQL or frontend surface changed.
- Prior R2 acceptance and its R1 source/assertion matrices cover unchanged direct eligibility, scheduling, resources, exact policy/source identities, SQL named decoding, restart and all client projections. Carry-forward is grounded in those evidence rows rather than unchanged paths alone.

No unavailable required report remains. A follow-up reviewer is unnecessary: discovery's one proposal is independently source-resolved below; there is no material conflict or new common cause.

## Reconciled acceptance and closure

| Obligation | Independently inspected implementation/assertion or carried-forward source evidence | Result |
|---|---|---|
| FIND-007-1 publisher denied Artifact read, refusal before custody | Service subscribers builds TenantEvidence with publisher Caller and admits before accept. Connected test seeds run-only role, publishes valid Artifact and asserts INPUT_INVALID/400; subsequent authorized identical source/input/value receives accepted, proving prior refusal retained nothing | CLOSED; source plus recorded connected PASS |
| FIND-007-1 successful consumption by same execution | Test declares `extract` from deferred report, CSV output requirement, waits for `[report]`, then obtains completed/passed for the same `waiting` execution ID after publication | CLOSED; source plus recorded connected PASS |
| FIND-007-6 stable refusal identities | Artifact matrix retains wrong format/path/version/size; asserts exact status and INPUT_INVALID or INPUT_TOO_LARGE codes; valid publish follows | CLOSED |
| FIND-007-2 attribution runtime proof | `caller.attribution()` remains reused; maintainer rejects delegated publication runtime-test requirement for recorded unsupported credential-root reason | REJECTED; no further work |
| FIND-007-3 losing-offer reconciliation | Actual service merges canonical raced records into held and clears fresh; new unit supplies held and false already arranged, then tests receipt calculation | Production correct; proof FAIL |
| FIND-007-4 lifecycle routing / FIND-007-5 named SQL mapping | R2 independently accepted source/assertions remain unaffected | CLOSED carried forward |
| REQ-008 registration/direct refusal and consumers | Prior behavior/invariant/standards exact registration and engine assertion matrices | PASS carried forward |
| REQ-009 baseline source/subscription/policy matching, unmatched/history, ACK, conflicts and sequential replay | Prior security/data/behavior and R2 matrices; main connected replay asserts matching evidence_id/time, ACK-unknown path retained. Current helper is equivalent calculation for supplied state | PASS except focused race proof above |
| REQ-011 / AC-007 compute/claim ownership, attempts/budget/deadline, same-ID restart/running consumers | Prior invariant/system/behavior and R2 accepted lifecycle assertions; no changed runtime dependency | PASS carried forward |
| INV-004 unavailable never absent/fresh | Prior canonical fence/read source trace and unknown-publication refusal/retry proof; maintainer accepts source evidence for recovery read | PASS carried forward |
| INV-005/006 authority, supersession/revocation and no terminal reopening | Prior security/system/invariant matrices; no affected accepted-authority owner changed | PASS carried forward |
| REQ-016 / AC-011 shipped projections/docs/source resolution | Prior cross-tier standards/behavior/maintainer coverage; common service result still used | PASS carried forward |
| Changed docs, fixture limits, non-goals and owner reuse | New pure helper/test carry intent docs; Artifact test documents outcome/Panics. Shared ci_gate drops unused report max64 but no size assertion is removed; new report-gate preserves max64 and 413 assertion. No table, dependency, hook, protocol, permission or second lifecycle owner | PASS |

## Final deduplicated retained ledger

### FIND-007-3 — CONFIRMED / MISSING: the new regression excludes the repaired losing-offer transition

Discovery source: R3-BEH-1 (retains prior DATA-007-2/FIND-007-3). Violated obligation: R2 task FIND-007-3 and focused acceptance, R2 validation's regression requirement, original REQ-009 identical accepted evidence replays its canonical receipt. Exact location: `crates/wyrd/wyrd-server/src/components/verification/service/evidence.rs:145–162,403–442`.

Producer-to-consumer trace: a supported publication initially reads no acceptance, builds a fresh offer and local `now`, then loses canonical custody to another identical publisher. Custody returns an error; service rereads canonical rows for fresh subscribers, rejects conflicts or incomplete resolution, then transfers the winning records into held and removes fresh state at lines 155–156. Receipt status/time at line 162 consequently describe replay at the winner's first timestamp. HTTP and MCP share this owner. The deterministic evidence identity is source/content-derived and conflict checks remain intact.

The new test constructs a map already containing the canonical record and passes literal false to receipt. It does not feed a losing offer or canonical reread through the production state transfer. Removing `held.extend(raced)` leaves the unit unchanged even though the service would return its offered timestamp; removing `fresh.clear()` also leaves the unit unchanged even though the service would answer Accepted. Removing both restores the exact original incorrect receipt while this test still passes. This is established by dependency inspection, not a mutation of reviewed source. The identity assertion compares the fixture's inserted clone with its original; receipt neither selects nor returns identity. Sequential HTTP replay starts with held already present and bypasses this race/error path, so its accepted coverage cannot close this gap.

Observable consequence and materiality: current runtime code is correct, but the explicit focused regression does not detect reintroduction of the previously confirmed supported concurrent-publication receipt defect. This is unfinished approved remediation proof, not hypothetical extra concurrency hardening and not a Docker limitation. Fresh unit PASS establishes only the narrower formatting behavior tested.

Correction burden / selected minimum: preserve the existing publication/custody/fence owners and correct current semantics. Complete deterministic owner-level proof from losing fresh offer plus canonical acceptance to the production-used canonical replay result, including first acceptance time and unchanged identity. The tested production boundary must own the state transfer rather than having its corrected results prearranged solely in test setup. A small local rearrangement at this owner is sufficient if needed for direct testing; private helper names/signatures/layout remain implementation-owned. Deletion or another receipt-formatting assertion cannot satisfy the missing transition proof; no dependency, public API, new persistence/concurrency protocol, test-only production hook, sleep, probabilistic race or general harness is justified. Preserve neighboring conflict/completeness refusal, ACK/unavailability and wake-on-replay behavior.

Focused closure proof: one deterministic regression uses distinct offered and canonical timestamps and establishes Replayed plus first accepted timestamp through the actual production-used losing-offer reconciliation. Its assertions must fail when the canonical state transfer is omitted or the losing offer is left fresh. Keep existing canonical identity, real custody and sequential public replay evidence; no schedule-dependent integration race or whole-task rerun is required.

## Ledger checker reconciliation

`ledger-check.md` is accurate with one interpretation made explicit: a Source cell pointing to “this file” is weak direct traceability, not absence of a contract when the diagnosis resolves it to REQ-009 or the mandatory Rust stable-code rule. No ledger row adds a wholly unsupported product promise. FIND-007-1/-6 duplicate valid-after-refusal acceptance as shared proof; FIND-007-3 is focused proof of the same original REQ-009 replay promise. Original REQ-008/-009 share no-publication-started-run; REQ-011/AC-007 share same-ID durability; INV-004 shares custody/recovery proof. Do not multiply tests or findings for those overlaps.

The FIND-007-3 planned “extracted production receipt” is private implementation wording, not an acceptance constraint. The reason for this retained finding is missing behavior proof, not failure to follow a helper prescription. FIND-007-2 rejection is an explicit disposition, not an unimplemented open obligation. Ledger normalization itself is non-blocking and creates no remediation work.

## Verification limits and convergence

Orchestrator verification records exact selected unit 1/1 PASS, formatting and whitespace PASS. Artifact exact selector/module PASS and custody/sequential replay PASS are implementation-recorded evidence. Connected execution was not repeated because Docker is unavailable as stated by the user; broad SDK/whole-task gates are outside this round. No claimed fresh connected PASS is retained.

This is the second unsuccessful remediation review. The sole remaining issue is incomplete proof of the original receipt repair. No fix-induced production regression, missed new runtime defect or expanded expectation is retained. Non-convergence comes from extracting only final formatting while leaving the repaired transition outside the tested boundary. Next scope is that transition and its deterministic regression, preserving all closed findings and maintainer dispositions. No specification or boundary revision is required.

Skipped fresh connected tests and broad rediscovery. Risk: the new green unit cannot detect loss of the canonical race reconciliation despite source-correct production behavior.
