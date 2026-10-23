# TASK-003 independent invariant review

**Overall result: FAIL.** The seven prior findings have source-backed behavioral closure. The canonical-problem remediation introduces one independently reproduced public-package runtime regression, INV-003-8 below.

## Subject and method

Reviewed original base `695ca914df61a8683059f6f32279ec2148424e06` to candidate `c2bbd8e47e4752b3772ad75acfaf23f3400ac704`, original TASK-003, approved revision-12 specification, prior verdict and validated FIND-003-1–7 ledger, and remediation task/evidence. HEAD remained the candidate. No current-attempt peer report was read. Only this assigned report was written; package verification regenerated build outputs through the existing build owner.

Applied AGENTS.md and agent-rules; task-review role requirements; specification/TDD and maintainer guidance; applicable design/doctrine, Bifrost and error boundaries; UI skill and reference router. No CodeGraph index exists. Expanded the supplied navigation map through the complete foundation owners, public package manifests/exports, generators, components/primitives, WyrdView and host adapters, both panel endpoints, custom-app pages, session SDK factory, shared Rust query-only construction, build packing, and unit/live proof. The unrelated changes and successor TASK-027 identified by the subject are contextual, not TASK-003 drift. Layout state/geometry itself remains TASK-027-owned; its loader, action and document lifecycle seams were inspected here.

Producer-to-consumer traces:

- Authenticated `BrowserSession.client()` creates the request-local SDK client; Node uses existing Cards/Bifrost/Execution owners; MCP uses the caller's existing tools; portable loaders return generated domain values and positional results to render-only components. Neither endpoint selects a tenant from panel input.
- Chart range/input produces binds in `loadChart/chartParams`; the same `executed` object feeds Open in Query. Query descriptors and rows are retained; one first-match column authority now governs validation and drawing.
- Query streams are acquired/collected/closed by the Node adapter, with the SDK's `return()` retaining native cancellation ownership. Supersession belongs to WyrdView's controllers/current-generation check; dependent Card listing checks its signal before each next read.
- Action schemas validate the host handoff; WyrdView owns pending/outcome state; CardHeader/Chart own their controls. Late action completion mutates only the obsolete panel object after replacement, without writing a new panel's state.
- Canonical metadata is produced by wyrd-spec's generated problem examples, copied by the existing catalog generator and imported by `contract.ts`. That last import is now a runtime dependency of public loaders and MCP adapters and is the new failure source.

## Acceptance and invariant matrix

UI paths in this report are relative to `crates/wyrd/wyrd-server/wyrd-ui/` unless otherwise stated. PASS is scoped to inspected source and the stated proof; it does not claim an unexecuted platform lane passed.

| Requirement, acceptance criterion, constraint, or non-goal | Implementation evidence | Verification evidence | Result |
|---|---|---|---|
| Approved portable exports build and run from package contents, including Node loaders | `packages/ui/package.json` default `./loaders` export; `loaders.ts` runtime `contract.ts` dependency | Fresh direct Node public-export import fails with missing JSON import attribute; INV-003-8 | FAIL |
| Browser exports remain free of native Node SDK imports | `contract.ts`/`schemas.ts` use type-only wyrd-ai imports; runtime SDK stays in `node.ts`; browser artifact uses MCP | Package build and focused rendering proof pass; artifact source graph inspected | PASS |
| Catalog and generated public-error projections use their existing source authority | `scripts/catalog.ts`, `schemas.ts`, generated catalog/problems, mise codegen integration | Supplied codegen check; fresh package regeneration and canonical-problem assertions pass | PASS |
| Only catalog schemas define allowed panel/action dispatch | `validatePanel`, `validateIntent`, closed LOADERS/`components` map; strict schema objects | Rendering and dependent/action focused tests pass | PASS |
| Exact SQL and typed range/bucket binds; Open in Query reproduces execution | `chartParams`, `loadChart`, `ChartProps.executed`, Chart action payload | Range/brush/reset and fixed-clock relative override assertions pass | PASS |
| Explicit panel range wins; other panels share once-resolved view bounds | `loadChart` input-first precedence; WyrdView resolves inherited range once and passes it to each loader | Fresh focused explicit/inherited two-panel assertions pass | PASS |
| Schema mismatch/query refusal is visible and never partial success | `shapeChart` → `failed`/EmptyState; executor structured failures | Duplicate binding and canonical refusal assertions pass; historical live denial/limits proof | PASS |
| Positional duplicate descriptors, rows and nulls preserved | Existing bounded QueryResult; Node `collect`, MCP structured result; table positional cells | Historical SDK/MCP duplicate/null journey; fresh duplicate first-binding and table assertions | PASS |
| Full numeric timeseries/bar behavior, signed domains and approved folding threshold | `plot.ts` MAX_SERIES=20; Chart signed domains, zero baseline and style pairing | Fresh signed/mixed/flat/zero/positive mounted assertions and 5/20/21-series assertions pass; supplied browser spot-check | PASS |
| Exact Card read precedes dependent version listing; failure never triggers dependent reads | `loadCardHeader`, existing resolved Card identity, per-page cancellation check and versions failure state | Fresh dependent-read assertions; historical real Card/denial journey | PASS |
| CardHeader version sorting/paging/truncation, copied identity and honest failure states | `compareVersions`, bounded existing listing, CardHeader current ref/version/UID and versions state | Focused rendering/clipboard/paging assertions and supplied gallery inspection | PASS |
| Declared version/tab/Observe and chart action results stay visible; pending suppresses duplicates | WyrdView `act`, CardHeader version/tab state, ActionButton/ActionNote | Fresh direct controls and WyrdView outcome assertions pass | PASS |
| Supersession/removal/unmount prevents stale writes and stops subsequent dependent reads | WyrdView `start`/`abortAll`/`settle`; loadCardHeader signal check | Fresh dependent/supersession test; mount/destroy assertion | PASS |
| Supported query cancellation propagates during acquisition and collection | `node.ts` stream ownership; `signal.throwIfAborted()` after acquire/collect; SDK `return` | Fresh deterministic acquisition/collection/pre-aborted test passes | PASS |
| Existing six reads and shared query terminal/limit semantics retained | Node/MCP closed switches; existing SDK collect and shared Rust bounded projection; no new query transport | Historical live SDK/MCP read/denial/malformed-bind/ceiling evidence; source traces | PASS |
| Public local errors use canonical codes/title; malformed HTTP panels retain canonical metadata | `wyrdProblem`; generated Rust projection; both `refuse` endpoints; `failed` preserves upstream refusal state | Fresh Canonical UI problem projections passes; import-runtime defect tracked separately | PASS |
| Tenant/session boundary and no credentials in props/browser documents | Existing session client factory, request-scoped Node executor, server authorization; remoteLoader transports panel input only | Historical cross-tenant/authenticated BFF journey and secret absence assertions; source trace | PASS |
| Shipped and custom hosts share loaders/components; self-contained MCP artifact available | Gallery/BFF, example direct and view pages, `app/main.ts`, `vite.app.config.ts` | Fresh artifact build; supplied UI/example check/build, live AppBridge/Node/BFF proof | PASS |
| Individual component states/desktop-mobile/light-dark acceptance | Components/primitives, brand-generated tokens, gallery | Supplied human checkpoint and bounded signed-geometry inspection; fresh mounted component assertions | PASS |
| One build owner and real native SDK packaging | `scripts/build.sh` invokes existing ts:build, npm packing, host addon placement and runtime import check; Rust session Bifrost reuses read-only construction | Recorded macOS bundle import and Rust focused checks; source inspected | PASS |
| All three native targets/selected images and live degraded-terminal parity are actually proven | Build owner expresses targets; existing shared normalization preserves terminal evidence | Linux execution and live degraded parity remain explicitly outstanding; no platform defect inferred | FAIL (unverified evidence, not a proposed bounded implementation finding) |
| INV-001/002: current contracts, server durable/auth/audit ownership | Generated wire types; existing server reads and Rust client transport; components perform no durable mutation | Source and historical live boundary tests | PASS |
| INV-003/004/006: existing owners, minimal integration, no duplicate framework | Required package/loader/renderer split; query-only constructor extension; generator/build-owner extensions | Full foundation cumulative owner comparison | PASS |
| INV-005: do not misattribute TASK-001 prerequisites as domain foundation work | Original base already holds the session/query prerequisites identified by subject | Base/candidate ownership and diff inspection | PASS |
| INV-007/AC-010: credible narrow proof, no weakened checks or fake RED | Remediation retains canonical assertions and adds deterministic regression tests | Supplied RED records, fresh focused GREEN; signed RED was established in prior reviewer proof | PASS |
| Explicit non-goals: no Ask/A2UI adapter, saved store, generic writes, per-component backend routes, production MCP registration or publication | Read-only executors, host callbacks and unsaved view state; registration deferred to TASK-011 | Source/diff inspection | PASS |

The original broad AC-006/008/011 mappings are assessed for this foundation's reusable Card/query/component obligations; this task is not approval of future inventory/detail/telemetry pages or later component families.

## Prior finding closure

| Prior finding | Source-backed closure and affected siblings | Assessment |
|---|---|---|
| FIND-003-1 | `loaders.ts:159` selects explicit `input.range` before inherited context; the shared owner serves direct calls, WyrdView and both remote endpoints. Existing `executed` uses those same binds. | CLOSED |
| FIND-003-2 | `shapeChart.need` now selects the descriptor through `columnIndex`; stat/timeSeries/barRows use that same first-match selector, while tables retain every positional cell. Conflicting x/series descriptors are tested too. | CLOSED |
| FIND-003-3 | Chart `bounds` rounds signed sides outward with finite nonzero span for the required ordinary numeric cases; `barDomain/barY/barCells` include zero and nonnegative dimensions. Negative labels reserve room above the category axis. | CLOSED |
| FIND-003-4 | `plot.ts` folds only when entries exceed 20, retaining 19 plus other; Chart's existing 4-marker × 5-dash mapping handles every returned series and both legend/path consumers. | CLOSED |
| FIND-003-5 | CardHeader reads version/tab states, blocks pending/unavailable controls, restores authoritative selected version after emission and shares ActionNote with ActionButton. WyrdView still owns dispatch/state and exact intent validation. | CLOSED |
| FIND-003-6 | Node adapter closes a stream acquired after abort before collecting; active collection listener is removed in finally and post-collect abort cannot return rows. SDK `return()` already closes the native owner. | CLOSED |
| FIND-003-7 | Local fallback producers use generated canonical metadata, and both malformed-panel endpoints serialize full canonical problems. Upstream code/title and denied mapping remain unchanged; browser graph stays native-free. The new native ESM import defect is separately tracked below rather than reopening the canonical-code diagnosis. | CLOSED, with new regression INV-003-8 |

No further consumer guard, alternate error catalog, second query collector, transport, session authority or lifecycle owner is needed to retain these closures. The new ActionNote extracts existing feedback behavior for the newly covered controls; the new problem projection reuses existing source generation. These additions are earned by the approved correction rather than optional extensions.

## Proposed finding

### INV-003-8 — canonical problem import prevents public loaders loading in Node

**Classification:** REGRESSION. **Proposed next stable ID:** FIND-003-8, subject to independent validation.

**Violated obligation:** TASK-003 requires all approved exports to work from package contents; REQ-030 requires portable loaders to run in the user's Node server. Canonical projection reuse cannot break that existing host.

**Location:** `packages/ui/src/lib/contract.ts:19` (`import problems from './problems.json'`). The generated `dist/contract.js` preserves this import verbatim. `package.json:./loaders` exports `dist/loaders.js` as native ESM.

**Producer/state/sibling trace:** `scripts/catalog.ts` correctly copies the Rust browser-safe JSON to package source; svelte-package correctly includes/transpiles it; contract.ts introduces a runtime JSON module import without the standard JSON import attribute. `loaders.ts` imports `failed` and `wyrdProblem` from that module. The MCP executor also imports `wyrdProblem`; view/component bundling reaches the same metadata through their existing dependencies. Vite/Vitest resolve JSON during bundling and therefore hide the native Node failure. The `/node` adapter imports contract types only, so importing only that adapter does not prove the portable loader surface works.

**Reachability and observable consequence:** A supported custom Node server importing `wyrd-ai-ui/loaders` fails during module linking, before any loader or error helper executes. It cannot read Chart/CardHeader props. This is not a source-path-only issue: the failure also reproduces through the public package specifier from the workspace consumer directory.

**Independent reproduction:** after the normal package build, from `crates/wyrd/wyrd-server/wyrd-ui`:

```sh
mise exec -- node --input-type=module -e 'await import("wyrd-ai-ui/loaders")'
```

Exit 1 on repository-pinned Node v26.2.0: `ERR_IMPORT_ATTRIBUTE_MISSING`, `packages/ui/dist/problems.json needs an import attribute of "type: json"`. Direct import of built `dist/loaders.js` produces the same result. No mock, source edit or new harness was used.

**Smallest testable correction:** keep the generated JSON and current error owner. Add the standard JSON import attribute to the existing `contract.ts` import so the published ESM is valid in Node, and retain browser/package bundler compatibility. Do not add a loader hook, custom JSON transport, error catalog, dependency or fallback path. Prove the public loaders export imports in plain pinned Node from built package contents and can call a catalog-backed error path, then retain the focused browser canonical/rendering checks. A native import smoke check is necessary because another Vite test would keep masking this boundary.

## Verification assessment and limits

Fresh existing-owner focused verification:

```sh
mise exec -- pnpm --dir crates/wyrd/wyrd-server/wyrd-ui test src/lib/ui-library.test.ts -t '^TASK-003: Validated portable rendering$|^TASK-003: Dependent loads and safe actions$|^Node query acquisition cancellation$|^Canonical UI problem projections$'
```

Package and artifact built; four selected cases passed, six unrelated cases skipped by the explicit selector. This positively supports the seven source-backed prior closures while showing why bundler-only GREEN misses INV-003-8. Full base-to-candidate `git diff --check` passed. Supplied UI/example checks/builds, codegen, historical Rust/SDK/MCP/BFF journeys and human acceptance were assessed against their actual assertions; they were not all rerun here. The example's documented workspace invocation is the supported entry, so a direct arbitrary-cwd workspace-resolution failure is not a finding.

The signed-domain RED is substantive prior reviewer evidence, transparently recorded as not separately rerun by the implementer; no assertion was weakened to make that fix green. Requiring another artificial failure round would not improve proof. Linux bundle/image execution and live degraded Node/MCP parity remain unverified at their owning final stage, as the task requires; they do not justify a speculative platform fix here.

Skipped: fresh live infrastructure/Rust/platform aggregates and new visual inspection. Risk: the built public loaders currently fail to import in native Node; Linux and degraded-terminal runtime evidence also remains incomplete.
