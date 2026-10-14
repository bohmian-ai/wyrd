# Persistent-data and tenancy domain review

## Immutable subject

- Base: `ef03f624a1f3a6bcd51bbd17060271f0b5e8cabf`
- Candidate: `c4ba2b0502f8968a78e5e960bcc9eba43e960643`
- Approved behavior: `changes/active/example-journey-fixes/spec.md` revision 4 and binding owner decisions OD-1 through OD-7
- Domain: tenant-scoped durable attribution, Bifrost persistence/query behavior, and deployment-keyring storage of gateway managed secrets

The candidate commit remained unchanged during this review.

## Boundary and authority coverage

| Boundary | Producer to durable sink/read path inspected | Governing authority | Result |
|---|---|---|---|
| Gateway-call attribution | `ingress::requested_subject` parses independent optional headers; `GatewayInvocation::attribute` applies the existing Card-scope check only when a Card is present; `CallCapture::calls_batch` projects principal plus independent Card/Run correlation; the shared Scribe path stamps and retains `vala.gateway.calls` | Spec REQ-004, INV-001; OD-2; `AGENTS.md` tenant and Bifrost rules | PASS |
| Verification-result attribution | Direct execution and queued execution build `ResultRun`; named-column assembly writes `requested_by_principal_id`, subject, application Run, source record, binding, owner, and Trigger; the process Scribe outbox writes the built-in result tables | Spec REQ-004; OD-1; one-Scribe-write-path rule | PASS for newly produced rows; see DATA-001 for migrated queue history |
| `wyrd.verifier_runs` actor/origin | Manual enqueue freezes the authenticated caller; scheduled and observation enqueue select the tenant SYSTEM principal under `TenantConn`; claim reads the frozen requester; result mapping copies it while origin remains references | Spec REQ-004; OD-1; `AGENTS.md` `TenantConn`/RLS rules | FAIL: DATA-001 |
| Observation origin and trace | Observation records retain their managed writer, Run, trace, and span; queued runs retain `input_record_id`; execution reads those fields under tenant-scoped Bifrost authority, links the engine span to the observation span, and attributes judge calls to the subject Card and observed Run | Spec REQ-004; OD-1; known open observation-judge authority decision | PASS |
| Gateway managed-secret persistence | Gateway administration seals a payload containing tenant/name/provider/secret under `SealingKeyring`, then writes opaque bytes with `TenantConn`; snapshot resolution opens it against the same binding; row RLS and authenticated binding both fail copied envelopes closed | Spec REQ-007, INV-001/002; OD-5; security-posture secret and tenant rules | PASS |
| Key rotation and process restart | `SealedSecretTable::GatewayProviderCredentials` lets the existing `OperatorPool` boot rewrap enumerate and compare-and-swap every tenant envelope; retained keys serve old envelopes while rewrap converges; the CLI creates one no-clobber owner-only local key before migration/boot and reuses it | Spec REQ-007; OD-5; restart/multi-replica behavior in TASK-005 evidence | PASS |
| Managed-secret schema migration | Migration removes obsolete nonce/version columns; no alternate gateway keyring or compatibility reader remains | OD-5, which explicitly rejects compatibility | PASS |
| Live/mixed/remote Bifrost subqueries | The live leaf leaves scalar-subquery expressions at the leader instead of serializing them into remote Scribe fragments; shared support workload compares rich seeded answers with plain DataFusion and covers published, live, mixed, and remote-Scribe states | Spec REQ-006, INV-001; OD-6; Bifrost design | PASS |
| SQL capability and tenancy | Changed tenant CRUD/queue paths accept `TenantConn`; cross-tenant sealed-secret rewrap alone uses `OperatorPool`; Bifrost bindings and correlation stay tenant-qualified | `architecture/agent-rules.md`, security posture, Bifrost design | PASS |

## Material finding

### DATA-001 — MISSING: the migration does not attribute already-enqueued internal verifier runs

- **Violated obligation:** Spec REQ-004 requires every Verifier execution to record its actor on `wyrd.verifier_runs`, and OD-1 defines that actor as the tenant SYSTEM principal for every scheduled and observation-triggered run (`changes/active/example-journey-fixes/spec.md:125`, `changes/active/example-journey-fixes/owner-decisions.md:13`).
- **Exact location:** `crates/wyrd/wyrd-sql/migrations/20261014000001_verifier_run_requester_every_origin.sql:12-30` only removes the old manual-only check and adds the relaxed replacement. It contains no update of existing scheduled or observation rows. The implementation record explicitly acknowledges that those rows remain null at `changes/active/example-journey-fixes/tasks/TASK-004-gateway-attribution-and-trace.md:57`.
- **Evidence and reachability:** The original table permits only manual rows to carry a requester (`crates/wyrd/wyrd-sql/migrations/20260601000029_verifier_runs.sql:98-100`), so every internal row committed before this migration necessarily has `requested_by_principal_id IS NULL`. Existing tenants already have exactly one stable SYSTEM principal, established and backfilled by `crates/wyrd/wyrd-sql/migrations/20260601000028_system_principal.sql:40-42` and `:96-106`. Thus an upgrade with queued or terminal internal-run history deterministically retains unattributed durable rows even though their actor is resolvable.
- **Observable consequence:** After upgrade, the same tenant's `wyrd.verifier_runs` contains two attribution regimes: historical scheduled/observation executions have no actor while later ones name SYSTEM. Audit, investigation, and joins over verification history cannot satisfy the approved actor invariant, including for rows that later point at retained results.
- **Required testable correction:** In the existing requester migration, backfill null requesters for `origin IN ('schedule', 'observation')` from that row's tenant SYSTEM principal before installing the replacement constraint; tenants with no SYSTEM principal must remain null, preserving the owner-approved exception. Add a migration test that seeds pre-migration scheduled and observation rows for one tenant with SYSTEM and one tenant without it, applies the migration, and proves only the resolvable rows gain the correct same-tenant SYSTEM ID while manual attribution is unchanged. This reuses the existing SYSTEM principal authority and needs no new column, lookup path, or compatibility mechanism.

## Verification assessment and limits

- Reviewed the complete base-to-candidate diff and the task-recorded focused evidence for verifier queue/migrations, server verification journeys, gateway restart/rotation, tenant-isolation, Bifrost support workload, and remote-Scribe coverage.
- `git diff --check base..candidate` passed during this review.
- The recorded verifier migration lane is insufficient for DATA-001: its own task record states the historical rows remain null, and no focused test asserts upgrade backfill.
- I did not rerun the PostgreSQL or distributed Bifrost suites; their recorded green results do not exercise the identified migration state. Static source and migration evidence is decisive for the missing update.
- Per the owner's exclusions, I did not treat the built-in results-table schema evolution gap, a tenant lacking SYSTEM for internal work, or the still-open observation-judge execution authority as findings.

## Overall result

**FAIL** — one material persistent-attribution finding remains: DATA-001.
