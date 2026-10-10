---
id: TASK-003-R1
kind: remediation
status: proposed
remediates: [FIND-TASK-003-1, FIND-TASK-003-2, FIND-TASK-003-3, FIND-TASK-003-4, FIND-TASK-003-5, FIND-TASK-003-6, FIND-TASK-003-7]
---

# TASK-003 R1: Close seven validated gaps in the canonical journey

## Subject and authority

- Approved spec: `changes/active/verification-closeout/spec.md` revision 3.
- Original task: `changes/active/verification-closeout/tasks/TASK-003-r4-canonical-support-desk-closeout.md`.
- Review and source evidence: `changes/active/verification-closeout/review/task-003-r4-canonical-closeout/findings-validation.md` and `verdict.md`.
- Original base: `7f79fb3417db651adedac194ada8908f0a0372d7`; reviewed candidate: `f6c841d57fb19517ddefe83c826b24085b853845`. Reassess the full cumulative candidate against that base after this remediation.

The reviewed candidate has seven independently validated gaps. Correct them within the existing owners and approved contracts. This packet routes directly to `$wyrd-implement`; it adds no product or architecture decision.

## Diagnoses and selected correction boundaries

### FIND-TASK-003-1 — Exact deployment identity

The Prompt specifies `open_ai_chat_completion/gpt-4o`; the existing workflow route projects this to gateway `openai/gpt-4o`. Each of `examples/support-desk/{python/support_desk.py,rust/support_desk.rs,typescript/support-desk.ts}` currently accepts any deployment with model name `gpt-4o` and reports a missing name without a configuration action. The three SDK journeys call these examples. A different provider's same-name model makes `deploy` succeed but the later Agent gateway route fail. The provider/model pair is already present in the gateway deployment response and runtime route; compare that exact identity in each example and make the existing refusal name the missing pair and how to configure it. Preserve each language's public workflow, capture policy, and error surface. Do not add provider options, a model registry, or a new deployment resolver.

### FIND-TASK-003-2 — Oracle-owned shutdown memory

`crates/vala/vala-bifrost-redux/src/oracle/admission.rs:737-771` waits for `shared_memory_reserved()==0`, although `resources.rs:1740-1758` defines it as Oracle plus Forge and Scribe follower reservations. `crates/wyrd/wyrd-server/src/state.rs:684-700,1948-2008` drains Oracle before Scribe and aborts remaining owners if Oracle reports residual bytes. A mixed-role pod can therefore consume its process deadline and lose a Scribe drain while Oracle has no work left. Reuse the existing Oracle-holder accounting for Oracle's completion predicate and residual report. Retain the process-wide memory governor, both wakeups enabled before every check, and the wait for delayed Oracle child reservations. Do not redefine sibling memory as Oracle state or silently accept real Oracle residuals.

### FIND-TASK-003-7 — Forge self-reclaim after physical quiescence

`crates/vala/vala-bifrost-redux/src/forge/worker.rs:2148-2191,2427-2470,6140-6177,8319-8350` joins spawned plans on normal loop return, but a worker panic skips that join. Plan and heartbeat Tokio handles detach when their owners drop. The server supervisor at `crates/wyrd/wyrd-server/src/app/supervise.rs:119-180` restarts the worker with the same owner; the startup path then passes that owner to `crates/vala/vala-sql/src/queries/forge_tasks.rs:890-900`, clearing still-unexpired attempts while old work may be live. Exact-attempt SQL transitions and the table lease protect some paths, but an owner match alone cannot prove physical quiescence. Keep the existing worker stop/join ownership as the proof before fast unexpired self-reclaim. When a panic leaves that proof unavailable, retain ordinary lease-expiry and table-fence recovery. Preserve fast reclaim after an orderly joined loop, periodic expired reclaim, exact-attempt checks, and unrelated API availability. Do not add a persistent fence or change the shared supervisor policy.

### FIND-TASK-003-6 — Required late-Scribe proof

`crates/wyrd/wyrd-testing/tests/bifrost/server/query.rs:764-810` starts both roles before it stages the audit. `verification_runtime.rs:71-155` does likewise. Neither test proves the task's explicit empty-roster retry followed by Scribe discovery. Adapt the existing Oracle-only audit journey using the repository's delayed-last cluster start or restart control: hold the Scribe-only pod unbooted, stage the audit on the Oracle-only pod, start Scribe, drain Oracle within the deadline, and read the same decision from `vala.system.audit_log`. This is a test correction only; preserve the current route, retry identity, and accepted abrupt-process-loss window. No new routing layer, delay setting, or fixture framework is needed.

### FIND-TASK-003-5 — Role-idempotency failure propagation

The second `roles.sql` application in `scripts/postgres/test-roles.sh:27-29` now uses container `psql` but omits `ON_ERROR_STOP`, unlike the wrapper bootstrap and neighboring role commands. An intermediate SQL error can leave `psql` green and final assertions satisfied by the first bootstrap. Add `-v ON_ERROR_STOP=1` to this existing rerun, preserving stdin, role passwords, container execution, cleanup, and assertions. The rerun must actually prove that the script succeeds twice.

### FIND-TASK-003-4 — Active design correlation contract

The cumulative candidate authorizes asserted `card_uid` on observation, OTLP, and gateway paths, but `architecture/wyrd-design.md:132,762,773,880-891` and `architecture/references/domain/evaluation.md:13` still direct implementers to use observation `card_ref`. These conflict with the approved UID contract and the same design's updated lines 811-870. Reconcile only the active observation wire, stored-row, and verification-routing passages with Card UID authorization. Keep CardRef where it describes registry identity, principal declaration, or other legitimate Card references. Do not restore a CardRef correlation alias or edit generated contracts to match stale prose.

### FIND-TASK-003-3 — Mandatory item documentation

`crates/shared/wyrd-client/src/bifrost/mod.rs:152` changed its test fixture helper to return `CardUid` without item rustdoc. New `examples/support-desk/rust/main.rs:13` is a fallible binary entry point with neither item rustdoc nor `# Errors`. `AGENTS.md` §16 makes these blocking. Add concise descriptions of the fixture UID and runnable workflow, and name `main`'s failure boundary under `# Errors`. No test, wrapper, or behavior change is needed.

## Acceptance and focused proof

| Finding | Observable closure | Narrow proof |
|---|---|---|
| FIND-TASK-003-1 | All three `deploy` workflows reject a model-name collision under another provider and tell the caller which exact deployment to configure; correct deployment still succeeds | Focused collision and absence checks at each language's existing example/SDK test surface; `mise run check:examples` builds the Rust example only |
| FIND-TASK-003-2 | Oracle shutdown finishes with non-Oracle bytes live, while still waiting for and reporting delayed Oracle-child bytes | Focused Oracle admission/lifecycle test with both states; `mise run fmt` and `mise run lints` for changed Rust |
| FIND-TASK-003-7 | A panicked worker cannot fast-reclaim an in-flight unexpired attempt before old plan and heartbeat work stop; orderly joined restart still fast-reclaims; lease expiry remains available | Focused worker/supervisor restart proof and the existing exact `pg_forge_tasks::previous_owner_reclaims_its_unexpired_attempt` selector through `mise exec --` with its Postgres setup wrapper |
| FIND-TASK-003-6 | An audit staged before Scribe registers is retained after Scribe joins and Oracle drains | Exact `oracle_only_pod_retains_audit_staged_before_its_drain` selector via `mise exec -- cargo nextest run --locked -p wyrd-testing --test server -P journey --run-ignored=all -E 'test(=query::oracle_only_pod_retains_audit_staged_before_its_drain)'`, under `scripts/postgres/with-test-postgres.sh` and the required migration setup |
| FIND-TASK-003-5 | Second role bootstrap fails the lane on SQL error and passes on valid idempotent SQL | `mise run test:postgres:roles` plus a direct failure-on-SQL-error check using its existing contract pattern |
| FIND-TASK-003-4 | Current design and evaluation reference describe the same Card UID wire/row correlation as the implementation | Source search of the corrected passages; no generated alias |
| FIND-TASK-003-3 | Both changed functions have item rustdoc and fallible `main` has `# Errors` | Source inspection and `mise run fmt` |

Use the exact focused selectors for any specifically named Rust tests added to the implementation evidence, as required by `AGENTS.md`. Run only the narrowest owning `mise` lanes for the actual write set, including `mise run lints` for Rust, `mise run py:format`/`mise run py:lints` for changed Python, and the owning TypeScript checks for changed TypeScript. The broad Scribe/Forge/Oracle suites and `mise run gate` are change-review aggregate proof, not this remediation's iteration requirement.

## Constraints and non-goals

Preserve the approved service-table registration, direct and continuous result semantics, one non-blocking Scribe outbox, gateway UID authorization, initiating-principal judge calls, tenant isolation, three SDK public workflows, and accepted in-memory loss window. Do not add CardRef compatibility, a new result sink, new deployment configuration, a new memory governor, a persistent Forge fence, new test harness machinery, or unrelated refactoring. Keep the rustdoc gate intact. Revisit the original task and cumulative candidate in the next task review.

## Implementation Evidence (R1)

Commits `9655d73fe..3bf7b0d97` on `f6c841d57`.

| Finding | Implementation evidence | Verification evidence | Result |
|---|---|---|---|
| FIND-TASK-003-1 | `examples/support-desk/{rust/support_desk.rs,python/support_desk.py,typescript/support-desk.ts}` `deploy` matches the exact gateway pair `openai/<Prompt model>` (`PROVIDER` constant: the gateway provider of the Prompt's `open_ai_chat_completion` request) and its existing refusal names the pair and `PUT /v1/admin/gateway/provider-deployments/{name}`; each SDK journey first deploys `anthropic/gpt-4o` only and asserts the refusal, then configures `openai/gpt-4o` and deploys | `mise exec -- cargo nextest run --locked -p wyrd-sdk-rust --test integration -P journey --run-ignored=all -E 'test(=support_desk::support_desk_answers_verifies_and_explains_every_request)'`, `pytest -m integration tests/integration/test_support_desk.py::test_support_desk_answers_verifies_and_explains_every_request`, `vitest run tests/integration/support-desk.test.ts` (each under `with-test-postgres.sh` + `db:migrate:all:inner`); `mise run check:examples`, `py:format`, `py:lints`, `ts:lints`, `ts:typecheck` | PASS |
| FIND-TASK-003-2 | `oracle/admission.rs` `shutdown` completes and reports on `oracle_memory_reserved()` (governor `oracle_query_memory_used_bytes`; a poisoned ledger reads as fully held); both wakeups and the process pool are unchanged | `mise exec -- cargo nextest run --locked -p vala-bifrost-redux --lib -E 'test(=oracle::admission::tests::oracle_shutdown_drains_only_oracle_memory)'` (Forge bytes live, Oracle child residual reported, late child release awaited); `oracle::admission::` 28/28 | PASS |
| FIND-TASK-003-7 | `forge/worker.rs` `ForgeLoopQuiescence` shared by every supervised clone: `run` marks entry before `run_event_loop` and joined after it returns; startup reclaim passes `Some(owner)` only when joined; a stop-token drop guard cancels detached plans/heartbeats on unwind so their leases lapse | `-E 'test(=forge::worker::tests::only_a_joined_loop_enables_same_owner_reclaim)'`; `vala-sql --test integration -E 'test(=pg_forge_tasks::pg_tests::previous_owner_reclaims_its_unexpired_attempt)'`; `wyrd-testing --test forge -P journey -E 'test(=production_closeout::one_leader_failover_volatile_state) \| test(=live_rewrite::failed_worker_restarts_while_the_api_serves)'` | PASS |
| FIND-TASK-003-6 | `server/query.rs` `oracle_only_pod_retains_audit_staged_before_its_drain` uses `start_spec_delayed_last`, asserts no ingest node, stages, `restart_node(scribe)`, drains, reads retained audit | Exact selector from the packet under `with-test-postgres.sh`; `WYRD_LOG` trace shows repeated `scribe unavailable` retries before delivery | PASS |
| FIND-TASK-003-5 | `scripts/postgres/test-roles.sh` rerun passes `-v ON_ERROR_STOP=1` | `mise run test:postgres:roles`; container `psql -v ON_ERROR_STOP=1` on `SELECT 1/0; SELECT 1;` exits 3 | PASS |
| FIND-TASK-003-4 | `architecture/wyrd-design.md` observation passages, `references/domain/evaluation.md`, `references/domain/telemetry-observations.md` use `card_uid`; registry/principal `card_ref` text kept | Source search; `mise run docs:check` | PASS |
| FIND-TASK-003-3 | Rustdoc on `wyrd-client` `bifrost::tests::card` and `examples/support-desk/rust/main.rs` `main` with `# Errors` | Source; `mise run fmt` | PASS |

`mise run fmt` and `mise run lints` pass; `git diff --check` clean. Non-goals held: no CardRef alias, no deployment options or resolver, no new memory governor, no persistent Forge fence, no supervisor policy change.

Limits: FIND-7 is proven at the quiescence owner (a real panicking task) and by the restart journeys, not by a journey that panics the event loop with a plan in flight; that needs a loop-panic injection hook the packet excludes. Oracle's predicate counts every Oracle-holder pool view, which includes Scribe-node follower fragments of Oracle queries by existing design.
