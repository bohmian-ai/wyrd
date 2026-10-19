---
id: TASK-001-R1
kind: remediation
status: implemented
spec: SPEC-long-horizon-verification
spec_revision: 13
requirements: [REQ-001, REQ-002, REQ-003, REQ-004, REQ-005, REQ-006, REQ-009, REQ-011, REQ-013, AC-001, AC-002, AC-003, AC-004, AC-006, AC-008, INV-003, INV-004]
parent_task: TASK-001
remediates: [FIND-TASK-001-1, FIND-TASK-001-2, FIND-TASK-001-3, FIND-TASK-001-4, FIND-TASK-001-5, FIND-TASK-001-6, FIND-TASK-001-7, FIND-TASK-001-8]
---

# Close grouped verification selection, authority and acceptance gaps

Execute with `$wyrd-implement`. This is one bounded remediation of the original task, not a new design plan. Make the smallest correction at the existing owners identified below. Follow AGENTS and its applicable authority/testing rules, including Red–Green–Refactor and diagnosis with tracing before explaining a failing test.

## Subject and approved outcome

- Repository: `/home/thorrester/Documents/GitHub/wyrd-worktrees/long-horizon-verification-task-001`.
- Original task: `changes/active/long-horizon-verification/tasks/TASK-001-grouped-verification.md`.
- Approved specification: `changes/active/long-horizon-verification/spec.md`, revision **13 requirements** selected by the user. The checked-in header is draft revision 14; history records approval of 13 and unchanged requirements in 14. Do not treat the draft schema-reset exception as newly approved or introduce retained-upgrade scope.
- Review base: `ef3a57b0776dd7bc451273b9176498fcca700550`.
- Reviewed candidate: `e4b1e313aacab50d7eb0f0a2060b8547bf19a632`.
- Verdict and independent evidence: `verdict.md` and `findings-validation.md` in this directory. Their eight FIND IDs are the remediation authority; discovery IDs are reconciled in validation.

Deliver retained, authorized, bounded source selection that eventually reaches the existing serial execution owner, safe timing, accurate generated contracts, aligned authority/docs and the omitted explicit acceptance proof. All corrections remain within approved behavior; no new material decision is required.

## Diagnoses and selected correction boundaries

### FIND-TASK-001-1 — INCORRECT: discovery loses retained uncovered work

REQ-003–005 and AC-002/003/006 require arrivals during a review and late-visible in-finality rows to reach a later whole-group snapshot. `crates/wyrd/wyrd-server/src/verification/groups.rs:217–240` instead uses review result `ended_at` as a coverage watermark. A source timed before a slow review finishes can be absent from that review yet excluded forever after its publication. Queue `GroupState::covers` (`wyrd-sql/src/queries/verifier_executions.rs:985–994`) would notice increased membership, but discovery prevents it from running. The `idle+1h` filter also permanently drops retained unjudged groups after an outage. The repeated earliest-64 page can remain occupied by covered/post-finality/unprocessable groups and starve later work. Ordinary later-timed arrivals and short restart tests do not exercise these failures.

Correct the existing GroupPoller discovery boundary with the simplest bound (user decision, 2026-10-17). Delete the completion-time watermark and the `idle + 1h` slack. Use the existing queue coverage (`GroupState::covers`) and the outstanding fence as the only coverage authority. Bound discovery by the binding's own pinned `finality`: only a `group_id` whose first source row is still within finality is a candidate, because nothing past finality can open a group execution. Order candidates newest activity first. Preserve fixed `next_run_at` cadence. No scan cursor, per-group lifecycle/membership store or new producer state is added.

Ceiling, accepted by the user: a binding with more open in-finality groups than one discovery page examines only the newest-active page per poll. This is marked with a `ponytail:` comment, and a continuing cursor is the recorded upgrade path.

Closure must demonstrate two things: an outstanding group execution with uncovered source executions recorded before it completed, and a deliberately late null→actual result. Each must be judged by exactly one next full immutable snapshot, with retained old history, bounded reads and no long sleep.

### FIND-TASK-001-2 — INCORRECT: source bounds are enforced after the wrong selection

REQ-009 counts distinct eligible executions per source reference and requires an explicit completed inconclusive overflow result before payload collection. At `groups.rs:159–168,263–273`, more than 100000 physical identity rows are skipped before deduplication, refused removal, finality and the real ceiling. A large logical group therefore has no required result; physical overlap can also suppress an allowed group. At `:293–308,376–438`, readable IDs are selected, but both payload SQL statements still query all tuples. With ceiling 20, tuple A=21 and tuple B=1, B triggers reads that fetch A's payloads before Rust filters them. The runner's later inconclusive result cannot undo the read. Independently pinned query cuts can add qualifying rows between identity and payload reads; an unordered broad LIMIT can then omit a selected payload and turn that omission into ordinary null. This last path is source-validated, not an executed reproduction; shared transport-terminal validation works correctly.

Correct the existing analytical selection owner: bound/deduplicate eligible logical identities per tuple before collecting payloads, and reuse the existing overflow marker/runner reason instead of a permanent physical-row skip. Carry the pinned readable membership into both input and outcome SQL selections while retaining tenant/owner/exact tuple/group/finality predicates. Reuse queue `selected_results` and `refused_submissions` (`verifier_executions.rs:1699–1745`) for authoritative queued outcomes/exclusion. Complete selected coverage must be established; an unresolved physical cap defers/retries rather than pins a prefix or false absence. Preserve genuine missing-value nulls and input-time/ID ordering. No new limit, budget, query consumer, evaluator guard or transport implementation is needed.

Closure must cover mixed over/under-ceiling tuples without fetching the over-limit payloads, physical duplicates versus logical membership, a group crossing the old raw boundary receiving its explicit reason without payload collection, and incomplete selected SQL coverage refusing enqueue then retrying. Inspect generated query/result membership at the existing seam; do not generate host load.

### FIND-TASK-001-3 — INCORRECT: accepted duration panics shared scheduling

`TriggerDuration::parse` at `crates/wyrd-spec/src/card/trigger.rs:94–115` accepts `999999999d`. It fits TimeDelta but exceeds chrono DateTime range. `groups.rs:218–220,636–644` uses panicking DateTime subtraction/addition. Locked chrono 0.4.45 checks then expects these operators. A valid tenant Trigger can therefore repeatedly stop the shared scheduler; runtime recovery reencounters the same due binding. This does not imply whole-server death.

Make the existing timing boundary total over its accepted domain using elapsed comparisons and installed checked timestamp arithmetic where an instant is necessary. Preserve long-duration semantics, including boundaries outside representable timestamps. Do not add an arbitrary authored maximum, shorten duration or add a timer/runtime. Removing finding 1's horizon does not resolve the remaining Ended arithmetic.

Closure: maximum accepted s/m/h/d values for idle/max_age/finality and exact ordinary boundaries do not panic and yield the mathematically correct eligibility. A focused registration/scheduler seam must show a neighboring ordinary binding still progresses. Pure timing proof plus this seam is sufficient.

### FIND-TASK-001-4 — INCORRECT: generated source input object cannot decode

REQ-002/011 require one wire shape. `wyrd-spec/src/card/verifier/mod.rs:128–176` and `reference.rs:183–185` decode SourceText as String, but `sdks/wyrd-sdk-ts/wyrd/src/card-types.ts:2364,2429–2431` also exports `{type: "source"}`. Typed authoring accepts an object that registration cannot deserialize. Green codegen proves reproduction, not parity.

Correct InputSlot's source-owned schema projection to exactly the four typed object variants plus the source-reference string; regenerate through the existing schema/TS pipeline and keep served projection coherent. Preserve runtime grammar and validation. Do not hand-edit generated output, introduce a TS-only validator or accept a compatibility object.

Closure: schema/runtime parity accepts a valid source string and refuses the phantom object; regenerated TS typing rejects that object. Verify generated output and the served schema when affected.

### FIND-TASK-001-5 — VIOLATION: grouped reads widen accepted owner permissions

REQ-011 (`spec.md:524–528`), INV-003 and security posture require existing sensitive-read permissions; SYSTEM adds no authority. Scheduler recovers owner grants, but `groups.rs:130–150` replaces them with SystemReadAuthority grants over execution_inputs/results. `authority.rs:178–189` only changes attribution in `serving`, not permissions. All statements then run through an unauthenticated scheduled caller. A runtime-active owner without query capability or with scope excluding the source table still has protected values snapshotted and passed to a judge. Correct tuple isolation does not confer read permission; later runner authority cannot undo the read.

Remove this grouped caller's manufactured grant. Reuse `Caller::accepted` from recovered ExecutionAuthority and `ScheduledQueryCaller::authenticated` for discovery, identities and payloads. Existing admission/resolved-table authorization/non-blocking denial audit own the trust boundary. An extra permission check followed by the privileged read is insufficient. Preserve Drift Reader::authority and its expressly approved observation-table exception, as well as terminal-safe settlement, cursor retry and producer no-IO stamping.

Closure: independently omit query capability and scope reads away from a verification source table while keeping owner active/invocation-capable. No protected payload reaches snapshot/judge; ordinary denial audit is observable. Grant existing required access and show a later poll succeeds for input and outcome sources. Retain existing Drift authority proof. Live-tool denial is a different boundary.

### FIND-TASK-001-6 — VIOLATION: required rustdoc is missing

AGENTS §16 explicitly blocks missing rustdoc, including private fields/modules and Errors on fallible methods. Complete the new duration fmt/deserialize Errors contracts (`card/trigger.rs:137–155`), SourceText tuple field (`card/verifier/reference.rs:185`), new groups test module (`groups.rs:804–805`) and Forward sender field (`sdks/wyrd-sdk-rust/tests/integration/support/mod.rs:499`). Explain writer failure, non-string/invalid-duration decoding, and field/module roles. Inspect materially changed remediation items for the same rule. Batch this work here; do not create a checker, unrelated cleanup or separate style round. Source inspection and applicable existing checks suffice; no behavioral test is required for prose.

### FIND-TASK-001-7 — VIOLATION: active protocol contradicts approved tool behavior

`architecture/wyrd-design.md:1200,1405–1406` and `architecture/references/domain/evaluation.md:55–68` still prohibit all check-Agent tools. Validator/runtime and public docs permit the approved bifrost.query exception. This conflicts with the active protocol's synchronization rule and can cause a future conforming implementation to reject J-003.

Update those descriptions to exactly the approved explicitly declared bifrost.query exception: ordinary tool/query/payload permissions, no workspace or mutation tools, live span data outside the pinned snapshot. Preserve refusal of other tools. This is synchronization of an already approved decision, not a new capability or spec revision. Compare prose with existing validator/hosted execution and focused live/refusal proof.

### FIND-TASK-001-8 — MISSING: explicit acceptance cases are absent

The existing Rust/Python/TS grouped stories and MCP story cover the main paths, but TASK-001 acceptance and AC-001/003/004/006/008 explicitly require additional variants. Sources are Rust `verify_in_real_time.rs:1276,1660,1998,2288`, Python `test_verify_in_real_time.py:467–945`, TS `verify-in-real-time.test.ts:525–1085`, and MCP `verification.rs:1325–1539`.

Add the following missing proof to those existing real-server/registration/agent owners:

- Typed and source `inputs.group_id` registration refusal and renamed-slot success through the three SDK registration paths and HTTP/MCP enforcement, including no invalid persistence/activation.
- Clean non-trace coding with no implicit query; spans made visible after source inputs are pinned; tool-invoke and query-read denied independently.
- Same-request and registered external reference success, participating binding ambiguity and missing declared target refusals.
- Agent HTTP/MCP media failure/zero/ceiling/isolation negatives; the current MCP addition is only a JSON support story.
- Deliberately failed/truncated discovery/snapshot reads: no partial enqueue, then successful retry using the existing query fault/terminal facilities.

Reuse current real server, local provider, transfer and query-fault facilities. Do not create a general harness, new restart control API or duplicate terminal parser. Rust's existing worker-restart test is credible shared-runtime proof; missing Python/TS restart controls alone requires no new capability. Late/absent-result, overlap, outstanding/coalescing and source-permission cases are already owned by findings 1/2/5 and must not become duplicate mandates. Static producer tracing credibly proves no-new-IO; no monitoring subsystem is requested.

## Preserved behavior, constraints and non-goals

Keep ordinary verify/submit and nullable owner invocation; exact tenant/owner/subject/Verifier/group isolation; selected authoritative queued results; deterministic list alignment and genuine nulls; fixed Trigger cadence; finality as a background bound without producer refusal/deletion; one outstanding execution; immutable snapshots/retries/history; first-result baseline and verdict-change dispatch; ordinary on_failure and Window/Drift. Preserve TenantConn/RLS, caller-owned transactions, lease fences, one ScribeOutbox and its accepted crash loss, ordinary query admission and non-blocking audit.

No new producer Postgres IO, group membership/readiness/revision store, queue, transport, source syntax, compatibility alias, retention policy, span snapshot, media codec, internal fan-out, budget, dependency or platform-specific mechanism. Do not implement TASK-002 schedule inputs, TASK-003 outbox pressure/progress or capacity qualification. Do not weaken tests/gates. Reversible private helper/query/layout choices remain implementation-owned.

## Acceptance and focused verification

| Finding | Required closure |
|---|---|
| 1 | Uncovered in-finality source executions and late results are judged serially; discovery is bounded by finality and newest activity, with the accepted one-page ceiling marked |
| 2 | Logical distinct ceiling governs before payload reads; overflow is explicit; selected payload coverage is complete or retryable, never truncated |
| 3 | Every accepted duration is safe; ordinary neighboring scheduling remains available |
| 4 | Runtime, source schema, served contract and generated TS accept the same InputSlot forms |
| 5 | All group source reads use accepted owner grants and ordinary denial audit; Drift exception stays intact |
| 6 | Listed omissions and materially changed remediation Rust items satisfy mandatory rustdoc |
| 7 | Active design/evaluation prose matches only the approved narrow read-tool exception |
| 8 | Listed explicit missing cases have credible focused real-surface proof with nonzero selection |

Use Red–Green–Refactor for each changed behavior. Use pure timing/schema tests and narrow query/SQL seams where they directly prove the gap, plus focused real-server variants where cross-boundary state/authorization requires them. Select and record exact test names/targets before execution; use `mise exec -- cargo nextest run --locked -p <owner> --lib|--test <target> -E 'test(=<actual name>)'` for newly named tests, with the owning Postgres wrapper when needed. Do not substitute an empty selector, skipped case or aggregate for the required proof.

For modified existing journeys, run only their exact selectors. Examples (choose the cases actually changed, not every story):

```bash
mise exec -- scripts/postgres/with-test-postgres.sh -- bash -lc 'mise run db:migrate:all:inner && mise exec -- cargo nextest run --locked -p wyrd-ai --test integration -P journey --run-ignored=all -E "test(=verify_in_real_time::grouped_conversations_recover)"'
mise exec -- scripts/postgres/with-test-postgres.sh -- bash -lc 'mise run db:migrate:all:inner && mise exec -- cargo nextest run --locked -p wyrd-ai --test integration -P journey --run-ignored=all -E "test(=verify_in_real_time::grouped_coding_uses_declared_sources)"'
mise exec -- scripts/postgres/with-test-postgres.sh -- bash -lc 'mise run db:migrate:all:inner && cd sdks/wyrd-sdk-python && mise exec -- uv run python -m pytest -q -m integration tests/integration/test_verify_in_real_time.py::test_grouped_support_resumes'
mise exec -- scripts/postgres/with-test-postgres.sh -- bash -lc 'mise run db:migrate:all:inner && cd sdks/wyrd-sdk-ts/wyrd && mise exec -- pnpm exec vitest run tests/integration/verify-in-real-time.test.ts -t "^grouped support resumes$"'
mise exec -- scripts/postgres/with-test-postgres.sh -- bash -lc 'mise run db:migrate:all:inner && mise exec -- cargo nextest run --locked -p wyrd-mcp --test mcp -P journey --run-ignored=all -E "test(=verification::pg_tests::grouped_verification_discover_act_observe)"'
mise exec -- scripts/postgres/with-test-postgres.sh -- bash -lc 'mise run db:migrate:all:inner && mise exec -- cargo nextest run --locked -p wyrd-server --features test-support --test integration -E "test(=pg_openapi_contract::grouped_verification_contract_is_served)"'
```

Build `mise run build:monty-worker` as needed; rebuild changed Python/TS native test packages with `mise run py:setup` / `mise run ts:build:testing` before their focused runtime cases. Cargo-backed lanes run sequentially. Re-run failures with WYRD_LOG before diagnosis; use no synthetic host load.

Run `mise run fmt` and `mise run lints` for changed Rust. For source schema/generated changes, regenerate through the owning pipeline and run `mise run codegen:check`, `mise run ts:typecheck`, and `mise run ts:napi:check` only if N-API declarations change. For Python test/package changes run `mise run py:format` and `mise run py:lints`; add `mise run py:typecheck` for typing/stub/export changes. For TS source/test changes run `mise run ts:format` and `mise run ts:lints`. Run `mise run check:tenant-isolation` for changed SQL/query paths; `mise run check:deps` only if dependency/boundary composition changes; `mise run docs:check` if docs-site files change. These are write-set checks, not a request for a broad aggregate.

Do not require full user-journey suites, `mise run gate`, capacity runs or all language suites for this remediation. Run exact focused closure and only the narrowest lanes covering its actual write set; integrated change review owns broader journey verification. Record diagnosis, RED/GREEN/REFACTOR, exact commands, features, selected counts and finding-to-evidence mapping. Return to cumulative review against the original task after implementation. Completion requires that review's PASS; this task grants no merge or deployment authorization.

## Implementation evidence

Candidate range `e4b1e313a..HEAD` on `change/long-horizon-verification-task-001`. Journey selectors run under `mise exec -- scripts/postgres/with-test-postgres.sh -- bash -lc 'mise run db:migrate:all:inner && …'`; each selected a nonzero count and passed.

| Finding | Implementation evidence | Verification evidence | Result |
|---|---|---|---|
| 1 | `26a3ee94e` `groups.rs` discovery: no completion watermark or `idle+1h` slack; candidates are groups whose first row is within the binding's `finality`, newest activity first; `GroupState::covers` and the outstanding fence are the only coverage authority; one-page ceiling marked `ponytail:` | `wyrd-ai --test integration -P journey --run-ignored=all -E 'test(=verify_in_real_time::grouped_reviews_cover_late_work)'` (late answers recorded during an outstanding group execution and a late null→actual result each reach exactly one next snapshot); `grouped_conversations_recover`, `grouped_support_resumes` | PASS |
| 2 | `26a3ee94e`, `3662a1d9d` `groups.rs`: per-tuple distinct logical counts before payload reads, over-ceiling tuples take the existing `source_execution_limit_exceeded` reason without payload reads, both payload statements select only the listed membership, incomplete coverage defers (`complete()`) | `wyrd-server --lib -E 'test(/^verification::groups::tests::/)'` 5 passed (`over_ceiling_counts_join_the_group_end`, `reads_select_only_listed_membership`, `groups_end_count_and_order_by_their_rows`, `values_unwrap_inputs_and_outcomes`); `grouped_sampled_frames_are_bounded` (21 frames → limit reason) | PASS |
| 3 | `26a3ee94e` `groups.rs` timing: elapsed comparisons and checked timestamp arithmetic, no panicking `DateTime ± TimeDelta` | `wyrd-server --lib -E 'test(=verification::groups::tests::largest_accepted_durations_judge_without_overflow)'`; neighbouring progress beside a `999999999d` binding (`forever.yaml`) in `grouped_reviews_cover_late_work` | PASS |
| 4 | `cd8f0fb8d` `wyrd-spec/src/card/verifier/mod.rs` InputSlot schema = four typed objects or a source string; TS regenerated via codegen; `card-types.test-d.ts` rejects `{type: "source"}` | `wyrd-spec --lib -E 'test(=card::verifier::tests::input_slot_schema_matches_decoding)'`; `wyrd-spec --features server --lib -E 'test(=card::verifier::tests::served_input_slot_schema_matches_decoding)'`; `wyrd-server --features test-support --test integration --run-ignored=all -E 'test(=pg_openapi_contract::grouped_verification_contract_is_served)'`; `mise run codegen:check`; `mise run ts:typecheck` | PASS |
| 5 | `26a3ee94e` `groups.rs`/`scheduler.rs`: manufactured SystemReadAuthority grants removed; discovery, identities and payloads use `Caller::accepted` + `ScheduledQueryCaller::authenticated` from the recovered owner authority; Drift `Reader::authority` untouched | `grouped_reads_need_the_owner_grant`: without query read → no group execution and a denied `bifrost_query:read` audit row; with query read scoped to `vala.traces` → no group execution; with `workload` restored → the next poll judges all turns; judge receives no request | PASS |
| 6 | `eef2efb45` rustdoc for duration codec `# Errors`, `SourceText` field, groups test module, Forward sender field; every new remediation item documented | `mise run lints`; source inspection | PASS |
| 7 | `3d257ee6a` `architecture/wyrd-design.md`, `architecture/references/domain/evaluation.md`: only an explicitly declared `bifrost.query`, ordinary permissions, no workspace or mutation tools, live spans outside the snapshot | Compared with the validator and `grouped_coding_uses_declared_sources` (refuses other tools) | PASS |
| 8a | Reserved `inputs.group_id` (typed and source) refused, renamed slot succeeds; `7e23a6761` server `validate_request` now runs `VerifierSpec::validate` | Rust `grouped_coding_uses_declared_sources` (SDK refusal `WYRD_LOADER_400_INVALID_ENVELOPE`, `resolve_latest` → `WYRD_REGISTRY_404_CARD_NOT_FOUND`, HTTP `POST /v1/cards` → 400 `WYRD_SPEC_400_VALIDATION`); Python `test_grouped_coding_uses_declared_sources`; TS `grouped coding uses declared sources` | PASS |
| 8b | Coding: non-trace group execution queries nothing; spans after pinning; tool-invoke and query-read each denied | `grouped_reads_need_the_owner_grant` (judge receives no request); `grouped_reviews_cover_late_work`; `grouped_coding_uses_declared_sources` loop (`lacks Tool/Invoke`, `WYRD_VALA_403_QUERY_FORBIDDEN`) | PASS |
| 8c | Same-request siblings, registered external reference (`coding-external.yaml`), ambiguity (`support-ambiguous.yaml`), missing declared target (`support-undeclared.yaml`) | Rust/Python/TS coding journeys above; `bda7242d5` Python/TS | PASS |
| 8d | `c7475a5be` MCP `grouped_frames_over_mcp_are_bounded`: frames planned via `verification.evidence`, PUT over HTTP, enqueued with `group_id`; zero frames → `empty_selection`; failed frame kept; another Service's frame excluded; reader refused RBAC; 21 frames → limit reason | `wyrd-mcp --test mcp -P journey --run-ignored=all -E 'test(=verification::pg_tests::grouped_frames_over_mcp_are_bounded) \| test(=verification::pg_tests::grouped_verification_discover_act_observe)'` 2 passed | PASS |
| 8e | Failed discovery enqueues nothing, then a later poll succeeds; incomplete coverage defers | `grouped_reads_need_the_owner_grant`; finding 2 unit tests | PASS |

Python: `pytest -q -m integration test_verify_in_real_time.py::test_grouped_support_resumes …::test_grouped_coding_uses_declared_sources` 2 passed. TS: `vitest run verify-in-real-time.test.ts -t "^grouped (support resumes|coding uses declared sources)$"` 2 passed. Rust journeys: all seven `verify_in_real_time::grouped_*` selected and passed.

Lanes: `fmt`, `lints`, `codegen:check`, `check:tenant-isolation`, `check:deps` (sha2 dev-dependency added to wyrd-mcp), `py:format`, `py:lints`, `ts:format`, `ts:lints`, `ts:typecheck`; `git diff --check` clean.

Diagnoses:
- **Query-denied variant showed no denial / showed `Tool/Invoke`.** Cause: the default `workload` role grants query read *and* gateway invoke, so revoking it stopped the check Agent before it reached the judge. Fix site: the test grant sets (a verification-schema-scoped query role that keeps gateway invoke); no production change.
- **HTTP registration accepted a reserved `group_id` slot (201).** Cause: the server did not run `VerifierSpec::validate`; only the loader did. Fix site: `wyrd-server/src/components/cards/service.rs::validate_request`, the single path every registration route goes through.
- **MCP registration.** MCP has no Card registration tool, so registration is enforced over HTTP; MCP proof covers grouped execution and media.

Non-goals stayed out: no TASK-002/003 work, no new store, queue, transport, harness, budget or limit setting. The only dependency added is the dev-only `sha2`.
- **Loader-only artifact path rule.** The server is the authority for every registration rule. Comparing the loader's rules with `validate_request` found one more that only the loader enforced: a manifest that repeats a path got a 503 from the database unique constraint, and a nested pair (`weights` beside `weights/a.bin`) was accepted. Fix site: `wyrd_spec::registry::validate_artifact_manifest`. `validate_request` and the loader both call it, and the refusal is `WYRD_REGISTRY_400_INVALID_ARTIFACT_PATH`. Evidence: `wyrd-server --features test-support --test integration -E 'test(=pg_card_registration_route::conflicting_artifact_paths_are_refused)'` (red 503 → green), `wyrd-spec --lib -E 'test(=registry::submission::tests::validate_artifact_manifest_refuses_repeated_and_nested_paths)'`, the `wyrd-loader` suite (38 passed), and the `pg_card_registration_route` module (32 passed). Every other loader rule is already enforced on the server: duplicate identity, unresolved paths, unresolved references, spec bindings and Verifier validation. The server overwrites an authored reference `uid` itself.
