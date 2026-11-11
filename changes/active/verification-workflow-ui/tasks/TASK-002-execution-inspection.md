---
id: TASK-002
kind: implementation
status: proposed
spec: SPEC-verification-workflow-ui
spec_revision: 2
requirements: [REQ-002, REQ-004, REQ-005, REQ-006, REQ-007, REQ-009, REQ-014, AC-002, AC-003, AC-005, AC-008, AC-009]
depends_on: [TASK-001]
---

# Exact execution, publications, session and drift inspection

## Outcome and prerequisite

An execution ID from a public SDK call opens a working workflow page with live
recorded state, exact producer outputs and contextual judgment evidence. Session
and drift use this same graph/inspector; cancel preserves accepted evidence.
No discovery list or GitHub engine is needed to open that exact ID.

Consume 001's shared graph. Activation additionally requires the existing
building-block contract delivery researched in
[source alignment](../design/source-alignment.md): exact Verifier-authorized
retained execution/result reads, bounded invocations/publications, content/
download and cancel, with generated SDK/MCP projections and real journeys.
Main's older `get_execution`/Card-read/query path is not that dependency output.
Record the integrated source revision; do not duplicate the runtime or expose
sensitive Bifrost payloads to make this task appear unblocked.

## Flow and owner

Extend the shared component/loader family (`VerificationResult`, requirement
detail and drift feature presentation) and typed read/action contracts in
`packages/ui/src/lib/{contract,loaders,node,mcp}.ts`, catalog/view entries and
host dispatch. Reuse existing Chart, Card content and exact trace inspection.
Inputs come from domain execution state, pinned group snapshots, invocation and
publication pages; bodies are fetched only for an opened exact producer.
For drift history/adjacent-window navigation, reuse authorized contextual reads
when available; if the delivered inspector lacks the bounded projection, extend
that existing domain read and normal client surfaces here. Do not depend on the
later dashboard or bypass permissions with public payload SQL.

Mount the same components at the existing agreed production execution route
`/t/[tenantKey]/verify/executions/[executionId]`. Use generated identities,
not mock hash routes. Public SDK-created IDs provide the first real consumer
journey before 003's list exists. Preserve exact execution/result, selected
step/requirement, item/iteration/attempt and output across refresh and copy links.
New publications/failures do not relayout nodes or steal selection. Retained
speculative/discarded state does not become successful judgment state.

Show equal named output metadata with bounded safe text/Markdown/JSON/directory
previews and authorized downloads, never a primary-report shortcut. Render
session pinned chronological members and recorded requirements, with exact
member/citation navigation and restored selection on return. A failed session
requirement does not make passing members or edges appear responsible. Drift
retains baseline/method/window, all recorded features, threshold/history and
highlighted viewed window; previous/next uses exact recorded results. Never
derive a verdict, membership or baseline distribution in the browser.

Trace reads use current evidence permissions and exact span identity, outside
list predicates; current spans do not pretend to be a past snapshot. The
server-selected result stays authoritative after later spans arrive. Inputs
start collapsed; initial failed-requirement selection yields to explicit links
and subsequent user selection.

Cancel is an authorized host action, guarded while pending and confirmed by
domain readback. Completion can win; keep its terminal result. Reopening
cancelled work retains accepted outputs. Unknown/foreign, denied, unavailable,
not-yet-visible, skipped and terminal-error states remain distinguishable.
No UI retry/resubmission controller. Exact wait details/approval actions belong
to 006; GitHub delivery projection belongs to 007.

## Observable proof

At existing UI unit owners, test selection/producer changes, safe/oversized
preview refusal, stable graph viewport, session return and drift history. At
the real server/host and production browser owners, create ordinary parallel/
repeated work through public clients, open intermediate output while running,
refresh and reopen terminal state. Assert exact identities and no invented
verdict; page and shared host component show the same retained records.

Create owner-distinct session reviews and drift windows through the existing
public setup; inspect members, cross-workload/untagged/out-of-range trace
citations, later spans, denied/truncated evidence and foreign IDs. Exercise
cancel before settlement and a synchronized completion race, retaining evidence.
Compare ordinary/session/drift and wide graph/inspector regions with the mock;
include keyboard/mobile and both themes. Use the verification owners and commands below
and the domain dependency's real surface journeys; fixtures alone cannot close
this task's delivery dependency or acceptance.

## Execution and verification

This task is proposed against draft spec revision 2. Activation requires
explicit spec approval, the applicable scope transfer, and only this task's
named contract/authority and predecessor outputs. Unrelated backend delivery
or authority amendments do not block it. Record the integrated source revisions
and actual dependency journeys it consumes; upstream task status is not proof.

Read the repo-local `wyrd-ui` skill before UI edits. Reuse the shared component,
loader, catalog, Node/MCP executor and host action owners. Test shared components
in `src/lib/ui-library.test.ts` and pages in `src/lib/ui-pages.test.ts`;
authorized host/built-artifact proof belongs to `ui-library.integration.test.ts`
and production BFF/Chromium/public-login proof to `ui-pages.integration.test.ts`.
Record actual focused selectors for added tests before their first run; follow
the existing TDD discipline without inventing test names in this task.

```bash
mise exec -- pnpm --dir crates/wyrd/wyrd-server/wyrd-ui test src/lib/ui-library.test.ts src/lib/ui-pages.test.ts
mise exec -- pnpm --dir crates/wyrd/wyrd-server/wyrd-ui check
mise exec -- pnpm --dir crates/wyrd/wyrd-server/wyrd-ui build
mise exec -- scripts/postgres/with-test-postgres.sh -- env WYRD_UI_INTEGRATION=1 pnpm --dir crates/wyrd/wyrd-server/wyrd-ui test src/lib/ui-library.integration.test.ts src/lib/ui-pages.integration.test.ts
```

Narrow test files with actual relevant selectors. The integration command needs
the existing `ts:build`, `ts:build:testing`, `build:server` and UI-build
prerequisites; inspect `mise.toml` and the managed Postgres wrapper before
running it. Missing prerequisites are not passing or skipped proof.

Extend normal `mise run dev:full` public setup where needed to demonstrate this
task's real inputs. Independently compare each introduced component/page with
the approved mock on desktop/mobile, light/dark, keyboard/focus/touch, long
content and error states. Record reference regions, screenshots and individual
human acceptance. Fixtures and automated checks alone do not prove fidelity.

For introduced domain reads, use the existing server/MCP verification journey
owners (`wyrd-testing/tests/bifrost/server/verification_runtime.rs` and
`wyrd-mcp/tests/bifrost/mcp/verification.rs`) and the normal SDK integration
trees. Select affected stories through `test:bifrost:journey:server`,
`test:bifrost:journey:mcp`, `test:rust-sdk:journey`, `py:test:integration` and
`ts:test:integration`; preserve the spec's shipped-surface and negative-flow
proof. Exact new selectors are recorded by the implementer. Regenerate through
the owning generators and run `codegen:check`; Rust changes add `fmt`/`lints`,
and SDK changes add their owning format/lint/type, Python stub and N-API
checks. Relevant tenancy/dependency changes add `check:tenant-isolation` and
`check:deps`. Full shipped-surface lanes belong to integrated change review.
