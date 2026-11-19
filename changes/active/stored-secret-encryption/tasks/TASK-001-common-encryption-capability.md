---
id: TASK-001
kind: implementation
status: proposed
spec: SPEC-stored-secret-encryption
spec_revision: 1
requirements: [REQ-001, REQ-002, REQ-003, REQ-004, REQ-005]
depends_on: []
---

# Build the common encryption capability

## Outcome and flow

Provide the shared capability that every secret owner will consume: verified
ownership context and secret bytes → `WyrdKms` → common envelope; expected
context and envelope → the same facade → protected plaintext for intended use.
File, explicit memory, GCP KMS, AWS KMS, and Vault Transit differ only in wrapping
and authentication. This task supplies the capability and common configuration;
Task 2 connects production resource owners and performs the runtime cutover.

Use the [approved spec](../spec.md) and finalized design sections
[One Wyrd KMS facade](../design/notes.md#one-wyrd-kms-facade) and
[What rotation versions mean and what Wyrd loads](../design/notes.md#what-rotation-versions-mean-and-what-wyrd-loads).
Do not create another secret registry or a public encryption/decryption API.

## Existing owner and scope

`crates/shared/wyrd-crypt/src/lib.rs` already provides AES-GCM/AAD, fresh-DEK
envelope helpers, canonical length-prefixed context, and zeroizing key types.
Its envelope helpers currently require a raw local KEK and cannot call remote
wrapping providers. Extend/reuse those primitives, with one server-tier facade
for wrapping clients and envelope lifecycle. Auth and Gateway must consume it
without depending on their consuming `wyrd-server`; cloud dependencies do not
belong in pure crypto, `wyrd-spec`, or SDKs. Private crate placement and provider
client choice remain implementation-owned under those boundaries.

The facade owns seal/open, fresh-DEK renewal, and routine rewrap. Backends supply
authenticated wrap/unwrap, configured-key resolution, and reusable authenticated
clients. Define one typed configuration and canonical projections, including
backend selection, active/retained logical references, provider authentication,
validation, and redaction. Task 2 replaces old server configuration with it.
Do not give tenant writers key-administration authority or generate fallback keys.

Before replacement behavior is implemented, make the spec-authorized authority
updates in `architecture/wyrd-security-posture.md` (stored credentials and tenant
isolation), `architecture/wyrd-design.md` (Operator connections), and
`architecture/operations/{deployment-and-release,reliability-and-recovery}.md`
(configuration and key retention/restore). Preserve unrelated storage/erasure
rules. These updates describe the approved contract; later tasks implement it.

## Ordered acceptance scenarios and evidence

Add actual assertion locations, focused commands, RED/GREEN results, and limits
to these entries during implementation. No new test names are prescribed.

1. **Seal and open through the facade** — REQ-001–002, REQ-004.
   Given two writes of the same bytes and trusted owner context, invoke the
   common file/memory-backed facade. Both open correctly, have independent DEKs
   and fresh payload nonces, and use the common envelope. Unit tests in the
   shared capability/crypto owner exercise its actual API; a caller-local cipher
   cannot satisfy them. RED is the missing facade/common-envelope behavior.
2. **Reject substituted ownership and ciphertext** — REQ-003–004.
   Starting from a valid envelope, change expected tenant/platform scope,
   purpose, owner, applicable provider/client authority or secret version; also
   tamper with payload/wrapped DEK, use a disallowed key reference, unknown
   format, or invalid unwrapped DEK length. Opening refuses without trying an
   unrelated source. Unit and adapter-boundary tests observe refusal and absence
   of unintended backend requests. RED is missing shared validation/binding.
3. **Use the same envelope with each remote backend** — REQ-001, REQ-004–005.
   Given configured provider identity and allowed active/retained keys, wrap and
   unwrap facade-generated DEKs through GCP, AWS, and Transit adapters and open
   the payload through the facade. Integration tests exercise actual adapter
   HTTP/auth boundaries with local protocol fixtures and real dev Transit where
   applicable. Assert configured resource selection, exact context, opaque
   ciphertext round-trip, access-credential refresh, and redacted failures on
   timeout/denial. Native version changes remain provider-owned. RED is absent
   adapters/common provider behavior; local fixtures are not live-cloud proof.
4. **Prepare renewal and rewrap for consumers** — REQ-002, REQ-004.
   Given an old readable envelope and permitted replacement wrapping key, renew
   encryption and separately rewrap through the facade. Renewal preserves value
   with a fresh DEK/payload; rewrap preserves DEK/payload and opens under the new
   reference. Provider failure returns no successful replacement envelope.
   Facade unit/integration assertions drive the primitives consumed by Tasks 3–4.
   RED is missing shared renewal/rewrap, not a manufactured resource operation.
5. **Validate backend setup without fallback** — REQ-005.
   Exercise typed configuration with valid selections and malformed/missing or
   incoherent settings. Assert redacted diagnostics and refusal rather than
   substitute file/memory keys. Explicit memory mode loses old readability when
   its key is lost; file access retains only configured active/retained material.
   Configuration/file integration tests cover the actual facade constructor.
   RED is missing common configuration validation; preserve existing safeguards.

## Verification and handoff

Use the owning library's unit runner: `mise exec -- cargo nextest run --locked
-p <actual-owning-crate> --lib`, recording each added test's exact focused
selector before RED. Existing primitive owner: `wyrd-crypt`. Any new server-tier
owner uses the same runner after its actual package is registered; do not invent
an existing lane. Reuse installed `wiremock`/HTTP infrastructure and the dev Vault
tool/lifecycle pattern in `mise.toml` rather than requiring production secrets.
Disclose which provider checks used local protocols versus a real service.

Run affected owner tests, `mise run check:deps`, `mise run fmt`, and
`mise run lints`; run `mise run codegen:check` if error/contract outputs change.
No runtime consumer or hot path is switched in this task. Explain the common
configuration and provider setup in its owning reference documentation; Task 2
connects the deployment guides to the actual server setup.

Output: one tested shared capability/configuration and updated authority for
Task 2, with no requirement for SQL, migrations, cloud key provisioning per
tenant, private SaaS code, or legacy readers.
