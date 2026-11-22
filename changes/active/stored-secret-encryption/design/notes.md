# One mechanism for stored secrets

Status: finalized

Finalized at the maintainer's explicit request. This locks the design direction;
it does not approve a specification, implementation tasks, or production changes.

## Problem

Wyrd has separate auth/Gateway sealing and Operator envelope-encryption systems.
`wyrd init` prepares only the former, so local Operator connection creation fails
with `WYRD_OPERATOR_503_KEY_UNAVAILABLE`. Tenant and owner binding also differ.

The maintainer operates a private multitenant SaaS and distributes the same
open-source software for self-hosting. They require an understandable proposal
based on established SaaS security practice, tenant-specific data keys, and one
mechanism. Provider selection and cost must not substitute for that explanation.
The maintainer has aligned with the shared-wrapping-key envelope approach and
the requirements below. This is design agreement, not spec approval.

## Desired outcome

All stored recoverable credentials use one tenant-bound envelope-encryption
workflow. Local Operator creation works after init without additional setup.
The same workflow works with protected files, development memory, GCP KMS,
AWS KMS, or Vault Transit. No SaaS-only code path or mandatory Vault deployment
is introduced. Administrators operate stable resource identities through simple
Rotate and Revoke actions; clients do not need to learn new credential names.

## Agreed requirements

- One deployment wrapping key protects tenant-specific data keys.
- Every newly stored secret value gets a fresh random data key.
- One encryption/decryption path covers every current and future stored-secret
  owner: Gateway, auth providers, workload issuers, platform OIDC, Operators,
  and subsequent recoverable secret types.
- One server-owned Wyrd KMS facade hides all wrapping backends. Reading and
  provider-specific operations are internal details.
- No shipped-data migration, compatibility reader, or legacy format.
- Simple server-side Rotate and Revoke API actions, suitable for eventual UI
  buttons. Operators should not handle DEKs, envelopes, or database updates.
- The operational scenarios are a leaked DEK, leaked credential, leaked shared
  wrapping key, and an expired token/key. Preserve existing owner response,
  propagation, and upstream-action semantics; do not add a new completion protocol.

The [administrator journey](admin-journey.md) explains Rotate/Revoke behavior,
including the limits of uninterrupted rotation and third-party revocation.
[Four recommended journeys](journey-scenarios.md) express the user goals as
Given/When/Then recommendations. Backend/owner coverage, failures, and security
invariants are supporting checks rather than separate journeys.

## Constraints and non-goals

- Nothing shipped: replace initial formats and schemas directly after spec
  approval. No migrations for existing data, compatibility readers, or aliases.
- No production, authority, branch, staging, or commit changes during design.
- Signing, password hashing, token digests, external Gateway references, and
  object-store encryption retain their separate responsibilities.
- This is a proposed security design, not a claim of implementation readiness.

## Proposed implementation

Finalized direction: conventional envelope encryption with one versioned deployment
key-encryption key (KEK), outside Postgres, protecting independent, tenant-bound
data keys. Each new secret value gets a fresh random 256-bit data key (DEK).
AES-256-GCM encrypts the secret under the DEK; the shared KEK wraps that DEK.
Store the encrypted secret and wrapped DEK together in Postgres. Independent
tenant-level KMS revocation is not promised by the agreed shared wrapping key.
Tenant isolation is authenticated ownership plus independent per-secret DEKs.

```text
Deployment KEK, outside Postgres
    wraps fresh, tenant-bound DEKs
        each DEK encrypts one secret value

Tenant A and B never share a DEK; they share the deployment KEK.
```

This deliberately does not introduce a deployment-root/tenant-branch/record-key
hierarchy. A shared-root hierarchy is a legitimate alternative, but is not
needed to explain or deliver this baseline. It also does not reuse one tenant
DEK indefinitely to encrypt every credential. Those are different designs with
additional usage-limit, caching, and isolation trade-offs.

### Self-hosted single tenant

Init generates and atomically persists the deployment KEK in protected,
persistent owner-only key storage. Restart reuses it. One shared owner supplies
it to auth, Gateway, and Operators. Tenant and platform credentials have separate
DEKs and distinct authenticated scope/purpose binding. Local wrapping/unwrapping
uses existing cryptographic primitives. No external key service is required.

The operator must preserve and separately back up that key storage; database
backups alone cannot recover credentials. For multiple replicas, all replicas
must receive the same active and retained KEKs. A missing key is a recovery
failure, never permission to generate a replacement for existing ciphertext.

### Multitenant SaaS

The deployment operator provisions one symmetric KEK in the selected key
manager. Trusted deployment configuration identifies the exact allowed key
resource; tenant input and ciphertext cannot redirect the backend to arbitrary
resources. The service uses workload identity, or Vault authentication, to wrap
and unwrap tenant-bound DEKs. KMS/Transit retains the KEK; Wyrd receives
plaintext DEKs when needed to decrypt credentials.

The backend changes these operations, not the credential format or ownership:

| Backend | Wrap / unwrap DEK | KEK location |
|---|---|---|
| File | Local authenticated encryption / decryption | Protected persistent file and process memory |
| GCP KMS | Encrypt / Decrypt | Deployment CryptoKey |
| AWS KMS | Encrypt / Decrypt | Deployment customer-managed KMS key |
| Vault Transit | Encrypt / Decrypt | Deployment Transit key |
| Development memory | Same local authenticated wrapping operations as file mode | Ephemeral process memory |

Memory mode is explicit and disposable. Restarting without its key cannot open
previously persisted envelopes. It is for isolated tests and throwaway development
stores; durable local init uses file mode. No failed backend silently falls back
to a new memory or file key.

Vault KV is a raw-secret store and is not equivalent to Transit: the finalized
remote wrapping backend is Transit. The superseded KV-only setup proposal has
been removed. Exact provider configuration/provisioning contracts still need to
be fixed before specification approval.

### One credential write and read

1. Existing authorization establishes the tenant and owner; ciphertext cannot
   select a different tenant, provider endpoint, or arbitrary key resource.
2. Generate a fresh DEK and nonce, encrypt the secret, and wrap the DEK under
   the configured KEK. Persist only after both operations succeed.
3. Authenticate canonical tenant, purpose, owner identity, provider/credential
   authority, and secret version where the domain already has one. Apply
   binding to both payload encryption and key wrapping. Expected context comes
   from trusted row metadata, not solely from the encrypted value.
4. Read using the explicitly selected deployment KEK, unwrap the DEK, authenticate
   and decrypt the payload, and use the plaintext only for its intended action.
   Never try other tenants or alternate sources after authentication failure.
5. Zeroize transient key/plaintext buffers and keep all responses, logs, errors,
   and diagnostics redacted.

Auth, Gateway, and Operators retain their domain authorization and storage
owners. There is one shared encryption/lifecycle owner, not one per feature.

### One Wyrd KMS facade

Proposed owner name: `WyrdKms`, instantiated once by server composition. It owns
fresh DEK generation, canonical AAD, envelope format/validation, secret
seal/open, fresh-DEK rotation, routine DEK rewrap, redacted errors, and bounded
runtime preparation/caching. Consumers never call provider APIs, generate their
own DEKs, or implement another decryption path.

The internal backends supply authenticated wrap/unwrap and provider setup.
GCP/AWS/Vault do not get their own secret payload format. File and memory modes
implement those same operations with local key material. A backend-specific
opaque wrapped-DEK blob is contained within one common envelope.

Reuse `wyrd-crypt` for primitives and its existing envelope/context/zeroization
mechanics. A server-tier `wyrd-kms` crate is a proposed placement for the shared
owner: auth and Gateway cannot depend on their consuming application server,
and pure cryptographic helpers should not acquire cloud-client dependencies.
The facade does not need to own SQL. Existing resource owners retain storage,
authorization, and domain lifecycle behavior while using the facade.

Suggested facade operations are seal, open, rotate-encryption (open then the
same seal path with a fresh DEK), and rewrap (same DEK, new wrapping). These are
private API sketches, not approved public Rust signatures. Prepared/cached reads
still pass through the shared implementation; preparation does not authorize a
second owner-specific cipher path. Cloud clients, pools, and runtimes are not
constructed per request.

Future secret owners add a typed identity/purpose and call this facade. They do
not introduce a new key source or separate crypto helper. This is a reusable
capability, not automatic discovery of future database columns or a new generic
secret registry. SDKs project resource operations; they do not receive server
DEKs, KEKs, or plaintext through a decryption endpoint.

### What rotation versions mean and what Wyrd loads

Routine GCP rotation keeps one CryptoKey resource such as
`projects/P/locations/L/keyRings/R/cryptoKeys/wyrd-secrets` and adds
`cryptoKeyVersions/2`, making it primary. Version 1 remains available for
decryption until explicitly disabled or destroyed. Wyrd configures the stable
CryptoKey name; Encrypt uses its primary, and Decrypt selects the version
needed by the ciphertext. Wyrd does not download either wrapping-key version.
See the [Encrypt API](https://docs.cloud.google.com/kms/docs/reference/rest/v1/projects.locations.keyRings.cryptoKeys/encrypt)
and [Decrypt API](https://docs.cloud.google.com/kms/docs/reference/rest/v1/projects.locations.keyRings.cryptoKeys/decrypt).

AWS native rotation likewise retains one logical key ARN and historical key
material. AWS selects that material internally; Wyrd cannot model it as
individually addressable version numbers. Vault Transit retains one named key
and versions its encrypted output (`vault:v1:...`, `vault:v2:...`).
Provider-wrapped ciphertext remains opaque to Wyrd: never parse its private
format or assume all backends expose the same version identifier.

Proposed startup composition:

1. Read the selected backend configuration, active logical key reference, and
   explicitly retained references permitted for old values.
2. Build one reusable authenticated provider client with bounds and normal
   credential refresh, or load the active/retained file keyring into redacted
   process memory for local mode.
3. Optionally prove current wrap/unwrap access with a transient random probe;
   this proves current-key access only, not decryptability of every old row.
4. Inject the same owner into credential writers and decryption preparation.

The stored envelope needs its format/algorithm identifiers, logical wrapping
key reference, opaque wrapped DEK, payload nonce, and authenticated ciphertext.
File mode's wrapped-DEK blob includes its local key ID and wrapping nonce;
cloud/Vault mode stores the provider response blob. A provider-returned version
identifier can aid inventory where available, but is not universally required
for selecting decryption material. Never trust a stored reference to choose an
arbitrary endpoint: resolve it only through the configured allowed key set.

For a remote-backed write, the shared owner generates the DEK, encrypts the
secret locally, calls provider Encrypt on the DEK with canonical owner binding,
and stores the returned wrapped blob. For loading an uncached secret, the
shared owner reads the envelope, validates its configured key reference, calls
provider Decrypt with the opaque blob and the same expected binding, validates
the returned DEK length, and authenticates/decrypts the payload locally. That
Decrypt request loads a plaintext DEK, not a KEK. Provider refusal, timeout,
unknown reference, or authentication failure refuses use; no other key/source
is guessed.

Warm Gateway preparation belongs in the inspected background snapshot loader;
it is not a remote call in `CredentialResolver::resolve` on every attempt.
Cache identity must include tenant, owner/secret identity, and the exact secret
revision or envelope digest. A replacement must not accidentally reuse an old
DEK. Prepared decryption material follows the existing owner snapshot/cache
lifetime and invalidation behavior. This does not introduce a separate global
DEK cache, new TTL policy, or new propagation/completion protocol.

Changing to a different logical key is separate from native rotation. Example:
`wyrd-secrets` to `wyrd-secrets-replacement`. Publish the replacement as readable,
then activate it for writes across replicas while retaining the original
reference for reads. A background pass changes the stored wrapped DEKs and
logical references through CAS. Drop the old reference and retire its key only
after live/cached/backup dependencies are resolved. Unlike native rotation,
this requires a configuration rollout and persisted-reference changes.

### Rotation, failure, and performance

New writes use the active deployment KEK version. Background maintenance rewraps
DEKs under the new version, preserving payload ciphertext and racing safely
with credential replacement through existing version/CAS fences. The portable
operation can unwrap then wrap; native provider rewrap is an optimization,
not a different lifecycle. Routine rotation is distinct from replacing a
provider credential after exposure.

Retain old wrapping keys until live references, caches, and backup recovery
requirements allow retirement. Restore keys before serving restored ciphertext.
A compromised shared server can use every key its identity permits; encryption
does not eliminate server authorization or RLS requirements.

Remote unwrapping adds a real network dependency. Warm Gateway attempts must
remain local: prepare required decryption material during the existing
background snapshot load and preserve the existing retryable unavailable
behavior while a snapshot cannot be loaded. Reuse existing bounded caches,
invalidation, and admitted-request snapshot lifetimes. KMS provider calls remain
real IO in preparation/control paths, not warm execution.

### Compromise and revocation boundary

A leaked plaintext DEK exposes its one secret value, including any copied old
ciphertext under that DEK; it does not derive other data keys. A shared KEK
compromise plus database ciphertext, or broad authorized KMS use, can expose
all tenants. Tenant/owner AAD prevents substitution, not use by a fully
compromised server that can supply the correct context.

Disabling a tenant or connection is application revocation, governed by the
existing authority checks, invalidation, and in-flight-operation semantics.
It is not independent cryptographic revocation. Disabling the shared KEK
blocks future unwraps for every tenant and does not erase already cached DEKs.
A tenant-specific KEK would permit independently denying future unwraps for
that tenant, still subject to caches and any already stolen plaintext.

Deleting live encrypted DEKs does not erase copies in retained backups. With a
shared KEK those backup copies remain decryptable. Separate tenant KEKs can
support tenant-specific crypto-erasure only when all versions/copies of that
KEK, and cached plaintext, are also accounted for.

### Three different rotations

- Provider credential replacement: replace the actual Slack/provider token,
  encrypt the replacement under a fresh DEK, commit it atomically, and invalidate
  old runtime material. If exposure is suspected, revoke the old token upstream.
- Routine KEK rotation: new wraps use new key material; old wrapped DEKs remain
  readable while old material is retained. Native rotation alone does not
  rewrite stored ciphertext or repair an exposed DEK.
- Retirement of an old KEK: background unwrap/rewrap every affected DEK under
  the replacement KEK, preserve payload ciphertext, use CAS/version fencing,
  and prove live/cached/backup references are safe before disabling/destroying
  the old key. AWS rotation retains historical material inside one logical KMS
  key, so replacing the logical key is the portable retirement workflow.

If a DEK itself is exposed, changing its wrapping is insufficient: generate a
new DEK and re-encrypt the payload. None of these actions withdraw plaintext
or ciphertext an attacker has already copied.

Sources: [GCP key rotation](https://docs.cloud.google.com/kms/docs/key-rotation),
[AWS rotation](https://docs.aws.amazon.com/kms/latest/developerguide/rotate-keys.html),
and [AWS caching](https://docs.aws.amazon.com/encryption-sdk/latest/developer-guide/data-key-caching.html).

## Evidence and rationale

Primary security guidance supports the recommendation:

- [OWASP cryptographic storage](https://cheatsheetseries.owasp.org/cheatsheets/Cryptographic_Storage_Cheat_Sheet.html#encrypting-stored-keys)
  distinguishes DEKs from KEKs and recommends protecting stored keys separately.
- [Google envelope encryption](https://docs.cloud.google.com/kms/docs/envelope-encryption)
  recommends fresh data keys for writes, protected by centrally managed KEKs.
- [AWS data-key caching](https://docs.aws.amazon.com/encryption-sdk/latest/developer-guide/data-key-caching.html)
  says fresh data keys are the default and caching/reuse requires explicit
  security thresholds. It does not justify indefinite reuse of one tenant DEK.
- [AWS Encrypt](https://docs.aws.amazon.com/kms/latest/APIReference/API_Encrypt.html),
  [GCP AAD](https://docs.cloud.google.com/kms/docs/additional-authenticated-data),
  and [Vault Transit](https://developer.hashicorp.com/vault/api-docs/secret/transit)
  document the relevant authenticated wrapping operations.

These sources establish patterns, not a statistical claim about all SaaS.
A shared KEK can protect many independent DEKs. Separate managed tenant KEKs
provide a stronger independent-revocation boundary and are not being silently
treated as equivalent to shared wrapping. The maintainer has now aligned with
shared wrapping and tenant-bound per-secret DEKs. That agreement does not promise
independent tenant-level KMS revocation or crypto-erasure across backups.

Verified repository reuse and gaps:

- [wyrd-crypt](../../../../crates/shared/wyrd-crypt/src/lib.rs): existing
  `Envelope`, `seal`, `open`, `rewrap`, direct AAD operations, zeroizing key type,
  and length-prefixed context. Reuse these instead of inventing cryptography.
- [OperatorKeys](../../../../crates/wyrd/wyrd-server/src/components/operators/keys.rs):
  tenant/owner/version binding, bounded rotation, and fenced updates exist;
  raw KEK retrieval and Operator-only ownership must be replaced.
- [Auth resolvers](../../../../crates/wyrd/wyrd-auth/src/pg_resolvers.rs) and
  [Gateway managed secrets](../../../../crates/wyrd/wyrd-gateway/src/managed.rs)
  currently use direct deployment sealing. They must adopt the common envelope.
- [Auth rotation](../../../../crates/wyrd/wyrd-auth/src/sealing.rs) already owns
  cross-product CAS resealing; consolidate its lifecycle with Operator rewrap.
- [GatewaySnapshots](../../../../crates/wyrd/wyrd-server/src/components/gateway/snapshots.rs)
  already performs background reloads and refuses uncached calls with retryable
  503. [load_snapshot](../../../../crates/wyrd/wyrd-server/src/components/gateway/service.rs)
  is the existing preparation seam; do not add remote unwrap per warm attempt.
- [Init](../../../../crates/wyrd/wyrd-cli/src/init.rs) already persists protected
  keys atomically. Extend its complete lifecycle for the common key owner.
- [Tenant provisioning](../../../../crates/wyrd/wyrd-server/src/components/platform/provisioning.rs)
  assigns the UUID before activation. Shared-KEK envelope encryption requires
  no new cloud key resource when a tenant is created; new DEKs are local.

Delete deployment-only direct sealing and its legacy/unversioned readers,
Operator-specific key configuration and ownership, and duplicated rotation.
Keep one common configuration and the shared Vault reader for external Gateway
credentials. Update all encrypted-column writers/readers, boot injection,
fixtures, schemas, setup/deployment docs, and the parked Operator guide together.

## Decisions and alternatives

The maintainer has aligned with one deployment wrapping key, fresh tenant-bound
DEKs per secret, all backends behind one facade, all current/future secret owners,
no shipped-data migration, and simple Rotate/Revoke operations with stable client
references. This is design alignment; no approved specification exists.
Direct deployment sealing, mandatory Vault, permanent tenant-wide DEKs, and
extra key hierarchies are superseded alternatives. The maintainer conditionally
requested both encryption renewal and credential replacement to cover leaked
keys. The separate deployment-wide wrapping-key recovery boundary is explicit
in the administrator journey; it has not been silently equated with a
selected-secret rotation.

## Standards conflicts

[Security posture](../../../../architecture/wyrd-security-posture.md) limits
sealing to auth/Gateway and requires tenant isolation.
[Operator design](../../../../architecture/wyrd-design.md) prescribes its own
Vault KV/raw-KEK owner. Revise those authorities through an approved spec before
implementation. Keep [operations](../../../../architecture/operations/deployment-and-release.md)
configuration authority and recovery rules. The unshipped-schema exception was
explicitly supplied by the maintainer; the spec must name any initial schema
changes. No authority or production files have changed.

## Open questions

No new policy decision is required for cross-replica propagation, in-flight
operations, owner-cache validity, auth session effects, or maintenance progress.
Those behaviors already have owners and are preserved:

| Behavior | Existing owner and retained behavior |
|---|---|
| Replica/cache refresh | `GatewaySnapshots`: post-commit notification, background reload, generation fencing, bounded entries, fail-closed listener loss; the writing server warms its snapshot |
| In-flight Gateway calls | `AdmittedCall`: an immutable admitted snapshot lives for the call; rotation does not retroactively replace it |
| Auth value replacement | `HumanConnections::activate`: existing candidate/test/activate and session-revision rules remain; encryption-only resealing is not semantic connection replacement |
| Rotation safety and progress | `SealedSecretRewrap` plus Operator maintenance: CAS/version fences, bounded cancellable batches, retry/resumption, current/rewrapped/remaining counts and existing diagnostics |

This corrects earlier notes that turned these into artificial open design
questions. Do not add distributed completion acknowledgements, a new TTL system,
a separate recovery framework, or new job tables. The common facade/adapters,
common persisted envelope, and requested admin actions are new work; route them
through the existing lifecycle owners. The later spec records the concrete API,
configuration, and envelope contracts without reopening these established
semantics. The four journeys are acceptance recommendations, not extra policy.

## Handoff

Design is finalized. The agreed hierarchy, one facade, owner coverage, and
Rotate/Revoke administrator experience are locked as design input. Existing
owner semantics above carry into the later human-approved spec; finalization
does not grant implementation readiness. No spec or task packet
was created, and no authority or production code changed.

Use the [recommended behavior-driven scenarios](journey-scenarios.md) as the
acceptance discussion for the specification, with existing local-dev, Operator,
identity, and Gateway lanes as verification owners. Each new action needs proof
on every surface that ships it. Gateway changes require a full clean applicable
L1 benchmark and an enumeration of added/moved external waits. No runtime tests,
cloud experiments, benchmarks, or independent security audit were performed.
Local design-document links were checked.
