# Data-layer domain review

Result: **PASS** for the reviewed tenancy, concurrency, and persistence boundary. No material findings proposed. This is source review, not a claim that execution gates passed.

## Immutable subject and authority

- Base: `526355fb8c58dae6d67d516f98dddcd5c02b0e79`.
- Candidate: `202e5bab768cb998a01f04555c39f43b53c09929`.
- Specification: `changes/active/sealing-keyring/spec.md`, revision 1, REQ-007 and INV-001/002.
- No original task exists; the human explicitly authorizes specification-and-commit review. Nothing has shipped, so compatibility with previously deployed Gateway envelopes is not required.
- Governing boundaries: `AGENTS.md` §§2–6, 9, 11–12, 15; `architecture/agent-rules.md` SQL capabilities and transaction ownership; `architecture/wyrd-design.md` server ownership and verified tenant identity; `architecture/wyrd-security-posture.md` trust boundaries, stored secrets, tenancy, cryptography; `architecture/operations/deployment-and-release.md` database and persistent-state ownership; `architecture/operations/reliability-and-recovery.md` secret uncertainty and recovery. Review discipline follows the spec-driven-development and maintainer-style references and `TESTING.md`.

## Boundary and source coverage

CodeGraph was used before source navigation. The complete cumulative diff was inspected, with deeper tracing through these owners and consumers:

| Changed boundary | Source and caller evidence | Assessment |
| --- | --- | --- |
| Tenant credential persistence | `GatewayAdministration::put_credential` and `seal` in server `components/gateway/service.rs`; `GatewayCredentialWrite` and `upsert_gateway_credential` in SQL `queries/gateway.rs` | The verified caller's tenant enters both the binding and `TenantConn`. The service seals before the upsert and owns commit. The SQL helper receives a tenant capability, not an operator or raw pool. A rejected seal leaves the previous row untouched. The existing atomic upsert and deployment/provider foreign key remain intact. |
| Durable schema and tenancy | `20260924000000_gateway_administration.sql:18–57`; `20261014000000_gateway_managed_secret_sealing_keyring.sql:8–10` | Dropping the obsolete key-version and nonce columns preserves the separate CHECK equating managed source with ciphertext presence. The tenant/name primary key, provider composite foreign key target, forced RLS, tenant policy, and privileged operator policy remain intact. No deployed-data compatibility path is needed. |
| Admission snapshot and dispatch | SQL `SNAPSHOT_SQL`; server `GatewayAdministration::snapshot`/`credential_snapshot`; gateway `snapshot.rs`, `CredentialResolver::resolve`, `ManagedSecretBinding::open` | The existing one-statement snapshot observes one tenant configuration under RLS. Only opaque ciphertext enters the snapshot. Dispatch checks active state and provider and verifies the authenticated payload's tenant, name, and provider before returning a credential. Malformed, absent, or foreign ciphertext fails before provider dispatch. |
| Rotation and concurrent credential replacement | Auth `SealedSecretRewrap::run`/`settle`; SQL `SealedSecretTable::ALL`, `select_sql`, `swap_sql`, `sealed_tenant_secrets`, `swap_sealed_tenant_secret`; server `rewrap_sealed_secrets` | Gateway joins the existing cross-tenant OperatorPool pass, including revoked rows. The CAS matches tenant, name, and the complete previous ciphertext. A simultaneous administrative replacement, source change, deletion, or another replica's rewrap cannot be overwritten. Lost CAS counts as remaining. Each successful statement persists independently, so interruption is resumable. No new transaction or lock authority duplicates the existing owner. |
| Shared envelope and key ownership | Existing `wyrd-crypt::SealingKeyring::{seal,open,rewrap}`; server `build_sealing_keyring` and boot resolver injection; server `AppState.auth.sealing_key` | Gateway uses the existing envelope, retained keys, and boot rewrap rather than introducing a second encryption lifecycle. Rewrap preserves the bound plaintext. Missing retained keys refuse affected use; keyless boot accounts for stored Gateway ciphertext through the same pass. Key retirement still requires the existing all-writers-switched procedure documented on the rewrap owner. |
| Local key lifetime | CLI `DevArgs::run`, `DevServer::{new,command}`, `local_sealing_key`; `ServerInstaller::{official,root}` | The key is created before server child boot at the installation root, outside version directories. All child commands receive the same key path. Explicit key environment inputs take precedence. An owner-only temporary file is fully written and synced before no-clobber publication; concurrent creators select the same winning path, and interruption during staging does not publish a partial key. Existing files are reused rather than replaced. |

## Reuse assessment

The existing durable owners are sufficient: SQL tenant capabilities and atomic credential upsert, the one-statement admission snapshot, the deployment `SealingKeyring`, and the existing OperatorPool rewrap with exact-byte CAS. The candidate extends those owners and removes the separate Gateway keyring. The new local file creation fills the actual missing CLI provisioning behavior and reuses installed `tempfile` support. No second database authority, duplicate rotation runner, or per-tenant key lifecycle remains in the changed path.

## Failure and recovery assessment

- A failed administrative seal or write does not publish a partial credential or replace a prior usable secret.
- A process interrupted during rewrap leaves completed row updates durable and remaining rows available through retained keys; the next boot resumes by inspecting their envelopes.
- Two rewrap passes race safely on exact ciphertext. An administrator changing the credential cannot have that change rolled back by a stale rewrap read.
- Copying a valid ciphertext into another tenant or credential row does not cross tenancy, even though tenants share the deployment keyring: the runtime verifies the encrypted binding before exposing the key to dispatch.
- A first-run CLI interruption before key publication leaves no final partial file; interruption after publication allows the next run to reuse the same key. Replacement of a server release does not move or remove the key root.
- Host power-loss qualification and filesystem-specific persistence guarantees were not exercised. No broader availability or storage promise is inferred from the local development requirement.

## Proof and limits

Source proof was inspected in `gateway_managed_secrets_persist_sealed_and_bound_envelopes`, `gateway_managed_credentials_resolve_per_tenant_across_restart_and_rotation`, the managed-binding unit tests, and the installed-CLI journey `server_dev_keeps_the_gateway_managed_secret_key`. They cover persisted redaction, failed replacement preservation, foreign envelope rejection, two-tenant dispatch, restart, production boot rewrap, retained-key retirement, and CLI first boot/restart. The CLI journey exercises a real saved credential and public Gateway HTTP path against a mock provider.

No tests, migration rehearsal, format/lint lane, or codegen check were executed in this independent discovery pass, and none had recorded results in the supplied subject. Concurrent first-run key publication and CAS races were assessed from the native no-clobber operation and SQL predicates, not from an executed race test. Overall acceptance still requires the orchestrator's reconciliation of AC-001/AC-003 evidence; this report establishes no passing execution evidence for those obligations.

## Proposed finding ledger

Empty. No material source-backed tenancy, concurrency, or persistence defect was found within the approved boundary. There is no remediation recommendation and no compatibility or deployed-data migration finding.
