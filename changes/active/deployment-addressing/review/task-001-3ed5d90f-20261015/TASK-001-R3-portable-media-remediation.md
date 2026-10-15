---
id: TASK-001-R3
kind: remediation
status: ready
spec: SPEC-deployment-addressing
spec_revision: 6
requirements: [REQ-006, REQ-008, REQ-011, AC-004, AC-006, AC-009]
depends_on: []
parent_task: TASK-001
remediates: [FIND-001-13, FIND-001-16, FIND-001-17]
---

# Preserve media inputs, hydrated metadata and loaded Agent exports

## Authority and outcome

Approved specification: `changes/active/deployment-addressing/spec.md`, r6.
Original task: `changes/active/deployment-addressing/tasks/TASK-001-deployment-addressing.md`.
Validated source findings: `findings-validation.md` and `verdict.md` in this
review directory. Prior review packets and remediations are under
`../task-001-ef2fe8bd-20261015/` and `../task-001-9516afcd-20261015/`.

Original base: `1d3b5ffa0d34f75fa5b22c9b68f6ec0fc8d0cc70`.
Previous candidate: `9516afcdc7d3ffb58d5f20e85c4341d0df3cef23`.
Reviewed candidate: `3ed5d90ff53a2ed082decd66bf7aab168a5afdcb`.
Read-only snapshot: `/tmp/wyrd-task-review-3ed5d90f`.
Locations below refer to that candidate; preserve concurrent approved work
when revalidating the existing owners. Review has not implemented or executed
these planned corrections. Route this ready packet directly to `$wyrd-implement`.

The outcome is that a portable Prompt's literal attachment cannot consume a
separate authored media input, and the existing TypeScript lifecycle journey
proves real nonempty media names survive registration and hydration. Loaded
Agents must also preserve their declared spec/hash on export. This
requires the shared compiler correction, faithful Agent export at its existing
owner, and extensions to current test/fixture owners. No new product or
specification decision is pending.

**Owner-directed addition:** after this review the owner explicitly instructed,
“add the agent export to this as 1 more finding.” FIND-001-17 is therefore a
required part of this same task. It supplements the original r6 task scope by
current owner instruction; it is not retroactively presented as an independently
discovered or originally required finding. The source behavior was already
confirmed during review; the earlier separate-tracking recommendation is
superseded.

## FIND-001-16 — Temporary literal names can consume caller media

**Plainly:** a valid media input name can match the compiler's temporary name,
causing a fixed attachment to replace the user's input.

**Obligations and consequence.** REQ-006 requires ordered typed media inputs;
REQ-008 requires correct shared compilation/derived metadata; AC-006 forbids
silent content replacement. The bug can send the wrong attachment and omit a
required runtime input while leaving the authored JSON/hash unchanged.

**Exact diagnosis.** `skald-spec/src/authoring.rs:412–487`, in
`PortableMessage::lower`, emits accepted authored `${media:__wyrd_media_0}`
bindings but independently gives the first literal source the same
`__wyrd_media_0` name. `portable_request:352–408` has no complete-content
collision check. `AuthoredPrompt::compile:215–234` constructs the Prompt,
binds its literal sources, then attaches expected portable media kinds.

`prompt.rs:176–188` expressly permits leading underscores in authored media
names. Normalization/binding at448–454 and513–560 reaches ordinary OpenAI,
Anthropic and Google binders at988–1017 and1067–1122, which correctly replace
every occurrence of a matching media token. Binding then removes the name
from derived media variables, so `render:569–581` no longer requires the
caller's input. A declared Document can also become an Image before expected
kinds are attached. This is one compiler-created identity problem, not a bug
in each provider or in ordinary repeated-name binding.

For example, this valid portable input uses the fixed URL twice after compile:

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

The same cause reaches Anthropic and Google with their supported literal
sources. Prompt deserialization, PromptSpec creation/deserialization, inline
Agent Prompts, registration, local loading and hydration share the compiler.
Checking only bindings already encountered misses a placeholder in a later
turn. Legal media tokens in message Text content also participate in normal
media normalization and must be included; ordinary `${name}`/`{{name}}` text
variables with the same bare name are a separate grammar.

**Selected minimal correction.** Extend the existing shared portable compiler
so its temporary literal-media identities are disjoint from every authored
media token in the complete portable content before ordinary binding occurs.
Reuse the existing placeholder grammar and binder. Preserve all currently
accepted authored names, repeated user bindings, content order, expected
kind/source validation, native behavior and authored JSON/hash.

Fix identity production once. Do not reserve a new public prefix, use random
names as a collision guarantee, change the binder to first-match, add per-SDK
or provider guards, or create another compiler/binder/dependency. Attaching
kinds earlier alone leaves same-kind replacement unfixed. Do not relax the
existing system-media refusal or add portable Jev/local Vertex support.

**Required proof.** Extend existing shared compiler tests for
OpenAI/Anthropic/Google with supported literals mixed with authored
`__wyrd_media_0`: binding before and after the literal, another turn, repeated
bindings, and media tokens in message text. The authored name must remain
derived; render without its input must refuse; binding a distinct input must
put supplied and fixed sources in their intended positions. Include a
Document binding/Image literal and wrong-kind refusal, plus a same-bare-name
ordinary text variable case to preserve the separate grammars. Keep existing
native untyped media and payload/MIME/size/URI validation regressions.

## FIND-001-13 — Prove nonempty media metadata through TypeScript

**Plainly:** TypeScript now exposes media variables, but the journey only
checks an empty list, so it cannot detect lost media names.

**Obligations and gap.** REQ-008/011, AC-004/009 and R2's explicit focused proof
require derived media names through real SDK registration and hydration.
Current `register-and-hydrate.test.ts:176–225` uses media-free fixtures and
asserts `mediaVariables == []` for all four forms. An always-empty projection
would pass. R2's stated absence of media fixtures does not waive the required
proof. No broken production projection or authored storage is alleged.

**Existing implementation and selected correction.** N-API
`cards.rs:645–663::hydrated_prompt` reads actual shared Rust variables/media
variables/response metadata for standalone and Agent-linked paths. TS
`index.ts:2359–2397` projects those typed read-only values without deriving
or storing them. Keep that implementation.

Extend the existing authored fixture and exact four-form register/GET/hydrate
journey with named Image and Document inputs, asserting the exact nonempty
media names through standalone Prompt lookup and Agent-linked navigation.
Retain authored JSON/ref/server hash equality, text variables, response
schema/name, and native/plain-text empty/null cases. Use the existing portable
fixtures, `portable-prompt.yaml` and `inline-prompt-agent.yaml`, and their
current graph/journey owners; do not add another registry journey or harness.
Include FIND-16's legal temporary-looking binding mixed with a literal in an
existing portable fixture so this same registry/hydration journey also proves
the authored name survives compilation across the client/server boundary.
This adds the regression to the current journey rather than another harness.

Those fixtures are also consumed by Python's existing
`test_prompt_forms_round_trip_as_authored`; update its expected authored
portable form (`PORTABLE_SPEC`) and any other actual expectations affected
by the fixture edit. Reuse the existing Rust compiler and SDK projections;
no accessor, parser, endpoint, schema/dependency change or stored metadata is
selected. Change production projection only if the focused proof exposes an
additional actual defect.

## FIND-001-17 — Loaded Agent export changes the authored spec and hash

**Provenance:** this is the owner's explicit addition to this same remediation,
after the independent review. The original separate-tracking recommendation is
superseded. The defect was source-confirmed during review, but its inclusion
is human-directed, not a retroactively claimed independent finding.

**Plainly:** saving a loaded Agent adds omitted defaults and loses verification
bindings; preserve its declared spec and hash while keeping runtime defaults
and intentional edits working.

**Exact diagnosis.** `skald-agent/src/agent.rs:674–699::from_card` converts the
incoming `AgentRunConfigSpec` through `run_config_from_agent_run_config_spec`
at844–860. That helper starts with `RunConfig::default`, resolving absent
`max_iterations` to10 and absent `tool_concurrency_cap` toSome(8).
`agent_run_config_spec_from_run_config` at833–840 then emits those effective
values as authored values. `Agent::to_spec:784–793` also always writes
`verified_by: Vec::new()`. Consequently an untouched loaded Agent exported
through `to_card`, `to_wire`, YAML/save or Python runtime projections has a
different canonical spec/hash and can lose its declared verification bindings.
The registered Card holder retaining its original spec does not repair the
separate runtime export path.

The previous installed-CLI test caught the default insertion with a whole-spec
assertion, then narrowed the assertion to the earlier loader criterion. The
owner now requires the broader preservation contract, so that narrowed proof
alone is insufficient. Required equality concerns the typed/canonical AgentSpec
and its hash, not YAML formatting/comments, timestamps, server-managed status
or metadata not carried by the runtime holder.

**Selected correction at the existing owner.** Retain the incoming authored
run-configuration representation and verification bindings on the existing
Agent as part of its durable projection state. Continue resolving effective
defaults for execution through the existing RunConfig conversion. For export,
retain each unchanged field's original optional/explicit representation and
bindings; project intentional changes from the current runtime configuration
without discarding unrelated authored values. Compare the original effective
configuration with the current configuration where needed so direct public
Rust `run_config` edits cannot leave export stale. Preserve both omitted and
explicit default values, rather than universally dropping or materializing
defaults. Keep newly constructed Agents' existing valid export behavior.

All constructors/clones, Card/wire/YAML loaders, `with_run_config`, direct Rust
configuration mutation and Python's existing configuration setter/projection
must converge on the same owner semantics. The reverse conversion used for a
fresh runtime configuration is not by itself capable of recovering authored
omissions; do not change it globally and regress observed Run/judge callers.
Reuse existing shared types and export methods. Add no second Card store,
loader, SDK-specific normalization, new endpoint or new Agent runtime API.
Preserve nonempty `verified_by` declarations without resolving or executing
them during load/export, and without widening authority or changing evaluation
scheduling. Existing host caller, provider registry, callbacks, session and
journal remain runtime-only and are never serialized.

**Required focused proof.** Extend existing Rust/Python load/export/save tests
with absent run_config, partially specified settings, explicitly authored
default cap8/max_iterations10, nondefault settings and nonempty verification
bindings. For an untouched loaded Agent, compare the complete canonical spec
and canonical spec hash before export, after `to_card`/wire projection, and
after YAML save/reload. Verify identities, Prompt ref and execution_route stay
unchanged and runtime still uses the existing defaults. Then edit configuration
through existing builder/setter and direct public Rust mutation paths: changed
values must persist through export/reload while unaffected omissions and
verification bindings remain intact; a real authored change must change its
canonical hash normally. No hash exception or alias is allowed.

Extend the existing downloaded-Agent Rust child within the same installed-CLI
journey to assert complete loaded/exported spec and recomputed hash equality
against the downloaded registered Agent before and after execution/restart.
Preserve the already-correct actual loader, normal host caller, four-provider
runs, retained evidence, denial, native trap and key reuse. Local Rust/Python
cases cover configuration/binding variants without adding a new server harness
or requiring unrelated verifier fixtures in every provider journey.

## Existing owners, write set and preserved consumers

| Capability | Existing owner/consumers | Selected extension |
|---|---|---|
| Portable media lowering | AuthoredPrompt compile/portable_request, PortableMessage::lower; PromptSpec and SDK deserialization | Collision-free internal identities across complete authored media content. |
| Runtime media binding/render | Existing shared grammar and OpenAI/Anthropic/Google binders | Preserve repeated-name and kind/source semantics; exercise them, do not replace them. |
| Hydrated derived metadata | Existing Rust Prompt, N-API hydrated_prompt, TS PromptCard | No production change indicated; prove actual nonempty values. |
| Authored lifecycle fixtures | Shared register_and_hydrate portable/inline fixtures, TS journey and Python expected spec/test | Add real image/document names, retain four-form JSON/hash/schema/ref proof and update affected expectations. |
| Loaded Agent durable projection | Existing Agent from_card/to_spec and config conversions, constructors/clones/configuration mutations, Rust/Python exports and actual installed-CLI child | Preserve authored optional values/bindings while existing runtime defaults and intentional edits continue to work. |

Likely production edits are in the shared portable compiler and the existing
Agent projection owner; tests remain in their current owners and SDK
journey/fixtures. Paths
guide ownership, not private helper/module choices. Document new/materially
changed Rust items and tests, including applicable Errors/Panics contracts, in
the same implementation pass. No permanent checker or style-only round.

Preserve all closed deployment-addressing boundaries: named deterministic
deployments, credential-first single/list CLI, exact delegation and tenant
Open/Granted access, actual-snapshot batch admission, pre-LIMIT visibility,
uncertain-dispatch fences, literal model and standard offline schema checks,
media payload/kind checks, actual provider capture, required route/isolated
override, actual loaded-Agent four-provider execution/restart, opt-in capture
docs/framework evidence, and existing nonblocking Scribe outbox. Workflow stays
removed; native/server capabilities, supported translations/fallbacks, judges
and independent Eval behavior remain. Approved adjacent work is preserved.

No new scopes, model mapping, external bypass, portable audio/video, local
Vertex gateway, general Jev tools/streaming, hosted jobs, credential/media
fetcher, graph store, loader, transport, loop, queue or default-policy change.
Generic runtime Agent export fidelity is now required by owner-directed
FIND-001-17. `agent-export-follow-up.md` is retained only as the historical
scope record; its separate-tracking recommendation is superseded.

## Ordered implementation scenarios

### Scenario 1 — Literal media cannot consume authored input (FIND-001-16)

**Behavior.** Mixed literal/bound portable content preserves each source and
requires the original caller inputs, across supported built-in lowerings.

**RED.** Extend P1 with the diagnosed same-name examples and observe the
required name disappearing/wrong source appearing before the correction.
Include later-turn and cross-kind coverage so an encountered-so-far check or
earlier kind check cannot falsely pass. Compile/setup errors are not RED.

**GREEN.** Correct only the shared internal identity producer; literals bind
without aliasing any authored media token. Re-run the new cases and existing
source/kind/order/repeated/native regressions in P1.

**REFACTOR.** Reuse existing grammar/lowering/binding owners; keep private
mechanics implementation-owned and remove any superseded identity assumptions
without adding another media validation or dispatch path.

### Scenario 2 — Real SDK media metadata survives lifecycle (FIND-001-13)

**Behavior.** Exact nonempty image/document names reach both public hydrated
TS paths after real registration/GET/hydration, with unchanged authored
JSON/hash/ref and existing text/schema/native cases.

**RED.** Add the actual nonempty fixture/assertions to P2. A missing/always-empty
projection must fail; if the already-correct projection passes, record an
honest verification-only coverage correction instead of manufacturing a
production defect or historical RED.

**GREEN.** Complete the current fixture/assertion extension, update its Python
expected authored form and run P2/P3 with fresh normal native artifacts.

**REFACTOR.** Reuse existing lifecycle owners and shared fixture consumers;
keep derivation in Rust and retain both nonempty and empty/plain-text cases.

### Scenario 3 — Faithful loaded Agent export (owner FIND-001-17)

**Behavior.** Unchanged loaded Agents retain their canonical spec/hash through
Card/wire/YAML export and reload, including omitted/explicit configuration and
verification bindings; runtime defaults and intentional edits remain correct.

**RED.** Extend P4/P5 with omitted and explicit-default fields and a nonempty
binding, and restore the broader spec/hash assertion in P6's actual loaded-Agent
child. Record default materialization/binding loss as the behavioral failure,
not a stale extension or setup error.

**GREEN.** Preserve authored projection state at the existing Agent owner and
export it together with intentional current configuration changes. Run the
local configuration/binding cases and the same installed-CLI journey before
and after restart. Do not change runtime defaults to make equality pass.

**REFACTOR.** Keep one shared Rust conversion/projection boundary; remove stale
lossy projection assumptions without adding a parallel export or SDK algorithm.

## Focused verification — planned commands, not review execution

```bash
# P1: existing shared portable media compiler/kind owners
mise exec -- cargo nextest run --locked -p wyrd-skald-spec --lib -E 'test(=authoring::tests::portable_media_bindings_keep_their_kind) | test(=authoring::tests::portable_content_settings_and_schema)'
# P2: real TS registration/GET/hydration and both metadata navigation paths
mise exec -- scripts/postgres/with-test-postgres.sh -- pnpm --dir sdks/wyrd-sdk-ts/wyrd exec vitest run tests/integration/register-and-hydrate.test.ts -t '^prompt forms round trip as authored$'
# P3: existing consumer of the shared portable/inline fixture
mise exec -- scripts/postgres/with-test-postgres.sh -- uv run --directory sdks/wyrd-sdk-python python -m pytest -q -m integration tests/integration/test_register_and_hydrate.py::test_prompt_forms_round_trip_as_authored
# P4: existing Rust Agent author/save/load/run owner; extend spec/hash cases
mise exec -- cargo nextest run --locked -p wyrd-skald-agent --test integration -E 'test(=agent_journey::journey_author_save_load_run)'
# P5: existing Python runtime Agent Card/save/reload owner
mise exec -- uv run --directory sdks/wyrd-sdk-python python -m pytest -q tests/unit/runtime/agent/test_agent_journey.py::test_journey_to_card_round_trip
# P6: same installed server and actual downloaded Agent child, now full spec/hash
mise exec -- scripts/postgres/with-test-postgres.sh -- node scripts/check_ts_package.mjs mise exec -- uv run --directory sdks/wyrd-sdk-python python -m pytest -q -m local_dev tests/integration/test_local_development.py::test_installed_cli_gateway_setup
```

Confirm nonempty selectors and normal managed Postgres/migration setup. If
adding a separate focused compiler test, record its real exact selector before
running it. Rebuild native packages through existing `ts:build`,
`ts:build:testing` and `py:setup` prerequisites as needed for the changed shared
compiler; `uv run` does not rebuild a native extension. Record current-artifact
build and test commands rather than infer candidate proof from stale binaries.
Coordinate Cargo work sequentially with any concurrent builds.
P6 already invokes the Rust integration child
`local_development::cli_gateway_agents`; retain its existing exact selector and
build prerequisite in the parent driver/evidence rather than launch another
server. Record exact selectors for additional local configuration cases if
new tests are added. Keep the normal CLI/server artifact prerequisites current.

Use only applicable write-set static checks: `mise run fmt`, `mise run lints`,
`mise run ts:format:check`, `mise run ts:lints`, `mise run ts:typecheck`,
`mise run py:format` and `mise run py:lints` for changed tests/expectations.
Run `mise run py:typecheck` if public Python signatures/stubs change, and keep
existing Python exports/stubs synchronized if a boundary adaptation is needed;
no new public surface is selected by this task.
If actual public typing/generated/boundary changes become necessary, run their
existing type/codegen/dependency checks; none are selected in advance here.
There is no docs-site, SQL, wire-schema or dependency change selected.
Do not require full journey/family suites or `mise run gate`; broad integrated
verification remains change-review work. On failure rerun with tracing and
diagnose the actual cause; never weaken assertions, checks or timeouts.

## Acceptance and handoff

| Criterion | Findings | Required closure |
|---|---|---|
| R3-AC-01 | FIND-001-16 | Valid authored media names, including temporary-looking names and later-turn/text tokens, remain required and bind to caller sources independently of literal attachments. |
| R3-AC-02 | FIND-001-16 | Source positions/order, repeated bindings, kind refusal, all supported built-in lowerings, ordinary text grammar, authored hashes and native behavior remain correct. |
| R3-AC-03 | FIND-001-13, FIND-001-16 | Existing TS lifecycle journey proves exact nonempty image/document metadata through standalone and Agent navigation, including mixed literal/colliding-name authoring, without losing four-form JSON/hash/ref/text/schema proof. |
| R3-AC-04 | FIND-001-13 | Shared fixture consumers and Python expected authored form remain correct and the exact focused selectors pass using current native artifacts. |
| R3-AC-05 | FIND-001-17 | Unchanged loaded Agent Card/wire/YAML save/reload retains complete canonical spec/hash, including omitted/explicit settings and nonempty verification bindings, with runtime defaults unchanged. |
| R3-AC-06 | FIND-001-17 | Existing configuration mutation paths persist actual edits while preserving unaffected authored values/bindings; the actual downloaded-Agent journey proves full spec/hash preservation before/after execution and restart. |

Append one evidence row per finding: source correction, behavioral RED/GREEN
or justified verification-only proof, exact nonempty commands/results, relevant
checks and concrete limits. Keep prior evidence intact. The existing R2
production metadata projection is not still missing; do not report a new API
as required. The media collision is a distinct shared producer cause, not a
reopening of earlier kind/payload-validation findings.
For owner FIND-001-17, record omission/default/binding and mutation cases,
runtime-default regression proof, and full-spec/hash checks in the existing
downloaded-Agent journey. Retain its owner-directed provenance; do not claim
this was an independently discovered requirement in the original review.

Return this task, original approved r6/task, prior R1/R2 and verdicts/ledgers
with a new immutable candidate to `$wyrd-task-review`. The next review still
uses the complete original base-to-candidate range. No material decision is
pending; if a genuinely new approved-behavior conflict appears, identify it
concretely rather than seeking approval for reversible implementation choices.
This packet authorizes no merge, push or deployment.
