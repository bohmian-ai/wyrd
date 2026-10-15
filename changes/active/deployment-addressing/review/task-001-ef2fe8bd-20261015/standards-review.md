# Repository standards review

## Review Findings

### Critical

None confirmed by this standards audit.

### Important

**REP-001 — VIOLATION — Required rustdoc is missing from materially changed Rust items, including tests.**

- Primary location: `crates/skald/skald-spec/src/authoring.rs:273` (`AuthoredPrompt::native_request`).
- Authority: `AGENTS.md` §16 and `architecture/agent-rules.md` Rust documentation rule. Every new or materially modified item, irrespective of visibility or test status, requires substantive rustdoc; every fallible function requires an `# Errors` section. The authority explicitly makes missing/incomplete rustdoc a merge blocker. `architecture/references/languages/rust-core.md` and `maintainer-style.md` repeat this requirement.
- Evidence and shared cause: the new compiler's public `compile` documents its errors, but its private fallible stages and several changed consumers/tests do not. The documentation pass stopped at selected public APIs instead of applying the same rule to the complete changed Rust surface. Confirmed examples in the immutable candidate:

| Candidate location | Missing required documentation |
|---|---|
| `crates/skald/skald-spec/src/authoring.rs:273` | `native_request` has a one-line description and no `# Errors` for mixed portable/native fields, incompatible format, duplicate `body.model`, or body decoding failure. |
| `crates/skald/skald-spec/src/authoring.rs:328` | `portable_request` lacks `# Errors` for unsupported portable provider, missing content, media lowering, or invalid settings. |
| `crates/skald/skald-spec/src/authoring.rs:386` | `PortableMessage::lower` lacks `# Errors` for assistant media and malformed bindings. |
| `crates/skald/skald-spec/src/authoring.rs:451` | `check_format` lacks `# Errors`. |
| `crates/skald/skald-spec/src/authoring.rs:480`, `:561`, `:625` | `openai_chat`, `anthropic`, and `google` lack `# Errors` for their different supported settings, bounds, and required values. |
| `crates/skald/skald-spec/src/authoring.rs:701`, `:717`, `:734`, `:745`, `:764` | New fallible `refuse`, `within`, `positive`, `stops`, and `to_body` lack `# Errors`. |
| `crates/skald/skald-spec/src/prompt.rs:60`, `:73`, `:81`, `:90`, `:94` | New/changed equality, serde, and schema methods lack item rustdoc; the enclosing `PartialEq` implementation's description does not document the methods or serde failures. |
| `crates/skald/skald-spec/src/request.rs:92`, `:135`, `:141`, `:147`, `:156`, `:160` | The changed provider parser's associated error type and formatting/serde/schema methods lack item rustdoc. |
| `crates/vala/vala-eval/src/orchestrator/judge.rs:137` | Materially changed `agent_for` has no rustdoc explaining its one-judge cache, resolution, structured-output/route refusals, or errors. |
| `crates/wyrd-spec/src/card/prompt/spec.rs:59` | Replaced `PromptSpec::deserialize` has no item rustdoc or `# Errors`. |
| `crates/wyrd/wyrd-cards/src/prompt/io.rs:203` | New `yaml_native_prompt_card_preserves_extension_options` test has no rustdoc or panic documentation despite filesystem, parse, and assertion panics. |
| `crates/shared/wyrd-loader/src/lib.rs:217`; `crates/shared/wyrd-loader/src/resolve.rs:253` | Changed `prompt` fixture helper and `resolve_path_rewrites_durable_ref_and_loads_sibling` test have no rustdoc. |
| `crates/skald/skald-agent/tests/integration/agent_construction.rs:10`; `crates/skald/skald-spec/src/prompt.rs:1192`, `:1201`, `:1210`, `:1227` | Changed Prompt construction helper, rendering tests, and media helper have no rustdoc. |
| `crates/wyrd-spec/src/card/agent.rs:291`, `:315` | Materially changed Agent contract round-trip tests have no rustdoc. |
| `sdks/wyrd-sdk-rust/tests/integration/gateway_inference.rs:651`, `:741`, `:742` | New function-local `BACKUP_KEY`, `EXTERNAL_KEY`, and `MODEL` constants lack item rustdoc; the test's surrounding docs do not document these constants. |
| `crates/skald/skald-spec/src/authoring.rs:790`; `crates/wyrd-spec/src/card/prompt/mod.rs:393` | New inline test modules have no module rustdoc. |

- Producer-to-consumer path and consequence: `AuthoredPrompt::compile` calls `native_request` or `portable_request`; the latter calls `PortableMessage::lower` and the protocol compilers and validators. `Prompt::deserialize` and `PromptSpec::deserialize` expose these semantics to Card/Agent registration, local loading and SDK hydration. `SkaldJudgeInvoker::agent_for` consumes the resulting Prompt and execution route for verification. The undocumented rules are therefore active contract and judge behavior, not dead helpers. The test omissions independently violate the explicit completion gate. This is a source-established policy failure; it is not a claim that rustc fails, or that documentation absence itself changes runtime output.
- Smallest correction: document the touched items in place, including private helpers, trait methods, associated types, test modules/helpers/functions/constants. Describe actual error conditions and remaining panics; document cache, cancellation or partial-progress behavior where relevant. Do not change runtime behavior or add a new checker. Audit the complete changed Rust item set rather than fixing only this representative list, and leave untouched items alone.
- Focused proof: repeat the source-level changed-item rustdoc audit, including test bodies changed without signature changes. Run the owning formatting/lint checks for the documentation patch. Existing compilation or test success cannot waive this rule and compiler `missing_docs` does not cover every private/test item.

### Suggestions

No optional improvements proposed in this standards role.

## Subject and method

- Base: `1d3b5ffa0d34f75fa5b22c9b68f6ec0fc8d0cc70`.
- Candidate: `ef2fe8bda2071f7b32cb6803b7f9f4599f0d4183`.
- Source: immutable archive `/tmp/wyrd-task-review-ef2fe8bd`; later live-worktree edits are excluded. The new subject supersedes the interrupted `0aae38f9` attempt. Its additional commit changes only the existing registration-route tests; that file is included in this audit.
- Reviewed the cumulative changed-path inventory and diff, then source/callers at the independently governed boundaries below. Read repository rules and the canonical reference router; used CodeGraph before source discovery in the initial indexed checkout. The immutable archive has no `.codegraph`, so archive navigation used ordinary source reads/searches. No source, task, authority, or another reviewer's report was changed.
- This report audits standards, not the task's 32 acceptance obligations or optional simplification. Explicit approval of adjacent changes is respected.

## Changed-surface authority coverage

Every changed-path family is covered below; deletions are included through their surviving registrations, imports, consumers and tests. Common authority for all rows is `AGENTS.md`, `architecture/agent-rules.md`, `architecture/wyrd-design.md`, `architecture/wyrd-doctrine.mdx`, and the reference router. Applicable focused references were read as complete documents.

| Changed surface | Additional governing authority | Source/consumer coverage |
|---|---|---|
| Workspace manifests, lockfile, release workflow/config, nextest, mise and selection/check scripts | Operations `deployment-and-release.md`, `reliability-and-recovery.md`; spec-driven-development, implementation-execution, testing-workflows | Cargo feature/dependency changes, deleted workflow crate, telemetry publication metadata, protoc retry/ARM swap, retired test selections, installed-CLI setup lane, deps checker. |
| `wyrd-spec` envelope/kinds/IDs, gateway deployment/policy/record, permission scope, Agent/Prompt contracts, refs/graph/metadata/errors/schema generator | Positioning-and-vocabulary, architecture-constraints, patterns, rust-core, errors, agent-harness | Typed deployment/model identity; required closed execution route; pure contracts; centralized reference inventory; serde/schema projections and removed Workflow public contract. |
| Shared client Cards/hydration/state, loader, gateway caller, Run invocation, shared permission/roles and config | Architecture-constraints, patterns, rust-core, errors, agent-harness | Existing `Cards`/registry/hydrator owners retained; `Run::invoke_on` consumed by all language boundaries; moved public gateway caller uses shared transport; canonical loader/ref slots; permission coverage. |
| Skald authoring/request/Prompt/builder/coercion, Agent/gateway/loop, provider removal/endpoint | Rust-core, pyo3-boundaries, python-api-and-stubs, errors, patterns | Pure synchronous Prompt compilation; native provider request owners; meaningful retained gateway adapter; two real gateway caller implementations; per-Agent route/provider registry; existing optional Python boundary. |
| Server gateway administration/invocation/batches/ingress/caller/capture/ledger/routes, auth/boot/config/state | Security posture; operations authorities; patterns, rust-core, errors, agent-harness, telemetry-observations | Actual SQL capability signatures and retained owners; effective permission check/outbox stage; credential/provider binding; endpoint screening path; native route registration; removed Workflow host/listeners/state. |
| SQL gateway migration/queries/rows, Card registration service/resolve/mapping | Security posture; patterns, rust-core, testing-workflows | TenantConn signatures and caller transaction lifecycle; composite tenant credential FK and RLS; durable server registration owner; no introduced raw production pool interface. |
| Vala judge/evaluation and Bifrost gateway tables, server bounded query and telemetry export | Bifrost design; evaluation, telemetry-observations, olap-serving, analytical-operations-reliability | `SkaldJudgeInvoker`, server gateway caller, existing accepted authority/outbox ownership; named Arrow fields and deployment/model structs; query collector limits retained; renamed trace catalog entry. |
| Rust tests/examples/harness and YAML/JSON fixtures | Testing-workflows, rust-core, errors; relevant domain authorities above | New/changed routing, registration, authorization, native ingress, structured judge, migration, CLI and real SDK journeys; mock providers and repository Postgres fixture use; modified test rustdoc included in REP-001. |
| Python native wrappers, authored package/stubs, integration/unit/smoke/type fixtures and examples | Pyo3-boundaries, python-api-and-stubs, errors, testing-workflows | Registration/public export/stub removal parity for Workflow; Prompt constructor changes; route converted before shared runtime bridge; public Python imports and real-server journeys. |
| TypeScript N-API Rust, generated declarations/wire/error unions, wrapper/types/tests/examples | Typescript-guide, errors, testing-workflows | Native `Run.invoke` optional route argument; wrapper forwards typed ExecutionRoute; generated declaration signature and source schema changes; retired Workflow export cleanup; real Node journeys. |
| Svelte/UI Card types, registry, fixtures, tests, CSS/workspace deletion | Repo `wyrd-ui` skill; typescript-guide; doctrine and testing-workflows | Removed Workflow kind/presentation/workspace and selectors; surviving Service/Verifier registry tests; no new style tokens or frontend durable owner introduced. |
| Architecture, README, docs generator/pages, examples and fixture documentation | Human-readable doctrine/protocol and operations authority; agent-harness; relevant language references | Updated 14-kind vocabulary, named deployment examples and routes, inline Prompt materialization contract, removed Workflow public documentation, generator source update. |
| Active spec/task packet | Spec-driven-development and implementation-execution | Approved r5 authority and focused-verification table read; recorded results treated as claims, not independent executions. |

## Applicable-rule results

PASS here means no standards violation established by source inspection in that rule family. It does not claim fresh runtime proof or that another review role cannot establish a behavioral defect.

| Rule | Result | Source evidence |
|---|---|---|
| Pure foundational contracts; no IO/async/PyO3 added to wyrd-spec | PASS | Contract changes stay in schema/domain values; Prompt compilation lives in `skald-spec::AuthoredPrompt`. |
| Rust/server/Vala/Skald/client ownership and narrow dependency placement | PASS | Gateway execution uses `GatewayInvocation`; Vala retains judge engine; SDKs consume shared `Run::invoke_on`; removed workflow dependency edges are removed from manifests/lockfile. |
| Meaningful retained-state structs; real polymorphism | PASS | `GatewayAdministration`, `GatewayInvocation`, `CallPlan`, `CredentialResolver`, `SkaldJudgeInvoker` and public/server gateway callers retain dependencies/invariants; pure compiler helpers are synchronous; WyrdGatewayCaller has public and server implementations. |
| Async earns IO; runtime lifetimes belong to owning language | PASS | Caller `call` awaits shared HTTP/in-process gateway; judge resolves Cards; pure authoring remains synchronous. No added Python initialization or Node lifetime emulation in Rust tests. |
| SQL capability types, owning-tier imports, caller-owned commit/rollback, RLS | PASS | `queries/gateway.rs` functions accept `&mut TenantConn<'_>`; `GatewayAccounting` retains the same capability; `EffectiveSpecs` receives it without committing; server administration owns acquired transactions. Raw pools seen in changed source files belong to existing cfg(test)/test-harness code, not introduced production signatures. |
| Effective authorization, tenant/credential binding, nonblocking audit path | PASS | `GatewayInvocation::decide` checks the effective principal and granted-deployment object scope before staging exactly its decision on `scribe_outbox`; credential resolver retains tenant/provider/endpoint assignment; migration's composite tenant credential FK remains. |
| Secret wrappers/redaction and screened effective endpoints | PASS | `ProviderSecret(SecretString)`, existing sealed managed binding and credential matching retained; `GatewayConfig::validate` rejects built-in endpoint overrides in production; dispatch continues through screened/pinned HTTP owner. |
| Derive-backed public errors and typed HTTP/headless contracts | PASS | Errors remain WyrdError metadata; CLI/shared callers preserve stable problems; native ingress and MCP administration retain server authorization owners. |
| One canonical Card/reference inventory and durable registry owner | PASS | Workflow slots/kind removed from the existing inventories rather than adding a second one; existing registration and hydration owners remain. |
| Python registration/exports/authored stubs and GIL boundary | PASS | PyRun decodes route to Rust before `trace::block_on`; changed Prompt wrappers remain in the existing migration boundary; SDK aggregate/export/stub removals align. No Bound object is captured by new async invocation work. |
| TypeScript thin binding/generated declaration source | PASS | `native/src/cards.rs` owns new N-API argument; `index.d.ts`/`.d.cts` reflect it; generated card/error types correspond to modified schema/catalog and generator inputs; wrapper does not implement routing. Regeneration itself is only recorded evidence. |
| Runtime-only tests owned by Python/Node; integration files earn dependency/server boundary | PASS | Python/TS journeys execute owning runtimes; Rust integration journeys use real HTTP/SDK/server or multiple non-foundational crates; added DB journeys are gated. |
| No weakened live gate or arbitrary feature expansion established | PASS | deps/nextest/mise removals match retired Workflow owners; new setup journey is selected; no new Cargo feature added; test commands retain narrow feature sets. |
| New user-facing work has real journeys on first-class/headless surfaces | PASS | Sources include Rust gateway/registration/native/judge journeys, Python and TS agent-route journeys, MCP administration and served OpenAPI contract tests, and installed CLI fresh-client journey. Detailed acceptance completeness belongs to the two implementation reviews. |
| Every touched Rust item has substantive rustdoc, including Errors/Panics where required | **FAIL** | REP-001; applies to private and test items independently of compilation. |
| UI remains a client; contract vocabulary and generated styling preserved | PASS | Kind/workspace cleanup projects server vocabulary; CSS changes remove retired presentation only. No new server-owned frontend behavior or hand-edited theme output. |
| Focused command precision and verification scope | PASS with limits below | Task records exact selectors and environment wrapper for T01–T17; integrated aggregate explicitly belongs to change review. No new aggregate required during read-only discovery. |

## REP-001 expanded affected-function inventory

The following cumulative changed functions also require correction under the same finding. `item` means no item rustdoc; `Errors` means rustdoc exists but the required error section is absent. This includes material body edits where the signature itself was unchanged. The preceding finding also identifies affected modules, an associated type, and function-local constants.

| File | Functions and candidate lines |
|---|---|
| `crates/shared/wyrd-loader/src/lib.rs` | `prompt`:217 (item) |
| `crates/shared/wyrd-loader/src/parse.rs` | `parse_rejects_missing_kind`:353 (item); `parse_accepts_multi_doc_yaml`:420 (item); `parse_preserves_source_path`:464 (item); `parse_collects_every_envelope_violation`:490 (item); `parse_rejects_inline_file_symlink_outside_workspace`:527 (item) |
| `crates/shared/wyrd-loader/src/path.rs` | `path_discovery_does_not_follow_symlink_directories`:265 (item) |
| `crates/shared/wyrd-loader/src/resolve.rs` | `resolve_path_rewrites_durable_ref_and_loads_sibling`:253 (item); `resolve_rejects_multi_document_path_targets`:282 (item); `resolve_loads_transitive_path_references_in_discovery_order`:306 (item) |
| `crates/skald/skald-agent/tests/integration/agent_construction.rs` | `openai_prompt`:10 (item); `agent_with_prompt_overrides_prompt_reference`:49 (item) |
| `crates/skald/skald-agent/tests/integration/agent_fields.rs` | `test_prompt`:14 (item) |
| `crates/skald/skald-agent/tests/integration/agent_journey.rs` | `test_prompt`:178 (item) |
| `crates/skald/skald-agent/tests/integration/construct.rs` | `prompt`:37 (item); `unknown_runtime_local_tool_uses_wyrd_code`:305 (item) |
| `crates/skald/skald-agent/tests/integration/delegate.rs` | `test_prompt`:196 (item) |
| `crates/skald/skald-agent/tests/integration/loop_basic.rs` | `build_agent`:15 (item); `run_prompt_sends_rendered_prompt_verbatim_on_first_turn`:188 (item); `run_prompt_provider_mismatch_returns_409_provider_mismatch`:261 (item); `run_prompt_missing_variable_emits_prompt_error`:289 (item) |
| `crates/skald/skald-agent/tests/integration/loop_conversation.rs` | `test_prompt`:386 (item) |
| `crates/skald/skald-agent/tests/integration/loop_gemini.rs` | `agent_run_executes_gemini_loop_against_mock_provider`:61 (item) |
| `crates/skald/skald-agent/tests/integration/session_journal.rs` | `test_prompt`:745 (item) |
| `crates/skald/skald-prompt/src/builder.rs` | `openai_chat`:90 (Errors); `openai_responses`:130 (Errors); `anthropic`:164 (Errors) |
| `crates/skald/skald-prompt/src/coerce.rs` | `provider_name_from_py`:31 (Errors) |
| `crates/skald/skald-prompt/src/error.rs` | `from`:84 (item) |
| `crates/skald/skald-prompt/src/prompt.rs` | `with_system`:89 (Errors); `with_tool_result`:127 (Errors); `with_role_text`:186 (item); `unsupported_role`:277 (item); `wrong_provider`:285 (item); `openai_chat`:445 (Errors); `openai_responses`:477 (Errors); `anthropic`:507 (Errors); `gemini`:537 (Errors); `vertex`:565 (Errors); `image_url`:732 (Errors); `set_request`:925 (Errors); `append_native_json_content`:1513 (item) |
| `crates/skald/skald-spec/src/authoring.rs` | `native_request`:273 (Errors); `portable_request`:328 (Errors); `lower`:386 (Errors); `check_format`:451 (Errors); `openai_chat`:480 (Errors); `anthropic`:561 (Errors); `google`:625 (Errors); `refuse`:701 (Errors); `within`:717 (Errors); `positive`:734 (Errors); `stops`:745 (Errors); `to_body`:764 (Errors); `compile`:798 (Errors) |
| `crates/skald/skald-spec/src/prompt.rs` | `eq`:60 (item); `serialize`:73 (item); `deserialize`:81 (item); `schema_name`:90 (item); `json_schema`:94 (item); `bind_mut`:324 (Errors); `bind_media_mut`:346 (Errors); `render_binds_dollar_brace`:1192 (item); `render_binds_mustache`:1201 (item); `render_binds_mixed_syntax_in_one_pass`:1210 (item); `prompt`:1227 (item); `media_placeholder_in_system_message_is_rejected`:1280 (item); `prompt`:1517 (item) |
| `crates/skald/skald-spec/src/request.rs` | `fmt`:135 (item); `serialize`:141 (item); `deserialize`:147 (item); `schema_name`:156 (item); `json_schema`:160 (item) |
| `crates/vala/vala-eval/src/orchestrator/judge.rs` | `agent_for`:137 (item) |
| `crates/vala/vala-eval/tests/integration/orchestrator_judge_skald.rs` | `resolve`:35 (item); `inline_agent_ref`:81 (item) |
| `crates/wyrd-spec/src/card/agent.rs` | `agent_spec_inline_prompt_roundtrips`:291 (item); `agent_spec_card_prompt_uses_single_version_field`:315 (item); `agent_card_uses_the_shared_envelope_without_client_derived_state`:343 (item) |
| `crates/wyrd-spec/src/card/mod.rs` | `prompt_spec`:2500 (item) |
| `crates/wyrd-spec/src/card/prompt/spec.rs` | `deserialize`:59 (item) |
| `crates/wyrd-spec/src/error.rs` | `from`:4036 (item) |
| `crates/wyrd-spec/src/graph/mod.rs` | `build_ignores_external_refs`:241 (item); `build_derives_sibling_edges_from_nested_refs`:261 (item); `graph_ready_submissions_preserves_authored_version_selection`:305 (item); `duplicate_submission_identity_is_rejected`:323 (item) |
| `crates/wyrd-spec/src/metadata.rs` | `prompt_spec`:386 (item) |
| `crates/wyrd-spec/src/reference.rs` | `unresolved_card_ref_paths_include_nested_slots`:896 (item) |
| `crates/wyrd-spec/src/refs/mod.rs` | `agent`:539 (item) |
| `crates/wyrd/wyrd-auth/src/card_scope.rs` | `resolve_caps_at_max_scope_cards`:599 (item) |
| `crates/wyrd/wyrd-cards/src/agent.rs` | `__new__`:103 (Errors) |
| `crates/wyrd/wyrd-cards/src/prompt.rs` | `converts_holder_metadata_to_prompt_spec`:878 (item) |
| `crates/wyrd/wyrd-cards/src/prompt/io.rs` | `prompt_card`:139 (item); `yaml_native_prompt_card_preserves_extension_options`:203 (item) |
| `crates/wyrd/wyrd-cli/src/card.rs` | `parse_kind`:745 (item) |
| `crates/wyrd/wyrd-cli/tests/card_lifecycle.rs` | `write_prompt`:72 (item); `write_multi_card_service`:88 (item) |
| `crates/wyrd/wyrd-gateway/src/adapter/http.rs` | `dispatch`:641 (item) |
| `crates/wyrd/wyrd-server/src/auth/jwt_bearer.rs` | `insert_test_card`:708 (item) |
| `crates/wyrd/wyrd-server/src/components/cards/resolve.rs` | `submission`:724 (item); `sibling_prompt_reference_is_not_external`:779 (item) |
| `crates/wyrd/wyrd-server/src/http/error.rs` | `sample_errors`:595 (item) |
| `crates/wyrd/wyrd/tests/umbrella_integration.rs` | `prompt`:40 (item) |

## Open Questions

None needed to establish REP-001. It is visible directly in the candidate source.

## Verification Notes

No tests, builds, lint, codegen, or package installation were run, as required by discovery instructions. The task records T01–T17 plus a judge-refusal test and owner-check passes; these are implementation evidence claims and do not establish independent execution on this exact snapshot. The integrated aggregate, production/cloud qualification and later concurrent commits remain outside this report. Generated declaration parity was inspected statically; it was not regenerated. No external version-sensitive API claim is used to support the finding.

Overall result: **FAIL** — REP-001 is an explicit hard repository gate. No other material standards finding is asserted.
