---
id: TASK-008-R6
title: Align current shutdown residue descriptions with staged retention
kind: remediation
status: ready
spec: SPEC-bifrost-scribe-live-reads
spec_revision: 20
requirements: [REQ-004, REQ-005, REQ-014, REQ-015]
acceptance: [AC-016, AC-017]
parent_task: TASK-008
remediates: [FIND-007-13]
---

# TASK-008-R6

Route directly to `$wyrd-implement`. Documentation-only correction under the approved 2026-09-30 shutdown decision; do not change executable behavior.

## Inputs and authority

Approved spec: `changes/active/bifrost-scribe-live-reads/spec.md`, revision 20. Original tasks: `changes/active/bifrost-scribe-live-reads/tasks/TASK-007-one-parquet-scan-for-live-reads.md` and `TASK-008-tenant-proven-per-file.md` in that same directory. Prior remediation/evidence: `review/task-008-r4-review/TASK-008-R5-preserve-oracle-only-peer-drain-and-complete-evidence.md`. Diagnosis and independent source validation: sibling `verdict.md`, `findings-validation.md` and `followup-review.md`.

Candidate `1a66d8a7b4583c9798c2f1573dc6ab1f3eadb027`; correction parent `ca99db0af5a0d898ef67834699405c1c73719f56`; cumulative TASK-007 base `a7582db587c6170a290760f1741673125612b797`; TASK-008 base `f7bebf704d6f3b1dd20d041e70c6ca512c0da307`.

Current authorization remains static only: no cargo, nextest, mise, builds, tests, benchmark or commits. This task grants no exception. FIND-007-10/11/12 are closed. All standing maintainer decisions remain unchanged.

## FIND-007-13 diagnosis

The approved shutdown path closes admission, rotates/flushes admitted generations, drains durable staging and retains below-target staged members for restart. The deleted `shutdown -> publish_staged_residue -> publish_residue(Drain)` chain no longer sweeps every ready key. Startup restores validated staging and original claims before WAL replay/readiness; the ordinary publication tick uses original ready timestamps. Explicit `flush_staged -> publish_residue(Drain)` still deliberately publishes residue, and normal admitted target/dwell publication can finish during drain.

Current descriptions nevertheless assign the removed sweep to shutdown:

- `architecture/references/domain/analytical-operations-reliability.md:40–43` says shutdown closes residue claims.
- `crates/vala/vala-bifrost-redux/src/scribe/assembly.rs:437` describes Drain as graceful settlement before shutdown; :982–985 says every ready key is claimed as residue at shutdown.
- `crates/wyrd/wyrd-testing/tests/bifrost/scribe/lifecycle.rs:106–110` says graceful stop sweeps every staged member and leaves nothing for restart; :149 says stopping owes publication. The associated Panics/proof account at :122–124 and :159–162 also needs to match the retained journey.

These false guarantees misstate the durable boundary and what the journey proves. Its actual assertions already permit zero or complete publication after restart and prove exact acknowledged-row readback; they need no correction. This is documentation correctness within the expressly requested leftover-reference scope, not loss, duplicate replay, failed recovery, or a reason to restore publication at shutdown.

## Selected correction

Delete or replace the false guarantees in those existing descriptions. Reuse the accurate account already present in `scribe/mod.rs:1369–1371` and the explicit-flush worker documentation in `scribe/persistence.rs:2018–2022`. Assign staging retention to shutdown, validated restoration to startup, and due publication to the production tick. Describe Drain as explicit residue flush without renaming it. Preserve the reliability guide's valid admitted-work drain statement. Align the journey prose with exactly-once readback, its existing zero-or-complete publication outcome and its separate later-publication proof.

The source of the inconsistency is the former shutdown producer's documentation; correcting it once across its active descriptions closes the common issue. No downstream runtime guard, test alteration, new helper, dependency, abstraction, check or harness is needed. Worker-field queue-drain wording and the ambiguous publish_due phrase were rejected as standalone blockers and are not extra cleanup obligations.

## Constraints and non-goals

Preserve every executable statement and assertion, explicit flush/residue methods, ClaimCause::Drain and metric labels, ordinary target/dwell publication, admitted-work drain, source leases, ACK/WAL/manifest/claim fences and startup ordering. Preserve Oracle-only/public graceful runners and Scribe-containing stopping IO. Do not rename tests, strengthen shutdown to require publication, weaken test outcomes, rewrite historical evidence or refactor adjacent code. Do not reopen FIND-007-3, excluded SQL tenant columns, the chosen error code, no-cap policy or the shutdown decision.

## Acceptance and focused proof

| Finding | Required outcome |
|---|---|
| FIND-007-13 | Current guide, assembly documentation and lifecycle journey prose no longer promise forced residue publication at shutdown; they correctly distinguish staged retention/restart/tick from explicit flush and valid admitted publication. Runtime code and all assertions remain identical. |

This is a prose-only obligation; use static verification rather than manufactured RED/GREEN runtime changes. Read the corrected descriptions against the full shutdown, flush and ready-key consumer paths; search active shutdown/residue descriptions to confirm the definite obsolete sweep guarantees are gone. Confirm explicit flush and startup/tick remain untouched, and review the diff for documentation-only edits. Run `git diff --check` against the implementation parent and original cumulative base. Existing supplied runtime evidence need not be rerun to prove the wording correction; no new permanent check is justified.

Broader proof remains the recorded fmt/lints, focused 1+3 passes, peer 11/11, redux 872/872, Scribe 20/20 and server 26/26. Record static closure evidence honestly; do not claim fresh runtime results. Any necessary future runtime execution requires separate authorization. A later independent review reassesses the cumulative task. No merge, push, deploy or commit is authorized.

## Implementation evidence (2026-09-30)

Documentation-only. No executable statement or assertion changed.

| Site | Correction |
|---|---|
| `architecture/references/domain/analytical-operations-reliability.md` | shutdown no longer "closes residue claims"; staged members stay staged, startup restores, tick publishes |
| `scribe/assembly.rs` `ClaimCause::Drain`, `ready_keys` | Drain described as explicit flush; shutdown does not sweep |
| `scribe/staging_runtime.rs` drain test doc | "explicit flush" instead of "drain" |
| `tests/bifrost/scribe/lifecycle.rs` `scribe_shutdown_drains_or_preserves_replay` | doc, Panics and inline comments describe retention, restart restore and tick publication; assertions unchanged |

Static checks: `mise run fmt` exit 0; `mise run docs:check` exit 0; `git diff --check a7582db58…` and `git diff --check 1a66d8a7b…` exit 0; search of shutdown/stop residue descriptions in architecture, Scribe sources and Scribe journeys finds no remaining shutdown-sweep guarantee. No runtime run claimed for this step.
