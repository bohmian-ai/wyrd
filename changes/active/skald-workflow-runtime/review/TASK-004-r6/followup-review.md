# Focused follow-up review — TASK-004 r6

## Subject and scope

- Repository: `/home/thorrester/Documents/GitHub/wyrd`
- Base: `b90335e0991438e8c28b65ed9c0c4cd80f9b0d56`
- Candidate: `5f3b521b5005c26277d53e7dfd2458c4f740e8be`
- Approved authority: `changes/active/skald-workflow-runtime/spec.md`, Revision 14
- Original task: `changes/active/skald-workflow-runtime/tasks/TASK-004-accepted-server-jobs.md`
- Remediation history: TASK-004 R1 through R5
- Review mode: source, immutable Git objects, existing review reports, and recorded evidence only. No build, test, formatter, linter, generator, Cargo, or `mise` command was run.

The candidate resolved to the requested commit before this review and again after the report was written. The repository has no `.codegraph/` directory, so direct source and Git reads were used. The fixed human decisions in the review request were treated as authority and were not reopened.

This follow-up resolves only the four conflicts assigned by the orchestrator. It does not vote on unrelated discovery findings.

## Source paths inspected

- `changes/active/skald-workflow-runtime/spec.md`, especially Revision 14
- `changes/active/skald-workflow-runtime/tasks/TASK-004-accepted-server-jobs.md`
- `changes/active/skald-workflow-runtime/review/TASK-004-r5/TASK-004-R5-close-compatible-route-and-deadline-proof-gaps.md`
- `changes/active/skald-workflow-runtime/review/TASK-004-r6/{task-review-behavior,task-review-invariants,standards-review,maintainer-review,system-review,domain-review-provider-contract}.md`
- `AGENTS.md` section 16
- `architecture/agent-rules.md`
- `architecture/references/languages/maintainer-style.md`
- `crates/skald/skald-spec/src/{prompt,error,response}.rs`
- `crates/skald/skald-spec/src/authoring.rs`
- `crates/skald/skald-prompt/src/{builder,prompt}.rs`
- `crates/skald/skald-prompt/src/python/response.rs`
- `crates/skald/skald-agent/src/loop_runtime.rs`
- `crates/skald/skald-cache/src/{lib,key,inmemory}.rs`
- `crates/skald/skald-runtime/src/runtime.rs`
- `crates/vala/vala-eval/src/orchestrator/judge.rs`
- `crates/wyrd-spec/src/error.rs`
- `sdks/wyrd-sdk-python/tests/unit/cards/prompt/{test_prompt_media,test_prompt_media_binding}.py`
- Base-to-candidate diffs and base versions of the materially changed provider, Prompt, cache, response, Agent telemetry, and builder symbols

## A. GenerateContent media binding and Vertex attribution

**Resolution: confirmed as a reachable regression. Retain `DOMAIN-PROVIDER-R6-1`.**

Revision 14 makes the request variant identify the wire schema and moves native dispatch destination to `Prompt.provider`. The current Vertex authoring paths establish exactly that representation: `skald-prompt/src/builder.rs:240-244,273-307` builds `ProviderRequest::GeminiGenerateContent` and sets `provider: Some(ProviderName::Vertex)`; the Python `Prompt.vertex` path delegates to that builder; `PromptDraft` has the same destination-bearing shape. `Prompt::provider()` at `skald-spec/src/prompt.rs:165-173` is the established effective-destination owner.

The media path is reachable independently of provider dispatch:

1. Rust callers can call `Prompt::bind_media`/`bind_media_mut` directly.
2. Python exposes the same operations at `skald-prompt/src/prompt.rs:886-904`; the current Python tests construct a Vertex Prompt and bind native media.
3. Vala's real Judge path clones an authored Prompt and calls `bind_media_mut` for each resolved record-media binding at `vala-eval/src/orchestrator/judge.rs:94-111,190-199`.

At `skald-spec/src/prompt.rs:244-258`, the folded `GeminiGenerateContent` arm unconditionally passes `ProviderName::Google` to `bind_media_google`. An ordinary HTTPS media URL reaches `build_google_part` at `prompt.rs:910-936`, which returns `UnsupportedMediaForProvider` carrying that supplied provider. The Wyrd error conversion preserves the provider in structured details at `wyrd-spec/src/error.rs:4232-4238`. Thus a Vertex-targeted Prompt reports `Google` in a stable, user-visible error.

This is not merely terminology about a shared dialect. The base implementation had separate Google and Vertex request arms and passed `ProviderName::Google` and `ProviderName::Vertex`, respectively. The error itself says that the *selected provider* does not support the media and documents its field as the provider that rejected it (`skald-spec/src/error.rs:71-77`). Folding the request schema removed the Vertex branch without moving its destination input to the new owner. Current proof covers successful Vertex base64 replacement but not the provider-bearing Vertex refusal; the Gemini refusal asserts only the stable code at Python level.

The smallest correction boundary is the existing `Prompt::bind_media_mut` owner: obtain the effective destination through `Prompt::provider()` before taking the mutable request borrow and pass that destination to the existing GenerateContent media helper. Do not restore a Vertex request variant, add a second binder, special-case Python, or add a new mechanism. Focused closure should prove that the same unsupported GenerateContent media reports Vertex for a Vertex Prompt and Google for an ordinary Gemini Prompt while retaining the existing successful replacement cases.

## B. OpenAI Chat cache scope after destination moved to Prompt

**Resolution: confirmed as a public cache-contract regression. Retain `DOMAIN-PROVIDER-R6-2`, with the correction narrowed to replacing the insufficient request-only derivation boundary rather than adding a parallel compatibility path.**

`CacheKey` is a public `skald-cache` primitive re-exported from `skald-cache/src/lib.rs`. Its declared invariant is deterministic identity scoped by provider, model, and native content, and its `provider` field explicitly prevents same-hash collisions across APIs (`skald-cache/src/key.rs:14-22`). The base `CacheKey::from_request` preserved that invariant for `OpenAiChatCompatible` by reading the custom provider embedded in the old request variant.

The candidate changed this same public derivation owner as part of Revision 14. Its only Chat arm now always supplies `ProviderName::OpenAi` (`key.rs:58-64,91-105`). A Revision 14 custom Prompt carries an ordinary `OpenAiChatCompletion` body plus `Prompt.provider = Some(Custom(...))`; passing its request to the public derivation operation therefore yields the same provider, model, and prefix hash as an OpenAI Prompt with equal cache material. Two distinct custom destinations likewise collide. A caller can then use the resulting key through the public `PromptCache`/`InMemoryCache` operations. The observable risk is concrete: a provider-owned cached resource placed under one destination's key can be returned for another destination.

No current workspace production caller invokes `CacheKey::from_request`; existing in-tree invocations are focused tests, and `SkaldRuntime` exposes the cache handle without deriving keys. That limits present internal exposure but does not make the path dormant or speculative: the changed symbol is the public derivation API for this public cache crate, the collision is produced by an ordinary direct call, and Revision 14 expressly changed the representation of custom OpenAI-compatible Prompts while requiring the custom destination to remain meaningful. The R5 evidence also expressly lists `skald-cache/src/key.rs` among the Revision 14 match-arm changes. Source inspection, rather than a synthetic harness, is enough to show that a request-only input cannot recover a destination which the approved contract says is no longer in the request.

The source correction belongs at cache-key derivation, where provider scope is owned. Replace the destination-sensitive request-only derivation boundary with one that receives the effective Prompt destination (preferably the existing `Prompt` owner, or an explicit already-resolved provider together with the request). Do not keep a second custom-unsafe convenience path, recreate `OpenAiChatCompatible`, embed destination in the wire request, or add an adapter/checker. Focused owner-level proof should use otherwise identical OpenAI Chat Prompts with different effective destinations and assert distinct provider scopes/keys, while preserving the default OpenAI and Anthropic cases.

## C. Response and Agent telemetry documentation

**Resolution: confirmed as materially misleading changed documentation. Retain `MAINT-R6-1`.**

The two cited blocks are both reachable and now state the opposite side of Revision 14's schema/destination split:

- `ProviderResponse::provider` is public (`skald-spec/src/response.rs:95-112`) and is projected by the public Python response wrapper. After deletion of `VertexGenerateContent`, a response to a Vertex GenerateContent call is represented by `GeminiGenerateContent`, whose `provider()` result is necessarily Google. The unchanged rustdoc at line 101 says this is the provider that produced the response. The candidate's Python wrapper already documents the correct meaning as the response dialect's default provider and explicitly notes that a Vertex body reports Google (`skald-prompt/src/python/response.rs:36-43`). The Rust statement is therefore demonstrably false, not merely incomplete.
- `Agent::invoke_agent_span` is used for every `run_prompt` call. Its implementation was changed to record `prompt.provider()` at `skald-agent/src/loop_runtime.rs:663-670`, but its rustdoc still says the semantic provider name is derived from the typed request. For custom Chat and Vertex GenerateContent, those two values intentionally differ under Revision 14.

`AGENTS.md` section 16 requires materially modified Rust documentation to describe the operation and its relevant invariants, and treats incorrect or missing documentation as a hard blocker. The smallest correction is documentation only: describe `ProviderResponse::provider()` as the response schema/dialect default and say that the shared GenerateContent response does not retain Gemini-versus-Vertex destination; describe the Agent span field as the Prompt's effective native dispatch target. No response field, wrapper, telemetry fallback, test harness, or API rename is needed.

## D. Prompt construction documentation and destination preservation

**Resolution: confirmed as a repository-rule violation distinct from the runtime findings. Retain `MAINT-R6-2`, as one documentation finding covering the shared construction seam.**

The governing rule is explicit: `AGENTS.md:716-730` and `architecture/agent-rules.md` require substantive rustdoc for every new or materially modified Rust item regardless of visibility, a `# Errors` section on every fallible function, and documentation of non-obvious invariants needed for safe maintenance.

The cited symbols were materially changed at Revision 14's central construction seam:

- `skald_spec::Prompt::new` now creates the new `provider: None` state before fallible media normalization (`skald-spec/src/prompt.rs:142-163`). Its public docs have no `# Errors` section.
- Public `builder::vertex` now returns the shared `GeminiGenerateContent` schema with Vertex stored as the dispatch destination through the changed `google_prompt` helper (`skald-prompt/src/builder.rs:240-244,273-307`). Its one-line docs neither describe that non-obvious invariant nor name its model/settings/normalization failures.
- Private, fallible `google_prompt` was materially changed to encode destination separately from schema and remains undocumented (`builder.rs:273-308`).
- Private, fallible `finalize_prompt` was materially changed to save `prompt.provider`, call `Prompt::new` (which resets it), and restore it (`builder.rs:310-324`). It remains undocumented. Without an explanation, the save/restore looks redundant even though deleting it silently retargets Vertex to Google whenever the no-explicit-variables branch is taken.

These omissions are material under the repository's hard rule and the preservation invariant is observable through ordinary public Vertex construction. They do not duplicate the media or cache findings: those are incorrect runtime consumers of effective destination; this finding concerns the documentation required to keep the correct construction source from being accidentally simplified away.

The smallest correction is documentation only on the four materially changed symbols: state the shared GenerateContent schema versus Vertex destination contract, document finalization's preservation of the optional destination across `Prompt::new`, and add exact `# Errors` sections for existing failure conditions. Do not introduce a new builder type, setter, constructor, checker, harness, or behavioral change. Existing construction, round-trip, and dispatch proof remains the behavioral evidence; normal format/lint evidence is sufficient for the documentation-only closure.

## Relationship among the four uncertainties

All four were exposed by Revision 14 moving destination identity out of request/response wire variants, but they require three distinct correction boundaries:

1. `Prompt::bind_media_mut` must consume the existing effective destination for provider-bearing media errors.
2. `skald-cache` key derivation must receive the effective destination because a request alone no longer contains it.
3. Existing docs at the response/telemetry and Prompt-construction seams must accurately preserve the schema-versus-destination contract.

The two documentation proposals can be packaged together as documentation remediation, but neither is a substitute for the two runtime corrections. Conversely, fixing media or cache behavior does not correct the false/missing documentation. No proposed resolution requires a new product decision, compatibility route, mechanism, check, setting, option, dependency, or harness. Restoring deleted variants or adding parallel compatibility APIs would be DRIFT.

## New proposed findings

None. The four assigned discovery proposals are resolved as follows:

| Discovery ID | Resolution |
|---|---|
| `DOMAIN-PROVIDER-R6-1` | Confirmed; reachable Vertex error-attribution regression at the Prompt media owner. |
| `DOMAIN-PROVIDER-R6-2` | Confirmed; reachable public cache-key collision contract after destination moved out of the request. |
| `MAINT-R6-1` | Confirmed; two materially false stale rustdoc blocks. |
| `MAINT-R6-2` | Confirmed; one shared Prompt-construction documentation violation covering four materially changed fallible symbols. |

## Verification limits

- No commands that build, test, format, lint, generate, or modify production code were run.
- R5 records broad final-tree lanes plus focused native custom-provider and Vertex dispatch tests. Those claims were checked against their named source paths and assertions.
- The recorded tests do not exercise the Vertex provider-bearing unsupported-media result or destination-distinct cache keys.
- Documentation correctness is established directly from the current implementations and Revision 14's approved terminology; a new test harness or repository checker would be unnecessary DRIFT.

## Overall result

**RESOLVED**

The conflicts are resolved from approved authority and reachable source paths. Both domain findings and both maintainer findings remain valid at distinct correction boundaries; no further discovery pass is required for these uncertainties.
