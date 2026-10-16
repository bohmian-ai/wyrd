# Maintainer review

Result: **FAIL**

Subject: original base `08b38f74255d344e1c5ec9a6d278935a26277622` through immutable candidate `eb1681461897ee1eb9fe1fa4e00cb0b435658559`. HEAD remained the candidate during inspection. This is a cumulative source review, including the original task, specification snapshot, prior verdict/validated ledger and R1 remediation task. No implementation or other current discovery report was used as authority.

## Changed-surface coverage

| Cumulative changed surface | Owning symbols, callers and proof inspected | Maintainer assessment |
| --- | --- | --- |
| `wyrd-spec/src/card/blocks/mod.rs` | All authoring values, defaults, methods and DriftCheck custom codecs; validation and reference visitor consumers | Declarative types retain no IO or clients; action/check positions and flattened drift behavior are explicit. Drift codec rustdoc is repaired. |
| `card/blocks/reference.rs` | Full Reference parser, Tokens cursor, formatting, serde, schema and all three parser tests; Validator reference consumer | Cursor earns state ownership; root parsing owns the new scope language. One residual documentation obligation below. |
| `card/blocks/validate.rs` | Full Validator and VerifierSpec methods, all helpers and all tests; inline/resolver Agent paths, schedule mappings, dependencies, ancestors and known-output consumers | The pass retains its resolver and lookups. No zero-sized utility owner or duplicate orchestrator. Fallible helper documentation is repaired. |
| `card/blocks/schema_path.rs` | Entire Walk, admits/fits, reference resolution, type/value/object checks and selector test; both input and output selector consumers and check-Agent schema consumer | A private finite analysis budget is owned once per walk. Structural compatibility remains separate from runtime decoding. No replacement schema-validation platform is introduced. |
| `card/blocks/extract.rs` | Every extraction output variant/field, schema_for, three canonical/negative/schema tests, output_schema consumer | One typed output source drives per-format schema projection; no parallel hand-authored extraction schema. |
| `card/agent.rs`, `auth/permission_scope.rs` | BuiltinTool catalog and accessors; scope decoding, validate/covers; AgentSpec.tool_names and all BuiltinTool consumers | One typed closed vocabulary is shared by declaration validation, exact authorization scope and tool provenance. Existing general Agent tools remain strings for runtime registration. |
| `wyrd-runtime/src/permission.rs` | New resource, labels/parser, constructors, full Permission::validate, PermissionWire decode, covers/subsumes and PermissionSet intersection machinery; new tool-scope test | Existing authorization owner is extended; no second permission store/checker or token representation. |
| `wyrd-runtime/src/request_context.rs` | New accepted-authority round-trip test; AcceptedAuthority::new, Principal fields, existing intersection and delegated attribution | Test checks preservation and non-widening through the existing owner. No new production authority path. |
| `skald-spec/src/media.rs`, `prompt.rs`, `lib.rs` | Full new MediaFormat vocabulary/parser/codecs/schema/tests and exports; media_kind accessor, Prompt retained media-kind state and binding/schema logic | Existing Skald media/Prompt owners are extended; legacy Office distinctions stay visible. One residual documentation obligation below. |
| `wyrd-spec/src/verification/execution.rs` | Entire request/evidence/status/result/provenance/citation contract, error conversion and three tests; ActionOutput and StepResult consumers | The same ToolCallRecord crosses the handoff; optional digest and zero-byte/no-output semantics are documented without inventing a runtime projector. |
| `vala-eval/src/blocks.rs`, `lib.rs` | Every new seam/value/error/trait and both tests; existing TaskExecutor, Executors and JudgeInvoker interfaces and driver ownership | New seams distinguish data output from typed verdicts and expose deadline/cancellation ownership. They add no driver, registry or runtime implementation. Temporary legacy judge coexistence is the authorized staged handoff. |
| `wyrd-spec/src/vala/eval/{plan,operator,spec,mod}.rs` | Full planning algorithm and shared plan_stages entry point; validate_dag and VerifierSpec::plan callers; ComparisonOperator::validate and both validation delegates | Reuses the complete existing DAG algorithm and operator validation. Both new callers justify the generalization; no independent graph store. |
| `wyrd-spec/src/refs/mod.rs` | New Visit implementation and helpers/test; existing Verifier/Agent/drift visitor owners and SlotEntry | Extends the existing inventory and recurses through its existing Agent/drift workflows. No replacement loader or relationship graph. |
| `wyrd-spec/src/{ids,error,verification}.rs`, `card/{mod,prompt/parameter}.rs` | New IDs and validator/tests, existing ID macros; message_details visibility and conversion caller; staging exports and ParameterName ordering derives | Reuses identifier invariants, UUIDv7 identity, error catalog and existing declaration types. Staging does not mount another live format. |
| `wyrd-server/src/components/gateway/batches.rs` | Full invocable and its real batch-list caller, filtering before the audited GatewayInvocation decision | Exhaustive-match adaptation ignores Tool scope; existing gateway admission/discovery mechanism remains the owner. |

All 26 changed source files are covered above. There are no Python, TypeScript, manifest or generated declaration changes in the pinned cumulative diff; declaration parity for future activation remains with its assigned later tasks.

## Complete-owner reuse assessment

The existing `vala::eval::plan::validate_dag` algorithm is extended through `plan_stages`; the new validator does not merely call a cycle helper while rebuilding the DAG elsewhere. Derived ancestor lookup is an execution-scope accessibility rule, not another planner.

The existing `refs::Visit` inventory remains the Card reference owner. New scope references (`inputs.*`, `steps.*`, `requirement.*`) address execution values, so neither durable `Ref`/`InlineableRef` nor the existing `JsonPath` (which starts with `$` and does not encode block ownership/position) supplies this contract. The private Tokens parser does not replace Card loading.

Existing Skald `Prompt::new`/`check_output_schema` validate schema syntax/provider constraints and derive placeholders/response format. The new validator reads that compiled Prompt; it does not reproduce its compiler. `schema_path::admits` checks selector reachability and `fits` compares an authored output with the generated typed Verdict; the existing schema syntax checker supplies neither operation. The private bounded walk is therefore justified by current consumers. It introduces no schema rewriting, fetching or generalized public schema API.

PermissionWire, Permission::validate/covers and PermissionSet::intersection remain the complete auth workflow. Tool scopes extend the existing union, while AcceptedAuthority preserves the same typed grants. No second resolver/checker is required. MediaFormat supplies new approved normalization and compatibility semantics beside MediaRef; native Prompt binding still belongs to Prompt. Existing extension-to-MIME helpers have different fallback behavior and are not an interchangeable complete format owner.

## Prior-finding closure

| Prior finding | Source evidence in this candidate | Maintainer closure assessment |
| --- | --- | --- |
| FIND-TASK-001-1 | PermissionScope::Tool, Resource::Tool, Permission::validate/tool_invoke/tool_invoke_any and existing covers/intersection; tool permission and accepted-authority tests | Shared ownership and maintainability obligations closed. Security correctness belongs to the independent domain audit. |
| FIND-TASK-001-2 | BuiltinTool::ALL/from_name and Validator::validate_agent; all-ten-tools test and unknown/tool-less position refusals | Closed at one vocabulary owner. |
| FIND-TASK-001-3 | Validator::validate_agent calls fits against generated Verdict; positive equivalent-schema and negative structural-schema test cases | Existing schema/compiler ownership preserved; no maintainer defect proposed. Behavioral completeness belongs to implementation reviewers. |
| FIND-TASK-001-4 | ToolCallRecord output_sha256/returned_bytes rustdoc and both shared consumers; wire and handoff tests | Closed for the declared handoff, including honest no-output semantics. |
| FIND-TASK-001-5 | One Walk budget spans recursion and local refs, used by both schema consumers; cyclic and finite-recursion validation tests | Budget ownership is clear; no new maintainer defect proposed. |
| FIND-TASK-001-6 | Fallible validation helpers, DriftCheck codecs, cursor/trait items, MediaFormat field/trait items, eval delegate and all nine previously named test modules now have substantive summaries/module docs | **Partially closed**: two changed fallible formatting methods still omit mandatory error sections. See MNT-R1-001. |

## Material finding

### MNT-R1-001 — residual FIND-TASK-001-6 — VIOLATION — must-fix

- Locations: `crates/wyrd-spec/src/card/blocks/reference.rs:235` (`Display::fmt`) and `crates/skald/skald-spec/src/media.rs:304` (`Display::fmt`).
- Authority: AGENTS.md §16 requires a `# Errors` section naming error conditions on every fallible Rust function/method; missing documentation is the explicit blocking exception in wyrd-task-review. Maintainer Style §Documentation applies this to trait items as well.
- Evidence: Reference::fmt propagates formatter failures from write!/write_str using `?`; MediaFormat::fmt directly returns `f.write_str(self.as_str())`. Both return fmt::Result. Their added summaries contain no `# Errors` section. The full bodies and serde/display consumers were inspected.
- Consequence: the cumulative documentation completion gate remains unsatisfied; the remediated trait methods still do not state their propagated error contract. This does not assert a runtime bug or request a naming/style redesign.
- Smallest correction: add an accurate formatter-write failure `# Errors` section to these two existing methods. Keep their code and trait structure unchanged. Do not add a wrapper, checker, dependency or behavioral test.
- Closure proof: static inspection of these two method docs and normal formatting. Carry this under existing FIND-TASK-001-6 rather than allocating a new finding ID.

## Verification and limits

This review inspected pinned source, test bodies, cumulative and remediation diffs, task/spec snapshots, the prior validated findings and remediation evidence. The evidence records focused owner tests, 1139 passing library tests, Clippy, workspace all-features compilation, formatting, dependencies and codegen checks. Those are supplied execution claims; this maintainer pass did not independently rerun them or infer documentation completeness from Clippy.

No optional naming/placement redesign is proposed. No production files were edited. Runtime execution, providers, persistence and full client journeys are deferred by the approved task and were not exercised here.
