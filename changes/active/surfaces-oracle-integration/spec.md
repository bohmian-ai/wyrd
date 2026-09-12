---
id: SPEC-surfaces-oracle-integration
revision: 7
status: approved
---

# Surfaces and Oracle integration

## Human intent and user value

Produce one coherent Wyrd codebase by integrating the refreshed
Oracle/Bifrost work into the Surfaces integration branch without losing either
side's authoritative behavior.

The result must preserve Surfaces as the authority for Wyrd outside Bifrost,
preserve Oracle as the authority for Bifrost, and resolve overlapping client,
server, contract, SQL, testing-harness, CI, and generated surfaces as one
architecture rather than as parallel implementations.

## Authority inputs and working checkpoints

- Integration worktree:
  `/Users/stevenforrester/Documents/GitHub/wyrd-bifrost-surfaces`
- Integration branch: `change/surfaces-oracle-integration`
- Rewritten Surfaces parent and non-Bifrost authority:
  `surfaces-python@fde37011ad056cfca5b0039b3716fb9e1b7f8c62`
- Revision 5 destination checkpoint:
  `change/surfaces-oracle-integration@2c5f107ff1613d201686b0cead9bf0ab4c900a26`
- Revision 5 Oracle audit checkpoint:
  `oracle-distributed@b088ca91af4c98ee66458d76988d1b21006b5c36`
- Incorporated Forge source tip:
  `forge-compaction-refactor@351902b0855c69849a88e7a76c9e596a666f716d`
  (already an ancestor of the Oracle checkpoint through integration commit
  `d3888ddae` and therefore not a second merge input)
- Superseded Oracle checkpoint:
  `merge-input/oracle-distributed-20260902@b9d7d0cafafc421b4215917391d8827a672e6dc3`
- Superseded Oracle pin:
  `merge-input/oracle-distributed-20260901@25a3aa94e7bb782c21d41e7c5da3c0e97f8ea77a`
  (historical comparison evidence only)
- The archived first merge attempt is reference evidence only.
- The Oracle checkpoint is the immutable input audited for revision 5.
- If `oracle-distributed` advances before task decomposition or merge
  execution, a later draft revision MUST name one new clean committed
  checkpoint and audit only the delta from this checkpoint. Perpetually moving
  refs are not merge inputs.

## Scope

- Reconcile every conflicting or semantically overlapping change between the
  final refreshed authority inputs.
- Build the exhaustive conflict ledger used to derive implementation tasks and
  govern merge resolution.
- Preserve Surfaces Card, registration, reference, Eval, Drift, Python,
  `WyrdState`, and general Wyrd behavior.
- Preserve Oracle Bifrost, Scribe, Oracle, integrated Forge, WAL, canonical
  telemetry, distributed query, deployment, production-harness, and telemetry
  behavior that conforms to current architecture.
- Delete the legacy `vala-bifrost` crate and every consumer completely;
  `vala-bifrost-redux` is the sole Bifrost engine authority.
- Treat the refreshed Oracle `wyrd-client` and `wyrd-queue` implementations as
  authoritative over their Surfaces counterparts, subject only to the final
  Wyrd-owned SDK boundary and preserved non-Bifrost user workflows.
- Converge the public Rust, Python, and TypeScript client surfaces on one
  Wyrd-owned client implementation.
- Converge public errors, audit behavior, SQL fixtures, generated artifacts,
  and GitHub Actions on the reconciled architecture.
- Complete the retained audit-history lifecycle, which the refreshed Oracle
  input still does not wire end to end.
- Complete the single Bifrost data-root outcome in a required follow-up task
  after the core Redux integration and before this change may complete.
- Produce a reviewable integration result and merge evidence on the dedicated
  integration branch.

## Non-goals

- Merging, pushing, releasing, or deploying the integration result to `main`.
- Rebasing or modifying either source branch or the active `wyrd`
  worktree.
- Performing the older multi-repository decomposition described by the
  referenced decomposition plan. This change adopts only its single
  SDK-facing Wyrd client boundary.
- Compatibility aliases, legacy routes, obsolete Card kinds, or parallel
  client APIs.
- A second Bifrost optimizer, query scheduler, materialized shuffle service,
  external-system write path, or client-owned durable behavior.
- Expanding the v1 Card catalog or making `External` registrable.
- Extracting every existing optional Python feature from otherwise untouched
  owner crates solely to achieve directory purity.
- Restoring Oracle's deleted Bifrost benchmark/qualification harness layer,
  removed public testing APIs, or retired benchmark CI tasks.
- Restoring the legacy `wyrd audit verify`, admin audit-integrity, or admin
  audit-verification surfaces. A future verified need may introduce a new
  capability over the current retained audit architecture.
- Restoring `wyrd dev bootstrap` or another CLI-owned path that connects
  directly to Postgres, runs migrations, and provisions durable server state.
- Retaining compiled native SDK build outputs as tracked source artifacts.
- Preserving any `vala-bifrost` compatibility crate, re-export, feature,
  dependency, import, test, benchmark, route, schema, migration owner, or
  documentation alias.
- Adding a live Oracle query workspace to the developer UI. Existing UI work
  may later project the reconciled server contract through its BFF; fixture-only
  Observe screens are not evidence for this integration.
- Running implementation verification while this specification is being
  drafted or approved. Verification belongs to later implementation tasks.

## Definitions

- **Surfaces authority:** the fixed `surfaces-python` input for all Wyrd
  behavior except Bifrost-owned behavior.
- **Bifrost authority:** the revision-pinned Oracle input for Scribe, Oracle,
  Forge, analytical storage, WALs, query execution, maintenance, and
  production analytical reliability.
- **Shared client implementation:** `crates/shared/wyrd-client`, the sole
  SDK-facing Wyrd client API and owner of the composed client capabilities.
- **Language SDKs:** `sdks/wyrd-sdk-rust`, `sdks/wyrd-sdk-python`, and
  `sdks/wyrd-sdk-ts`.
- **Audit staging:** `vala.audit_staging`, transient transactional write-ahead
  state awaiting retained publication; it is not an external-consumer outbox.
- **Retained audit history:** tenant-qualified
  `vala.system.audit_log` in Bifrost.
- **Semantic overlap:** behavior changed by both branches, including changes
  Git can merge without textual conflict.
- **Canonical signal tables:** `vala.traces.spans`, `vala.logs.records`, and
  `vala.metrics.points`, the sole durable OpenTelemetry signal tables.
- **Legacy Bifrost:** the entire Surfaces `crates/vala/vala-bifrost` package and
  every `vala-bifrost`/`vala_bifrost` consumer or compatibility surface.
- **Bifrost Redux:** `crates/vala/vala-bifrost-redux`, the sole retained
  Bifrost engine and implementation authority.
- **Conflict ledger:** the exhaustive machine-readable disposition of textual
  conflicts, clean two-sided overlaps, and one-sided semantic coupling at
  `.dev/merge-audits/surfaces-oracle-integration/review-ledger.json`, summarized
  by `merge-inventory.md` and a revision-aligned `decision-summary.md` in that
  directory.

## Required behavior

### Integration authority

- **REQ-001:** The integration result MUST preserve the named Surfaces parent
  ancestry and reconcile the revision-pinned Oracle checkpoint as its sole
  Bifrost merge input. The incorporated Forge tip MUST NOT be merged again.
- **REQ-002:** Every textual conflict and semantic overlap MUST be resolved
  against the owning architecture and recorded authority; a clean Git merge
  MUST NOT be treated as semantic proof.
- **REQ-003:** When compatible changes affect the same capability, the result
  MUST preserve both. Surfaces MUST win outside Bifrost and Oracle MUST win
  inside Bifrost except for the typed scope, shared credential, Python error,
  SDK behavior, testing, and generated-contract decisions explicitly adopted
  by this revision. Any other material conflict requires a later human-approved
  specification revision.
- **REQ-060:** The refreshed Oracle versions of `wyrd-client` and `wyrd-queue`
  MUST supersede the Surfaces implementations rather than being textually
  combined with them. Oracle's bounded producer ownership, Arrow material,
  sink settlement, backpressure, drain, credential, TLS, transport, and
  catalog-backed error behavior MUST survive. Surfaces-only implementations,
  configuration shapes, aliases, and tests MAY survive only when another
  requirement in this specification explicitly preserves their observable
  non-Bifrost behavior.
- **REQ-061:** Oracle's current Bifrost client behavior, presently implemented
  in `vala-sdk`, MUST be composed into `wyrd_client::Bifrost` as required by
  REQ-016 through REQ-021. Treating Oracle `wyrd-client` as authoritative does
  not permit `vala-sdk`, `QueryClient`, or a transport type to remain a sibling
  SDK-facing client owner.

### Wyrd contracts and registration

- **REQ-004:** Card registration MUST preserve Surfaces' composite lifecycle,
  including multi-submission DAG resolution, inline child handling,
  registration-only sibling rewriting, server-derived relationships, and
  atomic Postgres registration behavior. Blob persistence that occurs after
  commit MUST retain its existing reconciliation semantics and MUST NOT be
  represented as part of the database transaction.
- **REQ-005:** Reference slots MUST use `Ref` or `InlineableRef<T>`; authored
  paths are loader inputs and `Sibling` is registration-only. No selector
  reference variant or separate version-requirement field may be introduced.
- **REQ-006:** `CardRef` MUST contain `kind`, `name`, one `version`, optional
  `space`, and optional `uid`.
- **REQ-007:** Eval and Drift Cards MUST remain reusable and subject-less.
  Deployment composition MUST bind them through versioned `publishes_to`, and
  runtime observation identity MUST identify the subject.
- **REQ-008:** The v1 public catalog MUST contain the 16 registrable native
  kinds defined by architecture. `External` MUST remain a non-registerable
  foreign-schema discriminator without `ExternalSpec`; `Tool`, `Skill`, and
  `SubAgent` MUST NOT become Card kinds.
- **REQ-009:** An Audit Card MUST remain an immutable investigator-created
  case file distinct from transactional audit events and retained audit
  history.

### Bifrost and deployment

- **REQ-010:** The result MUST conform to `architecture/bifrost-design.md` and
  preserve compatible refreshed-Oracle Scribe, Oracle, and integrated Forge
  behavior, durability transitions, resource ownership, recovery rules,
  readiness, and production telemetry.
- **REQ-011:** Each Bifrost physical analytical table MUST be identified by
  `(tenant, logical table)`. Shared physical analytical tables MUST NOT exist.
- **REQ-012:** The only deployment targets MUST be `all`, `server`, `oracle`,
  `scribe`, and `forge-worker`. Gate MUST remain the server dispatcher and MUST
  NOT become a deployment role or client.
- **REQ-013:** One logical Wyrd server MUST support one or more tenants across
  self-hosted, multi-tenant SaaS, and single-tenant enterprise deployment
  topologies without weakening tenant isolation.
- **REQ-014:** Scribe MUST preserve Oracle's pod-local WAL v6 and its
  acknowledgement, replay, staging, publication, and retirement boundaries.
  Oracle MUST preserve its separate local read-audit WAL and fsync acceptance
  before returning query rows.
- **REQ-015:** A selected analytical query MUST have one server execution
  attempt. Post-selection peer, transport, resource, cancellation, deadline,
  or execution failure MUST terminate the stream without a server successor
  attempt or interactive fallback.

- **REQ-048:** Oracle MUST build each query once through the pinned distributed
  planner. A normal DataFusion root selects Interactive execution and a
  `DistributedExec` root selects Analytical execution. No heuristic candidate
  phase, operator allowlist, second physical build, or pre-selection fallback
  may survive. Planning, codec, and worker incompatibility MUST be a structured
  terminal failure. One bounded full-cut reprepare after catalog promotion MAY
  occur before source IO; it is not a successor execution attempt.

- **REQ-049:** Oracle admission MUST be pod-local. Each pod MUST own bounded
  Interactive and Analytical queues, FIFO ordering within each tenant,
  equal-weight rotation across ready tenants, a protected Interactive floor,
  and one hard aggregate governed DataFusion memory root shared by leader and
  follower operators and exchanges. CPU parallelism, concurrency, query
  memory ceilings, scratch, and selected-worker limits MUST remain distinct.
  PostgreSQL admission policies, allocations, leases, renewals, and overdraft
  MUST NOT participate. Replicas MAY have different local capacities and no
  response or metric may promise a cluster-wide tenant quota.

- **REQ-050:** Oracle MUST establish one fenced renewable reader epoch per
  process and durably protect each tenant-qualified table and snapshot ancestry
  before snapshot-dependent source IO. Protection MUST cover lazy streams,
  backpressure, distributed descendants, cancellation, deadline, failure, and
  cleanup until all protected IO joins. Protection widening and Forge snapshot
  expiration preparation MUST serialize per tenant-qualified table. Lease loss
  MUST close admission, remove readiness, self-fence source IO, cancel and join
  protected work, and only then release protection. Missing, stale,
  contradictory, or unreadable authority MUST make expiration and deletion
  fail closed.

- **REQ-051:** Forge MUST preserve the integrated maintenance authority:
  Scribe promotion, compaction, snapshot expiration, expired-object cleanup,
  and orphan cleanup are independent production protocols. Ordinary
  compaction plans publish independently so one sibling failure does not erase
  committed sibling progress. Definite catalog conflicts use the retained
  one-, two-, and four-second retry schedule, at most three retries after the
  initial commit attempt and always within the original deadline; ambiguous
  acceptance retains the same durable operation for reconciliation without
  replaying the uncertain catalog call.
  Readiness MUST be withheld or retracted while durable authority is
  unresolved.

- **REQ-052:** Forge plan admission MUST use the retained strict worker-local
  FIFO over bounded pending/running parallelism and aggregate estimated running
  memory. Forge uses DataFusion's unbounded pool without Forge spill or scratch;
  the estimate is admission control rather than a hard allocation guarantee.
  Dedicated and co-located targets MUST retain their approved memory budgets
  and protected role floors without introducing a second scheduler or resource
  root.

- **REQ-053:** The canonical OpenTelemetry persistence contract MUST use only
  the three canonical signal tables. Trace events and links MUST remain nested
  in their span row, and GenAI data MUST remain in its originating signal row;
  physical trace-child and `vala.genai.*` copies MUST NOT survive. The table
  owner is the single schema, field, semantic projection, fingerprint, Arrow,
  Parquet, and Iceberg authority. OTLP and canonical Arrow writes MUST converge
  before Scribe WAL preparation. Each OTLP record is atomic; mixed-validity
  requests persist complete valid siblings and return exact standard partial
  success, while request-wide trust, size, admission, or durability failure
  persists no authoritative partial batch.

- **REQ-053A:** Every accepted telemetry row MUST carry the authenticated
  non-null `principal_id`. `card_ref` and `run_id` are optional per-row
  correlation. A present Card identity MUST be authorized and resolved to its
  server-trusted UID from the bounded signed scope mapping without an ingest-
  time registry lookup; a client UID is never trusted. OTLP correlation comes
  only from the final record-level `wyrd.card_ref` and `wyrd.run_id` attributes
  while every source attribute remains losslessly stored. Replay identity MUST
  preserve publisher and correlation attribution.

- **REQ-054:** `Permission` MUST carry required typed scope. The closed initial
  scopes are `All`, Bifrost schema identity, and exact Bifrost table UID plus
  canonical catalog/schema identity. Scope-less or invalid persisted input MUST
  fail without a compatibility decoder. Oracle MUST authorize every table in
  the resolved logical scan set before physical planning, resource admission,
  read-audit acceptance, peer dispatch, or source IO. One uncovered direct,
  joined, or expanded table denies the whole query without rows, and the
  distributed permission digest MUST bind the exact approved object set.
  Scope never selects tenancy; complete table permission governs all columns.

- **REQ-055:** The server MUST derive every Bifrost-managed local path from
  `WYRD_BIFROST_DATA_DIR`, defaulting locally to `.wyrd/bifrost`. It MUST create
  required Scribe and Oracle paths before role activation and fail readiness if
  the root is unusable. `WYRD_SCRIBE_WAL_DIR` and the independent Oracle
  audit-WAL-root setting MUST be removed without aliases. Durable replicas MUST
  not share one writable WAL identity. Forge has no spill or scratch child
  path.

- **REQ-055A:** REQ-055 MUST be delivered as a required follow-up task after
  the core Bifrost Redux integration. The task MAY depend on the integrated
  server, Scribe, Oracle, and deployment owners, but this change MUST NOT be
  considered complete until the task is implemented, reviewed, and its
  acceptance evidence passes.

- **REQ-062:** `vala-bifrost-redux` MUST replace legacy `vala-bifrost` in full.
  The legacy package directory, workspace membership, dependency key, crate
  imports, feature edges, source, tests, benchmarks, examples, migrations,
  routes, schemas, generated artifacts, documentation, checks, and CI selection
  MUST be deleted or redirected to the owning Redux/server contract. No
  compatibility crate, facade, re-export, alias, or dual-engine feature may
  remain. Required audit-history behavior MUST be implemented against Redux;
  the legacy relay, sealing, derivation, and typed-observation machinery MUST
  NOT be ported merely to preserve deleted code.

- **REQ-063:** Bifrost Redux is the complete engine authority, including Gate,
  Scribe, Oracle, integrated Forge, catalog, canonical tables, query execution,
  persistence, maintenance, telemetry, and recovery. For those owners, Redux
  files are retained or adapted to current repository boundaries; legacy
  `vala-bifrost` code is never used as conflict-resolution input.

### Client and SDK topology

- **REQ-016:** `wyrd-server` MUST own all server logic and durable behavior.
  `wyrd-client` MUST own the shared client implementations and expose the sole
  SDK-facing Wyrd client API.
- **REQ-017:** `wyrd-client` MUST compose and publicly expose the client
  capabilities required by the three SDKs, including authentication and
  transport, Cards, `WyrdState`, and Bifrost. Canonical Bifrost SQL is the sole
  observation-read contract; removed typed trace, log, metric, GenAI, Eval,
  Drift, and agent-trace read routes, RPCs, and SDK methods MUST NOT be restored.
- **REQ-018:** `sdks/wyrd-sdk-rust`, `sdks/wyrd-sdk-python`, and
  `sdks/wyrd-sdk-ts` MUST be thin, idiomatic language projections over
  `wyrd-client`. They MUST NOT import Wyrd domain-client implementation crates
  as alternate public authorities or independently implement HTTP, gRPC,
  validation, registry, storage, or lifecycle behavior.
- **REQ-019:** Bifrost client behavior MUST be exposed through
  `wyrd_client::Bifrost`, including table management, buffered ingestion,
  canonical Arrow batch writes, table description, SQL and caller-supplied row
  schema projection, query streaming, and query lifecycle. An internal
  `query_client` accessor MAY expose advanced lifecycle mechanics without
  becoming a separately constructed public owner. `QueryClient` and
  `BifrostGrpcTransport` MAY remain private mechanics but MUST NOT remain
  sibling public clients.
- **REQ-019A:** `ClientConfig::credential` MUST be the single explicit shared
  credential field. Per-client `api_key` configuration names and compatibility
  aliases MUST NOT be restored.
- **REQ-020:** The Python SDK MUST preserve the public `wyrd.cards`,
  `wyrd.data`, `wyrd.model`, `wyrd.prompt`, `wyrd.state`, and
  `wyrd.bifrost` surfaces. Its final PyO3 and package aggregation MUST live in
  `sdks/wyrd-sdk-python`; private `_wyrd` topology MUST NOT become a second
  public hierarchy. `wyrd.bifrost` MUST retain synchronous and asynchronous
  table, buffered/direct Arrow write, flush/shutdown, SQL, streaming,
  lifecycle, description, typed-row, PyArrow, Polars, pandas, and Arrow-IPC
  behavior with terminal-safe iterator settlement.
- **REQ-021:** The TypeScript SDK MUST preserve Oracle's implemented
  `@wyrd/sdk` behavior while moving its package and private N-API boundary to
  `sdks/wyrd-sdk-ts`. It MUST project `wyrd-client`, not assemble independent
  Bifrost transports. Its retained Bifrost surface includes Arrow/JSON-Schema/
  model-derived table declarations, buffered and direct Arrow writes, raw and
  typed SQL, Arrow-native streaming, lifecycle calls, description, structured
  errors, and Arrow IPC conversion.

- **REQ-056:** HTTP and gRPC query streams MUST carry exactly one schema frame,
  zero or more batch frames, and exactly one closed success, degraded-success,
  or failure terminal under one request and attempt identity. Successful
  completion MUST agree on row
  count and Arrow EOS; duplicate or missing schema, frames after terminal,
  missing terminal, malformed EOS, mismatched identity, broken EOF, or row-count
  mismatch MUST be an incomplete-stream failure. Dropped callers MUST trigger
  bounded cancel/drain or lifecycle settlement. Callers MUST NOT select tenant,
  execution class, path, topology, or plan.

- **REQ-057:** The server-owned `/mcp` surface MUST expose exactly
  `bifrost.list_tables`, `bifrost.describe_table`, and `bifrost.query` for this
  capability. Query collection MUST be bounded by closed row and JSON-byte
  limits and MUST return a complete validated result or structured failure,
  never successful truncation or partial rows. Removed static permissions and
  error-catalog tools MUST NOT be restored.

- **REQ-058:** `wyrd query` MUST preserve one-of `--sql`/`--file`, visibility,
  freshness, JSONL/Arrow output, stdout row data, stderr terminal metadata, and
  structured failure semantics while consuming `wyrd_client::Bifrost` rather
  than a sibling query client.

- **REQ-059:** Generated OpenAPI and protobuf authority MUST include the public
  Bifrost table registration/list/description routes, query and lifecycle
  operations, streaming media type, typed permissions, and structured errors.
  The refreshed Oracle checkpoint's omission of table routes from OpenAPI is
  implementation drift, not the target contract.

### Errors

- **REQ-022:** `WYRD_REGISTRY_*` MUST be the sole stable registry error-code
  prefix. `WYRD_REG_*` spellings MUST be removed without aliases.
- **REQ-023:** Public Rust errors MUST preserve the Surfaces general catalog,
  `WyrdProblem`, and storage reconstruction together with Oracle's typed
  Bifrost catalog and HTTP/gRPC reconstruction under the derive-backed error
  authority.
- **REQ-024:** Python MUST expose one structured `WyrdError` root. Expected
  Wyrd-owned failures MUST expose direct `code`, `message`, `detail`, `details`,
  `remediation`, `status`, `title`, and `type` attributes derived from the
  catalog-backed problem projection. The exception MUST NOT expose an
  aggregate `problem` attribute. RuntimeError-based Bifrost exception classes,
  owner-local metadata projectors, generic Python exceptions for Wyrd-owned
  failures, and fake `Cfg*` aliases MUST be removed. Intrinsic interpreter,
  binding, import, and deliberately preserved callback failures remain
  idiomatic Python exceptions.
- **REQ-024B:** Python MUST NOT restore `BifrostQueryError`,
  `IncompleteQueryStreamError`, or `NoCredentialsError`. Centrally selected
  domain subclasses MAY remain only when they inherit `WyrdError` and receive
  the same eight-field projection without a parallel metadata authority.
- **REQ-024A:** Every stable failure reachable through a public Python
  operation MUST have a derive-backed catalog entry and retain its safe
  structured metadata rather than degrading to `WYRD_INTERNAL_500`. Existing
  optional owner-crate adapters MAY remain migration state only when enabled
  and aggregated exclusively by `wyrd-sdk-python`; the final public projection
  belongs to the Python SDK boundary.
- **REQ-025:** TypeScript MUST expose a runtime `WyrdError` and a generated
  literal `WyrdErrorCode` union derived from the same stable catalog.

### Audit history

- **REQ-026:** Every authorization decision that evaluates a principal's
  permission MUST append exactly one canonical audit event at that receiving
  boundary before an allowed operation proceeds or a denial is returned. Both
  allowed and denied decisions are audited, and audit unavailability fails the
  operation closed. Engine-internal Scribe, Forge, Oracle reader-protection,
  and retained-publication transitions evaluate no permission and MUST remain
  lineage rather than audit. Oracle query reads are the sole WAL-first
  exception and MUST relay accepted events into the same tenant staging chain
  at least once.
- **REQ-026A:** One logical Oracle query MUST produce one read-audit event and
  distributed stages MUST produce none. The relay MUST checkpoint only after
  its Postgres commit. Commit-before-checkpoint replay MAY produce one valid
  duplicate but MUST NOT lose the accepted event. Verified delegation MUST be
  stored initiator-first, survive WAL framing and replay, and remain hash
  covered; an empty delegation chain MUST remain omitted from canonical bytes.
- **REQ-026B:** Object-scoped denial MUST be durably audited before refusal is
  returned; audit failure MUST preserve the denial and surface audit
  unavailability. Peer-ticket rejection before verified tenant decoding MUST
  use the system audit tenant, while a verified signed-ticket violation MUST
  use its authenticated tenant chain. Query cancellation MUST preserve its
  pre-dispatch authorization audit and MUST NOT create or rewrite an operation-
  result audit outcome.
- **REQ-027:** A bounded publisher MUST move tenant audit events idempotently
  from `vala.audit_staging` directly through a local Scribe into
  `vala.system.audit_log` and the existing Forge path without passing through
  Gate or generating another audit event. The publisher MUST run only in a
  process that owns a local Scribe.
- **REQ-027A:** The per-tenant `entry_hash` MUST be reproducible from the
  retained event content, sequence, and preceding retained `entry_hash`.
  Staging-only or deleted fields MUST NOT be part of its canonical preimage.
  The chain MUST NOT introduce another table, service, key, or publication
  stage.
- **REQ-027B:** Retained audit history MUST preserve the effective dynamic
  permission alongside principal, operation, resource, `allowed | denied`
  outcome, request and trace correlation, redacted detail, sequence, and chain
  hash. The original server-stamped decision time MUST become
  `wyrd_event_time`; audit backlog publication is exempt from the ordinary past
  event-time admission window; and `vala.system.audit_log` partitions daily.
  Audit state has not shipped: edit its existing schema definition in place.
  Do not add a migration, compatibility path, or backfill.
- **REQ-027C:** Before publication leaves Postgres, the publisher MUST durably
  freeze one contiguous upper sequence bound for the tenant. Concurrent and
  restarted publishers MUST reuse that bound and therefore the same batch
  identity until it settles; rows appended above it wait for the next batch.
  Publication progress consists only of the monotonic watermark and at most one
  in-flight upper bound per tenant.
- **REQ-028:** After a range is durably published, one tenant-scoped Postgres
  transaction MUST advance its monotonic watermark and garbage-collect every
  staged row through that watermark. No grace tail remains; an idle tenant's
  staging rows drain to zero.
- **REQ-029:** Audit recovery MUST safely retry events whose publication,
  watermark, or garbage-collection outcome is uncertain without duplicating
  retained audit history or losing staged rows.
- **REQ-030:** Neither a legacy direct-Iceberg relay nor a second audit table
  may survive. The unused Postgres `platform.audit_log` table MUST be removed
  from the greenfield migration baseline.
- **REQ-030A:** `vala.forge_operation_state` MUST be Forge's self-contained
  operational recovery and lineage authority. Forge MUST NOT require an audit
  payload or audit sequence when listing open or reset operations or when
  rereading locked transition state. No additional recovery digest, identity
  column, or recovery table is introduced.

### Postgres harness and CI

- **REQ-031:** Repository-managed Postgres verification MUST use Oracle's
  isolated per-test database model. Only `wyrd_test_admin` may create or drop
  those databases; `wyrd_migrator` owns schema migration but not database
  lifecycle; attached child fixtures MUST be non-owning.
- **REQ-032:** `wyrd-testing` MUST preserve the refreshed Oracle production-
  shaped, isolated-Postgres capability journeys for SDK, Forge, Scribe, Oracle,
  OTLP, server, MCP, Python, and TypeScript behavior. Private fixture names are
  not contracts. Files, features, public testing APIs, fixtures, scripts, docs,
  and `mise` tasks deliberately deleted by the refreshed Oracle cleanup MUST
  NOT be restored from Surfaces or a superseded Oracle pin.
- **REQ-033:** Pull requests MUST run only verification lanes selected by the
  affected code and its dependency closure. A generic Rust change MUST NOT
  automatically run the complete repository aggregate.
- **REQ-034:** The complete non-credentialed correctness suite MUST run
  nightly against `main`. Live-cloud and performance or qualification suites
  MUST run in their separate GitHub Actions workflows.
- **REQ-035:** Required-check aggregation MUST remain stable when unaffected
  lanes are skipped and MUST fail when any selected required lane fails.
- **REQ-035A:** The Bifrost capability gate MUST cover its Rust, Python, and
  TypeScript unit, integration, and user-journey closure, including canonical
  Arrow/OTLP, local admission, object-scoped RBAC, MCP, and Forge recovery.
  The broad final integration gate supplements rather than replaces focused
  capability evidence. Nightly correctness MUST include the local identity
  journey and the isolated-Postgres contract, concurrency, role, and inventory
  checks rather than assuming the aggregate already contains them.
- **REQ-064:** The final integrated candidate MUST pass every repository-owned
  local non-credentialed unit, integration, and user-journey test across Rust,
  Python, TypeScript, HTTP, gRPC, MCP, CLI, Postgres, identity, storage, and
  Bifrost. Gated or ignored journeys MUST be invoked through their owning
  `mise` lanes; their default exclusion is not passing evidence. The broad
  repository gate and every focused capability lane MUST pass locally.
  Credentialed live-cloud tests MUST pass in their owning GitHub Actions
  workflows. No pre-existing-failure waiver, selective omission, empty test
  selection, restored deletion, weakened assertion, ignored failure, or
  replacement by a lower test tier is acceptable.

### Generated artifacts and handoff

- **REQ-036:** Generated schemas, OpenAPI, stubs, declarations, and other
  derived files MUST be regenerated from reconciled authoritative sources and
  MUST NOT be hand-merged as independent authorities.
- **REQ-037:** Obsolete protocol documents, sample specs, aliases, routes,
  comments, and checks MAY be removed only when their responsibility is
  unreachable or enforced by the surviving authority; removals MUST NOT hide a
  live invariant.
- **REQ-038:** The completed change MUST remain on the dedicated integration
  branch with a durable summary of the final refreshed inputs, material decisions,
  unresolved risks, and readiness for a later human-authorized merge to
  `main`.
- **REQ-039:** Task decomposition and merge execution MUST use the completed
  conflict ledger as input. They MUST NOT begin while any material ledger entry
  lacks branch intent, owning authority, resolution, and disposition.
- **REQ-040:** Existing owner crates MAY retain optional `python` features
  during this integration. Only `wyrd-sdk-python` MAY enable and aggregate
  them; `wyrd-sdk-rust` and `wyrd-sdk-ts` MUST NOT enable them.
- **REQ-041:** New or materially relocated Python logic MUST live in
  `sdks/wyrd-sdk-python`. Retained owner-crate Python features MUST be treated
  as migration state toward eventual Python-SDK consolidation, not as
  precedent for new mixed ownership.
- **REQ-042:** This integration targets a greenfield database because Wyrd has
  never shipped. Oracle's cleaned migration baseline is authoritative;
  obsolete Surfaces migrations MUST NOT be retained or replaced with upgrade
  compatibility migrations for nonexistent deployments.
- **REQ-043:** Oracle's deletion of the legacy CLI and admin audit-integrity
  verification surfaces MUST be preserved. The merge MUST NOT recreate them or
  retain their dependencies on the retired Bifrost sealing implementation.
- **REQ-044:** Surfaces' deletion of `wyrd dev bootstrap` MUST be preserved.
  The CLI MUST NOT own database migration, tenant provisioning, service-account
  creation, or API-key persistence.
- **REQ-045:** The two Oracle-tracked `.node` binaries MUST be excluded from the
  integrated tree. TypeScript manifests, loaders, declarations, and release
  automation MUST build platform artifacts rather than read tracked binaries.
- **REQ-046:** History sanitization of the two formerly tracked `.node` paths
  is an external prerequisite for entering `main`, not an implementation step
  or invariant of this integration. Its separately authorized workflow MUST
  inventory affected commits, refs, and active worktrees before any history
  rewrite or force-push.
- **REQ-047:** Task decomposition and merge execution MUST use the immutable
  revision 5 Oracle checkpoint. If the source advances, the spec returns to
  draft, names one replacement clean commit, audits the delta, and resolves
  every new material conflict before proceeding.

## Invariants and prohibited outcomes

- **INV-001:** No Surfaces-authoritative non-Bifrost behavior may be silently
  lost because Oracle changed the same file.
- **INV-002:** No Oracle-authoritative Bifrost behavior may be silently lost
  because Surfaces changed the same file.
- **INV-003:** Git's textual merge result is never sufficient proof for a
  two-sided semantic overlap.
- **INV-004:** Language SDKs never own durable server behavior or parallel Wyrd
  client implementations.
- **INV-005:** No public SDK exposes Gate, Scribe, Oracle, Forge,
  `QueryClient`, or `BifrostGrpcTransport` as independent client owners.
- **INV-006:** No client-tier crate depends on SQL, cloud SDKs, DataFusion,
  Delta Lake, Iceberg, or server implementation crates.
- **INV-007:** No operation derives effective tenant identity from an
  untrusted request field, path, object key, query predicate, or peer payload.
- **INV-008:** No staging row is deleted before its corresponding audit-log
  event is durably published and the tenant watermark advances through it.
- **INV-008B:** The retained audit event and its predecessor contain everything
  required to reproduce its hash; staging is not a second audit authority.
- **INV-008A:** Garbage-collecting staging never removes information required
  for Forge recovery, reconciliation, or idempotent state transitions.
- **INV-008C:** Retained audit publication cannot append an audit event, and a
  successfully drained idle tenant retains no staging tail.
- **INV-008D:** A growing staging tail cannot change the identity of an
  in-flight audit batch or allow overlapping non-identical ranges to reach
  retained history.
- **INV-009:** No failed or partial analytical result is represented as a
  successful query.
- **INV-010:** No compatibility shim preserves a rejected contract or stale
  public name.
- **INV-011:** The active source worktrees and their branches remain
  unchanged by this integration.
- **INV-012:** Generated output never overrides its owning source contract.
- **INV-013:** A Rust or TypeScript SDK build never activates PyO3 or a
  Python-only interface adapter.
- **INV-014:** Merge resolution never resurrects a file or public testing API
  deliberately deleted by the current Oracle `wyrd-testing` authority.
- **INV-015:** No migration compatibility path is added solely for a Wyrd
  database version that was never released.
- **INV-016:** No integrated source tree or generated-artifact workflow treats
  a compiled `.node` output as tracked source.
- **INV-017:** Oracle query admission has no PostgreSQL or cluster-wide quota
  dependency, and differently sized replicas may serve together.
- **INV-018:** No snapshot-dependent source IO begins without durable reader
  protection, and no Forge expiration or deletion passes uncertain protection.
- **INV-019:** No typed observation-read API or duplicate trace-child or GenAI
  physical table survives alongside canonical SQL and the canonical signal
  tables.
- **INV-020:** No query reads a table outside the verified principal's complete
  resolved object scope, and one unauthorized table yields no rows.
- **INV-021:** Forge has no hard DataFusion memory-pool or spill guarantee; its
  bounded safety contract is worker-local FIFO admission over estimated running
  memory plus durable fail-closed recovery.
- **INV-022:** No role becomes ready with an unusable Bifrost data root,
  unhealthy Oracle reader authority, or unresolved Forge maintenance authority.
- **INV-023:** The final dependency graph contains exactly one Bifrost engine:
  `vala-bifrost-redux`. No path, package, symbol, feature, artifact, or prose
  presents legacy `vala-bifrost` as live code or a compatibility authority.
- **INV-024:** No Surfaces-era `wyrd-client` or `wyrd-queue` implementation
  silently replaces Oracle's newer bounded ownership, settlement, transport,
  credential, TLS, or error behavior.
- **INV-025:** The integration has no accepted baseline failures. A failing or
  unexecuted required test or journey prevents completion.

## Externally observable behavior and failure modes

- Existing Surfaces Card authoring, loading, composite registration, and
  `WyrdState` workflows continue through the Rust, Python, HTTP, CLI, and MCP
  surfaces that expose them.
- Bifrost callers use one facade per language for table management, ingestion,
  querying, and lifecycle. A caller does not construct separate write and
  query clients.
- Observation callers read canonical signal tables through bounded SQL. They
  do not receive a second typed observation-query hierarchy.
- Bifrost query authorization covers every resolved table. Schema- and
  table-scoped roles reject uncovered and mixed-table reads without rows.
- OTLP callers may receive standard per-record partial success; request-wide
  trust, size, admission, or durability failures do not commit a partial batch.
- Oracle replicas may have heterogeneous capacity. Saturation is local and
  returns typed retryable admission or terminal resource failure without a
  cluster-wide quota claim.
- Rust, Python, TypeScript, HTTP, gRPC, MCP, and CLI failures preserve the same
  stable error identity and structured metadata.
- Under-privileged, cross-tenant, invalid, conflicting, oversized,
  unauditable, or resource-exhausted operations fail closed with a structured
  error and without an unauthorized durable effect.
- An acknowledged Scribe append remains recoverable under the Oracle-defined
  WAL and staging lifecycle.
- A Bifrost query returns one terminal success or failure. Frames preceding a
  failure terminal are not a complete successful result.
- Public query deadlines use the architecture-defined `1..=u32::MAX`
  millisecond range across Rust, HTTP, gRPC, Python, TypeScript, and MCP. No
  public distributed-plan or execution-path `EXPLAIN` surface is introduced.
- Retained audit queries read `vala.system.audit_log`; transient audit staging
  is not presented as the retained historical ledger.
- Pull-request feedback is limited to affected lanes, while nightly `main`
  qualification detects whole-repository regressions.

## Material constraints

- Architecture documents in this repository and explicit human decisions are
  authoritative over either implementation input.
- The integration must preserve tenant isolation, audit fail-closed behavior,
  WAL durability, structured terminal outcomes, and bounded resource ownership
  even when a smaller textual merge would be easier.
- Internal crates may remain focused implementation owners, but language SDKs
  consume their Wyrd-facing behavior only through `wyrd-client`.
- Existing optional owner-crate Python features may remain until their code is
  materially changed or moving them solves a concrete packaging or dependency
  problem. The target remains consolidation in `wyrd-sdk-python`.
- Language-specific code must be justified by a real Python, Rust, Node, OTEL,
  authoring, or runtime integration need.
- No new shared-client abstraction is introduced above or beside
  `wyrd-client`.
- The current Oracle checkpoint contains the approved single-data-root intent
  but not complete production enforcement; integration MUST implement and
  verify REQ-055 rather than treating branch presence as evidence.

## Required system boundaries and flow

```text
                              +-> sdks/wyrd-sdk-rust
wyrd-server <- wyrd-client ---+-> sdks/wyrd-sdk-python
    |                         +-> sdks/wyrd-sdk-ts
    |
    +-> server-owned Wyrd services
    +-> server-owned Scribe / Oracle / Forge
```

The request direction is language SDK → `wyrd-client` → `wyrd-server`.
Server-owned engines never become SDK dependencies or public listeners.

The query and maintenance safety flow is:

```text
authenticate + coarse capability
  -> resolve immutable table cut
  -> authorize every resolved object
  -> commit Oracle reader protection
  -> accept the read-audit WAL record
  -> admit pod-local resources
  -> execute one selected attempt
  -> terminal + joined cleanup
  -> release reader protection

Forge expiration preparation serializes with protection widening
  -> logical expiration
  -> separately scheduled, revalidated physical cleanup
```

The audit-history flow is:

```text
authorization decision or accepted Oracle read
  -> vala.audit_staging
  -> freeze one tenant range
  -> local Scribe (never Gate)
  -> vala.system.audit_log
  -> atomic watermark advance and staging garbage collection after durable publication
```

## Acceptance obligations

- **AC-001:**
  `.dev/merge-audits/surfaces-oracle-integration/review-ledger.json` exhaustively
  maps every textual conflict, clean two-sided overlap, and one-sided semantic
  coupling to its branch intent, owning authority, resolution, disposition,
  and surviving consumer evidence; `merge-inventory.md` summarizes the same
  audited input set with no pending material entry, and `decision-summary.md`
  is refreshed to revision 5 or explicitly marked historical so stale pins
  cannot compete with this specification.
- **AC-002:** Contract and generated-artifact evidence shows one coherent Card,
  reference, error, HTTP/gRPC, MCP, Python, and TypeScript surface with no stale
  generated authority. OpenAPI includes every public Bifrost table, query, and
  lifecycle route rather than reproducing the Oracle checkpoint's omission.
- **AC-003:** Focused user-journey evidence demonstrates the preserved Surfaces
  registration, loading, and `WyrdState` behavior through each affected Rust,
  Python, TypeScript, HTTP, CLI, and MCP surface. Missing TypeScript Card or
  `WyrdState` journeys are coverage to add, not evidence to waive parity.
- **AC-004:** Real Rust, Python sync/async, and TypeScript SDK journeys through
  `wyrd_client::Bifrost` demonstrate table lifecycle, buffered and direct Arrow
  ingestion, flush/shutdown durability, replay idempotency, SQL-only reads,
  typed collection, streaming, lifecycle operations, description, and the
  absence of sibling public engine clients.
- **AC-005:** SQL and audit evidence demonstrates fail-closed audit of allowed
  and denied authorization decisions, Oracle WAL-first read acceptance,
  idempotent staging publication, reproducible retained hashes, and atomic
  watermark advancement plus garbage collection across partial failures.
  Existing real-server retained-history evidence proves publication into
  `vala.system.audit_log`, including a growing tail with competing publishers,
  crash replay without duplication, and an idle staging table that drains to
  zero. Existing multi-pod Scribe journey infrastructure also proves that one
  immutable canonical batch submitted concurrently through distinct server
  endpoints is visible exactly once. No new test harness or test file is
  introduced.
- **AC-006:** Tenant and authorization evidence demonstrates isolation across
  registry, SQL, Bifrost physical tables, object paths, query plans, and every
  public client surface.
- **AC-007:** Fixture evidence demonstrates isolated database lifecycle under
  `wyrd_test_admin`, schema-only `wyrd_migrator`, and non-owning child attach.
- **AC-007A:** The completed conflict ledger enumerates the refreshed Oracle
  testing-stack deletions, and capability evidence covers SDK, Forge, Scribe,
  Oracle, OTLP, server, MCP, Python, and TypeScript journeys without restoring
  removed typed-read, benchmark/qualification, or runtime-emulation layers.
- **AC-008:** Workflow evidence demonstrates affected-code pull-request lane
  selection, stable required-job aggregation, full nightly-main correctness,
  and separate live-cloud and performance schedules. It exercises success,
  skipped, failure, mixed, global, and unclassified classifier/aggregation
  outcomes and includes the local identity journey plus isolated-Postgres
  contract, concurrency, roles, and inventory checks.
- **AC-009:** Final static review maps every `REQ-*` and `INV-*` to credible
  evidence and identifies any remaining implementation drift before the
  integration branch is proposed for merge.
- **AC-010:** Dependency and feature evidence shows that only
  `wyrd-sdk-python` activates retained owner-crate `python` features and that
  Rust and TypeScript SDK builds remain PyO3-free.
- **AC-011:** Distributed query journeys prove the single physical-build
  decision, normal-root Interactive and `DistributedExec` Analytical paths,
  bounded worker selection, one original deadline, schema/batch/EOS/terminal
  validation, cancellation cleanup, and post-selection peer failure without a
  successor attempt or fallback.
- **AC-012:** Deterministic local-admission tests and a multi-replica journey
  prove heterogeneous pod capacity, FIFO per tenant, equal-weight tenant
  rotation, Interactive-floor protection, bounded queue refusal, one governed
  memory root, selected-worker limits, and the absence of PostgreSQL admission
  state from every production query path.
- **AC-013:** Reader-authority integration and process journeys prove durable
  protection before source IO, conservative process-global aggregation,
  serialization with snapshot expiration, protection through leader/follower
  cleanup, fail-closed lease loss, startup recovery, and readiness removal.
- **AC-014:** Forge production journeys prove independent per-plan publication,
  retained sibling progress, worker-FIFO estimated-memory admission, the
  bounded conflict schedule, durable ambiguous-outcome reconciliation,
  separate maintenance strategies, and readiness retraction. Production
  geometry is qualified on its dedicated evidence lane.
- **AC-015:** Stock Rust, Python, and TypeScript OTLP exporters plus canonical
  Arrow journeys prove equivalent signal rows, nested trace children, complete
  supported log/metric shapes, GenAI retention without duplicate tables,
  per-record partial success, request-wide fail-closed behavior, contiguous
  ordinals, and SQL readback through all durability stages.
- **AC-016:** Scoped-role journeys prove `All`, schema, and stable table-UID
  coverage, scope validation, future schema members, recreated-table refusal,
  mixed-table denial with no rows, authorization before source IO, and no
  distributed widening.
- **AC-017:** Audit evidence proves delegation and typed scoped permission
  identity survive Oracle WAL framing and replay, relay checkpointing follows
  Postgres commit, one logical query creates one event, object denial fails
  closed when audit is unavailable, and peer rejection uses the correct tenant
  or system chain.
- **AC-018:** Configuration, boot, restart, and deployment evidence proves the
  default and overridden single Bifrost data root, automatic managed paths,
  removal of old settings, failure before readiness, durable restart behavior,
  and no shared writable WAL identity. Task evidence shows this work followed
  the core Redux integration and completed before integration closeout.
- **AC-019:** MCP discovery and invocation journeys prove the exact three-tool
  catalog, bounded complete results, structured refusal, delegation/audit, and
  disconnect cleanup. CLI journeys prove the retained query I/O contract
  through the shared facade. Live UI Oracle behavior is not claimed.
- **AC-020:** Static tree, workspace metadata, dependency-graph, feature, code,
  generated-artifact, documentation, and test-inventory evidence finds no
  `crates/vala/vala-bifrost` package, `vala-bifrost` dependency, `vala_bifrost`
  import, compatibility facade, or dual-engine consumer. All Bifrost server,
  ingest, SDK, audit, query, and test paths resolve to Redux or their approved
  server/client owner.
- **AC-021:** Base-to-candidate review shows the Oracle `wyrd-client` and
  `wyrd-queue` behavior survived intact, including bounded Arrow material,
  producer capacity, ambiguous-batch retention, transactional permit
  admission, cleanup ownership, drain/backpressure, credentials, TLS,
  transport, and catalog-backed errors. Public Bifrost journeys enter through
  `wyrd_client::Bifrost`, proving adaptation to the final client boundary
  without restoring Surfaces implementations.
- **AC-022:** Completion evidence records a passing result for every
  local non-credentialed test and journey lane required by REQ-064, including
  the broad gate and all gated journeys. GitHub Actions evidence records the
  passing credentialed live-cloud workflows. The final review independently
  confirms that no required lane was skipped, filtered to zero tests, weakened,
  or excused as a baseline failure.

No command named by these acceptance obligations is executed during drafting
or approval of this specification. Exact focused commands belong in tasks
derived after approval.

## Open material decisions

None. The user resolved the remaining scope and acceptance decisions on
2026-09-11: all local non-credentialed tests and gated journeys must pass;
credentialed cloud tests run and pass in GitHub Actions; the single Bifrost
data-root outcome is a required completion-blocking follow-up task; and live UI
integration remains outside this change because that work is ongoing.

The user approved revision 7 on 2026-09-11. It retains the authorization-only
audit boundary, freezes one in-flight publication range per tenant, routes the
publisher directly to its local Scribe, and requires one canonical multi-pod
deduplication control using existing test infrastructure. Audit state remains
unshipped, so its existing schema definition changes in place with no migration
or backfill. TASK-001 implementation is complete; its closeout remains blocked
until TASK-005 and TASK-006 complete. Task decomposition and merge execution
remain subject to REQ-039 and REQ-047.

## Revision history

- Revision 1 (`draft`, 2026-09-02): Initial specification synthesized from the
  branch audit and the user's architecture decisions. Locks Surfaces as
  non-Bifrost authority, Oracle as Bifrost authority, `wyrd-client` as the
  shared client implementation, and the three `wyrd-sdk-*` language packages
  as thin projections. Requires the exhaustive conflict ledger before task
  decomposition or merge execution. Allows existing optional owner-crate
  Python features as migration state while directing all new or materially
  relocated Python logic into `wyrd-sdk-python`.
- Revision 2 (`draft`, 2026-09-11): Refreshes Oracle from `b9d7d0caf` to
  `b088ca91a` after auditing 593 commits and records that Forge is already
  integrated. Adds the completed one-build distributed query contract,
  pod-local admission, durable Oracle reader protection, current Forge
  publication/recovery/readiness semantics, canonical OTel tables and SQL-only
  reads, typed object-scoped RBAC, catalog-backed Python errors, single Bifrost
  data root, server-owned MCP/CLI behavior, and refreshed verification evidence.
- Revision 3 (`draft`, 2026-09-11): Records the user's clarified integration
  authority. The complete Bifrost Redux implementation replaces legacy
  `vala-bifrost`; Oracle `wyrd-client` and `wyrd-queue` supersede their
  Surfaces implementations while Bifrost client behavior is composed into
  `wyrd_client::Bifrost`; the unified Python error projection is authoritative;
  and every repository-owned test and journey must pass without a baseline-
  failure exception.
- Revision 4 (`approved`, 2026-09-11): Records the user's final scope and
  acceptance decisions. Every local non-credentialed test and gated journey
  must pass without waivers; credentialed cloud tests pass in GitHub Actions;
  the single Bifrost data-root outcome is a required follow-up task that blocks
  integration completion; and live UI integration remains out of scope while
  its separate work continues.
- Revision 5 (`approved`, 2026-09-11): Refreshes the destination and Surfaces
  authority pins after the user-directed history purge of 97 generated UI
  evidence images. Regenerates the conflict inventory against the committed
  destination baseline without changing behavior, scope, or task outcomes.
- Revision 6 (`approved`, 2026-09-11): Records the user-approved audit boundary
  revision. Audit records allowed and denied permission decisions rather than
  engine mechanics; Oracle reads retain their WAL-first exception;
  `vala.audit_staging` publishes by watermark without a grace tail; retained
  hashes use retained fields; original decision time survives publication; and
  audit history partitions daily. No migration compatibility is required for
  unshipped audit state. TASK-005 is implemented when the paused, in-progress
  TASK-001 resumes and blocks its closeout. Existing verification is reused
  without adding tests.
- Revision 7 (`approved`, 2026-09-11): Freezes one in-flight audit publication
  range per tenant so a growing tail, crash, or competing Scribe replica cannot
  produce overlapping batch identities; routes retained publication directly
  to the local Scribe rather than Gate; and adds one canonical multi-pod
  deduplication control using existing harnesses. Audit state remains unshipped,
  so the existing schema definition changes in place with no migration,
  compatibility path, or backfill. TASK-001 remains implemented with closeout
  blocked on TASK-005 and TASK-006.

## Material authority

- [`AGENTS.md`](../../../AGENTS.md)
- [`architecture/wyrd-design.md`](../../../architecture/wyrd-design.md)
- [`architecture/wyrd-doctrine.mdx`](../../../architecture/wyrd-doctrine.mdx)
- [`architecture/bifrost-design.md`](../../../architecture/bifrost-design.md)
- [`architecture/wyrd-security-posture.md`](../../../architecture/wyrd-security-posture.md)
- [`architecture/operations/README.md`](../../../architecture/operations/README.md)
- [`architecture/references/architecture/patterns.md`](../../../architecture/references/architecture/patterns.md)
- [`architecture/references/doctrine/architecture-constraints.md`](../../../architecture/references/doctrine/architecture-constraints.md)
- [`architecture/references/languages/spec-driven-development.md`](../../../architecture/references/languages/spec-driven-development.md)
- [`architecture/references/languages/testing-workflows.md`](../../../architecture/references/languages/testing-workflows.md)
- [`architecture/references/languages/errors.md`](../../../architecture/references/languages/errors.md)
- [`architecture/references/languages/pyo3-boundaries.md`](../../../architecture/references/languages/pyo3-boundaries.md)
- [`architecture/references/languages/python-api-and-stubs.md`](../../../architecture/references/languages/python-api-and-stubs.md)
- [`architecture/references/languages/typescript-guide.md`](../../../architecture/references/languages/typescript-guide.md)
- [`architecture/references/domain/vala-architecture.md`](../../../architecture/references/domain/vala-architecture.md)
- [`architecture/references/domain/olap-serving.md`](../../../architecture/references/domain/olap-serving.md)
- [`architecture/references/domain/datafusion.md`](../../../architecture/references/domain/datafusion.md)
- [`architecture/references/domain/iceberg.md`](../../../architecture/references/domain/iceberg.md)
- [`architecture/references/domain/telemetry-observations.md`](../../../architecture/references/domain/telemetry-observations.md)
- [`architecture/references/domain/analytical-operations-reliability.md`](../../../architecture/references/domain/analytical-operations-reliability.md)
- [`architecture/v1/00-foundations/permission-check.md`](../../../architecture/v1/00-foundations/permission-check.md)
- [`architecture/operations/deployment-and-release.md`](../../../architecture/operations/deployment-and-release.md)
- [`architecture/operations/reliability-and-recovery.md`](../../../architecture/operations/reliability-and-recovery.md)
- Decomposition inspiration:
  `/Users/stevenforrester/Documents/GitHub/wyrd/.dev/plan/not-started/09-repo-decomposition-topology.md`
  (boundary reference only; its multi-repository topology is not authority for
  this change).
