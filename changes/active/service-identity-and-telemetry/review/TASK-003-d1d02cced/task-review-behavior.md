# TASK-003 behavior acceptance review

Overall result: **FAIL**. Proposed finding set: **BEH-001, BEH-002**.

## Subject, scope, and evidence

First review of base `ca2ac4a37e229a8f95667c073db11c70ad7ecb8b` to candidate
`d1d02cced07582a8f817d3f5651e7895b96d046d`. HEAD was checked and remained that
candidate. This is the complete task acceptance review, with no carried-forward
acceptance. Source was not edited.

Authority: approved spec revision 16; authentication, telemetry-schema, and
table-storage/correlation supplements; original TASK-003 and all seven scenarios;
root and applicable Python/TypeScript/docs/SQL AGENTS rules; spec-driven workflow;
maintainer, Rust, error and agent-surface guides; TESTING.md; owning Bifrost table,
ingest and public-surface rules; Wyrd configured context, telemetry and gateway
rules; security request identity, internal capture and tenant isolation rules.

The source review followed these paths, expanding the supplied navigation map:

| Surface | Inspected producer, consumer, and proof |
| --- | --- |
| Catalog | BifrostNamespace, TableRef, logical/physical identities, catalog wire projection, SQL metadata/publication migration, Oracle catalog registration and permission scope, server Bifrost operations, SDK table/default literals, MCP and CLI changed callers |
| Schema and layout | Managed-column owner, all nine table declarations, shared dimensions/default layout builder, signal ledgers, stored describe classification, runtime schema/layout assertions |
| Signal production/admission | ResourceEnvelope/ResourcePromotions, final attribute selection and typed Variant, trace/log/metric projection and native derivation checks, Gate auth/attributed Cards/frame dimensions, native decode/stamping, public HTTP/gRPC adapters |
| Trusted origins | ResultPayloadBuilder, direct verification and runner gateway callers, group/drift readers, audit projection, ScribeWrite/Chunk/ScribeBatch, capture-peer construction, Scribe decode and per-row stamping |
| Gateway | Public context parser, call admission and subject authorization, native/compatible and Batch route callers, buffered/streaming settlement, CallCapture calls and attempt-span projection, outbox encoding |
| Lifecycle and SDKs | WAL/replay/preprocess context changes, Oracle/recovered reads, Forge/canonical physical schema consumers, client observation/preflight and queue changes, Python and TypeScript projections/declarations and added OTel/gateway journeys |

Verification actually performed: static Git range/source/test inspection only.
No Cargo, Python, TypeScript, database, or runtime check was run by this reviewer;
shared-checkout Cargo serialization was respected. The task's recorded focused
test and lane results are available evidence, not independently rerun results.
In particular, its recorded gateway test reads gateway calls, not the attempt
spans implicated in BEH-001; its native spoof test exercises spans, not the
public drift table implicated in BEH-002.

## Acceptance matrix

PASS below means the inspected source and available proof support that bounded
obligation; it is not a claim this reviewer executed the referenced test.

| Requirement, criterion, constraint, or non-goal | Implementation evidence | Verification evidence inspected / recorded | Result |
| --- | --- | --- | --- |
| Scenario 1: one bifrost catalog, no alias, matching table grants and tenant-bound lookup (REQ-026,039–040; AC-008,014) | `namespaces.rs`, `catalog/table_ref.rs`, catalog physical namespace identity, Oracle catalog/default registration, `permission_scope.rs`, SQL metadata/publication function replacements and client defaults agree on bifrost | `canonical_catalog_has_no_alias`; `canonical_catalog_queries_and_grants_agree`; changed MCP query/discovery and CLI query cases, recorded green | PASS |
| Scenario 2: universal seven-field envelope, nine built-ins with six dimensions once, dynamic table envelope only (REQ-022,028,034,051; AC-012,024) | `tables/managed_columns.rs`, `with_shared_dimensions`, all nine DomainTable declarations; audit principal/request fields removed from authored block; three explicit owner columns moved into managed workload slot; catalog describe classifies only subject/Run as public correlations | `verification_tables_match_their_approved_schemas`, `system_and_dynamic_tables_share_attribution`, physical schema round-trip tests | PASS |
| Scenario 2: exact defaults, null ordering, shared-plus-specific Bloom sets, hourly/daily axes, no mandatory floor (REQ-025,035–036,042,044,048; AC-016,018,021) | `attributed_layout` constructs E/S/V/time in order; each table supplies its ledger time/lookup keys, audit keeps accountability layout; defaults flow through existing catalog engine and do not reinsert filters at write time | `physical_layout_contract_all_builtins`, fixed-point/layout tests and nine-table describe assertions; existing result-layout journey now uses declared Blooms | PASS |
| Scenario 3: 65/46/58 schemas; resource/scope presence, final duplicates, stable/old environment typed fallback and no envelope crossover (REQ-021–024; AC-007,026; INV-008) | ResourcePromotions and ResourceEnvelope preserve raw attributes; string/integer extraction uses final typed occurrence; three projectors carry added columns and resource/scope flags | maximal trace/log/metric tests; `resource_promotions_preserve_final_typed_values`; Python/TS OTel rows include canonical owned values and raw labels | PASS |
| Scenario 3: URL/exception rules, strict GenAI, metric integer and nested value precision, bounded output | Existing signal owners retain metric-kind/type paths; trace last-exception strings and any-exception boolean are separate; log boolean uses presence including empty string; native trace/log checks reuse those extractors | maximal signal tests and `native_canonical_signals_refuse_forged_derivations`; exact Variant and overflow/expanded-output tests retained | PASS |
| Scenario 4: OTLP HTTP protobuf/JSON and gRPC retain partial-success and request-context refusals; labels never establish ownership | AuthContext parses environment independently; Gate validates scope/registry references and resolves frame dimensions before Scribe; request environment overrides promoted resource environment only when present | OTLP negative cases and existing transport smoke journeys; application-owned OTel Python/TS cases use standard providers/exporters | PASS |
| Scenario 4: public native admission refuses forged derived attribution on every public built-in (REQ-024,051–052; AC-007,024; INV-001–003) | Canonical signal validators validate promotions, but public drift observations inherit the shape-only validator and retain supplied environment/service values through stamping | New native journey covers only spans; no public drift dimension-spoof assertion | **FAIL — BEH-002** |
| Scenario 4: subject/Run batching remains independent, roots/auth do not leak, native replay preserves accepted context | Queue still carries subject/Run per row and immutable client context per producer; Gate OTLP identity now includes root/environment; public native managed root/principal input remains refused | native changed-owner replay test; existing queue retry/atomicity tests retained; configured SDK context journeys recorded | PASS |
| Scenario 5: verification inputs/results/drift features and audit retain originating pair, binding owner/subject, environment and request (REQ-028,034,049–053; INV-001–005,007) | ResultRun carries exact owner/subject/environment; rows leave descriptor fields to registry UID resolution; row-attributed outbox keeps carried owner/pair/request; audit keeps its original pair/sentinel/request and separate target | `trusted_delivery_preserves_each_origin` stages two origins for one Agent through production builders; peer-runner and audit identity journeys; recorded verification_runtime lane | PASS for these producers |
| Scenario 5/7: gateway/internal capture retains the same accepted origin on provider attempts and trusted delivery (REQ-028,051–052,055–056) | Gateway calls batch carries context, but CallCapture attempt-span batch never carries it | Existing configured gateway test queries only calls; its success does not cover attempt-span attribution | **FAIL — BEH-001** |
| Scenario 6: hot/recovered/published/compacted values, dynamic correlation, and renamed specialized evidence readers remain exact (REQ-024,026,039–040; AC-007,008,014) | Lifecycle carries stamped rows; group owner/subject predicates and result/drift/trace consumers use renamed columns without adding a time window or changing evidence predicates | `canonical_signal_rows_survive_publication_recovery_and_compaction` reads complete rows and compares after each transition; group/runner/client read regressions retained | PASS |
| Scenario 7: gateway credential modes and requested UID/environment context validate per request before dispatch; Batch invocations retain HTTP context | requested_context rejects duplicate/malformed headers; requested subject is authorized before provider execution; root comes from verified caller; Batch request owner now retains supplied context; streamed settlement retains admitted call | `configured_gateway_headers_are_authorized_and_captured` includes no-provider-on-refusal and calls-row assertions for OpenAI, Anthropic and upload/create Batch operations; Python/TS state adapter readbacks | PASS for call rows; attempt rows fail above |
| Contract closure, authority amendments, SDK thinness, fresh baseline and task non-goals (REQ-027,031–032; AC-009–010; INV-006) | Changed Rust/Python/TS public names and examples use bifrost and subject_card_uid; server owns validation/storage; no extra ingestion writer, registry join, compatibility reader, partition default or trace semantics introduced | Recorded codegen/SDK typing/napi/dependency/tenant/docs/format/lint checks; current source declarations inspected | PASS, subject to separate mandatory-standards report |

## Material findings

### BEH-001 — INCORRECT: gateway attempt spans lose admitted application context

Obligation: REQ-028 and REQ-051 carry truthful workload/subject context through
internal capture and all telemetry; REQ-052 explicitly preserves a verification
gateway call's accepted Service/subject, independently of provider/Verifier;
REQ-055 preserves context through gateway attempts. TASK-003 scenarios 5 and 7
require retained originating gateway context through capture and delivery.

Primary location: `crates/wyrd/wyrd-server/src/components/gateway/capture.rs:438`
(`CallCapture::spans_batch`), with the omission also visible in
`otlp_spans` at line 403. This is adjacent to the changed `calls_batch` context
projection at lines 375–385; both outputs are produced by `CallCapture::project`
at line 482.

Reachable path and evidence:

1. A normal accepted configured Agent call, public or verification-triggered,
   has `GatewayCallContext` with workload, subject, Run and environment.
   `GatewayInvocation::facts` and `CallCapture::from_facts` retain it.
2. `calls_batch` reads that context. In contrast, `otlp_spans` reads only
   provider attempt fields and trace parent; it never reads `self.subject` or
   inserts subject/Run correlation attributes.
3. `spans_batch` combines all captures under a synthetic resource containing
   only `service.name = GATEWAY_SERVICE`. It projects that resource through
   `scribe_outbox::project_spans` with no Card scope. No workload or environment
   column is added after projection.
4. `Chunk::attributed` only adds original principal pairs (and request identity
   specially for calls). `ScribeBatch::into_frame` derives descriptors from UIDs
   actually in IPC; the span batch contains no workload/subject UID. In
   `managed_arrays`, row attribution without a carried workload stamps null.
   `stamp_dimensions` keeps the synthetic observed Service and null environment
   because there is no workload descriptor or accepted environment.

Observable consequence: the accepted call row correctly names the application's
Service and Agent, while its retained provider-attempt spans have null
workload/subject/Run/environment and synthetic gateway service_name. Service,
subject and environment telemetry filters therefore omit the attempts, including
gateway-backed verification. Mixed captures cannot reconstruct their origins
later. This is ordinary configured gateway use, not an unusual input.

Sibling paths checked: public OpenAI/Anthropic/native routes, Batch invocations,
direct verification `GatewayCallContext`, queued runner context, buffered and
stream settlement all converge on CallCapture. The outbox's separate server-span
tenant-copy producer also calls `project_spans`; it must retain its existing
behavior. The defect is the gateway producer failing to attach the context, not
a missing generic Scribe inference rule.

Minimum safe correction: extend the existing CallCapture attempt projection to
carry each attempt's own admitted workload/subject/Run/environment into its
canonical row, repeating that capture's context for each attempt. Reuse existing
correlation projection, row-attributed managed stamping and UID descriptor
resolution. Preserve the synthetic raw resource where required, trace linkage,
principal pairing and attempt order. Do not infer origin in Scribe, split the
capture pipeline, or add a writer. A local projection change is sufficient;
neither new configuration nor a new durable owner is justified.

Closure proof: extend the existing gateway journey to query attempt spans by
gateway_call_id for two differently attributed calls sharing a flushed outbox;
assert exact workload/subject UIDs, Service/subject dimensions, environment, Run,
original pair and trace. Include one verification-triggered or streaming call
through the same capture owner. A focused projection assertion can cover multiple
attempts without a new harness.

### BEH-002 — INCORRECT: public drift rows can supply canonical dimensions

Obligation: REQ-024 applies the same trust boundary to native admission; REQ-051
makes system-table identity descriptors server-derived; REQ-052 says public
managed descriptors cannot be forged before ACK. TASK-003 explicitly requires
public forged managed/derived values to fail before ACK, and scenario 4 includes
native Arrow whole-batch refusal. Resource fallback is approved for telemetry,
not for arbitrary drift row fields.

Primary location: `crates/vala/vala-bifrost-redux/src/tables/drift/observations.rs:26`
and inherited `tables/mod.rs:388` (`validate_predeclared`); preserving the forged
values occurs in `scribe/execution_lanes.rs:1278` (`stamp_dimensions`).

Reachable path and evidence:

1. An authorized public native writer can write `bifrost.drift.observations`;
   Gate reserves the verification output tables and gateway calls, not this
   observation table. Its described schema now requires six nullable dimension
   columns. The ordinary client drift projector fills them with null.
2. A native caller supplies the same valid Arrow schema, valid subject and
   feature row, but sets `deployment_environment = 'prod'` in the row while
   omitting the environment metadata. Use a normal selected Service root; no
   admin-only privilege or fabricated UID is needed.
3. `ObservationsTable` has no custom CANONICAL_VALIDATOR, so it inherits
   `validate_predeclared`. That verifies declared shape, Variant and whole
   structs only. `decode_rows` does not reserve these six descriptive fields.
4. Scribe gets `FrameDimensions.environment = None` from the authentic request.
   `stamp_dimensions` replaces environment only for Some(request environment),
   so it retains the caller's 'prod'. Workload and canonical Service are valid,
   making this an apparently owned production observation after ACK. For a
   rootless caller the same path also retains arbitrary service_name/version.
   Forged subject descriptors on this table are silently overwritten instead
   of refused; they share the same absent source validation.

Observable consequence: a public row can enter an owned environment filter
without that environment ever being accepted as request context. The table has
no raw OTel resource from which such a value could be derived. Public native
derived values therefore bypass the required pre-ACK refusal. This directly
affects the newly required cross-table Service/environment filtering contract.

Sibling paths checked: canonical span/log/metric validators already reproduce
their own source-derived values and refuse differences; native spans have the
new negative journey. Gateway/results/features/audit are internal destinations
whose carried dimensions must remain usable. Dynamic tables deliberately do
not gain mandatory dimensions. The shared stamp fallback is valid for telemetry
and trusted row origins, so deleting it globally would break approved behavior.

Minimum safe correction: make the public drift-observation source contract
reject non-null caller-supplied shared-dimension placeholders before stamping,
using the existing table validator/admission boundary. The server then derives
identity fields and obtains environment solely from accepted context. Keep
trusted row-attributed origins and telemetry resource fallback intact. No new
schema, credential, parser or second validation pipeline is needed; a narrow
public-table validator check is sufficient.

Closure proof: extend the existing native admission journey with a correctly
shaped drift batch setting each dimension, including a selected-root batch with
forged environment and no environment metadata. Assert one exact whole-batch
refusal code before ACK and no durable rows. Retain positive public drift
readback with null placeholders and valid environment metadata, plus existing
OTLP fallback and trusted mixed-origin regressions.

## Evidence diagnoses and review limits

The recorded result-Bloom diagnosis matches the revised complete defaults: the
old principal-column floor is absent by approved design, so changing that test
to the declared ledger does not weaken the new contract. The observation schema
diagnosis matches ConformPlan's requirement for every unreserved registered
field; using the owning ObservationsTable fields is the correct fixture repair.
The MCP attribute diagnosis matches native promotion validation requiring raw
source keys to accompany derived GenAI values. The lifecycle setup repair
creates multiple files for the existing small-files strategy and retains exact
readback comparisons; no timeout or production compaction policy was changed
to clear it. I inspected those diagnoses against source; I did not rerun their
failed or green commands or claim independent access to diagnostician sessions.

No optional refactors or speculative hardening are proposed. UID descriptor
lookup at delivery is accepted here as equivalent for immutable registered
descriptors on the inspected successful path; this report does not establish
registry-disappearance recovery, which belongs to the selected lifecycle/system
review. Mandatory standards and persistent-state specialists retain their own
coverage; this behavior result does not waive their gates.

Skipped runtime reruns and unrelated optimization work. The remaining risk is
accepted rows losing gateway origin or accepting forged drift dimensions until
BEH-001 and BEH-002 are independently validated and closed.
