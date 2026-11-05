# Repository standards remediation review

Result: **FAIL**. The original listed rustdoc omissions are corrected, but `FIND-TASK-004-8` remains open for one new undocumented Rust item introduced by remediation.

Subject: `/home/thorrester/Documents/GitHub/wyrd`; incremental immutable range `ed82f63bf35ca124c0a2980c160f0d64535eebd1..0c2bfe70d5186dff4dc224c7e402b57c24f354d9`, with original `9692bfd06c6bdf2585e88c791884a90669a5dc26` cumulative context. HEAD matched the candidate at inspection. Only this report was written. This is standards coverage, not a task-acceptance or behavioral verdict.

## Scope and carried evidence

Read the current review scope, prior standards report, verdict, validated ledger and remediation task including appended evidence. The prior report's source-backed accepted rules remain carried for unchanged dev workload, sorting/paging, integer transport, schema fingerprint/WAL fixture, Bloom/fork/dependency and permission boundaries. None of their owning code, dependencies or contracts changes in this incremental range. The new context key, ordered-admission consumers, recovery fixtures, shared Chart rendering and changed tests were inspected directly; prior acceptance was not substituted for their review.

CodeGraph was consulted for Rust series/index owners and UI metric/plot owners. Inspected the actual incremental diff, complete changed Rust helper/method bodies, newly added tests and helpers, UI query/loader/plot changes and affected consumers. Nested `AGENTS.md` inventory has no narrower ancestor in redux, UI, changes or architecture. SQL's nested rules were read for the capability audit even though no SQL-owned source changes. SDK and docs-site nested rules do not acquire a new changed surface this round.

Authority keys:

- **R**: root `AGENTS.md`; `agent_docs/index.md`, `maintainer-style.md`; `architecture/README.md`; `changes/spec-driven-development.md`. These apply to every row.
- **RS**: `agent_docs/rust.md` (including private/test-item rustdoc), `errors.md`, root testing rules and `TESTING.md`.
- **B**: `architecture/bifrost-design.md`, canonical points/series identity and context, table/field ownership, ordinary ordered durability and recovery.
- **SEC**: `architecture/wyrd-security-posture.md`, tenant/data isolation, Postgres tenant binding, first-party browser sessions; SQL capability requirements from `crates/wyrd/wyrd-sql/AGENTS.md`.
- **CLIENT**: `architecture/wyrd-design.md`, Client model: Rust durable ownership and common typed wire contracts.
- **OPS**: `architecture/operations/reliability-and-recovery.md`, Scribe failure boundaries. Test-only recovery additions do not revise production readiness/recovery semantics.
- **UI**: `.agents/skills/wyrd-ui/SKILL.md`, SvelteKit architecture, data-performance and testing-verification references; UI `brand/DESIGN.md`, `palette.json`, `components.json` and existing shared controls.
- **GEN**: root generated-artifact rules and `agent_docs/agent-surfaces.md`.
- **PKT**: approved spec revision 27, existing task transfer and remediation context. The caller's deviation authorization permits the recorded reconciliation; it does not waive mandatory gates or alter architecture precedence.

Prefixes: `redux/` = `crates/vala/vala-bifrost-redux/`; `ui/` = `crates/wyrd/wyrd-server/wyrd-ui/`; `prior/` = `changes/active/wyrd-ui-main-integration/review/range-9692bfd06-ed82f63bf-20261104/`.

## Authority coverage of every incremental surface

| Changed surface | Applicable authorities and inspected rule coverage |
|---|---|
| `architecture/bifrost-design.md` | R, B, SEC, CLIENT; context columns, per-hour/context suppression, ordinary ordered admissions; one current authority rather than a parallel contract |
| `changes/active/wyrd-ui-main-integration/spec.md` | R, PKT, B; reconciled REQ-058/059 and history agree with the Bifrost owner; existing caller authorization retained |
| `prior/TASK-004-R1-metrics-correctness-and-index-coverage.md` | R, PKT; preserved remediation and appended verification, limits distinguished from acceptance claims |
| `prior/claim-comparison.md`, `domain-review-persistent-data.md`, `findings-validation.md`, `maintainer-review.md`, `scope.md`, `standards-review.md`, `system-review.md`, `task-review-behavior.md`, `task-review-invariants.md`, `verdict.md`, `verification.md` | R, PKT; historical reports newly committed in this range, retained as review context; no executable behavior or generated contract; range whitespace observation below |
| `redux/src/scribe/series_index.rs` | R, RS, B, SEC, OPS; owning retained set and admission method, tenant-scoped key, context-aware deterministic derived frame id, errors, changed tests and rustdoc |
| `redux/src/scribe/tests/scribe_persistence_path.rs` | R, RS, B, SEC, OPS; stable fixture principal, new accepted-context/recovery helpers and tests, actual retained WAL, ordinary replay, explicit panic documentation and fixture limits |
| `redux/src/tables/metrics/series.rs` | R, RS, B, SEC; context columns and keys, named column access, Arrow row encoding, typed error propagation, unchanged stored series-id encoding and all changed helper/test rustdoc |
| `redux/src/tables/mod.rs` | R, RS, B, GEN; nested schema assertion docs only; registered schema/fingerprint behavior unchanged |
| `ui/packages/ui/src/lib/components/Chart.svelte` | R, UI, CLIENT; optional host-qualified point text rendered as escaped text; existing keyed/generic selection and shared chart semantics retained |
| `ui/packages/ui/src/lib/observe.ts` | R, UI, B, SEC, CLIENT, GEN; bound server SQL, bounded reads, typed identity and missing/qualified values, shared loader error handling; no new public ChartInput/generated schema |
| `ui/packages/ui/src/lib/plot.ts` | R, UI, CLIENT; typed keyed presentation and optional point text; existing generic chart fold owner remains separate |
| `ui/src/lib/ui-library.integration.test.ts` | R, UI, B, CLIENT, SEC; real server/SDK OTLP and query execution, owning existing integration boundary, local fixtures, bounded readiness polling |
| `ui/src/lib/ui-library.test.ts` | R, UI; component regression for keyed lines, values and qualification; existing generic chart regression remains |
| `ui/src/lib/ui-pages.integration.test.ts` | R, UI, B, CLIENT, SEC; existing authenticated Metrics browser journey, real Service/workload fixtures, scoped metadata discovery and histogram control |
| `ui/src/lib/ui-pages.test.ts` | R, UI, B; pure query/loader/histogram shaping tests and prior exact-integer assertions updated with changed SQL |
| `ui/src/lib/ui-signals.integration.ts` | R, UI, SEC; test-only OTLP optional header argument; existing calls retain default headers; no production authorization bypass |
| `ui/src/routes/t/[tenantKey]/observe/+layout.svelte` | R, UI, CLIENT, SEC; existing segmented links, labeled calculation group, URL-based state, escaped values and existing focus/theme styling |

This accounts for all 27 changed files: two authority/spec files, twelve prior review files, four Rust files and nine UI files. No Python, SDK binding, manifest, migration, generated output, theme generator or tooling source changed incrementally.

## Applicable rule results

| Rule | Result | Source evidence |
|---|---|---|
| Immutable subject and reports-only review | PASS | HEAD `0c2bfe70d5186dff4dc224c7e402b57c24f354d9`; status contains only this round's untracked review directory; no source edits or Git mutation |
| Authority precedence / no duplicate contract | PASS | Bifrost series paragraph and spec REQ-058/059 both identify hour plus accepted context; architecture owns the column definition and excludes request id/admission instant |
| Wyrd vocabulary and client/server ownership | PASS | Series derivation remains in redux; UI submits analytical SQL through the existing executor/client; no durable browser/Node state, new service or alternate wire contract |
| Layer/import/dependency boundaries | PASS | Context encoder uses existing Arrow/SHA dependencies; no new dependency/feature, server import into client tier, Python feature, IO/async in wyrd-spec or generated SDK edit |
| Actual SQL capability signatures and fields | PASS | Changed production owners introduce no SQL connection/pool field or parameter. `ScribeImpl` retains `fence_operator: Option<vala_sql::OperatorPool>` (`scribe/mod.rs:276`) and `control_postgres: Option<Arc<vala_sql::ValaPostgres>>` (`:335`); no raw `PgPool`, `PgConnection` or transaction added. UI SQL uses Bifrost's query surface, not Postgres pool access |
| Tenancy / authority-derived context | PASS for standards | `SeriesSeen` retains `(DataTenantId, SeriesKey)` keying; series rows derive from stamped prepared points; no caller-selected tenant or request authority added. Browser session/executor ownership is unchanged. Context tests exercise one stable writer principal rather than changing production stamping |
| Context and copied-column authority | PASS for standards | `CONTEXT_COLUMNS` covers promoted Service/environment/SDK/subject plus Card/run/workload/principals; request/admission fields excluded. `series_rows` obtains copied/context arrays by column name; seen and in-batch suppression use the same triple key; stored `series_id` algorithm is unchanged |
| Error and validation rules | PASS | `series_rows` propagates `BifrostError::SchemaParse`; admission maps through existing `ScribeError::ContractViolation`. No public code, handwritten error mapper, validation bypass or new unwrap. New failure in Arrow context encoding is included in `# Errors` |
| Struct-centered Rust / sync / lifecycle ownership | PASS | Existing `SeriesSeen` retains bounded state and `ScribeImpl` retains ordered durable admission. Context encoding is synchronous pure transformation. New free async helpers are tests whose state/lifetime is caller-owned; no new runtime/production lane or lifecycle owner |
| Portable platforms / coordination time | PASS | No OS gating, forbidden host mechanism, production deadline source or Postgres timestamp change |
| Changed-item rustdoc, all new helpers/tests | FAIL; F8 incomplete | Detailed item inventory below; assertion helpers/tests document panic conditions and the fallible production helper documents errors, but the new nested CONTEXT constant has no rustdoc |
| Tests at owning tiers / no weakened gate | PASS for source and placement | New embedded persistence cases remain under existing `src/scribe/tests` module; new real SQL/browser regressions extend existing integration/journey files; no skip/ignore, weakened assertion, timeout increase, new harness or credentials requirement added |
| Bounded typed queries and UI data | PASS for standards | Existing `Binds`, MAX_SERIES and plot ceilings retained; new histogram ranking executes on the server; labels read only plotted ids through existing executor. Values remain parameters and grouping columns remain existing schema-owned expressions |
| Svelte state / semantic keyboard controls / tokens | PASS by source | Calculation links reuse `href`/`closed` URL state, visible labels, role/group aria label and existing segmented styles. Point text renders through Svelte interpolation. No new color, CSS token, geometry, subscription, network client or browser secret |
| Generated artifacts and wire/schema parity | PASS / regeneration N/A | No generated input or artifact changes in incremental range; optional keyed point text and KeyedChartProps are internal presentation data, not a ChartInput/wire-schema change. Prior generated acceptance carried forward |
| Required verification evidence | PASS for scoped supplied evidence; limits below | Appended exact nextest, UI unit/integration/journey, check/build and fmt/clippy commands are present; no no-selection/skipped result is claimed. Root independently reports 22 UI unit tests and all three exact recovery/context tests passing in this round |

## F8 closure and complete changed Rust-item documentation inventory

The five series test helpers `point` (`series.rs:697`), `projected` (`:746`), `id` (`:759`), `otlp_id` (`:773`), `replaced` (`:783`) now have meaningful intent plus `# Panics`. The five identity tests at `:797`, `:810`, `:823`, `:840` and `:861` now document the asserted failure conditions. Nested `assert_untagged` (`tables/mod.rs:2511`) has its own purpose, recursive invariant and `# Panics`. The surrounding schema test retains its own existing panic documentation. No behavior was removed to clear this documentation gate.

Additional incremental items were checked rather than stopping at the prior list:

- `SERIES_FIELDS`, `CONTEXT_COLUMNS`, `SeriesKey`, `SeriesSeen`/its changed field docs, `series_rows` and `admit_series_first` explain the changed context/granularity, workflow and suppression/durability invariant. `series_rows` has accurate new encoding errors. `admit_series_first` documents partial durable progress, ordered dispatch, failure and retry together with its module/owning type.
- `series_batch_id` (`series_index.rs:132`) documents deterministic context-aware retry identity. Its `expect("sixteen digest bytes")` converts the statically 16-byte prefix of `[u8; 32]` into `[u8; 16]`; no input can cause that conversion to fail. No fictitious reachable panic condition is required. The modified seen-set and batch-id tests retain updated, accurate `# Panics` sections.
- Changed `points_frame` (`scribe_persistence_path.rs:1104`) explains its stable principal-per-tenant fixture semantics and is infallible. New `with_runs` (`:1277`), `series_contexts` (`:1294`), accepted-context test (`:1370`), `crash_once_durable` (`:1539`), `restarted_scribe` (`:1576`) and the two interruption tests (`:1614`, `:1673`) each have workflow/fixture docs and `# Panics`. Fallible work is asserted inside tests, not exposed as an undocumented Result-returning helper. The new nested CONTEXT constant at `:1301` is a separate Rust item with no rustdoc; helper-level documentation does not cover it.

New function-local test imports and fully qualified signature spellings remain the same guide-shape discrepancy already calibrated non-blocking in the prior report. No unrelated cleanup is requested.

### REPO-R1-1 — New nested constant omits mandatory item rustdoc

- Stable prior finding: `FIND-TASK-004-8`; classification **VIOLATION**, same documentation-gate cause rather than a new finding ID.
- Obligation: root AGENTS requires rustdoc on every new/changed Rust item and explicitly blocks merge for missing rustdoc. `agent_docs/rust.md` Rustdoc applies regardless of visibility and includes private/test items. A nested constant is a Rust item; documentation on its enclosing helper is not documentation on that item, just as for the prior nested assertion helper.
- Exact location: `redux/src/scribe/tests/scribe_persistence_path.rs:1301`, `const CONTEXT: [&str; 4]`, newly added in this range inside `series_contexts`.
- Evidence/consequence: the four context names are introduced with no `///` documentation. The candidate therefore leaves the explicit changed-item documentation gate unsatisfied, despite closing all originally enumerated omissions. This finding claims no production panic or behavioral defect.
- Smallest correction: document why this fixture reads these four representative accepted-context fields. No new helper, test, parser, runtime guard or API is needed; source inspection plus already-required formatting/lint evidence closes it. Batch this bounded correction with any retained behavioral remediation; do not create an isolated style-only round.
- Correction burden/materiality: one local rustdoc comment; retained solely because the repository declares the item-documentation gate mandatory, not because of speculative operational impact.

## Verification and limitations

This reviewer independently ran only read-only navigation/Git checks. No Cargo, frontend build, database or browser suite was repeated; concurrent checks are owned by the orchestrator. Appended implementation evidence reports 17 focused Rust tests, exact retained-WAL/context cases, 22 UI unit tests, metric-math integration, filtered authenticated Metrics journey, check/build and fmt/clippy passing. Root supplied independently repeated 22 UI unit passes and three exact recovery/context passes during this review. These are distinguished from this reviewer's own execution.

`git diff --check ed82f63bf 0c2bfe70d` independently exits 2 for `prior/system-review.md:78: new blank line at EOF`. The recorded plain `git diff --check` checks an empty working diff and does not establish whitespace cleanliness of the reviewed range. This is a minor preserved-report formatting issue, not a material supported-use defect or an explicitly mandatory repository merge gate; it creates no remediation finding and prior reports were not edited.

Recovery coverage explicitly uses retained WAL across one restart with the existing embedded in-memory object store; it does not establish a second restart after replay retirement. Persistence-behavior adequacy belongs to the selected domain reviewer. No new recovery production path is introduced here. Existing measured latency miss and absent human visual acceptance remain disclosed; this standards PASS does not assert either is resolved. Theme/geometry source was inspected; rendered light/dark manual acceptance was not performed. No generated input changed, so full codegen is not newly required by this incremental scope.

Proposed mandatory-gate ledger: **REPO-R1-1**, continuing `FIND-TASK-004-8`. The original listed helper/test omissions are closed; the newly added constant prevents complete F8 closure.
