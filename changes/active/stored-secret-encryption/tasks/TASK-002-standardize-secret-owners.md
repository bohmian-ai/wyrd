---
id: TASK-002
kind: implementation
status: proposed
spec: SPEC-stored-secret-encryption
spec_revision: 1
requirements: [REQ-001, REQ-002, REQ-003, REQ-004, REQ-005, REQ-008, REQ-009, REQ-012]
depends_on: [TASK-001]
---

# Standardize every existing secret owner

## Outcome and flow

Make the standardized system usable: real initialization and owner-specific
credential writes → Task 1's facade/common envelope → existing Postgres resource
rows → actual auth, Gateway, and Operator use through the same facade. A local
administrator can initialize, create an Operator connection, and receive a
failed-Verifier notification without another encryption setup, including after
restart. This is the consuming end-to-end proof of Task 1, not another crypto
implementation.

Use the [approved spec](../spec.md), design
[One credential write and read](../design/notes.md#one-credential-write-and-read),
[Rotation, failure, and performance](../design/notes.md#rotation-failure-and-performance),
and [journey 1](../design/journey-scenarios.md#1-get-started-and-use-stored-credentials).
Task 1 supplies the common owner, context/envelope operations, backends, and
typed configuration. Tasks 3–4 subsequently add the requested admin workflows.

## Existing owners and scope

Connect server composition/configuration and CLI `init.rs` to the common owner.
Reuse init's atomic protected-key persistence and restart lifecycle. Convert
auth `pg_resolvers.rs`, Gateway `managed.rs`/`credential.rs` and server snapshot
preparation, and Operator service/delivery to consume it. Expected context comes
from their trusted metadata. Platform OIDC remains platform-authorized. Reuse
existing Gateway snapshots and auth candidate/activation semantics.

Make only the initial schema/format changes explicitly named in the spec's
**Persistent and authority changes** section: existing auth/platform/Gateway
columns carry the common envelope; consolidate Operator ciphertext into its
existing column, remove the named split crypto columns/index, retain semantic
version and lineage/RLS. The explicit unshipped exception permits direct initial
definition edits. No data migration, legacy reader, new registry, or new table.

Delete direct sealing, unversioned readers, Operator raw-key/Vault KV ownership,
and old configuration/env projections once callers move. Update fixtures and
schema comments. Existing maintenance must use the common crypto path; retain
its safety behavior while Task 4 consolidates orchestration and exposes actions.
Do not remove maintenance safeguards in anticipation of that task. Keep the
shared Vault KV reader for external Gateway references and all excluded crypto.

Remote crypto for existing writes must run outside held DB locks/transactions
and use existing version/byte fences for atomic commits. This includes the
Operator update path, which currently waits on key operations with a row lock.
Prepare Gateway decryption material in the existing background snapshot-loading
owner; warm calls use memory and add no remote unwrap/per-request DB check.
Bind prepared material to exact envelope/revision and existing cache lifetimes.

## Ordered acceptance scenarios and evidence

Record focused commands and actual RED/GREEN/assertion evidence in these entries.

1. **Initialize and create a connection** — REQ-001–005, REQ-012.
   Given a fresh local init profile, run the real CLI initialization and create
   an Operator connection through its public client path without separate key
   settings. Verify protected persistent key reuse and a common encrypted row;
   views remain redacted. Extend the CLI local-development/server integration
   owners. RED is absent common init injection, not a test-supplied Operator KEK.
2. **Deliver after restart** — REQ-001, REQ-005, REQ-009, REQ-012.
   Starting from that persisted deployment/connection, trigger a failed Verifier
   and observe delivery to the local provider fixture. Restart with the saved
   configuration and repeat. Exercise the real server and SDK/CLI journey using
   the connection from setup; do not substitute an independently seeded fixture.
   RED is the missing common-key consuming flow. Make the parked guide runnable
   from this proof with repository journey-tested examples.
3. **Use every existing secret owner** — REQ-001–005, REQ-008.
   Create tenant human-auth, workload-issuer, platform OIDC, Gateway managed,
   and Operator credentials through existing owner APIs, then perform their real
   authentication/exchange/provider/delivery operations. Parameterize backend
   boundary coverage without creating new user journeys. Use real Postgres/
   server and local OIDC/provider fixtures; a redacted GET alone is insufficient.
   Verify the common envelopes are actually consumed, including on restart.
   RED is an owner still using the old crypto/configuration.
4. **Enforce scope and failure safety through consumers** — REQ-003–004,
   REQ-008. With two tenants and distinct owners, copy/tamper encrypted values
   or deny the configured key provider, then invoke actual use/write paths.
   Assert refusal, no cross-tenant success or secret leakage, and no partial
   overwrite of committed state. Race an existing replacement with revoke/update
   and assert fences preserve the winner. Server/Postgres integration tests cover
   RLS and owner metadata, beyond Task 1's isolated AAD checks. RED is missing
   consumer binding or unsafe remote-IO/commit integration.
5. **Preserve Gateway admission and preparation** — REQ-009.
   Load a managed credential through the common facade, warm a Gateway snapshot,
   then invoke the actual Gateway with a controlled provider fixture. Assert warm
   calls cause no wrapping-service requests or extra DB checks. Change/revoke the
   credential and exercise existing notification reload, generation fencing,
   listener loss and admitted-snapshot behavior. Integration and Gateway journey
   owners distinguish background preparation from warm admission. RED is remote
   decryption on an attempt or stale prepared material surviving owner refresh;
   already-correct lifecycle cases need regression proof, not artificial churn.
6. **Remove competing formats and setup** — REQ-012.
   On a fresh repository database, run the revised initial schema and fixture
   lifecycle, and inspect the resolved production caller/configuration graph.
   All recoverable owners use the facade, old crypto readers/configuration are
   absent, and excluded external-secret/signing paths still work. SQL integration
   and source/config checks provide static proof; do not add a test duplicating
   compiler guarantees. Update setup/team-deployment/provider references for all
   backends, including actual Vault Transit provisioning/auth/policy steps.

## Verification and handoff

Test owners: `wyrd-cli` local init/CLI journeys; `wyrd-server` integration modules
for Operator routes/delivery, identity, and platform; `wyrd-auth` PG tests;
`wyrd-testing` Gateway journeys; SDK/MCP consuming journeys. Existing runners:
`test:local-dev:journey`, `test:operators:integration`, `test:identity:journey`,
`test:platform:journey`, `test:gateway:journey`, `test:wyrdstate:journey`, and
`test:sql`, all via `mise run`. Select the focused new/changed tests during task
execution, using those lanes' Postgres/provider setup; integrated review owns
their full combined runs and each shipped language surface. Record real focused
selectors before RED rather than guessing new test names here.

Run affected owner/SQL tests, `mise run check:tenant-isolation`,
`mise run check:deps`, `mise run fmt`, `mise run lints`, and applicable
`mise run codegen:check` / `mise run docs:check` / `mise run check:examples`.
List every added/moved external wait and its authority. Gateway changes require
a full clean `mise run bench:l1:gateway` VALID run against the newest same-host/
experiment record; no smoke, subset, or dirty qualification. Apply the same
repository rule if another hot path changes.

Output: all owners, setup, persisted formats, fixtures, and existing runtime
lifecycle use the common system. Tasks 3–4 consume those rows and owner services;
neither must translate an old encryption format or duplicate initialization.
