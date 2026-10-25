# TASK-007 remediation review scope

Repository: /Users/stevenforrester/Documents/GitHub/schema-change-task-7
Original base: 76e095738d2cf89e10523a92bb9b4f585fa1dd3d
Prior candidate: dabe0e7dde58c6e26d584120ae0a48cfede79436
New candidate: bfe479b9248f4f71c43a719b61db1ce35bad69d9
Review delta: git diff dabe0e7dd bfe479b92
Cumulative context: git diff 76e095738...bfe479b92
Initial HEAD equals candidate; tracked tree clean. No .codegraph directory or applicable nested AGENTS exists. Prior candidate is an ancestor, so no patch correspondence/rebase audit is needed.

## Authorities and prior evidence

Read AGENTS.md; agent_docs/index.md, rust.md and maintainer-style.md; changes/spec-driven-development.md; TESTING.md; approved revision-16 spec and lookup-performance supplement for service-identity-and-telemetry; applicable Bifrost measurement/source/authority sections. Original task: tasks/TASK-007-verification-query-performance.md in this change packet (original obligations at original base). Prior review: review/TASK-007-dabe0e7dd/{verdict,findings-validation,task-review-behavior,domain-review-query-execution,verification}.md. Remediation: review/TASK-007-dabe0e7dd/TASK-007-R1-complete-performance-evidence.md, which now appends implementation evidence. Actual Git diff and source govern; appended PASS claims require validation.

Carry forward prior source-backed accepted production ordering/distributed planning, exact result/window/group/trace semantics, source visibility, authority, limits/deadlines and defaults/non-goals. Verify whether any actual delta affects these; do not rediscover unchanged production paths or demand broader experiments. Reopen only a specific affected or invalidated prior proof, documenting why.

## Starting navigation map (not a conclusion)

- wyrd-testing/src/bifrost/telemetry.rs: BifrostTelemetryDelta::resources and resources_render_elapsed_and_process_scope. Existing checkpoint/delta/process capture and evidence rendering own elapsed, CPU/Tokio busy and RSS. New method exposes a read-only evidence line; trace callers and true scope of reported resources.
- tests/bifrost/server/verification_runtime.rs: WORK_FAMILIES/ScanEvidence::of adds query duration and elapsed/resource output; existing measured result/window/group/custom/trace paths consume it.
- Exploration: Explored/EXPLORED now includes audit, fixed cutoff for growing audit; Attributed::of maps named JSON columns including omitted nulls; POSITIVE and audit keys; StoredBlooms::read separates writer/audit filters; Release/gateway setup and release-specific calls; exploration_filters_keep_exact_populations now uses full/partial/empty declarations and measured equal writer rounds, known positive/UID-name equivalences and audit keys. Trace modified assertions to actual public producers and source answers.
- Existing gateway capture/server/provider mocks and typed-payload journey are reuse context. No private capture or new ingress should be needed.
- Task Markdown replaces obsolete unsupported claims with R1 evidence; R1 appended measurements record three repetitions per side and baseline production-file substitution on common harness. Verify baseline correspondence, honest incorrect-answer handling, scope/variance and process attribution from source/evidence.
- Prior reports are newly committed in this delta; preserve them unchanged as historical review context. Inspect changes versus prior record only when needed, not as new runtime behavior.

## Reviewer selection and rules

One fresh ponytail-rev for remediation closure, affected test/telemetry callers, standards/rustdoc and evidence. No production/query/auth/persistence boundary is materially changed; no additional discovery/domain/repo/lifecycle specialist is selected. Prior reports cover unchanged production boundaries. No intended verdict is supplied. Fresh validator must independently prove closure of FIND-TASK-007-1/2, retain stable IDs if open, explicitly validate empty open ledger if closed, and flag any new proposed blocker for separate focused source validation before retention.

Write reports only in this new directory; do not modify reviewed source, stash/checkout source for measurements, save patches/diffs/archives here or rerun the original whole discovery campaign. Tests through mise may run; coordinate Cargo with orchestrator. Orchestrator will retain current focused rerun results separately. Follow-up only for material unresolved path/conflict/new blocker; decision to be recorded on receipt of validation.

## Validation reconciliation and scope closure

The fresh remediation validator completed findings-validation.md. Both prior findings are closed; its independently validated open ledger is empty. No additional specialist or follow-up is selected: no new blocker, material conflict or untraced failing path was identified. Existing measurement logs supplied the missing detailed cells; the validator independently summarized them in its report instead of prescribing another experiment. The report narrows an unsupported blanket below-variation timing claim to actual bounded observations. This is evidence interpretation within the existing obligation, not a new finding or scope expansion. Production acceptance carries forward because neither Oracle file, Cargo manifest nor lockfile changes in the remediation range. Final verdict also depends on completing the orchestrator's recorded focused rerun and confirming immutability.
