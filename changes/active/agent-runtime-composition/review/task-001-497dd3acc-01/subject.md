# TASK-001 immutable review subject

- Repository: `/home/thorrester/Documents/GitHub/wyrd-worktrees/agent-runtime-composition`
- Base: `06a368906187be20364a2d959da311151964b617`
- Candidate: `497dd3acc8fd06567a0e0ff01baf599f9af1711b`
- Branch at establishment: `change/agent-runtime-composition`
- Base is the parent of the first implementation commit `178129c6c`, to include that commit in the cumulative review.
- Initial tracked/untracked status: clean.
- Original task: `changes/active/agent-runtime-composition/tasks/TASK-001-delete-sdk-agent-runtime.md` as committed at base; inspect with `git show <base>:<path>`.
- Approved spec: `changes/active/agent-runtime-composition/spec.md`, revision 3, unchanged from base to candidate.
- Available verification evidence: candidate task file's Implementation Evidence section. Treat outcomes and diagnostics as claims, not proof. Do not use the conversation's implementation summary as review evidence.
- Read the actual complete diff with `git diff 06a368906187be20364a2d959da311151964b617...497dd3acc8fd06567a0e0ff01baf599f9af1711b` and expand surrounding source/callers independently.
- Review source is immutable. Do not edit implementation, spec, task, or authority. Each reviewer may write only its assigned report. Do not save diffs, patches, or source archives here.
- No Cargo/build/test commands concurrently. This is source/evidence review by default; ask orchestrator before any build or runtime verification. Lightweight read-only checks are permitted.

## Shared navigation map (starting points, not coverage or conclusions)

| Changed area | Symbols/owners | Consumer/test starting points |
|---|---|---|
| Python runtime removal | `sdks/wyrd-sdk-python/src/lib.rs` native registration; root exports/stubs; deleted `python/wyrd/agent`; three Skald crates' python feature/glue removal | Python public-export/parity tests, Prompt authoring/loading tests, removed Agent runtime/smoke tests, Python manifest and surviving framework integrations |
| Shared client execution removal | Deleted `observe/invoke.rs`, `gateway_caller.rs`; `lib.rs`, `state.rs`, `transport/http.rs`, dependency cleanup | Deleted client gateway/local-development tests; first-class SDK migrations; surviving shared transport/auth/correlation seams |
| Internal preservation | Skald `agent.rs`, `callbacks.rs`, `loop_runtime.rs`, `run.rs`, `session.rs` materially lose Python-only code | Vala Agent executor/simulator; server hosted tools and routes; SDK Verifier building-block and hydration journeys |
| Application-owned model calls | `examples/support-desk/{python,rust,typescript}`, `examples/support-desk-app/rust/lib.rs` | SDK support-desk tests; gateway inference and local-development journeys; native gateway handler and transport auth |
| Native/public removal parity | Python observe pymethod/stub; TS native `cards.rs`, public `index.ts`, generated `.d.ts`/`.d.cts`; Rust SDK docs/exports | Package/typecheck/generated evidence; surviving AgentCard/AgentSpec/Prompt navigation |
| Request/correlation contracts | Application request rendering, deployment/model selection, Run/Card/provider headers, response/error handling | Gateway-inference positive/negative cases in all languages; server subject authorization, provider dispatch, capture, upstream header stripping |
| Guidance/examples/build closure | Docs runs/observations page, gateway-setup README, Rust/Python examples/manifest, mise task removal, `wyrd-utils/src/py.rs` | Published doc links, example inclusion/compilation, deleted vs preserved runtime/Prompt tests |

## Governing authority

Read AGENTS.md, architecture/agent-rules.md, architecture/references/README.md,
architecture/references/languages/spec-driven-development.md,
architecture/references/languages/maintainer-style.md, approved spec and original task.
Select the minimum complete applicable references, including protocol/doctrine,
SDK/language and testing references. Security reviewer additionally reads security posture.
Use CodeGraph before locating/reading code where possible; bounded rg/source fallback
is appropriate when CodeGraph does not return the requested owner.

## Review contract

Acceptance audit only. No optional improvements, speculative hardening, unrelated debt,
or changes to the approved product boundary. All SDK runtime and Run invocation removal
must be assessed together with required consumer closure and preserved capabilities.
Classify findings MISSING/INCORRECT/DRIFT/VIOLATION/REGRESSION, with source-local ID,
exact candidate location, obligation, reachable consequence, evidence and smallest
safe correction. Rustdoc omissions on changed Rust items remain blocking per AGENTS.md.
Do not require arbitrary refactors, duplicate validators/transports, or new public APIs.
Mark verification claims independently inspected versus merely reported.
