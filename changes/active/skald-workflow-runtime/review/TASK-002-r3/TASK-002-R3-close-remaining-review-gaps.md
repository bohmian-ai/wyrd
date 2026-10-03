---
id: TASK-002-R3
kind: remediation
status: ready
spec: SPEC-skald-workflow-runtime
spec_revision: 12
parent_task: TASK-002-cleanup
remediates: [FIND-TASK-002-7, FIND-TASK-002-12, FIND-TASK-002-13, FIND-TASK-002-14, FIND-TASK-002-15]
base: 0569b79702218600c4f9790f45cc03100d5c6f1c
reviewed_candidate: 8a8282331042c3cc7610478b3d46583dbbf121f8
---

# Close remaining cumulative TASK-002 review gaps

Implementation skill: `$wyrd-implement`. This is a bounded implementation handoff from independently validated findings. Work forward on the current branch; preserve newer work and validate it rather than resetting to the older reviewed candidate or duplicating an already-written correction.

## Authority and subject

- Repository: `/home/thorrester/Documents/GitHub/wyrd`
- Approved spec: `changes/active/skald-workflow-runtime/spec.md`, Revision 12
- Original superseded task: `changes/active/skald-workflow-runtime/tasks/TASK-002-load-and-register-graphs.md`
- Active replacement: `changes/active/skald-workflow-runtime/tasks/TASK-002-cleanup.md`
- Original cumulative base: `0569b79702218600c4f9790f45cc03100d5c6f1c`
- Reviewed candidate: `8a8282331042c3cc7610478b3d46583dbbf121f8`
- Validated diagnosis: `changes/active/skald-workflow-runtime/review/TASK-002-r3/findings-validation.md`
- Verdict and independent reports: same r3 directory; prior history in r1/r2, including `TASK-002-R2-close-cleanup-review-gaps.md`
- Governing rules: AGENTS.md, architecture/agent-rules.md, current wyrd-design/doctrine and applicable reference-router guidance

The live branch advanced to `375d97e67f3affe0d5c59727ef3135b22a459140` during the review. FIND-TASK-002-11 was resolved by that commit and is removed from this task. FIND-12 is an explicitly requested supplemental review of older non-Workflow Python behavior, not a regression attributed to TASK-002.

## Diagnosis and selected corrections

### FIND-TASK-002-7 — Finish exact relationship proof in each language

Revision 12 AC-029 requires each real SDK journey to assert exact relationships; R2 also requires locked Agent/Prompt identities. Native execution, body-source distinction, ambient configuration and after-v2 output proof now pass in all three languages. The remaining gap is narrower: Rust `tests/workflow_loading.rs:345` compares two Agent target UIDs; Python `test_cards_crud.py:578` checks UID substrings in YAML; TS `workflow-loading.test.ts:127` inspects neither dependency relationship layer. None compares all three Agent/Prompt pairs, including the local final reviewer, as structured exact references and server-derived relationships. Supporting PG persistence assertions do not substitute for these each-language obligations. Missing relationship metadata could leave output-only assertions green.

Extend the existing journeys, retaining registration receipt identities and comparing Workflow→all three Agents and each Agent→its Prompt: kind, resolved space, name, exact version and UID, both spec references and matching outbound relationship targets. Preserve public apply/load/run, exact/UID reload and after-v2 runs. No runtime or persistence fix is justified by this proof finding.

Reuse existing envelope reads: Rust Cards::get/get_response and TS Cards.get. Python Cards has no generic Agent/Workflow envelope get; do not invent one. Supplement its existing SDK journey with the existing public HTTP reads `GET {wyrd_server.base_url}/v1/cards/by-uid/Workflow/{workflow_uid}` and `/v1/cards/by-uid/Agent/{agent_uid}`, header `x-wyrd-access-token: Bearer {reader_key}`, decode response `card`. The registered routes and shared reads use this authorized/audited contract; existing integration httpx usage supplies the test mechanism. Compare SDK identity observations with these envelopes, and state that HTTP supplies envelope inspection rather than claiming a new Python envelope projection. No Service-root WyrdState detour, SDK transport implementation or dependency is needed.

### FIND-TASK-002-12 — Correct generic Python registry selector error production

Explicit supplemental scope: shared `state/mod.rs:3010–3071` selector and identity parsers use the Data-specific interfaces constructor. Data/Model/Prompt get and generic list/latest/delete consume them before registry IO. Malformed space/name/version, missing identity and malformed UID receive DataCard schema/artifact advice even for Prompt/Model selection. The downstream metadata mapper is correct; the wrong variant is produced by the selector owner.

Correct that shared generic selector/identity boundary once to construct existing `WyrdError::Validation` (`WYRD_SPEC_400_VALIDATION`, 400), with the relevant field details and canonical conversion. Keep optional version and UID-plus-identity-assertion semantics intact. Workflow selectors keep `WorkflowInvalidCardRef`. Genuine DataCard schema/interface/artifact validation remains on DataValidation: do not globally redefine `wyrd_interfaces::WyrdPyError::validation`, alter its catalog meaning or add per-kind parsers. Unrelated Python validation uses remain outside this correction.

Add one small parameterized public-Python check in the existing registry-surface unit tests covering Data/Model/Prompt invalid identity fields/UID/missing selectors and list/latest/delete sibling calls. Assert code/status/title/remediation/details.field and local refusal before IO. Unit proof is sufficient for this stateless boundary; retain Workflow negatives in the real journey and genuine Data validation regression coverage.

### FIND-TASK-002-13 — Isolate synchronous filesystem work at the shared async facade

AGENTS §6 and rust-core Async require an explicit blocking strategy. `wyrd-client/src/workflow.rs:51–64` directly calls synchronous loader and canonicalize before its first await; loader parse performs metadata/read. Real Rust and Node async callers therefore execute filesystem IO on the polling thread. Python GIL detachment does not provide the shared boundary. This is a standards violation; no measured starvation or server outage is asserted.

Keep Workflow::from_path as the async composition owner and the loader synchronous. Offload the existing bundle load and entry canonicalization together through Tokio's installed blocking pool, await that result, then retain ordinary body preparation and lazy async registry reads. The shared client's existing auth blocking boundary demonstrates the mechanism. No dedicated pool, runtime, loader service, parser replacement or SDK-specific workaround is needed. Preserve diagnostic mapping and handle join failure through canonical errors without external-IO unwrap. Document that an already-started read may finish after abandonment but cannot publish a partial Workflow or durable state. Source inspection and existing focused loader/client/Rust/Node journeys suffice; no synthetic host-load suite.

### FIND-TASK-002-14 — Repair the known generated feature-union failure

Candidate-bound `mise run check:workspace-hack` fails at hakari generate --diff. Four generated OpenTelemetry/SDK normal/build entries still include `spec_unstable_logs_enabled` contrary to hakari output (`workspace-hack/Cargo.toml:91,93,228,230`). AGENTS §12 makes the known red check a completion blocker regardless of origin or task-lane list. This is generated manifest drift, not a Workflow runtime defect.

Use sanctioned `mise exec -- cargo hakari generate` and `mise exec -- cargo hakari manage-deps`, inspect the generated changes, and pass `mise run check:workspace-hack`. Do not weaken/delete/exempt the check, investigate authorship with older checkouts, or redesign telemetry features merely to conceal drift.

### FIND-TASK-002-15 — Close cumulative patch hygiene

Explicit `git diff --check ORIGINAL_BASE REVIEWED_CANDIDATE` fails at prior `review/TASK-002-r2/maintainer-review.md:74` for a new blank line at EOF. A plain dirty-tree check does not prove this cumulative patch is clean. Remove that excess EOF blank only, preserving historical report content, then pass the explicit original-base-to-next-candidate command. No behavior test is needed.

## Preserved behavior and non-goals

Preserve approved untagged reference forms, Ref/Sibling provenance, exact Active UID fences, tenant/RLS authorization, audit and caller-owned transaction composition, idempotency/version allocation, original authored intent, bindings/relationships and existing postcommit upload recovery. Keep the current hydrator, EffectiveSpecs, RegistrationWriter, canonical visitor, loader and Skald binder/executor owners. Keep contracts pure and Workflow declarative/non-principal; WyrdState stays Service-rooted. Runtime graph reads still omit artifact inventory, while Service Bundle hydration retains publication behavior.

No new public registry API, runtime/transport/cache/graph/parser, dependency, gate/scanner, broad Python validation rewrite or unrelated structural cleanup. Do not implement TASK-003/004/005, selected gateway preparation, server hosting, CLI apply, live-provider or production chaos qualification. Preserve prior closed findings rather than rebuilding superseded mechanics.

## Acceptance mapped to findings

| Finding | Closure |
|---|---|
| 7 | Each existing language journey compares full exact spec refs and relationship targets for all three Agent/Prompt pairs, retaining public Native exact/UID and after-v2 execution. |
| 12 | Shared generic Python get/list/latest/delete identity failures emit request Validation before IO, while genuine Data validation remains DataValidation and Workflow keeps `WorkflowInvalidCardRef`. |
| 13 | Existing synchronous filesystem load/canonicalization runs off the async polling thread; diagnostics, lazy hydration and no-partial/read-only behavior remain green. |
| 14 | Sanctioned generated workspace feature union passes check:workspace-hack. |
| 15 | Exact original-base-to-next-committed-candidate whitespace check passes with historical content preserved. |

## Focused proof

Extend existing tests and record meaningful RED/GREEN where behavior/assertions change; do not manufacture runtime failures for documentation, structural or generated-file hygiene. Record selected counts and actual diagnostics. Follow traced failure diagnosis before modifying assertions/timing/skips. All named tests require exact execution, not only aggregate lanes.

```bash
mise exec -- cargo nextest run --locked -p wyrd-client --lib -E 'test(=workflow::tests::from_path_uses_existing_loader)'
mise exec -- cargo nextest run --locked -p wyrd-loader --lib -E 'test(=tests::load_explicit_workflow_bundle)'
mise exec -- scripts/postgres/with-test-postgres.sh -- bash -lc 'mise run db:migrate:all:inner && mise exec -- cargo nextest run --locked -p wyrd-server --test pg_workflow_registration -E "test(=registers_only_valid_explicit_workflow_graphs)"'
mise exec -- scripts/postgres/with-test-postgres.sh -- bash -lc 'mise run db:migrate:all:inner && mise exec -- cargo nextest run --locked -p wyrd-server --test pg_workflow_registration -E "test(=refuses_stale_preflight_after_dependency_replacement)"'
mise exec -- scripts/postgres/with-test-postgres.sh -- bash -lc 'mise run db:migrate:all:inner && mise exec -- cargo nextest run --locked -p wyrd-server --test pg_workflow_registration -E "test(=fetches_and_executes_locked_workflow_graph)"'
mise exec -- scripts/postgres/with-test-postgres.sh -- bash -lc 'mise run db:migrate:all:inner && mise exec -- cargo nextest run --locked -p wyrd-sql --test pg_cards_register -E "test(=relationship_recheck_blocks_target_lifecycle_race)"'
mise exec -- scripts/postgres/with-test-postgres.sh -- bash -lc 'mise run db:migrate:all:inner && mise exec -- cargo nextest run --locked -p wyrd-sdk-rust --test workflow_loading --run-ignored all -E "test(=workflow_loading_journey)"'
mise exec -- scripts/postgres/with-test-postgres.sh -- bash -lc 'mise run db:migrate:all:inner && mise run py:setup && cd sdks/wyrd-sdk-python && mise exec -- uv run python -m pytest -q -m integration tests/integration/cards/test_cards_crud.py -k test_workflow_loading_journey'
mise exec -- scripts/postgres/with-test-postgres.sh -- bash -lc 'mise run db:migrate:all:inner && mise run ts:build && mise run ts:build:testing && cd sdks/wyrd-sdk-ts/wyrd && mise exec -- pnpm exec vitest run tests/integration/workflow-loading.test.ts -t "^Workflow loading workflow loading journey$"'
mise run py:setup
mise exec -- uv run --directory sdks/wyrd-sdk-python python -m pytest -q tests/unit/cards/test_registry_surface.py::test_registry_selector_errors_use_request_validation
mise exec -- uv run --directory sdks/wyrd-sdk-python python -m pytest -q tests/unit/cards/data/test_datacard_serde.py::test_user_metadata_rejects_invalid_reserved_and_secret_values
mise run check:workspace-hack
git diff --check 0569b79702218600c4f9790f45cc03100d5c6f1c <NEXT_COMMITTED_CANDIDATE>
```

The supplemental registry test name is planned, not claimed to exist at candidate 8a; if naming changes, record/run its exact replacement command. Rust journey is ignored/gated and needs explicit selection; TS filter must include the describe block. Python envelope inspection uses the journey's authorized key and existing HTTP route.

## Broader verification and handoff

Run the cleanup task's applicable final lanes: `test:shared`, `test:skald`, `test:cards:integration`, `test:wyrd-sdk`, `py:test:unit`, `py:test:cards:integration`, `py:typecheck`, `ts:test:unit`, `ts:test:integration`, `ts:typecheck`, `ts:napi:check`, `codegen:check`, `check:client-tier`, `check:sdk-client-tier`, `check:pyo3-scope`, `check:registry-tx-coupling`, and format/lints for touched languages, plus repaired workspace-hack and explicit cumulative hygiene. Regenerate contract artifacts from source; do not hand-edit them. Preserve shared Service/loader regression coverage. Add docs:check or check:skills-sync only if those surfaces are touched; no unsolicited release aggregate.

Record each finding's actual source/proof, including what the newer existing fix supplies, and any diagnosed blockers. Route implementation directly to `$wyrd-implement`; subsequent `$wyrd-task-review` must audit the original-base-to-new-immutable-candidate range against all prior obligations and this remediation. The present review does not implement these changes or approve the excluded later commit.

## Implementation evidence

### Diagnosis: TypeScript journey timeout

- **Symptom:** after adding the envelope reads, `workflow-loading.test.ts` "workflow loading journey" failed with `Test timed out in 5000ms` at the `it` on line 72.
- **Evidence:** the trace shows server start at 23:56:51, the last registration (team-v2) at 23:56:56.306 and shutdown at 23:56:56.78, with no error; the same command with `--testTimeout=60000` passed in 6303ms.
- **Cause:** no vitest config exists under `sdks/wyrd-sdk-ts`, so this single-`it` journey (which starts its own server) ran under vitest's 5000ms default; the four added `Cards.get` round-trips pushed it past that budget.
- **Fix site:** the journey's own `it` call, given `60_000` like every other `startTestServer` integration test in the directory (`gateway-admin`, `cards-state`, `verification-run`, `operator-connections`). A read-only diagnostician independently confirmed the cause and fix site; no other file is affected.
