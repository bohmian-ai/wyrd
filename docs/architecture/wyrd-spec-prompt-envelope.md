# wyrd-spec Prompt Envelope

The PR3 PromptCard envelope in `wyrd-spec` is a thin Wyrd card contract around a
native Skald prompt.

## Wrapped Native Prompt

`PromptSpec` contains a single native prompt payload:

```rust
pub struct PromptSpec {
    pub prompt: skald_spec::Prompt,
}
```

The Wyrd layer owns card validation, content hashing, codec helpers, and
references. Provider request and response shapes stay in `skald-spec`.

## Allowed Dependency Arrow

PR3 permits one Wyrd-to-Skald dependency arrow:

```text
wyrd-spec -> skald-spec
```

That arrow is allowed because `skald-spec` is a pure contract crate:
PyO3-free, IO-free, async-free, and wasm-safe. `skald-spec` does not depend on
`wyrd-spec`, and Skald engine crates do not import Wyrd contracts.

## PromptRef

`PromptRef` is defined with two variants:

- `Card(CardRef)`: points at a registered Prompt Card by Wyrd card identity;
- `Inline(Box<PromptSpec>)`: carries an inline prompt spec.

PR3 defines and tests the shared reference type, but it does not add
`PromptRef` fields to other card specs. Other card kinds will add their own
prompt-reference fields in their own PRs.

## Envelope Responsibilities

`wyrd-spec` stays a pure contract crate. The prompt envelope provides:

- validation for text variables, media variables, model, and response schema;
- content hash computation over native prompt JSON with prompt version cleared;
- JSON/YAML byte codecs for Prompt Card envelopes and bare prompt specs;
- Wyrd error mapping for prompt validation and loader failures;
- native Skald type re-exports needed by card holders.

Filesystem IO lives in `wyrd-cards`, not `wyrd-spec`. Python lifetimes live in
`skald-prompt` and `wyrd-cards`, not `wyrd-spec`.
