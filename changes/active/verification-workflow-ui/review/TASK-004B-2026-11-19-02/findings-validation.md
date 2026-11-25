# TASK-004B-R2 independent remediation validation

Reviewer: fresh `r2_ponytail_rev`. Immutable candidate:
`7cf5bc4abb1462e5783df67ff6d0a2a5f2a06b14`. Prior reviewed HEAD:
`79a4ef66190e572c47aa93351e7362329f7bc73f`, whose reviewed uncommitted R1
tests were committed as `41f2419c8` before R2. The reviewer inspected
`git diff 41f2419c8 7cf5bc4ab` with the cumulative prior range available.
HEAD remained the candidate and implementation files have no relevant dirty
changes. Unrelated skill/design edits were preserved.

Authority: approved SPEC-verification-workflow-ui revision 6 and supporting UI
revision 28; original TASK-004B; prior verdict/findings; TASK-004B-R2 and R1's
approved browser acceptance; root/SDK AGENTS; spec-driven development;
maintainer style; TESTING and applicable Rust/UI guidance. The current caller
confirms the maintainer's per-host-request validation ruling. That explicit
ruling governs FIND-7 rather than its superseded per-refresh expectation.

CodeGraph preceded source tracing. The reviewer checked changed producers,
consumers, assertions and recorded executions, then independently validated
the cancellation specialist's new documentation proposal against source and
mandatory authority. SDK cancellation boundary evidence is in
[task-review-cancellation.md](task-review-cancellation.md).
No implementation changes, tests, builds, package commands, screenshot
generation or benchmarks were performed by this reviewer.

## Decisions and reconciled coverage

PASS below applies to the stated behavior within the supplied execution
evidence; it is not a claim of fresh runtime execution by review.

| Existing obligation / finding | Source and actual proof | Assessment |
|---|---|---|
| FIND-TASK-004B-1 / REQ-014, original scenario 2 | No R2 change to accepted per-stat matched-population projection, complete original/new series and rendered sparse-trend proof. Prior source-backed closure remains applicable. | PASS carried forward |
| FIND-TASK-004B-2 / original scenario 5 | Population-specific support remains separate from list-only choices. Existing browser assertions still pin no supporting refresh on paging/sort/columns and retain support while list updates. | PASS carried forward |
| FIND-TASK-004B-3 / original scenario 5 | Existing automatic refresh awaits list plus support. New real-browser assertion at `ui-pages.integration.test.ts:941` confirms a same-population Live reread has no busy marker; prior held-support success/failure/pause/departure assertions remain. | PASS |
| FIND-TASK-004B-4 / R2 startup and Card rows | Node executor forwards signal to SDK stream/list; NativeAbort drops pending client futures. Browser gate now holds SQL/Card headers; real navigation observes abandoned upstream requests; SDK silent-server tests observe pending socket closure. Specialist traced both phases. | PASS for client-owned cancellation; no server-compute-termination claim |
| FIND-TASK-004B-5 / populated analytical proof | No R2 change to populated independent facet cutoff/null/native and metered/unmetered/multi-currency comparison owners. Prior closure remains applicable. | PASS carried forward |
| FIND-TASK-004B-6 / REQ-018, AC-012 | No R2 change to explained-query denial/tenant/deadline/cancellation/resource/output assertions. Prior closure remains applicable. | PASS carried forward |
| FIND-TASK-004B-7 / explicit maintainer ruling | R2 evidence records one validation per host request. New `ui-pages.test.ts:992` characterization exercises list and support, each validating first once, and invalid/denied cases issuing no dependent reads. Standalone and list-only proof is retained. | PASS under ruling; original finding closed by approved boundary clarification |
| R1 Revision 1 / explicit loading and stale totals | `Streamed.pending` distinguishes replacement population/remembered Back from same-population Live. Updating wraps facets/charts; Observe initial overview/timeline render loading/errors; list-bar summaries are busy-marked; Verify suppresses old-total counting/paging. Browser assertions exercise pending markers, usable list, cleared markers and no false counts on failure. | PASS for functional behavior |
| R1 Revision 1 / visual acceptance | Captures now include loading/updating/settled/error/Back in light and dark. Overlay/hint overlap, boxed loading and existing full-page dark background difference explicitly await maintainer rulings. | UNRESOLVED; reviewer cannot provide approval |
| Mandatory changed-Rust-item rustdoc | Two newly introduced native items lack documentation; specialist C-01 independently checked below. | FAIL: FIND-TASK-004B-8 |

The evidence's tested production source is `b5f8a322a`; candidate `7cf5bc4ab`
adds its task evidence, with no subsequent production change. The implementer
records six focused real-browser journeys, 25 focused UI unit tests,
`ui:check` with zero errors and 95 passes, SDK unit/check lanes, and Rust
format/clippy success. The reviewer inspected assertions and candidate
applicability rather than rerunning those commands. Passing compiler/lint
checks do not establish the explicit rustdoc obligation they do not enforce.

## Retained finding FIND-TASK-004B-8 — VIOLATION: new native items lack required rustdoc

**Proposal decision:** cancellation specialist C-01 **CONFIRMED** by this fresh
reviewer, independently of its discovery. This is the only newly retained
blocking finding. No other new implementation defect was established.

**Authority:** root `AGENTS.md:152–154` requires rustdoc on every new/changed
Rust item and explicitly says missing rustdoc blocks merge.
`agent_docs/rust.md:235–241` applies regardless of visibility, explicitly
including fields. It supplies no trait-implementation exemption. This finding
enforces that mandatory gate rather than a preferred commenting style.

**Exact locations:** `sdks/wyrd-sdk-ts/native/src/lib.rs:324`, the new
`NativeAbort.aborted` watch sender field, and `:346`, the new
`impl Default for NativeAbort::default` method. Both are introduced in R2's
native cancellation diff; neither has a rustdoc comment. The type and adjacent
constructor/methods have their own documentation, which does not document
these individual items under the repository's explicit rule.

**Conditions and trigger:** accept the new native cancellation bridge in the
reviewed candidate. **Expected:** all newly introduced Rust items meet the
repository's documentation gate. **Actual:** the persistent cancellation state
and default-construction method have no item rustdoc despite that gate.

**Evidence and consequence:** direct candidate diff/source establishes the two
omissions. Source ownership is `NativeAbort` and its Default implementation;
the affected SDK clients remain functionally covered. The merge requirement
is unmet even though clippy and functional tests pass. No runtime failure or
new cancellation defect is alleged.

**Smallest testable correction:** add two accurate, concise rustdoc comments:
describe the watch sender's shared cancellation state/monotonic abort role,
and that Default creates a fresh un-aborted signal through the constructor.
Keep signatures and logic unchanged. Source inspection plus the narrow
format/diff checks is adequate for this documentation-only correction; no
new runtime test, native abstraction or broad suite is required. If the actual
correction changes generated public declarations, use their existing generator
and required parity check rather than hand-editing generated files.

## Visual evidence and pending decisions

The reviewer visually inspected these actual captures under the preceding
review's `captures/` directory:

- `lifecycle-observe-updating-light.png`;
- `lifecycle-observe-loading-light.png`;
- `lifecycle-verify-updating-dark.png`;
- `lifecycle-verify-loading-light.png`;
- `lifecycle-observe-support-error-dark.png`;
- `lifecycle-observe-back-dark.png`.

They corroborate that retained support is visibly distinguished, the first
Observe overview now says it is loading, and support errors preserve the list
without substituting zero counts. The Updating overlay does cover part of the
facet hint; boxed loading uses the existing EmptyState vocabulary. The reported
full-page dark background difference remains visible. These are the exact
differences already submitted for the maintainer's decision, not new findings
or authority for an unsolicited redesign. R1 Revision 1 and R2 require those
rulings before visual acceptance, so evidence preparation cannot be labeled
approval. No particular alternative design is prescribed here.

## Scope, waits, process and convergence

Primary coverage plus the SDK cancellation specialist covers the materially
changed boundaries. No additional repo/system/domain fanout is necessary for
unchanged accepted analytical and server behavior. New NativeAbort cancellation
stays client-owned; no server hot-path production code, PostgreSQL/Scribe/
object-store/Vault wait site or server API is added by this remediation. The
earlier hosted benchmark override remains a disclosed performance limit and
does not become a general waiver for future hot-path changes.

TDD history is separate from behavioral acceptance. Preserve original-task
nonbehavioral RED/test-order violations, R1's malformed-JSON test-after-fix,
and R2's SDK unit-test-after-native-signal violation. Browser RED and the later
SDK mutation strengthen proof but do not reconstruct missing test-first order.
No historical RED or production churn is requested.

After R1 and this R2 review, the original cancellation and loading functional
corrections have converged; FIND-7 follows the explicit maintainer ruling.
The remaining mandatory code correction is the bounded R2-introduced rustdoc
omission, plus the already-required visual decision. These are not grounds to
restart SQL consolidation, reopen prior closed findings or expand product
scope. This is not an automatic PASS after two rounds.

Recommendation: **FIX_REQUIRED** for FIND-TASK-004B-8, with visual acceptance
separately **UNRESOLVED** pending the maintainer's rulings. Once the two comments
and required visual decisions are supplied, a focused remediation review can
carry forward the unaffected functional evidence. Do not demand a third full
implementation or test matrix for those remaining closure items.
