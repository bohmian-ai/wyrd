# Immutable review subject

Repository: /home/thorrester/Documents/GitHub/wyrd
Base: 4a76331113864f9bddc43d3cc8df01080a696937
Candidate: f6aeb83a77179ac4450110671df6f564adc1914a
Original task: changes/active/wyrd-ui-main-integration/tasks/TASK-004-revision/TASK-02-live-observe-workflow.md (TASK-004-REV-02)
Approved spec: changes/active/wyrd-ui-main-integration/spec.md revision 18.
First independent review of this task: complete base...candidate range; no prior task-review acceptance carries forward. Retired TASK-004 R1 is inherited obligation/evidence context, not a prior verdict on this candidate.
Working tree was clean at start. Source must remain immutable; only this review directory may be written. Read the actual Git diff directly; do not save patches here.

## Navigation map (starting points, not coverage conclusions)

- Authenticated shell: src/routes/t/[tenantKey]/+layout.{server.ts,svelte}; Observe and Query +page.server.ts/+page.svelte under that route; src/lib/observe-host.ts.
- Shared owners under crates/wyrd/wyrd-server/wyrd-ui/packages/ui/src/lib/: WyrdView.svelte, observe.ts, schemas.ts, plot.ts, components/{Chart,FacetRail,RecordTable,TraceWaterfall,RecordDetail,StatStrip,Drawer}.svelte; transport contract.ts, node.ts, mcp.ts; app /ui/load route and server auth/session owners.
- Reference compositions: changes/active/wyrd-ui-main-integration/references/canvas/{Main,Spans,TraceDrawer,TimePicker,Logs,Metrics}.dc.html and png/; inherited tasks TASK-012/020/021/022 and references/retired-task-004/TASK-004-R1-canvas-fidelity.md.
- Shared/page proof: src/lib/ui-pages.test.ts, ui-pages.integration.test.ts, ui-library.test.ts, ui-library.integration.test.ts, ui-signals.integration.ts. Task Execution Evidence records verification claims; independently inspect source and assertion scope.
- Predicate authority: wyrd-server/src/query/{floor,service,routes}.rs and mcp/bifrost.rs; wyrd-spec/src/vala/api.rs; wyrd-client/src/bifrost/{query,facade,blocking}.rs; Python/TS Bifrost projections and named predicate integration tests.
- Dev lifecycle: mise.toml dev:full/dev:batch, scripts/postgres/dev-server.sh, scripts/dev/example-app.sh and wyrd-example-py.patch; canonical base pinned in task.

## Scope and reviewer selection

behavior-rev: full original task acceptance and invariants, including design fidelity and evidence requirements.
maintainer-rev: changed frontend composition/loaders and predicate ownership; compare existing whole workflows, reuse, SSR/BFF lifecycle, and unnecessary duplication.
repo-rev: full cross-tier diff and mandatory rules, Rust docs, Python/TS exports, generated projections, tests/tooling.
domain-rev-security: changed predicate parser/authorization/tenant disclosure boundary and all consumer projections.
system-rev: changed dev startup/drain/failure lifecycle and BFF load/cancel/failure propagation.
No separate invariant reviewer: behavior reviewer covers shared UI state, and security/system specialists cover their boundaries.
A fresh independent validator follows discovery. Follow-up only if a material conflict, untraced path, or new validator finding needs separate validation.

## Human direction

User rejects the delivered implementation and requests a decision-complete correction task. Requirements highlighted: reusable time picker, existing components, approved left-rail and trace/log/metric boards, conventional SvelteKit SSR/BFF. These are acceptance priorities, not evidence of specific defects. Verify against source. Any genuinely new human addition must stay separate from FIND IDs.
