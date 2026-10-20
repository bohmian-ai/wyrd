# TASK-003 focused follow-up — aggregate label collision

Result: **RESOLVED**. This resolves the correction-boundary conflict between B-003-8 and MNT-003-08; it is discovery evidence for independent validation, not a task verdict.

Subject: original base `695ca914df61a8683059f6f32279ec2148424e06`, immutable candidate `c2bbd8e47e4752b3772ad75acfaf23f3400ac704`. HEAD still matched that candidate during inspection. Only this report was written.

## Applicable authority and inspected paths

Read the subject and claims comparison, then only the relevant B-003-8 and MNT-003-08 discovery claims. Independently inspected the original TASK-003, approved revision-12 spec REQ-025 and REQ-030, repository rules, reference router, spec-driven-development and maintainer guidance, applicable design/doctrine boundaries, Wyrd UI skill and its chart/UX/Svelte/testing guidance, and brand DESIGN.md's series-encoding rule. There is no CodeGraph index.

Inspected the complete cumulative diff and current bodies of the applicable producer and renderer: `packages/ui/src/lib/plot.ts` and `packages/ui/src/lib/components/Chart.svelte` (UI-relative paths below), plus the latest remediation diff. Inspected `shapeChart`/`loadChart`, public index/package exports, fixtures and cardinality/component assertions, WyrdView's Chart branch, gallery, and custom-app direct/view consumers. A repository source search across crates/examples/SDKs found only Chart calling `timeSeries`; the loader imports only `columnIndex` from that module. The three keyed loops and aria-label are sibling consumers of one derived `lines` array, not separate aggregation owners. `Series`/`timeSeries` have no declared public package export.

REQ-025 permits arbitrary text values in the optional series column and requires overflow folding into “other”; it reserves no authored text value. REQ-030 requires a complete Chart in the supported hosts. Neither authority requires a new series identity field, nor two indistinguishable displayed captions for authored and synthetic data. The existing brand rule provides distinct dash/marker combinations in plot and legend; it does not establish an identity channel in the tooltip.

## Producer-to-consumer evidence

1. `loaders.ts:122–134` validates the series column's UTF-8 type, without reserving `other`. Both direct props and executor-loaded results can therefore supply this ordinary value.
2. `plot.ts:32–58` first groups authored series, then, above twenty groups, keeps nineteen by descending total and appends a separate synthetic `['other', other]` entry. A high-total authored `other` survives independently. Its measured points are not accidentally merged with the aggregate at this stage.
3. `Chart.svelte:42–44` uses that result directly. The plot, tooltip and legend key entries by `line.label` at lines 233, 256 and 263. Installed Svelte's `src/internal/client/dom/blocks/each.js:758–781` explicitly rejects duplicate keys in its validation path; reconciliation also indexes items by key. This is a reachable rendering defect, not merely a naming preference.
4. Additional sibling consumption matters when selecting a fix: the SVG accessible name joins labels at line 209, the tooltip prints plain `<span>{line.label}: {value}</span>` at lines 254–259, and legend text prints the same label at line 270. Tooltip spans have neither the plot's dash/marker nor another aggregate discriminator. Removing keyed identities leaves `other: 21` and `other: 3` with no explicit indication of which is an authored measurement and which is a sum of omitted groups. The accessible series list also repeats `other` without qualification. Legend dash/marker differences distinguish two drawn lines but do not identify their different meanings.
5. WyrdView instantiates this same Chart at lines 313–314; custom-app direct components and the MCP artifact also consume it. No host-specific guard or executor change addresses the source of the collision.

Fresh read-only probe, using existing built `packages/ui/dist/plot.js` from the UI workspace through `mise exec -- node --input-type=module -e ...`: supplied a single valid timestamp, twenty text groups `s0` through `s19` with values 1 through 20, and a text group `other` with value 21. Called `timeSeries(data, 'at', ['v'], 'series')`. Result:

```json
{"count":20,"unique":19,"duplicates":[{"label":"other","points":[{"x":1792490400000,"y":21}]},{"label":"other","points":[{"x":1792490400000,"y":3}]}]}
```

Built/source folding bodies agree. This probe reproduced the data state; a new browser/mount reproduction was not run. The installed renderer source establishes why the three keyed consumers cannot safely accept it. Existing `ui-library.test.ts:111–124` covers 5/20/21 groups named only `m1..m21`, so its meaningful threshold and sum proof does not cover this value.

## Selected smallest sufficient correction

Retain **one finding and one producer correction boundary**: B-003-8 and MNT-003-08 describe the same reachable cause. No additional finding is proposed.

Correct the synthetic aggregate's display label in the existing `timeSeries` overflow branch so it remains recognizably the “other” aggregate and is distinct from every retained authored label. Preserve the exact authored labels and points, ranking, retained count, sum per instant, ordinary non-colliding `other` caption, and all behavior at or below twenty series. A deterministic aggregate qualification, with collision avoidance against the retained labels, is sufficient. A single fixed alternate spelling is insufficient because that spelling is also valid authored text.

This reuses the existing `Series.label` representation and fixes all four consuming surfaces together. It needs no new public identity, schema, aggregate flag, renderer normalization, host behavior, dependency, or durable contract. The exact local spelling/suffix mechanics are reversible implementation choices; the required observable outcome is a distinct, identifiable aggregate caption that cannot collide with retained authored captions.

Assessment of the competing proposals:

- **B-003-8's producer/display correction is sufficient**, narrowed to the existing label field. Interpreting “distinct synthetic identity” as a requirement to introduce a new ID field would add machinery without closing any additional required behavior here. A new ID alone would still leave the tooltip ambiguity.
- **MNT-003-08's unkeyed/index-loop correction alone is insufficient.** It removes the renderer's key precondition but preserves ambiguous authored/aggregate captions in the tooltip and accessible name. Adding more tooltip/legend aggregate metadata afterward would require more consumer changes than correcting the producer's synthetic caption once. This does not mean ordinary stateless loops always need semantic keys; it means changing those loops is incomplete for this produced state.

The recommendation does not change REQ-025's folding policy or reserve an authored value. Qualifying the existing aggregate caption on collision is a private presentation correction within approved Chart behavior, not a material public-contract decision.

## Smallest credible closure proof

Extend the existing `TASK-003: Validated portable rendering` case in `src/lib/ui-library.test.ts`; use its existing Chart render/focus/rerender support. No live server or new harness is necessary for this pure result-to-render transformation.

- Mount twenty-one groups including a retained authored `other`; assert twenty lines and legend entries, the authored label/value 21 intact, the aggregate sum 3 under a distinct aggregate caption, and no duplicate-key failure. Focus the tooltip and assert both separately attributable values; verify the accessible name carries the distinct captions.
- Include retained authored text equal to the chosen aggregate qualification so the proof rejects merely replacing `other` with a different fixed string.
- Rerender with changed ranking/cardinality and verify labels, values and cues match the current data. Preserve existing 5/20/21 threshold and fold-total assertions.

Focused command:

```bash
mise exec -- pnpm --dir crates/wyrd/wyrd-server/wyrd-ui test src/lib/ui-library.test.ts -t '^TASK-003: Validated portable rendering$'
```

Then the existing UI check/build commands cover the producer's package and renderer consumers; no codegen change is needed unless implementation unnecessarily changes a generated/public contract.

Skipped: implementation, source edits, new test/harness files, broad verification and a fresh browser mount. Risk remains the reproduced collision until the bounded correction is implemented and independently validated.
