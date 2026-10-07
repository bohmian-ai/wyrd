# SPEC-bifrost-variant revision 21: one stratified sampling algorithm

Status: approved by the user on 2026-10-07. Substance agreed in
`tasks/TASK-003-decisions.md` §2.

Kept apart from `spec.md` for the same reason as revision 20: `spec.md`
carries uncommitted revision 19. When revision 19 is committed, revisions 20
and 21 are applied to `spec.md` (frontmatter `revision: 21`) and these files
are deleted.

## Why

Today each Scribe hot object and each Forge output picks its Variant layout
from its first rows, buffered up to 4,096 rows or 64 MiB. Both limits are
arbitrary. The first rows of a sorted, merged file are not representative. A
writer with few rows is easily missed. The buffer needs memory accounting
(revision 19's REQ-031 and AC-013).

Revision 21 replaces that with:

- Scribe takes one seeded, stratified sample of the whole claim before the
  writer opens.
- Forge combines its source files' layouts from their footers and samples
  nothing.

No rows are buffered, so the prefix, its caps, and its memory accounting are
deleted.

## Changes to `spec.md`

### "Per-output-file shredding policy"

Replace the paragraphs from "For Forge, the same fork provides one concrete"
through "or public memory setting is added." with:

> The layout of a Variant column is decided before its writer opens, so no
> writer buffers rows to infer it.
>
> **Scribe** samples each final-object claim with one seeded, stratified
> algorithm:
>
> - **Strata.** Rows are grouped by the server-injected `principal_id`. A
>   table without that column is one stratum. Writers with fewer than 30 rows
>   in the claim are merged into one shared stratum.
> - **Sample size.** Each stratum gets the same precision, using Cochran's
>   formula for a proportion: `n0 = z² · p(1−p) / e²`, with 99% confidence
>   (`z = 2.5758`), margin `e = 0.02` and worst case `p = 0.5`, so `n0 = 4147`.
>   With the finite-population correction, a stratum of `N` rows samples
>   `n = n0 / (1 + (n0 − 1) / N)`. A small stratum is therefore read almost
>   whole.
> - **Selection, in two passes over the claim's local, unshredded staged
>   runs.**
>   - Pass 1 reads only `principal_id` and counts the rows in each stratum.
>   - Pass 2 reads only the Variant columns and keeps a row when
>     `hash(seed, row position) < n_h / N_h`.
>   - The seed is the claim identity. Row position is the claim's
>     deterministic merge order. A re-run claim therefore picks the same
>     sample and writes the same objects. Whole row groups are never sampled.
> - **Decision.**
>   - A key is eligible when its sampled frequency among non-null Variant
>     values is at least 8% in any stratum. That is the 10% threshold minus
>     the margin, so the rule leans toward shredding a key rather than
>     missing it.
>   - Candidates are ranked by estimated rows covered,
>     `Σ_h N_h · p̂_h`, with ties broken by name.
> - **Memory.** The passes hold only per-stratum key and type counters, never
>   rows. All objects of one claim use the claim's layout.
>
> **Forge** does not sample. Every source data file's footer already names the
> keys that file shredded, with their types. Each shredded leaf's row count
> minus its null count is the exact number of rows holding that key with that
> type. Forge reads every source footer anyway, so this adds no read.
>
> - **Counting.** Summing the counts over the rewrite's source files gives
>   each key's exact share of the combined rows.
> - **Which keys survive.** Candidates are the keys any source shredded.
>   Forge keeps those whose combined share is at least the 10% threshold,
>   ranked by rows covered and capped as below. A key below the threshold in
>   every source is below it in the union, so no source shredding it is the
>   right answer.
> - **Type conflicts.** The type covering more rows wins. Other rows go to
>   the residual.
> - **Missing null counts.** If a footer lacks a leaf's null count, that file
>   counts every row as holding the key.
> - **Output files.** Every output of one rewrite uses this one combined
>   layout.
> - **What Forge cannot do.** It only keeps or drops keys; Scribe is where new
>   keys are discovered.
>
> Each written file records the share of its non-null Variant bytes left in
> the residual `value`, as an existing Bifrost metric labelled by table. A
> rising share means layouts are missing keys.

Replace the paragraph "The 4,096-row and 64-MiB limits are internal
writer-policy constants, ..." with:

> The sampling parameters (99% confidence, ±0.02 margin, 30-row stratum floor,
> 8% eligibility, 10% Forge threshold) and the child and depth caps are
> internal writer-policy constants, not public, wire, table, fingerprint, or
> persisted contracts. Changing them never changes file compatibility or
> query results.

In the inference-rules paragraph, replace "Fields present in at least 10% of
sampled non-null root Variant values are eligible." with "Fields are eligible
by the Scribe and Forge rules above."

In the following paragraph, replace "sampling changes performance only, never
correctness" with "the layout choice changes performance only, never
correctness".

### "Required system boundaries and flow"

Replace these three lines:

```text
  -> fork-owned analyzer handles the final Scribe object
  -> RollingFileWriter build(output) creates a fresh deferred Forge writer
  -> that final Scribe/Forge output retains at most 4,096 rows or 64 MiB
```

with:

```text
  -> Scribe: stratified seeded sample of the claim's staged runs picks the layout
  -> Forge: source footers' shredded-leaf counts combine into one layout
  -> each writer opens with its layout already known (no row buffer)
```

### REQ-020 — Per-file layout

Replace the first paragraph with:

> Scribe recovery-stage runs written by `encode_batch` remain unshredded and
> use the stable logical Variant schema. Each final Scribe hot object takes
> its claim's layout, chosen by the stratified seeded sample over the claim's
> staged runs. Each Forge output takes the rewrite's layout, combined from its
> source footers. Both writers open with the layout known. No writer buffers
> rows to infer a layout, so there is no prefix, row or byte bound, prefix
> memory charge, or oversized-first-row exception. Empty output creates no
> file.

Replace "Fields observed in at least 10% of sampled non-null root Variant
values are eligible when" with "Fields are eligible under the Scribe and Forge
rules when".

### REQ-021 — Choosing the layout before the writer opens

Retitle from "Choosing the layout without a second pass". Replace the body
with:

> The pinned iceberg-rust fork owns the one pure analyzer and the Arrow
> `ShreddedSchemaBuilder` / `shred_variant` wrapper.
>
> - **Scribe** runs the analyzer over its seeded, stratified sample before
>   opening the final-object writer. The two passes read only
>   `principal_id` and the Variant columns of local staged runs.
> - **Forge** gives the fork's `VariantParquetWriterBuilder` the layout
>   combined from source footers. Every rolled output uses it.
>
> Later incompatible values go to the standard residual `value`, so the layout
> affects performance only. The Parquet `metadata`/`value`/`typed_value`
> schema is the only persisted layout authority; no Wyrd summary or layout
> metadata is written or required. Scribe never infers or persists a shredded
> recovery run.

### REQ-031 — Forge holds no prefix memory

Retitle from "Forge accounts only retained prefix memory". Replace the body
with:

> Forge buffers no rows to choose a layout, so it reserves, predicts, or
> charges no prefix memory. The layout comes from source footers Forge
> already reads. No pool, ledger, reservation, spill path, or tuning surface
> exists for layout inference.

### INV-009 — No speculative Forge prefix capacity

Replace the body with:

> Forge admission reflects memory that the rewrite actually uses; layout
> inference adds none. Shredding does not change the managed core's
> selection, grouping, queue, fanout, result handoff, publication, or retry
> topology.

### AC-013 — Forge layout from source footers

Replace the title and body with:

> Tests show that:
>
> - a Forge rewrite's outputs all use the layout combined from source-footer
>   counts;
> - a key shredded in some sources survives only when its combined share
>   meets the threshold;
> - a type conflict keeps the type that covers more rows;
> - Forge reads no source Variant column before opening its writer and makes
>   no prefix reservation;
> - logical values, row lineage, result handoff, and publication are
>   unchanged.

### AC-015 — Stratified seeded sampling (new)

> Tests prove:
>
> 1. **Reproducible.** Re-running the same claim yields the same sample, the
>    same layout, and byte-identical objects.
> 2. **Small writers count.** A key common in one writer's rows (≥ 8% of its
>    sample) and rare overall is shredded, both when that writer has ≥ 30
>    rows and when it has fewer and is merged into the shared stratum.
> 3. **Cochran sizes.** Per-stratum sample sizes match Cochran's formula with
>    the finite-population correction: 4147 at 99% / ±0.02 for a large
>    stratum, and nearly all rows for a small one.
> 4. **Ranking.** At the 300-child cap, keys are kept by estimated rows
>    covered, with ties broken by name.
> 5. **No buffer.** The first-rows buffer, its 4,096-row and 64 MiB bounds,
>    and its memory charge are gone.
> 6. **Residual metric.** The per-file residual-share metric is recorded.

### Revision history

Prepend:

> - **Revision 21 (2026-10-07, approved):** By explicit human direction, replaces
>   first-rows inference with one seeded algorithm.
>   - Scribe samples each claim, stratified by `principal_id`: Cochran at 99%
>     confidence and ±0.02 margin (4147 rows per stratum, with
>     finite-population correction); writers under 30 rows merged into one
>     stratum; two-pass seeded hash selection; a key is eligible at 8%;
>     ranked by rows covered, capped at 300.
>   - Forge combines its sources' layouts from footer null counts and samples
>     nothing.
>   - The prefix buffer, its 4,096-row / 64 MiB bounds, and revision 19's
>     prefix memory accounting (REQ-031, AC-013) are removed.
>   - A per-file residual-share metric is added.
>   - REQ-020, REQ-021, REQ-031, INV-009, AC-013, the shredding policy
>     section, and the flow are reworded. AC-015 adds the proofs.
>   - REQ-030 (Forge parallelism) is unchanged.
