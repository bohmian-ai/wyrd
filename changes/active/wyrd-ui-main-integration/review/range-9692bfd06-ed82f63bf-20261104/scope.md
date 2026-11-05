# Immutable review subject
Root: /home/thorrester/Documents/GitHub/wyrd
Base: 9692bfd06
Candidate: ed82f63bf35ca124c0a2980c160f0d64535eebd1
First full range review: git diff 9692bfd06...ed82f63bf. No carried-forward acceptance evidence. Source immutable; reports only. User authorizes all potential spec deviations; assess correctness and mandatory rules, without inventing approval gates. Explicit required behavior remains acceptance context.

## Authority and original assignments
Read AGENTS.md, applicable nested rules, changes/spec-driven-development.md, agent_docs/maintainer-style.md, architecture/README.md and owning sections. Approved spec: changes/active/wyrd-ui-main-integration/spec.md revision 27 including revisions 20–27.
Original tasks under changes/active/wyrd-ui-main-integration/tasks/: TASK-004-observe-components.md; TASK-022-metrics-page.md; TASK-004-revision/TASK-04-bifrost-bloom-pruning.md.
Skill: .agents/skills/wyrd-task-review/SKILL.md. UI reviewers also read .agents/skills/wyrd-ui/SKILL.md. CodeGraph exists: mise exec -- codegraph explore before searching/reading code.

## Starting navigation map (expand independently)
UI root: crates/wyrd/wyrd-server/wyrd-ui.
- packages/ui/src/lib/observe.ts: SQL loaders, histogram/aggregation, metric discovery/labels, keyset sorting. Consumers src/lib/observe-host.ts, routes/t/[tenantKey]/observe/+page.server.ts, +page.svelte, +layout.svelte, metrics/+server.ts. Contracts schemas.ts/loaders.ts. Proof ui-pages.test.ts, ui-pages.integration.test.ts, ui-library.test.ts.
- Presentation: Chart, Drawer, RecordDetail, RecordTable; brand/gen-theme.mjs and generated CSS; Home/Query/Settings/Account padding. Accessibility and light/dark obligations apply.
- vala-bifrost-redux tables/metrics/{points,projection,series}.rs, tables/mod.rs registration, gate/mod.rs; scribe/{series_index,execution_lanes,ingress,replay,mod}.rs; tests/scribe_persistence_path.rs. Trace identity producer through durable admission and series/point reads.
- Physical schemas/pruning: tables/{fields,managed_columns,signal,mod}.rs and signal tables, schema/fingerprint.rs, catalog/bifrost_catalog.rs, oracle/{codec,exec}.rs. Cargo pins and ORACLE_DATAFUSION_REVISION; external fork commit 5f4b1713a boundary outside local diff. Tests oracle/exec and wyrd-testing tests/bifrost/oracle/{distributed,published}.rs; tagged-spans-wal fixture.
- Tooling: scripts/dev/example-app.sh, deleted patch, MockServer expectations; SDK card-types.ts; docs generated tokens.

## Available verification claims
Caller reports metrics journey passes, 516 library tests pass, oracle journey five passes, touched-crate fmt/clippy pass, fork opener metadata filter test passes. Distinguish reported from independent evidence. Read task evidence. Coordinate costly tests with orchestrator; no synthetic host load.

## Reviewer selection
behavior-rev: complete range acceptance and realistic caller-result paths.
invariant-rev: UI SQL numeric precision, pagination, grouping and filtering.
repo-rev: all changed surfaces mandatory rules, rustdoc, generated parity, authority.
maintainer-rev: loaders and series admission ownership/reuse.
system-rev: Scribe failure/replay/publication/cache propagation and test synchronization.
domain-rev-persistent-data: series identity/index integrity, schema/WAL compatibility, Bloom/fork boundary.
Reports include exact coverage, source-based material findings, acceptance matrix as applicable, verification limits and PASS/FAIL/BLOCKED. Independent ponytail validation follows. No implementation edits.
