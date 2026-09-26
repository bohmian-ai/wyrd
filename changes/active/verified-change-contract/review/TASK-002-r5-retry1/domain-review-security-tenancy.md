# TASK-002 R5 retry 1 security and tenancy domain review

## Immutable subject

- Repository: `/home/thorrester/Documents/GitHub/wyrd-verified-change-contract`
- Base: `c8bb490ad814c0c7770cac33ed7779897ff776e4`
- Candidate: `935cc6324d213414402a1d73d6eeec475965fbcb`
- Approved specification: `changes/active/verified-change-contract/spec.md`, revision 32
- Original task: `changes/active/verified-change-contract/tasks/TASK-002-scoped-observation-and-bifrost-tables.md`
- Prior remediation and review: TASK-002 R1 through R4, inspected cumulatively from the original base
- AI co-author trailers are allowed by the current user instruction and are outside this review.

`HEAD` resolved to the exact candidate before inspection and again before this report was written. The worktree contains review artifacts from this review wave but no source change, so the reviewed source remained immutable.

## Reviewed boundary

This review traced the TypeScript observation trust boundary fixed by R4 and the cumulative security-sensitive path from a Card-bound SDK credential through fixed and dynamic table description, canonical authorization audit, Gate/Scribe admission, signed Card-scope subject resolution, server-stamped publisher and tenant identity, schema fencing, and tenant-qualified readback. It also rechecked the test-only describe/audit-publication controls and the production audit-publisher composition split.

## Authority and source coverage

| Boundary | Governing authority | Source and evidence inspected | Result |
|---|---|---|---|
| TypeScript observation values before native admission | REQ-075, REQ-076, REQ-124, REQ-129; R4 remediation; server-owns-durable-validation rule | `sdks/wyrd-sdk-ts/wyrd/src/index.ts:1085-1221`; `tests/unit/observe.test.ts:134-259`; R4 diff | PASS |
| Fixed and lazy table describe authorization | REQ-127, REQ-145; `architecture/agent-rules.md` audit rules; `architecture/wyrd-security-posture.md` authorization | `crates/wyrd/wyrd-server/src/bifrost/service.rs:226-269`; real denied-describe proof in `pg_bifrost_e2e.rs` | PASS |
| Writer/subject split and signed scope | REQ-076, REQ-118, REQ-121; `architecture/bifrost-design.md` table and row identity | `crates/vala/vala-bifrost-redux/src/scribe/execution_lanes.rs:853-900,1136-1268`; three SDK journeys and stored-row assertions | PASS |
| Tenant and publisher authority | `architecture/wyrd-security-posture.md` tenant isolation; Bifrost identity contract | Scribe derives `principal_id` and `data_tenant_id` from the authenticated principal and resolves `card_uid` only from its signed `CardRefScope` | PASS |
| Canonical audit publication | Repository single-audit-path rules; `architecture/bifrost-design.md` audit contract | `crates/vala/vala-sql/src/queries/audit_staging.rs`; `crates/wyrd/wyrd-server/src/app/server.rs:620-634`; focused lock-timeout evidence | PASS |
| Test-support isolation | Production composition and fail-closed audit rules | `crates/wyrd/wyrd-server/src/state.rs`; `app/server.rs:623-628`; `wyrd-testing/src/server.rs:1580-1662`; Python/TypeScript testing projections | PASS |
| Schema and admission fencing | AC-025 and Bifrost fail-closed schema contract | stale-writer flush and replacement-registration refusal in `crates/shared/wyrd-client/tests/pg_bifrost_e2e.rs` | PASS |

## Security and tenancy analysis

### Observation input trust boundary

The R4 correction keeps `strictJson` as the single TypeScript serializer but changes its recursive validation walk to return the exact plain JSON snapshot that is serialized. Every accepted object property and array element is therefore read once, while unsupported primitives, unsafe or non-finite numbers, cycles, non-plain objects, symbol keys, hidden properties, and non-element array properties still fail with `WYRD_SPEC_400_VALIDATION` before native admission. `mediaJson` separately checks each original descriptor's own keys against the closed `id`, `kind`, `uri`, and `mediaType` set before projection, rejects undeclared, symbol, and hidden keys, reads declared fields once, and sends the projected snapshot through `strictJson`. The focused tests exercise Drift, Eval, and generic record getter paths, unsupported first values, all three refused media-key classes, exact single reads, exact native payloads, and zero native calls after refusal.

The correction adds no serializer, dependency, public contract, error code, durable validation path, authorization behavior, or alternate transport. Rust remains the durable media and record validator after the TypeScript boundary has prevented JavaScript-only data loss.

### Authorization and canonical audit

`describe_table` constructs the requested FQN, then calls the canonical fail-closed `audit::authorize` for `bifrost_table:read` before namespace conversion or catalog access. Consequently an unauthorized caller cannot use validation differences as a table-existence oracle, the audit tenant always comes from `caller.data_tenant_id`, and an audit-append failure refuses startup or lazy describe. The real denied-describe journey proves one denied canonical staging row and no cached table or producer.

### Writer, subject, and tenant identity

Scribe excludes managed identity columns from the user projection, resolves a supplied `card_ref` by exact identity against the authenticated principal's signed Card scope, rejects absent or unauthorized scope members, and stamps only the UID from that signed member. It independently stamps `principal_id` from the authenticated principal and `data_tenant_id` from that principal's verified tenant. The cumulative SDK journeys use the registered Service principal as writer while reading Model and Agent rows by their distinct subject UIDs and one invocation ID, so client payloads cannot substitute either publisher or tenant identity.

### Test-only controls and production composition

The audit-publication disable flag exists only under `feature = "test-support"`, defaults off, and the non-test-support branch unconditionally constructs the normal publisher. The describe fault hook lives in `wyrd-testing`, restricts its interpolated FQN to ASCII identifier characters, predicates the trigger on both exact FQN and fixture tenant UUID, and is projected only through test packages. These controls do not create a production configuration surface or authorization bypass.

### Audit publication timeout

The three-second transaction-local lock timeout on `freeze_publication_range` preserves the one frozen per-tenant bound: a timeout propagates as an error from the aborted transaction rather than pretending the tenant is idle, and a later publisher retries the unchanged range through the same canonical path. No alternate audit sink, lease, owner token, or unaudited publication path was introduced.

## Prior-finding closure

- `FIND-TASK-002-4`: **CLOSED** for the security/trust-boundary scope. The serializer now validates and serializes one snapshot, and original media descriptors are checked before projection; the R4 focused cases prove single reads, exact native payloads, and pre-native refusal.
- `FIND-TASK-002-11`: remains closed; the import-only correction does not alter security behavior.
- `FIND-TASK-002-16`: remains closed; readonly aliases preserve the same runtime boundary.
- `FIND-TASK-002-17`: remains closed; the evidence names the actual structured error.
- Earlier security-relevant findings remain closed: failed or denied describes admit nothing and are canonically audited, signed scope controls subject stamping, authenticated authority controls publisher and tenant stamping, stale schema writes are fenced, and production audit publication remains mandatory.

## Verification evidence and limits

The candidate records passing focused TypeScript observation tests (`10/10`), `ts:test:unit` (`22`), `ts:test:integration` (`18`), `ts:typecheck`, `fmt`, `lints`, and `git diff --check`. Earlier cumulative evidence includes real Rust, Python, and TypeScript SDK journeys, the ignored real-server denied-describe journey, stale-schema fencing, the audit-lock timeout test, and the prior nine-lane capability run.

The new `verify:bifrost` run had not completed when review began. That is a verification limit, not a security finding or blocker for this domain report: the only R4 production change is the TypeScript pre-native serializer/media projection, its focused and TypeScript integration lanes passed, and static inspection found no change to server authorization, audit, tenancy, identity stamping, or production composition.

## Findings

No material security, authorization, audit, credential, tenant-isolation, trust-boundary, writer/subject-identity, or schema-admission finding was identified.

## Overall result

**PASS**
