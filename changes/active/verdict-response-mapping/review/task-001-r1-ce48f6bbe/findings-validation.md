# TASK-001 R1 independent remediation validation

Recommended verdict: **PASS**. Findings 1, 2, 3, 4 and 7 are closed. Findings 5 and 6 are waived by the maintainer's current instruction. No new blocking finding was identified.

## Immutable subject and authority

- Repository: `/home/thorrester/Documents/GitHub/wyrd`.
- Approved specification/original-task base: `08998b9e9b6f04825f7743b877391247e0ad9621`, SPEC-verdict-response-mapping revision 1.
- Prior reviewed candidate: `9a0b9112523252328813041c0b11be375f0eed29`.
- Remediation candidate: `ce48f6bbe367dace64c25d3324592ae5d5406636`.
- Reviewed remediation diff: `git diff 9a0b91125 ce48f6bbe`; cumulative `08998b9e9..ce48f6bbe` retained as context. Ancestry is continuous, with no rebase correspondence issue.
- Source was inspected in the clean detached checkout `/tmp/wyrd-task-review-ce48f6bbe`. HEAD and clean tracked source were checked before and after inspection. The shared checkout's unrelated source was not substituted for the candidate. The detached tree has no `.codegraph/`; direct reads/diffs follow the repository rule, and no index was created.

The original task, original validated findings/verdict, all selected original specialist reports, and TASK-001-R1 remediation packet were read from the committed candidate. Applicable root/SDK AGENTS, spec-driven workflow, TESTING, maintainer/Rust/error/agent-surface guides, architecture authority map, and Prompt/Verifier/in-memory authoring, recording, Bifrost storage/Variant and security trust/tenant sections were consulted. Those authorities are unchanged by remediation.

The current maintainer explicitly forbids benchmarks and states that profiling and L1 findings are waived. That instruction overrides older packet/performance text for this review. Neither performance measurements nor benchmark execution/comparison is required for acceptance, and neither was run. The historical committed reports' old blocker counts and benchmark prescriptions remain historical review records rather than live obligations. The task's phrase “Open for change review” does not override the current prohibition or automatically authorize a future run.

## Independent review coverage

This is the fresh remediation reviewer required by `wyrd-task-review`. Scope was prior-finding closure, actual changed code/assertions, affected callers and consumers, and validity of carried-forward evidence. No new mechanism for storage, coordination, recovery, authorization or native-provider transport was added; no extra specialist is needed for another vote. Original source-backed coverage remains usable except where R1 directly changes validation/projection behavior. No validator-only finding requires an additional source validator.

Navigation: Score native answer → existing unit-interval predicate → check-error owner; Pydantic model → native registration → shared `VerdictTransform::authored`; TS Check options → native lifecycle result → existing `projectedError`; nullable AgentCheck decode → direct-Verdict branch; real MCP execute → existing asynchronous visibility helper → details/capture/provenance and foreign read.

## Stable finding dispositions

### FIND-TASK-001-1 — CLOSED

`crates/vala/vala-verify/src/blocks/transform.rs:111–131` now checks present Score confidence and every present probability with the existing `unit` predicate before grading. `unit` at 155 rejects nonfinite and out-of-range values; absent optional fields remain valid. This is a synchronous check at the selected-answer owner, and valid low confidence still cannot change the authored comparison.

Owner test `score_metadata_must_lie_in_the_unit_interval` at 485 exercises absent/valid/low confidence and invalid bounds, NaN and infinity, with the explicit terminal reason. The existing real Rust `gateway_jev::jev_malformed_answers_are_check_errors` adds `SCORE-CONFIDENCE` at `sdks/wyrd-sdk-rust/tests/integration/gateway_jev.rs:1026`; its downstream assertions verify only `compliant` becomes inconclusive, siblings pass, the reason is present and upstream calls remain one per check. Candidate task R1-1 records both exact focused commands and GREEN 1 passed each, with the expected owner RED. The original producer/engine check-error path is unchanged.

### FIND-TASK-001-2 — CLOSED

`sdks/wyrd-sdk-python/python/wyrd/verify/_transforms.py:34–44` adds an inherited Pydantic after-validator which calls the registered native helper. `src/verify/definitions.rs:63–84` routes it through the same `VerdictTransform::authored` used for attachment; `src/lib.rs:62` registers the helper and `_wyrd/__init__.pyi:35` declares it. The shared method at `crates/wyrd-spec/src/card/verifier/transform.rs:268` decodes and calls the existing self-contained validator without independent Python domain rules or IO.

Public Python tests at `tests/unit/cards/test_verdict_transform_authoring.py:137–155` construct reversed, out-of-range and NaN bounds before any attachment, assert exact `WYRD_SPEC_400_VALIDATION`, details/path and remediation, then validate a corrected policy. R1-2 records the construction RED, 21 passing focused tests after `py:setup`, 445 cards tests and 33 public-export tests. Existing optional Pydantic imports and mapping-only authoring remain unchanged; native registration imports no Pydantic.

### FIND-TASK-001-3 — CLOSED

`sdks/wyrd-sdk-ts/native/src/authoring.rs:1370–1387` replaces the prior generic exception path with `NativeLifecycleResult::outcome` carrying the shared authored-policy error. `wyrd/src/definitions.ts:2213–2220` separates the policy from ordinary options and sends it through `lifecycleValue`. Existing `native.ts:104–108` throws the catalog-backed `projectedError`; no new error family or message parsing is introduced. On validation failure, native state is not replaced.

`wyrd/tests/unit/verdict-transform-authoring.test.ts:48–95` asserts WyrdError identity, exact code/status/details and remediation, then successful correction. R1-3 records the former “not an instance of WyrdError” RED and 14 passing tests after the native build. Generated `index.d.ts` and `.d.cts` each expose the native method at 222; recorded `ts:napi:check` and codegen checks are clean. The small `observe.test.ts` export pin update preserves the already-required question exports and is backed by its recorded 38 passing tests.

### FIND-TASK-001-4 — CLOSED

The shared AgentCheck decoder at `crates/wyrd-spec/src/card/verifier/transform.rs:68–73` treats a present null policy as absent; malformed non-null values still decode through the closed union and refuse. The TS native policy method handles null separately using the existing canonical Check field owner. Omission and explicit null now agree with the unchanged generated optional schema/type and use the unchanged direct-Verdict branch.

`agent_check_reads_a_null_policy_as_omitted` at 545 asserts equal omitted/null values and continued malformed non-null refusal. TS `a null verdict_transform is the omitted policy: the check judges directly` at `wyrd/tests/unit/verdict-transform-authoring.test.ts:98` validates both forms with a normal direct-Verdict Prompt. R1-4 records the exact failed-null RED and focused Rust GREEN 1 passed; the TS case is included in the 14 passing focused tests. No new execution semantics are introduced that require a separate new hosted engine path.

### FIND-TASK-001-5 — WAIVED

The maintainer waived profiling/allocation measurement. R1-5 records source analysis of existing captures and new conversions, including the additional Python model-to-JSON validation boundary. No measured allocation, retained-memory or timing claim is made. Missing measurements are not a blocker and must not be restored through a different label or deferred task.

### FIND-TASK-001-6 — WAIVED

The maintainer rejected the L1 benchmark gate and currently forbids benchmarks. No L1 result, VALID verdict or history comparison is required by this review. The task records no qualifying results and discloses an earlier interrupted, unapproved start; this reviewer did not run or request a benchmark. That historical event supplies neither performance proof nor a new acceptance obligation.

### FIND-TASK-001-7 — CLOSED

`crates/wyrd/wyrd-mcp/tests/bifrost/mcp/verification.rs:1104–1325` now boots the real server/runtime and configured Jev gateway, mocks only the external provider, then calls the public MCP execute/details tools. It inspects the actual runtime details schema's closed leaf union; executes full/hash versions; asserts returned and retained failed verdict/exact reason, exact authored policy, full native answer or a 64-hex digest, exact Agent kind/name/version, registered Prompt name/UID and gateway route; and checks the exact catalog refusal identity in the foreign tenant's MCP refusal. It reuses the existing settling/details helpers, so the read follows recorded execution output rather than a disconnected fixture.

R1-7 records exact `wyrd-mcp` target `mcp` nextest selector, repository Postgres setup and GREEN 1 passed. The initial incorrect expectation of absent Prompt provenance was corrected against the existing inline-Prompt registration owner. The original task's false “no MCP details tool” claim is corrected at 370–375. This closes a focused proof gap without adding a tool, result envelope or production path.

## Carried-forward requirements and evidence binding

The final reconciled requirement matrix belongs in `verdict.md`. Original native Prompt/question lowering, actual Jev input binding, Choice mapping, probability arithmetic, Score comparison, local/registered graph checks, direct-Verdict behavior, existing retries, async recording and SDK retained-read/tenant assertions were not materially replaced by R1. Their original independently validated evidence is carried forward; affected policy construction/null/error/metadata claims are now supported by the corrections above. TASK-002 aggregation and TASK-003 Google behavior remain excluded.

R1 task evidence explicitly names runtime source through `519044207`. `git diff 519044207 ce48f6bbe` changes only the task Markdown (96 added lines), so the recorded focused execution is applicable to identical production/test source at the final candidate. The same evidence reports format, Rust/Python/TS lints/types, N-API declarations and codegen checks clean. These are inspected recorded results, not fresh runs by this reviewer. No installed native module was treated as proof of candidate identity.

## Complete added/moved wait inventory

| Boundary | R1 added/moved PostgreSQL, Scribe, object-storage or Vault wait | Cumulative disposition |
| --- | --- | --- |
| Vala Score validation | None; optional metadata checks use only borrowed process memory. | Original invocation wait remains in place; no second provider call. |
| Shared authored-policy validation and null decoder | None; decoding and existing validation are synchronous. | No request/storage owner changes. |
| Python model/native helper and TS policy/lifecycle projection | None; JSON conversion, validation and error projection are synchronous local work. | No clients, pools or runtimes are constructed. |
| MCP change | None in production; the change is a journey test using existing server operations. | Existing recording/details/auth owners unchanged. |
| Original declaration Agent lookup at `wyrd-server/src/components/cards/service.rs:2034` | Unchanged by R1. | One non-hot lookup per distinct referenced Agent UID per Verifier declaration; REQ-011 authoritative resolution, existing TenantConn. |
| Original declaration Prompt lookup at the same file:2052 | Unchanged by R1. | Same non-hot authoritative resolution/deduplication; not an execution-cache miss. |
| Recording, audit, object storage, Vault and durable writes | None added or moved by R1. | Original async Scribe staging and tenant reads preserved; no new durable-handler write. |

The complete cumulative added/moved hot-path wait set remains empty. Benchmark/profiling waivers do not waive these source-verified ownership and no-additional-IO constraints.

## Behavioral acceptance, process history and limits

Behavioral acceptance passes with five findings closed and two waived. The original historical TDD deviations remain as recorded: scenarios 3/5/6 followed behavior introduced by scenario 1, and later GREEN does not create historical RED. No retrospective RED or production churn is required. R1 records behavioral RED for Score metadata, typed construction, TS error identity and null decoding; the MCP change closes evidence rather than introducing new product behavior. Reported export/provenance test failures have concrete owner diagnoses and bounded test corrections. This review does not manufacture absent historical diagnostic traces.

This was the first R1 review, so no two-round convergence assessment is triggered. No implementation changes or new remediation packet are required. No Cargo/native builds, tests, codegen, broad suites, profiling or benchmarks were run by this reviewer. Acceptance relies on source tracing, actual assertion inspection and candidate-bound recorded focused execution results. No empirical performance or real-model-quality assertion is made.
