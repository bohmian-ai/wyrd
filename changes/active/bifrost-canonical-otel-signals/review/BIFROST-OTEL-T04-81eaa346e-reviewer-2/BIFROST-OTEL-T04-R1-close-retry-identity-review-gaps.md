---
id: BIFROST-OTEL-T04-R1
title: Close OTLP retry-identity and Rust compliance gaps
kind: remediation
status: proposed
spec: SPEC-bifrost-canonical-otel-signals
spec_revision: 11
original_task: changes/active/bifrost-canonical-otel-signals/tasks/04-integrated-public-journeys.md
base: 442da074cb316be7f580694ba8274229561935a8
candidate: 81eaa346e643ac6315e517041fc838c41057f7ac
findings: [FIND-BIFROST-OTEL-T04-1, FIND-BIFROST-OTEL-T04-3, FIND-BIFROST-OTEL-T04-4]
required_skill: wyrd-implement
---

# Close OTLP retry-identity and Rust compliance gaps

## Objective

Finish BIFROST-OTEL-T04 without changing its approved public or persisted
contracts: preserve byte-identical retry suppression for one authenticated
publisher, prevent identical payloads from different authenticated attribution
from collapsing onto one fence, and bring the touched Rust into
repository-standard shape.

`FIND-BIFROST-OTEL-T04-2` is withdrawn. The existing repository boundary treats
the current version/variant-stamped value as UUIDv7-compatible; this remediation
does not change that representation, add a second fence key, or revise the
specification.

## Issue diagnosis

### FIND-BIFROST-OTEL-T04-1 — authenticated writes can share one fence

Revision 11 requires every accepted row to retain its authenticated publisher
and requires acknowledgement to mean that row is authoritative. The candidate's
Gate identity at `crates/vala/vala-bifrost-redux/src/gate/mod.rs:101` hashes the
tenant, table, and `logical_data_identity`. That reused logical identity excludes
the universal correlation columns, including `principal_id`, in
`scribe/preprocess.rs:453-466`. `dispatch_canonical` supplies the principal only
after deriving the batch ID.

Two authorized principals in one tenant can therefore send the same canonical
OTLP payload and reach the same `(tenant, table, batch_id)` fence. Scribe treats
the second request as already committed, so it can return success without the
second principal's row or audit attribution. The current unit test varies only
tenant, table, order, and payload; the public replay journey uses one token.

### FIND-BIFROST-OTEL-T04-3 — imports violate the repository Rust rule

The new Gate helper uses fully qualified types in its signature and imports
`sha2` inside the function. `architecture/agent-rules.md` requires all ordinary
imports at module scope and bare imported type names in signatures.

### FIND-BIFROST-OTEL-T04-4 — touched Rust lacks mandatory documentation

The materially changed durable dispatch method, the new test module, and the
panicking test batch helper do not carry all rustdoc required by `AGENTS.md`
§16 and `architecture/agent-rules.md`. The missing documentation hides the
retry/fence behavior and leaves a panic undocumented.

## Intended correction outcome

One authenticated publisher retrying the same accepted canonical OTLP batch
converges on the existing Scribe fence and produces one row set. A different
authenticated publisher or different accepted correlation attribution does not
reuse that fence and produces its own correctly attributed row set. The Gate
continues to use the existing logical digest, UUID representation, Scribe WAL,
and Postgres fence; no API, schema, migration, dependency, feature, or new
identity mechanism is introduced.

The changed Rust satisfies mandatory import and rustdoc rules.

## Decision-complete recommendation

Keep the correction in the existing Gate owner and reuse
`scribe::preprocess::logical_data_identity` plus the existing Scribe batch
fence. Bind the derived identity to the authenticated principal and every stable
accepted per-row correlation value excluded by the reused logical digest. Keep
request-scoped values such as request ID and ingest time excluded so a genuine
retry still converges. Do not add a second idempotency store, header, protocol
field, database column, dependency, trait, or abstraction.

Extend the existing OTLP negative journey rather than adding a target or
fixture. It must prove both sides of the boundary: replaying with the same
principal remains exactly once, while two principals in the same tenant sending
identical user payloads both persist and read back with their own
`principal_id`. Also cover differing accepted Card/Run correlation when the
existing fixture can express it without creating another journey.

Move the helper's imports into the current module import block, use bare type
names, and add only the rustdoc demanded for the materially changed method,
test module, and panicking helper.

## Constraints and preserved behavior

- Preserve revision 11 and the original T04 behavior; no spec revision.
- Preserve OTLP HTTP protobuf, HTTP JSON, and gRPC behavior.
- Preserve partial-success counts, stable reasons, accepted-row order, and
  contiguous ordinals.
- Preserve all-invalid and request-wide refusal behavior.
- Preserve current tenant scoping, permissions, payload protection, audit,
  acknowledgement, WAL, and fence ownership.
- Preserve native canonical Arrow ingestion and its client-supplied batch IDs.
- Preserve the corrected log-schema restamping test.
- Use the existing harness, test target, fixtures, owners, dependencies, and
  UUID representation.
- Do not weaken timeouts, durability assertions, permissions, topology, or
  ignored-test policy.

## Explicit non-goals

- No new public idempotency key, header, route, request field, or SDK surface.
- No new persisted fence key, table, column, migration, or compatibility path.
- No alternate OTLP projector, mapper, collector, harness, or test target.
- No refactor of Scribe fencing or `logical_data_identity` beyond what the
  accepted attribution boundary strictly requires.
- No repair of unrelated Oracle timing behavior.

## Acceptance criteria

| Finding | Required proof |
|---|---|
| `FIND-BIFROST-OTEL-T04-1` | Same tenant/table/payload and same authenticated principal derive one stable identity across replay; changing authenticated principal or stable accepted correlation attribution derives a different identity. A public OTLP journey proves same-principal replay stores one row set and two principals store two correctly attributed row sets. |
| `FIND-BIFROST-OTEL-T04-3` | All new ordinary imports are module-top and the changed signatures use bare imported types. |
| `FIND-BIFROST-OTEL-T04-4` | Every new or materially changed Rust item in the remediation has meaningful required rustdoc, including `# Errors`, retry/partial-progress behavior, and `# Panics` where applicable. |

## Verification

Run all Cargo-backed commands sequentially through `mise`:

```bash
mise run fmt
mise run lints
mise exec -- cargo nextest run --locked -p vala-bifrost-redux --lib \
  -E 'test(=gate::otlp_batch_id_tests::derived_identity_is_stable_per_tenant_table_and_payload)'
mise exec -- scripts/postgres/with-test-postgres.sh -- bash -lc "mise run db:migrate:inner && mise exec -- cargo nextest run --locked -p wyrd-testing --test otlp -P journey -E 'test(=negative::pg_tests::mixed_otlp_requests_commit_only_complete_siblings_and_exact_partial_success)' --run-ignored=all"
mise run verify:bifrost
git diff --check
```

Before returning `IMPLEMENTED`, record the exact focused test name used for the
two-principal proof if it is added as a separate named test.
