---
id: TASK-006A
kind: implementation
status: proposed
spec: SPEC-verification-workflow-ui
spec_revision: 9
requirements: [REQ-005, REQ-008, REQ-014, REQ-022, AC-015]
depends_on: []
---

# Report task input waits while independent work runs

## Outcome and execution agreement

Implement approved [REQ-022/AC-015](../spec.md) using the
[finalized design](../design/input-wait-reporting.md). An engineer or agent sees
which declared task needs which deferred input while a peer runs and an approval
is pending, through existing public status and the shared UI. This is one
reporting change across existing owners, with unchanged scheduling/parking.

The maintainer explicitly waived TDD ordering on 2026-11-24. Tests may follow
implementation; do not require or invent RED evidence. Append assertion paths,
actual focused commands, passing results and limitations to the acceptance
entries below. Report TDD `NOT_APPLICABLE — maintainer waiver`; tests and
repository checks are still required. Read `wyrd-implement` and `wyrd-ui` before
execution, plus owning SDK guidance. No implementation review, commit, push or
deployment is authorized by this task.

Inputs already exist: the Verifier's declared references, engine pending nodes
and missing-slot set; `RunState` and its fenced Scribe writer; retained inspection;
SDK status readers; TASK-006's shared inspector and held-provider fixture.
No new prerequisite task is needed. Use their integrated implementations, not
their historical task-status headers. This task supplies the missing reporting
and simultaneous-state proof needed by TASK-006-R1; it does not reopen completed
rejection/idempotency or Origin work. Revision 9 overrides that remediation's
backend exclusion only for REQ-022. The architecture deferred-input section
already records the approved contract; keep its wording aligned if implementation
requires explanatory corrections, without expanding semantics.

## Connected implementation

1. In `wyrd-spec/src/verification/{execution,state}.rs`, add the shared
   `InputWait { task_id: TaskId, slots: BTreeSet<SlotName> }` and optional/default
   `input_waits: Vec<InputWait>` to public status and recoverable state. Apply
   existing schema/serialization derives and slot-set treatment. Bump state format
   5 → 6, retaining 3–5 decoding with an empty list. Update all constructors,
   including direct status and `wyrd-sql`'s response projection. Change generator
   sources when needed and regenerate artifacts; do not commit JSON Schemas.
2. In `vala-verify/src/blocks/engine.rs::Engine::drive`, reuse `waited(node)` for
   the complete pending-consumer snapshot after readiness and before awaiting
   running tasks/returning Waiting. Extend the existing `Checkpoints` boundary
   with a synchronous fallible snapshot callback, using existing checkpoint
   failure behavior. Steps include attached requirements via existing
   `input_slots`; standalone requirements use their own task ID. Sort entries;
   do not introduce another readiness evaluator or fabricated invocation.
3. In server `verification/state.rs::RunState`, replace snapshots through
   `change`, skip identical snapshots and preserve writer fencing/asynchronous
   Scribe staging. In `accept_evidence`, remove the consumed slot from every
   entry and drop empty entries. In terminal selection/failure clear active
   waits. Retain `Runner::deferred`'s existing consume-before-drive behavior:
   publication alone does not clear waits in a running drive. No additional IO,
   database statement, capacity ownership or wake mechanism is needed.
4. In `components/verification/service.rs` and `service/inspection.rs::Retained`,
   project retained waits with current parent authorization. Terminal inspection
   must return empty even when cancellation/supersession bypasses ordinary engine
   completion. Existing lifecycle projection stays authoritative; neither the
   list nor pending approval changes `running` into `waiting` while peers run.
5. Carry the field through existing Rust, Python, TS, HTTP, MCP and CLI status.
   Python's authored `verify/__init__.pyi` needs the TypedDict projection and
   matching export treatment; inspect its runtime owner before adding wrappers.
   TS generated `card-types.ts` comes from `codegen:regen`; existing generic
   status wrappers preserve the field. Audit actual exports/constructors without
   introducing client-owned behavior.
6. In UI `packages/ui/src/lib/execution.ts::executionStates` and
   `components/VerificationExecution.svelte`, map reported tasks/slots to the
   existing waiting presentation; remove graph-edge inference including
   `awaitedBy`. Include waiting tasks in the existing definition-free recorded
   view as `Task <id>`. With definitions show step/standalone requirement waits
   in their existing positions; attached checks belong to their step. Missing
   input presentation comes from reported slots (plus any retained parked
   summary), never graph guesses. Reuse `loadExecution`; no extra read is needed.

No new endpoint, scheduler, wait enum/framework, migration, dependency, feature,
permission model or history API. Preserve existing errors and availability
semantics. Format-6 readers/writers require the approved coordinated upgrade:
stop old workers first; add no rolling-deployment switch or dual writer.

## Acceptance and implementation evidence

1. **Report exact pre-dispatch consumers.** Start with direct step consumers,
   attached checks, a whole-Verifier requirement, an indirectly blocked dependent
   and an unused deferred slot. Drive the real readiness code: only direct
   consumers appear, with ordered unique task IDs and exact missing-slot sets;
   no invocation/item/iteration is fabricated. Replace with the same snapshot:
   no new retained revision; a stale writer changes nothing. **Map:** REQ-022,
   AC-015. **Proof:** engine unit owner `blocks/engine/tests.rs` and server
   RunState tests. Current missing snapshot callback/projection is the gap.
2. **Observe concurrent work through real status and UI.** Extend
   `fixtures/cards/approvals/beside-review.yaml` with a `ci-review` step reading
   `ci`; keep the `green` requirement and held `review-notes` provider. Through
   the existing real-server TASK-006 library journey, read and render `running`,
   empty parked summary, pending approval, both input consumers, a genuinely
   started independent peer and no verdict, all together. Exercise the existing
   no-definition reader and a definition-available rendering. The blocked step
   has no invocation; the UI receives task waits from `loadExecution` status.
   **Map:** REQ-005/008/014/022, AC-015. **Proof:** tier-1
   `ui-library.integration.test.ts`, plus focused component mapping tests. Current
   status reports inputs only after parking; fixture-only states are insufficient.
3. **Consume, resume, recover and end cleanly.** With an actual peer held,
   publish deferred evidence: the running drive retains its wait until consumption.
   Release the peer and let existing wake/recheck consume it; waits clear, the
   consumer runs, and the same execution continues. Exercise partial consumption
   of multiple slots, park/restart recovery and terminal inspection for completion,
   cancel, supersession, deadline and access loss; none reopens terminal work.
   Verify retained status after queue-row cleanup. **Map:** REQ-022, AC-015.
   **Proof:** tier-1 server `tests/integration/pg_deferred_evidence.rs` and existing
   cancellation/inspection owners; reuse relevant lifecycle cases. A scripted
   engine may prove wake behavior but cannot prove concurrent task-wait semantics.
4. **Keep stored and public contracts coherent.** Round-trip format 6; decode
   3–5 empty; refuse future formats; resume an older state to populate waits.
   Read the field through public Rust/Python/TS and MCP clients using existing
   deferred-evidence/verification journey owners. A reader with the retained
   parent but no definition grant succeeds; another parent/tenant receives the
   existing indistinguishable refusal. CLI uses typed status JSON unchanged.
   **Map:** REQ-014/022, AC-015. **Proof:** state unit tests; focused SDK
   `deferred_evidence.rs`, `test_deferred_evidence.py`, `deferred-evidence.test.ts`
   and MCP `tests/bifrost/mcp/verification.rs` journeys, generated/type/export
   checks and CLI source inspection. No new authorization evaluation is needed.

## Verification and handoff

Use deterministic gates for concurrency, existing repository Postgres and
`WyrdTestServer` runners. Add actual focused selectors for new tests to the
entries above; confirm they select tests. Existing TASK-006 journey selector:

```sh
mise exec -- scripts/postgres/with-test-postgres.sh -- env WYRD_UI_INTEGRATION=1 pnpm --dir crates/wyrd/wyrd-server/wyrd-ui exec vitest run src/lib/ui-library.integration.test.ts -t TASK-006
```

Prepare existing `build:server`, `ts:build`, `ts:build:testing` and UI build as
required by that runner. Focused Rust tests use `mise exec -- cargo nextest run
--locked -p <owner> --lib -E 'test(=<actual-name>)'`; server journeys use the
managed-Postgres wrapper, `db:migrate:all:inner`, `-p wyrd-server --test integration`
and exact new selectors. Existing SDK/MCP runners and setup are in `TESTING.md`
and `mise.toml`; use their focused equivalents for the owners in scenario 4,
not whole-suite runs as substitutes for exact named checks. Run Cargo-backed
commands sequentially.

Run `mise run fmt`, `mise run lints`, `mise run codegen:regen`,
`mise run codegen:check`, `mise run ui:check`, `mise run ts:typecheck`,
`mise run py:format:check`, `mise run py:lints`, `mise run py:typecheck`,
`mise run check:deps`, `mise run check:tenant-isolation`, and `git diff --check`.
Build the affected UI with the repository's existing recipe:
`mise exec -- pnpm --dir crates/wyrd/wyrd-server/wyrd-ui build`. No benchmarks or
broad aggregate; integrated change review owns full shipped-surface lanes.
Return implementation status and evidence in these acceptance entries, identifying
any unverified behavior and the coordinated-upgrade limitation. Do not rewrite
the expectations to match results or claim task acceptance without review.

Planning check: the independent read-only comprehension pass covered revision 9,
this task and its TASK-006/007/remediation dependencies. It identified one stale
specification exception list; that list now includes REQ-022. No further material
ambiguity was reported. Metadata, local links, preserved revision-8 archive and
diff whitespace were checked; runtime behavior remains unverified.
