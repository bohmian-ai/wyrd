# Maintainer review

Result: **FAIL**

Subject: `b22718f2cf5da0ebcb7f26bc376157e5eec7768f..518a5d8b822a4e649a5fe90b14f3eb2b1fc8a0aa`, against approved `SPEC-sub-act-attribution` r4 and `TASK-001-sub-act-attribution`. This is an independent static discovery report, not a validated final finding ledger.

## Changed-surface coverage

The cumulative diff has 222 paths. I used CodeGraph first for attribution/context owners and again for registration/result owners, then inspected the changed implementations, their owning modules, callers, projections, and relevant tests. Mechanical field-name/fixture adaptations were inspected as part of their owning surface; they were not treated as separate new abstractions.

| Changed surface | Owners and caller/consumer evidence inspected | Maintainer assessment |
| --- | --- | --- |
| Pure identity and accepted runtime authority | `wyrd-spec/src/auth/attribution.rs`; `wyrd-runtime/src/request_context.rs`; runtime/spec re-exports; `Caller::{from_authenticated,accepted,executed_by_system,attribution}`; authenticated HTTP and Gate extraction | The small pair and credential-free snapshot express different invariants. The existing verified principal and delegation order remain the source. No new token parser, permission algebra, or propagation service. Rustdoc gap noted below. |
| Realtime execution and hosted Workflow calls | `VerificationControl::{execute,stage_result,direct_target}`; `DirectExecutor`; `EvalEngine::{execute,score,judge_invoker}`; `Preparation::prepare`; `ServerWyrdGatewayCaller`; `GatewayInvocation` and its ledger/capture consumers | Reuses the existing captured `Caller`, hosted preparation, and governed gateway. Workflow Card attribution is added at the existing construction point. Direct results and judge operations intentionally have different actor pairs; public prose does not consistently preserve that distinction. |
| Deferred authority and scheduling | `BackgroundAuthority::resolve`; scheduler claim/settle flow; `ObservationRunSink`; `RunAuthority`, `RunRequest`, `DueBinding`, `DueDecision`, `ObservationBinding`, `VerifierRunQueue` enqueue/claim/read paths; runner/cache/results | The scheduler split permits server-owned identity resolution while preserving caller-owned SQL transactions. The observation split extends the existing binding-lock/batch-insert owner. Neither creates a parallel scheduler, resolver, queue, lease owner, or result publisher. `TenantTokenIssuer::recover` is reused. |
| Creator pairs and replay | Card registration SQL input/row types and lookup/insert functions; server registration replay, writer and `persist_node`; service-account projection and API-key insertion; principal routes; platform provisioning/recovery; operator connections; defining SQL and row projections | The pair is passed as one domain value; existing update statements preserve creation values. Existing key scope and conflict mechanisms are extended. Required fallible-operation rustdoc is incomplete. |
| Ingest and physical columns | `ScribeIngressFrame`; Gate dispatch/auth; Scribe ingress/preprocess/decode/stamping; `ManagedValues`; `CorrelationPolicy::{reserves,stamps}`; managed declarations/layout/wire and built-in audit/result/gateway tables; Variant claim sampler; queue preflight | Existing managed-column and table-policy owners are extended. Policy-sensitive stripping correctly has one local owner for audit content versus managed identity. Sampling reuses the existing sampler with the subject column. No new configuration or dependency. |
| Audit, results, spans and peer wire | Auth audit constructors and auth issuance/exchange/refusal callers; server audit; Oracle audit event projection and read context; `ResultPayloadBuilder`; `ScribeWrite`, `Rows`, `Frame`, `ScribeBatch`, `ScribePeers`; peer capture RPC/proto; `TenantSpanRoute`, route rebinding and export sink | The generic pair replaces the result-only attribution wrapper and optional principal combinations. Same outbox/frame path serves local and peer writes. Operation-route rebinding extends the existing tenant-copy registry. Architecture peer prose retains the superseded result-only envelope claim. |
| Client, generated and authored declarations | Shared client `BoundVerifier`, observe request, Bifrost table docs; spec verification/gateway/audit wire types; Python Bifrost docs; generated TypeScript table description | Wire renames originate in Rust contracts; no extra Python/TS transport or duplicate validation implementation. Changed authored docs describe the reserved names. No manifest or dependency additions are present. |
| Test and fixture adaptations | New SQL pair/replay tests; realtime/background/restart/audit server journeys; Rust ingest/Workflow/timeline tests; Python and TypeScript reserved-column/timeline tests; MCP direct/delegated trace test; dev-fixture SYSTEM seeding; server fixture/projection updates and remaining mechanical fixture changes | Tests use existing server, deployment, runtime-script, SQL fixture, and public client facilities. New private fixture helpers remove repeated setup without adding a harness. Some new tests omit required panic documentation. |
| Architecture, docs and examples | Changed design/security/Bifrost/reference prose; verification docs, MCP guide, SQL diagrams; support-desk examples in all three languages; task evidence and the two prerequisite-review whitespace changes | Card-role examples now agree across languages. Finding MAINT-002 covers the remaining contradictory actor/peer prose. Whitespace-only prerequisite artifacts do not introduce implementation machinery. |

Authority applied: `AGENTS.md` §§5, 9, 11, 15–16; `architecture/agent-rules.md`; `architecture/references/languages/maintainer-style.md`; approved spec r4, especially REQ-002/005/006/008/011 and its coverage inventory; applicable attribution, Workflow, security and Bifrost sections of the current architecture. The task's reported V1–V11 and owner/static results are evidence claims; no Cargo-backed commands were run by this reviewer.

## Findings

### MAINT-001 — VIOLATION: materially changed fallible operations and new tests omit required rustdoc sections

Severity: must-fix under the explicit rustdoc gate.

Locations:

- `crates/wyrd/wyrd-sql/src/queries/cards/register.rs:144`, `:168`, `:192`, `:211`, `:235`: `lookup_existing_operation`, `lookup_expired_operation`, `lookup_operation_by_id`, `insert_registration_operation`, and `insert_card_row` have one-line descriptions and no `# Errors` sections. Their bodies and/or durable projections were changed in this candidate.
- `crates/wyrd/wyrd-server/src/components/cards/service.rs:636`: materially changed `replay` also has no `# Errors` section, although it performs SQL/commit and returns the replay validation errors.
- `crates/shared/wyrd-runtime/src/request_context.rs:317`, `crates/vala/vala-sql/tests/integration/pg_olap_catalog.rs:84`, and `crates/wyrd/wyrd-sql/tests/integration/pg_cards_register.rs:239`: newly added tests use `expect` and assertions but omit `# Panics`.

Rule: `AGENTS.md` §16 and the maintainer guide's Documentation section explicitly require substantive rustdoc for materially modified private and public Rust items, `# Errors` on fallible operations, and `# Panics` when panic remains possible. The review skill explicitly keeps this gate blocking.

Evidence and maintenance cost: the registration SQL API now owns subject-and-actor replay isolation and immutable creation attribution, yet its operation documentation does not state failure conditions or transaction ownership. A maintainer cannot distinguish an unused/replayed key (`false`/`None`) from registry storage failure from these contracts. `insert_card_row` also rejects absent space and unresolved version before SQL. Green Clippy does not establish compliance with the repository's stricter documentation rule.

Smallest correction: document these changed items in place. State their actual failure conditions and relevant caller-owned transaction/rollback behavior; document the new tests' fixture/serialization/assertion panic conditions. Reuse the nearby substantive `VerifierRunQueue::enqueue_manual` and `upsert_service_account_from_card` documentation shape. Do not create helpers, alter runtime behavior, or expand this into an unrelated rustdoc sweep.

Focused closure proof: inspect each cited complete item against its body and the unchanged caller-owned transaction boundary; formatting/lints for the documentation-only write set suffice. No new runtime test is needed.

### MAINT-002 — INCORRECT: changed result documentation conflates request attribution with judge execution attribution

Severity: must-fix public contract documentation; batch with the implementation remediation rather than opening a style-only round.

Primary location: `docs/src/content/docs/understand/how-verification-works.md:167`. Related changed prose: `architecture/wyrd-design.md:1264–1269` and `architecture/bifrost-design.md:1406–1413`.

Rule: approved REQ-002 distinguishes a principal's direct verify call (A/A or delegated A/B) from judge model calls (A/SYSTEM); REQ-006 and the coverage inventory require documentation and consumers to agree. The maintainer guide requires documentation to describe the actual typed contract.

Evidence: the public results table now says `act_principal_id` is the tenant system principal for every completed check. `VerificationControl::stage_result` instead stages a direct result with `caller.attribution()` (`components/verification/service.rs:588–592`), preserving A/A or A/B, whereas queued `runner::result` uses the run subject with SYSTEM (`verification/runner.rs:774–777`). The realtime journey explicitly documents/asserts acceptance and results as A/A or A/B and judges as A/SYSTEM. The new general design paragraph says result writes are SYSTEM without making this direct/queued distinction. Bifrost prose also still says the peer RPC carries attribution "exactly when the table is a result table", although the new proto requires subject/actor fields on every capture request and `ScribePeers` supplies them for all destinations.

Observable consequence: a caller following the public column description and filtering realtime verification results by SYSTEM actor misses its actual result rows. A maintainer following the architecture would also reconstruct the removed result-only peer envelope or change the correct direct result path to match inaccurate prose.

Smallest correction: retain implementation behavior and state the direct-versus-queued result distinction explicitly in the result-column documentation and related architecture text. State that judge calls/input reads use SYSTEM and that the peer envelope carries a pair for every destination, while audit preserves its own content pair. No new field, alias, RPC, identity rule, or implementation change is needed.

Focused closure proof: compare the revised prose against direct `stage_result`, queued `runner::result`, `ScribePeers::submit` and `ScribeCapturePeerGrpc::batch`; run the existing docs lane only if docsite content is edited. Existing realtime/result/peer tests already demonstrate the relevant behavior.

## Reuse and complexity assessment

No blocking unearned abstraction was found. `Attribution` is a two-value domain fact; `AcceptedAuthority` retains the distinct permission snapshot/history invariant; `RunAuthority` combines that snapshot with the accepting actor needed by background work. `BackgroundAuthority` retains the existing tenant issuer collaborator for two actual background consumers. `DueBinding` and `ObservationBinding` carry the locked queue decisions across the existing server/SQL ownership boundary rather than adding an alternate durable workflow.

The candidate deletes `VerifierAttribution`, the stale `CorrelationColumns` utility, and the unused `ApiKeyRow` mirror, and replaces multiple Scribe optional-identity combinations with one pair. Those are useful simplifications. No dependency, feature flag, configuration knob, alternate audit sink, or compatibility alias was added.

Non-blocking preferences: none. I did not propose naming, layout, or line-count refactors.

## Invariant-review handoff

One source-backed attribution-conversion uncertainty was sent to the orchestrator for domain validation; it is not counted as a maintainer finding or used to establish this report's result. `SystemReadAuthority::serving` installs the run subject on `AuthorizedQueryContext::served_subject`, and Oracle's `query_audit_event` honors it. In contrast, `ScheduledQueryCaller::record_object_denial` (`crates/wyrd/wyrd-server/src/query/scheduled.rs:222–232`) constructs a `Caller` from `context.principal` with `executed_by: None` and ignores `served_subject`. For a Verifier input read, that principal is SYSTEM. If this path receives `BifrostError::QueryForbidden` from `dispatch`, its outer denial audit appears to become SYSTEM/SYSTEM instead of run-subject/SYSTEM. The conversion is an existing sibling consumer materially adapted by this change; the new subject is not carried through it. A security/invariant reviewer should establish the reachable denial case and its audit behavior before deciding whether this is a bounded missing propagation finding. No optional new guard or authority model is recommended here.

## Verification limits

This was static review of the immutable cumulative subject. Runtime and SDK checks were not rerun; the task's recorded results were read as reported evidence. Security, queue-resilience and full acceptance verdicts belong to their independent reviewers; this report does not substitute for them.
