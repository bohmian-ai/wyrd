---
id: TASK-005
kind: implementation
status: proposed
spec: SPEC-verification-workflow-ui
spec_revision: 8
supporting_spec: SPEC-authorization-model
supporting_spec_revision: 9
requirements: [REQ-001, REQ-002, REQ-003, REQ-004, REQ-005, REQ-006, REQ-011, REQ-012, REQ-014, REQ-020, AC-001, AC-002, AC-007, AC-008, AC-009]
depends_on: [TASK-002, TASK-003, TASK-004, TASK-005A]
---

# Current and historical PR verification sets

## Outcome and activation

A person granted one Repository follows its PR link to the latest review set,
selects an earlier set, opens a member and retained output, then returns with
the same selection. Another Repository contributes no members, counts or facets.
Each member keeps its own status and recorded verdict; a set has no combined
verdict.

Approved revision 8 reconciles REQ-003/020 with current retained-parent authority
and records this task's no-definition presentation in the selected
[resumption design](../design/resume-005-007.md). Before implementation, record
the integrated predecessor outputs and actual dependency proof below;
specification approval does not certify their runtime qualification.

TASK-003 is implemented. Consume its shared discovery queries and URL/filter
owners; do not restart it. TASK-002 supplies the inspector and exact producer
reads; TASK-004 supplies Repository navigation. TASK-005A supplies admitted
generations and complete retained membership, including refusals. Its request
and retention have recorded delivery evidence; the old table-only read blocker
is superseded by authorization-model REQ-006/007 and the current
[retained-parent authority](../../../../architecture/wyrd-security-posture.md#resource-and-retained-data-visibility).
Record the integrated revision and focused dependency results; neither old
proposed headers nor specification approval establish runtime acceptance.

## Existing owners and connected implementation

Carry forward the design's “Proposed TASK-005 revision” and its rationale.
Existing `execution_inputs` rows supply provider host/stable repository ID,
PR number, exact Repository policy, PostgreSQL observation time retained as
`recorded_at`, generation, complete member IDs and request/delivery identity.
TASK-005A's explicit review request is fixture input where needed, not a new UI
action. Admission authorization remains independent of reading a set.

Use existing caller-authenticated `bifrost.query` through the shared Node/MCP
executors. Oracle applies parent visibility before expressions, counts and
pagination. Repository Card read includes its PR execution/evidence reads,
including custom roles; no referenced-Verifier read or GitHub configuration
grant is needed. Table permission alone cannot unlock another parent's rows.
Opening a Verifier definition checks that Card independently.

Extend existing shared schemas/loaders/catalog/components, `observe.ts`,
`execution.ts`, `verify-host.ts` and Repository/page owners as appropriate:

- Resolve current from the newest retained observation for that PR, not row
  arrival, SHA alone or the Verify time picker. Explicit generation links never
  fall back to current when absent. Read bounded history with explicit bounds.
- Use frozen member identities and show incomplete retention/truncation
  truthfully. Reuse server-selected result semantics: a queued member's selected
  checkpoint result supplies its verdict, not the newest arbitrary result row.
  The dashboard's time-range/slack assumptions cannot silently cut off old sets.
- Add the approved PR context to Repository activity, Verify filters/groups/
  columns and exact execution links. Keep general verification filters and both
  charts on the same authorized population. No per-member status/detail fanout
  to render a set or list.
- Reuse the inspector on member open. When definition lookup is refused or the
  Card has been deleted, keep its truthful refusal and allow selection of
  readable recorded requirements, invocations and publications. Use existing
  node/producer selection and bounded reads; do not invent authored nodes/edges,
  make a definition grant implicit or hide completed progress solely because
  its definition is unavailable. Occurrence/decision controls belong to 006.
- Retained history uses its recorded identity; a current Repository policy or
  GitHub configuration read must not become a prerequisite for a retained PR
  link. Independent definition/configuration failures do not erase readable
  execution facts.

Route names, query decomposition, bounds, private helpers and component
factoring are implementation-owned. No new server endpoint, SDK wire field,
membership ledger, permission evaluator, migration, provider lookup, rerun
button or separate PR dashboard. Backend retained-status correction is in 007
and is not a predecessor of this task.

## Ordered acceptance scenarios and evidence

Implement each missing scenario test-first. Add assertion locations, exact
selectors, observed RED/GREEN and limitations to these entries; do not create a
second evidence ledger. Already-correct upstream behavior needs focused reuse
proof, not manufactured failures.

1. **Open an actual current set.** Public connection/declaration and PR admission
   produce a multi-Verifier generation and retained pins. A permitted caller
   loads it through the shared Node/MCP loader: exact source/policy/member IDs
   and each member's nullable selected result, status and verdict match the
   accepted records, with no combined verdict and no detail/status fanout.
   **Map:** REQ-003/012/014/020, AC-007/008. **Tier/owner:** shared loader and
   real-server `ui-library.integration.test.ts`. The missing PR-set loader/
   presentation should produce RED.

2. **Keep current and history exact.** Request a fresh review at the same head,
   then advance the head. Follow current, pin an earlier generation outside the
   Verify time range, refresh with delayed older records, and request an absent
   pinned generation. Current follows the latest retained observation; pinned
   history/execution never switches, and missing pinned history is reported
   rather than replaced. Interrupted retention, refused members and a read/page
   bound never turn a subset into a complete set.
   **Map:** REQ-002/003/012/020, AC-007/008. **Tier/owner:** existing real-server
   UI integration owner consuming actual admission/retention outputs; pure
   selection/bounds branches in `ui-library.test.ts`. Missing history and
   completeness handling should produce RED; reuse backend retention proof.

3. **Respect Repository grants everywhere.** Two Repositories share a Verifier.
   A family-scoped reader and a minimal custom Card-read role see only their
   Repository's sets/members/counts/pages/Verify facets. An exact-version grant
   covers only its policy UID; table-only permission reveals no hidden managed
   rows. GitHub configuration stays denied. Foreign/inaccessible execution links
   are indistinguishable from unknown; grant/removal affects subsequent
   authoritative reads in the existing signed-in session.
   **Map:** REQ-001/003/011/012/014/020, AC-001/007/008;
   authorization-model REQ-004/006/007. **Tier/owner:** real server → shared
   Node/MCP reads and production BFF in the existing UI integration owners.
   Extend the existing Alice journey's actual grants/admitted reviews rather
   than replacing its assertions. New PR-set consumer mismatches should be RED;
   unchanged source-boundary enforcement may already pass.

4. **Inspect without definition access.** The permitted Repository reader has
   no grant to the member's Verifier definition; also exercise retained work
   after a source Card is deleted while its retained parent remains authorized.
   Open the member, select a recorded requirement/producer/output and refresh.
   Authorized facts, bounded progress, previews/downloads and exact selection
   remain usable; graph/YAML stays unavailable and no topology is fabricated.
   **Map:** REQ-002/004/005/006/014, AC-002/008; authorization-model REQ-006.
   **Tier/owner:** component test plus real-server loader and BFF/browser.
   Missing recorded-selection navigation, including completed progress skipped
   because definition lookup failed, should produce RED.

5. **Connect navigation and hosts.** Through the production browser follow
   Repository → PR → older set → member → output → Back. Copy/reload links and
   change the PR while each selection is open. Return context, selected
   generation/execution/producer and existing general discovery behavior remain
   correct. Exercise affected reads in Node/MCP/custom/built-artifact hosts,
   denied/unavailable/empty/partial states, and the approved desktop/mobile,
   light/dark, keyboard/touch and long-content composition.
   **Map:** REQ-001/002/011/012/014, AC-001/007/008/009. **Tier/owner:** existing
   UI host/artifact and production-browser integration owners. Missing links/
   return wiring should produce RED. Supplement the approved mock only for
   definition-unavailable recorded selection; record AC-009 comparison evidence
   and get the maintainer's ruling on remaining mock differences as required by
   [wyrd-ui](../../../../.agents/skills/wyrd-ui/SKILL.md#matching-an-approved-mock).

## Verification and handoff

Read `wyrd-ui` before UI edits. Existing owners are `ui-library.test.ts`,
`ui-pages.test.ts`, `ui-library.integration.test.ts` and
`ui-pages.integration.test.ts`; fixtures supplement connected journeys.
Record new focused selectors before RED. Current prerequisite regression commands:

```sh
mise exec -- scripts/postgres/with-test-postgres.sh -- bash -lc 'mise run db:migrate:all:inner && cargo nextest run --locked -p wyrd-server --test integration -E "test(=pg_github_pull_requests::retained_reads_resolve_current_and_history_membership)"'
mise exec -- scripts/postgres/with-test-postgres.sh -- env WYRD_UI_INTEGRATION=1 pnpm --dir crates/wyrd/wyrd-server/wyrd-ui test src/lib/ui-pages.integration.test.ts -t 'Alice is given payments in Settings'
```

Prepare current `ts:build`, `ts:build:testing`, `build:server` and UI build
prerequisites through mise before UI integration. Run focused new/affected
component and integration selectors, `mise run ui:check`, UI build, catalog
regeneration and `mise run codegen:check` for schema/catalog changes. Apply
affected BFF/session owner checks from the UI skill, including
`mise run test:identity:ui` where that boundary changes. Preserve existing
query consolidation and bounded reads; add no benchmark or unrelated gate.

Planning claims no implementation, runtime proof or independent acceptance.
Full shipped-surface journey lanes belong to integrated change review.
