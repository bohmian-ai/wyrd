# TASK-004 r6 structured Ponytail validation

## Immutable subject and method

- Repository: `/home/thorrester/Documents/GitHub/wyrd`
- Base: `b90335e0991438e8c28b65ed9c0c4cd80f9b0d56`
- Candidate: `5f3b521b5005c26277d53e7dfd2458c4f740e8be`
- Approved authority: `changes/active/skald-workflow-runtime/spec.md`, Revision 14
- Original task:
  `changes/active/skald-workflow-runtime/tasks/TASK-004-accepted-server-jobs.md`
- Remediations: TASK-004 R1 through R5 in their preceding review directories

The candidate resolved to the requested commit before validation. `.codegraph/`
is absent, so validation used immutable Git objects, the complete cumulative
file inventory and relevant diffs, repository search, and direct source and
caller inspection. Per the review direction, no build, test, Cargo, `mise`,
formatter, linter, code-generation, package-manager, or production-source edit
was run.

I read and validated every proposal in:

- `task-review-behavior.md`;
- `task-review-invariants.md`;
- `standards-review.md`;
- `maintainer-review.md`;
- `system-review.md`;
- `domain-review-concurrency-lifecycle.md`;
- `domain-review-security-tenancy.md`;
- `domain-review-query-settlement.md`;
- `domain-review-provider-contract.md`; and
- `followup-review.md`.

The applicable authority included `AGENTS.md`, `architecture/agent-rules.md`,
the spec-driven-development and maintainer-style references, the approved
specification, the original task, R1-R5, and the applicable Wyrd, security,
Skald, and Bifrost architecture. The fixed human decisions were treated as
authority and not reopened. In particular, validation did not require a
supervisor poll, idle refusal, follower release acknowledgement, changed
foreign-tenant harness, different attempt settlement, another deadline owner,
restored provider wrapper, compatibility path, or changed local Vertex
behavior.

## Proposed-finding validation

| Discovery proposal | Validation | Source-backed disposition |
|---|---|---|
| `BEHAVIOR-R6-001`, `INV-R6-001`, `STANDARDS-R6-001` | Revision 14 is approved and expressly carried by TASK-004, while the active original task and active R5 remediation still identify Revision 13. R5 additionally retains live outcome, diagnosis, and acceptance text requiring the now-deleted compatible request variant, followed by contrary Revision 14 evidence. This is the same authority-chain invariant previously assigned `FIND-TASK-004-17`, not a new defect family. | **REVISED** and deduplicated by reopening `FIND-TASK-004-17`. Correct the current task/remediation authority and active acceptance text while preserving immutable historical review subjects. |
| `DOMAIN-PROVIDER-R6-1` | Vertex authoring produces a shared `GeminiGenerateContent` request plus `Prompt.provider = Vertex`. `Prompt::bind_media_mut` nevertheless supplies `Google` to the provider-bearing unsupported-media error. Rust, Python, and Vala Judge callers reach this path, and the base Vertex arm supplied `Vertex`. | **CONFIRMED** as `FIND-TASK-004-24`. |
| `DOMAIN-PROVIDER-R6-2` | `CacheKey` promises provider-scoped identity. Before Revision 14, the compatible Chat arm supplied its custom destination. The changed request-only derivation now necessarily supplies OpenAI because the destination moved to `Prompt`; equal cache material therefore collides across OpenAI and custom targets. The operation is a public workspace API and directly feeds the public cache backend even though no current production caller derives keys. | **REVISED** as `FIND-TASK-004-25`. The unsafe request-only derivation boundary must be replaced, not supplemented with a second convenience path. |
| `MAINT-R6-1` | `ProviderResponse::provider()` now reports the shared response schema's default provider, but its rustdoc still claims it returns the producer. `Agent::invoke_agent_span` now records `Prompt::provider()` but still claims the value comes from the typed request. Both items and their contract were materially changed, and the Python response documentation already states the correct distinction. | **CONFIRMED** as `FIND-TASK-004-26`. |
| `MAINT-R6-2` | `Prompt::new`, `builder::vertex`, `google_prompt`, and `finalize_prompt` are materially changed fallible items at the schema/destination construction seam. They omit required `# Errors` documentation; the private helpers also omit substantive rustdoc, including the reason `finalize_prompt` must preserve and restore the destination across `Prompt::new`. | **CONFIRMED** as `FIND-TASK-004-27`. |

The other discovery reports proposed no material findings. Their PASS results
were not used as proof of the proposals above. The focused follow-up resolved
the four assigned source uncertainties, and independent validation reproduced
each reachable path before retaining it.

## Producer-to-consumer validation

### Active Revision 14 authority

1. `spec.md:1-54,2760-2764` marks Revision 14 approved, defines the single
   Chat and GenerateContent schema variants plus optional Prompt destination,
   and expressly assigns the change to TASK-004.
2. `TASK-004-accepted-server-jobs.md:1-7,433-439` remains a `ready` active task
   with `spec_revision: 13` and an “Approved Revision 13” current-authority
   link.
3. The R5 remediation remains `ready` with `spec_revision: 13`. Its immutable
   review-input section is correctly historical, but its current outcome,
   diagnosis, and acceptance rows still require
   `OpenAiChatCompatible`, while its appended Revision 14 evidence records that
   variant's deletion.
4. The task contract in the spec-driven-development authority requires active
   tasks to identify the approved revision and permits `superseded` when a
   later approved revision invalidates task instructions. R2 previously closed
   this exact invariant as `FIND-TASK-004-17` when Revision 13 superseded
   Revision 12.

The current packet therefore presents mutually exclusive live acceptance
instructions. Reopening `FIND-TASK-004-17` preserves stable identity for the
same recurring authority defect. The correction belongs only in current task
authority and active task text; immutable r1-r5 review reports and historical
input identities remain accurate evidence and must not be relabelled.

### Vertex media error attribution

1. Revision 14 authoring in `skald-prompt/src/builder.rs:240-324` and
   `skald-spec/src/authoring.rs` produces
   `ProviderRequest::GeminiGenerateContent` with
   `Prompt.provider = Some(ProviderName::Vertex)`.
2. `Prompt::provider()` at `skald-spec/src/prompt.rs:165-173` is the established
   effective native-destination owner. Native Agent and Workflow dispatch
   consume that value.
3. Public `Prompt::bind_media` and `bind_media_mut` reach
   `prompt.rs:232-269`; Python projects both operations, and Vala Judge calls
   `bind_media_mut` for resolved record media.
4. The shared GenerateContent arm unconditionally passes
   `ProviderName::Google` to `bind_media_google`. An ordinary unsupported HTTPS
   URL reaches `build_google_part` at `prompt.rs:910-948`, which embeds that
   value in `UnsupportedMediaForProvider`; the stable Wyrd error conversion
   preserves it in user-visible details.
5. The base had separate Google and Vertex request arms and passed the matching
   destination to the same helper. The fold removed the Vertex variant but did
   not move this consumer to the new destination owner.

The error concerns the selected provider, not merely the shared body dialect.
The current successful Vertex base64 proof cannot detect the wrong provider on
the refusal branch.

### Provider-scoped cache identity

1. `skald-cache/src/key.rs:14-23` defines `CacheKey` as deterministic identity
   scoped by provider and says that scope prevents cross-API collisions.
2. At the base, `CacheKey::from_request` passed the custom provider carried by
   `OpenAiChatCompatible` into `openai_cache_key`.
3. Revision 14 intentionally moved that destination out of
   `ProviderRequest` and into `Prompt`. The changed Chat arm at `key.rs:58-64`
   now always passes `ProviderName::OpenAi`.
4. Two otherwise identical Chat Prompts targeting OpenAI and a custom endpoint,
   or two custom endpoints, therefore yield the same provider/model/prefix
   identity when their request is supplied to the current operation.
5. `CacheKey`, `PromptCache`, and `InMemoryCache` are public workspace
   primitives. A derived key is accepted directly by `get`, `put`, and
   `invalidate`, so the collision can return a resource owned by another
   provider scope. `skald-cache` is not published and no current production
   caller invokes the derivation method, which narrows present exposure but
   does not make the materially changed public operation unreachable.

The request alone can no longer satisfy the type's existing provider-scope
contract. Keeping it and adding another correct operation would preserve an
obvious unsafe path and create two ways to derive a key. The correction must
replace that boundary with the existing `Prompt` effective destination (or the
already-resolved provider together with its request), while leaving native
cache-directive extraction in the cache owner.

### Schema/destination documentation

1. `ProviderResponse::GeminiGenerateContent` now represents responses to both
   Gemini and Vertex GenerateContent calls. No response field retains the
   dispatch destination. `ProviderResponse::provider()` consequently returns
   Google for that variant, while its rustdoc says “the provider that produced
   this response.” The Python wrapper already documents the correct dialect
   default.
2. `Agent::invoke_agent_span` is called for `run_prompt` and now records the
   Prompt's effective destination through `prompt.provider()`. Its rustdoc
   still says the value is derived from the typed request, which is false for
   both custom Chat and Vertex GenerateContent Prompts.
3. `Prompt::new` now initializes the optional destination before normalization
   but lacks `# Errors`. Public `builder::vertex` now builds a shared
   GenerateContent body targeted through `Prompt.provider` but documents
   neither that invariant nor its errors.
4. Changed private `google_prompt` encodes the same split and has no rustdoc or
   `# Errors`. Changed private `finalize_prompt` saves the destination, calls
   `Prompt::new` (which intentionally defaults it to `None`), then restores it;
   it has no documentation explaining that required preservation or its
   failures.

These are hard repository documentation violations under `AGENTS.md` section
16 and `architecture/agent-rules.md`, not optional prose improvements. They do
not justify a response wrapper, destination field, telemetry fallback, builder
layer, checker, or test harness.

## Ponytail ladder and correction boundaries

### Reopened authority chain

1. **Delete:** delete or supersede only active Revision 13 instructions that
   conflict with Revision 14; do not rewrite historical review evidence.
2. **Reuse:** use the existing task front matter, authority link, and task
   correction path. R2 is the repository precedent for this same revision
   advance.
3. **Native/dependency:** no mechanism or dependency applies.
4. **Minimum:** align TASK-004 and R5's current authority with Revision 14 and
   make R5's old compatible-variant outcome/criteria explicitly superseded,
   while preserving its still-applicable deadline proof and immutable input.

No metadata schema, checker, file, setting, option, or compatibility mechanism
is warranted.

### Media attribution

1. **Delete:** the public media operation and its stable error cannot be
   deleted without regressing existing Prompt behavior.
2. **Reuse:** `Prompt::provider()` already computes the correct destination;
   `bind_media_google` already accepts the provider used by the error.
3. **Native/dependency:** ordinary local value capture before the mutable
   request borrow is sufficient.
4. **Minimum:** have `Prompt::bind_media_mut` pass the effective Prompt
   destination to the existing shared GenerateContent media helper.

Do not restore a Vertex request variant, duplicate the binder, or special-case
Python.

### Cache identity

1. **Delete:** the unsafe request-only derivation boundary can be deleted, but
   deleting provider-scoped cache derivation altogether would unnecessarily
   remove existing OpenAI and Anthropic behavior.
2. **Reuse:** `Prompt::provider()` and the existing request-specific directive
   extractors provide every required input.
3. **Native/dependency:** no dependency, registry, adapter, or wrapper is
   needed.
4. **Minimum:** replace `CacheKey::from_request` with one unambiguous derivation
   operation that receives a `Prompt` (or an already-resolved provider plus the
   request) and scopes Chat keys by that effective provider. Do not retain a
   second request-only Chat path.

This is a private-workspace Rust boundary correction within Revision 14's
already-approved representation change, not a new product or compatibility
decision.

### Documentation

1. **Delete:** the false wording can be deleted, but the touched fallible items
   still require substantive documentation under repository authority.
2. **Reuse:** use the schema-default/effective-destination terminology already
   present on `ProviderRequest::provider`, `Prompt::provider`, and the Python
   response projection.
3. **Native/dependency:** rustdoc is sufficient; no checker or dependency is
   justified.
4. **Minimum:** correct the two false rustdoc blocks and document the four
   changed construction items, including exact existing errors and the
   destination-preservation invariant.

## Prior-finding closure

`FIND-TASK-004-1` through `-16` and `-18` through `-23` remain closed in the
current source and recorded evidence. In particular:

- `FIND-TASK-004-18` stays closed: follower release is grant-stream close,
  with no leader acknowledgement, polling restoration, or idle refusal.
- `FIND-TASK-004-19` stays closed: published `Running` reserves attempt one,
  and interrupted published work settles `Cancelled`.
- `FIND-TASK-004-21` stays closed: `PreparedWorkflowRun` owns one deadline and
  the server binds that exact instant once into shared tools before acceptance.
- `FIND-TASK-004-22` stays closed: the existing forwarded Oracle journey now
  drives omitted, explicitly longer, and explicitly shorter positive inputs
  through `QueryTool`, distinguishes run-bound timeout from the shorter query
  timeout, and preserves settlement/recovery proof.
- `FIND-TASK-004-23` stays closed under Revision 14: one OpenAI Chat body with a
  custom Prompt destination passes the existing external `openai_chat` route,
  reaches `/v1/chat/completions` unchanged, and bypasses Wyrd gateway ingress.

`FIND-TASK-004-17` is **REOPENED**. Its prior correction established the active
Revision 13 chain, but the same invariant regressed when approved Revision 14
was assigned to TASK-004 without advancing the active TASK-004/R5 authority and
without superseding R5's incompatible request-variant instructions.

## Final deduplicated finding ledger

### FIND-TASK-004-17 — REVISED / REOPENED — VIOLATION: active TASK-004 authority and R5 acceptance text conflict with Revision 14

- **Discovery sources:** `BEHAVIOR-R6-001`, `INV-R6-001`,
  `STANDARDS-R6-001`.
- **Violated obligation:** active implementation and remediation tasks must
  identify and conform to the approved specification revision. Revision 14 is
  approved and expressly carried by TASK-004.
- **Exact locations:**
  `changes/active/skald-workflow-runtime/tasks/TASK-004-accepted-server-jobs.md:1-7,433-439`;
  `changes/active/skald-workflow-runtime/review/TASK-004-r5/TASK-004-R5-close-compatible-route-and-deadline-proof-gaps.md:1-22,48-75,127-145,169-205`.
- **Evidence:** TASK-004 and R5 still declare Revision 13. R5's live outcome,
  diagnosis, and acceptance criteria require `OpenAiChatCompatible`, while its
  later Revision 14 evidence and the candidate correctly delete that variant.
- **Observable consequence:** an implementer or reviewer following the active
  packet receives mutually exclusive completion instructions for the same
  candidate and can restore behavior the approved contract prohibits.
- **Decision-complete correction:** through the existing task-correction path,
  update TASK-004's current revision/link and R5's current governing metadata
  to Revision 14. Mark the Revision 13 compatible-variant diagnosis and
  acceptance rows superseded by the single Chat schema plus optional Prompt
  destination; preserve the R5 deadline-proof obligation, its immutable
  Revision 13 review input, and all prior review reports. Add no checker,
  schema, compatibility path, file, setting, or option.
- **Focused closure proof:** source inspection shows one unambiguous current
  Revision 14 authority chain, no live R5 criterion requires a deleted request
  variant, and historical review subjects retain their true revisions.

### FIND-TASK-004-24 — CONFIRMED — REGRESSION: Vertex media refusal reports Google as the selected provider

- **Discovery source:** `DOMAIN-PROVIDER-R6-1`, confirmed by the focused
  follow-up.
- **Violated obligation:** Revision 14 makes `Prompt.provider` the effective
  native dispatch target while sharing the GenerateContent body. Existing
  provider-bearing Prompt errors must continue to identify that selected
  provider.
- **Exact location:**
  `crates/skald/skald-spec/src/prompt.rs:244-258,910-948`.
- **Evidence:** a Vertex Prompt carries a GenerateContent request plus
  `provider = Vertex`, but `bind_media_mut` hard-codes Google when constructing
  `UnsupportedMediaForProvider`. The base Vertex arm supplied Vertex, and
  public Rust/Python plus Vala Judge callers reach the refusal.
- **Observable consequence:** callers receive a stable structured error naming
  Google for a Vertex-targeted Prompt, contradicting authoring and dispatch and
  regressing the pre-fold error identity.
- **Decision-complete correction:** in the existing `Prompt::bind_media_mut`
  owner, resolve `Prompt::provider()` before mutably borrowing the request and
  pass that value to the existing GenerateContent media helper. Preserve all
  other media behavior. Do not restore a Vertex request variant or duplicate a
  provider-specific media path.
- **Focused closure proof:** an existing Prompt media test home exercises the
  same unsupported URL for a Vertex Prompt and asserts the stable error details
  identify `vertex`; the Gemini counterpart still identifies `google`, and the
  existing successful Gemini/Vertex replacements remain green.

### FIND-TASK-004-25 — REVISED — REGRESSION: request-only cache derivation erases custom Chat destination scope

- **Discovery source:** `DOMAIN-PROVIDER-R6-2`, confirmed and narrowed by the
  focused follow-up.
- **Violated obligation:** `CacheKey`'s existing contract scopes deterministic
  cache identity by provider so provider-owned resource IDs cannot collide.
  Revision 14 moved Chat destination identity to `Prompt` without authorizing
  loss of that isolation.
- **Exact location:** `crates/skald/skald-cache/src/key.rs:14-23,58-105`.
- **Evidence:** the base compatible Chat arm supplied its embedded custom
  provider. The candidate's sole Chat arm always supplies OpenAI because its
  request-only input cannot observe `Prompt.provider`. Identical cache material
  therefore creates equal keys for distinct destinations, and those keys feed
  the public cache operations directly.
- **Observable consequence:** a provider-owned cached resource can be looked up
  under another custom target or OpenAI scope. No current production caller
  invokes derivation, but the materially changed public workspace operation is
  directly reachable and contradicts its own provider-isolation contract.
- **Decision-complete correction:** replace the request-only derivation API
  with one cache-owner operation that receives the effective Prompt destination
  (prefer the existing `Prompt` owner, or an already-resolved provider plus the
  request), uses it for Chat scope, and retains current request inspection for
  cache directives. Remove rather than preserve a custom-unsafe request-only
  alternative. Do not recreate `OpenAiChatCompatible`, add a registry,
  compatibility adapter, setting, checker, or synthetic caller.
- **Focused closure proof:** extend the existing `skald-cache` key tests with
  otherwise identical cacheable Chat Prompts targeting OpenAI and distinct
  custom providers; assert their provider scopes and keys differ, while the
  ordinary OpenAI and Anthropic cases retain their existing results.

### FIND-TASK-004-26 — CONFIRMED — VIOLATION: changed Rust documentation reverses the schema/destination contract

- **Discovery source:** `MAINT-R6-1`, confirmed by the focused follow-up.
- **Violated obligation:** materially modified Rust documentation must
  accurately describe the item's operation and relevant invariant; language
  projections must describe the same contract.
- **Exact locations:** `crates/skald/skald-spec/src/response.rs:95-112` and
  `crates/skald/skald-agent/src/loop_runtime.rs:656-670`.
- **Evidence:** `ProviderResponse::provider()` returns Google for a shared
  GenerateContent response even when Vertex produced it, contrary to its
  rustdoc. `invoke_agent_span` records `Prompt::provider()` while its rustdoc
  says the value comes from the typed request. Python already documents the
  correct response-dialect default.
- **Observable consequence:** Rust callers and telemetry maintainers are told
  to infer a destination that the response does not contain and may undo the
  approved effective-destination telemetry path.
- **Decision-complete correction:** update only the two rustdoc blocks. Define
  `ProviderResponse::provider()` as the response schema/dialect default and
  state that shared GenerateContent responses do not retain Gemini-versus-
  Vertex destination; define the Agent span value as the Prompt's effective
  dispatch target. Add no field, wrapper, fallback, checker, or test harness.
- **Focused closure proof:** direct source inspection against the existing
  Python wording and the current implementations; normal format/lint evidence
  is sufficient for the documentation-only change.

### FIND-TASK-004-27 — CONFIRMED — VIOLATION: materially changed Prompt construction items omit required errors and destination-preservation documentation

- **Discovery source:** `MAINT-R6-2`, confirmed by the focused follow-up.
- **Violated obligation:** `AGENTS.md` section 16 and
  `architecture/agent-rules.md` require substantive rustdoc for every new or
  materially modified Rust item, including private helpers, and exact
  `# Errors` sections for fallible operations.
- **Exact locations:** `crates/skald/skald-spec/src/prompt.rs:142-163` and
  `crates/skald/skald-prompt/src/builder.rs:240-244,273-324`.
- **Evidence:** `Prompt::new` and public `vertex` omit `# Errors`; changed
  private `google_prompt` and `finalize_prompt` omit rustdoc and `# Errors`.
  The latter's save/restore is the only thing preserving a Vertex/custom
  destination when `Prompt::new` normalizes the request and defaults the
  destination to `None`.
- **Observable consequence:** callers cannot determine existing failure
  conditions, and a maintainer can remove apparently redundant preservation
  code and silently retarget Vertex to Google.
- **Decision-complete correction:** add substantive rustdoc and accurate
  `# Errors` sections to those four existing items. Explain that
  GenerateContent is the shared body schema, `Prompt.provider` is the optional
  destination, and finalization preserves that destination across existing
  normalization. Do not add a constructor, builder abstraction, setter,
  checker, harness, or behavior change.
- **Focused closure proof:** direct documentation review plus normal format and
  lint evidence; existing Vertex construction, round-trip, and native-dispatch
  tests remain the behavioral proof.

## Validated ledger summary

| Stable ID | Status | Classification |
|---|---|---|
| `FIND-TASK-004-1` through `-16` | **CLOSED** | Prior remediation findings |
| `FIND-TASK-004-17` | **REVISED / REOPENED** | VIOLATION |
| `FIND-TASK-004-18` through `-23` | **CLOSED** | Prior remediation findings |
| `FIND-TASK-004-24` | **CONFIRMED** | REGRESSION |
| `FIND-TASK-004-25` | **REVISED** | REGRESSION |
| `FIND-TASK-004-26` | **CONFIRMED** | VIOLATION |
| `FIND-TASK-004-27` | **CONFIRMED** | VIOLATION |

No retained correction requires a new product, architecture, security,
compatibility, cross-service, concurrency, resource-ownership, persistent-data,
or deployment decision. The cache correction changes one unpublished workspace
Rust derivation boundary already made insufficient by Revision 14; it adds no
parallel API or compatibility promise. Every correction reuses an existing
owner or ordinary Rust/rustdoc mechanism. Under the standing DRIFT direction,
no remediation may add a bespoke checker, metadata system, wrapper variant,
registry, adapter, option, setting, test harness, or acknowledgement/polling
mechanism.

## Completion status

All proposed findings were independently traced through source, callers, and
sibling consumers; the follow-up conflicts are resolved from approved
authority. The validated ledger is non-empty.

**COMPLETE — five retained findings: `FIND-TASK-004-17`,
`FIND-TASK-004-24`, `FIND-TASK-004-25`, `FIND-TASK-004-26`, and
`FIND-TASK-004-27`.**
