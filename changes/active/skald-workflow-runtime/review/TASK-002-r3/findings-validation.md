# Independent structured Ponytail validation — TASK-002-r3

Validation complete: six bounded findings remain, including the expressly requested supplemental Python selector boundary. No unresolved product decision or specification revision is needed. This is the independent ledger, not the orchestrator's verdict.

## Subject, authority and limits

Original cumulative base `0569b79702218600c4f9790f45cc03100d5c6f1c`; candidate `8a8282331042c3cc7610478b3d46583dbbf121f8`. Source evidence refers exclusively to `/tmp/wyrd-task002-r3-8a8282331`; actual cumulative diff is `/tmp/wyrd-task002-r3.diff`. The live review reports, subject, claim comparison and verification are inputs; live implementation and subsequent candidate `375d97e` are excluded. Only this report was written. No source edit, build, runtime test, code generation, baseline checkout or authorship investigation was performed.

Applied current user instructions, snapshot AGENTS §§2–9,11–12,15–16, agent-rules, current design/doctrine, reference router and applicable ownership, error, async, maintainer, spec-driven-development and testing guidance. Approved Revision 12 and replacement TASK-002-cleanup govern; original task, both prior verdict/validation ledgers and remediations establish closure obligations. Prior keyed-normalization instructions yield to Revision 12. CLI apply, selected gateway composition and server hosting remain later tasks.

Read all seven discovery reports and focused follow-up. Independent tracing inspected the correction owners and their callers: shared Workflow/file loader, Python selectors/parsers and exception conversion, native TS selector/result conversion, Cards exact reads and envelope projection, WorkflowBodies, server EffectiveSpecs and RegistrationWriter, SQL expected-UID recheck, registered relationship assertions, and owning-language journeys. Bundle hydration, binding/baseline preflight, original registration submissions, audit/version/persistence/upload boundaries are preserved sibling consumers. No CodeGraph index exists.

The current user handoff explicitly selects an owning Workflow error: `WyrdError::WorkflowInvalidCardRef`, `WYRD_WORKFLOW_400_INVALID_CARD_REF`, status 400, with `details.field` naming the bad field, consistently across all three languages. That is user authority for the correction outcome, not evidence that the excluded implementation achieves it. Generic request Validation is available in this candidate, but blanket replacement with it does not honor this selected Workflow owner. Its narrower reuse remains suitable for the separately requested generic non-Workflow Cards selector boundary.

Candidate-bound root evidence: fmt:check and check:skills-sync PASS; check:workspace-hack FAIL; explicit cumulative diff whitespace FAIL. Snapshot integrity was independently checked by root against the same candidate archive. Committed R2 execution claims and inspected assertions supply the runtime evidence; this validator claims no independent runtime reproduction and uses no newer follow-up results.

## Every proposed finding and conflicting closure claim

| Discovery proposal or claim | Disposition | Independent resolution |
|---|---|---|
| BEH-R3-001, INV-R3-002 ordinary slice, REPO-R3-11 ordinary slice, MNT-R3-1, SYS-001 ordinary slice, SEC-R3-001 ordinary slice | REVISED → FIND-TASK-002-11 | Python produces DataValidation before IO. The actual corresponding Rust and TS selector boundaries also use different generic/registry codes. Correct the user-selected Workflow owner across all three. Reject blanket generic Validation as the ordinary Workflow recommendation. |
| BEH-R3-SUP-001, INV-R3-002 supplemental slice, REPO-R3-11 supplemental slice, MNT-SUP-1, SYS-001 supplemental slice, SEC-R3-001 supplemental slice | CONFIRMED → FIND-TASK-002-12 | Model/Prompt/Data registry get/delete/list/latest identity parsing reaches the DataCard constructor. This is expressly requested older behavior, not attributed to this diff. Generic registry request parsing can reuse existing Validation; genuine Data body/interface validation cannot be changed. |
| INV-R3-001 and follow-up §1; behavior/maintainer blanket FIND-7 closure | REVISED → preserve FIND-TASK-002-7 | Native execution/provenance/v2 output closure is real. Each-language exact relationship proof still absent. Lower-tier PG checks do not replace AC-029. Python remedy is a concrete existing HTTP read, not an invented Cards.get API. Reject blanket closure, not the demonstrated execution improvements. |
| REPO-R3-12 and follow-up §2; system's bounded cancellation/availability assessment | CONFIRMED → FIND-TASK-002-13 | Actual filesystem calls occur directly on the async polling thread. Existing Tokio blocking pool solves this boundary. System's request-local/no-durable-state assessment remains correct; reject any inferred measured starvation or server outage. |
| REPO-R3-13 | CONFIRMED → FIND-TASK-002-14 | Candidate-bound hakari diff gives an exact generated feature-union mismatch. Known red gate is not waivable by scope/authorship language. |
| REPO-R3-14 | CONFIRMED → FIND-TASK-002-15 | Explicit base-to-candidate diff check reports the one new EOF blank. Plain dirty-tree diff check is not equivalent proof. |
| Registry durability report: empty ledger; security/system integrity assessments | Validated, no additional finding | Expected UID + identity + Active are selected under FOR SHARE and retained to audited write commit. Separate provenance maps and graph-only root metadata preserve tested behavior. No unsupported durability, privilege, lifecycle or recovery redesign is warranted. |

All proposals are mapped. Rejected remedy portions are omitted from the retained ledger: globally changing the Data-specific error helper, creating a generic Python registry API, replacing loaders/parsers/runtime, converting the synchronous loader itself to async, adding a pool/framework/checker, demanding production chaos proof or later-task CLI/gateway behavior, and treating excluded375 as closure or as a new defect.

## Final deduplicated stable ledger

### FIND-TASK-002-7 — REVISED — MISSING: required per-language exact relationship proof remains open

Discovery INV-R3-001; focused follow-up §1. Violates Revision 12 AC-029 and retained R2 FIND-7 locked Agent/Prompt identity proof. Locations: Rust `sdks/wyrd-sdk-rust/tests/workflow_loading.rs:345`, Python `sdks/wyrd-sdk-python/tests/integration/cards/test_cards_crud.py:578`, TS `sdks/wyrd-sdk-ts/wyrd/tests/integration/workflow-loading.test.ts:127`.

Producer: team and mixed registration receipts return exact registered references. Consumer: stored Workflow step refs/relationships and each Agent's Prompt ref/relationships. Rust serializes only the Workflow spec and compares two Agent UIDs; Python searches YAML for two UIDs; TS reads neither layer. None asserts the complete three-Agent/three-Prompt dependency graph, including the repository-local final reviewer. Successful output after v2 proves selected bodies and binder semantics, but can remain green while relationship metadata is omitted or incorrect. Server `pg_workflow_registration.rs:419–485` checks both persistence layers; that is supporting integration proof, not each SDK path required by AC-029. No execution or persistence defect is established by this finding.

Minimum correction: retain complete receipt identities from team registrations and mixed apply; extend the same three journeys with structured equality for Workflow→all three Agents and each Agent→its Prompt: kind, resolved space, name, exact version and UID, in both spec references and server-derived outbound relationship targets. Retain successful public exact/UID reload and after-v2 runs. Do not add another harness or change registration/execution.

Concrete existing reads: Rust `Cards::get`/`get_response` (`cards/handle.rs:279–294`) returns the envelope including relationships. TS public `Cards.get` (`wyrd/src/index.ts:1158`) returns the serialized full Card. Python's public Workflow returns a Skald value; `to_yaml` is not the registry envelope, and `PyCards` has no generic Agent/Workflow envelope get. Use supplemental public HTTP inspection inside the existing Python SDK journey: `GET {wyrd_server.base_url}/v1/cards/by-uid/Workflow/{workflow_uid}` and `/v1/cards/by-uid/Agent/{agent_uid}`, header `x-wyrd-access-token: Bearer {reader_key}`, decode `GetCardResponse["card"]`. This route is concretely implemented by `cards/routes.rs:46–111` and used by shared `reads/get.rs`; it performs normal read authorization/audit. Existing Python integration HTTP precedent uses httpx and this header (`gateway/test_gateway_openai_client.py:303`). This adds test inspection of the existing HTTP contract, no SDK API, transport implementation or Service-root WyrdState detour. Use the existing public Python SDK for apply/load/run and retain its structured/YAML identity observations; compare those to the HTTP envelope rather than claiming HTTP adds a missing Python envelope projection.

Closure: the three exact journey commands below must select and pass their strengthened assertions. Compare exact sets at both relationship layers, not substring presence or counts alone.

### FIND-TASK-002-11 — REVISED — INCORRECT: malformed Workflow load selectors lack the selected owning error across languages

Discovery ordinary slices listed above; expanded against actual equivalent shared/native consumers, under the user's explicit owning-parent choice. Violates REQ-054/INV-007 and current user-selected consistent error semantics.

Locations: Python `src/state/mod.rs:2599–2624`; shared Rust `wyrd-client/src/workflow.rs:141–159`; TS native `src/workflow.rs:75–101` and `src/cards.rs:167`. Producer-to-consumer: Python UID/space/name/version parsing or invalid combinations call interfaces `WyrdPyError::validation` (`wyrd-interfaces/src/error.rs:64–75`), producing DataValidation, whose catalog (`wyrd-spec/src/error.rs:1910`) instructs DataCard schema/artifact repair; conversion at interfaces `error.rs:282–296` faithfully copies it. Rust wrong-kind and versionless selectors instead produce RegistryInvalidCardSpec and RegistryVersionRequired. TS closed deserialization/exact CardRef construction and mixed shapes produce generic Validation. The wrappers pass those results through canonical Python/NativeWyrdError projection. These are reachable public failures before a registry read, not merely unused helpers; existing journey assertions pin the inconsistent results.

Minimum correction: use the user-selected derive-backed WorkflowInvalidCardRef contract in the canonical catalog and malformed registered-Workflow selector owner. Shared `WorkflowCards::load` owns wrong-kind/exactness checks; Python and TS own conversion of their untyped selector fields/combinations. They must produce that same code/status with field-specific details (`uid`, `space`, `name`, `version`, `kind`, or `selector` for the invalid combination). Reuse typed identity constructors and canonical exception/native projection. Python must classify shared parser failures at its Workflow boundary as Workflow-owned errors, while the generic parser correction in FIND-12 stays generic for siblings. This boundary conversion belongs to the Workflow view because it owns the requested operation; it is not a repeated downstream guard or permission to redefine DataValidation globally.

Existing generic Validation was checked, not assumed: catalog `error.rs:306–320` owns malformed Wyrd request fields; WorkflowValidation (`:3100`) is 422 validation of a WorkflowCard body. Neither is the user's selected 400 Workflow load-reference outcome. Adding the specified variant is authorized by the user's selection, not reviewer invention or approval of newer code. Preserve valid UID identity assertions, exact named selection, closed mixed-shape refusals, no IO/dispatch on malformed input, and permission/not-found/inactive/dependency errors returned by reads. Do not translate every Workflow-related server failure into the selector error.

Closure: extend the existing Rust/Python/TS journeys for malformed selector branches each language can express; assert selected code, status and field details, plus canonical title/remediation. Typed Rust cannot express an invalid raw string without first failing construction; cover wrong kind and versionless/exactness at its actual facade. Python/TS cover malformed UID/space/name/version and mixed/incomplete shape. Update source documentation and regenerate declarations/catalog; run the same exact journeys and codegen/type/native/error projection lanes. This report makes no claim about excluded375 or its tests.

### FIND-TASK-002-12 — CONFIRMED — INCORRECT: supplemental non-Workflow Python Cards selectors produce DataCard validation

Discovery supplemental slices listed above. Explicit user-requested supplemental scope; not a new cumulative regression. Locations: SDK `src/state/mod.rs:3010–3071`, callers `list_registry:1974`, `resolve_latest_registry:2013`, `delete_registry:2037`, typed Data/Model/Prompt get at `2199`, `2383`, `2549`.

Full selector/parser bodies show missing space/name, malformed UID and typed space/name/version failures all produce the same Data-specific interfaces helper. Model/Prompt retrieval and all typed delete/latest/list siblings reach these helpers before their detached registry calls. The downstream Python metadata mapper is correct; variant production is wrong. A Prompt/Model identity refusal reports DataCard schema/artifact advice even though no Data body was examined.

Minimum correction: correct shared generic registry identity parsers and selector-shape/UID branches to construct existing request `WyrdError::Validation` (`WYRD_SPEC_400_VALIDATION`, 400) with relevant field details, then preserve canonical conversion. Keep non-Workflow UID-plus-identity-assertions and optional version semantics unchanged. FIND-11's Workflow operation maps its own failures to its separately selected owner. Genuine DataCard schema/interface/artifact failures remain on DataValidation; do not globally modify `wyrd_interfaces::WyrdPyError::validation`, change the Data catalog, or create per-kind parsers. Limit this supplemental correction to the diagnosed selector/identity owner; unrelated Python validation uses are outside scope.

Closure: add one small parameterized public-Python selector check in existing `tests/unit/cards/test_registry_surface.py` (planned name `test_registry_selector_errors_use_request_validation`) for Data/Model/Prompt malformed space/name/version, missing identity and malformed UID, checking code/status/title/remediation/details.field before any registry IO. Include list/latest/delete calls to prove the shared owner rather than per-get patches. Unit proof is appropriate for local input rejection with no cross-boundary state; retain actual Workflow negatives in its real journey. Existing genuine Data validation regression `tests/unit/cards/data/test_datacard_serde.py::test_user_metadata_rejects_invalid_reserved_and_secret_values` must still return DataValidation. Exact focused commands below; no new harness/dependency needed.

### FIND-TASK-002-13 — CONFIRMED — VIOLATION: shared async file facade directly performs blocking filesystem IO

Discovery REPO-R3-12; focused follow-up §2. AGENTS §6 requires an explicit blocking strategy; rust-core Async says never block the async worker. Location `wyrd-client/src/workflow.rs:51–64` calls `wyrd_loader::load` and `Path::canonicalize` synchronously before its first await. Loader `parse.rs:68–77` performs filesystem metadata/read; local bundles may complete without yielding at all. Rust async journeys/shared test and exported native Node `workflow.rs:113` await this same facade. Python detaches its GIL and uses the shared runtime, but does not supply isolation for Rust/Node consumers. No server caller or measured scheduler outage is established.

Minimum correction: keep the public async facade and synchronous loader. Within `Workflow::from_path`, move the existing bundle load and entry canonicalization together into Tokio's already-installed blocking pool with an owned path, await that operation, then continue unchanged synchronous body preparation and lazy async external reads. Existing shared-client `auth.rs:600–610` demonstrates spawn_blocking; no runtime/helper service/dedicated pool is required. Preserve loader diagnostic mapping and handle join failure through the canonical error channel without an IO unwrap. A started blocking read may finish after abandonment; document that it cannot publish a partial Workflow or durable write. Do not move registry IO to blocking work or alter loader/parser public signatures.

Closure: source inspection proves filesystem work no longer runs on the polling thread; exact shared-client loading test and existing Rust/Node journeys pass. No synthetic host-load/starvation suite is warranted.

### FIND-TASK-002-14 — CONFIRMED — VIOLATION: generated workspace-hack gate fails

Discovery REPO-R3-13. AGENTS §12 makes a known red gate a completion blocker regardless of authorship. Candidate `mise run check:workspace-hack` exits 1; `/tmp/wyrd-task002-r3-hakari.log` shows four stale `spec_unstable_logs_enabled` inclusions for opentelemetry/opentelemetry_sdk in generated normal/build dependency sections of `crates/shared/workspace-hack/Cargo.toml:91,93,228,230`. Lane `mise.toml:1346` uses generate --diff, manage-deps --dry-run and verify. This is generated feature-union drift, not a Workflow runtime diagnosis.

Minimum correction: use the lane's sanctioned existing `mise exec -- cargo hakari generate` and `mise exec -- cargo hakari manage-deps`, inspect generated changes and rerun `mise run check:workspace-hack`. No baseline comparison, exemption, deleted check or unrelated feature redesign. Existing locked dependency generation covers the outcome.

### FIND-TASK-002-15 — CONFIRMED — VIOLATION: explicit cumulative patch whitespace fails

Discovery REPO-R3-14. Replacement task patch hygiene and AGENTS completion evidence apply. `git diff --check BASE CANDIDATE` exits 2 for `changes/active/skald-workflow-runtime/review/TASK-002-r2/maintainer-review.md:74`: new blank line at EOF. Earlier plain `git diff --check` examined a different patch. Minimum correction is removal of that excess final blank only; preserve historical report content. Prove against the original cumulative base and next committed candidate; no runtime test or broader rewrite needed.

## Prior FIND-1 through FIND-10 closure

| Stable ID | Independent closure at reviewed candidate |
|---|---|
| 1 | Closed invalid-state producer: WorkflowBodies separates sibling and registered sources, body lookup preserves Ref/Sibling and UID; EffectiveSpecs does the same for binding/baseline/Workflow siblings. Distinct-body Native outputs now prove selection in all languages. |
| 2 | Closed: actual example contains three versioned Prompt Cards, seven registered outcomes and PG proof of both relationship layers. |
| 3 | Superseded/closed by approved Revision 12: obsolete keyed normalizer/sole-use metadata removed; canonical untagged visitor retained. Do not recreate historical remedy. |
| 4 | Closed: RegistrationWriter forwards expected `(CardRef, CardUid)` pairs; SQL selects identity+expected UID+Active under FOR SHARE before reservation and holds locks through commit. Replacement and lifecycle lock tests assert refusal/interleaving. |
| 5 | Closed requested loading documentation/import-independent scope: Python # Errors and native outcome/no-partial/read-only docs are present. Separate false selector-domain semantics retained in FIND-11 do not reopen the old omission. |
| 6 | Closed: native Workflow signatures use module-top imported StdResult. |
| 7 | Partially open, narrowed above: successful public Native dispatch, ambient Rust child, body provenance and after-v2 outputs are closed; exact each-language relationship assertions remain missing. |
| 8 | Closed: TS recursive JsonValue input and canonical closed step/run/error fields match native serialization; type test exercises rejected non-JSON values and full snapshot access. |
| 9 | Closed: hydrator graph entrypoints are inherent; EffectiveSpecs owns preflight; meaningful RegistrationWriter owns state/caller. Audit/replay/lock/version/binding/upload seams remain in their existing owners/order. |
| 10 | Closed: transient graph_ready_submissions holders permit None/Scope Workflow preflight while original submissions still control hash/replay/persistence/allocation. PG valid omitted/scoped reload and invalid resolved binding no-write assertions cover both intents. |

No weakened authorization/UID/audit/provenance/version/Service behavior is required by these corrections. Registration remains declaration-only; Workflow remains non-principal and WyrdState Service-rooted.

## Focused proof and broader verification

Use these existing exact commands after corrections, recording selected counts; they are required recommendations, not executions by this validator:

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

The supplemental test name is planned in the existing owning test file, not claimed to exist at candidate 8a. TS full-name selector includes its describe block; Rust ignored SDK journey needs explicit selection. The preserved registration/provenance/version/audit fence commands above are copied from snapshot R2 remediation with their repository-managed setup. No later live evidence is substituted.

Broader final proof remains the cleanup task's complete relevant lanes: `test:shared`, `test:skald`, `test:cards:integration`, `test:wyrd-sdk`, `py:test:unit`, `py:test:cards:integration`, `py:typecheck`, `ts:test:unit`, `ts:test:integration`, `ts:typecheck`, `ts:napi:check`, `codegen:check`, `check:client-tier`, `check:sdk-client-tier`, `check:pyo3-scope`, `check:registry-tx-coupling`, formatting/lints for touched languages, plus repaired workspace-hack and explicit cumulative patch hygiene. Codegen derives stubs/catalog/declarations from source; no hand editing. Existing Service/loader regressions stay covered. Docs-site/skills lanes only need rerunning if touched; current candidate root fmt/skills passes stand as limited evidence. No unsolicited full repository release gate, live provider qualification or synthetic load.

Retained ledger: FIND-TASK-002-7, -11, -12, -13, -14, -15. Bounded correction recommendations are decision-complete; newer user-described Workflow implementation must receive its own immutable review and is neither approved nor rejected here.
