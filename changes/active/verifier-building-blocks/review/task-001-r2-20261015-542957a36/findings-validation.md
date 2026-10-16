# Independent findings validation — TASK-001 cumulative R2

**Recommendation: FIX_REQUIRED.** One new bounded finding is confirmed: **FIND-TASK-001-7**. Prior findings **1–6 remain CLOSED**. No Critical issue, live authorization bypass, new schema profile, or specification revision is established.

## Review Findings

### Critical

None.

### Important

**FIND-TASK-001-7 — A declared literal-pattern field is refused as missing.**

- **Discovery / disposition / classification:** INV-R2-001 / CONFIRMED / INCORRECT.
- **Violated obligation:** REQ-003 and original Task 001 Scenario 2 require known-missing detection while preserving usable known and unknown shapes. This is reference-authoring behavior, independent of FIND-3's check-Agent Verdict compatibility obligation.
- **Exact location:** `crates/wyrd-spec/src/card/blocks/schema_path.rs:93–94`, in `Walk::admits`; consumers at `card/blocks/validate.rs:692` and `:738`.
- **Evidence:** for schema `{"type":"object","patternProperties":{"^score$":{"type":"number"}},"additionalProperties":false}`, `score` is a declared field and `{"score":1}` conforms. `admits` finds no `properties.score`, does not exclude object type, then returns false solely from `additionalProperties:false`. It never consults `patternProperties`. The existing `literal_name` at `schema_path.rs:280` already recognizes precisely this bounded declaration in the sibling `fits_object` consumer at `:202–218`.
- **Observable consequence:** public staged `VerifierSpec::validate()` refuses a valid JSON-input reference or preparation-output reference with rule `missing_field`. This blocks pure authoring now; it does not depend on a future provider, engine or mounted server route.
- **Decision-complete minimal correction:** change the field-presence decision once in the existing bounded `Walk::admits`, reusing `literal_name` to recognize an exact anchored declaration matching the selected field before concluding that a closed object's field is absent. Traverse that declaration's remaining selectors using the same root and retained work budget. Preserve named-property traversal, ordinary known-missing refusal, local refs, unknown/budget fallback, action output freedom, and the distinct Verdict-fit refusal behavior. Keep this as bounded structural evidence; do not introduce arbitrary regex matching/inclusion, a schema validator, a dependency, a public authoring profile, keyword bans, per-caller guards, or weaker missing-field checks.
- **Focused closure proof:** extend existing pure public-validation fixtures to accept the exact field from a JSON slot and from an action Agent output, through inline and supplied-resolver production. Include a nested exact-name selector, and a missing sibling under the literal-only closed schema that still yields `missing_field`. Retain cyclic/local-ref/finite-recursive selector and Verdict-fit cases. Reuse the existing fixture and owner tests; no provider/server/journey harness or generic schema theorem suite is warranted. The existing exact `card::blocks::validate::tests::verifier_spec_refuses_invalid_graphs_and_references` and `card::blocks::validate::tests::verifier_spec_bounds_cyclic_schema_references` targets are available for extension. Run whichever targets are actually changed using exact nextest selection through mise, plus only the narrow affected owner verification. This review source-proved the new cases; it did not add or execute them.

### Suggestions

None. Optional hardening and unrelated refactors are excluded.

## Immutable subject and review inputs

- Repository: `/home/thorrester/Documents/GitHub/wyrd-verifier-building-blocks`.
- Original base: `08b38f74255d344e1c5ec9a6d278935a26277622`.
- Previous candidate: `eb1681461897ee1eb9fe1fa4e00cb0b435658559`.
- Candidate: `542957a3622461e259aee5f8108d978e18adad1c`.
- Original r6 specification and Task 001 snapshots, both earlier validated ledgers/verdict context, R1/R2 remediation obligations, cumulative 26-file inventory/diff and latest four-file remediation were the review subject. Latest remediation was a locator, not replacement acceptance authority.
- All six current discovery reports and claims-comparison were supplied to this fresh validator. The invariant report contains the sole new proposal; other reports propose no material finding. Their agreement on closures was treated as a hypothesis and checked at source.
- Applied AGENTS, agent rules, task-review instructions, spec-driven authority order, maintainer guidance, active design/doctrine and relevant authorization boundaries. CodeGraph preceded source discovery. Uncommitted docsite/planning changes are excluded. Only this report is written.

The approved task explicitly delivers pure authoring/validation and compile-consumable signatures before live activation. Zero mounted callers therefore does not waive the public authoring obligation. Conversely, no runtime executor, permission/audit dispatcher, sandbox, storage writer, queue or SDK journey is inferred from declarations. Full legacy LLM-judge removal, including offline evaluation, remains Task 005; temporary build-preserving retention is permitted, and scenario simulation is not blanket-removed.

## Per-proposal dispositions

| Proposal or claim | Disposition | Source-based decision |
|---|---|---|
| INV-R2-001: exact literal-pattern fields wrongly refused by selectors | CONFIRMED | Complete `Walk::admits` and `literal_name` bodies establish the failure. Public JSON-slot and inline/resolved action-output producers reach its two callers without an earlier refusal. Retained as FIND-7. |
| Behavior/security generic REQ-003 PASS | REVISED, narrowly | Grammar, position, graph and recursion claims stand. Their generic selector assessment does not cover this concrete pattern-declared field. Neither report provides a contrary successful source trace. Only this field-presence obligation is reopened. |
| Current closure of FIND-1/2/3/4/5/6 | CONFIRMED | Independent owner/source checks below preserve the six IDs as closed. The new selector defect is neither unbounded recursion nor Verdict compatibility and must not reopen those IDs. |
| No additional system-resilience finding | CONFIRMED within staged scope | Vala seams retain data-versus-Verdict, sources, deadline/drop and error distinctions; no new production executor or lifecycle owner is present. Gateway's Tool arm is a disjoint exhaustive-match adaptation. |
| Standards' import-placement/qualified-type notes | REJECTED as blocking proposals | Report itself marks them nonblocking; no behavioral, contract, security or durability consequence is established. No remediation finding is created. |
| Generic regex/schema inclusion, stronger numeric/string constraints, keyword prohibitions or new public schema profile | REJECTED | Unnecessary for the concrete named declaration, expressly outside the selected bounded comparison, and not authorized by the task. Existing literal-name recognition supplies the minimal correction. |
| Extra focused discovery follow-up | REJECTED as unnecessary | There is one concrete counterexample and a complete source path, not conflicting traced behavior. Independent validation resolved its producer, owner, consumers and authority. Repeated schema work shares an identified bounded owner; there is no remaining unexplored root-cause question requiring another agent. |

## Producer-to-consumer and existing-alternative validation

| Boundary | Actual source path and relevant complete bodies checked | Minimum ownership decision |
|---|---|---|
| JSON input producer | `InputSlot::Json` retains authored schema (`blocks/mod.rs:66`); `validate_slot` (`validate.rs:786`) accepts this ordinary object; `Reference::parse` accepts `inputs.answer.score`; `Validator::run` → `validate_requirement` assertion → `validate_reference` input arm → `admits`. | No prior meta-validation or schema transformation eliminates the state. Correct the shared presence decision, not the slot codec or assertion operator. |
| Inline action-output producer | `VerifierSpec::validate` resolves inline Agent/Prompt; `validate_step_shape` → `validate_agent` validates placeholders/tools/workspace. Action position skips the check-Verdict restriction. `output_schema` (`:756–769`) returns its Prompt JsonSchema unchanged; own-step/whole checks and downstream consumers then enter `validate_reference` output arm. | Keep action output arbitrary and Prompt-derived. Do not impose Verdict or a schema profile on action Agents. |
| Referenced/resolved action producer | `validate_with` supplies the same `ResolvedAgent` used by both validation and `output_schema`; `Prompt::check_schema` expressly traverses `patternProperties` (`skald-spec/prompt.rs:283–291`). Existing `review()` action can retain outline/workspace bindings while declaring this response. | No hidden loader or provider rejects the exact object. Resolver mode changes schema production, not the shared selector semantics. |
| All sibling selector consumers | `validate_reference` is called from `when`, extract, Agent input/workspace, code input, assertions and trace checks. Its only two `admits` calls are JSON slots and step outputs. Whole/step requirements share it; graph/visibility checks remain independent. Extract returns generated schemas; code has unknown schema. | One owner correction reaches all affected readers. No repeated guards or new runtime evaluator belongs in individual checks/executors. |
| Sibling schema analysis | Complete `Walk::{spend,resolve,admits,fits,fits_object,values}` and `types/json_type/literal_name/required/excludes_type`; typed Verdict/Citation/Locator and generated schema source. | `literal_name` is the existing bounded recognition source. `fits_object` already uses it. Keep selector uncertainty admitting and unproved Verdict fit refusing. No second schema mechanism is needed. |
| Durable grant producer/consumers | `BuiltinTool`, `PermissionScope::{validate,covers}`, strict `PermissionWire::try_from`, `Permission::{validate,covers,subsumes}`, generic `PermissionSet::intersection`, accepted-authority encoding and full gateway `invocable`. | Existing three-axis permissions remain authoritative. No duplicate tool authorization service or implied data grant is introduced. |
| Provenance producer/consumers | Full Vala block seam, `ActionOutput`, shared `ToolCallRecord`, `StepResult`, result serialization and handoff fixtures. | One record carries typed optional output digest and mandatory returned bytes, including honest no-output. A production projector is later work. |

The exact no-Agent reproduction is:

```json
{
  "inputs": {"answer": {"type": "json", "schema": {
    "type": "object",
    "patternProperties": {"^score$": {"type": "number"}},
    "additionalProperties": false
  }}},
  "requirements": [{"id": "positive", "statement": "Score is positive",
    "check": {"kind": "assert", "path": "inputs.answer.score",
      "operator": "greater_than", "expected": 0}}]
}
```

No required step or `decide` field is missing: those defaults are supplied by the authoring contract. The assertion operator is a supported comparison. With no steps or Agent, there is no resolver, graph dependency, media, workspace or Verdict prerequisite that could make this path dormant.

## Prior-finding closure table

| Stable finding | Independent source evidence at this candidate | Disposition |
|---|---|---|
| FIND-TASK-001-1 — tool permissions | `auth/permission_scope.rs:54,153–163` supplies exact Tool identity/equality and All coverage. Runtime `permission.rs:342–372` requires Tool/Invoke for exact scope; strict wire conversion invokes validation. Constructors `:789–809` express exact/global. Generic intersection `:980–1033` chooses the narrower scope, retains axis checks and gateway-only conditional handling. `request_context.rs:78–110` preserves effective principal/chain without credentials; gateway `batches.rs:169–195` retains its resource/action filter and ignores Tool scope. | CLOSED; no widening or alternate authority carrier found. |
| FIND-TASK-001-2 — catalog | `card/agent.rs:61–139` supplies one closed ten-name vocabulary, ALL and exact lookup; `validate_agent:533–542` uses it. Filesystem/workspace and tool-less check guards remain. The same enum types exact permissions and provenance; all-builtins/unknown-name tests are present. | CLOSED. |
| FIND-TASK-001-3 — bounded Verdict fit | Shared guard at `validate.rs:568` compares generated typed Verdict. `fits:130–136` now falls through to same-node facts after insufficient allOf parts; `:153–158` infers types from finite values; `fits_object:202–218` checks exact literal-name additions while permitting same-field refinements. Existing test at `validate.rs:1456` retains typed/local-ref/annotation positives and prior wrong-type/outcome/citation/extra-field negatives, adding the specified R2 cases. Step-check and resolver paths still share the guard. | CLOSED within the approved bounded structural comparison. FIND-7 concerns `admits`, not a new inclusion rule. |
| FIND-TASK-001-4 — provenance | `execution.rs:315–336` retains BuiltinTool, arguments, optional Sha256Hex, returned_bytes and error with bounded/no-output semantics. `ActionOutput` and `StepResult` share the record; result serde and pure action-to-step handoff fixtures preserve it. | CLOSED; no missing runtime writer is asserted. |
| FIND-TASK-001-5 — recursion | Entry allocates one Walk; each `admits/fits/values` recursive visit and each resolve hop spends the same retained finite budget (`schema_path.rs:14,49–81,109–117,234–252`). Selector exhaustion returns true/unknown; fit exhaustion refuses. Public cyclic input/output and finite-tree known-missing tests remain (`validate.rs:1586`). | CLOSED; the new presence defect does not defeat the bound. |
| FIND-TASK-001-6 — rustdoc | Actual Reference Display docs at `reference.rs:233–238` and MediaFormat Display docs at `media.rs:303–308` now document writer failure under Errors with unchanged bodies. Previously repaired helper/codec/trait/test documentation is retained; latest schema helpers are documented. The standards and maintainer cumulative inventories identify no outstanding mandatory changed-item omission, and direct inspection of the changed residuals agrees. | CLOSED; no style-only round or new checker is required. |

## Ponytail correction decision

Deleting the selector walker would lose required known-missing validation. Deleting pattern declarations or rejecting this valid schema would change authoring behavior. Existing Prompt/schema syntax checking supplies valid schemas but does not answer selector presence; existing Card reference loaders and runtime JSONPath are different owners. The current `literal_name` and `Walk` already provide the needed recognition and bounded traversal. Reusing them at the shared source is the first complete option; a new dependency, regex engine, schema profile or downstream guard has no justification.

The selected correction is an ordinary bounded implementation fix within REQ-003. It changes no public field, architecture, security policy, persistent format, cross-service lifecycle or expensive-to-reverse product decision. **SPEC_REVISION_REQUIRED is not warranted.**

## Open Questions

None affecting this bounded correction or the stable prior closures.

## Verification Notes

Inspected current candidate logs: **869 wyrd-spec tests passed, zero skipped**; the exact existing Verdict regression selection ran **1 test and passed**; narrow owner all-features Clippy finished successfully; codegen checks completed successfully. Subject records unchanged tracked generated outputs and cumulative whitespace proof. The invariant reviewer separately records **1139/1139** four-library tests passing. These are identified supplied executions, not fresh runs by this validator. Existing selector tests omit the literal-pattern input/output state; green tests do not disprove the source-traced defect.

No source/test mutation, broad suite, live experiment, schema dependency or new harness was performed. Candidate/source immutability was checked during validation. Only the deduplicated new FIND-7 remains open; IDs 1–6 must remain closed in the next remediation packet.

Skipped independent test reruns, live provider/server/SQL/sandbox checks and SDK journeys, as deferred by this staged task. Risk: valid exact literal-pattern references remain wrongly rejected until FIND-7 closes; future runtime isolation, authorization and integrated journeys still need their assigned proof.
