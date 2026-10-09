# TASK-003 r4 independent behavior review

## Subject and method

- Repository: `/Users/stevenforrester/Documents/GitHub/wyrd-verification-closeout-task3`
- Immutable range: `7f79fb341..f6c841d57`; candidate `f6c841d57fb19517ddefe83c826b24085b853845` matched `HEAD` at review start.
- Authority: `changes/active/verification-closeout/spec.md` revision 3, the original `TASK-003-r4-canonical-support-desk-closeout.md`, `AGENTS.md`, `architecture/agent-rules.md`, `architecture/wyrd-design.md`, `architecture/wyrd-doctrine.mdx`, and `architecture/bifrost-design.md`.
- Navigation: inspected the cumulative 220-file diff list, gateway ingress/invocation/capture and caller tests, Run invocation and the gateway model projection, table registration, observation activation, direct and queued results, outbox staging, cluster recovery paths, the three support-desk examples and journeys, the task evidence, and the `mise` lane membership. No tests were rerun during this read-only review; the reported green gate is implementer evidence.

## Acceptance matrix

| Obligation | Implementation evidence | Verification evidence | Result |
|---|---|---|---|
| REQ-006: optional activation and unbound observation writer | `components/cards/service.rs` projects bindings; `verification/observations.rs` stages the subject for unbound writers | Card and verification integration targets named in task; claimed broad gate | PASS |
| REQ-006: completed direct and queued results use `ScribeOutbox`, with application Run identity and no direct durable run/Operator | `verification/runner.rs:682`, `components/verification/service.rs:503-588`, `verification/results.rs`; `verification/direct.rs` sends execution identity | `pg_verification_routes.rs`, `pg_verification_runtime.rs`, `verification::results` tests; claimed broad gate | PASS |
| REQ-007: declared Service dataset table is ensured before Card activation and incompatible schema refuses | `components/cards/service.rs:562-607` reuses `bifrost::service::register_table`; no delete path added | Three support-desk journeys describe the table and refuse conflicting schema; claimed broad gate | PASS |
| REQ-007: paired Run/Card UID gateway headers, scope or tenant-registry authorization before dispatch, captured UID | `gateway/ingress.rs:250-285`, `gateway/invocation.rs:455,700-735`; `capture.rs`; shared client gateway caller | `pg_invocation_tests.rs:3115` proves valid capture, out-of-scope and unregistered refusal before dispatch; OpenAPI and SDK tests; claimed broad gate | PASS |
| REQ-008: one tool-free Agent invocation and gateway-owned judge under initiating authority | `observe/invoke.rs`; `verification/engines.rs`; `vala-eval/orchestrator/judge.rs` | Three SDK journeys prove invocation and viewer refusal; verification tests named in task; claimed broad gate | PASS |
| REQ-009: telemetry setup, Run scope, credential refresh, foreign-provider refusal, shutdown flush | Rust/Python/TypeScript SDK OTEL modules and state/Run projections | OTLP and SDK tests named in task; claimed broad gate | PASS |
| REQ-010 and task contract: checked-in `deploy` confirms the Agent's exact gateway model deployment and gives remediation when absent | All three `examples/support-desk/*/support_desk*` implementations compare only the model-name field; none gives a remediation step | Journeys use only one matching provider and do not exercise an absent exact deployment | FAIL (BEH-001) |
| REQ-010/AC-005: 100 requests, 10 refund promises, both verifier totals, MCP explanation of passing and failing requests across three SDKs | Three example `serve`, `wait_for_verdicts`, and `explain` functions; three fixed journey files | Three journey assertions prove counts and six-way evidence joins; claimed broad gate | PASS |
| Scenario 5: peer Scribe routing independent of Oracle; membership and heartbeat lifetimes; Forge and Oracle recovery | `scribe_outbox.rs`, `boot/mod.rs`, `cluster/mod.rs`, `forge_tasks.rs`, `oracle/admission.rs`; no new route protocol | Task records Scribe 28/28, Forge 22/22, Oracle 50/50 and focused tests; claimed broad gate | PASS |
| Scenario 6: Postgres bootstrap uses container `psql` and propagates failure | `scripts/postgres/with-test-postgres.sh`, `test-contract.sh`, `test-roles.sh` | `test:postgres:contract`, `test:postgres:roles`; claimed broad gate | PASS |
| AC-006: generated contracts and repository checks | Spec, SDK declarations/stubs, OpenAPI tests, docs, `mise.toml` | Task records `codegen:check`, `fmt`, `lints`, `git diff --check`, final `mise run -c gate` exit 0 | PASS on supplied evidence |
| Non-goals: no Tool calling, Workflow correlation, new MCP tool, table drop/evolution, telemetry metrics/log setup, UI feature | Scope of changed public implementations and support-desk example | Diff inspection | PASS |

## Proposed finding

### BEH-001 — INCORRECT — `deploy` accepts the wrong provider's model

**Violated obligation:** Spec R4 decision 14 and REQ-010, plus TASK-003 Required Contract Detail and Scenario 4, require the support desk to confirm the Prompt's model is deployed and to name the missing model with remediation. Gateway model identity is the `(provider, model)` pair (`wyrd-spec/src/gateway/mod.rs:119-124`); the Agent projects its Prompt's provider and model through `skald-workflow/src/route.rs:439-445,495-506`.

**Exact changed locations:** Python `examples/support-desk/python/support_desk.py:91-93`, Rust `examples/support-desk/rust/support_desk.rs:122-132`, and TypeScript `examples/support-desk/typescript/support-desk.ts:77-80`. Each tests only `deployment.model.model == prompt.model`. The absence messages name only the model and supply no deployment remediation.

**Reachable consequence:** Configure a deployment for `anthropic/gpt-4o` while the checked-in support Prompt requests `openai/gpt-4o` (`service/support-prompt.yaml`). All three `deploy` calls report success. The first `Run.invoke` then asks the gateway for `openai/gpt-4o` and receives model unavailable, after deployment has already been reported complete. A deployment with the same exact model but without the Agent's chat capability is another possible false positive. The journey fixture configures only the matching OpenAI deployment, so its green path cannot distinguish these cases.

**Required testable correction:** In each public example, compare the same exact provider/model identity the existing Agent gateway projection uses against `Gateway.deployments()`. Preserve the current no-deployment refusal and make that message name the exact missing identity and the configuration action. Add one focused cross-provider collision test in the narrowest existing journey or example test surface; the three SDK journeys should keep proving the shared happy path. No gateway owner, contract, new helper, or new deployment configuration is needed.

## Diagnosis checks and limits

The reported Scribe fault/counter, Forge restart and inherited-state, Oracle cleanup/order/memory, UI replica-expiry, Rust SDK lane, and Postgres-wrapper diagnoses were checked against their changed owners and reported focused tests. No contradictory behavior was established in this pass. In particular, `oracle/admission.rs:737-764` enables both notifications before checking active queries and shared memory; `forge_tasks.rs:890-900` includes the previous owner in the reclaim predicate; and `mise.toml:628` excludes the Keycloak-dependent Rust modules from the generic SDK lane.

The gate result, journey counts, and independent diagnostician claims are recorded in the task rather than accompanied by raw run logs in this report. This review did not rerun the broad gate. The one finding is source-proven and is outside the successful single-provider journey fixture.

**Overall: FAIL.**
