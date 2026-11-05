# Observe source and reference audit — Task 03

Audited source: `010ef9bab1932b299b8d72aae8f05fcfd7a3e15f` (includes R2 UI
candidate and default 500). Inputs: named Main, Metrics and Logs `.dc.html`
boards, plus TraceDrawer, their approved PNGs, current reusable components,
SvelteKit routes, canonical brand palette and existing browser assertions.

**Result: exact visual/layout/color alignment is not established, and source
proves differences that still need correction.** Existing components are used,
but their current composition and color assignment do not reproduce all the
approved regions. The table below is an audit result, not another acceptance
claim based on passing data queries.

UI paths start at `crates/wyrd/wyrd-server/wyrd-ui/`; board paths start at
`changes/active/wyrd-ui-main-integration/references/canvas/`.

| Region | Approved reference | Current source | Result and required outcome |
|---|---|---|---|
| Metrics chart and series cards | `Metrics.dc.html:73–104` chart card, `106–119` separate series-summary card | Observe `+layout.svelte:307–345` wraps a generic Chart that supplies additional framing/header, internal series table and SQL footer; `:349` adds a raw RecordTable | Different composition. Expose shared presentational seams for the two reference cards; retain exact reusable series selection and evidence inspection without an extra default raw-table region. |
| Selected Log detail | `Logs.dc.html:80–123`, detail `113–122` inside the same flex work region | Observe layout `:270–274` closes the split before rendering children; page `:82–88` renders log aside as child | Detail renders below rather than alongside the list. Compose it inside the records work region, with responsive equivalent on mobile. |
| Traces duration colors | `Main.dc.html:94–100` p95 lime `#c5f23b`, p50 periwinkle `#7a8cff` | `observe.ts:510` y order p95,p50; default Chart cycle begins declare,observe; route `:234` has no color override | Reversed assignment. Preserve exact semantic series names and use explicit reference role colors. |
| Overview density | Main chart areas are 56px | Chart compact box `:535–537` minimum64px with independent axes/header | Source dimensions differ; match reference composition/density and verify actual aligned plot baselines in browser. |
| Logs severity colors | `Logs.dc.html:65,72` info `#3a4170`, warn `#f0a13c`, error `#e2484d` | Observe LEVELS gives info `--declare` (`#7a8cff`); Chart segment opacity .85 | Wrong info fill and blended colors. Canonical semantic tokens must reproduce reference fills, including opacity. |
| Waterfall bars | TraceDrawer uses periwinkle/indigo/lime distinctions plus red errors | TraceWaterfall `:295–302` assigns every nonerror bar `--declare` | Reference palette distinction absent. Reuse shared recorded identity/series color handling and real-field legend; never infer domain meaning from invented span names. |
| Navigation surface and selected borders | Main rail `#0e1017`, selected/filter border `#3a4170` | Tenant shell uses `--surface` (`#12141c`); canonical palette lacks some reference roles | Add/reconcile roles at the canonical palette/brand owner, regenerate projections and reuse tokens. Do not copy a second set of hex literals into components. |
| Extra series hues | Metrics/Main use `#a9b4ff`, `#8fae2a`, `#4d5ef0` alongside declare/observe | Generic Chart cycle declare/observe/text/muted | Map reference series palette explicitly at shared owner with noncolor cues and consistent recorded identities. |
| Drawer and waterfall grid | TraceDrawer approx884px; waterfall grid300/110/flex/64, rows28px | Current page884px and shared TraceWaterfall matching grid/row height | Substantial R2 alignment present; preserve header, Link, Expand/Escape and reuse. Current rendered acceptance still unverified. |
| Trace list controls | Main listbar Columns/CSV | Route now supplies shared RecordTable controls in listbar | Placement seam present. Logs keeps Wrap/Columns in its records header, matching Logs reference; do not move it incorrectly just to share page markup. |
| Light theme | Canvas dark-only; Wyrd brand requires a complete corresponding light rendering | Current page test file contains no theme-selection/computed-color/screenshot assertions; latest reported light Metrics capture was dark | No valid light-theme closure. Assert selected/computed theme before every capture/check; inspect all affected light states. |

## Governing reconciliation for Task 03

The user's exact layout/style/color instruction must be reflected in the
canonical visual authority. Retain one palette (`brand/palette.json` and its
generator), existing fonts shipped locally, original Wyrd mark assets, flat
rules and accessible noncolor cues. Add the necessary semantic/nav/series roles
there and update the owning brand guidance to match the named reference colors;
regenerate instead of hardcoding them into app pages. Dark reference values
are exact targets. The light counterpart needs an explicit token mapping and
individual visual acceptance because there is no approved light canvas image.

Real tenant/recorded values and current canonical field names replace
illustrative data. Unsupported metric percentiles stay visibly unavailable;
the reference's estimated histogram p90 does not authorize invented results.
The existing explicit unsafe-exemplar precision deferral remains in effect.
These are data/contract constraints, not permission to change regions, density,
color assignments or default panel composition silently.

## Evidence limit and required next proof

This was a read-only source/reference-image audit, not a newly rendered-browser
review. Latest external candidate captures had no reproducible path in the task
evidence; deleted committed captures were not restored. No exact current pixel
match, mobile interaction pass or light-theme pass is claimed.

Existing production-BFF/Chromium owner:
`src/lib/ui-pages.integration.test.ts::{authenticated traces and spans,
authenticated logs investigation,authenticated metrics investigation}`. It uses
real server/Postgres and initial-HTML assertions but currently signs in through
API-key recovery. Task 03 must exercise a normal seeded human session, set and
verify actual theme, compare each named board region at 1440 and 390px, assert
geometry/computed fills and inspect keyboard/touch/empty/denied/error/partial/
long-content states. Keep generated screenshots/diffs outside the repository;
record text results and separate human acceptance. A green query/count test is
not visual acceptance.
