---
id: TASK-003
kind: implementation
status: proposed
spec: SPEC-verification-workflow-ui
spec_revision: 2
requirements: [REQ-001, REQ-002, REQ-003, REQ-014, AC-001, AC-008, AC-009]
depends_on: [TASK-002]
---

# All-status discovery and the Verify dashboard

## Outcome

An engineer enters Verify, discovers unfinished and completed executions,
combines filters, opens 002's workflow page and returns to the same list state.
The compact verdict and Verifier charts, facet counts and table agree. This
task delivers the required new domain reads with their actual page consumer;
it does not wait for GitHub review extraction/delivery.

## Input, extension and consumer

Use accepted execution/status and selected-result records at existing
`VerificationControl`/queue/retained-read owners, not results-only enumeration.
Add bounded execution-summary/filter/aggregate reads to `wyrd-spec`, the owning
server verification routes/service and normal shared client, Rust/Python/
TypeScript SDK and MCP projections. Reuse existing generated identity/error
contracts and domain authorization. No new runtime/table or browser fetch of
all IDs. Exact schema/route spelling follows owner conventions; refuse and
escalate if it requires a material behavior/ownership change from REQ-003.

Summaries include exact execution/subject/Verifier and nullable selected result/
verdict, lifecycle, existing Service/Workload/Run/session/binding correlations
and applicable repository/PR context. 005 adds the actual PR admission/review
association and history reads; this task must not fabricate those values.
Authorize exact Verifier scope before list pagination, counts and facets.
Counts/charts cover the same filtered population across pages; no-result work
uses a neutral bucket, not a failed or fourth verdict. Status/verdict combine
normally, including empty incompatible combinations. Preserve existing null
correlations and distinction between application Run and execution.

Consume these generated operations through the shared closed WireRequest,
Node/MCP executors and portable loaders, then reuse Chart/FacetRail/RecordTable
and Observe's investigation grammar at `/t/[tenantKey]/verify`. Match the
[dashboard composition](../design/consolidation.md#approved-dashboard-behavior)
and [mock](../design/mock/README.md): dense table including Service, separate
Status/Verdict columns, short chart legends and combined general filters.
Dragging either chart changes the shared time range. Preserve WHERE/range/
Live/grouping and return position. Rows open the page directly, without a
drawer/Expand requirement. Connect 001's Results in Verify using exact Verifier
identity. Grouping is navigation only, not a calculated aggregate judgment.

## Observable proof

Use real SDK → server/Postgres → client journeys on all new shipped read
surfaces to create queued/running/terminal executions, absent and recorded
verdicts, distinct scopes/tenants and enough rows for pagination. Assert
combined filter results, aggregates across pages, no unauthorized counts,
nullable correlations and structured denied/unavailable outcomes. Implement
pure filter/shape tests only where behavior needs them; no fixture can replace
the domain journey.

Through the production BFF browser, navigate the rail, filter/group/brush/page,
open exact intermediate/terminal execution, copy its link and return. Assert
list state restoration and that a delayed/superseded load cannot overwrite
the new filter/selection. Exercise the same projection in existing Node/MCP/
custom/built-artifact host proof. Independently compare both charts, facets,
table columns/scrolling and empty/error states to the approved mock.

Use the verification owners and commands below,
including domain surface/schema/type checks for the introduced operations.
This task owns its read extension; it must not mislabel it an upstream GitHub
prerequisite. Specification approval, the applicable scope transfer and 002's delivered
inspection owner remain activation dependencies; unrelated GitHub amendments
do not block this task.

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
