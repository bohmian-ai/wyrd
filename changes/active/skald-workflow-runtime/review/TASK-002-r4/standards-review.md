# TASK-002-r4 repository standards review

Overall: **FAIL**.

Subject: complete cumulative
`0569b79702218600c4f9790f45cc03100d5c6f1c` →
`2ff7bbfa57737ce8a7a69287e2fa462c313b1a8c`, including
`375d97e67f3affe0d5c59727ef3135b22a459140`. The candidate remained `HEAD`
through this review. The only working-tree content was the orchestrator-created
untracked R4 review directory. No build, test, code-generation command, or
implementation edit was run by this reviewer.

This is an independent repository-standards audit. It does not use another R4
reviewer's conclusions and does not decide task acceptance. Source evidence is
from the immutable candidate; execution results are limited to the committed
task/remediation evidence.

## Authority and changed-surface coverage

The reference router is `architecture/references/README.md`. The complete
applicable authority set was `AGENTS.md`, `architecture/agent-rules.md`,
`architecture/wyrd-design.md`, `architecture/wyrd-doctrine.mdx`,
`architecture/wyrd-security-posture.md`, and the routed references listed
below. `architecture/bifrost-design.md`, analytical domain references, and
operations runbooks are not applicable: the cumulative diff changes no Bifrost
ingest/query/publication, analytical storage, deployment, backup, recovery, or
incident contract. The subject has no `.codegraph/` directory, so ordinary
repository navigation was used.

| Changed surface and consumers inspected | Governing authority and routed references | Coverage/result |
|---|---|---|
| `wyrd-spec` `CardRef` projection and `WorkflowInvalidCardRef`; generated error consumers | AGENTS §§2–4, 9, 12; design Card/reference/error contracts; doctrine; `doctrine/positioning-and-vocabulary.md`; `languages/errors.md` | Pure, typed, derive-backed contract. No IO, async, PyO3, second version field, or parallel error metadata. Source **PASS**; final generation proof fails below. |
| `wyrd-loader`; shared `Workflow`, `WorkflowBodies`, `CardGraphHydrator`, and `GraphTraversal`; existing Service-bundle consumers | AGENTS §§3–6, 9, 11–12, 16; design Workflow/loading/reference contracts; `doctrine/architecture-constraints.md`; `architecture/patterns.md`; `languages/{rust-core,maintainer-style,testing-workflows}.md` | Existing loader, graph owner, and Skald owner are composed rather than duplicated. `Workflow::from_path` isolates synchronous filesystem work with Tokio's installed `spawn_blocking`; pure validation remains synchronous. Struct ownership, rustdoc, cancellation/no-partial semantics, imports, and dependency direction **PASS**. |
| Skald body discovery, Agent/Prompt hydration, declaration-only registration validation, and execution callers | AGENTS §§3–6, 9–10, 16; design Skald boundary; doctrine/constraints/patterns; Rust/error/maintainer references | Skald remains registry/SQL/tenant independent. `WorkflowBodies` and `CardBodyResolver` own cohesive state; registration binds no tools, secrets, providers, or execution. **PASS**. |
| Server `EffectiveSpecs` and `RegistrationWriter`; SQL relationship UID recheck; registration/read callers | AGENTS §§3–6, 9, 11–12, 15–16; agent-rules SQL/RLS/audit rules; design registry lifecycle; security posture; architecture patterns; testing/errors | Durable behavior stays server-owned. Borrowed `TenantConn`, caller-owned transaction, authorization/audit path, exact Active UID fence, authored-submission persistence, tenant isolation, and postcommit behavior **PASS**. No raw pool or manual tenant-filter drift entered production paths. |
| Rust SDK exports and Cargo manifests/lock/workspace feature union | AGENTS §§2–6, 11–12; client model and dependency-cost rules; constraints/patterns; Rust/testing references | Rust SDK remains a thin re-export. Shared client uses existing Skald/loader/runtime owners; no client-tier SQL/cloud/DataFusion dependency or unearned feature was added. Hakari drift was regenerated through the existing standard mechanism. Source **PASS**; final boundary evidence is incomplete below. |
| Python `Workflow.from_path`, `PyWorkflowCards.load`, generic Cards selectors, exports, generated stubs, examples, unit and real-server journey | AGENTS §§2–4, 7–12, 16; `languages/{pyo3-boundaries,python-api-and-stubs,errors,rust-core,testing-workflows,maintainer-style}.md` | Wrapper placement, shared-runtime/GIL discipline, public imports, selector error ownership, and runtime-lifetime tests **PASS** in source. Generic selectors now use request validation; Workflow selectors use the Workflow catalog code. Final stub/type/codegen proof fails below. |
| TypeScript/N-API `Workflow`, Cards view, selector parser, JSON-safe run snapshot, generated declarations, unit and real-server journey | AGENTS §§2–6, 9, 11–12, 16; `languages/{typescript-guide,errors,rust-core,testing-workflows,maintainer-style}.md` | Native layer remains a thin shared-client projection; public selectors are a closed union, inputs/results are JSON-safe, stable errors are preserved, and Node behavior is tested under Node. Per-test `60_000` timeout is an established Vitest option already used by peer server journeys, not a bespoke mechanism. Source **PASS**; final N-API generation proof fails below. |
| Rust/Python/TypeScript journeys, server/Postgres and SQL tests, fixtures | AGENTS §§11–12; agent-rules test placement/runtime rules; testing-workflows | External tests earn their placement by spanning real server/Postgres or language runtimes. Journeys cover all three first-class SDKs and now inspect exact Workflow→Agent and Agent→Prompt spec refs and relationships. No assertion, test, check, allow, or ignore was weakened to clear a failure. Source shape **PASS**; execution evidence is assessed separately. |
| Example bundle, docs-site page, READMEs, Python example | AGENTS §§2, 8–12, 16; design/doctrine/reference forms; Python/errors/testing/maintainer references | Canonical untagged references, versioned Card envelope, provider/request shape, and `from_path` surface align. R2 independently records `docs:check` PASS, and these docs did not change after that candidate. **PASS**. |
| Architecture/doctrine/reference edits; canonical and Claude skill mirrors; active spec/tasks and prior review records | AGENTS §§1–2, 12, 14–16; spec-driven-development; implementation-execution; task-review skill | Revision 12 aligns authority with the selected existing owners and removes obsolete keyed reference prose. Skill mirrors compare byte-for-byte and the recorded skills-sync gate passed before unchanged later commits. No new scanner, gate, symbol allowlist, setting, compatibility option, or nonstandard enforcement file was introduced. The reuse table is review/task evidence, not an executable repository mechanism. **PASS**. |

## Applicable-rule results

| Rule | Candidate evidence | Result |
|---|---|---|
| Server owns durable registration; shared client owns SDK composition; Skald owns workflow runtime; `wyrd-spec` stays foundational | `wyrd-client/src/workflow.rs:19–191`; `cards/hydrate/{graph,workflow}.rs`; `skald-workflow/src/bodies.rs`; `wyrd-server/src/components/cards/{resolve,service}.rs`; manifests | **PASS** |
| New/materially changed Rust follows struct-centered ownership, narrow async, top-level imports, typed errors, and complete rustdoc | `GraphTraversal`, `WorkflowBodies`, `CardBodyResolver`, `EffectiveSpecs`, and `RegistrationWriter` own their workflows; `Workflow::from_path` uses `spawn_blocking` at `workflow.rs:57–64`; pure planning/validation remains sync | **PASS** |
| Public errors use the derive-backed catalog and project consistently | `wyrd-spec/src/error.rs:3114–3130`; shared/Python/N-API selector producers; TypeScript generated error-code union | **PASS** in source |
| PyO3 stays in the Python SDK, releases the GIL for blocking work, uses the shared runtime, and exports public typed surfaces | `sdks/wyrd-sdk-python/src/workflow.rs:472–507`; `src/state/mod.rs:2560–2640,3023–3097`; public package/stubs/tests | **PASS** in source |
| TypeScript/N-API stays thin, typed, JSON-safe, and runtime-tested | `native/src/workflow.rs`; `native/src/cards.rs:153–173`; `wyrd/src/index.ts:1189–1253`; TS unit/integration tests | **PASS** in source |
| Tenant/RLS, audit, caller-owned transaction, and exact relationship identity remain intact | server resolution/write flow, SQL `recheck_active_card_refs`, `pg_workflow_registration`, `pg_cards_register` | **PASS** |
| Every shipped SDK surface has a real client→server→client journey; language lifetime behavior stays in its runtime | Rust `workflow_loading.rs`; Python `test_cards_crud.py`; TS `workflow-loading.test.ts`; server/SQL seam tests | **PASS** in source and recorded focused execution |
| Generated contracts and public typing must regenerate/type-check after their owning sources change; matching boundary checks must pass | R3 remediation requires these final lanes at `TASK-002-R3-close-remaining-review-gaps.md:104–106`; final evidence at `:128–136` omits them after later catalog/wrapper/declaration edits | **FAIL**, REPO-R4-01 |
| Docs and mirrored skills use existing repository checks; cumulative patch hygiene and workspace feature union are clean | R2 `verification.md:18–20`; R3 evidence for workspace-hack; independent `cmp`; `git diff --check BASE..CANDIDATE` exit 0 | **PASS** |
| Human standing direction: reject bespoke mechanisms absent from established or broadly used practice | The candidate adds no new executable check, setting, feature flag, parser dialect, compatibility mode, or enforcement artifact. `spawn_blocking`, Vitest item timeouts, generated declarations, typed unions, and real-server journeys are established native/repository mechanisms. | **PASS** |

## Material finding

### REPO-R4-01 — VIOLATION: final generated-contract, typing, and boundary gates are not recorded for the final sources

**Violated rule.** AGENTS §§8, 11–12 require generated stubs/declarations to
regenerate cleanly, `py:typecheck` when public Python typing changes,
`ts:napi:check` for N-API declarations, `codegen:check` for catalog/stub drift,
and the matching client/PyO3 boundary checks for boundary-sensitive edits. The
approved task repeats these exact completion lanes at
`tasks/TASK-002-cleanup.md:254–267`; R3 repeats them as required final proof at
`review/TASK-002-r3/TASK-002-R3-close-remaining-review-gaps.md:104–106`.

**Location and evidence.** After the earlier recorded R2 gates, commit
`375d97e67` added `WyrdError::WorkflowInvalidCardRef` at
`crates/wyrd-spec/src/error.rs:3114–3130`, changed the Python wrapper and both
generated `.pyi` files, changed the N-API selector source and both generated
`index.d.ts`/`index.d.cts` files, and changed the generated TypeScript error-code
union. Later R3 commits changed `wyrd-client::Workflow::from_path` and the
generic PyO3 selector boundary. The final implementation evidence at
`TASK-002-R3-close-remaining-review-gaps.md:128–136` records the focused
journeys/tests, selected Rust clippy, Rust/Python formatting/linting,
`ts:typecheck`, `check:workspace-hack`, and cumulative diff hygiene, then states
that broader lanes were not run. It does not record final-candidate
`codegen:check`, `ts:napi:check`, `py:typecheck`, `check:client-tier`,
`check:sdk-client-tier`, or `check:pyo3-scope`. R2 success predates the exact
catalog, wrapper, and generated declaration changes and therefore cannot prove
the final generated outputs or boundary graph.

**Consequence.** The source and checked-in projections appear aligned, but the
repository's authoritative generators/type checkers have not demonstrated that
the final `.pyi`, `.d.ts`/`.d.cts`, error-code union, feature/dependency graph,
and PyO3/client-tier placement are reproducible and accepted. Under AGENTS
§12, absent required final proof prevents completion even when focused runtime
tests pass.

**Smallest testable correction.** Do not add a check, setting, script, or
allowlist. Run and record the existing standard lanes against an immutable next
candidate:

```bash
mise run codegen:check
mise run py:typecheck
mise run ts:napi:check
mise run check:client-tier
mise run check:sdk-client-tier
mise run check:pyo3-scope
```

If any lane changes generated output or fails, correct its owning source and
rerun the affected focused proof plus the lane; never hand-edit generated
artifacts or weaken a gate. Existing R2 `docs:check` and skills-sync evidence,
and final workspace-hack/diff-hygiene evidence, need no unrelated replacement
because their governed sources did not change afterward.

## Verification notes

- Recorded focused proof on the final candidate covers the shared loader/client,
  server registration/refusal/UID-race paths, all three SDK journeys, generic
  Python selector errors, formatting/lints, TypeScript typecheck,
  workspace-hack, and cumulative whitespace hygiene.
- R2 independently records `docs:check` and skills-sync PASS, and those governed
  sources were unchanged by the later R3 implementation commits.
- No runtime/build/codegen command was launched in this review, as assigned.
- No additional standards finding was found. In particular, the known
  Python-versus-Rust/TypeScript range-selector observation is not a repository
  rule violation: the approved public Python contract requires an exact version
  and permits pre-IO refusal, while Rust/TypeScript may let the server reject a
  syntactically valid version block.
