# TASK-001 round 4 — Domain review: public contracts and schemas

## Subject and reviewed boundary

- Repository: `/Users/stevenforrester/Documents/GitHub/verified-change-contract`
- Immutable subject: base `5293546f33b3a5fd9de529098e23ea70d472c412` to candidate `c8bb490ad814c0c7770cac33ed7779897ff776e4`
- Approved authority: `changes/active/verified-change-contract/spec.md`, revision 32
- Original task: `changes/active/verified-change-contract/tasks/TASK-001-verifier-contract-and-registration.md`
- Remediation authority: all prior verdicts, validated ledgers, and remediation tasks under `review/TASK-001-r1/`, `review/TASK-001-r2/`, and `review/TASK-001-r3-retry1/`
- Boundary reviewed: the Verifier/binding/Trigger/Operator authored contract; Card-kind, schema, OpenAPI, CLI, and public Rust vocabulary; retired Eval/Drift registration and pull contracts; and closure of `FIND-TASK-001-5`.

The candidate was `c8bb490ad` at review start and end. `.codegraph/` is absent, so discovery used `rg` and direct source reads.

## Authority and source coverage

| Boundary | Governing authority | Source and artifact evidence | Result |
|---|---|---|---|
| One registrable Verifier Card with exactly the Drift and Eval implementations | REQ-045/046/047/056/109/110/111; INV-006/012/014; `AGENTS.md` section 2; `architecture/wyrd-design.md`; `architecture/wyrd-doctrine.mdx`; vocabulary, Eval, and Drift references | `wyrd-spec/src/envelope.rs`; `card/{verifier,drift,eval}.rs`; generated `card_kind.json`, `verifier_spec.json`, and `verifier_implementation.json`. The generated catalog has 15 values and the implementation union has only `drift` and `eval`. | PASS |
| Binding, Trigger, and Operator public shape | REQ-090/091/093/094/102/103/143; INV-001/013; AC-004/018 | `card/{agent,service,trigger,operator,verifier}.rs`; generated `verification_binding.json`, `trigger_spec.json`, `trigger_activation.json`, and `operator_spec.json`. Bindings require `verifier` and `runs_on`; the closed Trigger branches reject unknown fields. | PASS |
| JSON Schema and OpenAPI projection | REQ-045/109/114; AC-021; generated artifacts must be source-owned | Existing generators and schema owners; `wyrd-server/src/http/openapi.rs`; `VerifierImplementation::{schema,schemas}`; generated `openapi.yaml`; recursive `verifier_contract_component_references_all_resolve` test. The five roots and the Drift dependency closure, including `DriftSignal`, are defined. | PASS |
| CLI and public Rust vocabulary | REQ-103/109/114; AC-021; TASK-001 scenarios 3 and 4 | `wyrd-cli/src/{error.rs,eval/run.rs}`; `vala-eval/src/{lib.rs,orchestrator/state.rs,results.rs}`; `wyrd-runtime/src/permission.rs`; `wyrd-spec/src/card/{eval,source}.rs` and `src/vala/eval/{mod,spec}.rs`. Accepted inputs and public descriptions consistently name an eval-backed or drift-backed Verifier Card. | PASS |
| Retired contract closure | REQ-116/120/144; AC-022 | Deleted Eval pull route/client/wire types, retired table owners and SQL API, alert-router crate and schemas; current source/build searches find no live `/v1/eval/runs`, `EvalRunOpen*`, `LeaseToken`, `vala.eval.runs`, `vala.eval.assertions`, `vala.drift_alerts`, or `alert_router` owner. Historical create/drop migrations remain intentionally. | PASS |
| Final remediation preserved APIs and behavior | R3 remediation constraints and non-goals | `9d7b62662..c8bb490ad` changes only rustdoc/diagnostic descriptions, two built-in-count docs, and review evidence. `load_eval_card`, `eval_ref`, `Resource::{Evals,Drift}`, stable errors, schema shapes, and runtime code are unchanged. | PASS |

## Prior-finding closure

| Finding | Contract-domain result | Evidence |
|---|---|---|
| `FIND-TASK-001-5` | CLOSED | The six final files contain no stale `eval card`, `drift card`, or `drift/eval card` wording. The replacements accurately describe Verifier implementations while retaining the existing API, field, resource, and error names. The earlier four-file remediation remains correct. |
| `FIND-TASK-001-14` | CLOSED | The source-owned OpenAPI registration still includes `TriggerActivation`, `VerificationBinding`, and `VerifierImplementation`; `VerifierImplementation::schemas` still forwards the Drift dependency closure; all required roots are present in `openapi.yaml`. |
| `FIND-TASK-001-15` | CLOSED for contract documentation | The canonical table owner now says six in both locations, matching `BUILTIN_TABLES: [BuiltinTableDefinition; 6]`; no contract or helper changed. |
| `FIND-TASK-001-17` | CLOSED as evidence | The three named proof groups are recorded through `mise exec -- cargo nextest run --locked`, with exact selectors and no-retry results. Editing the earlier command cells is not a contract defect: the addendum explicitly records that the original invocations used raw Cargo and that the displayed commands are later reruns, while Git history preserves the original record. |

## Broader live-surface assessment

The cumulative source still contains three categories of wording that are not
TASK-001 findings: test-only assertion labels such as `expect("eval card")`,
TASK-002-owned observation-record documentation for the still-present
`eval_ref`/`drift_ref` migration, and UI/brand material previously rejected or
deferred by the validated ledgers. None is an authored Card contract, generated
Card/schema projection, CLI instruction, or live TASK-001 registration surface.
Expanding this remediation into those owners would violate its explicit
non-goals rather than close a reachable TASK-001 contract gap.

The unreferenced `DriftSpec` OpenAPI component also remains the previously
accepted source-owned by-product of forwarding the inline Drift branch's
dependency closure. It exposes no route, accepted Card kind, or alternate
contract and is explicitly outside the final remediation.

## Verification evidence and limits

- Static inspection confirmed the final source diff is descriptive only and the generated Card-kind and Verifier schemas retain the approved closed shapes.
- The candidate records green `fmt`, `lints`, `docs:check`, `codegen:check`, and `git diff --check`, plus the focused CLI test and the exact pinned registration, restart, and Oracle proof reruns.
- Earlier cumulative evidence records green contract, registration, schema, SDK, documentation, and Bifrost lanes; this review did not rerun multi-hour or Postgres-backed lanes.
- The recursive OpenAPI test and generated document were inspected directly. `codegen:check` is recorded green; no generated artifact changed in the final remediation.
- Python and TypeScript runtime lanes were not rerun for the final three commits because no SDK source, public projection, or generated language artifact changed.

## Findings

Explicitly empty. No material contract, schema, generated-surface, or public-vocabulary finding remains.

## Overall result

**PASS.** `FIND-TASK-001-5` is fully closed, the final wording edits preserve
the retained APIs and stable errors, the Verifier/schema/OpenAPI contract
remains closed, and the retired registration and pull contracts have no live
TASK-001 owner.
