# TASK-002 R3 Task Implementation Review

## Immutable subject

- Repository: `/home/thorrester/Documents/GitHub/wyrd-verified-change-contract`
- Original base: `c8bb490ad814c0c7770cac33ed7779897ff776e4`
- Cumulative candidate: `04f73570397d5123eb767abafa60d016c37de1db`
- Reviewed range: `c8bb490ad814c0c7770cac33ed7779897ff776e4..04f73570397d5123eb767abafa60d016c37de1db`
- Approved specification: `changes/active/verified-change-contract/spec.md`, revision 32
- Original task: `changes/active/verified-change-contract/tasks/TASK-002-scoped-observation-and-bifrost-tables.md`
- Prior reviews and remediation: `review/TASK-002-r1/` and `review/TASK-002-r2/`
- Locked logic authorities: `changes/active/verified-change-contract/architecture/logic/run_api.md` and `table_schema.md`

The complete cumulative range was reviewed. The R2 remediation diff was used
only to trace closure of the four findings retained by the prior immutable
review. The candidate remained exact through source inspection and focused
verification.

## Acceptance matrix

| Requirement, acceptance criterion, constraint, or non-goal | Implementation evidence | Verification evidence | Result |
|---|---|---|---|
| TASK-002 outcome: one state-owned Bifrost writer, one invocation, immutable Card scopes, and Drift/Eval/generic enqueue | `crates/shared/wyrd-client/src/state.rs`, `src/observe/{mod,lifecycle,drift,eval}.rs`; all emits reuse the state-held `Bifrost` and existing `WriterPool` | Shared lifecycle/projection tests and all three real SDK journeys recorded in TASK-002 | PASS |
| REQ-075 / INV-012: reuse canonical Drift/Eval records and omit Verifier/run identity from authored records | Canonical records remain in `wyrd-spec`; `observe/drift.rs` and `observe/eval.rs` project them without `eval_ref`, `drift_ref`, or authored `run_id` | Shared projection/schema tests and generated-schema checks recorded in TASK-002 | PASS |
| REQ-076 / INV-010: reuse the bounded queue, Gate, Scribe, and Bifrost; no second ingest path | `Observe::{drift_value,eval_value,record_value}` route through `Bifrost::insert_into`; `WriterPool` remains the only producer pool | `verify:bifrost` 9/9 and the Rust/Python/TypeScript journeys recorded on the code-bearing remediation candidate | PASS |
| REQ-118 / REQ-121: authenticated writer and observed subject remain distinct | `Run::correlation` supplies the scoped `CardRef` and invocation `RunId`; server-managed principal/card columns remain outside user rows | Each journey reads rows back by exact subject UID and shared invocation ID | PASS |
| REQ-122 / AC-024 task slice: five exact schemas, daily layouts, Bloom declarations, and sensitivity | `crates/vala/vala-bifrost-redux/src/tables/{drift,eval,verification}` and the canonical registry in `tables/mod.rs` match `table_schema.md` | Catalog/schema unit tiers recorded in the task; no R2 remediation changed these contracts | PASS |
| REQ-123: locked Rust, Python, and TypeScript run APIs, including configurable Rust startup | `WyrdState::start_bifrost_with_config` reuses `Bifrost::connect_with_config`; Python and TypeScript remain thin projections of shared Rust | Rust journey compiles the configured call; language type, unit, and integration lanes recorded | PASS |
| Scenario 1 / REQ-127: startup describes both fixed tables and refuses missing or incompatible declarations | `StartClaim::complete` describes Drift and Eval; `require_projection` compares the complete ordered name/type/nullability sequence | Focused incompatible-schema tests plus each R3 SDK journey's per-table real-server startup refusal | PASS |
| Scenario 1 / REQ-133: one start, retry after ambiguous drain, and terminal successful shutdown across races | `BifrostLifecycle::{claim,shutdown}` and fenced `StartClaim::{complete,drop}` retain the same writer after drain failure and close permanently after success | Five focused lifecycle/cache tests and state-level same-batch retry proof recorded | PASS |
| Scenario 2: one UUIDv7 invocation with immutable root/Model/Agent views and local invalid-alias refusal | `Run::{new,for_card,correlation}` reuses the hydrated graph without mutable active scope or network IO | Shared unit tests and all three SDK journeys | PASS |
| Scenario 3 / REQ-124 / REQ-125: native inputs converge on canonical tall Drift rows; invalid values fail before admission | Shared Rust owns `FeatureName`/`FeatureValue` validation and row projection; Python validates mapping keys and supports mapping/dataclass/Pydantic; TypeScript uses one strict serializer | Recorded Rust/Python boundary tests and real journeys; `mise run ts:test:unit` rerun at candidate: 20/20 | PASS |
| R2 `FIND-TASK-002-4`: TypeScript must not discard an own symbol-keyed property | `strictJson` rejects `Object.getOwnPropertySymbols(node)` before traversing entries (`sdks/wyrd-sdk-ts/wyrd/src/index.ts:1121-1129`) | Root and nested symbol-key cases exercise Drift, Eval, and record and assert zero native calls (`tests/unit/observe.test.ts:134-181`); 20/20 unit tests passed | PASS |
| Scenario 4 / REQ-129 / AC-026: Eval session/media, explicit or active trace identity, fixed-width persistence, and negative refusal in every SDK | Existing canonical `EvalRecordObservation` projection remains shared; the Rust, Python, and TypeScript journeys supply session/media, read exact trace/span bytes, and leave only accepted rows | `observe_run.rs`, `test_observe_journey.py`, and `observe-run.test.ts` each cover invalid trace pair and malformed media; TypeScript uses a real active OTel span across N-API | PASS |
| Scenario 5 / REQ-132: generic fixed-size-binary lowercase-hex decode with exact widths | `wyrd-queue/src/batch_builder.rs` handles `FixedSizeBinary` generically and rejects uppercase, malformed, prefixed, or wrong-width text | Exact three-test nextest expression rerun at candidate: 3/3 | PASS |
| Scenario 6 / REQ-128 / AC-025: explicit dynamic routing, one cached describe, convergence, stale fingerprint fence, and unknown/denied/reserved refusal | `Observe::record_value` enforces `vala.datasets`; `Bifrost::writer_table` uses the existing owner-local miss gate and cache; `assert_stale_writer_is_fenced` crosses the real server fence | Every SDK journey writes one dataset twice and observes one describe; shared owner test covers concurrent convergence; `pg_bifrost_e2e` proves stale flush/register refusal and no stale row | PASS |
| Scenario 7 / AC-017 and AC-020 task slice: every first-class SDK crosses client, queue/IPC, Gate, Scribe, shutdown, and query readback | Shared Rust owns lifecycle/projection; Python/TypeScript add only their runtime-earned conversion and tracing boundaries | Three gated journeys, `verify:bifrost`, `test:shared`, SDK/language integration and type lanes recorded | PASS |
| REQ-126: no observation waits for a verdict or performs per-observation flush; shutdown is the durability barrier | Drift/Eval are synchronous project-and-enqueue calls; generic record awaits only a cache miss describe; shutdown drains all producers | Source inspection and lifecycle/SDK journeys | PASS |
| REQ-145 / INV-007 task slice: existing permission, Card-scope, tenancy, and transactional audit owners remain authoritative | No new auth model or admission path; describes and inserts retain existing authenticated server paths | Real denied-describe test proves stable denial, one audit row, no cached table, and zero producers | PASS |
| R2 `FIND-TASK-002-10`: every added/materially changed Rust item has intent-bearing rustdoc without suppression | The cumulative Rust declarations, including `DomainTable` associated items and test-local items, now carry item/field docs and required `# Errors`/`# Panics` sections; the two Oracle follow-up test edits are also documented | Recorded workspace lints are clean; source audit found no remaining item from the prior finding | PASS |
| R2 `FIND-TASK-002-13`: all SDK Eval journeys prove the full accepted and rejected observation boundary | The three existing journey files were extended rather than introducing another record, queue, or harness | Exact focused journey commands and broad language integration lanes are recorded as passing | PASS |
| R2 `FIND-TASK-002-14`: real SDK boundaries prove fixed-table refusal, cached reuse, and stale writer refusal | Existing `WyrdTestServer` exposes narrow test-only describe count/fault hooks; all languages reuse them; the shared Rust e2e owns the stale-fingerprint proof | Three SDK journeys plus `unified_client_registers_writes_swaps_and_reads_both_tables` are recorded as passing | PASS |
| Non-goals: no second serializer, dependency, queue, cache, producer pool, transport, schema authority, lifecycle state, authorization model, run registry, durable observation type, retention policy, verdict wait, or atomic multi-row API | Existing owners and types are reused; the TypeScript symbol fix stays in `strictJson`; owner-level concurrency proof is not copied into every language | Complete cumulative diff inspection | PASS |
| Verification-forced adjacent fixes preserve their owners and do not broaden product behavior | SQL clock fixes remain in `vala-sql`; harness fixes remain test support; the R3 Oracle commits change only timing assertions in existing tests | Recorded focused SQL/Oracle tests and earlier broad Bifrost lanes | PASS |
| Repository provenance constraint: never add AI co-author trailers (`AGENTS.md` §13) | 47 of the 50 commits in the reviewed range contain `Co-Authored-By: Claude Opus 5 ...`; examples include the four R2 remediation commits `2fb00f19`, `50f38108`, `a87f0e8b`, and `b9ea02a4` | `git log --format='%H%n%B' c8bb490a..04f73570` and a case-insensitive trailer count return 47 | **FAIL — `TASKREV-R3-001`** |

## Prior-finding closure

| Stable finding | R3 status | Closure evidence |
|---|---|---|
| `FIND-TASK-002-1` | CLOSED | Configured Rust startup and public `QueueConfig` remain present and exercised. |
| `FIND-TASK-002-2` | CLOSED | Fixed-schema comparison remains exact for count, order, type, and nullability. |
| `FIND-TASK-002-3` | CLOSED | Python rejects coercible non-string mapping/dataclass keys before serialization. |
| `FIND-TASK-002-4` | CLOSED | The one TypeScript serializer now rejects root and nested own symbol keys before native admission. |
| `FIND-TASK-002-5` | CLOSED | Python and Node capture their own active spans with explicit identity precedence. |
| `FIND-TASK-002-6` | CLOSED | The existing Bifrost owner serializes cache misses, rechecks, and describes once. |
| `FIND-TASK-002-7` | CLOSED | Successful shutdown is terminal across never-started and in-flight-start states. |
| `FIND-TASK-002-8` | CLOSED | Ambiguous shutdown retries the same writer and batch on the same state. |
| `FIND-TASK-002-9` | CLOSED | PostgreSQL lock timeout propagates as an honest transaction error. |
| `FIND-TASK-002-10` | CLOSED | The cumulative Rust item audit added the missing associated/test-item documentation without suppression. |
| `FIND-TASK-002-11` | CLOSED | Imports remain module-scoped and signatures use imported bare types. |
| `FIND-TASK-002-12` | CLOSED | Real unknown and denied describes fail before admission with canonical audit proof. |
| `FIND-TASK-002-13` | CLOSED | All three Eval journeys now persist session/media and exact trace/span identity and prove both required refusals add no row. |
| `FIND-TASK-002-14` | CLOSED | All three SDK journeys cover both fixed-table failures and cached reuse; the shared real-server path proves stale-fingerprint fencing. |

## Proposed findings

### `TASKREV-R3-001` — AI co-author trailers violate the required commit identity policy

- **Classification:** VIOLATION
- **Violated obligation:** `AGENTS.md` §13 says “Never add AI co-author trailers.”
- **Exact location:** Git commit messages for 47 commits in
  `c8bb490ad814c0c7770cac33ed7779897ff776e4..04f73570397d5123eb767abafa60d016c37de1db`;
  for example `git show -s --format=%B 2fb00f19` ends with
  `Co-Authored-By: Claude Opus 5 (1M context) <noreply@anthropic.com>`.
- **Evidence:** The configured author and committer are correctly
  `Thorrester <sjforrester32@gmail.com>`, but an exact case-insensitive scan of
  commit bodies finds 47 AI co-author trailers. This is independent of source
  correctness and remains present in the immutable candidate.
- **Observable consequence:** Accepting this candidate would preserve commit
  attribution explicitly prohibited by the repository's provenance policy, so
  the cumulative task cannot pass its hard repository constraints.
- **Required testable correction:** Rebuild only the unpushed task range with
  the same commit ordering, authors/committers, messages, and trees while
  removing every AI `Co-Authored-By` trailer; do not change source or squash
  task history merely to hide the trailers. Prove the replacement range has
  zero matching trailers and that its final tree is identical to
  `04f73570397d5123eb767abafa60d016c37de1db`.

## Verification notes

The task records passing full capability closure on the code-bearing R2
candidate: `verify:bifrost` 9/9, `test:shared` 689, `test:wyrd-sdk`, all Python
and TypeScript unit/integration/type/N-API lanes, code generation, client/PyO3
checks, formatting, lints, and focused Postgres journeys. The later Oracle
changes touch tests only; their two exact focused tests and Clippy passed, while
the full aggregate was not rerun after the last timing-only test edit.

This reviewer additionally ran on `04f73570`:

- `mise run ts:test:unit`: 20 passed, including the 8-case observe suite;
- the exact three-test `wyrd-queue` fixed-binary nextest expression: 3 passed;
- `git diff --check c8bb490a..04f73570`: clean.

The real Postgres journeys and broad aggregates were not rerun in this Wave 1
review. Their recorded commands and results were inspected and match the source.

## Overall result

**FAIL**

The cumulative implementation closes all fourteen prior behavioral and
repository-source findings and satisfies TASK-002's scoped observation
contract, but the immutable candidate violates the explicit commit provenance
rule in 47 commit messages. This is a bounded history-only correction and does
not require a specification revision.
