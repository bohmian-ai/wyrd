# Rotate and revoke: the administrator's journey

Status: reference for the finalized design.
The operator model is agreed. Existing owner lifecycle semantics are retained;
proposed route and implementation details are identified below. No specification or implementation approval
is implied.

## The operator contract

An administrator chooses a resource and uses **Rotate** or **Revoke** through
one server API action, projected into CLI, SDK, MCP, and eventually UI. Clients
continue referencing the same connection/credential identity. The server owns
key generation, wrapping, encrypted persistence, and cache transition.

This applies to recoverable secrets the server stores and uses on clients'
behalf. An API key presented by an external client is different: replacing
that credential requires delivering its replacement to the client. This design
does not change Wyrd's API-key issuance, hashing, or authentication contracts.

The cryptographic implementation remains one `WyrdKms` facade across every
secret owner and backend. The operator-facing operation is a resource lifecycle
action, not a raw encrypt/decrypt endpoint.

## The incidents and the action

| Incident | Operator action | What the server does | What remains outside the action |
|---|---|---|---|
| A stored secret's DEK leaked | Rotate the selected secret's encryption | Open the current value, generate a fresh DEK/nonce, encrypt and wrap using the active deployment key, atomically replace the envelope | Copied old ciphertext still decrypts with a stolen old DEK; replace the actual credential too if its value may have been exposed |
| A provider token/password leaked | Rotate the selected credential using a valid replacement; revoke old value at its issuer | Persist the new value through the common fresh-DEK encryption path, preserve its resource identity, and transition runtime material | Wyrd cannot generically mint or revoke another service's credentials without a supported issuer integration |
| A provider token/password expired | Rotate with a valid replacement | Same replacement path; no client configuration change | Already-failed calls cannot be repaired; an expired upstream credential cannot keep authenticating while a replacement is missing |
| Deployment wrapping key leaked | Deployment administrator rotates/replaces the wrapping key and starts recovery for all stored secrets | Switch new writers to a safe replacement; regenerate DEKs and ciphertext for every affected secret through bounded, fenced background passes | Rewrapping old DEKs alone is insufficient if they may be exposed; revoke compromised provider access credentials too |
| Access credentials used to call KMS expired | Restore/renew the workload identity or Vault authentication | The provider client uses refreshed access credentials; unchanged secret envelopes do not need rotation solely because a login token expired | If identity renewal fails, provider operations can remain unavailable; never generate replacement KEKs to hide the outage |
| A selected credential must stop being used | Revoke that resource | Persist disabled/revoked authority, invalidate prepared material, refuse further use under that authority, and record the audit decision | Upstream revocation is a separate issuer operation unless explicitly supported; admitted/in-flight calls and cache propagation retain the existing owner behavior |
| The wrapping key must stop being usable | Deployment administrator revokes/disables it | Stop new wrapping/unwrapping through the facade; invalidate dependent prepared material through the existing owners | All tenants using that wrapping key are affected; provider disable and already-cached material are not an instantaneous global erase |

The main security distinction is operationally visible: refreshing encryption
protects the newly stored representation. It does not revoke a copied Slack
token. Replacing that token and revoking it at Slack does. A shared wrapping-key
incident is deployment-wide, not a selected-secret repair.

## Rotate a selected secret

The action stays on the existing owning resource API. An illustrative route is
`POST /v1/operator-connections/{id}/rotate`; its final spelling/body is a spec
decision. Other owners expose the equivalent action under their own permissions.
One facade is not a reason to bypass domain authorization with an arbitrary
ciphertext or database-selector endpoint.

The operator's Rotate action distinguishes two modes:

- **Renew encryption:** no replacement credential is supplied. The server opens
  the existing value and seals it with a fresh DEK and nonce.
- **Replace credential:** a replacement is supplied in the owner's existing
  typed secret shape. The server seals it directly with a fresh DEK; it need not
  decrypt the old value first. This remains possible when old ciphertext cannot
  be opened, provided a valid active wrapping key is available.

The API/UI must name the mode and its effect explicitly. It cannot report that
a leaked provider credential was revoked merely because encryption was renewed.
Existing replacement operations remain usable and route through the same facade;
do not add a competing generic secret store or duplicate credential lifecycle.

The one-resource operation is bounded and needs no new job/progress table:

1. Authenticate and authorize the owner-specific write; stage audit on the
   existing Scribe outbox. The resource is resolved under verified tenant or
   platform authority, never under a caller-supplied unverified tenant.
2. Read a coherent current envelope and binding metadata. Release the database
   transaction before a remote key operation.
3. Renew encryption or seal the replacement through the shared facade. Keep
   plaintext and transient keys in redacted/zeroizing buffers.
4. Compare-and-swap the complete envelope against the read version/bytes and
   relevant binding metadata. A concurrent replacement or revocation wins;
   return a conflict rather than overwrite it or re-enable the resource.
5. Commit the complete update, trigger the existing owner cache transition, and
   return redacted metadata under the existing owner response semantics.

Provider failures, malformed input, binding mismatch, or a failed commit leave
the current stored value intact. No partial encryption state is committed.
If the response is lost, retrying may produce another fresh encryption of the
same value; it must not lose, empty, or expose the secret. The action never
accepts or returns plaintext data keys, wrapping keys, or raw ciphertext.

## Rotate without requiring client changes

The stable resource name is the client's indirection. A binding or Operator
Card continues naming the same connection; the server changes which secret
version it uses. No client redeployment, new Card declaration, or application
configuration change is required for that server-stored credential.

For planned provider-credential rotation, the operational sequence is: obtain a
valid replacement, validate it where supported, switch Wyrd's active value,
allow the defined runtime transition, then revoke the old provider credential.
An issuer that permits overlapping credentials makes this possible. Do not
claim uninterrupted authentication when the old credential has already expired,
the issuer immediately invalidates it, or a replacement is invalid.

Encryption-only renewal preserves domain identity and lifecycle: resealing
unchanged auth plaintext does not create a new connection revision or log users
out. Actual auth-provider credential replacement retains the existing
candidate/test/activate workflow and its session-revision effects. Do not bypass
that security workflow or invent a new session-preservation policy. Stable
client references do not promise that an existing owner-mandated sign-in or
validation step disappears.

## Revoke is a resource state transition

Revocation is not deleting the data key and hoping every cache forgets it.
The server commits an authority state that forbids use, triggers invalidation,
and prevents stale loading from reactivating the resource. Cached plaintext
and cached DEKs must not bypass that state. Keep redacted history/lineage under
the existing owner's retention policy; do not accidentally change reversible
Operator disable into destructive deletion.

Revoking a resource blocks its intended capability. It is not a zero-downtime
operation for callers relying exclusively on that credential; those callers
must receive a clear failure rather than silently use an unauthorized fallback.
Rotating a revoked resource does not implicitly re-enable it. Re-enabling, where
supported, is the owner's separate authorized action.

The server cannot withdraw a token already copied to an attacker. Upstream
revocation must happen at the issuer. If a future integration supports issuer
revocation, it must report upstream success/failure separately and never claim
that merely disabling the Wyrd resource accomplished it.

## Preserve existing propagation and in-flight behavior

`GatewaySnapshots` already propagates committed changes through Postgres
notifications and background reload, fences stale installs by generation, and
fails closed when its listener disconnects. The writing server warms its own
snapshot after commit. Another replica can briefly serve its previous snapshot
while the notification/reload propagates. Preserve that contract; an admin
response is not a new global acknowledgement barrier.

An admitted Gateway call retains its immutable snapshot. New admission uses the
owner's current snapshot; already-admitted work follows the existing call
lifecycle. Operator delivery retains its existing next-attempt credential and
authority checks. Prepared DEKs follow these owner lifetimes and invalidation,
not a second cache with a new TTL policy.

Use the existing resource write/revoke response conventions. Do not add a
committed-versus-applied state machine, distributed completion protocol,
coordination service, or per-request Postgres check. Provider disable is also
subject to provider propagation and does not erase plaintext keys already in
memory; do not describe it as instantaneous global erasure.

Sources: [AWS key disabling](https://docs.aws.amazon.com/kms/latest/developerguide/enabling-keys.html),
[GCP key-version disabling](https://docs.cloud.google.com/kms/docs/enable-disable).

## Deployment wrapping-key operation

Selected-secret actions do not change the deployment wrapping key. The
deployment administrator has a separate platform-authorized action to activate
a permitted replacement and run whole-store maintenance. It must expose counts
and failed owners, resume after interruption, preserve concurrent credential
changes, and preserve old-key read access until transition/backup policy permits
retirement. Routine maintenance may rewrap existing DEKs; suspected DEK exposure
requires fresh DEKs and payload re-encryption.

Cloud/provider setup determines how the replacement is provisioned and how
activation becomes authoritative across replicas. Do not silently grant every
tenant credential writer cloud key-admin privileges. A provider identity leak
also requires identity/access remediation; rotating key material does not revoke
an IAM credential that remains authorized on the replacement.

Disabling the shared wrapping key is an explicit deployment-wide action with a
clear affected-tenant warning, not a tenant's revoke button. Permanently
destroying it is a separate destructive action governed by backup/retirement
proof; ordinary revoke must not silently destroy data-recovery capability.

## Implementation reuse and boundary changes

- [Operator update/disable](../../../../crates/wyrd/wyrd-server/src/components/operators/service.rs)
  already preserves identity and increments secret versions for replacement.
  Its current update holds a database row lock while awaiting key operations;
  move remote operations outside the transaction and preserve concurrency through
  the CAS contract above.
- [Gateway administration](../../../../crates/wyrd/wyrd-server/src/components/gateway/service.rs)
  already replaces credentials on the same name and has revocation semantics;
  do not create a parallel registry.
- [Auth connection lifecycle](../../../../crates/wyrd/wyrd-auth/src/connections.rs)
  owns candidate validation, activation, deactivation, and session effects.
  Encryption renewal and credential replacement have different consequences.
- [Gateway snapshots](../../../../crates/wyrd/wyrd-server/src/components/gateway/snapshots.rs)
  already own notification ordering, reload generations, and fail-closed
  listener loss. Extend this owner instead of implementing another cache bus.
- The facade, shared encrypted representation, action schemas/errors, affected
  initial schema edits, and authority updates must be specified before code.
  Wyrd API-key/token issuance remains outside this stored-secret change unless
  the maintainer explicitly expands scope.

## Evidence and remaining decisions

OWASP treats secret creation, rotation, revocation, and expiry as distinct
lifecycle responsibilities; it does not make third-party credential revocation
a consequence of encryption-key rotation.
[OWASP secrets management](https://cheatsheetseries.owasp.org/cheatsheets/Secrets_Management_Cheat_Sheet.html)

This supplement is source inspection and proposed behavior, not runtime proof.
Cross-replica invalidation, admitted-call lifetimes, auth candidate/session rules,
and bounded maintenance/CAS/reporting already exist. Reuse them. Implement the
shared encryption facade, backend adapters, common envelope, and requested
resource actions without introducing a new cache, completion, or job framework.
The spec must name the new public/persisted contracts, not demand decisions on
existing behavior merely because the encryption backend changes.
