# Repository standards review — TASK-001 cumulative R1

Result: **FAIL** — residual documentation obligation FIND-TASK-001-6. No additional material repository-rule finding established.

## Subject and method

- Repository: `/home/thorrester/Documents/GitHub/wyrd-verifier-building-blocks`.
- Complete reviewed range: `08b38f74255d344e1c5ec9a6d278935a26277622..eb1681461897ee1eb9fe1fa4e00cb0b435658559` (26 Rust files).
- HEAD remained `eb1681461897ee1eb9fe1fa4e00cb0b435658559` at final source inspection. Uncommitted docsite/planning content is excluded.
- Read the task-review skill, subject, original task/spec snapshots, preceding verdict/validation/remediation, repository rules and routed authorities. No other current discovery report was consulted.
- CodeGraph was the first code-discovery tool; expanded its map with the cumulative diff, full new modules, modified owners, manifests, existing permission intersection and gateway callers, generator roots and tests. Review is static; no production file was changed.

## Authority coverage

All rows additionally apply `AGENTS.md`, `architecture/agent-rules.md`, the authority hierarchy in `architecture/references/README.md`, and `languages/spec-driven-development.md`, `languages/implementation-execution.md`, `languages/maintainer-style.md`, `languages/rust-core.md` and `languages/testing-workflows.md` under `architecture/references/`.

| Changed surface (complete cumulative file coverage) | Applicable additional authority | Source/consumer coverage |
|---|---|---|
| `wyrd-spec/src/card/blocks/{mod,reference,validate,schema_path,extract}.rs`; `card/mod.rs` | `wyrd-design.md` doctrine, Card/reference/Prompt/Agent/Verifier and registration contracts; `wyrd-doctrine.mdx`; `references/doctrine/{positioning-and-vocabulary,architecture-constraints}.md`; `references/architecture/patterns.md`; `references/domain/evaluation.md`; `references/languages/{agent-harness,errors}.md`; approved r6 replacement and Task 001 staging boundary | Declarative data, parser/codec/schema source, `VerifierSpec` methods, concrete `Validator` and bounded `Walk`, resolved Agent and shared Prompt inputs, generated extraction schema, position and refusal tests. |
| `wyrd-spec/src/auth/permission_scope.rs`, `card/agent.rs`; `wyrd-runtime/src/{permission,request_context}.rs` | Design Auth/Policy distinction and accepted delegation authority; `wyrd-security-posture.md` authorization/delegation; doctrine/constraints, patterns, agent harness, errors | Closed `BuiltinTool` source shared with declaration validation; Tool scope validation/coverage, Resource labels/parser, generic intersection/subsumption, strict permission wire decoding, AcceptedAuthority and role JSON tests. |
| `wyrd-spec/src/verification/execution.rs`, `verification.rs`, `ids.rs`, `error.rs` | Design typed identities, subject/actor and Run correlation, error catalog; r6 execution/result replacement; domain evaluation and telemetry-observations; agent harness/errors | Existing UUIDv7/id macros, `Sha256Hex`, result/input/locator/provenance types, existing status/verdict/error reuse, stable-code projection and wire tests; new protocol remains staged. |
| `wyrd-spec/src/refs/mod.rs`, `vala/eval/{mod,operator,plan,spec}.rs`, `card/prompt/parameter.rs` | Design single reference inventory and declarative resolution, patterns, domain evaluation | Existing Visit owner extended; inline Agent descent and Artifact/drift references covered; existing DAG planner generalized and legacy delegate retained; regex validation consolidated on existing operator; ordered ParameterName needed by maps. |
| `skald/skald-spec/src/{lib,media,prompt}.rs` | Design Prompt-derived variables/media/schema and Skald ownership; constraints/patterns; domain evaluation | Pure MediaFormat vocabulary/normalization, codec and schema trait implementations, existing Prompt media-kind state accessor, native binding check and media tests; no provider IO moved into foundation. |
| `vala/vala-eval/src/{lib,blocks}.rs` | Design Vala/Skald/server direction, `references/domain/{vala-architecture,evaluation,telemetry-observations}.md`, constraints/patterns/errors | Injected ActionExecutor/CheckExecutor, typed call/result/error values, deadline/cancellation and per-execution accepted authority contract, test implementation and provenance handoff. No new execution runtime or durable owner. |
| `wyrd/wyrd-server/src/components/gateway/batches.rs` | Design gateway explicit grants, security authorization/delegation, patterns/agent harness | Complete `invocable` body: filters Gateway/Invoke grants before scope projection; added Tool scope remains non-gateway; real batch listing retains the authoritative audited invocation decision. |
| All changed inline test modules and trait/private items | AGENTS §11/§16; agent rules testing/rustdoc; Rust/testing/maintainer references | Included tests/helpers/modules, required error/panic documentation and pure versus runtime ownership, not only exported structs. |
| Source-driven generated contracts and adjacent language consumers | AGENTS §8/§11 and generated-artifact agent rule; agent harness; generator inputs in `gen_schemas.rs` and `mise.toml` | No Python/TypeScript implementation, stubs, binding registration, UI, schema snapshot or generator output was edited. Existing consumed generator roots do not publish runtime Permission/PermissionScope or staged block types; unchanged generated TS outputs are therefore consistent with this bounded task. Tool scope and BuiltinTool retain source derives for JSON Schema and server ToSchema. Full SDK/protocol activation belongs to later tasks. |

Bifrost storage/query/admission, SQL transactions, deployment/recovery, provider transports, PyO3/Node lifetimes, Python stubs and TypeScript implementations are not materially changed here. Their implementation-specific authority does not independently apply merely because a declared tool name contains `bifrost`. No raw SQL capability, pool, external URL fetch, object-store access, new listener or audit writer is added. This is not an audit of later hosted tools, queue or sandbox behavior.

## Applicable rule results

| Rule | Evidence | Result |
|---|---|---|
| AGENTS §2/§3/§9: pure foundation, durable ownership and language-agnostic contracts | New `wyrd-spec` code uses synchronous serde/schema/validation and typed values. Vala owns injected execution seams; Skald owns media/Prompt information. No manifest or feature changes in the cumulative diff. | PASS |
| AGENTS §2 and design: no new Card kind, alternate Card envelope or live compatibility protocol | BuiltinTool is a closed tool-name enum, not CardKind; new `card::blocks` and `verification::execution` exports remain outside existing Spec/route activation. Old live contracts remain intact until Task 005. | PASS |
| AGENTS §5: concrete ownership, reusable owners and bounded abstraction | Validator owns resolver/spec/derived maps, Tokens owns cursor, Walk owns traversal budget. Pure helpers remain pure. Visit, existing DAG stages, Prompt compilation, operator rules, permission set intersection and typed hashes/identities are reused. Executor polymorphism is the approved injected action/check seam, not an added platform service. | PASS |
| AGENTS §6: async only at actual runtime seams; no ad hoc runtime or blocking IO | Foundation is synchronous. Vala declares async executor boundaries for actual injected execution; deadline/cancellation and resource release are stated in module contract. Test executor runs under the existing Tokio test harness. | PASS |
| Security/AGENTS: no authority widening, strict scope identity, independent protected resources | Tool scope carries BuiltinTool, with exact equality or All coverage. Permission::validate only admits that scope on Tool/Invoke. Existing narrower/intersection code supplies attenuation; tests exercise wrong combinations, wildcard, disjoint names and independence. No defaults or identity/audit bypass was added. | PASS |
| AGENTS §4/§9: typed identity and existing error owner | SlotName/ExecutionId use existing newtype macros; ToolCallRecord uses BuiltinTool/Sha256Hex; public validation remains WyrdError::Validation with structured rule/path, and verification error mapping uses existing stable metadata. | PASS |
| Agent rule: actual SQL capability signatures and lifecycle | No changed production signature/field accepts PgPool, PgConnection or caller transaction; no SQL or commit/rollback was introduced. Gateway helper uses Caller and existing capability flow. A green tenant checker is not used to infer this. | PASS (not a SQL change) |
| Agent rules: generated artifacts source-driven, no gate circumvention | Cumulative diff contains only Rust sources. No hand-edited generated output, new allow/ignore, removed assertion, changed timeout/sleep gate, boundary glob or Cargo feature. Codegen fresh log passed with consumed outputs unchanged. | PASS |
| AGENTS §11: tests in owning runtime and tier; targeted verification | Pure cases are inline Rust unit tests; no language lifetime is emulated, credentials required or external fast-lane binary added. Nine focused remediation cases and four-package library run pass. This bounded staged task explicitly defers assembled SDK/server journeys. | PASS for bounded task; integrated journeys not established |
| AGENTS §16: every changed item documented, every fallible method has Errors | Most prior omissions are repaired, including validation helpers, codecs, trait associated Err/items, tuple field and test-module docs. Two newly introduced Display methods remain fallible without Errors sections (finding below). | FAIL |
| AGENTS §11/§12: lint, format, build and dependency verification evidence | Fresh library/focused/codegen/diff evidence is available. Narrow all-feature Clippy, workspace all-feature tests check, format and check:deps are recorded implementation evidence rather than fresh reruns by this reviewer. No red gate is established. | PASS on available evidence; freshness limit below |

## Material finding

### RS-R1-001 — retain FIND-TASK-001-6 — Important / VIOLATION

**Required rustdoc error sections remain missing on both formatting methods.**

- Exact locations: `crates/wyrd-spec/src/card/blocks/reference.rs:235` (`Reference::fmt`) and `crates/skald/skald-spec/src/media.rs:304` (`MediaFormat::fmt`). Both are new methods in the original cumulative diff and were included among the prior finding's undocumented trait methods.
- Violated rule: AGENTS §16, “Every fallible Rust function or method MUST include a `# Errors` section naming the error conditions”; agent-rules documentation rule explicitly includes trait/private/test items and treats incomplete required rustdoc as blocking. The task-review skill explicitly preserves mandatory rustdoc as the exception to non-blocking style notes.
- Source evidence: `Reference::fmt` returns `fmt::Result` and propagates formatter writes with `?`; `MediaFormat::fmt` directly returns `f.write_str(self.as_str())`. Their new summaries document canonical text but neither names error behavior in an Errors section. Existing Reference Serialize and MediaFormat Serialize immediately nearby do have Errors sections, so the missing coverage is localized to the Display implementations.
- Consequence: prior FIND-TASK-001-6 is only partially closed; this candidate still fails the explicit changed-item documentation completion gate. This is a documentation-contract consequence, not a claimed runtime bug or auth failure. Rust/Clippy success does not prove the repository's rustdoc requirement.
- Smallest correction: at these existing methods, document that formatting returns the underlying formatter/write failure. No API change, new checker, new test fixture, refactor or documentation of untouched legacy neighbors is needed. Preserve their canonical output and parsing behavior.
- Focused closure proof: inspect both completed method rustdocs against §16 and confirm formatting. No behavioral test is needed for these two documentation-only corrections; batch them with any validated behavioral remediation rather than opening an unrelated style-only round.

## Prior closure and cumulative audit notes

| Prior finding | Source-confirmed state within standards scope |
|---|---|
| FIND-TASK-001-1 | Tool resource, exact scope and shared typed catalog exist in current owners. Strict validation, normal All/exact coverage and generic attenuation are covered by focused Permission/AcceptedAuthority tests; no new permission store or default grant. |
| FIND-TASK-001-2 | Validator membership now uses BuiltinTool::from_name; the ten-name source is shared with auth and provenance. Complete catalog positive/unknown-name negative test is present. |
| FIND-TASK-001-3 | Check-position guard uses bounded structural fits against generated typed Verdict schema, rather than only required-name presence. Types, enum/const values, closed objects and citation/locator shape have focused cases in both positions and resolver path. This row does not claim arbitrary JSON-Schema subset proof. |
| FIND-TASK-001-4 | ToolCallRecord now has optional typed output_sha256 and required returned_bytes; no-output semantics are documented. Both ActionOutput and StepResult retain the same record, with serde/handoff proof. |
| FIND-TASK-001-5 | One Walk budget is retained across branch recursion and local-reference resolution. Public validation tests cover cyclic schemas through both JSON input and resolved Agent output, preserving finite recursive known-missing refusal. |
| FIND-TASK-001-6 | Validator fallible helpers, Drift codecs, cursor field, parse/schema/serde trait methods, UnknownMediaFormat tuple field, Eval operator delegate and all newly added test modules are documented. Display Errors omissions above remain open. |

The original cumulative additions, not only remediation, were included: extraction fields/nullability and tagged unions, public result provenance/identities, reference visitor helpers, planner/operator delegates, Prompt accessor, test helpers, private walker methods and staged exports. No optional naming/placement/import cleanup is promoted to a blocking finding. Existing legacy judge code is left compiled as required by the present handoff; its eventual removal remains Task 005's responsibility.

## Verification notes

- Reviewed current `library-check.log`: **1139 passed, zero skipped**, four library binaries.
- Reviewed current `focused-check.log`: **9 passed**, actual named remediation cases selected (1031 unrelated tests skipped by that focused selection).
- Reviewed current `codegen-check.log` and source task definition: generation/theme checks pass; orchestrator confirms tracked generated outputs unchanged. This lane does not prove the served OpenAPI document or runtime MCP tool catalog.
- Cumulative whitespace check passed per current subject proof. Clippy/workspace check/fmt/check:deps successes are implementation-record evidence; I did not rerun those or overlap Cargo work with concurrent reviewers.

Skipped fresh build/lint and assembled server/SDK journeys; no source was edited. Residual risk is the explicit rustdoc gate above and later-task runtime/integration behavior that this staged contract audit cannot prove.
