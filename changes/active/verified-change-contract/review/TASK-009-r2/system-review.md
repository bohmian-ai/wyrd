# TASK-009 Round 2 System-Resilience Review

## Immutable subject

- Repository: `/home/thorrester/Documents/GitHub/wyrd-verification-closeout`
- Base: `7d96c30066425e0cde2290842d5801307843283d`
- Candidate: `c47761decff8db95768d2c05f0b85b8ba62a021a`
- Approved specification: `changes/active/verified-change-contract/spec.md`, revision 35
- Original task: `changes/active/verified-change-contract/tasks/TASK-009-run-context-and-python-otel-correlation.md`
- Prior validated ledger: `changes/active/verified-change-contract/review/TASK-009-r1/findings-validation.md`
- Remediation task: `changes/active/verified-change-contract/review/TASK-009-r1/TASK-009-R1-restore-otel-correlation.md`

The candidate remained at the stated commit before and after review. The repository has no `.codegraph/` directory, so source navigation used repository search and direct caller inspection.

## Deployed path and affected capabilities

The changed runtime path is local to the Python client process:

1. Shared `WyrdState::run` / `run_for_card` resolves the hydrated subject and `Run::new` mints one UUIDv7 invocation ID. Immutable `Run::for_card` views share that ID while selecting their own exact CardRef (`crates/shared/wyrd-client/src/state.rs:485-511`, `crates/shared/wyrd-client/src/observe/mod.rs:42-98`).
2. PyO3 `Run.__enter__` / `__exit__` delegates only ambient telemetry work to `wyrd.otel`; it neither performs network IO nor owns Bifrost/provider lifecycle (`sdks/wyrd-sdk-python/src/observe/mod.rs:195-236`).
3. `_enter_run` best-effort installs the processor, attaches the exact pair to Python OTel context, records the exact token and prior value execution-locally, and stamps an active recording span (`sdks/wyrd-sdk-python/python/wyrd/otel.py:279-297`).
4. `_RunCorrelationProcessor.on_start` copies the pair to framework-created spans. A caller/global provider's stock exporter independently sends those spans to authenticated `/v1/traces`; explicit Drift, Eval, and generic writes continue through the state-owned Bifrost writer (`otel.py:215-243`; `tests/integration/state/test_observe_journey.py:365-402,550-562`).
5. Gate/Scribe resolve asserted Card identity under signed scope and persist server-derived tenant, publisher, and Card UID. The journey reads persisted spans, custom rows, and Eval rows and proves their joins (`test_observe_journey.py:413-474`).

Affected user capabilities are Python ambient span correlation and the evidence joins that consume it. Rust/TypeScript initial-Card selection and explicit observation paths share the native `Run` owner but do not depend on Python OTel state. No server, Gate, Scribe, schema, authorization, queue, or deployment-process topology changed in the remediation.

## Failure and recovery assessment

| Failure or interruption | Observable boundary and recovery | Evidence and result |
|---|---|---|
| OpenTelemetry package missing, provider API-only/unsupported, or processor registration fails | Run construction, entry/exit, user code, and explicit observations remain available; ambient span enrichment is absent. Card lookup and explicit write errors remain strict. | Complete try boundaries in `install_run_correlation` and `_enter_run` (`otel.py:246-297`); focused missing/API-only/registration tests reach the ordinary explicit Drift boundary (`test_observe_surface.py:375-425`). PASS. |
| Context attach or span enrichment fails | The failed entry records a paired `None` stack entry, so later exit cannot detach another scope. Processor lookup/attribute failures are contained per span. Explicit observations remain independent. | `otel.py:279-297,222-231`; `test_registration_and_attach_failures_never_block_observations` and `test_enrichment_failure_never_blocks_observations` (`test_observe_surface.py:414-445`). PASS. |
| Detach raises or OTel internally swallows reset failure | Exit first attempts the exact token, then compares the current Wyrd value with the recorded prior value and re-attaches that prior value if needed. An inner failure restores the outer pair; an outer failure leaves subsequent spans unenriched. Fallback errors are contained and user exceptions still propagate. | `otel.py:300-326`; same-context nested and outer recovery proof at `test_observe_surface.py:448-467`. This closes prior `FIND-TASK-009-3`. PASS. |
| Two threads directly enter scopes on first use | Lazy key creation is serialized with the existing module lock and repeats the `None` check under the lock. Every processor and entry therefore uses one process-private key; scope values remain execution-local. | `otel.py:201-212`; deterministically contended two-thread proof at `test_observe_surface.py:470-512`. This closes prior `FIND-TASK-009-4`. PASS. |
| Nested scopes, `await`, task concurrency, or task creation inside a scope | Token/prior stacks and OTel values are `ContextVar`-backed, so each execution context restores its own innermost entry. A task created inside a scope intentionally retains its copied correlation after the creator exits, matching the approved contract. | `otel.py:189-195`; healthy nested/async tests at `test_observe_surface.py:290-350`. PASS. |
| User exception or task cancellation unwinds a `with` block | `__exit__` always invokes cleanup best-effort and returns `False`; application exceptions, including cancellation expressed through normal Python unwinding, remain unsuppressed. | PyO3 exit at `src/observe/mod.rs:219-236`; user-exception proof at `test_observe_surface.py:443-445`. PASS. Abrupt process termination has no recoverable client-local context and creates no server Run state. |
| Exporter or `/v1/traces` is unavailable | Provider/exporter failure affects ambient trace delivery only; Run exit does not flush, retry, shut down, or convert exporter state into application/Bifrost failure. Explicit observations use the separate existing state-owned Bifrost pipeline. Recovery remains caller/provider lifecycle: explicit provider flush/shutdown and state shutdown are separate operations. | Ownership is separated in `src/observe/mod.rs:195-236`; the production-shaped journey explicitly flushes the provider, shuts down Bifrost, waits for publication, and only then queries (`test_observe_journey.py:550-563`). PASS within the approved fail-open and no-provider-lifecycle boundary. |
| Client process restart or crash | The execution-local correlation and provider registration disappear with the process; no durable client Run resource or token is expected to survive. Already-exported server evidence remains durable; buffered provider/Bifrost data may be lost without their explicit lifecycle barriers, as documented. Restart creates a new invocation and reinstalls correlation lazily. | No new persistent state, lease, retry loop, health check, or global mutable Card scope was introduced. PASS. |
| Server rejects asserted Card scope | This remains an ingest refusal rather than a client-side fallback to another Card or identity. The client injects only CardRef and Run ID; tenant, publisher, and Card UID remain server-derived. | Attribute injection is limited to `_CARD_REF` and `_RUN_ID` (`otel.py:186-187,226-229`); authenticated persisted journey checks publisher and resolved UID (`test_observe_journey.py:421-474`). PASS. |

The remediation does not amplify dependency outages: it adds no retry, warning loop, background task, exporter, queue, health probe, or shutdown hook. Locking covers only one-time key creation and provider-registration bookkeeping; span starts do not hold that lock after key publication.

## Prior-finding closure

- `FIND-TASK-009-1`: closed for system resilience. Each required optional-telemetry failure class now reaches a representative explicit observation's ordinary error boundary; unknown alias and user-exception behavior remain strict.
- `FIND-TASK-009-2`: closed. `Run::subject` now documents root, initially selected, and sibling-selected views accurately (`crates/shared/wyrd-client/src/observe/mod.rs:49-55`).
- `FIND-TASK-009-3`: closed by recorded-prior restoration after raising or swallowed detach failure, with nested and same-context post-exit proof.
- `FIND-TASK-009-4`: closed by lock-protected double-checked key creation and deterministic concurrent-first-entry proof.

## Material proposed findings

None.

## Verification and limits

- Independently ran `mise exec -- uv run python -m pytest -q tests/unit/state/test_observe_surface.py`: **33 passed**.
- Inspected the recorded remediation evidence for the focused persisted journey, `test:shared`, Python unit/integration/typecheck, codegen, PyO3 boundary, format, lint, and diff checks; all are recorded as exit 0 in the immutable remediation artifact.
- This system review did not rerun the Postgres-backed journey or broad Rust/Python gates. Their recorded evidence was checked against the source path but is not represented as independently rerun evidence.
- Exporter outage and abrupt process death are assessed from ownership and lifecycle source because the approved task deliberately does not add exporter lifecycle ownership, retries, or crash durability.

## Overall result

**PASS**

The cumulative candidate satisfies the task's deployed failure and recovery boundaries. The remediation closes the stale-correlation and concurrent-first-use defects without widening failure propagation or taking ownership of provider, exporter, Bifrost, or server lifecycle.
