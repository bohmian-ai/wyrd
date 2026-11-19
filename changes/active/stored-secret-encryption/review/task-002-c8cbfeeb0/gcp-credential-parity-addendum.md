# Next TASK-002 remediation: Google credential parity

Provenance: **USER-REQUESTED SCOPE ADDITION**, requested after the immutable
TASK-002 review. This is not a fifth independently discovered finding and does
not amend the verdict or source identities of the `c8cbfeeb0` review. Carry it
into the next remediation round; current TASK-002-R1 implementation is underway.
Source inspection for this addendum observed HEAD
`d725e6e11c3e3c7d07f5a6f807f6a5e3336ae051`; concurrent working changes were
left untouched. No acceptance verdict is issued on that mutable implementation.

## User outcome and observed gap

An administrator already running Wyrd storage with a Google service-account JSON
credential should be able to select Cloud KMS using the same configured credential
source and identity, with the separately required key permissions. They should
not need to duplicate the credential into a file merely because KMS was enabled.

`wyrd-storage/src/factory/mod.rs:83–99` resolves
`GOOGLE_ACCOUNT_JSON_BASE64` first, then `GOOGLE_APPLICATION_CREDENTIALS_JSON`.
`wyrd-storage/src/cloud.rs:90–97` applies that result to its existing
GoogleCloudStorageBuilder with `with_service_account_key`.
`wyrd-kms/src/gcp.rs:47–59` uses only GoogleCloudStorageBuilder::from_env and
therefore misses Wyrd's two explicit JSON credential inputs. A storage deployment
using only one of those inputs can consequently obtain its storage identity but
fail to obtain that identity for KMS, or resolve a different ambient identity.
This is credential-source integration, not a request for a new Google login flow.

## Bounded correction

Give storage and KMS one authoritative implementation of the existing JSON-input
resolution, reusing the existing service-account parser, credential refresh and
provider clients. Move/reuse the narrow existing behavior at a dependency boundary
both owners may consume; do not make KMS depend on storage or Skald and do not add
a vendor SDK, another credential framework or a new environment variable. Exact
helper/type placement remains an implementation decision under repository rules.

Preserve existing precedence and failure behavior rather than redesigning them:

- A present UTF-8 base64 input takes precedence over raw JSON. Successful decode
  supplies that JSON to the existing builder and overrides its ambient credentials.
- If base64 decoding or UTF-8 conversion fails, the existing helper returns None;
  it does not continue to the raw-JSON input. The builder's existing credential
  chain remains responsible. Do not silently change this behavior as part of parity.
- Without that base64 input, raw JSON is passed through unchanged to the existing
  builder. Its malformed-service-account errors remain the owner's normal redacted
  construction errors.
- With neither explicit input, preserve supported file/ADC/metadata behavior.

Affected reach: existing storage credential resolver/caller and tests; KMS GCP
production constructor and credential-selection tests; narrow shared-owner exports
if relocation requires them; stored-secret provider setup documentation so existing
storage users know these inputs are accepted. Encryption envelopes, tenant binding,
provider IAM, AWS/Vault behavior and SQL/schema remain unchanged.

## Closure proof for the next remediation task

1. Under each existing JSON input separately, construct storage and KMS through
   their production credential-selection paths and demonstrate selection of the
   same test service-account identity. In KMS, exercise a credential-authenticated
   operation against the existing local token/provider fixture where appropriate;
   an injected GcpCredentialProvider alone does not test constructor selection.
   Keep these checks credential-free and network-local.
2. With both inputs set to different valid identities, prove base64 wins in both
   consumers. With the base64 input malformed and raw JSON otherwise valid, pin
   the existing fallback behavior in the shared resolver instead of introducing
   new validation semantics. Preserve normal error redaction for invalid JSON.
3. With neither input configured, retain existing supported file/ADC/metadata
   selection and credential-refresh behavior. Existing provider protocol and
   common-facade tests remain applicable; no new authentication mode is authorized.

Use focused tests in the existing shared resolver, storage cloud-feature test
owner and KMS constructor/protocol owner. Record actual module-qualified selectors
and narrow mise setup after tests exist; do not require live cloud keys to prove
selection. Run relevant format/lints, dependency checks and documentation checks
if setup docs change. Existing live GCP CI execution remains its accepted separate
provider boundary; no new cloud lane, benchmark or full journey aggregate is needed.

This check inspected source only. No production code was changed and no build,
test or live-provider execution was performed.
