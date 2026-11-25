# Native decision verdict implementation tasks

Contract: [SPEC-verdict-response-mapping revision 1](../spec.md), approved by
the maintainer on 2026-11-18. Tasks are proposed implementation packets; this
planning pass does not execute them. The [finalized design](../design/notes.md)
and its supplements explain the selected approach.

## Delivery order

| Task | Delivered user outcome | Dependency output consumed |
|---|---|---|
| [TASK-001 — Jev verdict checks](TASK-001-jev-verdict-checks.md) | Author, validate, execute and inspect a Jev check using typed questions and any leaf policy across the three SDKs and existing agent-facing surfaces. | Existing native Jev transport, gateway deployment, authoring graph and result recording. |
| [TASK-002 — Multiple questions](TASK-002-multiple-question-verdicts.md) | Ask multiple questions once and inspect one explained aggregate verdict. | TASK-001's typed authoring, leaf conversion, graph validation and captured result path. |
| [TASK-003 — Gemini Decisions](TASK-003-gemini-decisions.md) | Use that same workflow through native Google Decisions, including its gateway operation and native media input. | TASK-001's provider-response facade and TASK-002's aggregation and SDK policies. |

Each task includes its required SDK projections, validation, capture, generated
contracts, documentation and connected tests. The split follows independently
observable outcomes and actual dependencies, rather than wire/SDK/test layers.
J5–J7 evidence, authoring errors and response failures accompany the task that
introduces the behavior; they are not a later testing task. J8 regressions remain
with each affected owner. The acceptance entries in each task are the live
implementation evidence record; do not create another obligations ledger.

## Verification ownership

Task execution runs the smallest focused tests first, then the affected owner
checks. Add actual test locations and exact commands before each new scenario's
RED run; no new test names or selectors are prescribed by this plan. Provision
repository Postgres with `scripts/postgres/with-test-postgres.sh` and the owning
task's migration/setup recipe, not production credentials. Keep Cargo-backed
commands sequential in the shared checkout.

The approved capability is SDK authoring itself: its tests must actually call
the Prompt/question/transform constructors, as existing authoring journeys do.
Keep file-authored equivalents as checked-in fixture Cards and compare their
meaning through public declarations. Do not replace the constructor journey
with YAML-only registration, construct YAML strings in tests, or parse SDK
results as raw JSON. Exact upstream JSON assertions belong at the external
provider mock or native gateway seam. Use public result types and existing
flush/wait/read controls for recording proof.

At integrated change review, exercise the completed capability with the full
`verify:rust-sdk`, `verify:python-sdk`, `verify:typescript-sdk` and
`test:gateway:journey` lanes. Include the MCP/HTTP/CLI declaration, execution and
inspection proof in their existing owners; gateway administration alone does
not establish the transformed-verdict contract. Include affected served OpenAPI
and runtime MCP catalog checks, codegen, dependency boundaries and docs/example
checks. Reuse already-run equivalent coverage instead of running a broader
aggregate as an additional checklist. No whole-repository `gate` is prescribed.

Gemini Decisions review must explicitly verify REQ-004's provider-pattern
parity, using Jev's native typed contracts and provider path as the reference.
TASK-003 names the existing owned/borrowed wire contracts, validation/body
forwarding, native dispatch and shared transport owners to compare. Neither
functional verdict journeys nor aggregate benchmarks substitute for that source
and contract evidence.

Every implementation review returning PASS on a hot-path change must include
the sanctioned-wait inventory and clean benchmark evidence required by the
specification: full `mise run bench:l1:verification` and
`mise run bench:l1:gateway`, verdict VALID, compared with the newest
same-experiment/same-host records under `benchmarks/history/`. This applies to
task reviews as relevant and the integrated candidate; planning itself runs
neither benchmarks nor runtime tests. No new waits, migrations or dependencies
are authorized. No performance claim follows from native mock responses.
