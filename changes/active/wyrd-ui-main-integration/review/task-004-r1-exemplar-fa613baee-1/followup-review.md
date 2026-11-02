# Focused exemplar stop-condition review

**Result: UNRESOLVED capability claim; prior FIND-TASK-004-REV-02-4 remains open.** The current raw exemplar JSON path does lose unsafe integer precision in JavaScript. The report does not establish that closing it requires a new server/public contract, and “server-side change” is not itself a material stop condition.

## Subject and scope

Repository `/home/thorrester/Documents/GitHub/wyrd`; original base `4a76331113864f9bddc43d3cc8df01080a696937`; prior candidate `f6aeb83a77179ac4450110671df6f564adc1914a`; candidate `fa613baee5e70f9458442f22d74ecebc22893ebc`. Assignment: only the exemplar portion of prior FIND-TASK-004-REV-02-4 and its asserted stop condition. This is not a full F1–F10 or visual acceptance review. Source was immutable; only this report was written. CodeGraph was used first for JsonResultBuilder/NESTED_EXACT/exemplar projection navigation.

Authorities read: original task precision paragraph and Material Stop Conditions; approved spec REQ-028/039 precision requirements; prior validated F4 and current R1 evidence; Bifrost Variant SQL architecture. Inspected shared UI Metrics projections, current exemplar test seed/assertions, canonical metrics schema, queue JSON encoder and Variant factory, TS bounded collect consumer, and pinned installed DataFusion sources/tests for Variant JSON, named_struct, unnest and ordered list/string aggregates.

## What is actually failing

1. Canonical `crates/vala/vala-bifrost-redux/src/tables/metrics/points.rs:80–96` stores an exemplar as a list element struct with Int64 `int_value`, Float64 `double_value`, Variant `filtered_attributes` and trace/span identity. Storage therefore retains the integer as an integer; this evidence does not establish storage corruption.
2. UI `packages/ui/src/lib/observe.ts:285–320` spells scalar counts and other nested counts as decimal text but leaves `exemplars` unchanged: NESTED_EXACT includes bucket_counts/positive_buckets/negative_buckets only.
3. `crates/shared/wyrd-queue/src/json_result.rs:120–147` encodes each Arrow value with the current Wyrd JSON factory and accepts the JSON into serde_json values; the factory's `variant.rs:463–525` keeps default Arrow encoders for ordinary integers and specially renders actual Variant extension fields. This path returns the exemplar integer as a JSON numeric literal.
4. `sdks/wyrd-sdk-ts/wyrd/src/index.ts:666–673` uses JSON.parse on the bounded result. JavaScript Number cannot distinguish every integer above 2^53−1: the literal 9007199254740993 becomes 9007199254740992. Once parsed, frontend formatting cannot recover the original digit.
5. Current `ui-pages.integration.test.ts:309` seeds an exemplar **double** 7.5 and readable `/exemplar` attributes; `:406–418` asserts structure/attributes and other unsafe counts. It does not prove unsafe exemplar int_value. Thus the current evidence cannot close that part of F4.

Plainly: an exemplar is a sample attached to a metric point, carrying its measured value, selected attributes and trace context. The value remains exact in the stored typed record, but its ordinary JSON-number representation is rounded when the JavaScript client reads it. The UI can show the surrounding sample correctly while showing the wrong last digit of a large integer.

## Why it blocks completion

Original task lines 148–157 expressly require exact display through existing SQL/results, including lossless nested projections, with approximation allowed only for disclosed numeric plot geometry. The reviewed R1 task likewise demands exact retained structures and unsafe-count proof. Leaving this exemplar integer silently numeric does not fulfill that obligation; declaring all F1–F10 implemented is therefore too broad. F4 remains incomplete even if the rest of the Metrics page is usable.

The task's stop conditions at lines 493–500 cover a durable schema/migration, new execution authority, credential/tenant change, unsupported public contract, a **new wire encoding**, or conflict with architecture. They do not prohibit all server-side work. Indeed lines 151–154 explicitly authorize lossless **server-side projection** through existing SQL/results. A private correction in an existing server owner does not automatically create a new product/architecture decision. Whether it changes the public result contract must be shown specifically.

## What the reported experiments establish—and do not

- Reading `exemplars` as-is genuinely leaves the unsafe JSON-number path described above.
- Whole-list/struct string casts can expose the Variant storage fields instead of readable attributes because those operations erase or bypass the field's extension semantics. The Wyrd encoder explicitly relies on the Variant field extension. Pinned DataFusion `datafusion/functions/src/core/named_struct.rs:98–148` constructs new child `Field::new` values from types without original extension metadata, so indiscriminate rebuilding is also unsafe for retaining a raw Variant child.
- A normal cast to Utf8 is not the same operation as Variant-to-JSON decoding. **An existing Variant-to-JSON SQL capability already exists**: architecture/bifrost-design.md:898–922 specifies `to_json(v)`, and pinned DataFusion `functions-variant/src/json.rs:103–172` implements it as Utf8 JSON text. Therefore “closing it needs server-side Variant-to-JSON” is not proof that a missing capability must be added.
- Unnesting by itself changes result cardinality; it is not proof that a final per-point list cannot be reconstructed. The pinned fork has `array_agg(expression ORDER BY ...)` (`functions-aggregate/src/array_agg.rs:68–114`), ordered string_agg (`functions-aggregate/src/lib.rs:90,184`; aggregate SQL tests around 8050–8100), to_json/parse_json and nested struct/list access. Existing Wyrd nested-read test `oracle/nested_pushdown.rs:928–945` demonstrates that an unnested list's Variant attributes remain addressable by Variant operators.
- These functions supply source-supported avenues to extract each exemplar's value as text, decode attributes using the existing function, and reassemble a list or JSON-list projection with original order and per-point membership. **This review has not executed such a complete query and does not claim it is proven to work.** Correlated lateral UNNEST has documented physical-planning limitations in the pinned fork, so an arbitrary proposed lateral query is not acceptable evidence either. Any reconstruction must preserve duplicate metric points, element order, empty/null lists, exact scalar alternatives and payload permissions; grouping by a guessed metric identity would be wrong.

The four reported forms establish failures of those forms, not exhaustive impossibility of the existing SQL/result path. No exact probe transcript/query was committed in the R1 evidence; the integration test does not cover the unsafe exemplar case. The broad inability claim must therefore remain unproven.

## Correction boundary and review routing

Retain existing FIND-TASK-004-REV-02-4; do not invent a new finding or force SPEC_REVISION_REQUIRED merely because the remaining work crosses into an existing server owner. The selected approved boundary is still the shared Metrics SQL/projection owner, before JSON-number parsing, preserving exemplar list/attribute meaning and exact decimal display through the existing result format.

Before approving a wider boundary, obtain a narrow real-SQL proof covering reconstruction with the existing to_json/parse_json, list/string aggregate and nested access capabilities—or a concrete unsupported planner/encoder operation explaining why that mechanism cannot preserve the required shape. A narrow existing-owner server fix may be valid without a spec revision when it preserves public result semantics. A global policy converting existing numeric result values to strings, a new tagged JSON encoding, Arrow transport, durable schema or public API would change the contract and must use the original stop-condition process. Do not globally change JsonResultBuilder numeric semantics as an unreviewed UI fix.

Smallest closure evidence is an exemplar with int_value 9007199254740993, readable nested attributes, multiple ordered elements, empty/null behavior and duplicated parent points through real SQL → bounded result → browser display. Existing UI integration/loader tests can carry this case. Add no new transport or separate metric detail service, and do not substitute a visibly rounded value for exact evidence. Full development replay is irrelevant to proving this precision seam.

## Limits

Static source/caller review only; no Rust builds, live SQL, Postgres/server/browser replay or dependency changes. A local Python DataFusion probe could not start because this mise environment has no `python` executable; no fallback environment was installed. No standalone DataFusion SQL test executable was found in the existing target artifacts. The report deliberately distinguishes an independently established precision defect from an unproven exact SQL fix or unproven need for a public contract change. HEAD remained fa613baee5e70f9458442f22d74ecebc22893ebc.
