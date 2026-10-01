---
id: TASK-007-R4
kind: remediation
status: ready
spec: SPEC-verified-change-contract
spec_revision: 36
requirements: [REQ-147, REQ-150, AC-031]
depends_on: []
parent_task: TASK-007
remediates: [FIND-TASK-007-15, FIND-TASK-007-16, FIND-TASK-007-17]
---

# TASK-007-R4 — Record the Approved Operator Key Source and Close Remaining Findings

## Authority and immutable subject

- Spec: `changes/active/verified-change-contract/spec.md`, revision 35 → 36 (this task)
- Original task: `changes/active/verified-change-contract/tasks/TASK-007-operator-connections-and-delivery.md`
- Review ledger: `changes/active/verified-change-contract/review/TASK-007-r4/findings-validation.md`
- Base: `f8811ac5035c3aa165d34c38992f9889b3c9081f`
- Reviewed candidate: `e2da2769`

## Human decision (approved by the user on 2026-09-24)

The R4 review returned `SPEC_REVISION_REQUIRED` for `FIND-TASK-007-16`: revision
35 and TASK-007 assumed an existing backend-agnostic `SecretRef::Vault` resolver
(`wyrd_auth::resolve`) supplies tenant KEKs, but no such resolver exists. The
user chose to approve the implemented key source rather than build a shared
resolver:

- Tenant key-encryption keys are read by the server-owned `OperatorKeys` owner
  from exactly one configured source: an environment variable, owner-only
  mounted files, or HashiCorp Vault KV v2 over the already-installed `reqwest`.
- Multi-tenant production requires the HashiCorp Vault source over HTTPS and
  fails start unless every active tenant key is readable before readiness.
- Without a readable key, only connection create/update (sealing) refuses;
  other surfaces keep working and delivery retries `credential_store_unavailable`.
- A shared `SecretRef` resolver and AWS Secrets Manager / Google Secret
  Manager KEK sources are deferred; `SecretRef` is not the KEK configuration
  contract for this change.

## Required corrections

### `FIND-TASK-007-16` — spec revision 36

Edit `changes/active/verified-change-contract/spec.md` only to record the
decision above:

1. Bump front matter `revision: 35` → `revision: 36` (status stays `approved`).
2. Amend REQ-147's KEK sentences so the "deployment secret/KMS boundary" is
   the three sources above with the multi-tenant production Vault/HTTPS and
   fail-start rules; remove any requirement to reuse a `SecretRef`/external
   secret resolver for KEKs, and state the deferrals.
3. Fix any other spec sentence that names the `SecretRef::Vault` resolver as
   the KEK source (grep `SecretRef`, `resolver`, `KMS`).
4. Add a `Revision 36 Operator key source (2026-09-24)` entry to the revision
   history in the existing style, ending "This revision was explicitly
   approved by the user on 2026-09-24."
5. Update "Open material decisions" to name revision 36 as approved on
   2026-09-24.
6. Update the original TASK-007 file's `spec_revision` and the lines that
   require reusing the `SecretRef::Vault` resolver (around lines 23, 45, 68,
   100, 102, 198) so the task matches revision 36. Keep
   `architecture/wyrd-design.md` consistent with the same wording.

Do not change product behavior; the implementation already matches this
decision. Do not add a resolver, trait, or new provider.

### `FIND-TASK-007-15` — module-scope imports

Import the already-used declaration types at module scope and use bare names
at every candidate-added declaration/type position enumerated in
`review/TASK-007-r4/findings-validation.md` (`operator.rs`, `keys.rs`,
`operators.rs`, the private provider modules, `operator_dispatches.rs`,
`wyrd-testing::server`). Expression and macro paths are excluded. Sweep the
full `f8811ac5..HEAD` diff for any remaining candidate-added fully qualified
declaration types so this does not reopen.

### `FIND-TASK-007-17` — MCP list descriptor

Remove the false "secret version" promise from the Operator connection list
MCP descriptor (`mcp/operators.rs` `descriptors_unscoped`) and pin the
descriptor text with the existing MCP catalog proof.

## Verification

- `mise run fmt`, `mise run lints`, `git diff --check`
- `mise run codegen:check` (MCP descriptor)
- `mise run test:bifrost:journey:mcp` and the focused MCP catalog test by exact
  nextest expression
- `mise run test:wyrd`
- Final `git status` clean

## Evidence

Append `| Acceptance criterion | Implementation evidence | Verification evidence | Result |`
rows for each finding here.

| Acceptance criterion | Implementation evidence | Verification evidence | Result |
|---|---|---|---|
| `FIND-TASK-007-16` spec revision 36 records the user-approved Operator key source (2026-09-24) | `spec.md` front matter `revision: 36`; REQ-147 KEK paragraph (env / owner-only files / HashiCorp Vault KV v2 over `reqwest`, multi-tenant production Vault over HTTPS with fail-start, missing key refuses only sealing, delivery retries `credential_store_unavailable`, `SecretRef` resolver and AWS/GCP sources deferred); `Revision 36 Operator key source (2026-09-24)` history entry; Open material decisions names revision 36; TASK-007 `spec_revision: 36` and owner/approach/Scenario 1 and 3/write-set lines; `architecture/wyrd-design.md` Operator connections paragraph | `grep -n 'SecretRef\|resolver\|KMS'` over spec and TASK-007 shows only the deferral and revision-history text; no product code changed for this finding | PASS |
| `FIND-TASK-007-15` candidate-added declaration types imported at module scope and used by bare name | `keys.rs` (`Client`, `Value`, `SqlxError`, test `io`/`Write`/`MakeWriter`), `verification/operators.rs` (`Client`, `ReqwestError`, `Value`, `BTreeMap`), `operators/slack.rs`, `operators/pager_duty.rs`, `wyrd-spec/src/card/operator.rs`, `wyrd-spec/src/operator_connection.rs` (`Url`, `Display`, `Formatter`, `Deserializer`), `wyrd-spec/src/error.rs` (two candidate `details` fields), `operator_dispatches.rs` (`IdError`), `boot/mod.rs` (`WyrdServerConfig`), `mcp/mod.rs` (`Serialize`, `CallToolResult`), `mcp/operators.rs` (`DeserializeOwned`), `wyrd-testing/src/server.rs` (`TempDir`, `OperatorKeysConfig`, `OperatorKeySource`, `Path`), `pg_operator_connection_routes.rs`, `pg_operator_delivery.rs` (`MockRequest`), `wyrd-sdk-rust/tests/operator_connections.rs` (`Debug`) | Full `f8811ac5..HEAD` added-line sweep for qualified paths in field, parameter, return, bound, and impl positions returns only `io::Result` (module-qualified, matching the accepted `fmt::Result` precedent); `mise run fmt`, `mise run lints` exit 0 | PASS |
| `FIND-TASK-007-17` MCP list descriptor no longer promises a secret version | `crates/wyrd/wyrd-server/src/mcp/operators.rs` `descriptors_unscoped`; descriptor text pinned in `crates/wyrd/wyrd-mcp/tests/bifrost/mcp/discovery.rs` | `mise exec -- cargo nextest run --locked -p wyrd-mcp --test mcp -P journey --run-ignored=all -E 'test(=discovery::pg_tests::agent_discovers_only_authorized_tables_and_layout)'` (under `scripts/postgres/with-test-postgres.sh` + `db:migrate:inner`) PASS; `mise run test:bifrost:journey:mcp` 11/11 PASS; `mise run codegen:check` exit 0 | PASS |

Verification run in this session: `mise run fmt`, `mise run lints`, `git diff --check`,
`mise run codegen:check`, the focused MCP catalog test above,
`mise run test:bifrost:journey:mcp` (11 passed), and `mise run test:wyrd`
(2158 passed, 120 skipped) all exited 0. Non-goals held: no resolver, trait,
provider, dependency, or behavior change was added.
