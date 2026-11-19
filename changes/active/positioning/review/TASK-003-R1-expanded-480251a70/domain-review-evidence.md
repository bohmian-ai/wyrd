# Evidence boundary review — expanded TASK-003 remediation

Result: **FAIL** for the explicit Rust tutorial setup obligation below. The changed trace marker, evidence completeness, native judgments, and retained authority paths pass this scoped inspection.

## Subject and authority

Reviewed immutable remediation `634e19cbc517293ed2458f30ce73401714f3447d` → `480251a70f799b8581ca5a4be77983d6f345d5e3`, with original task and cumulative context available. HEAD remained the candidate. Approved working positioning revision 6 (`7f9bdb2a67e6b113b02a58526925b75f91028112`), service identity revision 17 (`6561fb1a747687f56a051edfeff23cc1f0e2375f`), and original task (`6846032793910170e76c0cc0ec9de2f50cb6ad6f`) hashes match the scope pins.

Authority coverage:

| Boundary | Authority read and applied |
| --- | --- |
| Marker and SDK evidence model | Positioning REQ-017 / AC-016; committed `architecture/wyrd-design.md` authoring section around 2426; root and Python/TypeScript AGENTS rules |
| Managed export and explicit flush | Positioning REQ-017; service identity approved telemetry setup/drain clauses; committed design telemetry sections around 718 and 826 |
| Native evidence sufficiency and retained attribution | Committed design trace contract around 1574; Bifrost durability/visibility section around 594; security posture principles on caller-derived tenant and authorization |
| Remediation scope and correction burden | Task-review skill; spec-driven workflow authority; maintainer style; prior evidence review at `../TASK-003-first-634e19cbc/domain-review-evidence.md` |

The approved revisions authorize these SDK additions; original revision-5 no-runtime wording does not reject them. No CodeGraph directory exists. No source was changed. A working-tree edit to `examples/refund-agent/refund_agent.py` appeared during inspection; the committed candidate was read with `git show` and that dirty edit supplies no acceptance evidence. The orchestrator was informed so it can resolve the review-wide cleanliness condition.

## Coverage matrix

| Obligation | Implementation evidence | Verification evidence | Result |
| --- | --- | --- | --- |
| Same-name Trace marker becomes a canonical trace slot as model field or mapping value | Rust `authoring/verifier.rs:101–166` marks an inline string schema and consumes its `x-wyrd-slot` in the shared `InputSlot::json`; `fields` resolves field definitions before the same conversion. Python `verify/__init__.py:65–85` emits that marker through Pydantic or a direct class schema; `src/verify/definitions.rs:132` and `:394` route both forms through shared `InputSlot`. TypeScript `definitions.ts:164–178`, `inputSlot` and `inputSlots` emit marked schemas through native `slot_json` (`native/src/authoring.rs:1087`), again into the shared lowerer. | Committed Rust `trace_field_declares_a_trace_slot`, Python `test_trace_fields_declare_trace_slots`, TS `a Trace property declares a trace slot and its value is the trace ID` assert model and mapping canonical output plus unchanged ordinary field schema. These were inspected, not rerun. | PASS |
| Schema and instance agree without a wire/schema fork | Rust transparent `Trace(String)`, Python string schema/serialization, and TS identity-preserving Zod metadata all yield the bare trace ID. Stored slot is exactly `{"type":"trace"}`. Existing `verify/mod.rs:379–390` parses this string as `TraceId` and builds the original `InputDeclaration::Trace`; malformed IDs still receive the existing input refusal. `wyrd-spec` trace slot/check declarations remain unchanged. | Same committed authoring tests assert string values. Actual refund programs serialize the marked model and supply it to the existing verifier. | PASS |
| Actual finished work proves complete versus skipped, despite equal replies | Each committed refund program records its real lookup/drafting inside a real root span; remembered-order execution omits only lookup. All spans finish before the export barrier. Python/TS configure installs their managed providers, and their state flush awaits provider force-flush before native drain. Rust retains its existing `Telemetry::shutdown`, which calls `provider.force_flush()` (`sdks/wyrd-sdk-rust/src/otel.rs:64–71`). No fabricated span lists or completion labels enter the checks. | Python committed actual-program journey asserts equal replies and exact three-span/two-span timelines; Rust/TS actual-program journeys compare captured complete/skipped/no-trace output. Supplied execution claims remain separate from independent source review. | PASS |
| Missing trace stays inconclusive; native whole-trace authority remains intact | `vala-verify/src/blocks/engine.rs:832–861` requires a trace declaration, returns inconclusive when absent/unreadable, then evaluates the original selector. `CallerTraces::fetch` uses the accepted caller's ordinary query; statement selects exact trace ID without workload/time narrowing; no rows is NotYetLanded, never proof of absence. Neither owner changed in this remediation. | Actual refund output assertions cover no-trace inconclusive and answer-only passed; prior source-backed native absence/authority coverage is carried forward. | PASS |
| Rust guide program uses approved configure-plus-managed-setup and flush sequence | Committed `examples/refund-agent/refund_agent.rs:105–115` declares the state, calls `otel::start_telemetry`, executes work and calls `telemetry.shutdown`; it never calls state configure or state flush. See EV-1. | Actual-program success proves existing export works, but does not demonstrate the explicit AC-016 setup. | FAIL |
| Retained exact attribution, grants, tenant refusal, and scheduled evidence stay valid | Carry forward the prior evidence report's retained Execution.details, exact refs, ordinary caller authority, foreign-tenant refusal, native scheduling and window evidence. Changed investigation/results Rust lines are documentation only. Changed refund model still supplies exactly the same reply and trace ID to the same check and exact binding. Managed exporter uses the same state client, frozen Service/version/environment, workload-root export and existing credential refresh; no new tenant authority is derived from the marker. | Existing Python investigation journey still asserts retained reply, unknown ID 404 and foreign tenant 404. Prior actual-program Rust/TS investigation and schedule proof remains applicable; engine/query/security owners are unchanged. | PASS |

Public marker reexports were traced through Rust authoring → verify, Python public verify imports, and TypeScript verify definitions. The shared lowerer changes authoring only; server trace read, persistent schemas, grants and outcome ownership retain their existing owners. No additional trace-completeness protocol or generalized distributed-trace guarantee is necessary for these finished local workflows.

## Proposed finding

**EV-1 — MISSING: Rust refund program omits the explicitly approved tutorial setup sequence.**

- Obligation: positioning AC-016 says all three agent-workflow programs set up with `configure()` (Rust: configure plus `otel::start_telemetry`) and `flush()`.
- Location: committed `examples/refund-agent/refund_agent.rs:105–115`.
- Evidence and consequence: the Rust program retains the old implicit-default configuration and telemetry-handle barrier. Those produce adequate evidence, but the runnable source does not exercise the configure and no-writer flush workflow specifically added and required by AC-016. The captured-output journey cannot detect their omission because it only checks verdict/output behavior.
- Materiality: this is a direct, bounded acceptance obligation for a published runnable language variant, not a claim that its current native verdicts are unsound. No evidence-integrity or tenant defect was established.
- Smallest correction: use existing `WyrdState::configure(ConfigureOptions::default())` before the managed Rust setup, and exercise the existing state `flush().await` after exporting finished spans. Preserve `Telemetry::shutdown` as the actual Rust provider force-flush: the Rust state does not own this SDK handle and state flush alone cannot export its pending spans. No SDK runtime change, wrapper, new configuration surface or new harness is needed.
- Focused proof: the existing actual Rust refund journey still produces its complete/skipped/no-trace captures with the committed approved setup sequence. Static inspection must additionally establish those required calls are present; do not add a test that merely mirrors source spelling.

## Limits

Static source review only; no Cargo, tests, live provider, benchmarks or broad checks were run by this specialist. Uncommitted example/test edits are excluded. The orchestrator owns verification and the final review-wide subject/cleanliness decision. No optional hardening or new evidence machinery is proposed.
