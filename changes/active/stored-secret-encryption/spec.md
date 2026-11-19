---
id: SPEC-stored-secret-encryption
revision: 1
status: approved
---

# One mechanism for stored secrets

## Problem and outcome

Wyrd currently has two systems for encrypting stored secrets, with different
key configuration, encrypted formats, ownership binding, and rotation paths.
Auth and Gateway use deployment sealing; Operators use separate envelope
encryption. This duplication makes protection and administration inconsistent.
For example, `wyrd init` prepares only the sealing key, so creating a local
Operator connection fails with `WYRD_OPERATOR_503_KEY_UNAVAILABLE`.

The outcome is a revised, consistent, and standardized secret encryption
system for all current and future recoverable stored secrets. One server-owned
encryption and decryption path applies the same envelope format, tenant/owner
binding, and key lifecycle across auth, Gateway, Operators, and future owners.
Self-hosted single-tenant and multitenant SaaS deployments use that same system
with their selected wrapping backend. Administrators have clear rotation and
revocation operations under existing resource identities; clients keep their
references. Local initialization configures this common system without
feature-specific key setup.

This records the [finalized design](design/notes.md),
[administrator journey](design/admin-journey.md), and
[four journeys](design/journey-scenarios.md). The maintainer approved revision 1,
including that behavior and the authority/schema replacements below. Execution
continues through the repository's task workflow.

## Agreed approach and limits

This is a stored-secret encryption change, not an authentication change
packet. It changes how existing secrets, including auth-owned client secrets,
are protected at rest. Existing identity, credential acquisition, token
issuance/exchange, and authentication/session semantics retain their owners.
Adding a workload authentication method such as federation is outside this
packet; backend support does not implicitly require every provider login flow.

Use envelope encryption: a **data-encryption key (DEK)** encrypts one stored
secret value; a **key-encryption key (KEK)** wraps that DEK. One deployment KEK
protects fresh, independent, tenant-bound DEKs. There is no reused tenant-wide
DEK or intermediate tenant wrapping-key layer.

The shared `WyrdKms` facade generates the DEK, encrypts the value locally,
wraps the DEK through the selected backend, and returns one envelope for the
existing resource owner to persist. Reading reverses those steps. Remote
providers retain the KEK; Wyrd receives plaintext DEKs, not remote KEKs.
Prepared reads use this same implementation.

This separates database ciphertext from wrapping-key custody and confines a
leaked DEK to its one value. A compromised shared KEK, or a sufficiently
authorized compromised server identity, can expose every tenant's stored
secrets. Authenticated tenant binding prevents substitution; it does not stop
a compromised server from requesting decryption with valid context. Independent
tenant KMS revocation and tenant crypto-erasure of backup copies are not promised.
File mode has the same encryption mechanism, but its KEK is in process memory;
it does not provide remote KMS custody or protection against process compromise.

Remote wrapping/unwrapping adds provider availability and latency dependencies.
Prepare Gateway material during existing background snapshot loading so warm
calls do not acquire that dependency. Provider-native rotation alone neither
rewrites stored envelopes nor repairs exposure of a DEK or provider token.

## Required behavior

### Common protection

**REQ-001 — One encryption owner.** Every write and read of a recoverable
stored secret uses `WyrdKms`, including tenant auth-provider client secrets,
workload-issuer client secrets, platform OIDC, Gateway managed provider keys,
and Operator connection secrets. Existing resource owners retain authorization,
persistence, and domain lifecycle. Future recoverable secret owners use this
capability rather than adding another key source or cipher path. Provider
clients and wrapping adapters stay behind the facade; SDKs never own encryption.

**REQ-002 — Independent data keys.** On creation, credential replacement, or
encryption renewal, the facade generates a fresh random 32-byte DEK using the
OS cryptographic random source and encrypts with AES-256-GCM and a fresh nonce.
Each DEK protects one stored value. Routine DEK rewrapping may retain that DEK
and payload ciphertext. Neither plaintext DEKs nor KEKs are stored in Postgres.

**REQ-003 — Authenticated ownership.** Both payload protection and DEK wrapping
authenticate canonical context derived from verified authority and trusted row
metadata: tenant or explicit platform scope, purpose, owner identity, applicable
provider/credential authority, and secret version where the owner has one.
Operator context retains connection, provider, name, and secret-version binding;
Gateway retains tenant, credential name, and provider binding. Auth credentials
bind their connection/issuer identity and applicable client authentication
metadata. Platform secrets use a distinct platform scope, never an invented
tenant or an unscoped tenant secret. A context mismatch or tampered envelope
refuses use without trying another tenant or unrelated key source.

**REQ-004 — Common envelope and trusted key selection.** Stored values contain
one common envelope with format/algorithm identifiers, a logical wrapping-key
reference, an opaque backend-wrapped DEK, the payload nonce, and authenticated
ciphertext. Decryption resolves the reference only through deployment-configured
active/retained keys; ciphertext cannot select arbitrary endpoints or resources.
The backend validates the returned DEK length. Unknown formats, disallowed keys,
authentication failures, and provider failures refuse the affected operation;
there is no legacy reader or fallback that generates a substitute key.

### Deployment and key loading

**REQ-005 — One backend contract.** File, explicit development memory, GCP KMS,
AWS KMS, and Vault Transit implement authenticated wrap/unwrap under the same
facade and envelope. Deployment configuration selects the backend, its trusted
resource/endpoint and authentication, the active wrapping-key reference, and
retained readable references. Configuration is validated and redacted under the
existing [configuration authority](../../../architecture/operations/deployment-and-release.md#configuration-authority).
Backend identity and allowed-key changes use controlled deployment rollout;
an admin action cannot grant itself arbitrary cloud key access. No backend
silently falls back to another. Reusable provider clients refresh their access
credentials using the selected provider's existing authentication lifecycle.
Authenticate the wrapping API through existing supported credential sources;
this requirement does not introduce new identity or token-exchange flows.

| Deployment/backend | Setup and read/write behavior |
|---|---|
| Durable local or self-hosted file | `wyrd init` generates and atomically persists an owner-only KEK, reuses it after restart, and supplies the common facade to every owner. Replicas need the same active/retained keys; recovery needs preserved key files as well as the database. |
| Disposable development memory | Explicitly selected ephemeral KEK; loss of the process key makes previous envelopes unreadable. Durable init never selects it implicitly. |
| GCP KMS | Operator provisions a symmetric CryptoKey and grants the service identity encrypt/decrypt access. Wrap uses the configured key's primary version; unwrap supplies that allowed CryptoKey and the opaque ciphertext/context. KMS selects the required retained version. |
| AWS KMS | Operator provisions a symmetric KMS key and grants the service identity encrypt/decrypt access. Wrap/unwrap supply the allowed key identity and encryption context. KMS selects historical material internally; Wyrd does not invent addressable AWS material-version numbers. |
| Vault Transit | Operator enables Transit, creates a symmetric nonexportable encryption key, and gives Wyrd authenticated access to that named key's encrypt/decrypt operations, plus rewrap if used. Wyrd sends context and DEKs to Transit; it stores returned opaque ciphertext. Transit retains key versions. Vault KV raw-KEK retrieval is removed. |

Self-hosted installations may use a remote backend too. SaaS uses the same path:
tenant creation does not provision another cloud key, and secrets for different
tenants receive independent DEKs under the deployment KEK. Setup documentation
must show provider provisioning, service authentication/permissions, Wyrd
configuration, and preservation of old-key access for each supported backend.

### Resource administration

**REQ-006 — Rotate under the same identity.** An authorized owner-specific
Rotate action explicitly distinguishes **renew encryption** from **replace
credential**. Renewal opens the existing value and reseals it with a fresh DEK;
replacement accepts the owner's typed replacement and seals it without requiring
the old ciphertext to decrypt. Both preserve the resource identity and client
references. Existing replacement operations also use the common facade. Actual
auth value replacement retains candidate/test/activate validation and session
effects; encryption-only renewal does not change semantic auth revision or
invalidate sessions. An expired or exposed upstream credential requires a valid
replacement, not just renewed encryption.

**REQ-007 — Revoke through resource authority.** An authorized Revoke action
uses the owner's existing disable/revoke/deactivate lifecycle to forbid use and
refresh prepared material. Restart, stale loading, and a concurrent rotation
must not re-enable the resource. Re-enabling, where supported, remains a separate
authorized action. Other resources remain usable. Existing reversible Operator
disable is preserved, not replaced with destructive deletion.

**REQ-008 — Safe commit and response.** Resource actions authorize under verified
tenant/platform authority, prepare encryption outside database locks/transactions
that would wait on remote key calls, and atomically persist a complete envelope
only if the relevant secret and authority state have not changed. A racing
replacement/revocation returns a conflict rather than losing the change.
Validation, encryption, provider, or commit failure leaves prior committed state
intact. Responses, errors, logs, and audit expose redacted metadata, never keys
or secret plaintext. Transient key/plaintext buffers use the shared redaction
and zeroization facilities. No generic plaintext-decryption or database-selector
API is introduced. Rotate/Revoke ship through HTTP and the CLI using the
resource's existing permission boundary; SDKs and MCP do not expose them.
UI buttons are deferred.

**REQ-009 — Retain runtime lifecycle.** Committed actions use existing owner
cache refresh and invalidation. Gateway keeps post-commit notifications,
background reload, generation fencing, fail-closed listener loss, and writing
server warming; another replica may briefly use its previous snapshot during
propagation. Admitted calls retain their immutable snapshot. Operator delivery
checks latest authority/credentials on the next attempt. Prepared key material
is tied to tenant, owner, and exact secret revision/envelope, and follows that
owner's cache lifetime. Warm Gateway calls add no remote unwrap or per-request
database check. Do not add a global acknowledgement barrier, independent DEK
cache/TTL policy, or new session rules.

Wyrd revocation does not revoke a copied credential at its issuer. Rotation
requires no client reconfiguration but cannot prevent failures while a credential
is expired or invalid, or remove existing auth validation/session consequences.
API/docs must distinguish these effects and direct administrators to upstream
replacement/revocation where Wyrd has no supported issuer integration.

### Deployment wrapping-key lifecycle

**REQ-010 — Rotation and recovery.** A deployment/platform-authorized action
starts whole-store maintenance against a configured permitted replacement.
New writes use the activated replacement; retained keys keep existing values
readable during rollout. Routine rotation rewraps DEKs without changing values
or resource identities. Compromise recovery generates fresh DEKs and payload
ciphertext for affected secrets; rewrapping potentially exposed DEKs is
insufficient. Extend existing bounded, cancellable maintenance and CAS/version
fences across all secret owners, with current/changed/remaining counts and
redacted failed-owner diagnostics. Interrupted work resumes from stored state
without overwriting concurrent replacement or revocation. No new job/progress
table or recovery framework is required. Counts describe each maintenance pass:
already-current values, successfully transitioned values, and observed values
still requiring transition. Concurrent writes can change the next pass's counts;
a completed pass is not a global retirement or backup-safety acknowledgement.

**REQ-011 — Version retention and retirement.** Native GCP/Transit rotation
adds versions under the same key name; AWS retains historical material inside
the same logical key. The provider retains that material, not Wyrd's database.
Wyrd passes opaque wrapped DEKs back to the configured provider for decryption.
Replacing a logical key adds the replacement to deployment configuration,
activates it for writers, retains the old reference for reads, and transitions
stored envelopes. Old keys are retired only after live, prepared/cached, and
backup recovery dependencies are accounted for. Restore makes required keys
available before serving recovered credentials.

Disabling the shared wrapping key is deployment-wide containment, not tenant
revocation. A Wyrd-initiated disable must make that scope clear and trigger
existing owner invalidation for dependent material. Disabling a key directly in
the provider does not itself notify Wyrd or invalidate its caches; subsequent
provider calls refuse access. Neither path instantly erases cached plaintext
or cancels admitted operations. Ordinary disable does not
silently destroy recovery keys. A leaked provider identity also requires access
remediation: rotating material does not remove its authorization. No operation
can withdraw plaintext or ciphertext already copied by an attacker.

### Replacement and documentation

**REQ-012 — Remove the competing systems.** Delete direct deployment sealing
and its `SealingKeyring` versioned/unversioned readers, Operator-only raw-KEK
ownership and Vault KV interpretation, and duplicate rotation orchestration.
Replace `OperatorKeysConfig`, `[verification.operator_keys]`,
`WYRD_OPERATOR_KEK_*` including `WYRD_OPERATOR_KEK_V<n>`, and
`WYRD_SEALING_KEY_{FILE,BASE64}` / `WYRD_SEALING_RETAINED_KEYS_{FILE,BASE64}`
with one common typed backend configuration and its canonical projections.
No aliases or compatibility readers remain. Retain shared crypto primitives and
the Vault KV reader used for external Gateway references.

Update server composition, encrypted-column readers/writers, fixtures, initial
schema definitions, and setup/deployment/rotation/recovery documentation together.
The parked failed-verification Operator guide must become runnable through the
init journey. Documentation explains encryption renewal, credential replacement,
resource revocation, wrapping-key compromise recovery, and upstream limits.

## Persistent and authority changes authorized by this revision

Nothing has shipped. The maintainer explicitly permits replacing initial SQL
definitions and stored formats directly; there is no existing-data migration,
backfill, old-format reader, or compatibility deployment plan.

The common envelope replaces the contents of existing `client_secret_enc`
columns in `wyrd.auth_human_connections`, `wyrd.auth_trusted_issuers`, and
`platform.oidc_connection`, and `secret_ciphertext` in
`wyrd.gateway_provider_credentials` and `wyrd.operator_connections`.
For Operators, consolidate the envelope into `secret_ciphertext`, remove
`secret_nonce`, `wrapped_dek`, `dek_nonce`, and `key_version`, and remove the
`operator_connections_key_version` index tied to the old format. Retain
`secret_version` and domain authority/lineage fields. Update the corresponding
initial migration definitions and their format comments, including the Gateway
sealing-keyring definition. No new table is authorized. Tenant/RLS boundaries
remain unchanged.

Before implementing the replacement contracts, update the owning
[security posture](../../../architecture/wyrd-security-posture.md) stored-secret
and tenant-isolation sections and
[Operator connection design](../../../architecture/wyrd-design.md#operator)
to describe the shared facade, independent tenant-bound DEKs, shared KEK risk,
backend choices, and retained domain lifecycle. This replaces the mandatory
Operator-specific Vault KV/per-tenant-KEK rule and the auth/Gateway-only sealing
rule. It does not weaken unrelated Bifrost/storage encryption or erasure rules.
Update [deployment configuration](../../../architecture/operations/deployment-and-release.md#configuration-authority)
and [recovery guidance](../../../architecture/operations/reliability-and-recovery.md#backup-contract)
for the common key lifecycle; preserve their configuration and restore authority.
Approval authorizes those authority updates; the updates must be made before
the replacement contracts are implemented.

Signing, password hashing/token digests, Wyrd-issued API-key/token lifecycle,
external Gateway secret references, and object-store encryption are excluded.
Call backend provider APIs directly where possible, reusing the existing HTTP
clients and credential/signing mechanisms. Do not add a vendor SDK or another
dependency when those mechanisms satisfy the encryption/key-lifecycle need.
If a named requirement cannot be met with that reuse, explain the concrete
limitation and proposed dependency, including its transitive/runtime costs,
to the maintainer before adding it. Backend integration does not authorize
new authentication capabilities. Private crate placement, method signatures,
envelope binary encoding, and test fixtures remain implementation decisions
within this contract.

## Acceptance observations

These are four user journeys, with supporting owner/backend and negative checks,
not additional workflows. Plans reference the requirement IDs below without
creating another behavioral contract.

| Journey | Observation and expected proof | Requirements |
|---|---|---|
| Get started and use stored credentials | Initialize a durable local server, create an Operator connection without extra key setup, fail a Verifier, and observe delivery; restart and repeat. Exercise actual use of auth, issuer, platform, and Gateway secrets through the common facade, including distinct tenants and each backend's integration boundary. | REQ-001–005, REQ-012 |
| Rotate without changing clients | Renew encryption and replace an expired/leaked credential while retaining client references. Verify fresh DEKs, unchanged value/session semantics for renewal, existing auth activation behavior for replacement, and replacement when the old envelope cannot open. Inject provider failure and race replacement/revocation; committed state is preserved or the losing write conflicts. | REQ-002, REQ-006, REQ-008–009 |
| Revoke and stop use | Revoke an in-use resource, exercise owner refresh including replicas and restart, and observe refusal under the retained propagation/admission rules. Race rotation against revocation; it cannot re-enable the resource. Another resource still works. | REQ-007–009 |
| Rotate the deployment wrapping key safely | Exercise routine rewrap and compromise recovery across owners/tenants, interrupt/resume maintenance, and race a credential update. Verify truthful counts, retained old reads, safe retirement/restore dependencies, and explicit deployment-wide disable effects. | REQ-010–011 |

Supporting crypto/integration checks demonstrate independent DEKs, tenant/owner/
purpose binding, tamper and unknown-key refusal, permissions, redaction, and
provider-outage behavior through the real consuming owners. Journey proof uses
the real server and repository Postgres on each shipped public projection;
provider adapter checks do not substitute for the consuming journey.
Supporting provider proof includes small live GCP/AWS integration tests and
the existing real dev Vault Transit test. Follow storage integration-test
layout, naming, mise runners, and credentialed CI organization. Reuse existing
provider APIs and credential mechanisms; live tests add no authentication
capability or provider SDK requirement.
Hot-path verification enumerates added/moved external waits and their authority.
The maintainer withdrew automatic benchmark qualification on 2026-11-19;
benchmarks require an explicit request for the current change. No runtime
tests, cloud experiments, or benchmarks have been executed for this specification.

## Approval and revision

Revision 1 records the finalized design. No new cache, session, distributed
completion, or maintenance-policy decision is requested. Approval accepts the
behavior, backend coverage, shared-KEK limits, four journeys, and named
authority/initial-schema replacements. The maintainer explicitly approved
revision 1 and requested the four agreed implementation tasks. This records
approval without changing the revision's behavioral requirements.
