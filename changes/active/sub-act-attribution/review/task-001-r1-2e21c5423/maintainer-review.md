# Maintainer review

Result: **FAIL**

Subject: `b22718f2cf5da0ebcb7f26bc376157e5eec7768f..2e21c5423036460d5da2ca22a51efde63635e352`, against approved spec revision 6, the original TASK-001, and R1. This is an independent discovery report, not the final validated finding ledger.

## Changed-surface coverage

Used CodeGraph before source navigation, then inspected cumulative owner changes, their immediate modules, callers, consumers, and relevant tests. The R1 diff was a locator, not the acceptance range. Mechanical row-name, projection, and fixture adaptations were reviewed by owning surface rather than treated as new capabilities. No current peer report or conclusion was read.

| Surface | Owners, callers, and proof inspected | Assessment |
| --- | --- | --- |
| Identity and accepted authority | `Attribution`, `delegated_attribution`, `AcceptedAuthority`, runtime/spec exports; `Caller::from_authenticated`, `accepted`, `executed_by_system`, `attribution`; Gate authentication and frames | Pair, permission snapshot, and execution override have different responsibilities. Pure projections remain synchronous. No new token parser, permission evaluator, or propagation service. |
| Direct verification and gateway | `VerificationControl`, `DirectExecutor`, Eval judge construction, `ServerWyrdGatewayCaller`, `GatewayInvocation`, ledger and capture; realtime and timeline journeys | Existing execution and gateway owners retain authority and stamp attribution. The removed observation-writer authority recovery is not retained as a second path. |
| Durable queue and background acceptance | `VerifierRunQueue` enqueue/manual/claim/status, `RunAuthority`, `DueBinding`/`DueDecision`, observation binding locking and batch insertion; `BackgroundAuthority`; scheduler, runner, cache, observation outbox, boot/state composition | The existing scheduler and batch-insert workflow are extended. Server-side recovery uses `TenantTokenIssuer::recover`; SQL retains transaction, binding lock, cursor, deduplication, and lease ownership. `BackgroundAuthority` has two real consumers. `ObservationRunRows` owns the parallel insert arrays and their aligned append. No replacement queue, scheduler, cache, or authority store. |
| Creation provenance and replay | Defining SYSTEM SQL and wrapper, tenant provisioning/recovery, `BuiltinTables`, `BifrostCatalog`, OLAP upsert; Card registration/replay and projections; service accounts, key issuance, operator connections; fixture seeds | R5 creator rules enter the existing provisioning and catalog workflows. Conflict/update paths retain first attribution. Existing replay scope is extended with the pair rather than duplicated. |
| Local and hosted Workflow | `CardGraphHydrator::load_workflow`, `WorkflowCards::load`, `Workflow::run_on`, `SelectedRoutes::dependencies`, `PublicWyrdGatewayCaller::with_subject`, hosted `Preparation`, gateway Card check; Rust/Python/TS gateway journeys | Registered-root provenance is retained on the shared client facade and passed to the existing header/correlation owner. Authored/parsing constructors retain no registered provenance. This does not add a graph traversal or alternate loader. Hosted execution uses the existing captured caller and gateway. R6 refusal is exercised before provider work. |
| Managed ingest and storage | Gate/frame context, Scribe ingress/preprocess/decode/stamping, `ManagedValues`, `CorrelationPolicy`, schema fingerprint/layout/wire, built-in table declarations, claim sampling, client/queue reservation | Extends existing policy and managed-array owners. Audit content identity remains distinct from managed stamping. Subject sampling changes its axis without replacing the sampler. |
| Outbox, spans, and peer contracts | `ScribeWrite`, `Rows`, `Frame`, `ScribeBatch`, sink grouping/content identity, peer submission and capture decoding; `TenantSpanRoute`, operation rebinding, server span sink | The general pair replaces the result-only wrapper and optional identity combinations. Existing outbox batching and peer transport remain the workflow. Route rebinding lives on the existing live-route registry; no trace-derived authority. |
| Audit and internal reads | Auth event constructors/callers, `IssueApiKey::audit` and route, typed credential detail, server audit, `SystemReadAuthority::serving`, Drift/Eval readers, Oracle context/audit projection | Reuses typed chain projection and query-context serialization. No new audit sink, event type, token exchange, or query authority. SYSTEM's narrow table grants stay separate from served-subject/history attribution. |
| Public contracts and documentation | Verification/gateway/audit wire types, proto, shared client observe/state/table docs, Python table docs, generated TS declaration change, MCP guide/query consumers, docsite explanations, architecture and support-desk examples | Changed fields originate in existing source contracts. No new SDK transport, Python implementation, compatibility alias, manifest, dependency, or feature. Corrected result descriptions distinguish direct caller pairs from queued SYSTEM execution; peer descriptions carry the pair on all destinations. |
| Tests and fixtures | SQL creator/replay/queue tests; realtime/background/queued/reloaded-authority/read-history/issuance/scheduled journeys; SDK row, local/hosted Workflow and timeline tests; MCP delegated spans; platform provisioning and defining-SQL tests; remaining schema/frame fixture adaptations | Existing fixture/server/telemetry/query/runtime owners are reused. Tests observe retained rows, actual provider requests/refusals, and actual expiry rather than treating a successful call or unchanged snapshot as sufficient proof. One new test retains the mandatory rustdoc gap below. |

Authority applied: AGENTS §§2–11 and §16; agent rules; maintainer style; spec-driven-development; Rust-core, testing-workflows and ownership patterns; relevant identity, accepted-authority, Workflow, audit, Scribe and peer sections of Wyrd design, doctrine, security posture and Bifrost design; approved spec r6 and R1. Implementation evidence is reported evidence, not authority.

## Material proposed finding

### MAINT-R1-001 — VIOLATION: new protected-authority seam omits its panic contract

Prior stable finding: **FIND-TASK-001-8** (partial closure).

- **Changed location:** `crates/wyrd/wyrd-testing/tests/bifrost/server/verification_runtime.rs:3255`, `reloaded_authority_cannot_widen_gateway_access`; direct panic source at `:3362`.
- **Governing rule:** AGENTS §16 requires `# Panics` whenever a panic remains possible; the maintainer guide applies this to new tests. The task-review skill expressly retains missing mandatory rustdoc as blocking. R1 requires both the enumerated earlier items and newly touched items to satisfy this rule.
- **Evidence:** The new test documents `# Errors` only. Its `call` closure parses `openai/gpt-test` with `.expect("the model reference is valid")`; the test invokes this closure for both the reloaded denied call and the fresh permitted call. The missing section is on the owning test, not an unrelated untouched helper.
- **Concrete maintenance cost:** The test presents its failure contract as returned `ServerJourneyError` while retaining a separate fixture/model-invariant panic. The explicit repository documentation contract remains incomplete even though the current literal is valid. This is a documentation gate, not an assertion of a runtime authorization defect.
- **Smallest correction:** Add an accurate `# Panics` section naming the fixed model-reference invariant. Keep the test body, assertions, authority path, expiry observation, and provider checks unchanged. Nearby `audit_retention::issued_key_audit_keeps_delegation` documents both errors and panics for the same mixed failure style. No helper, harness, scanner, or additional runtime test is needed.
- **Closure proof:** Inspect the test rustdoc against its complete body and run the normal formatting/whitespace check for the prose edit. Consolidate with prior FIND-TASK-001-8; do not assign a new durable finding ID to the same mandatory documentation obligation.

## Prior closure hypotheses

| Prior finding | Current maintainer-source assessment |
| --- | --- |
| FIND-TASK-001-1 | Approved r5 selects creator/sentinel semantics. Existing SQL provisioning, authenticated tenant provisioning, catalog creation and callerless reconciliation now carry the chosen pair and preserve the first pair. |
| FIND-TASK-001-2 | Shared registered Workflow hydration returns root provenance to the existing client facade; dependency preparation reuses `with_subject`. Direct/delegated Rust journeys check exact Card/null Run and the out-of-scope refusal. Python/TS exercise the same shared implementation. |
| FIND-TASK-001-3 | Superseded by approved r5; ordinary unresolved authentication refusals remain unaudited. No new mechanism required. |
| FIND-TASK-001-4 | `ObservationRunSink::write` emits one binding-specific diagnostic at acceptance and keeps healthy siblings. Existing telemetry capture observes that diagnostic. |
| FIND-TASK-001-5 | Issuance completion uses the verified pair and typed chain; retained-event test compares creation values/history and checks secret absence. |
| FIND-TASK-001-6 | Drift/Eval pass accepted authority to the shared served-read context producer; typed context serialization preserves its chain without changing SYSTEM grants. |
| FIND-TASK-001-7 | New tests reach actual scheduled execution, retained input-read history, missing/foreign identities, further delegation and the reloaded protected consumer. No new engine or public API was introduced to manufacture proof. Runtime sufficiency remains for the acceptance/domain reviewers. |
| FIND-TASK-001-8 | Previously enumerated registration/replay/row-conversion and three test items now have the required sections. Not fully closed: MAINT-R1-001 identifies the new seam test's omitted panic contract. |
| FIND-TASK-001-9 | The cited direct/queued result table and attribution paragraphs now match the producers; peer prose matches the unconditional pair envelope and audit-content distinction. |

## What is simple here

The candidate deletes `VerifierAttribution`, the unused correlation utility and API-key row mirror, and the cached Verifier reference that no longer has a consumer. The required snapshot uses the existing durable run rather than introducing another persistence owner. No optional preference finding or new abstraction is requested.

Skipped independent runtime/Cargo/SDK execution as directed; reported green checks were not reproduced. HEAD remained the candidate and cumulative `git diff --check` passed; the finding concerns mandatory documentation, not a demonstrated runtime failure.
