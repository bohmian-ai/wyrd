# Focused stale/unused/retired grouping sweep

**RESOLVED** — the added sweep has concrete, bounded cleanup outcomes below. No source was changed. These are **HUMAN-CLEANUP** additions, separate from the independently discovered material-defect ledger; do not assign retrospective FIND IDs.

## Subject and method

Base `22335c2c40a5ad24c27bfa0d4686f4da49f922c5`; candidate `7cda4e93159ab328de2a74068d5e1978a2590818`. Reviewed the original task at base and approved specification revision 17, AGENTS and agent rules, the task-review skill, cumulative diff, current grouped owners and their callers, existing discovery reports, and root claims comparison. The uncertainty is the user's additional instruction: all unused, retired and stale code in the grouped-verification ownership being replaced must be deleted. It does not authorize deleting unrelated repository infrastructure.

CodeGraph was consulted before source search (`codegraph explore "verification groups GroupPoller GroupSnapshot VerifierExecutionQueue pending predecessor removed coverage ceiling"`). Follow-up searches inspected current source and consumers, excluding superseded change-review packets. Source only; no tests/builds were run.

## HUMAN-CLEANUP 1 — delete unused DueGroup projection fields

Locations: `crates/wyrd/wyrd-sql/src/queries/verifier_executions.rs:989` (`DueGroup::due_at`), `:993` (`card_uid`), `:995` (`verifier_card_uid`); their construction in `claim_due_group` at `:1758–1784`; `GROUP_BINDING_SQL` at `:236–242`; `DUE_GROUP_SQL` at `:229–234`.

These fields have producers and no consumers. A repository-wide `DueGroup` reference search finds only its definition, `claim_due_group`, `GroupPoller::poll`, and `Visible::pin`. The scheduler uses binding/group identity, owner principal, owner Card and settings. The poller reads owner Card/settings/group identity; `Visible::pin` uses binding/group identity. The SQL integration claim assertion at `pg_verifier_executions.rs:2681` reads binding/group/settings/owner, not these fields. `due.due_at` at query line 1673 belongs to the distinct **DueBinding** schedule path and must remain.

Smallest deletion boundary:

- Delete these three fields and their rustdoc/constructor assignments from **DueGroup** only.
- Stop selecting/deserializing `due_at` in the pending claim's returned projection. Keep the pending table column, its update, due-time filtering/order and due indexes: those are active scheduling state.
- Narrow `GROUP_BINDING_SQL` and its result tuple to owner Card and frozen `group_end`; remove only the discarded subject/Verifier identity decoding in this claim path.
- Keep `resolve_binding` and its exact target/readiness checks in `enqueue_group`; they own admission. Keep the exact owner/subject/source-Verifier matcher in `RECORD_GROUP_ACTIVITY_SQL`. Do not delete persisted Card/Verifier columns from verification bindings or GroupActivity.

Proof: compile/typecheck the existing SQL/server owners and run the existing focused `group_snapshots_are_idempotent_and_react_in_settlement_order` and `pending_groups_debounce_and_stop_after_handoff` tests when implementing. No new test is needed to mirror field deletion. Inspect the remaining DueGroup producer/consumers and confirm the three fields are absent.

## HUMAN-CLEANUP 2 — remove stale descriptions of retired behavior

1. `crates/wyrd/wyrd-server/src/verification/runner.rs:402–403` says a group snapshot runs "unless a source exceeded the read ceiling, which completes inconclusive without running." That branch and its outcome helper were deleted. Replace that clause with the actual behavior: a group snapshot runs over its pinned source lists. Keep the rest of dispatch's description and its actual live branches (`ExecutionInput::Grouped { inputs, .. }` at `:445`).
2. `docs/src/content/docs/understand/how-verification-works.md:166` says "`schedule` is the only automatic Trigger." Delete that sentence; retain its following Bind-a-Verifier link. The same page's table at `:163` and group-end explanation at `:168` describe the other automatic Trigger. This is a stale contract explanation inherited immediately beside the changed grouped documentation.
3. `crates/wyrd/wyrd-mcp/tests/bifrost/mcp/verification.rs:1652` still describes a media assertion as "review, coverage, or refusal". Remove the retired coverage term and describe the live selected-execution/provenance assertions, while doing the terminology cleanup below.

Proof: read the corrected documentation against the real dispatch branches and after-only Trigger; normal scoped docs/format checks suffice. Do not recreate the retired ceiling/verdict behavior to make the comment true.

## HUMAN-CLEANUP 3 — retire grouping's stale review names, preserve the live tests

The six fixtures below are **live**, not unused: Service fixture references reach them and the SDK/MCP tests register those Services. Delete the old filenames/metadata names through an atomic rename, not by deleting their useful Verifier behavior or keeping compatibility aliases. The original task's "Verification terminology" paragraph already explicitly requires this rename and all callers. Suggested concrete names use the task's existing Verifier vocabulary:

| Old fixture basename / metadata.name | Replacement basename / metadata.name | Direct fixture callers |
|---|---|---|
| `conversation-review` | `conversation-verifier` | `support.yaml`, `support-ambiguous.yaml`, `forever.yaml` |
| `coding-review` | `coding-verifier` | `coding.yaml`, `coding-external.yaml`, `coding-unbound.yaml` |
| `event-review` | `event-verifier` | `frames.yaml` |
| `late-review` | `late-verifier` | `late.yaml` |
| `refund-review` | `refund-verifier` | `refund.yaml`, `refund-on-failure.yaml` |
| `undeclared-review` | `undeclared-verifier` | `support-undeclared.yaml` |

All paths in that table are under `fixtures/cards/verify_in_real_time/`. Rename each YAML and its metadata.name together; update every relative YAML reference listed. Update accompanying fixture comments/descriptions in `coding-external.yaml`, `coding-unbound.yaml`, `support-ambiguous.yaml`, `frames.yaml`, `late.yaml`, `reserved-source.yaml`, `undeclared-review.yaml`, `late-review.yaml`, and `refund-alert.yaml`. Preserve the source grammar and exact version/source identities.

Live helper/caller boundaries to rename rather than remove:

- Rust `sdks/wyrd-sdk-rust/tests/integration/verify_in_real_time.rs:2495` `group_reviews` → `group_executions`, its calls at `:2540` and `:2659`; `assert_no_new_review` at `:2532` → `assert_no_new_group_execution`, with all calls at `:2722`, `:2865`, `:2985`, `:2994`. Update grouping-local `review`/`reviews` variables, assertion messages and rustdoc in the grouped refund, coding/media, conversation, late-evidence, and authority scenarios in this file. They read retained ordinary executions; their real readback/no-new-execution assertions must remain.
- Python `sdks/wyrd-sdk-python/tests/integration/test_verify_in_real_time.py:854` `group_reviews` → `group_executions`, calls at `:929` and `:954`; update grouped result variables/messages and refund history assertions. Update the coding fixture-name description in `tests/integration/support.py:32`.
- TypeScript `sdks/wyrd-sdk-ts/wyrd/tests/integration/verify-in-real-time.test.ts:897` `groupReviews` → `groupExecutions`, calls at `:951`, `:955`, `:978`; update grouped result variables/messages and fixture-name descriptions (including `:53`).
- MCP `crates/wyrd/wyrd-mcp/tests/bifrost/mcp/verification.rs:1331` `group_review` → `group_execution`, calls at `:1466`, `:1747`, `:1783`, `:1825`; update its helper parameter/result names, grouped comments/rustdoc and grouped assertion messages, including `ids` at `:1312`.
- The fixture-only webhook `/hooks/refund-review` in `refund-alert.yaml:11` becomes `/hooks/refund-verification`; change both receiver lookup occurrences in Python (`:604`, `:607`) and both expected-path values in TypeScript (`:644`) together. Keep both webhook assertions and the Operator action.

There are no current production SDK exports or generated declarations named GroupReview to delete. These stale names are fixture/test vocabulary. Actual code assessment language such as the coding Agent checking whether spans were reviewed live (`spans-reviewed`, prompt statements, LIVE_REVIEW provider matching), the unrelated customer-facing sentence "We will review your request", and software assessment workflow paths do not denote a separate grouping entity. Preserve that behavior and vocabulary where it describes the real check rather than naming a group execution. Do not blindly delete every lexical occurrence of "review" repository-wide.

Proof: source search for the six old fixture basenames and the four old helper names finds no remaining active caller/reference after the atomic rename; read the grouped local names/messages in the four journey owners. Run the existing affected focused grouped journeys in their owning Rust/Python/TypeScript/MCP lanes, retaining their evidence, no-repeat and webhook assertions. Do not introduce a permanent grep gate or new test framework.

## Retirement already complete / live mechanisms retained

- Old `GroupCoverage`, `GroupState`, `GroupSubmissions`, `group_states`, `group_submissions`, `group_through`, `group_members`, `GROUP_POLL_INTERVAL`, source-limit result helper/reason/config, and old outstanding/coverage group indexes are removed from active grouped production/contracts/SDK/docs consumers. The two old duration keys in `trigger.rs:302–303` are deliberate rejection fixtures, not compatibility fields.
- `GroupPoller`, Reader, Sources/SourceSlot, Earliest, Visible, Payloads, and their helpers have live production callers. Earliest serves both input and result deduplication. They are not obsolete discovery machinery merely because their owner name includes Poller.
- Pending schema/indexes/RLS are the new live scheduling owner. Binding schedule cursor/index/authentication arming still serve ordinary cron work. Manual request-key schema/constraints, submission handoff, ClaimLoop/leases, dispatch/settlement, scheduled queries and audit/outbox are live shared behavior. Do not remove them as grouping predecessors.
- PinnedGroup.snapshot and persisted group_sources have active enqueue/status/provenance consumers. The claimed Grouped snapshot also participates in the queue's claimed-input shape and the focused SQL assertion at `pg_verifier_executions.rs:2737`; no deletion is proposed on the strength of runner's `{ inputs, .. }` alone.
- Optional Agent/TraceSource reads, empty-each handling, normal resource bounds, and source/refusal/pressure negative fixtures remain required by the original scenarios. No extra recovery, serialization, fairness or ceiling machinery is proposed.
- Older observation-origin migration/history handling predates this grouping replacement. This sweep does not propose rewriting unrelated earlier Verifier migrations or deleting shared schedule/Drift infrastructure.

## Verification notes

Static source/caller review only; no tests, builds or benchmark runs. Report-only changes; immutable source remains at the candidate. Cleanup items require implementation and focused verification before closure; this report resolves their scope, not their completion. No independently new material behavior finding was discovered in this follow-up.
