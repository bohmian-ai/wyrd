# Security/RBAC and contract trust-boundary review

**Result: PASS.** No material proposed finding remains in this domain. The prior security-related findings are source-closed, including the residual bounded Verdict-schema cases. This is acceptance of the staged Task 001 contracts, not certification of the later hosted execution or authorization implementation.

## Subject, authority and coverage

- Original base: `08b38f74255d344e1c5ec9a6d278935a26277622`.
- Immutable cumulative candidate: `542957a3622461e259aee5f8108d978e18adad1c`; HEAD matched and the candidate-relative `crates/` diff was empty during inspection.
- Reviewed the full 26-file original-base-to-candidate inventory and relevant cumulative source, using the latest four-file remediation diff only as a locator. Reviewed original Task 001, approved specification r6, initial and R1 verdicts/validated ledgers, and R1/R2 remediation obligations. No current sibling discovery report was read.
- Authority: supplied AGENTS.md and repository agent rules; active design's pure contract, server ownership, identity/delegation and reference boundaries; doctrine; security posture's trust, permission attenuation and accepted-authority rules; reference router and applicable spec-driven-development, maintainer-style and agent-harness guidance. The approved task explicitly defers mounted routes, runtime executors, sandbox, persistence and integrated journeys.
- Used CodeGraph before code discovery. Expanded through role issuance, permission checking, accepted authority and gateway consumers. Only this assigned report is written. Source, tasks and prior reports remain unchanged; unrelated docsite/planning changes are excluded.

| Boundary / obligation | Producer → owner → consumers inspected | Assessment |
|---|---|---|
| REQ-008 typed tool authority | `card/agent.rs:61-139` BuiltinTool → `auth/permission_scope.rs:45-187` → runtime `PermissionWire`, `Permission::validate`, coverage/subsumption and PermissionSet | PASS. Closed exact catalog and ordinary three-axis permissions; no executable/path/prefix scope or implied domain authority. |
| Delegation and accepted authority | Runtime `permission.rs:980-1033` intersection → Principal effective permissions → `request_context.rs:78-110` AcceptedAuthority; existing issuance `issuance.rs:775-806` and RbacCheck | PASS. Existing owner decodes role grants through validated Permission, intersects without widening, and serializes exact tool scope with subject/chain. |
| Live sibling gateway safety | Entire `gateway/batches.rs::invocable` at :169-195; resource/action filter and added Tool scope arm | PASS. Tool is ignored as a gateway object scope. Existing All/deployment/model reach remains unchanged. The authoritative invocation decision is still separate from listing pruning. |
| Catalog and Agent position | Agent tool_names → `validate.rs::validate_agent` at :492-581; step and requirement dispatch, inline/resolver paths, workspace and Prompt placeholder/media checks | PASS. All ten approved tools admitted in step position; unknown names refused; checks have no tools/workspace; filesystem tools require a workspace and workspace requires fs/code. |
| Derived schema → typed judgment | Shared Prompt syntax checking → resolved Agent → same check-position guard at `validate.rs:568` → `schema_path.rs::Walk::{fits,fits_object,values}` → generated Verdict/Citation/Locator in `execution.rs:339-391` | PASS within the expressly bounded structural contract. Latest changes preserve sibling allOf evidence, infer primitive shape from finite values and account for exact literal-name pattern properties. |
| Selector analysis and bounded recursion | JSON slots / resolved Agent outputs / generated ExtractOutput schemas → `validate_reference`, `output_schema` → shared Walk::admits/resolve | PASS. One finite budget spans branch recursion and ref hops; unresolved/indeterminate selector shape admits, while unproven Verdict fit refuses. No remote schema fetching or second validator/runtime is added. |
| Input/reference and file authoring | Full reference parser and serde; SlotName/ExecutionId additions; block declaration codecs/validator; shared Visit implementation; media vocabulary/native-kind accessor and extraction shapes | PASS for contracts. Typed roots/positions, global IDs, DAG validation and virtual path segment refusal remain pure; no file open, URL fetch, archive extraction or command execution occurs here. |
| Execution/provenance handoff | `verification/execution.rs` request/result/error/provenance types → Vala `blocks.rs` BlockCall/CallInput/ActionOutput/CheckOutput/BlockError and tests | PASS. Request has no caller-controlled tenant/principal field; source-slot identity, position and deadline are explicit. One ToolCallRecord carries typed digest/returned-byte metadata into StepResult. |
| Cumulative ancillary changes | Shared DAG planner/operator delegate, exports, refs visitor, error-message access, Prompt/media accessor, schema/extraction types | No introduced SQL, process execution, deserialization of executable objects, credentials, log sink, dependency/lockfile, deployment configuration or durable writer. Existing owners are extended. |

## Prior finding closure from current source

| Stable finding | Source-backed disposition |
|---|---|
| FIND-TASK-001-1 — tool permission contract | CLOSED. `Resource::Tool`, `PermissionScope::Tool(BuiltinTool)`, `Permission::tool_invoke` and `tool_invoke_any` express both approved forms. Exact scope validation accepts only Tool/Invoke; mismatched resource/action and unknown tool decode fail. Scope coverage at `permission_scope.rs:153-163` is equality for Tool; All covers exact. Existing resource/action axes retain wildcard meaning. `PermissionSet::intersection` takes the narrower scope and returns no meet for distinct names, preserving generic role and accepted-authority paths. No new default grants or auth service. |
| FIND-TASK-001-2 — catalog | CLOSED. The one ten-member enum/ALL/from_name catalog is reused by exact permissions, declaration validation and provenance. Previously omitted cards.list/cards.get_by_ref/bifrost.list_tables/bifrost.describe_table are present. `validate_agent` does not retain an incomplete private list or permissive fallback. |
| FIND-TASK-001-3 — bounded Verdict fit | CLOSED. `schema_path.rs:130-136` returns when an allOf part suffices and otherwise checks the node's own keywords, preserving type/enum siblings. `:153-158` reuses finite values to infer shape when type is absent, accepting string const/enum and rejecting numeric/mixed values against reason. `fits_object` at :202-219 treats literal pattern names as properties: ^score$ is incompatible with closed Verdict, while ^reason$ refining its declared property adds no field. Generated typed schemas remain authoritative. Existing wrong outcomes/types, missing fields, extra fields, malformed citation/locator and cyclic check schemas remain refused. |
| FIND-TASK-001-4 — tool output provenance | CLOSED. `execution.rs:315-336` records BuiltinTool, arguments, optional Sha256Hex, returned_bytes and optional error. ActionOutput and StepResult share this record. No-output calls can honestly omit digest with zero returned bytes. The pure handoff/result serialization proof is appropriate; there is no production projector in this task. |
| FIND-TASK-001-5 — schema recursion | CLOSED. `schema_path.rs:14,49-64,68-83,109-117,234-252` applies one budget across traversal, values analysis and local refs. The earlier combinator/ref reset loop cannot recurse indefinitely. Input and resolved-output consumers both use this owner. Unknown-selector semantics and ordinary finite known-missing refusal are retained. |
| FIND-TASK-001-6 — rustdoc | The two residual formatter omissions are CLOSED on direct inspection: Reference fmt at `reference.rs:235-238` and MediaFormat fmt at `media.rs:304-307` now document formatter-write failure under Errors; bodies are unchanged. This domain pass does not replace the separate cumulative repository documentation audit. |

The complete caller trace for finding 3 is authored/native or portable Prompt → existing schema derivation/syntax validation → inline or resolver-provided ResolvedAgent → step-check or whole-Verifier-check dispatch → one validate_agent guard → bounded comparison against generated typed Verdict → later CheckExecutor result contract. Action Agents skip the Verdict guard and keep arbitrary structured output. No duplicate guard belongs in individual executors.

The remaining unsupported arbitrary regex inclusion, numeric bounds and string-format strength are explicitly outside the prior selected correction. They are not new findings or justification for a generic proof engine, blanket keyword ban or public schema profile. Runtime typed decoding and citation checks remain later owners' obligations. Likewise, unresolved referenced Agents are intentionally left to the supplied resolver/registration boundary; no newly mounted unauthenticated resolver path is introduced here.

## Security Audit

### Critical

- None established.

### High

- None established. No current live authorization bypass or remotely exposed schema exploit is claimed.

### Medium

- None remaining within the approved Task 001 scope.

### Low / Defense In Depth

- No optional hardening proposal. No open question blocks this bounded review.

### Positive Controls

- Tool identity is a closed typed built-in name, and each external grant decode runs the existing strict PermissionWire and Permission validation.
- Global and exact tool invocation authority remain independent from Cards, Bifrost and Verifier authority. Existing wildcard grants retain their documented coverage.
- Delegation uses existing intersection; accepted authority retains exact scope, verified subject and delegation history without storing bearer material.
- Schema recursion/ref work is bounded across the complete shared traversal; uncertain selectors and uncertain judgment fit receive their distinct required outcomes.
- Check Agents remain tool-less/workspace-free, while typed closed Verdict/Citation/Locator values remain the judgment boundary.
- Tool outputs are represented by bounded-output digest and byte count rather than embedding raw output in the call record.

## Verification and limits

Fresh focused execution passed **6 tests, 0 failed** (1,034 intentionally unselected), using:

```sh
mise exec -- cargo nextest run --locked -p wyrd-spec -p wyrd-runtime -p vala-eval --lib -E 'test(=card::blocks::validate::tests::verifier_spec_requires_check_agents_to_produce_a_verdict) | test(=card::blocks::validate::tests::verifier_spec_bounds_cyclic_schema_references) | test(=card::blocks::validate::tests::verifier_spec_admits_every_builtin_step_tool) | test(=permission::tests::tool_invoke_is_exact_name_scoped_and_independent) | test(=request_context::tests::accepted_authority_keeps_exact_tool_scope) | test(=blocks::tests::action_tool_calls_carry_into_step_results)'
```

Inspected test bodies establish what these checks prove: exact/global permission serde and wrong-name/axis refusals; domain independence and exact intersections; delegated accepted-authority round-trip; all built-ins; schema positives/negatives plus both positions and resolver mode; cyclic input/output traversal; provenance handoff. The latest regression fixture includes all requested concrete R2 cases. Subject/remediation verification records broader owner tests, Clippy, formatting and codegen evidence; those claims are not substituted for source proof or described as fresh reruns here.

The new blocks remain staged: the live Card Spec/verification service has not switched to the replacement, and the only ActionExecutor implementation found is the local Echo fixture. There is no hosted tool, runtime projector, sandbox, provider call, queue or persistent state introduced by these contracts. Legacy LLM-judge removal, including offline evaluation, remains assigned to Task 005; no indefinite preservation or premature removal is required here.

Skipped live server/SQL/provider/sandbox fault tests, full suites and SDK journeys; later runtime authorization, isolation and typed-output/citation enforcement still require their assigned integrated proof.
