# Repository standards review

Result: **FAIL**. Subject: base `08b38f74255d344e1c5ec9a6d278935a26277622` to candidate `896e563bee69ab6b5f47b31977d7de1a0ca88d57`. This is an independent standards audit, not task acceptance. No other review report was read. Production source was not modified; HEAD remained the candidate during inspection.

## Review Findings

### Critical

None.

### Important

**RS-001 — VIOLATION — Complete the required rustdoc on changed Rust items.**

Governing rule: `AGENTS.md` §16 requires substantive rustdoc on every new or materially modified item, including private fields, associated types, modules, helpers, and tests; every fallible function requires `# Errors`. `architecture/agent-rules.md` explicitly makes missing documentation `BLOCK_BEFORE_MERGE`. This is the explicit documentation exception to the review's otherwise consequence-based threshold.

Confirmed affected locations in the candidate:

| Source | Missing documentation |
| --- | --- |
| `crates/wyrd-spec/src/card/blocks/mod.rs:335`, `:345` | `DriftCheck::serialize` and `deserialize` have no rustdoc or `# Errors`, despite custom flattening, removal of the required `on` property, reference parsing, and drift-body decoding. |
| `crates/wyrd-spec/src/card/blocks/validate.rs:145`, `:208`, `:244`, `:292`, `:355`, `:401`, `:430`, `:468`, `:566`, `:639`, `:743`, `:783` | `requirement_owners`, `Validator::run`, the private validation methods, `validate_slot`, and `validate_operator` return `Result` but omit required `# Errors`. Their summaries do not substitute for the mandatory sections identifying refusals and error propagation. |
| `crates/wyrd-spec/src/card/blocks/reference.rs:179`, `:231`, `:267`, `:269`, `:275`, `:281`, `:288`, `:292` | `Tokens.rest`, Display/FromStr/Serde/schema methods, and `FromStr::Err` have no rustdoc. Fallible trait methods also lack `# Errors`. |
| `crates/skald/skald-spec/src/media.rs:183`, `:300`, `:306`, `:334`, `:340`, `:348`, `:352` | `UnknownMediaFormat`'s tuple field, Display/Serde/schema methods, and `FromStr::Err` are undocumented. Fallible trait methods lack `# Errors`. `from_str` itself is documented correctly. |
| `crates/wyrd-spec/src/vala/eval/spec.rs:182` | Materially changed `validate_task_operator` has no rustdoc or `# Errors`; it now delegates to the newly shared operator validator. |
| `crates/wyrd-spec/src/card/blocks/{extract.rs:187,reference.rs:314,schema_path.rs:74,validate.rs:867}`, `crates/wyrd-spec/src/{ids.rs:532,refs/mod.rs:541,verification/execution.rs:387}`, `crates/skald/skald-spec/src/media.rs:370`, `crates/vala/vala-eval/src/blocks.rs:159` | New test modules have no module rustdoc. Individual test functions generally do have scenario descriptions and `# Panics`; the modules are independently covered by the rule. |

Consequence: the explicit repository merge gate is unsatisfied; custom serialization and refusal contracts are missing the documentation required to change them safely. Green Clippy and behavioral tests do not prove this rule.

Smallest correction: add substantive rustdoc to the affected changed items in their current owners, documenting operation and relevant invariants and `# Errors` on every fallible function. Include all new private/test items in that same correction; do not rewrite APIs or add a documentation checker. Closure: inspect the cumulative changed items against §16, run formatting and the narrow owner lint checks, and retain the existing tests. No new behavioral tests are needed for documentation-only edits.

### Suggestions

None. Naming, import-placement, and equally valid structural preferences are not acceptance blockers here.

## Authority coverage

The reference router was read before selecting the following complete concern slices. `AGENTS.md`, `architecture/agent-rules.md`, `architecture/wyrd-design.md`, `architecture/wyrd-doctrine.mdx`, and `architecture/wyrd-security-posture.md` govern all applicable rows. The approved r6 spec explicitly supersedes the old Verifier implementation split; this audit does not reimpose that split. No Bifrost runtime, storage implementation, deployment operation, Python, TypeScript, PyO3, UI, SQL, or external URL fetch changes occur in this candidate, so their implementation-specific reference slices are not applicable.

| Changed surface (complete cumulative range) | Applicable routed authorities | Inspected evidence |
| --- | --- | --- |
| `card/blocks/{mod,validate,reference,schema_path,extract}.rs`; `card/mod.rs` | `doctrine/positioning-and-vocabulary.md`; `doctrine/architecture-constraints.md`; `architecture/patterns.md`; `languages/rust-core.md`; `languages/errors.md`; `languages/agent-harness.md`; `domain/evaluation.md` | Full new modules, closed enums, pure validation, schema-path handling, typed identities, inline tests, existing Card/Agent/Prompt/Drift references. |
| `verification/execution.rs`; `verification.rs`; `ids.rs`; `error.rs` | Same doctrine/architecture/Rust/error/agent-surface references; `domain/telemetry-observations.md` | Entire execution contracts, reused status/verdict/errors/upload plans/hash/trace IDs, identity macros, error conversion, round-trip tests; generator roots and TypeScript root selection. |
| `refs/mod.rs` | Doctrine vocabulary and architecture constraints; architecture patterns; Rust core | Existing `Visit`/`SlotEntry`/`SlotValue` inventory and new block visitor, nested Agent/Prompt and Artifact/baseline traversal; visitor test. |
| `vala/eval/{mod,operator,plan,spec}.rs` | Architecture constraints/patterns; Rust core/errors; `domain/evaluation.md`; `domain/vala-architecture.md` | Entire changed planner and operator delegation, existing DAG errors/cycle witness owner and offline `EvalSpec` consumers/tests. |
| `skald-spec/src/{media,prompt,lib}.rs`; `card/prompt/parameter.rs` | Architecture constraints/patterns; Rust core; evaluation | Media format enum/Serde/schema/MIME behavior/tests, Prompt binding-kind accessor and existing retained media binding map, ordering-only parameter change, re-exports. |
| `vala-eval/src/{blocks,lib}.rs` | Architecture constraints/patterns; Rust core/errors; evaluation; Vala architecture; telemetry/observations | Entire injected action/check seam, error behavior, resolved input provenance, deadline and cancellation contract, test implementation. |
| All new tests and changed crate boundaries | `languages/testing-workflows.md`; `languages/spec-driven-development.md`; `languages/implementation-execution.md`; `languages/maintainer-style.md` | In-module tests, no external service setup, manifest/dependency review, `mise.toml` verification commands, task evidence and fresh orchestrator test results. |

The `rust-python` skill was consulted for Rust guidance. Repository authority takes precedence over its generic Python/runtime examples; no Python runtime or binding code changed.

## Rule results

| Applicable rule | Result | Exact source/evidence |
| --- | --- | --- |
| `AGENTS.md` §§2–3: foundational contracts remain pure and durable runtime stays in its owner | PASS | New block/reference/schema/wire code is synchronous Serde/schemars/pure data; runtime `Instant`, async trait and executor seams are in `vala-eval/src/blocks.rs:17`–`:155`, not `wyrd-spec`. No manifests or features change. |
| §§2/9: canonical Card kinds/envelope, Ref/InlineableRef and shared wire vocabulary | PASS for standards placement | No CardKind/envelope change. `AgentCall.agent` uses `InlineableRef<AgentSpec>` (`blocks/mod.rs:187`); code sources use `Ref` (`:242`); result identities use `CardRef` (`execution.rs:170`). Staged types are not mounted as a second route/registry format. Detailed r6 behavior is for task reviewers. |
| §4: typed durable IDs and established error machinery | PASS | `SlotName` and `ExecutionId` extend existing ID macros (`ids.rs:131`, `:385`); declarations use `CardUid`, `RunId`, `TraceId`, `Sha256Hex`; validation returns existing `WyrdError::Validation`; local `BlockError` uses `thiserror` (`vala-eval/src/blocks.rs:112`). No parallel public catalog added. |
| §5: cohesive retained-state owner and narrow capability seams | PASS | `Validator` retains spec/resolver/lookups (`validate.rs:191`) and runs its methods; pure schema/reference helpers remain local. Generic `plan_stages` extends the existing planner and `validate_dag` delegates (`plan.rs:110`). Executor traits are the narrow injected Action/Check capabilities, not a second runtime orchestrator. |
| §6: narrow async, timeouts and cancellation ownership | PASS for staged boundary | Foundation remains synchronous. `BlockCall.deadline` and module cancellation contract (`vala-eval/src/blocks.rs:9`–`:15`, `:76`) state engine ownership and executor resource release; no new IO implementation, worker, runtime or blocking request path is introduced. |
| Agent rules: tenant SQL capability types, transaction lifecycle and tier imports | PASS / no SQL change | Actual new signatures/fields inspected: no `PgPool`, SQL connection, transaction, `TenantConn`, `OperatorPool`, pool construction, query, or commit/rollback. No SQL capability bypass is hidden outside the check's scan roots. |
| §§9–10/security posture: verified tenant authority, permission/audit ownership and secrets | PASS for changed contracts; execution deferred | `VerificationRequest` contains no trusted tenant/principal (`execution.rs:31`–`:49`). BlockCall has no credential field; executors are documented as constructed with accepted authority. No permission evaluation, audit writer, network fetch, secret-bearing service state, or live protected tool dispatch is added. This does not prove future executor authorization. |
| §11/agent rules: unit test placement/runtime ownership and no credentials in fast lane | PASS | All added tests inline in owning Rust modules; no Python/Node lifetime, Postgres, Docker or provider credential is initialized. Mock Echo only exercises the injected seam. |
| §11: regenerate contract outputs and prove drift | PASS | Orchestrator ran `mise run codegen:check` at the pinned candidate (exit 0; `codegen-check.log`), with tracked source/generated outputs unchanged. Original implementer omission is closed by fresh proof. Static generator-root inspection confirms new staged types are not registered roots; no premature activation or hand editing is required. |
| §11: capability-appropriate runtime proof | PASS for bounded staged task | No new live HTTP/MCP/SDK journey is shipped here. Approved task explicitly defers assembled journeys; fresh orchestrator library run passed 1069/1069 with zero skips, plus three exact tests. This is not final integrated journey acceptance. |
| §12/agent rules: never weaken gates, tests, assertions or dependency boundaries | PASS in diff | No new `#[allow]`, `#[ignore]`, deleted test, threshold/sleep workaround, dependency/feature, migration or boundary glob is introduced. New Serde skip-predicate `#[expect]` is narrow and reasoned. Claimed dependency check is not independently rerun here. |
| §16/agent rules: every changed Rust item documented, all fallible operations have Errors | FAIL | RS-001, including private validation helpers, trait implementations, field/associated types and test modules. |
| Agent rules: imports at module top, bare imported names in signatures | Non-blocking deviations | `reference.rs:293` and `media.rs:353` place schema imports inside `json_schema`; fully qualified signature types remain in `blocks/mod.rs:48`, `validate.rs:63`, `:784` and its schedule-test closure. These are style-only in this range and do not create a blocking finding. |
| Generated artifacts source-owned; SDK/runtime catalog parity | PASS for staged scope | No generated file is edited. Existing generator roots still point to old live contracts. No N-API/Python/OpenAPI/MCP route registration change occurs; activation/parity remains a later integrated obligation. |

## Open Questions

None affecting this standards verdict. Tool-catalog completeness, exact permission-scope delivery, reference semantics, and closed Verdict enforcement require the independent behavior/domain reviews; this report does not substitute a standards verdict for those acceptance audits.

## Verification Notes

Static review covered all 21 changed files, complete candidate source for new modules and materially changed surrounding functions, reference consumers, contract generator roots, manifests and canonical mise commands. CodeGraph was used first for navigation; pinned `git show`/diff supplied source truth. No report from another reviewer was consumed.

The orchestrator reports fresh exact selection of `blocks::tests::action_executor_receives_resolved_inputs`, `card::blocks::validate::tests::verifier_spec_enforces_agent_positions_and_media`, and `refs::blocks_tests::blocks_verifier_yields_every_reference_slot`: 3 passed, 1066 skipped; its fresh combined library run passed 1069/1069, zero skipped. Full cumulative `git diff --check` and `mise run codegen:check` also passed. The latter preserved tracked source/generated output and closes the implementer’s omitted contract-generation proof. Implementer-reported Clippy, workspace all-features check, format and dependency checks are claims reviewed for scope, not fresh execution by this reviewer.

No build, live integration or user journey was run by this reviewer; code generation and library tests were freshly run by the orchestrator. Later integration of staged contracts remains unproven; mandatory rustdoc is presently incomplete.
