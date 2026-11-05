# Repository standards review

Result: **FAIL** — one grouped mandatory rustdoc finding, `REPO-1`.

Subject: `/home/thorrester/Documents/GitHub/wyrd`, `9692bfd06...ed82f63bf35ca124c0a2980c160f0d64535eebd1`. First cumulative review; no carried-forward coverage. HEAD matched the candidate at inspection. Only this report was written. This review checks repository standards, not task acceptance or optional cleanup.

## Authorities and complete surface coverage

CodeGraph was consulted before source navigation (`mise exec -- codegraph explore`). The shared scope map was expanded through the actual diff, complete new series modules, surrounding ingestion/validation and auth consumers, and the changed test bodies. `git diff --name-only` identifies 61 changed files.

Authority keys used below:

- **R**: root `AGENTS.md`; `agent_docs/index.md`, `agent_docs/maintainer-style.md`; `architecture/README.md` authority precedence; `changes/spec-driven-development.md` for review immutability. These apply to every row.
- **RS**: `agent_docs/rust.md`, especially struct ownership, types/imports, async, diagnostics, and Rustdoc; `agent_docs/errors.md` where errors are propagated; root testing rules and `TESTING.md` for tests.
- **B**: `architecture/bifrost-design.md`, Table and row identity / Table versions and publication, Storage format and Variant, Declared types and the registered schema, Durability and visibility, Scribe admission/recovery, Oracle immutable planning/source authority and resource/failure invariants. The changed architecture sections are governing authority, not implementation evidence that can waive contributor rules.
- **SEC**: `architecture/wyrd-security-posture.md`, First-party browser sessions, Tenant and data isolation / Postgres tenant binding; relevant Gate permission and payload policy from Bifrost Public surface. `crates/wyrd/wyrd-sql/AGENTS.md` was also read to audit capability types; no file inside its directory changes.
- **OPS**: `architecture/operations/README.md` readiness model; `architecture/operations/reliability-and-recovery.md`, Scribe and Oracle failure boundaries.
- **UI**: `.agents/skills/wyrd-ui/SKILL.md`; its SvelteKit architecture and testing/verification references; `brand/DESIGN.md`, `palette.json`, `components.json`, existing application/component patterns. Includes server-side fetching, token/mode parity, semantic/keyboard controls, bounded data, and generated projections.
- **GEN**: `agent_docs/agent-surfaces.md`, generated-artifact rules; owning generator and `mise.toml` codegen tasks.
- **TS**: `sdks/wyrd-sdk-ts/AGENTS.md` and Rust-owned wire-contract projection.
- **DOC**: `docs/AGENTS.md`, checked for applicability: its page-writing rules apply to `src/content/docs/`; this range changes only generated styles, so page shape/examples rules do not apply.
- **PKT**: active specification revisions 20–27 and task packet as scope context only. TASK-022 is superseded by revision 18; TASK-004 transfers closure to `TASK-004-revision/TASK-02-live-observe-workflow.md`. Historical completion text is not treated as current proof or a new defect. The user authorizes possible spec deviations; this does not waive mandatory contributor gates.

Prefixes: `redux/` = `crates/vala/vala-bifrost-redux/`; `ui/` = `crates/wyrd/wyrd-server/wyrd-ui/`; `journey/` = `crates/wyrd/wyrd-testing/tests/bifrost/oracle/`.

| # | Changed file | Applicable authorities / inspection |
|---|---|---|
| 1 | `.agents/skills/wyrd-ui/references/wyrd-theme.css` | R, UI, GEN; generated light/dark projection |
| 2 | `Cargo.lock` | R, RS, B; pinned dependency closure |
| 3 | `Cargo.toml` | R, RS, B; workspace patches and no added features |
| 4 | `architecture/bifrost-design.md` | R, B, SEC, OPS; schema, series ordering, metadata and pruning authority |
| 5 | `changes/active/wyrd-ui-main-integration/spec.md` | R, PKT; scope/revision applicability, no new approval gate inferred |
| 6 | `changes/active/wyrd-ui-main-integration/tasks/TASK-004-revision/TASK-04-bifrost-bloom-pruning.md` | R, PKT; changed task/evidence, immutable review context |
| 7 | `redux/fixtures/tagged-spans-wal/00000000000000000000000000000000/1/shard-00/0.wal` | R, RS, B, OPS; binary fixture consumed by replay test, not a generated contract |
| 8 | `redux/src/catalog/bifrost_catalog.rs` | R, RS, B, SEC; comment correction, existing describe/capability owner |
| 9 | `redux/src/gate/mod.rs` | R, RS, B, SEC; reserved series write refusal and denied-decision test |
| 10 | `redux/src/oracle/codec.rs` | R, RS, B, SEC; pinned revision matches manifest |
| 11 | `redux/src/oracle/exec.rs` | R, RS, B; new pruning test and fixture helpers |
| 12 | `redux/src/schema/fingerprint.rs` | R, RS, B; nested metadata fingerprint documentation |
| 13 | `redux/src/scribe/execution_lanes.rs` | R, RS, B, SEC; derived-column replacement and raw-Arrow test |
| 14 | `redux/src/scribe/ingress.rs` | R, RS, B, SEC, OPS; series admission before dispatch |
| 15 | `redux/src/scribe/mod.rs` | R, RS, B, SEC, OPS; retained state owner and initialization |
| 16 | `redux/src/scribe/replay.rs` | R, RS, B, OPS; historical WAL recovery test |
| 17 | `redux/src/scribe/series_index.rs` | R, RS, B, SEC, OPS; complete new owner/fields/methods/tests |
| 18 | `redux/src/scribe/tests/scribe_persistence_path.rs` | R, RS, B, OPS; changed helper plus new embedded persistence tests/helpers |
| 19 | `redux/src/tables/fields.rs` | R, RS, B, SEC; class stays ledger-owned, bare physical field projection |
| 20 | `redux/src/tables/logs/mod.rs` | R, RS, B; schema-identity tests retain Variant validation |
| 21 | `redux/src/tables/managed_columns.rs` | R, RS, B, SEC; reuse canonical field projection |
| 22 | `redux/src/tables/metrics/mod.rs` | R, RS, B; owning module export |
| 23 | `redux/src/tables/metrics/points.rs` | R, RS, B; derived identity and physical layout |
| 24 | `redux/src/tables/metrics/projection.rs` | R, RS, B, SEC; shared derivation after canonical validation |
| 25 | `redux/src/tables/metrics/series.rs` | R, RS, B, SEC; complete encoding/table/row derivation and test helpers; REPO-1 |
| 26 | `redux/src/tables/mod.rs` | R, RS, B, SEC; registry, derived-column metadata, fingerprint and tests; REPO-1 |
| 27 | `redux/src/tables/signal.rs` | R, RS, B, SEC; metadata docs, narrow visibility of stored Variant reader |
| 28 | `redux/src/tables/traces/mod.rs` | R, RS, B; metadata assertions changed, value/identity checks retained |
| 29 | `ui/brand/gen-theme.mjs` | R, UI, GEN; native control scheme projection |
| 30 | `ui/brand/gen-theme.test.mjs` | R, UI, GEN; both-mode generator tests |
| 31 | `ui/brand/theme.css` | R, UI, GEN; generated parity |
| 32 | `ui/packages/ui/src/lib/catalog.json` | R, UI, GEN; generated from schemas, checked parity |
| 33 | `ui/packages/ui/src/lib/components/Chart.svelte` | R, UI, B; range axis, bucket width, tooltip positioning |
| 34 | `ui/packages/ui/src/lib/components/Drawer.svelte` | R, UI; platform dialog/backdrop with Escape equivalent |
| 35 | `ui/packages/ui/src/lib/components/RecordDetail.svelte` | R, UI; escaped text/raw JSON and constrained scrolling |
| 36 | `ui/packages/ui/src/lib/components/RecordTable.svelte` | R, UI, B; server sort/page intents, local complete-query presentation, semantic controls |
| 37 | `ui/packages/ui/src/lib/loaders.ts` | R, UI; reuse bucket owner |
| 38 | `ui/packages/ui/src/lib/observe.ts` | R, UI, B, SEC; bounded query plans, typed binds, server execution/error propagation |
| 39 | `ui/packages/ui/src/lib/schemas.ts` | R, UI, GEN; allowlisted sort/filter columns and cursor/action schema |
| 40 | `ui/packages/ui/src/lib/theme.css` | R, UI, GEN; generated parity |
| 41 | `ui/src/app.css` | R, UI; shared responsive page padding |
| 42 | `ui/src/lib/observe-host.ts` | R, UI; URL state, intents, bounded grouping and pagination transitions |
| 43 | `ui/src/lib/ui-fixtures.ts` | R, UI; read-shape fixture updated with severity number |
| 44 | `ui/src/lib/ui-library.test.ts` | R, UI; component/intent tests, updated server pagination assertions |
| 45 | `ui/src/lib/ui-pages.integration.test.ts` | R, UI, B, SEC; real authenticated UI journey assertions |
| 46 | `ui/src/lib/ui-pages.test.ts` | R, UI; planning, invalid-input, paging and histogram pure tests |
| 47 | `ui/src/routes/t/[tenantKey]/+page.svelte` | R, UI; shared page wrapper |
| 48 | `ui/src/routes/t/[tenantKey]/observe/+layout.svelte` | R, UI, SEC; search cancellation/debounce, chart and keyboard form controls |
| 49 | `ui/src/routes/t/[tenantKey]/observe/+page.server.ts` | R, UI, SEC; existing session executor/predicate normalization |
| 50 | `ui/src/routes/t/[tenantKey]/observe/+page.svelte` | R, UI; pointer and keyboard splitter, record rendering |
| 51 | `ui/src/routes/t/[tenantKey]/observe/metrics/+server.ts` | R, UI, SEC; RequestHandler session, range/search/rail validation, canonical problems |
| 52 | `ui/src/routes/t/[tenantKey]/query/+page.svelte` | R, UI; shared page wrapper |
| 53 | `ui/src/routes/t/[tenantKey]/settings/+page.svelte` | R, UI, SEC; shared page wrapper, auth behavior unchanged |
| 54 | `ui/src/routes/t/[tenantKey]/settings/account/+page.svelte` | R, UI, SEC; shared page wrapper, secret behavior unchanged |
| 55 | `journey/distributed.rs` | R, RS, B, SEC; real hot/published/rewritten Bloom assertions and probe field |
| 56 | `journey/published.rs` | R, RS, B, OPS; bounded wait for actual Forge settlement |
| 57 | `docs/src/styles/wyrd-tokens.css` | R, UI, GEN, DOC; generated parity; no prose page changed |
| 58 | `scripts/dev/example-app.sh` | R, RS platform policy, PKT; pinned external app checkout, dirty-tree refusal, production/test wheel separation |
| 59 | `scripts/dev/wyrd-example-py.patch` (deleted) | R, PKT; deleted local external-app overlay; no surviving Python source in this range |
| 60 | `sdks/wyrd-sdk-ts/wyrd/src/card-types.ts` | R, TS, GEN; generated comment matches owning `CheckAgent` Rust source |
| 61 | `tests/fixtures/mockserver/expectations.json` | R, PKT; local delayed/error provider fixtures, no live credentials |

Nested-rule inventory: only `docs/AGENTS.md` and `sdks/wyrd-sdk-ts/AGENTS.md` are ancestors of changed files; both read. Other nested files are Python SDK and SQL rules; no Python SDK/SQL-owned source changes. SQL rules were nevertheless applied to the production capability audit. No narrower UI or redux AGENTS file exists in the repository inventory.

## Rule results

| Applicable rule | Result | Source / evidence |
|---|---|---|
| Immutable source and reports-only review | PASS | HEAD equals candidate; status contains the new review directory only; no source edits or Git mutations |
| Wyrd vocabulary, no added compatibility surface | PASS | Changed routes/types retain Wyrd names; series is a Bifrost table, not a Card kind; old patch removed rather than aliased |
| Durable server ownership and thin clients | PASS for standards scope | Identity/series admission lives in redux; Node routes use `nodeExecutor(session.client())`; SQL executes through the existing Bifrost client; no new durable browser/Node store |
| Tier/dependency boundaries and pinned revision | PASS by source inspection | Only existing workspace DataFusion patches/lock revisions change, to `5f4b1713af4f0394c7c564b6bc6de42e92640186`; codec revision matches; no new feature, SDK heavy dependency or Python feature |
| Production SQL capability types | PASS for changed behavior | `ScribeImpl` retains `Option<vala_sql::OperatorPool>` and `Option<Arc<vala_sql::ValaPostgres>>`; new `SeriesSeen`/`SeriesFrame`/methods introduce no SQL pool or connection; ingress uses the existing Postgres boundary owner. Catalog change is comments only; no new raw `PgPool`, `PgConnection`, or transaction signature. Existing unrelated import spelling is not reported as a range defect |
| Tenant authority and reserved writes | PASS for standards scope | Tenant copied from admitted points; seen set keyed by `DataTenantId`; Gate adds exact reserved `metrics.series` table; browser endpoint uses the selected session installed by `src/hooks.server.ts`, not caller-supplied tenant authority |
| Managed fields, field-name mapping, and validation boundary | PASS for standards scope | Derived column replacement uses `schema.index_of`/`column_by_name`; canonical validator still runs before replacement; no validation bypass introduced; series copies named fields; field IDs remain registered-table-owned |
| Error conventions | PASS | Existing typed Scribe/Bifrost errors reused; UI problem shaping reuses `inputProblem`/`failed`; no invented public code or handwritten Rust problem mapper |
| Struct-centered Rust, bounded state and async | PASS | `SeriesSeen` owns retained mutex/set and bound; `ScribeImpl` owns admission workflow; encoding helpers are deterministic; new async admission awaits existing durable ingress; no runtime, single-implementation trait or new CPU lane |
| Changed-item rustdoc, including test helpers | **FAIL** | REPO-1 below; all changed Rust surfaces were inspected, including inline new helpers and assertions |
| Rust import/signature guide | Minor non-blocking discrepancy | New raw-admission test in `execution_lanes.rs:2959` adds function-local imports; new Oracle fixture helpers and `points_frame` retain fully-qualified type spellings. This is guide-shape deviation without a supported-use consequence or separately declared mandatory gate; no cleanup task proposed |
| Portable supported platforms | PASS by changed-source inspection | No OS-gated implementation or forbidden facility added; shell workflow continues existing tooling; no platform-specific Rust branch |
| Tests at owning tiers; no new skip/ignore | PASS for placement; execution limits below | Pure/table tests inline; embedded WAL tests need no Postgres; existing pg modules stay isolated; server journey changes remain in existing journey binary/UI journey. No added Rust allow/ignore, deleted gate, or boundary-glob weakening |
| Typed inputs, bounded requests and query values | PASS for standards scope | List schema checks sort/cursor shape; metrics route validates absolute range, search bound and rail; query values bind through Binds, columns are schema/allowlist-owned; plotted rows/bytes and group/key sample/name reads are bounded |
| SvelteKit server/client and session boundary | PASS | Backend fetching remains in server loads/handler; fetch route uses browser session's existing Rust client; search effect aborts superseded request and clears timeout; no browser credentials persisted/exposed |
| Tokens, shared modes and generated theme ownership | PASS | New styles use role variables; generator emits both schemes; independent six generator tests plus `--check` pass for all projections |
| Keyboard/semantic UI basics | PASS by source, no visual signoff | Sort buttons and aria-sort; labeled paging; native dialog Escape/close; splitter has Arrow-key path and focus treatment; fields/controls labeled. Source review does not claim rendered accessibility or light/dark geometry verification |
| Generated declaration/catalog parity | PASS for independently checked changed UI projections; limited for Rust codegen | Catalog `--check` passes; theme `--check` passes; changed generated `CheckAgent` comment matches `crates/wyrd-spec/src/card/verifier/mod.rs:663`; full Rust regeneration not rerun |
| Documentation-site rules | PASS / page rules N/A | Only generated CSS changes; no content page, frontmatter or copied example |
| Tooling/fixture syntax and credential isolation | PASS for inspected source/syntax | `bash -n` passes; MockServer JSON parses; placeholder mock API key only; app retains production wheel digest check and separated test environment |
| Required format/lint/build/journey evidence | Limited verification, no false PASS claim | Root independently checked Svelte types; caller reports fmt/clippy/journeys. Full build, Rust lanes and full codegen are not rerun by this reviewer; report does not convert claims into observed results |

The standards table is not a substitute for the behavior, invariant, persistence, or resilience reports. For example, SQL arithmetic, page completeness and series/fence recovery correctness remain those reviewers' behavioral questions. No task acceptance verdict is inferred here.

## Mandatory finding

### REPO-1 — New series tests and helper omit required panic rustdoc

- Classification: **VIOLATION**; explicit mandatory repository documentation gate.
- Rule: root AGENTS Code requires rustdoc on every new/changed Rust item and `# Panics` when it can panic, explicitly saying missing rustdoc blocks merge. `agent_docs/rust.md` Rustdoc explicitly includes private helpers, test helpers and tests.
- Locations: `redux/src/tables/metrics/series.rs:638–784`: helpers `point` (639), `projected` (683), `id` (691), `otlp_id` (701), `replaced` (706), and the five tests at 716, 725, 734, 747 and 764 have introductory docs but no `# Panics`. The latter four helpers and all tests directly call `expect`/`assert` or call the documented fixture assertion chain; `point` also carries a named-invariant `expect`. `redux/src/tables/mod.rs:2505` adds nested recursive `assert_untagged` with **no rustdoc at all**, despite its direct assertion.
- Evidence: `projected` expects projection/splitting and asserts zero rejected points; `id` expects the typed column and asserts a non-null first row; `replaced` expects the column/schema; the tests assert identity properties and the raw-Arrow test expects Variant encoding/validation. The enclosing test's `# Panics` does not document the separate nested helper item.
- Consequence/materiality: the new source fails a mandatory merge gate even if tests pass. This is not a claim of a reachable production panic, and probability does not waive the explicit gate.
- Required correction: add accurate rustdoc and `# Panics` to these changed items, explaining their fixture/invariant failure conditions. Preserve behavior and tests. No new helper, test harness, runtime guard, or abstraction is needed. Check the complete new module and changed nested helper once, in the same remediation; do not create a separate style-only round.
- Closure proof: source inspection verifies meaningful item docs and panic conditions for the listed helpers/tests; existing narrow format/lint checks suffice. No additional behavioral test is required for documentation.

Other new fallible production helpers in `series.rs` carry `# Errors`; new persisted-path helpers, replay regression and Oracle fixture helpers already document their panic conditions. The new admission method explains ordering, errors and when durable keys join the set; the module documents whole-batch retry and repeated rows. This finding deliberately groups the actual documentation omissions rather than turning each into another review round.

## Verification and limits

Independently run from the owning UI directory, all exit 0:

```text
mise exec -- node --test brand/gen-theme.test.mjs
  6 passed, 0 failed
mise exec -- node brand/gen-theme.mjs --check
  All targets are in sync with palette.json.
mise exec -- node packages/ui/scripts/catalog.ts --check
  exit 0
```

From repository root:

```text
mise exec -- bash -n scripts/dev/example-app.sh
  exit 0
mise exec -- python3 -c 'import json; json.load(open("tests/fixtures/mockserver/expectations.json")); print("MockServer JSON parses")'
  MockServer JSON parses
```

An initial JSON syntax command used `python`, unavailable in this mise environment; rerunning with `python3` passed. No fixture content changed.

Orchestrator `verification.md` independently records `mise exec -- pnpm check` passing shared package and host with 0 errors/warnings and `git diff --check` passing. Caller-reported evidence covers metrics journey, library tests, Oracle journey, touched-crate formatting/clippy and fork regression. This reviewer did not run Cargo, regenerate Rust-owned outputs, start services, inspect live browser rendering, or independently execute the external fork. Those are verification limits, not additional speculative findings. The binary fixture was checked through its source consumer/assertions, not independently decoded in this standards pass.

Overall: **FAIL (`REPO-1`)**. No other mandatory/material repository-rule finding retained.
