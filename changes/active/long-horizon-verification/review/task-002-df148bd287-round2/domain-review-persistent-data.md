# Persistent data, tenancy, and transaction review

Result: **PASS**. Proposed findings: **none**.

## Immutable subject and scope

- Base: `ef3a57b0776dd7bc451273b9176498fcca700550`.
- Candidate: `df148bd287d3e9fcae861470fe9c27bd576f2eeb`.
- Reviewed the cumulative range, including the SQL migration, binding projection, execution claim decoding, runner dispatch, scheduled Drift reads, supporting SQL tests, and affected contract consumers. Compared the remediation range from `5c421663b43d7deeca135134d6b07cf49a11b83f` to identify its additional changes.
- Authority: original TASK-002 and approved revision-13 REQ-012 / AC-007 / INV-001–005, under AGENTS.md, agent-rules, Wyrd design/doctrine, the reference router, architecture constraints, evaluation and analytical reliability references, spec workflow, maintainer style, and TESTING.md. No draft-14 addition is an acceptance requirement.
- Lens: persistent selector retirement; frozen subject/Verifier/window identities; transaction composition, tenant isolation, lease/retry/reclaim and dispatch preservation. This is not the separate evaluation or repository-standards audit.

CodeGraph was consulted first for queue/binding navigation and then for the existing Verifier cache. Its broad binding query returned unrelated symbols, so direct source inspection supplemented it.

## Boundary and owner coverage

| Boundary | Source evidence and assessment |
|---|---|
| Registration → binding projection | `components/cards/service.rs:1218` persists the Card, relationships, service principal, and bindings in the same caller-owned TenantConn. `BindingProjector::freeze` at 1304 uses pinned subject and Verifier UIDs, freezes Trigger/Operator identities, and removes only the redundant input-map field. No additional writer or transaction owner was introduced. |
| Binding persistence | `queries/verification.rs:39` and `project_bindings` at 316 retain the existing natural key, `wyrd.current_tenant()` stamping, parameter binding and no-commit behavior. `ON CONFLICT` preserves the existing immutable binding identity and frozen targets. Removal of argument 12 matches removal of the final insert column. |
| Migration | `20261017000000_drop_verification_binding_inputs.sql:6` drops only `verification_bindings.inputs`; it does not drop binding rows, subject/Verifier identities, windows, queue state, baselines, dispatches or constraints governing these authorities. The retired map has no remaining production SQL read/write in the candidate. No compatibility or retained-state upgrade mechanism is added. |
| Tenant isolation | Binding forced-RLS policies remain in `20260601000027_verification_bindings.sql:94–101`. Changed production query signatures continue to take TenantConn; cross-tenant discovery remains OperatorPool-owned. Claim subqueries remain on the same tenant connection and remove a selector lookup rather than widening a read. |
| Scheduling and deduplication | `VerifierExecutionQueue::claim_due`, `schedule_due`, and `enqueue_scheduled` retain the SKIP LOCKED binding claim, one transaction for occurrence insertion plus cursor advance, unique occurrence replay, exact Verifier/subject/Trigger/Operators and accepted authority. No selector value participates in occurrence identity. |
| Claim/retry/reclaim | `CLAIM_SQL` at `verifier_executions.rs:215`, `claim` at 1444, and `ClaimedRow::into_claimed` at 1927 retain the same execution row and frozen window on each attempt. Only the binding map lookup and decoded `slots` field disappear. Statement-time lease expiry, attempt accounting, fresh lease tokens, renewals and fenced settlements are unchanged. |
| Exact Verifier resolution | `VerifierRunner::resolve` at `runner.rs:294` resolves the claim's Verifier UID in the claim transaction; existing `VerifierCache` keys entries by tenant plus immutable Card UID. Reclaim cannot switch to a newly registered Verifier version or another tenant's cached declaration. The queue still returns the exact version for result provenance. |
| Observation read authority | `DriftEngine::checks` at `drift.rs:688` iterates every pinned Verifier drift requirement. `score` at 712 constructs ObservationWindow from `run.card_uid` and its frozen window; the existing Reader at 815 resolves the tenant SYSTEM identity and observation table under accepted authority and uses ScheduledQueryCaller. Removing the map does not replace or bypass those owners. |
| Result/dispatch preservation | Runner dispatch changes only its Scheduled pattern and argument list. Queue completion/retry/termination/release, failed-only dispatch creation, result identity and Scribe staging remain unchanged. No group, outbox, or admission changes enter this boundary. |

## Obligation and proof assessment

| Obligation | Implementation evidence | Supporting verification evidence inspected | Result |
|---|---|---|---|
| REQ-012: exact subject/window/Verifier remain the sole scheduled authority | Pinned binding projection → scheduled row → claimed window → tenant/UID cache → ObservationWindow; no second selector map remains | `pg_verifier_executions::scheduled_and_submitted_executions_freeze_their_inputs` still asserts subject, Verifier UID/version, origin, binding, Trigger and window | PASS |
| REQ-012: retry/recovery preserve requirements without selector state | Reclaim returns the same row's Verifier UID and window; exact immutable spec supplies all checks each attempt | Existing `reclaimed_lease_fences_the_stale_token`, retry and release tests remain; `concurrent_schedulers_create_one_run_per_occurrence` retains identity/window assertions | PASS |
| INV-003: tenant and accepted authority are preserved | TenantConn signatures, forced RLS, tenant-keyed cache and accepted-authority SYSTEM reads | Existing `queue_state_is_tenant_isolated`, keyed tenant isolation and binding tests remain; task evidence records the combined binding/execution module run (32 passed) | PASS |
| INV-004/005 and explicit non-goals: immutable identities, fences, audit/outbox and scoring remain owned by existing mechanisms | No edits to settlement SQL, lease policies, authority construction, cache identity, outbox writer or baseline-scoring algorithms | Existing SQL fence/replay tests and scheduled SDK assertions remain; task evidence records passing scheduled and server regression runs | PASS |

The supporting lease tests demonstrate the unchanged lease machinery; they do not themselves rerun a two-requirement scheduled judgment after a forced reclaim. Source tracing supplies the connection: the exact immutable spec and frozen window are unchanged between claims, and the only removed predicate was the selector-presence gate. I found no new state that could select a reduced requirement set on a later attempt.

## Remediation and verification limits

The remediation adds decoder documentation, corrects activation error metadata and its generated API row, and extends the existing HTTP refusal assertion. It does not alter the persistent-data/transaction paths above. The two prior findings are outside this domain's independent closure assignment; their complete closure belongs to the standards/behavior audits and final validation.

No runtime command, Postgres command, full journey suite, or broad gate was run by this reviewer, as assigned. Recorded implementer counts are historical claims, not independently reproduced runs. The orchestrator reports fresh Postgres migration/idempotence checks, the exact HTTP refusal/no-write test, decoder focused test and formatting passing; see `verification.md`. It did not rerun broader SQL/runtime history or journeys, and notes a documentation environment failure. Those executions are orchestrator evidence, not executions by this reviewer. HEAD remained the candidate when checked. No production source, tests, migrations, or prior reports were modified.
