# TASK-001 review subject

- Repository: `/home/thorrester/Documents/GitHub/wyrd`
- Base: `4d0f7005888a899910008399d1b5e15e2a3207f9` (parent of first implementation commit).
- Candidate: `a71156ea4c26d274e96f3f73a64442ff1c22aae4`.
- Complete range: `git diff 4d0f7005888a899910008399d1b5e15e2a3207f9...a71156ea4c26d274e96f3f73a64442ff1c22aae4`.
- Approved authority: `changes/active/verifier-response-data/spec.md`, revision 1.
- Original task and available verification evidence: `changes/active/verifier-response-data/tasks/TASK-001-verdict-formats-and-custom-data.md` (evidence at end).
- No prior task-review verdict or remediation in this packet.
- Pre-existing local modifications: `.agents/skills/wyrd-plan/SKILL.md`, `.agents/skills/wyrd-spec/SKILL.md`, `changes/spec-driven-development.md`. These are outside the candidate; do not modify them. Read current workflow instructions and use committed candidate for acceptance subject.
- Source is immutable. Write only the assigned report. Do not implement, commit, or run broad test aggregates. Coordinate any Cargo execution with orchestrator.

During discovery the user confirmed they are actively updating the working copy after upgrading `ty`. Preserve all of those edits. HEAD remains the candidate; the review subject is the committed range, not the evolving working tree. For any file differing from the candidate, read `git show a71156ea4c26d274e96f3f73a64442ff1c22aae4:<path>` rather than treating current disk content as reviewed source. Existing discovery was performed before these later edits where possible; final independent validation must check committed source explicitly.

## Shared navigation map (starting points, not exhaustive coverage)

| Path | Owners and callers | Tests/consumers |
| --- | --- | --- |
| `crates/skald/skald-spec/src/{authoring,prompt}.rs`, `crates/skald/skald-prompt/src/prompt.rs` | `ResponseFormat`, `NamedResponseFormat`, `AuthoredPrompt::compile_with`; Prompt lowering and rendering; host resolves schema | authoring tests, PromptSpec/AgentSpec, Python Prompt authoring |
| `crates/wyrd-spec/src/card/{prompt,agent.rs,verifier}` | PromptSpec/AgentSpec compilation; VerifierValidator checks code and Agent contracts, whole/step requirements | verifier validation tests, HTTP declaration journey |
| `crates/wyrd-spec/src/verification/execution.rs` | `Verdict::response_format`, `check_data_bounds`, generic result/outcome/requirement projections, `decode_data`; error catalog | bounds/typed-data tests, all SDK result projections |
| `crates/vala/vala-verify/src/blocks{.rs,/agent.rs,/engine.rs}` | `verdict_from`, Agent output and `Judged` reporting; server hosted sandbox is code caller | blocks_agent integration tests; building-block fixtures |
| `crates/wyrd/wyrd-server/src/verification/{hosted/sandbox.rs,results.rs,engines.rs}`, `src/scribe_outbox.rs` | code execution, result payload construction and durable outcome; existing HTTP/MCP/Bifrost access | pg_verification_routes; MCP verification; durable results/readback |
| `crates/shared/wyrd-client/src/{observe/mod.rs,verify/execution.rs}` | typed siblings reuse verify/status/wait; Rust SDK projects shared types | Rust verifier_building_blocks journeys |
| `sdks/wyrd-sdk-python/src/{observe/mod.rs,verify.rs}`, `python/wyrd/{observe,verify,prompt}/*.pyi` | Python decoder/model_validate, typed getters and authored stubs | Python journeys, verify_contracts typecheck, prompt authoring unit |
| `sdks/wyrd-sdk-ts/wyrd/src/{index,card-types,error-codes}.ts` | parse decoder and generic facade over existing native handles; generated wire contracts | TypeScript journeys and verification-data.test-d.ts |
| fixtures, support-desk examples, docs and architecture | named/plain/custom authored Cards, public examples, Prompt/Verifier and Variant authority | check:examples, docs and codegen evidence |

## Governing references

Read root `AGENTS.md`, nested SDK and docs AGENTS for touched surfaces, `.agents/skills/wyrd-task-review/SKILL.md` for your exact role, `changes/spec-driven-development.md`, `agent_docs/maintainer-style.md`, relevant topic guides (`rust`, `errors`, `agent-surfaces`), `TESTING.md`, and architecture authority map. Owning sections include `wyrd-design.md` Client model, Prompt and Verifier; `bifrost-design.md` Storage format and Variant plus result publication; `wyrd-security-posture.md` permissions/payload access and Tenant and data isolation. System reviewer additionally selects operational topology/recovery references.

`.codegraph/` exists: use `mise exec -- codegraph explore "<question or symbols>"` before code searches/reads. Current source comes from CodeGraph; immutable diff from Git. Expand caller and alternative-owner coverage independently.

## Required domain audits

- Security/tenancy: exact declared-output validation, diagnostics privacy, result/typed/published reads, existing permissions and tenant binding.
- Persistent data: result propagation through whole/step checks, direct/queued execution, retention/publication and durability; no new store/schema expected.

Each discovery reviewer receives this same map without other reviewers' conclusions. Reports must cover the complete range within their role and include exact evidence, material findings with local IDs, verification limits, and overall PASS/FAIL/BLOCKED. Implementation reviewers additionally include the full acceptance matrix. Do not elevate optional improvements or unrelated existing debt.
