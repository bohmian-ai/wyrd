---
id: TASK-005
kind: implementation
status: proposed
spec: SPEC-verification-workflow-ui
spec_revision: 3
requirements: [REQ-002, REQ-003, REQ-011, REQ-012, REQ-014, AC-007, AC-008, AC-009]
depends_on: [TASK-002, TASK-003, TASK-004]
---

# Current and historical PR verification sets

## Outcome and input

A general PR link opens its latest review set; explicit history and execution
links remain pinned. Each set shows its actual Verifier executions with separate
status/verdict, then opens the same workflow page. Repository activity and Verify
filters use that exact association. There is no aggregate verdict, special
review Verifier type or separate DAG viewer.

Consume 002's exact viewer, 003's summary/read/filter owner and 004's Repository
workspace. The delivered admission capability must supply durably accepted execution IDs
with frozen exact Repository policy, PR source and review occurrence/generation.
Record actual delivered contract and surface proof. Accepted admission is an input,
not a claim that the review-set list/history read already exists. No dependency
on PR extraction or provider delivery is needed to prove navigation with ordinary
declared Verifiers admitted through the real PR path.

## Same-commit request dependency

Before this task is ready, the external admission owner must confirm
which supported input requests another review at the same commit and exposes
its durable review-occurrence identity. A distinct explicit request creates a
new occurrence with the policy/membership frozen at acceptance; retry/redelivery
of the same request reuses that occurrence and does not admit duplicate members.
The inspected approved admission contract names opening/reopening,
readiness and source changes, but does not yet define this request path.
Exploratory design text is not confirmation or delivered API evidence.

Record the owner's confirmed operation/contract revision and real journey proof
here before activation. Confirmation is pending; this task remains proposed.
Do not invent a public endpoint, create review membership in the browser or add
a UI rerun button. This dependency does not block tasks 001–004 or 006/007 beyond
their own stated predecessors.

## Extension and consumer

Read actual accepted admission/set identity, exact frozen member execution IDs,
and current/history association through existing permitted Bifrost queries or
delivered domain operations. Reuse TASK-003's shared UI query/filter owner and
member summaries; TASK-002 supplies per-member detail on opening. Do not add a
server read, table, Card kind, SDK wire projection or membership ledger, or infer
membership from timestamps, display names, SHA alone or session group text.
A per-execution PR generation is input evidence, not proof that an existing
operation supplies full set membership/history. Record that external capability
before activation; if absent, retain the dependency instead of implementing it
here. These constraints preserve exact membership and authorization.

Extend the existing typed shared loader/host contract and use the approved PR
context in the [composition](../design/consolidation.md) and [mock](../design/mock/README.md).
The general tenant-scoped PR link follows the latest admitted set across head
updates; a selected set/execution stays exact during live refresh. Show recorded
repository, PR/head, policy and member identities. Wire Repository activity,
copy links and Verify repository/PR filters to these actual server associations.
Each member opens 002 with correct return context. A review set with no recorded
judgment does not acquire a calculated verdict or imply every member completed.

## Observable proof

Use real connection/declaration/admission outputs to admit multiple Verifiers
together, rerequest at the same head and then advance the head. Through existing
HTTP/SDK/MCP/Bifrost reads and the shared UI loaders assert current resolution, bounded history and exact
frozen membership, authorization before counts and no foreign disclosure.
The UI reads must consume the actual accepted admission records through existing
query/domain operations. Include a set with running, judged and details-unavailable
members: do not fetch every member's detail to render the set, invent a combined
verdict or let delayed detail replace selected history.

In the real browser, follow the general PR link, select a previous set, open a
member/output and return. Advance source while each view is open: only the
unselected current link follows latest; selected history/execution remains
pinned. Repository activity and Verify filtering select those same records.
Verify no combined verdict and no recomputation/resubmission. Run shared host
and built-artifact parity and the owner checks below
for the changed UI contracts. Compare PR context/history/member list with the approved
mock; no fixture-only admission closes this task.

## Execution and verification

This task is against approved spec revision 3, whose scope transfer is
synchronized. Activation requires only this task's named contract/authority
and predecessor outputs. Unrelated backend delivery
or authority amendments do not block it. This packet consumes existing read/action
capabilities and adds no underlying server changes; a missing required capability
remains an explicit external dependency. Record the integrated source revisions
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

Use the existing UI integration owners to prove real Bifrost/domain inputs through
Node/MCP/BFF/custom/built-artifact adapters. Do not add or change server/SDK wire
contracts or their generators. Regenerate the UI catalog when schemas change and
run `mise run codegen:check` where applicable. Full shipped-surface journey lanes
belong to integrated change review; focused UI/host journeys prove this task.
