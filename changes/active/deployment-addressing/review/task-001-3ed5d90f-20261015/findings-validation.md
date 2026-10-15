# Independent structured validation — TASK-001, R3

## Subject, inputs and result

Immutable source: `/tmp/wyrd-task-review-3ed5d90f`; original base
`1d3b5ffa0d34f75fa5b22c9b68f6ec0fc8d0cc70`; candidate
`3ed5d90ff53a2ed082decd66bf7aab168a5afdcb`; prior candidate
`9516afcdc7d3ffb58d5f20e85c4341d0df3cef23`. The complete cumulative range,
not only R2's diff, remains the acceptance subject. The archive has no CodeGraph
index. Source was inspected without mutation; only this report is written to
the live review directory. Adjacent changes and concurrent live work have the
owner's explicit approval and are not findings.

Inputs: approved deployment-addressing r6, original TASK-001, previous verdicts
and validation ledgers, R1/R2 corrections and committed execution evidence,
AGENTS/agent-rules, spec-driven-development and maintainer guidance, applicable
design/doctrine/security/Bifrost authority, both current task reviews, standards,
maintainer, system and all four domain reports, claim comparison, focused
follow-up, navigation, cumulative/remediation diffs and verification notes.
Source/callers, rather than the number of agreeing reviewers, determine the
ledger. The full range and owner map were considered; detailed independent
tracing targeted every proposed finding and prior correction boundary.

**Validated ledger: two bounded items remain, FIND-001-13 and new FIND-001-16.**
FIND-001-13 is narrowed to an explicitly required nonempty-media journey proof;
its production projection is implemented. FIND-001-16 is a shared compiler
identity collision that can silently substitute the wrong attachment. All
other prior causes close on current source. No approved behavior or material
boundary must change, so no specification revision is needed.

## Proposal dispositions

| Proposed claim | Independent disposition | Reason |
|---|---|---|
| B-01 / P-SDK-R3-01 / FUP-02, prior FIND-001-13 | **REVISED**, retain ID13 | Production Rust→N-API→TS metadata projection is correct; existing R2 proof expressly requires media names, while every current fixture asserts an empty list. |
| B-02 / FUP-01, next unused ID16 | **CONFIRMED**, assign FIND-001-16 | Legal authored media tokens alias the compiler's literal-media temporary names; all matching occurrences are replaced before expected kinds attach. |
| Invariant/data/maintainer full closure of FIND-13 | **REVISED** for proof only | Correct shared derivation and visible projection establish implementation, but do not satisfy the explicit first-class-language nonempty-media journey requirement. No production API remains missing. |
| Owner-raised runtime Agent export/default/hash concern | **REJECTED as a task-blocking finding; record separately** | Real lossy generic runtime projection, but the approved exact authored save/load rule is Prompt-specific; changed Agent route and invocation identity boundaries are preserved. This is not rejected merely because R2 omitted it. |
| Initial whole-Agent-spec equality test narrowing conceals missing route closure | **REJECTED** | Required identities/ref/route/provider/model are still inspected on the actual loaded instance, which is executed; route-loss mutation independently falsifies the required producer. |
| Historical RED limitations alone are current defects | **REJECTED** | Missing/compile-only historical RED is disclosed; do not manufacture a baseline run or production churn when current source and credible focused proof establish closure. |
| Card-scope403 is insufficient FIND-14 denial | **REJECTED** | A representative real refusal before provider IO is required; separate permission journeys prove RBAC. The evidence accurately identifies this as Card-scope denial. |
| Narrow iframe/theme inspection is no rendered verification | **REJECTED** | Supplied evidence describes actual production-page rendering at narrow content width and both themes, including the limitations; no independent visual defect is established. |
| Function-local imports in output_schema_field require a new blocking round | **REJECTED as material finding** | Standards report records a rule/style note with no behavioral/public-contract consequence; active review threshold makes it nonblocking. |

## FIND-001-13 — REVISED — MISSING focused verification — Important

**Plainly:** TypeScript exposes media variables, but its journey only checks an
empty list; extend the existing journey with named image/document inputs and
assert those names after registration and hydration.

**Obligations:** REQ-008/011, AC-004/009, R1-AC-13 and R2 FIND-13's explicit
focused proof. `spec.md:584–608` requires shared derived introspection on all
three SDKs; AC-004 requires the real three-SDK register/GET/hydrate proof. R2's
correction at lines206–210 explicitly requires existing media names through
fixtures. A correct API plus an empty-only assertion does not waive that proof.

**Current exact locations and source:**

- `sdks/wyrd-sdk-ts/native/src/cards.rs:645–663::hydrated_prompt` reads actual
  `prompt.variables`, `prompt.media_variables` and ResponseType metadata from
  Rust. Both standalone state lookup and `hydrated_agent` use it.
- `sdks/wyrd-sdk-ts/wyrd/src/index.ts:2359–2397::PromptCard`,
  `NativeHydratedPrompt` and `promptCard` expose the precise read-only metadata
  without rederiving or persisting it. This is the correct existing owner.
- `sdks/wyrd-sdk-ts/wyrd/tests/integration/register-and-hydrate.test.ts:176–225`
  uses media-free portable and native fixtures, then asserts
  `hydrated.mediaVariables` equals `[]` at219 for all four navigation forms.
- `fixtures/cards/register_and_hydrate/{portable-prompt,inline-prompt-agent}.yaml`
  contain only text. The shared authored graph is also consumed by
  `sdks/wyrd-sdk-python/tests/integration/test_register_and_hydrate.py:202–258`.

**Observable gap:** replacing the N-API media field with an unconditional empty
array leaves every current TypeScript assertion green, despite violating the
required public introspection. No storage/hash corruption or currently broken
projection is alleged. Existing pure Rust binder tests cannot exercise this
language handoff. This is one prior finding's proof remainder, not a new SDK
feature or another implementation defect.

**Selected smallest correction:** extend the same authored-form fixture and
register/GET/hydrate journey with supported named Image and Document bindings,
asserting their exact nonempty derived media names through standalone Prompt
lookup and Agent-linked Prompt navigation. Retain native/plain-text empty/null
cases, text variables, response schema/name, authored JSON equality, registered
refs and server/hydrated hash equality. Update the existing Python expected
authored form if the shared fixture changes; do not leave its existing consumer
broken. Reuse the existing Rust derivation, N-API projection and journey owner.
No production accessor/parser/endpoint, stored metadata, dependency, new harness
or second registry journey is indicated unless the focused proof exposes an
actual additional defect.

**Focused closure proof:** normal existing native build prerequisites followed
by the existing exact TypeScript selector; if shared fixtures change, run their
existing Python consumer too:

```sh
mise exec -- scripts/postgres/with-test-postgres.sh -- pnpm --dir sdks/wyrd-sdk-ts/wyrd exec vitest run tests/integration/register-and-hydrate.test.ts -t '^prompt forms round trip as authored$'
mise exec -- scripts/postgres/with-test-postgres.sh -- uv run --directory sdks/wyrd-sdk-python python -m pytest -q -m integration tests/integration/test_register_and_hydrate.py::test_prompt_forms_round_trip_as_authored
```

The new assertion must fail when media metadata is dropped/emptied. Record
nonempty test selection/results and only applicable fixture/TS/Python checks.
No full language journey aggregate is required for this correction.

## FIND-001-16 — CONFIRMED — INCORRECT portable-media compilation — Important

**Plainly:** A valid media input name can match a temporary name used by the
compiler, causing a fixed attachment to replace the user's input; make the
compiler's temporary media identities disjoint from all authored media tokens.

**Discovery provenance:** B-02 independently proposed this defect; FUP-01
expanded the path to later turns and media tokens in text. This validator
independently read the complete affected compiler methods, grammar,
normalization/render methods, all sibling media binders and construction/
serialization consumers. This is a different producer cause from original
FIND-4's missing kind check and FIND-11's unchecked source payload, whose
corrections remain closed.

**Obligations:** REQ-006 ordered typed portable media; REQ-008 correct shared
compilation/derived media metadata; AC-006 content/order preservation and no
silent conversion. The supported grammar is public and permits leading
underscores; there is no approved reserved-name prefix.

**Exact producer→consumer trace:**

1. `skald-spec/src/prompt.rs:176–188::media_placeholder_regex` accepts
   `${media:__wyrd_media_0}`. This is separate from the ordinary text grammar at
   155–173; `${__wyrd_media_0}` alone is not a media collision.
2. `skald-spec/src/authoring.rs:412–487::PortableMessage::lower` accepts that
   binding and records its expected kind. Its literal-source branch at474
   independently allocates `format!("__wyrd_media_{}", literals.len())`, so the
   first literal has the same name. No complete-message occupied-name check
   exists. Bindings appearing after the literal, including a later turn, are
   equally reachable through `portable_request:352–408`.
3. `AuthoredPrompt::compile:215–234` constructs the normalized Prompt and binds
   every literal through `Prompt::bind_media_mut`, **then** assigns
   `prompt.media_kinds`. Thus a cross-kind collision can also replace a
   declared Document placeholder with an Image before its expected kind exists.
4. `prompt.rs:448–454,513–560` normalizes all message media tokens, validates the
   literal source, and calls the ordinary provider binder. `bind_media_openai_chat`
   at988–1017, `bind_media_anthropic` at1067–1092 and `bind_media_google` at1094–1122
   correctly replace every matching token. Responses at1027–1065 uses the same
   repeated-binding semantics but is native-only in this portable revision.
5. Binding removes the collided name from `media_variables`. `render:569–581`
   consequently sees no remaining required media and succeeds without the
   user's input. Ordinary message Text content also undergoes normalization,
   so authored media tokens there must be included in the collision boundary;
   only checking structured Image/Document bindings is incomplete. Existing
   system-media refusal remains valid and must not be relaxed.

**Reachable authored example:**

```yaml
provider: openai
model: gpt-4o
messages:
  - role: user
    content:
      - type: image
        source: '${media:__wyrd_media_0}'
      - type: image
        source: {type: url, url: 'https://example.com/fixed.png'}
```

The fixed URL is substituted into both image positions and the caller's media
variable disappears. This is a valid authoring input, not a dormant helper or
malformed upstream response. With supported base64 sources the same producer
reaches Anthropic and Google; custom portable providers use the OpenAI lowering
and Vertex uses Google's lowering. Jev explicitly refuses portable authoring,
and no portable Jev or local Vertex feature is required.

`Prompt::Deserialize` (`prompt.rs:93–103`) calls this compiler;
`PromptSpec::Deserialize` (`wyrd-spec/src/card/prompt/spec.rs:62–78`) uses that
Prompt, while `PromptSpec::new:31–36` recompiles authored input. Normal standalone
registration, inline Agent Prompt decoding, local/bundle hydration and runtime
loading therefore share this producer. `Prompt::authored` also recompiles when
checking whether its stored form matches the current request. Authored JSON and
hash can remain correct while effective compiled content is wrong; do not
misclassify this as a durable storage bug.

**Selected minimal correction boundary:** extend the existing shared portable
compiler so temporary literal-media identities cannot equal **any authored
media token in the complete portable content** before ordinary literal binding.
Reuse its existing grammar and binder. Preserve every currently valid authored
name, repeated-name binding behavior, content order, kind/source validation,
authored JSON/hash and native untyped media semantics. The defect begins when
the compiler produces a conflicting identity; do not patch registration, SDKs,
provider dispatch or each binder separately.

The Ponytail alternatives do not eliminate the correction: literals and caller
bindings are both approved; the existing binder intentionally replaces all
occurrences and cannot distinguish these equal names. A first-match binder
would break legitimate repeated bindings. Attaching kinds earlier only catches
some cross-kind cases and leaves same-kind substitution. A reserved public
prefix would reject currently valid inputs, a random naming contract leaves a
possible collision, and a second compiler/binder/dependency is unnecessary.
Keep ordinary text and media grammars distinct; do not invent a public ban on a
same-bare-name ordinary text variable.

**Focused closure proof:** extend the existing shared compiler tests for
OpenAI/Anthropic/Google using their supported sources. Mix a literal with the
valid authored `__wyrd_media_0` media name before/after it, including a later
turn and a media token in message text; retain repeated-name replacement. The
derived authored media name must remain, rendering without caller input must
refuse, and distinct supplied versus literal sources must occupy the intended
positions. Include cross-kind Document binding/Image literal and its wrong-kind
refusal, plus a same-bare-name ordinary text variable assertion. Preserve
existing native behavior and media validation. Use existing test/fixture owners,
not a new harness:

```sh
mise exec -- cargo nextest run --locked -p wyrd-skald-spec --lib -E 'test(=authoring::tests::portable_media_bindings_keep_their_kind) | test(=authoring::tests::portable_content_settings_and_schema)'
```

These actual selectors exist in current source. If implementation introduces
another focused test, record its actual exact selector and execution. This
report establishes the defect through source, not a claimed runtime RED.
Required checks follow the small compiler/test write set; no full journey suite
or aggregate is selected. The existing real registration fixture in FIND13
continues to prove ordinary authored media reaches SDK hydration, without
adding a duplicate collision journey.

## Prior-finding closure retained from current source

| Prior ID | Independently reconciled closure and proof boundary |
|---|---|
| FIND-001-1 | **CLOSED.** `Permission::subsumes:307–313` retains explicit gateway grants alongside All; `PermissionSet::intersection:952–999` computes the complete meet and rejects unrepresentable conditional authority. `TenantGrant::into_access_grant:218–247` consumes the result before `TenantTokenIssuer::issue:450–484` signs. Permission/real-exchange tests cover both directions, mixed and empty representable results. No new empty-intersection refusal is inferred. |
| FIND-001-2 | **CLOSED.** `Agent::from_card:674–699` restores execution_route and `to_spec:784–793` emits it. `route_registry` selects before IO; both `run_with`/`run_prompt` use that immutable selection ahead of provider override, and a missing caller refuses. `Run::invoke_on` creates an invocation-local value. Existing tool-loop/isolation tests and the actual loaded-Agent child support closure. |
| FIND-001-3 | **CLOSED.** Public caller at `gateway_caller.rs:104–113` rejects typed Vertex before projection or transport; NativeCall retains deployment/model for Anthropic/Jev/Google, independent of deployment spelling. Separate server caller retains Vertex. P1 tests zero transport and Google on deployment named vertex. |
| FIND-001-4 | **CLOSED at original kind-loss cause.** Compiler retains explicit binding kinds/conflicts; `bind_media_mut:520–530` refuses wrong kind before mutation. New16 is the distinct synthetic-identity producer, not a reason to repeat this original correction at consumers. |
| FIND-001-5 | **CLOSED.** `prompt.rs:233–262` uses bundled standard `jsonschema::meta::validate`, guards unknown dialect before library retrieval, retains strict OpenAI object-root and field-path errors. Workspace defaults disabled, narrow skald-spec dependency. Installed library's `try_meta_validator_for` returns bundled validators for recognized drafts; user `$ref` is data, not compiled/retrieved. Native/portable/builders/PromptSpec converge here. P2–P6 cover malformed nested/draft-specific keywords and real no-write registration; existing OpenAPI/MCP consumers retain assertions. |
| FIND-001-6 | **CLOSED.** Typed Prompt provider reaches `GatewayInvocation::route:557` and `CallPlan::require_compatible_initial`; ordinary direct-client translations and separately authorized explicit fallbacks remain. Header is a compatibility constraint, not authority. |
| FIND-001-7 | **CLOSED.** `batches::list:575–624` computes explicit scopes and Granted exclusions from one snapshot before `gateway_batches` SQL's OR reach predicate, ORDER and LIMIT; subsequent audited decisions use the same snapshot. Existing bounded pagination proof is substantive. |
| FIND-001-8 | **CLOSED.** `Prompt::new:336–351` validates the canonical literal model and compares every Chat/Responses/Anthropic/Jev typed body model before variables/schema derivation; GenerateContent has no body model. Authored body.model remains refused. P7 covers templated/conflicting models and matching slash IDs. |
| FIND-001-9 | **CLOSED.** `engine::AttemptRecord` retains provider from the actual admitted adapter; live attempt span and captured OTLP mapping use that immutable fact, with deployment/model separate. Fallback retains its own provider rather than re-reading mutable deployment identity. |
| FIND-001-10 | **CLOSED.** `admit_call:457–520` loads its owned routing snapshot and always authorizes it before planning/dispatch. Batch create's earlier check still protects claims; settle/refresh use common admission. Undispatched failure releases fresh claim, ambiguous dispatch retains fence. Existing deterministic replacement/replay/release tests cover this seam. |
| FIND-001-11 | **CLOSED at original source acceptance cause.** `check_media_source:1314–1415` validates bounded/decoded base64 using existing20MiB budget, kind/provider MIME constraints and Google file URI before replacement. Literal and runtime paths share it. No new fetch/upload subsystem. New16 does not reopen source payload validation. |
| FIND-001-12 | **CLOSED.** Current standards report's changed-item/owner inventory and cumulative in-place rustdoc correction cover the original list and new R2 helpers/types/tests. No missing changed item was independently identified; no style-only remediation is warranted. |
| FIND-001-13 | **PARTIAL; retained above.** Shared authored hash/GET/hydration and production thin metadata projection pass; explicit nonempty TS media proof remains. |
| FIND-001-14 | **CLOSED, human-directed provenance retained.** `wyrd-client/tests/integration/local_development.rs:63–151` gets the downloaded validated Card/Prompt, registers its exact pinned Prompt under the existing resolver, calls public `Agent::from_card`, inspects the loaded instance, attaches only host caller, and executes that instance. Python driver runs the same child on the same downloaded bundle before/after restart; outer headers/native trap/denial and unique Run/Card retained rows are preserved. No replacement Agent, route setter or original authoring object repairs missing fields. |
| FIND-001-15 | **CLOSED.** Connect setup step5 now applies existing opt-in metadata capture before inference, with disabled/no-backfill explanation. Existing framework tests make real calls and assert their own retained rows through normal Bifrost helper on fresh function-scoped servers. P10/capture-removal mutation prove the corrected promise. No default-policy change. |

## Owner-raised Agent export question

**Recommendation: track separately; do not add it to this task remediation.**
The disclosed behavior is source-confirmed: `run_config_from_agent_run_config_spec`
(`agent.rs:844–860`) starts with `RunConfig::default` (`run.rs:38–47`), giving
max_iterations10 and capSome(8); the reverse converter at833–840 emits them
explicitly. `to_spec:784–793` also writes empty `verified_by`. `to_card`,
`to_wire`, YAML/save and Python runtime projections use that same boundary, so
faithful runtime Agent re-emission is a wider problem than one absent cap.
Public hydrated Card holders retain their stored spec separately.

This is not waived simply because R2 did not list it. The stronger approved
obligation at REQ-008 explicitly concerns the two authored **Prompt** forms;
INV-004 requires route overrides to preserve the stored Agent/hash, not an
arbitrary runtime Agent export to reproduce every authored omission. Existing
Run overrides do not save/re-register the Card, and required Agent route
projection is retained. The original route work and human14 require loaded
route/provider/model/ref/registered identity plus actual execution, now proved;
they do not introduce complete generic runtime export fidelity. The cumulative
diff retains these generic conversion/default/verified_by bodies while changing
route preservation. No newly introduced route behavior or required writer uses
the lost defaults to alter persisted registered state.

A separate scoped repair should address omitted versus explicit configuration
and verification bindings together at the existing Agent/spec conversion owner,
with load→save/hash and explicit mutation proof. Do not merely change global
defaults, set capNone universally, weaken hashing, or create another loader.
The present review records the real limitation rather than asserting export is
lossless or turning it into a new approved requirement.

## Reconciled obligations and evidence limits

| Obligation group | Result and remaining cause |
|---|---|
| REQ-001–005,007,009–010,012–013 | PASS on current owner/caller/source plus supplied focused evidence. |
| REQ-006 | FAIL: compiler-created media identity collision, FIND16. |
| REQ-008 | FAIL: FIND16 compilation/derived-media correctness and FIND13 required TS nonempty proof; authored persistence implementation itself passes. |
| REQ-011 | FAIL only for explicit SDK proof remainder13; actual projection/errors/catalogs/docs are implemented. |
| INV-001–007 | PASS for their original tenant/secret/compatibility/route/hash/owner/outbox/no-native-escape causes; supported media correctness remains specifically tracked under REQ006/008 and AC006. |
| AC-001–003,005,007–008,010–012 | PASS, including actual downloaded loader execution/restart and server/local Vertex boundary. |
| AC-004/009 | FAIL only for explicit FIND13 proof remainder. |
| AC-006 | FAIL for FIND16 silent attachment substitution. |
| R1-DOC-AC-01–03 | PASS on corrected source and supplied focused/Chrome evidence with disclosed iframe/theme limits. |
| Non-goals/owner boundaries | PASS: Workflow product stays removed; independent Eval survives; no model mapping/alias/catalog, hosted Agent runtime, new secret/media fetcher, external bypass, portable audio/video, local Vertex gateway, replacement transport/graph/loop/audit queue or remote schema retrieval. |

No tests, builds or browser checks were rerun by this validator. Existing
selectors/test assertions and committed execution evidence were inspected;
reported GREEN is not presented as independent execution. The synthetic-media
counterexample is proved statically through complete producer/binder/render
bodies; no retrospective runtime RED is claimed.

The stale Python-extension diagnosis is coherent with the actual current
`code_to_wyrd_error` fieldless Bifrost branch (`wyrd-client/src/error.rs:235–264`):
an older extension would lose the native gateway403 code and reach the502
fallback, while `uv run` does not rebuild bindings. Recorded tracing, artifact
times, `py:setup` rebuild and P9/P10 reruns are supplied evidence; no source or
assertion change was used to hide it. Missing historical behavioral RED is
honestly disclosed. The child whole-spec assertion adjustment was scoped to
the approved loader criterion and has separate route-loss mutation proof.

Rendered evidence is actual production-page inspection at desktop and narrow
iframe content widths in light/dark, with iframe and attribute-switch limits
stated; no unsupported visual defect is inferred. No live-provider entitlement,
release runner, synthetic load, baseline build or final aggregate gate was
checked. Broad integrated verification remains change-review work.

Both retained corrections are decision complete at existing owners. Package
them together for `$wyrd-implement`, with focused selectors and applicable
write-set checks, preserving all closed boundaries. Review performs no
production edit, commit, merge, push or deployment.


## Subsequent owner-directed addition — FIND-001-17

After the independent review above, the owner instructed: “add the agent export
to this as 1 more finding.” The same R3 remediation now includes FIND-001-17
for faithful loaded Agent export, covering optional run configuration and
verification bindings together. The source-confirmed behavior is described in
“Owner-raised Agent export question” above. The original rejection concerned
task scope, not the existence of the defect; the current owner instruction
supersedes that scope exclusion.

This is a human-directed requirement added after validation, not a retroactively
independently discovered finding. Preserve the original two-finding independent
ledger and review reports as historical evidence. The current packet contains
those two validated findings plus this owner-directed third item. The selected
correction and focused acceptance proof are in TASK-001-R3, including unchanged
load/export spec/hash, omitted versus explicit defaults, nonempty verified_by,
intentional edits, Rust/Python projection and the existing downloaded-Agent
journey. No implementation or test run is claimed by this addendum.
