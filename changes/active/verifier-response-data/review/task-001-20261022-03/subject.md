# Immutable review subject

- Repository: /home/thorrester/Documents/GitHub/wyrd
- Base: fb1fd9234acf9ff20b1feff89bf9f3da9cba4f14 (PR 205 head)
- Candidate: d2ffc59873b5af688adf1d1e5b2a3a388160bb83
- Cumulative range: git diff fb1fd9234...d2ffc5987
- Remediation locator range: 4b723032f..d2ffc5987; never substitutes for the cumulative audit.
- Approved spec: changes/active/verifier-response-data/spec.md (revision 1)
- Original task: changes/active/verifier-response-data/tasks/TASK-001-verdict-formats-and-custom-data.md
- Prior verdict and ledger: changes/active/verifier-response-data/review/task-001-20261022-02/{verdict.md,findings-validation.md}
- Remediation and recorded evidence: changes/active/verifier-response-data/review/task-001-20261022-02/TASK-001-R1-preserve-schema-and-safe-data.md
- Evidence: original task Implementation evidence; R1 Implementation evidence; prior verification.md files. These are claims to check, not conclusions.
- Excluded working-tree user edits: .agents/skills/wyrd-plan/SKILL.md, .agents/skills/wyrd-spec/SKILL.md, changes/spec-driven-development.md. Read committed workflow with git show d2ffc5987:changes/spec-driven-development.md for subject authority; do not edit these files.

## Shared navigation map

Starting points only; every reviewer expands and verifies coverage independently.

- Format contracts and generation: skald-spec prompt.rs (NamedResponseFormat, check_output_schema), authoring.rs (AuthoredPrompt compile/lowering), skald-prompt prompt.rs; wyrd-spec verification/execution.rs (Verdict::response_format, bounds, generic RequirementResult decoding), schema.rs (schemars generation).
- Declaration: wyrd-spec card/verifier/{mod.rs,validate.rs,extract.rs,reference.rs}, card/prompt; schema_path::fits is the existing schema compatibility owner. Inspect code-check and inline/referenced Agent/Prompt siblings.
- Execution: vala-verify blocks.rs (verdict_from), blocks/agent.rs and engine.rs; wyrd-server verification/{engines.rs,hosted/sandbox.rs,results.rs}, scribe_outbox.rs. Follow invalid output through requirement outcome and persisted result.
- SDK readers: shared/wyrd-client observe/mod.rs and verify/execution.rs; Python src/{observe/mod.rs,verify.rs,state/mod.rs} and authored stubs; TS wyrd/src/index.ts typedResult and typed operation siblings; Rust SDK re-exports.
- Schema migration consumers: workspace Cargo manifest/lock, all changed schemars implementations across wyrd-spec/shared/skald-spec; gen_schemas.rs, scripts/gen_ts_types.py, gateway/openai.rs, server/mcp/principals.rs, generated card-types.ts, docs diagrams test. Follow $defs, const and root $ref handling through downstream consumers.
- Proof: SDK verifier_building_blocks journeys (Rust/Python/TS), scored-review/scored-judge/plain-judge fixtures, blocks_agent integration, server pg_verification_routes/runtime, MCP verification tests, inline unit and TS parser-error table, typechecks.
- Public authority/docs: architecture/wyrd-design.md Prompt/Verifier/client/result sections; architecture/bifrost-design.md verification results and storage; security posture tenant/payload boundaries; verifier-card reference, verifier-recipes guide, support-desk policy judge fixtures.
- Rules: root AGENTS.md; nested docs/AGENTS.md, sdks/wyrd-sdk-python/AGENTS.md, sdks/wyrd-sdk-ts/AGENTS.md; agent_docs/{index,rust,errors,agent-surfaces,maintainer-style}.md; TESTING.md and relevant mise tasks. No SQL source changed.

## Review process

Seven independent discovery roles: behavior, invariants, repository standards, maintainer, resilience, security/privacy, persistent data. Then compare claims, conditional fresh follow-up, and mandatory fresh Ponytail validation. Source is immutable; only reports may be written in this review directory. Reviewers read their own cumulative diff. Do not consume other new discovery reports or implementation chat summary. Do not modify source or run mutating formatting/codegen commands. Record verification limits candidly.
