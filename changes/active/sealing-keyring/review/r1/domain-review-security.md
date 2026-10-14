# Security domain review

Result: PASS (source review; execution limits below).

## Immutable subject and boundary

Reviewed base `526355fb8c58dae6d67d516f98dddcd5c02b0e79` through candidate
`202e5bab768cb998a01f04555c39f43b53c09929`, against revision 1 of
`changes/active/sealing-keyring/spec.md`. No original task exists by explicit
human direction. Nothing has shipped; compatibility with the removed Gateway
key format is outside the required behavior.

Scope: deployment-key reuse, authenticated credential identity, local key
provisioning, secret disclosure, and changed authorization/trust boundaries.
The cumulative diff was inspected across cryptographic consumers, server
wiring, SQL, CLI, harness, contracts/schema, documentation, and lane changes.
CodeGraph was the initial navigation mechanism.

## Authority and source coverage

| Boundary | Authority | Source evidence and assessment |
| --- | --- | --- |
| Encryption owner and envelope | REQ-007; security posture, OAuth authorization-server section; AGENTS.md §§3–4 | `wyrd-gateway/src/managed.rs:91–154` delegates both encryption and opening to the existing `wyrd_crypt::SealingKeyring`. The existing crypt owner uses AES-256-GCM with fresh OS-random nonces and versioned key identifiers; the candidate adds no competing cipher, envelope, or fallback key-selection mechanism. |
| Tenant/name/provider binding | REQ-007; INV-001; security posture deployment-key paragraph | `ManagedSecretBinding::seal` writes all three identities inside the authenticated plaintext. `open` decrypts before parsing and checks all three before returning `payload.secret`; mismatches, malformed payloads, missing keys, and failed authentication return the same non-secret `Unavailable` error. Sharing the deployment key therefore does not remove identity binding. |
| Permission and tenant authority | INV-001; AGENTS.md §§9–10; agent rules SQL capability and audit requirements | `GatewayAdministration::put_credential` obtains the tenant from `Caller`, authorizes through the existing `audited` method before acquiring its `TenantConn`, checks path/body name equality, and seals before persistence. The resolver checks active lifecycle and provider equality before decrypting. Existing environment/Vault assignment checks remain in place. |
| Redacted write/read contracts and logs | INV-002; AGENTS.md §4 secret handling; security posture secret-disclosure rules | `wyrd-spec/src/gateway/credential.rs` retains `SecretBearer` in the write contract and the memberless `ManagedSecret` read view. Administration serializes the redacted view separately from opaque sealed bytes. Routes retain `skip_all` instrumentation and sanitized JSON extraction errors. `SealedPayload` uses `SecretString` with explicit redacted Debug; serialized/decrypted buffers are `Zeroizing`. Changed protection errors contain static reasons, not values. |
| Shared runtime key authority | REQ-007 | `boot/mod.rs::attach_config_fields` supplies `state.auth.sealing_key` to the resolver; `GatewayAdministration::new` reads the same state field. `GatewayConfig` and exports remove the tenant-keyring types and loaders. No alternate managed-secret key authority remains in the changed path. |
| Local key file | REQ-007; INV-002; security posture mounted-secret boundary | `wyrd-cli/src/server/dev.rs:541–573` respects explicit file/inline sources, generates 32 random bytes using the installed rand dependency, stages base64 in an owner-only tempfile, syncs file content, and publishes without clobbering an existing file. Competing creators reuse the winning pathname. The existing server loader (`config.rs:3672–3691` and `wyrd_utils::secret::read_secret_file`) validates the opened handle as regular, restrictive on Unix, and bounded before decoding. Existing malformed/permissive files are refused rather than replaced. Commands receive a pathname, never key contents; the new error path suppresses underlying content. |
| Rewrap security and persistence | REQ-007; INV-001 | `SealedSecretTable::GatewayProviderCredentials` reads opaque bytes with `OperatorPool` and compares tenant, row name, and previous ciphertext on update. The existing `SealedSecretRewrap` retains its key-selection and secret-free diagnostics. Rewrap preserves the bound payload; subsequent resolution still validates identity. It neither creates a new permission-bearing path nor returns plaintext. |

Also considered the applicable spec-driven-development authority hierarchy,
maintainer guide, repository reference router, relevant design identity/server
ownership guidance, and the security-posture change itself. The latter explicitly
adds Gateway secrets to deployment-key protection and retains authenticated
identity binding, consistently with the approved spec.

## Proof assessment

The source includes direct negative proof for the security changes:

- `managed::tests::sealed_values_open_only_for_their_own_binding` covers foreign
  tenant, name, provider, absent key, and tampering; the adjacent redacted-payload
  test checks Debug disclosure and successful round-trip.
- `gateway_managed_secrets_persist_sealed_and_bound_envelopes` checks persisted
  rows and redacted responses, copied envelopes, and preservation after a
  rejected blank replacement.
- `gateway_managed_credentials_resolve_per_tenant_across_restart_and_rotation`
  follows two tenants through retained-key rotation and production rewrap,
  rejects cross-tenant copied ciphertext and revoked/missing authority before
  provider dispatch, and checks the source tenant still works.
- Existing `sealing_key_files_require_a_restrictive_regular_bounded_file`
  covers the retained mounted-file security owner reused by the CLI.
- The new installed-CLI journey stores the quickstart-shaped managed secret,
  calls the provider, restarts, calls again, and checks captured CLI/server/
  gateway output for the submitted provider key. It exercises ordinary dev
  boot rather than the test harness's injected key.

These are inspected test bodies, not claimed passing executions. No runtime
checks, broad suites, external penetration tests, or gate runs were performed
in this independent discovery review; no execution evidence accompanied the
subject. In particular the CLI journey's final leak assertion targets the
provider-key sentinel; key-content non-disclosure was assessed from the new
writer, command construction, loader, and logging source. This is not an
independent acceptance of AC-003.

## Material findings

None. No reachable credential disclosure, authorization bypass, or loss of
required identity binding was established in the cumulative candidate. No new
security machinery or compatibility path is required.

Skipped runtime verification; the PASS above is limited to this security-domain
source audit and does not certify the complete change's verification gates.
