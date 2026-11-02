# Independent R1 findings validation

**Recommendation: FIX_REQUIRED.** Retain FIND-TASK-004-REV-02-1 through -4,
with the narrowed correction boundaries below; close -5 through -10; add
FIND-TASK-004-REV-02-11 for committed candidate images in a reports-only review
directory. No new finding was discovered by this validator. No demonstrated
need for a specification/public-contract revision follows from the exemplar
claim.

## Subject, scope and independence

Repository `/home/thorrester/Documents/GitHub/wyrd`; original base
`4a76331113864f9bddc43d3cc8df01080a696937`; prior candidate
`f6aeb83a77179ac4450110671df6f564adc1914a`; cumulative candidate
`fa613baee5e70f9458442f22d74ecebc22893ebc`. Reviewed the prior-candidate to
candidate diff and affected callers under the original TASK-004-REV-02,
approved spec revision 18, original verdict/ledger and R1 remediation with its
current execution evidence. No rebase correspondence is required. Source and
HEAD remained unchanged; only this report was written.

Read all three selected discovery reports: `task-review-remediation.md`,
`maintainer-review.md`, `followup-review.md`; `subject.md`,
`claims-comparison.md` and `verification.md`; original task and relevant
specification/architecture/repository/UI authority. CodeGraph preceded source
navigation. Independently inspected the changed conventional route/server
loaders, full RecordTable held/column/CSV owner and its app callers, full Chart
script and affected render/style consumers, recordedSeries and metricQueries,
TimePicker placement and full modal body/styles, generic Drawer and its
conventional trace composition, metric JSON/projection path, bounded server
predicate/stub fixes, dev lifecycle and canonical batch correlation changes.
Compared actual Metrics desktop, TimePicker mobile and TraceDrawer candidate
captures with reference roles/source, including the reference TraceDrawer PNG.
The discovery report covers the full six-board comparison; these independent
checks resolve the retained exact visual claims, not human visual acceptance.

Carry forward original source-backed accepted trace/root/detail, log
identity/correlation, query inspection and predicate grammar/tenant projection
behavior where unchanged. Reopen RecordTable held-state and Chart marker
consumers specifically because R1 changes their input/schema invariants, not
because unrelated old debt was rediscovered. This is the first unsuccessful
remediation review, so a two-round convergence assessment is not due.

## Proposal dispositions

| Proposal | Independent disposition | Stable ID / reason |
|---|---|---|
| R1-UI-01 | CONFIRMED | -1 remains open: clipped picker, displaced metric exact values, noncompact Logs composition, narrow/incomplete trace drawer and incorrect table-control region |
| MAINT-R1-01 | CONFIRMED | -2 remains open for affected held-projection coherence; conventional SSR and common-range portions are corrected |
| R1-METRIC-01 | CONFIRMED | -3 remains open: rounded double ordering can select wrong exact maximum |
| MAINT-R1-02 | CONFIRMED | -3 also covers fix-induced duplicate-time render keys at the retained-point consumer |
| R1-STRUCTURE-01 / focused exemplar report | REVISED | -4 remains open; precision defect proven, claim that a new server/public capability is required is unproven |
| ART-01 | CONFIRMED | new -11: skill already prohibited non-report artifacts in review directories; user's objection reinforces it |
| F5–F10 closure claims | CONFIRMED within scoped evidence | bounded source changes match selected prior corrections; no reopening without changed affected behavior |

No agreement is used as proof. No rejected or optional improvement enters the
ledger. Differences needed for current field names/brand, disabled unsupported
metric percentiles, and full-page screenshot placement of a viewport-fixed
dialog are distinguished below from actual defects.

## Retained validated ledger

UI-relative paths start at `crates/wyrd/wyrd-server/wyrd-ui/`. All retained
findings map to the original approved behavior; their IDs are preserved.

### FIND-TASK-004-REV-02-1 — Board composition and reachable controls remain incomplete

**Sources/status/classification:** R1-UI-01; CONFIRMED; MISSING/INCORRECT and
explicit styling VIOLATION. **Obligation:** REQ-007/015–017/020/021/039, the
original six-board target and R1 section 1's component composition,
responsive/keyboard/touch/overflow and token-only flat styling constraints.

**Source-to-user evidence:**

- `src/lib/TimePicker.svelte:215–227` anchors an absolute panel at the trigger's
  right edge while setting its width from the viewport. The wrapped trigger is
  not at that viewport edge. The actual 390px candidate light capture clips the
  panel's left labels and inputs, independently visible in this validation.
  Its `box-shadow: 0 12px 32px #00000040` is also expressly prohibited by the
  UI skill; the reference does not override current brand authority.
- `plot.ts::recordedSeries` captions differing dimensions with whole raw
  attribute JSON. `components/Chart.svelte:394–404,606–638` places that caption
  in a nowrap row heading with no bounding column width, before exact numeric
  cells. In the actual 1440px Metrics candidate, Latest/Max/Mean are beyond the
  visible chart region. The full identity lives only in `title`, which is not
  an equivalent keyboard/touch inspection. Stable identity is correctly
  preserved; it need not be the unbounded human caption.
- Observe `+layout.svelte:254–258` mounts the Logs timeline as a full Chart,
  including plot/summary/source regions, instead of the compact severity
  timeline. Existing compact chart presentation already serves the three trace
  overview tiles. This changes the approved composition and substantially
  pushes the log investigation list down; it is not an illustrated value
  difference.
- Observe `+page.svelte:35` passes the generic `Trace` title to the unchanged
  shared Drawer, whose width is `min(480px,100vw)`. Reference TraceDrawer is
  roughly 884px at 1440px and has useful trace identity/summary with Link,
  Expand and Close controls above the waterfall/detail. Actual candidate
  confirms 480px width, generic heading and unnecessary content truncation.
  Its data components are reused, but required trace-specific composition is
  still absent.
- RecordTable `:198–220` keeps Columns/CSV/Wrap in an extra private table bar,
  while the app's board listbar omits those controls. R1 requires the agreed
  board regions; existing component ownership does not authorize an exception
  to that explicit requirement. The correction must keep the controls'
  behavior single-owned while allowing approved placement.

**Consequence/materiality:** supported mobile users cannot reach/read parts of
the range picker, primary exact metric values are displaced even on desktop,
and the principal Logs/trace compositions still materially differ from the
approved investigative UI. These are normal generated content/viewport paths
and explicit approval obligations, not speculative pixel polish.

**Selected smallest correction:** finish app composition and bounded shared
presentational seams, preserving the existing components/data owners. Anchor or
clamp the reusable TimePicker panel within the actual narrow viewport and remove
the prohibited literal shadow. Keep typed metric equality keys unchanged;
bound concise distinguishable captions, keep Latest/Max/Mean in the visible
series-table region, and expose full recorded dimensions via keyboard/touch
accessible inspection. Compose the compact Logs severity timeline using the
existing chart owner, preserving its exact-query action and severity key rather
than globally removing generic Chart legends/footers. Extend the existing
Drawer's presentation seam for the approved trace width/header/link/expand
composition, retaining native modal focus/Escape and unrelated Drawer defaults.
Expose a shared presentational placement seam for the existing table controls
so the app places them in the listbar; do not copy their implementation into
page markup. No new component kit, transport, query or layout framework.

**Burden and proof:** these are bounded app/shared-presentation corrections;
renaming the full chart, clipping all labels, or moving controls by duplicate
page code cannot meet the same outcome safely. Use fixed reference-size and
390px light/dark browser checks with populated/empty/error and long-dimensional
metrics. Assert picker/control bounding boxes, visible exact summary columns,
full identity inspection, compact timeline, trace width/header/actions and
listbar placement. Compare the six affected boards; keep captures outside the
tracked repository and record textual evidence plus separate human acceptance.

**Narrowing:** do not fabricate p50/p90/p99 to match illustrative buttons;
original requirements explicitly forbid invented percentiles and require
unsupported operations unavailable. Current brand marks/fields/real names may
replace illustrative reference data. A full-page image showing a fixed dialog
partway down is not independently a live mobile defect; inspect its actual
viewport/focus behavior. These do not excuse the failures above.

### FIND-TASK-004-REV-02-2 — Held rows and the newly requested projection can disagree

**Source/status/classification:** MAINT-R1-01; CONFIRMED; INCORRECT affected-path
regression. **Obligation:** original truthful live hold/rows/counts/columns and
R1 coherent host-state/preserved RecordTable holds.

**Trace:** `RecordTable.svelte:48–69` snapshots only the loaded rows/counts in
`held`, but derives visible columns from the current `input`. R1's `toggle` at
`:152–165` calls host `oncolumns`; all Observe signal branches pass it and
navigate to a URL whose server list projection returns the new input/result.
SvelteKit reuses that layout/table instance. With Live held through open/focus,
`held` still contains the old schema; the new `input.columns` renders a newly
added heading against `row[columnIndex(oldRows,newColumn)]`, where index is -1.
The same mismatch feeds CSV at `:174–182`; current `executed` can likewise
refer to the newest read rather than the visible held snapshot.

**Consequence/materiality:** choosing a previously unread column in an ordinary
held investigation shows/exports an empty value despite a successful response
containing it. Columns and Live holds are explicitly supported together; this
is newly reachable through the remediation's host-column extension. The SSR,
common resolved-range and URL brush portions of F2 are corrected and must be
retained.

**Selected smallest correction:** make the existing RecordTable snapshot own
its matching input/schema, rows/counts and executed query. Distinguish normal
same-projection Live refreshes from an explicit column/projection change. When
the requested projection's result arrives, commit the new matching snapshot
coherently rather than showing new headings over old held rows; do not require
Resume as an undocumented repair. Preserve automatic-refresh held rows/counts,
new-row notice and resume for unchanged projections. CSV and Open in Query
must describe the visible snapshot. Do not remount the table on every refresh,
add page-level missing-cell guards or disable holding.

**Burden/proof:** one shared snapshot/transition correction, no new cache or
query owner. Extend the existing component test with held live rows, addition
of a previously unread nonempty column and a new result; assert heading/cells/
CSV/query remain coherent. Separately append same-schema data and verify the
existing hold/new-row/resume behavior. This focused case covers the changed
seam without a full lifecycle replay.

### FIND-TASK-004-REV-02-3 — Exact metric comparison and retained-point rendering are incomplete

**Sources/status/classification:** R1-METRIC-01, MAINT-R1-02; CONFIRMED;
INCORRECT and fix-induced regression. **Obligation:** original unsafe/mixed
scalar exact summaries and duplicate-time/no-implicit-sum metric semantics,
R1 F3's truthful chart/summary consumer invariant.

**Producer and consumer traces:**

1. `observe.ts::metricQueries:388–405` correctly retains integer text/DECIMAL,
   but selects exact Max using `first_value(t ORDER BY v DESC,i DESC NULLS
   LAST)` after `v=COALESCE(CAST(i AS DOUBLE),d)`. For integer
   `-9007199254740993` and double `-9007199254740992`, the latter is greater;
   both comparison keys round to the latter, then the non-null integer wins
   the tie. Returning its original text preserves the wrong selected value,
   not an exact maximum. The full function and Chart exact-summary consumer
   confirm this reachable supported scalar path; existing small mixed test
   does not exercise it.
2. `plot.ts:83–105::recordedSeries` now retains equal-time occurrences as
   required. `Chart.svelte:345` still keys markers by `point.x`; two retained
   points within a series and <=60 samples have duplicate keys. Installed
   Svelte's keyed-each runtime rejects those keys, as the maintainer's
   shape-to-runtime probe corroborates. This is not permission to restore the
   old generic duplicate summation; the changed producer invalidates a
   previous consumer uniqueness assumption. The tooltip at `:362` also finds
   only the first matching occurrence, so equal-time evidence must not be
   silently represented as a unique sample.

**Consequence/materiality:** legal retained samples can produce a wrong exact
maximum or a chart render failure. Both cases were explicitly within prior
unsafe/mixed/duplicate-time acceptance, not new product scope.

**Selected smallest correction:** at the existing shared metric SQL/projection
owner compare/select the winning original scalar using numeric authority that
preserves integer-vs-double ordering, including rounded-key ties; the chart's
lossy geometry key must not choose the exact summary. Preserve typed series,
explicit aggregate meaning and original display text. Prove both signs and
boundary cases in real SQL; do not globally cast all result integers or add a
client numerical parser. At the existing Chart retained-point render consumer,
use occurrence-safe marker identity (unkeyed/index occurrence is sufficient)
and expose retained equal-time samples truthfully in the tooltip/inspection.
Keep all samples without summing/dropping/inventing timestamps. Generic
 authored-query shaping/behavior stays unchanged.

**Burden/proof:** bounded shared planner and local Chart-consumer corrections
close the same prior evidence invariant. A legend label guard or SQL
 deduplication cannot repair either cause safely. Extend the existing real
Metrics journey with the unsafe negative pair (expected Max
`-9007199254740992`) and its positive adjacent counterpart, preserving original
text/latest semantics. Render Chart with two equal-time samples in one typed
series and assert no error and retained occurrences; cover equal-millisecond
instants if used by the fixture. Keep >20-series/no-fold and generic-chart
regressions green. This validator has not executed a replacement SQL query or
new browser test; the source-proven failure does not claim that proof.

### FIND-TASK-004-REV-02-4 — Unsafe exemplar integers still round; architectural escalation is not proven

**Sources/status/classification:** R1-STRUCTURE-01, focused exemplar report;
REVISED; INCORRECT/incomplete closure. **Obligation:** exact recorded exemplar
structures through existing SQL/results, original task precision paragraph,
R1 F4; no new wire encoding without the original material decision process.

**Trace/evidence:** canonical metrics `points.rs:80–96` stores exemplar
`int_value` as Int64 in a list/struct. UI `observe.ts:285–320` projects scalar
and histogram counts as text but has no exemplar projection. Queue
`json_result.rs:120–147` and its JSON factory preserve ordinary integers as
JSON numeric literals while specially decoding actual Variant extension
fields; TypeScript bounded collect at `src/index.ts:666–673` uses JSON.parse.
Thus stored `9007199254740993` can arrive in JavaScript as
`9007199254740992`. Storage corruption is not established. Current journey
seeds double exemplar 7.5, so readable attributes/structure assertions do not
cover the unsafe integer at all.

**Stop-claim validation:** the original task expressly permits server-side SQL
projection. Its material stop is a new public/wire/schema/authority decision,
not the word server-side. Bifrost architecture `Variant SQL:898–922` already
specifies `to_json(v)`; the pinned fork implements it. Casting a raw Variant
struct to text is not that decoding operation. Existing ordered list/string
aggregation and unnest/nested access supply reconstruction mechanisms; four
unsuccessful casts/flatten forms do not establish they cannot preserve a final
per-point ordered list. Metadata-erasing reconstruction must decode attributes
before losing their Variant extension semantics. No complete exact replacement
query was runtime-proven by discovery or this validator, and no exhaustive SQL
impossibility is claimed. A new global numeric-string encoding would change
public semantics; it is not the selected correction.

**Consequence/materiality:** a required displayed sample may silently change
value. Disclosure in an execution note does not close exact evidence, and the
available source does not justify asking the user to approve a new architecture
before investigating an already-authorized projection.

**Selected smallest correction:** retain the shared Metrics projection boundary
before JSON-number parsing. Convert the exemplar integer alternative to decimal
text, decode filtered attributes with existing Variant `to_json` before any
metadata-erasing reconstruction, and preserve each parent point's ordered list,
null/empty cases, remaining scalar/time/trace/span fields and readable attributes
through existing SQL/result capabilities. Reuse nested access and ordered
reconstruction in that same projection; do not group distinct/duplicate parent
points by a guessed identity. The precise private query mechanics remain
implementation-owned, but the chosen boundary and no-global-encoding constraint
are fixed. First obtain the narrow real-query proof, using the existing Metrics
integration owner, rather than another full dev replay. If an exact documented
planner/encoder limitation truly prevents this without a public/wire change,
return that evidence and actual proposed boundary for the original stop process;
current evidence has not established such a decision.

**Burden/proof:** one shared lossless projection extension, preserving the
existing wire and query owners. An ordinary cast, client formatting of rounded
numbers, hidden structure or new metric service is insufficient. Seed unsafe
int exemplar `9007199254740993`, readable nested attributes, multiple ordered
elements, empty/null lists and duplicated parent points; verify exact real SQL
→ bounded result → browser text. Retain proven scalar/explicit/exponential
count fixes. No new transport/encoding/dependency is prescribed.

### FIND-TASK-004-REV-02-11 — Candidate captures violate reports-only review storage

**Source/status/classification:** ART-01; CONFIRMED; VIOLATION. **Obligation:**
task-review skill “The review directory holds reports only”; user's explicit
current instruction reinforces the already applicable rule.

**Location/evidence:** prior-candidate→candidate adds 32 PNGs under
`changes/active/wyrd-ui-main-integration/review/task-004-rev-02-f6aeb83a7-1/candidate/`.
These are generated candidate captures, not the approved design PNGs under
`references/canvas/png/`. Git and the actual files establish the violation;
visual-evidence requirements did not authorize committing binaries there.

**Consequence/materiality:** an explicit artifact rule was violated, storing
generated proof in the tracked reports-only packet and contrary to the user's
instruction. This is independently source-confirmed, not a new human request
retroactively assigned a finding number.

**Selected smallest correction/burden:** remove those 32 newly committed
candidate PNGs in the next implementation; preserve approved reference assets,
prior reports and compact textual evidence. Future captures stay outside the
tracked repository/review tree, with text identifying inspected states/results
and an appropriate external/scratch review location. No new image archival,
ignore framework or rewritten history is required. Do not delete source during
this immutable audit.

**Focused proof:** scoped Git diff/status verifies removal of only the generated
candidate binaries, preservation of references/reports and useful text evidence.
No runtime test for artifact deletion.

## Prior closure and retained acceptance

| Finding/obligation | Validated closure and evidence | Result |
|---|---|---|
| -1 canvas/shared controls | Rail, reusable TimePicker and primary charts now exist, but above default/390px/long-label composition failures remain | OPEN |
| -2 conventional host/state | Observe `+layout.server.ts` runs shared loaders/nodeExecutor with one resolved context, page server loads exact details; direct components/URL brush/live invalidation implemented. New column/held snapshot seam remains | OPEN only for affected seam; retain SSR/range success |
| -3 metric series/exact values | Typed dimensions/no-fold and shared keyed selection implemented; exact ordering and retained duplicate rendering remain | OPEN |
| -4 recorded structures | Scalar and explicit/exponential counts have lossless text projection and recorded real-query proof; exemplar int64 still raw | OPEN only remaining exemplar precision |
| -5 setup inspectability | mise guarded setup failure reports state, skips app/batch and waits on existing healthy children; interruption/app-first cleanup retained. Existing negative lifecycle assertion inspected | CLOSED, source + recorded scoped test evidence |
| -6 actual batch membership | canonical TraceResponsePropagator exposes actual app trace; send retains IDs; missing checks workload trace/root/log and Run model/judge/verdict membership, unrelated-traffic negative test inspected | CLOSED, source + recorded scoped test evidence |
| -7 native Python stub | exact native method declaration/docs now present in authored Bifrost stub, public facades remain single-owned | CLOSED |
| -8 required rustdoc | changed fallible helper blocks now include workflow/rejections/# Errors | CLOSED |
| -9 scrubbed handler span | existing handler has named debug instrument with skip_all and no raw predicate logging | CLOSED |
| -10 configured permission owner | exact existing scoped table/payload permissions flow through state.auth.permission_check.check; coarse admission/refusal/audit ordering preserved | CLOSED |
| -11 reports-only artifact rule | 32 new candidate PNGs remain tracked | OPEN |
| Exact trace/log/query/predicate unaffected behavior | Prior source-backed acceptance retained, affected host/shared consumers inspected. No new contradictory source found outside retained seams | CARRY FORWARD; preserve during next fix |

## Verification limits and review routing

Root's fresh focused UI command passed two selected tests in two files; thirteen
unselected tests were skipped. Prior implementation browser/lifecycle/SDK results
remain recorded evidence, not fresh runs by this validator. The maintainer's
in-memory shape/key-validator probe corroborates the render-key path. This pass
performed independent source/caller validation and image inspection, not real
SQL reconstruction, Postgres/production-BFF/browser replay or full default batch.
Green tests do not close omitted cases. No new finding was discovered here, so
no separate source-validation follow-up is needed for a validator-origin issue.

**Final recommendation: FIX_REQUIRED**, not PASS or SPEC_REVISION_REQUIRED.
Write one bounded R2 remediation for -1/-2/-3/-4/-11, preserve -5 through -10
closure and already corrected portions, then review only closure/latest fix diff
and affected consumers. No broad aggregate or full 3,000-request replay belongs
to this remediation. Human per-board/component visual acceptance is still
pending and cannot be granted by any implementation/review agent.
