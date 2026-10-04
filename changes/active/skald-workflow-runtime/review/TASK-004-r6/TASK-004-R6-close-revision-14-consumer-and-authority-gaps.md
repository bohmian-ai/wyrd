---
id: TASK-004-R6
kind: remediation
status: ready
spec: SPEC-skald-workflow-runtime
spec_revision: 14
parent_task: TASK-004
remediates: [FIND-TASK-004-17, FIND-TASK-004-24, FIND-TASK-004-25, FIND-TASK-004-26, FIND-TASK-004-27]
---

# Close Revision 14 consumer and active-authority gaps

Implementation skill: `$wyrd-implement`.

## Immutable review inputs

- Approved specification:
  `changes/active/skald-workflow-runtime/spec.md`, Revision 14
- Original task:
  `changes/active/skald-workflow-runtime/tasks/TASK-004-accepted-server-jobs.md`
- Prior remediations: TASK-004 R1 through R5 in their preceding review
  directories
- Reviewed base: `b90335e0991438e8c28b65ed9c0c4cd80f9b0d56`
- Reviewed candidate: `5f3b521b5005c26277d53e7dfd2458c4f740e8be`
- Validated diagnosis:
  `changes/active/skald-workflow-runtime/review/TASK-004-r6/findings-validation.md`

## Outcome

Complete TASK-004 under approved Revision 14 by making the active task packet
unambiguous, preserving the effective Prompt destination in the two remaining
provider-sensitive consumers, and documenting the schema-versus-destination
contract at the changed Rust owners. Preserve the already-correct single wire-
schema variants, native and gateway dispatch, server run lifecycle, prepared
deadline, query settlement, and all prior finding closures.

## Diagnoses and required corrections

### Active Revision 14 authority (`FIND-TASK-004-17`)

Revision 14 is approved and expressly carried by TASK-004. The implementation
and appended R5 evidence follow it, but the active TASK-004 frontmatter/current-
authority link and R5 governing frontmatter still identify Revision 13. R5 also
retains live outcome, diagnosis, and acceptance text requiring
`ProviderRequest::OpenAiChatCompatible`, followed by Revision 14 evidence that
correctly deletes it. An implementer can therefore follow the active packet and
restore behavior the approved specification prohibits.

Use the existing task-correction path and the R2 revision-alignment precedent.
Update TASK-004's current revision and authority link and R5's current governing
metadata to Revision 14. Make the R5 compatible-variant diagnosis and acceptance
rows explicitly superseded by the single OpenAI Chat schema plus optional
`Prompt.provider` destination. Preserve R5's still-applicable deadline proof,
its immutable historical Revision 13 review input, and every prior review
report's true subject identity.

Do not add a metadata schema, checker, file, compatibility path, setting, or
option. Do not rewrite historical review verdicts or pretend they reviewed a
revision that did not yet exist.

### Vertex media error attribution (`FIND-TASK-004-24`)

A Revision 14 Vertex Prompt is a shared `GeminiGenerateContent` request with
`Prompt.provider = Vertex`. Native dispatch correctly consumes the effective
Prompt destination, but `Prompt::bind_media_mut` passes Google unconditionally
to the existing GenerateContent media helper. For an unsupported HTTPS media
URL, the stable `UnsupportedMediaForProvider` error therefore reports Google
for a Vertex-targeted Prompt. Before the schema fold, the Vertex request arm
passed Vertex to the same helper. Public Rust and Python operations and Vala
Judge can reach this refusal path.

Correct the existing Prompt media owner so the shared GenerateContent branch
uses the Prompt's effective destination when constructing provider-bearing
errors. Reuse `Prompt::provider()` and the current GenerateContent media helper;
preserve all successful media replacement and Gemini behavior. Do not restore a
Vertex request variant, duplicate the binder, or add a Python special case.

### Provider-scoped cache identity (`FIND-TASK-004-25`)

`CacheKey` promises provider-scoped deterministic identity. Before Revision
14, the compatible Chat request carried its custom provider and key derivation
used it. Revision 14 correctly moved the destination to `Prompt`, but the
remaining request-only derivation can observe only the OpenAI Chat schema
default. Otherwise identical Chat requests for OpenAI and custom endpoints can
therefore produce equal provider scopes and keys, and those keys feed the
public cache `get`, `put`, and `invalidate` operations directly.

Replace the insufficient request-only derivation boundary with one unambiguous
cache-owner operation that receives the effective Prompt destination—prefer the
existing `Prompt` owner, or an already-resolved provider together with its
request. Use that destination for Chat provider scope while retaining current
request inspection for provider-native cache directives. Remove rather than
preserve a custom-unsafe request-only alternative.

Do not recreate `OpenAiChatCompatible`, move destination back into the wire
request, add a registry or adapter, keep two key-derivation APIs, invent a
compatibility promise, or add a setting/checker/synthetic caller.

### Schema and destination documentation (`FIND-TASK-004-26`)

`ProviderResponse::provider()` now reports the shared response schema's default
provider. A GenerateContent response therefore reports Google even when Vertex
produced it, because the response does not retain the destination. Its rustdoc
still claims it returns the producing provider. Separately,
`Agent::invoke_agent_span` now records `Prompt::provider()` but its rustdoc
still says the value is derived from the typed request. Both descriptions
reverse Revision 14's schema/destination distinction.

Correct only those two rustdoc blocks. Describe the response operation as the
schema/dialect default and state that a shared GenerateContent response does
not retain Gemini-versus-Vertex destination. Describe the Agent span value as
the Prompt's effective native dispatch target. Reuse the terminology already
used by `ProviderRequest::provider()`, `Prompt::provider()`, and the Python
response projection. Add no response field, wrapper, telemetry fallback,
checker, or test harness.

### Prompt construction documentation (`FIND-TASK-004-27`)

`Prompt::new`, public `builder::vertex`, and private `google_prompt` and
`finalize_prompt` were materially changed at Revision 14's construction seam.
The fallible operations omit the repository-required `# Errors` documentation;
the private helpers omit substantive rustdoc. Most importantly,
`finalize_prompt` must preserve and restore the optional destination across
`Prompt::new`, which intentionally initializes it as absent. Without that
contract, the preservation can appear redundant and a maintenance edit can
silently retarget Vertex to Google.

Add substantive rustdoc and accurate `# Errors` sections to those four
existing items. Explain that GenerateContent is the shared body schema,
`Prompt.provider` is the optional dispatch destination, and finalization
preserves that destination across existing normalization. Document only the
existing failure conditions. Do not change behavior, introduce another
constructor or builder abstraction, add a setter, or create a documentation
check or harness.

## Preserved behavior and non-goals

- Preserve closure of `FIND-TASK-004-1` through `-16` and `-18` through
  `-23`.
- Preserve one OpenAI Chat request schema, one GenerateContent request schema,
  optional Prompt destination, Vertex Predict as a distinct schema, and no
  migration or compatibility reader.
- Preserve native custom-provider and Vertex dispatch, gateway routes ignoring
  `Prompt.provider`, external protocol matching by request schema, server
  Vertex gateway projection, and the unchanged local Wyrd-client Vertex
  refusal.
- Preserve the single prepared-run deadline owner, one-time `RunTools` bind,
  shorter query-deadline precedence, tracked settlement, cancellation,
  terminal compare-and-set, and capacity release ordering.
- Preserve authorization, audit, tenant isolation, captured run authority,
  Cards and Bifrost object checks, secrets, SSRF screening, snapshots,
  attempts, retention, and shutdown behavior.
- Keep supervisor drain polling and idle refusal deleted. Follower release is
  grant-stream close, and the leader awaits no acknowledgement.
- Keep the foreign-tenant harness unchanged. Published `Running` reserves
  attempt one, and interrupted published work settles `Cancelled` with its
  timestamps.
- Add no new wire variant, public field, compatibility route, migration,
  lifecycle owner, provider registry, cache adapter, query engine, dependency,
  repository check, fixture system, harness, setting, option, polling loop, or
  acknowledgement protocol.

## Acceptance criteria

| Finding | Closure criterion |
|---|---|
| `FIND-TASK-004-17` | TASK-004 and R5 identify Revision 14 as current authority; no live R5 criterion requires the deleted compatible request variant; R5's deadline proof and immutable historical subject remain intact. |
| `FIND-TASK-004-24` | A Vertex-targeted GenerateContent Prompt reports `vertex` in the existing stable unsupported-media error, while the equivalent Gemini Prompt reports `google`; successful media binding is unchanged. |
| `FIND-TASK-004-25` | Otherwise identical cacheable Chat Prompts targeting OpenAI and distinct custom providers derive distinct provider scopes and keys through one cache-owner operation; default OpenAI and Anthropic behavior remains intact. |
| `FIND-TASK-004-26` | Rust response and Agent telemetry documentation accurately distinguish schema/dialect default from effective Prompt destination and matches the current implementations and Python projection. |
| `FIND-TASK-004-27` | All four materially changed construction items have substantive rustdoc and accurate `# Errors`; finalization documents destination preservation across normalization without behavioral changes. |
| All | No prohibited compatibility, checker, adapter, registry, setting, option, harness, polling, acknowledgement, or duplicate API is introduced, and prior finding closures remain intact. |

## Focused and broader proof

Extend the existing Prompt media and cache-key unit-test homes; do not add a
test target, fixture system, or harness. The focused tests must directly prove
the two missing outcomes:

```bash
mise exec -- cargo nextest run --locked -p skald-spec --lib \
  -E 'test(=prompt::prompt_media::google_media_matrix_and_rejections)'

mise exec -- cargo nextest run --locked -p skald-cache --lib \
  -E 'test(=key::cache_key::key_hash_distinct_for_different_scopes)'
```

Source review must also prove the active Revision 14 task chain is
unambiguous, the two corrected rustdoc blocks match implementation, all four
construction items meet the repository documentation contract, and historical
review inputs remain accurate.

Then run the narrow existing broader gates for the touched Rust, contract, and
generated-facing surfaces:

```bash
mise run fmt
mise run lints
mise run test:skald
mise run codegen:check
mise run check:pyo3-scope
mise run py:typecheck
```

The implementation report must name the corrected media and cache owners,
show the Vertex/Gemini error identities and destination-distinct cache keys,
identify the authority text superseded by Revision 14, record the exact focused
commands and broader gate results, and confirm that no prohibited mechanism or
unrelated behavior entered the diff.
