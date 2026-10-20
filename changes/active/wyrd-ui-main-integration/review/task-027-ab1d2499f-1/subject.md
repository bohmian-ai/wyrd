# TASK-027 immutable review subject

Repository `/Users/stevenforrester/Documents/GitHub/wyrd-ui-main-integration`.
Candidate `ab1d2499f6a08a6b5afe1f6c904e72746d526bda`; base `a1e9b48d2550bc8e8ae8ef8037ce813e5beeeddb` (`d33de26e4^`). Read complete `git diff a1e9b48d2...ab1d2499f`, plus `git show d33de26e4 d90d98a12` to distinguish task implementation from later planning/evidence and merged unrelated upstream changes. Source reviewed at candidate. TASK-003 is separately reviewed; inspect its existing owners as dependencies, do not attribute pre-existing defects to TASK-027.

Authority: approved `changes/active/wyrd-ui-main-integration/spec.md` revision 12, original `tasks/TASK-027-view-layout.md` in that packet. Read AGENTS.md, architecture/agent-rules.md, spec-driven-development.md, maintainer-style.md, reference router and applicable UI/design/testing authority. No `.codegraph/` exists. Full role rules at `.agents/skills/wyrd-task-review/SKILL.md`.

## Starting navigation map (expand independently)

UI root `crates/wyrd/wyrd-server/wyrd-ui`; package owner `packages/ui/src/lib/`. `schemas.ts::LayoutSchema/PanelSchema` bounds, strict document validation and generated `catalog.json` via `scripts/catalog.ts`; pure `layout.ts::place/edit` and constants; `WyrdView.svelte` grid ordering, component containment, effective content/load identity, measurement, pointer/keyboard editing, commit/blur/cancel. `view.ts::MountViewOptions/mountView` host handler; `packages/ui/src/app/main.ts` MCP App per-render handler. Consumers: `src/routes/_gallery/+page.svelte`, `examples/ui-app/src/panels.ts` and `src/routes/view/+page.svelte`. Existing component fill behavior: Chart/CardHeader and shared primitives. Tests: `src/lib/ui-library.test.ts` four TASK-027 scenarios and TASK-003 scenarios 1/3 for regressions; inspect assertions against producer-to-consumer paths.

## Evidence and procedure

Original task includes exact focused scenario commands and recorded green results, UI 37 tests, check/build, example app check/build and codegen:check; human visual acceptance explicitly approved 2026-10-20. Recorded Playwright evidence covers desktop/light/dark/mobile, pointer move/resize, keyboard resize, redraw, collisions, no horizontal overflow. Automated real-browser driver deferred to TASK-011 by approved task. Independently assess evidence rather than assuming summaries establish acceptance. No persistence, added/removed panels, component size inputs, new dependencies or saved-view state are authorized.

Source immutable; not alone in workspace. Write only your assigned report, do not read peer reports during discovery. No implementation, merge, push or deployment. Findings need exact source evidence, observable consequence, violated obligation and smallest safe testable correction; report PASS/FAIL/BLOCKED and required coverage. Compare new machinery with existing owners and callers. Review each task independently.
