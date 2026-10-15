# Focused follow-up — portable media identity and TypeScript proof

**RESOLVED.** Immutable subject: base `1d3b5ffa0d34f75fa5b22c9b68f6ec0fc8d0cc70` to candidate `3ed5d90ff53a2ed082decd66bf7aab168a5afdcb`, read from `/tmp/wyrd-task-review-3ed5d90f`. This fresh review investigated only the two uncertainties in `claim-comparison.md`; it did not review Agent export or other boundaries. No production source, tests, assertions, builds, or runtime executions were changed or performed.

Authority: approved deployment-addressing spec r6 REQ-006/008/011 and AC-004/006/009; original task; R2 FIND-001-13 correction and proof; repository agent rules, spec-driven development and maintainer guidance. The archive has no CodeGraph index, so direct source navigation was used. Historical review claims were hypotheses, not conclusions.

## FUP-01 — new compiler identity collision is reachable

**Proposed classification: INCORRECT; new finding candidate FIND-001-16, subject to independent validation.**

The compiler silently substitutes a literal attachment for an unrelated user binding when both have the same internally generated name. This violates REQ-006's ordered typed media binding and REQ-008/AC-006's correct derived introspection and refusal of missing input.

Producer: `crates/skald/skald-spec/src/authoring.rs:215–234,352–408,449–487`. `PortableMessage::lower` accepts the whole `${media:__wyrd_media_0}` string as a binding, records its declared kind, then independently names the first literal attachment `__wyrd_media_0`. There is no occupied-name check, either for already encountered placeholders or later messages. `AuthoredPrompt::compile` builds a Prompt, binds every literal using the ordinary binder, and only then attaches portable `media_kinds`.

Grammar and normalization: `prompt.rs:155–188,294–312,336–374,448–454,652–674`. Leading underscores are explicitly accepted. `${media:name}` is a separate grammar from `${name}` / `{{name}}`; text variables with the same bare name do not themselves collide because the binder matches the media token. Media tokens in ordinary message text are normalized too, so collision prevention must include those legal authored media tokens, not only structured Image/Document binding declarations. System media tokens remain forbidden by the existing owner; this correction must not relax that rule. Scanning only messages already lowered is insufficient when a colliding authored placeholder occurs in a later turn.

Consumer: `prompt.rs:513–560,988–1017,1067–1122`. `bind_media_mut` normalizes all media tokens, and OpenAI Chat, Anthropic, and Google binders deliberately replace **every** matching token. The resulting name is removed from derived `media_variables`, and `render` at `prompt.rs:569–581` checks only the remaining names. OpenAI Responses uses the same repeated-name semantics at `1027–1065`, although portable lowering currently chooses Chat rather than Responses. These sibling binders are not independent defects: replacing every repeated authored binding is correct behavior.

A valid OpenAI authored example is:

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

Compilation uses the fixed URL for **both** positions, drops the only caller media variable and permits rendering without the requested input. With a Document binding followed by literal Image, that first position is silently converted to Image before its expected kind is attached. The same producer failure reaches Anthropic and Google with their supported literal base64 sources. Custom portable providers use the OpenAI shape; Vertex shares Google lowering, without implying local Vertex gateway support. Jev refuses portable authoring, so no Jev portable correction is needed. Native authoring does not allocate literal placeholders and must keep its current untyped media behavior.

Reachability is not test-only: `Prompt::Deserialize` (`prompt.rs:93–103`) invokes `AuthoredPrompt::compile`; `PromptSpec::Deserialize` (`crates/wyrd-spec/src/card/prompt/spec.rs:62–78`) uses that runtime Prompt, and `PromptSpec::new` recompiles the authored form (`spec.rs:31–36`). Thus normal authored Card registration, inline Prompt deserialization, bundle hydration and direct runtime loading all use this same invalid producer. Valid authored JSON/hash can remain unchanged while the effective compiled attachment is wrong.

**Selected minimum correction:** extend the existing shared portable compiler so its temporary literal-media identities are disjoint from every authored media token in the complete portable content before ordinary binding occurs. Reuse the existing placeholder grammar and binder; preserve all currently valid authored names, repeated binding replacement, order, expected kinds, source validation and native semantics. A new public reserved prefix, random-name contract, provider-specific guards, first-match binder, second compiler or new harness is unnecessary and would either leave collisions possible or regress supported behavior. Merely attaching expected kinds earlier can turn some cross-kind collisions into refusals but does not fix same-kind silent replacement; the identity producer owns this defect.

**Focused closure proof:** extend existing `authoring::tests::portable_media_bindings_keep_their_kind` or `portable_content_settings_and_schema` through the shared compiler for OpenAI/Anthropic/Google. Use supported literal base64 media plus authored `__wyrd_media_0`, include the binding before and after the literal (including another turn), repeated bindings and media tokens in message text. After compile, the exact authored media name must remain derived; render without input must refuse; binding distinct input must preserve literal versus supplied positions and repeated-name semantics. Include a Document binding/Image literal to preserve the kind refusal. Preserve a same-bare-name ordinary text-variable assertion because media and text grammars are separate. Reuse existing fixture media and exact test selectors, no new harness:

```sh
mise exec -- cargo nextest run --locked -p wyrd-skald-spec --lib -E 'test(=authoring::tests::portable_media_bindings_keep_their_kind) | test(=authoring::tests::portable_content_settings_and_schema)'
```

This source trace establishes the defect without claiming a runtime RED run. Exact additional test names, if introduced, must be recorded by implementation.

## FUP-02 — retain FIND-001-13 only for required nonempty-media proof

**Proposed classification: MISSING verification; retain prior FIND-001-13 with narrower correction.**

The current production projection is present and appropriate: `sdks/wyrd-sdk-ts/native/src/cards.rs:651–662` reads `prompt.variables`, `prompt.media_variables` and shared schema metadata; standalone lookup calls it at `447`, and hydrated Agent projection at `641`. `sdks/wyrd-sdk-ts/wyrd/src/index.ts:2370,2387–2396` exposes `mediaVariables` directly, without re-deriving data in TypeScript. Do not prescribe another production API or parser.

The current journey at `wyrd/tests/integration/register-and-hydrate.test.ts:177–225` only authors text in both portable/native fixtures and asserts `hydrated.mediaVariables == []` for all four navigation forms. That proves empty projection, not that media names survive the new N-API boundary. An always-empty implementation would pass this assertion.

This is explicitly required proof, not optional coverage expansion: R2 FIND-001-13's correction says "Include text/media variables" and focused proof at lines `206–210` says "include existing media names through existing fixtures". Approved spec lines `589–596` requires deriving media variables using existing grammar on hydrated objects, and AC-004 at `975–981` requires derived introspection across the three SDKs. The R2 evidence's honest empty-only limitation does not waive that obligation. Existing Rust kind/binder proof cannot exercise this new language handoff.

**Selected minimum correction:** update the existing authored portable standalone and inline fixtures consumed by this exact register/GET/hydrate test to contain real named Image and Document bindings and assert their exact nonempty derived names on both public navigation paths. Retain authored JSON/hash equality, text/schema assertions and native/plain-text empty metadata cases. No production code change is warranted unless that check actually exposes a defect. This also avoids creating a second registry/hydration journey or independent test harness.

```sh
mise exec -- scripts/postgres/with-test-postgres.sh -- pnpm --dir sdks/wyrd-sdk-ts/wyrd exec vitest run tests/integration/register-and-hydrate.test.ts -t '^prompt forms round trip as authored$'
```

Use normal existing native build prerequisites and only the narrow fixture/TypeScript checks for its write set.

## Limits

This is static source and authority validation; no runtime proof or fresh green result is claimed. It resolves both requested uncertainties and supplies the smallest shared correction boundaries for the separate independent validator. Other prior-finding closures and the owner-raised Agent export issue remain outside this follow-up's scope.
