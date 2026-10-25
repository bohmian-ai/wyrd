# Remediation-instruction refinement

Repository: /home/thorrester/Documents/GitHub/wyrd-worktrees/benchmark-experiments
Base: 0739bbf38af7cc05bbfefb09850490c55432ec2d
Candidate: aaa152dd4243d57752b73d3a4e9c140bccc7d171
Source unchanged since first review; this is refinement of the correction packet, not a new implementation candidate, finding-closure round or comprehensive acceptance audit.

Authority: changes/active/benchmark-experiments/spec.md revision 5 approved; original tasks/TASK-001-benchmark-experiments.md. Carry forward complete original review TASK-001-R1: scope, all selected reviewer reports, focused follow-up, findings-validation.md, verdict.md and verification.md. Carry forward all FIND-TASK-001-1 through -12 with no closure claimed. Preserve old remediation TASK-001-R1-restore-benchmark-evidence.md unchanged.

Pre-existing uncommitted telemetry i64 repair/export test remains outside candidate; original verification records successful exporter, runner/declaration checks and Clippy, one format difference. No benchmark or fresh source check is implied by this refinement. Two source files must remain unchanged; review artifacts only.

User asks concrete recommendations and refined remediation, after clarifying desired standard setup, latency/CPU/core metrics, benchmark practices, ease for external contributors and smallest implementation. Later advice proposed making historical comparisons/L2/profiling specialized optional features. That is not approved scope reduction: current REQ-009/011/012/015 retain obligations. Independently determine which instruction refinements preserve authority and which would require spec revision. Do not silently drop earlier required server-only exact timing in favor of client round trips.

## Navigation/coverage

Prior ledger maps source owners and callers: telemetry record_timing -> OTLP -> capacity Collector/Population -> engine readings -> shared experiment Repetition/derive/judge -> Record publish/render. levels assembles common controls; engine declarations add controls. Bifrost Isolated/Competing and Verification Bench have frozen resource-window/tally seams; Gateway Capacity owns scheduling/capture; release_server/os owns portable resource readings; Profiler finish and runner own finalization. No source diff since original review; expand only for a specific correction-burden/authority question.

## Proposed instruction refinements for independent validation (not intended verdict)

- Retain shared runner and real workload owners; original review established no broad abstraction DRIFT.
- Present four bounded execution stages: unblock exporter plus mandatory smoke first; trustworthy collection/accounting; engine correction/metric completeness; documented default sequential L1 and original final qualification.
- Group findings by shared source and outcome rather than twelve unrelated mini-projects; batch mandatory rustdoc with code and keep final proof separate from source correction.
- Smoke must reuse final production launch/collection/sample decoder/metric readings; seconds-scale work after release/prerequisite build, all engine producers/handoffs, explicit missing fields, abort all L1/L2 paths; rerun after shared-path changes.
- Preserve exact metrics/units/windows, both CPU denominators, matching/loss/cancellation and existing security/storage behavior; no ad hoc parser, second launcher/collector or generic scheduling framework.
- Refined task must keep decision-complete correction and focused proof for every retained finding without prescribing private helpers/control flow.
- Optional baseline factoring, ~65 lines of dead helper cleanup, singleton resource-shape refactor, new configuration DSL or implementation rewrite are not independently validated material remediation and must not become required work.
- Reducing mandatory L2/historical comparison/profile behavior needs spec revision and is separate from current FIX_REQUIRED implementation corrections; no new approval question is needed to finish the in-scope packet.

## Selection

One fresh ponytail-rev independently checks correction burden, source-backed existing alternatives, authority compatibility and carried evidence, with exact report findings-validation.md in this new directory. No first-round team is recreated because no source/authority changed and no new discovery is requested. If a new blocker is discovered, obtain separate focused source validation before retaining; no new findings are presumed. Existing acceptance verdict stays immutable until approved source correction or spec revision.
