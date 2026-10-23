# Focused follow-up — embedded custom-data identity

**RESOLVED.** BEH-1 is a reachable remainder of **FIND-TASK-001-1**. The claims that the resource/dialect defect is fully closed are too broad. The correction belongs at the shared schema producer, `Verdict::response_format`, together with its existing Skald schema-validation boundary; neither runtime consumer nor a result reader can correct changed schema meaning.

## Immutable subject and scope

Base `fb1fd9234acf9ff20b1feff89bf9f3da9cba4f14`; candidate/HEAD `d2ffc59873b5af688adf1d1e5b2a3a388160bb83`. Read `subject.md`, revision-1 spec, original TASK-001, attempt-02 validated diagnosis/R1, and the conflicting claims in attempt-03 behavior, invariant, system, and security reports. CodeGraph preceded code navigation. Inspected the cumulative diff for the affected producer/compiler/runtime paths, their actual source, applicable Prompt/Verifier architecture, and installed jsonschema/referencing 0.46.4 source. This is the requested identity uncertainty only, not another whole-task audit. No source changes or Cargo operations were made.

## Source trace and conflict resolution

1. `skald-spec/src/prompt.rs:246–272::check_output_schema` explicitly supports drafts 4, 6, 7, 2019-09, and 2020-12; no declaration defaults to 2020-12. It meta-validates without resolving user references.
2. `wyrd-spec/src/verification/execution.rs:569–627::Verdict::response_format` validates custom data, retains its dialect, and unconditionally inserts **`$id`** only when absent (618–619). This is not an effective draft-04 resource identifier. The rustdoc's statement that every embedded schema gets its own reference root is consequently false for accepted inputs.
3. `card/prompt/spec.rs:68–72::compile_prompt`, inline Agent decoding (`card/agent.rs:157–177`), and `CodeCheck::response_schema` (`card/verifier/mod.rs:488–496`) share this resolver. `AuthoredPrompt::compile_with/compile_resolved` (`authoring.rs:396–447`) lowers its output into portable/native provider settings, preserving authored Card representation.
4. Installed `referencing-0.46.4/src/draft.rs:86–93,113–115` selects `id` for draft 4, `$id` for later drafts. `spec/ids.rs:14–38` and `spec/mod.rs:60–100` also show that a fragment-only legacy ID is an anchor, rather than a new resource base, and a classic-draft root `$ref` suppresses its sibling ID. The registry resolves IDs against their enclosing base and records whether that base changed (`registry/build.rs:1280–1298,821–841`). Actual resource identity, not merely presence of a keyword, matters.
5. Both production consumers use the same validator: `vala-verify/src/blocks.rs:198–215::verdict_from` calls `jsonschema::validator_for`, checks the complete response, then decodes Verdict and checks bounds. Agent execution passes the compiled Prompt schema (`blocks/agent.rs:475–480`); hosted-code execution passes `CodeCheck::response_schema` (`hosted/sandbox.rs:370–374`). A failed output with empty citations reaches accepted CheckOutput without the Agent pass/citation downgrade. Validating the wrong schema therefore permits retained data that the author prohibited; the opposite input incorrectly becomes inconclusive.

The invariant/system/security reports correctly describe closure for the covered anonymous default/2020-12 and draft-07 cases. Their tests do not establish that every accepted dialect or authored identifier produces an independent resource. BEH-1 falsifies that stronger closure claim.

## Independent reproduction

Used `/tmp/followup-schema-probe.rs` and `/tmp/followup-schema-probe-extra.rs`, compiled with `mise exec -- rustc --edition=2024` against the existing repository-built `libjsonschema-d4dbafcac2763e43.rlib` and `libserde_json-b8fd2d102c1e5672.rlib`. No Cargo build, repository test/source modification, network reference retrieval, or provider request was needed. `/tmp/behavior-draft4-schema.json` supplies the actual compiled canonical envelope emitted by the earlier public Prompt experiment; the fresh probe independently varies only its custom schema and applies the exact candidate insertion operation. The source trace above independently checks that transformation.

The declared object is:

```json
{"$schema":"http://json-schema.org/draft-04/schema#","type":"object","required":["reason","copy"],"properties":{"reason":{"type":"integer"},"copy":{"$ref":"#/properties/reason"}}}
```

Compare these complete responses, using outcome `failed` to avoid citation-policy interference:

```json
{"outcome":"failed","reason":"judged","citations":[],"data":{"reason":1,"copy":2}}
{"outcome":"failed","reason":"judged","citations":[],"data":{"reason":1,"copy":"wrong"}}
```

| Schema/resource case | Good integer copy | Bad string copy |
| --- | --- | --- |
| Standalone authored draft-04 schema, validating `data` alone | accept | refuse |
| Actual candidate draft-04 embedding, only injected `$id` | refuse | accept |
| Same embedding with standard draft-04 `id: verdict-data` | accept | refuse |
| Candidate draft-04 with authored `id: https://example.org/data` | accept | refuse |
| Candidate draft-04 with authored `id: authored-data` | accept | refuse |
| Candidate default/6/7/2019-09/2020-12, anonymous or usable authored `$id` | accept | refuse |

Additional probes within the same resource-identity boundary prevent an incomplete remediation:

| Authored effective ID | Standalone | Candidate embedding | Merely selecting correct `id`/`$id` |
| --- | --- | --- | --- |
| `""`, `"#"`, `"."`, `"/"`, each supported draft/default | good accepted / bad refused | good refused / bad accepted | still wrong |
| draft-04 `id: "#foo"` | good accepted / bad refused | good refused / bad accepted | still wrong |
| nonempty relative `authored-data`, absolute `https://example.org/data`, `verdict-data#` | good accepted / bad refused | correct for applicable keyword | preserved correctly |

These are meta-schema-valid inputs, not unsupported custom dialects. With the candidate's anonymous envelope base, the no-op IDs resolve to the enclosing resource; fragment-only legacy IDs name an anchor in it. Presence of an authored ID therefore is insufficient. A draft-04 keyword-only fix closes the anonymous example but does not establish the required producer invariant for authored identities. Nonempty fragments in modern `$id` already fail the existing 2020-12 syntax validation and need no extra check.

## Standards and smallest correction

Draft-04 Core §7.2.2 uses `id` resolved against the parent scope; §7.2.1 discourages empty IDs/fragments and §7.2.2 recommends unique scopes. [Official draft-04 Core](https://json-schema.org/draft-04/json-schema-core#section-7.2).

2020-12 Core §8.2.1 defines `$id`/parent-base resolution and treats an empty fragment as the same resource. §§9.3.1–9.3.2 describe embedded resources and dialect inheritance. A declared dialect and an independently identified resource must travel together. [Official 2020-12 Core](https://json-schema.org/draft/2020-12/json-schema-core#section-8.2.1), [Compound documents](https://json-schema.org/draft/2020-12/json-schema-core#section-9.3).

Retain **one** finding, continuing FIND-TASK-001-1, classified INCORRECT against REQ-001/002/007 and AC-003/004. Its observable consequence is exact-schema violation in both Agent and hosted-code checks, followed by ordinary persistence/readback. No new product decision is required: the approved spec already requires exact meaning and existing compile errors for unsupported compositions.

Selected correction boundary and preserved behavior:

- Correct the standard resource construction at `Verdict::response_format`; use the effective supported draft (2020-12 when absent), including draft-04 `id` and later `$id`, and keep authored dialect/usable identity unchanged. Reuse the installed draft/URI interpretation at the existing Skald schema boundary and existing URI dependency as appropriate. Do not implement another draft/reference/schema walker, validator, keyword translation, definitions hoisting, compiler, or per-executor guard.
- An absent effective identifier receives the resolver's existing private resource identity, in the applicable draft's spelling. An authored identifier that actually establishes a distinct usable resource remains authored; do not replace it, require absolute IDs, reject a whole supported draft, or apply a broad relative-ID ban.
- If the authored identifier cannot establish an independent embedded resource without changing its meaning, refuse that **composition** before persistence through the existing `SkaldError::PromptInvalid` field/remediation path. This includes proven empty/fragment-only/same-base identities. Determine scope through standard URI resolution, not a blacklist of the demonstrated strings (`.` and `/` show why checking only empty strings is insufficient). This guard belongs at resource construction because it enforces the invariant that owner promises; it is not another schema-validation layer. Legacy root `$ref` suppression shown by the installed library must also be respected when deciding whether the proposed identity is effective. Do not silently overwrite authored IDs or retain a known-wrong emitted schema.
- Keep plain/handwritten formats, authored Card hash, citation policy, bounds, result storage, errors, SDK conversion and runtime consumers unchanged. Cross-dialect syntax currently refused by the enclosing compiler stays outside this correction.

Focused closure proof should extend the existing resource-root regression table (`blocks.rs::verdict_data_keeps_its_schema_resource`) with integer-good/string-bad anonymous draft-04 and authored usable draft-04 identities, retain all default/later-draft cases, and add declaration refusal for no-op authored identities without rewriting the authored form. Include both supported nonempty relative and absolute IDs so the correction cannot overrestrict authoring. Exercise the corrected resolved schema in the existing Agent and code focused execution proof; no new harness or broad journeys are necessary for this remediation. The exact selector/task lanes remain the implementation owner's existing ones.

## Limits

This follow-up independently ran installed-Rust-validator reproductions, not full executor/Postgres journeys or new Cargo tests. Standard URI/library source determines the correction invariant; this review does not prescribe a new helper/API or replace implementation-owned local decisions. The full supported-ID space was not exhaustively enumerated. The independently validated ledger must decide the retained finding and final remediation.
