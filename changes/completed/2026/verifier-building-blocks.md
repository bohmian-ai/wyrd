# Verifier building blocks

- Change: `verifier-building-blocks`
- Specification: `SPEC-verifier-building-blocks`, approved revision 11
- Completed: 2026-10-18
- Reviewed target: `606e3c2d6`. TASK-001 to TASK-004 were owner-accepted.
  TASK-005 was accepted by the owner after its R3 remediation. TASK-006 task
  review R3 returned PASS at `622800c7a` with an empty finding ledger. The
  owner approved completion without a final change review.
- Delivery reference: not supplied

A developer describes the evidence they have, optionally prepares it, and
checks declared requirements through one Verifier:
`inputs → optional steps → requirements → verdict`. The same primitives
cover a request assertion, a document review, an Agent trace, a multi-step
review, and scheduled statistical drift. There is no Verifier implementation
kind to choose. The obsolete Eval subsystem is deleted, and surviving pieces
use verification vocabulary.

## Shipped behavior

- **One declarative Verifier.** A subject-less `VerifierSpec` declares named
  `inputs`, optional `steps`, `requirements`, and `decide` in the normal
  `wyrd/v1` envelope. Step and requirement IDs are unique across the whole
  Verifier and are used unchanged in references, results, overrides, and
  fitted drift-state keys. Registration refuses duplicates, cycles, invalid
  positions, and incompatible run/window shapes.
- **Actions.** `extract` covers nine formats with locked output types (see
  the extraction reference). `agent` reuses Prompt and execution-route
  ownership. `code` runs in a bounded Monty sandbox (2 warm / 8 max workers,
  256 MiB operation, 10-second operations).
- **Checks.** `assert`, `trace`, `agent` (a tool-less, cited judge), `code`,
  and `drift` (PSI, SPC, Custom). Missing evidence, failed dependencies, and
  all-skipped are inconclusive, never a pass. The pass-rate denominator keeps
  missing results.
- **Agent tools.** A built-in read/compute tool catalog is resolved by the
  hosted Invoker. Every call passes two gates: tool invocation and the
  underlying resource permission. Both follow the global-or-scoped rule and
  preserve the subject or delegation.
- **Citations** stay attached to original evidence slots through derived
  output.
- **Entry points.** `verify` returns a real-time verdict. `submit` enters the
  existing batched queue with asynchronous stable-key replay and conflict
  reporting. Both return an execution ID. `status` and `wait` reconnect by
  ID, and another tenant is refused. Local backends upload evidence through
  `PUT /v1/verification/evidence/local/{sha256}`.
- **Scheduled drift** uses explicit slot/window mapping, existing baseline
  readiness, and failed-only Operator reactions templated on
  `{{execution_id}}`.
- **Surfaces.** The shared `wyrd-client` is projected by the Rust, Python, and
  TypeScript SDKs. Also shipped: named MCP tools, served OpenAPI, CLI, UI,
  matched `examples/support-desk`, and standalone `wyrd-example-rs`,
  `wyrd-example-py`, and `wyrd-example-ts` repositories.
- **Eval retirement.** `wyrd eval`, `evals:run`/`evals:invoke`, and SQL over
  `vala.eval.*` are unknown. `vala-eval` is now `vala-verify`. The live
  contracts live in `wyrd_spec::verification` (`ContextCapture`). GenAI OTel
  projections keep the standard `evaluation` semantics
  (`GenAiEvaluationResult`).

## Invariants

- `wyrd-spec` stays IO-, async-, and PyO3-free. Vala owns orchestration and
  statistics. Skald owns the Agent runtime. The server owns admission,
  authorization, evidence resolution, sandbox policy, scheduling, and
  reactions.
- Tenant and principals come from verified identity. A digest, execution ID,
  binding, or SYSTEM actor grants no authority.
- Permissions block and audit goes through the Scribe outbox. Execute and
  enqueue requests write no Postgres rows. Accepted does not mean durable, and
  completed does not mean the result is analytically visible.
- Verifier work is bounded read/compute, with no network, credentials, path
  escape, or state shared across calls or tenants.
- One contract per surface, with no aliases, compatibility routes, or
  historical Eval readers.

## Decisions

- Batched submission is retained, and replay conflicts are reported
  asynchronously (r2).
- Extraction outputs are locked (r3). Monty limits are set at startup (r4).
- Built-in tools are resolved automatically (r5), with two independent
  authorization gates (r6).
- The Task 004 handoff: named MCP tools, execution-based reads, and reuse of
  the existing Operator plumbing (r7). The example repositories moved to
  Task 005 (r8). The local evidence upload route was approved (r9).
- Obsolete Eval machinery is deleted, because nothing has shipped (r10). The
  repository-wide vocabulary scan was withdrawn as overkill. The served
  OpenAPI assertion and review enforce naming (r11).

## Revisions and deviations

- Revisions 1–11 are recorded in the spec. TASK-006 was added by r10.
- Owner acceptance replaced an independent PASS for TASK-001 to TASK-005. The
  earlier FIX_REQUIRED verdicts were remediated, and their task records hold
  the evidence.
- No final change review was run. TASK-005 recorded `mise run gate` exit 0 on
  its candidate. TASK-006 ran focused lanes only. Full `test:sql` and the full
  journey suites were not rerun after TASK-006.
- External example README quickstarts were not run live. Brand SVG renames
  were not visually inspected.

## Acceptance closure

| Criterion | Evidence |
| --- | --- |
| AC-001–002 contract and engine | `wyrd-spec` and `vala-verify` unit suites; registration and served-schema tests |
| AC-003–004 Agent, extraction, code, isolation | Hosted runtime tests; `hosted_tools_need_the_tool_and_domain_grants`, `delegated_tools_keep_the_narrower_authority`, `forbidden_code_access_cannot_pass_dependent_work` |
| AC-005–006 SDK journeys, negatives | `verifier_building_blocks` and `verify_in_real_time` journeys in Rust, Python, and TypeScript, including replay/conflict and cross-tenant resume refusal |
| AC-007 scheduled drift | `scheduled_drift_alerts_operator::*`, `unfitted_baseline_is_skipped_on_schedule`, `pg_operator_delivery` |
| AC-008 examples | `check:examples`; the three external repositories' test suites |
| AC-009, AC-014 consumers and generated contracts | `pg_openapi_contract`, `codegen:check`, MCP runtime tests, `py:typecheck`, `ts:typecheck`, `docs:check` |
| AC-010 gates | `mise run gate` (TASK-005), and focused lanes plus `check:deps` and `check:tenant-isolation` (TASK-006) |
| AC-011–013, AC-015–016 Eval retirement | `retired_*` refusal tests in CLI, permission, SQL, MCP, and the three SDKs; `pg_migration::retired_verification_namespace_upgrades` |

## Links

- Architecture: [Wyrd design: Verifier](../../../architecture/wyrd-design.md)
- Docs: [Verify](../../../docs/src/content/docs/build-and-verify/verify),
  [Extraction output](../../../docs/src/content/docs/reference/extraction-output.md),
  [Runtime limits](../../../docs/src/content/docs/reference/verification-runtime-limits.md)
- Example: [`examples/support-desk`](../../../examples/support-desk)
- Owners: [`wyrd_spec::verification`](../../../crates/wyrd-spec/src/verification),
  [`vala-verify`](../../../crates/vala/vala-verify),
  [server verification](../../../crates/wyrd/wyrd-server/src/verification),
  [client](../../../crates/shared/wyrd-client/src/verify)
- Journeys: [Rust](../../../sdks/wyrd-sdk-rust/tests/integration/verifier_building_blocks.rs),
  [Python](../../../sdks/wyrd-sdk-python/tests/integration/test_verifier_building_blocks.py),
  [TypeScript](../../../sdks/wyrd-sdk-ts/wyrd/tests/integration/verifier-building-blocks.test.ts),
  [served OpenAPI](../../../crates/wyrd/wyrd-server/tests/integration/pg_openapi_contract.rs)
