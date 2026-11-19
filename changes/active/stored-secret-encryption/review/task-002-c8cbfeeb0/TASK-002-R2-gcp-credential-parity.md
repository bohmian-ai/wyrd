---
id: TASK-002-R2
kind: remediation
status: ready
spec: SPEC-stored-secret-encryption
spec_revision: 1
parent_task: TASK-002
depends_on: [TASK-002-R1]
requirements: [REQ-005, REQ-012]
remediates: []
---

# Use existing Google credentials for storage and secret encryption

## Outcome and authorization

An administrator already supplying Google service-account credentials to Wyrd
storage can select Cloud KMS with the same credential inputs. They add the KMS
key reference and grant that identity KMS permissions; they do not have to change
how they deliver credentials to Wyrd.

The maintainer explicitly requested this addition to the next remediation round
in the current conversation. This is a **user-requested scope addition**, not a
retroactive independently discovered finding or a change to the immutable
TASK-002 review verdict. Preserve FIND-TASK-002-1 through -4 and their history.
R1 is already being implemented; this follow-on task does not change its scope
or claim its findings are closed. Other validated findings from the next review
may be incorporated by that review without overwriting this request.

References: [approved spec revision 1](../../spec.md) REQ-005 existing supported
credential sources and REQ-012 consistent setup;
[original task](../../tasks/TASK-002-standardize-secret-owners.md);
[R1](TASK-002-R1-secret-owner-binding-and-runtime-safety.md).
Source observation at HEAD `d725e6e11c3e3c7d07f5a6f807f6a5e3336ae051` is navigation,
not a new immutable candidate review; concurrent R1 work is preserved.

## Diagnosed gap and smallest correction

[Independent source check](gcp-credential-parity-addendum.md) confirms the paths
and existing precedence below.

Storage `crates/wyrd/wyrd-storage/src/factory/mod.rs::service_account_json`
recognizes `GOOGLE_ACCOUNT_JSON_BASE64` and
`GOOGLE_APPLICATION_CREDENTIALS_JSON`. Its `cloud.rs::CloudStore::gcs` passes
that JSON to the provider builder. KMS `crates/wyrd/wyrd-kms/src/gcp.rs::GcpKms::new`
uses the provider's environment builder without Wyrd's additional resolution.
A deployment using either storage input alone can therefore access GCS but fail
to authenticate Cloud KMS, or select a different ambient identity.

Reuse the existing credential resolution through a single appropriately placed
owner consumed by both construction paths. Preserve current storage precedence
and behavior: base64 input is considered first, then raw JSON when the base64
variable is absent; when no explicit resolved JSON exists, retain the existing
provider credential chain. Preserve existing malformed-input behavior rather
than adding a new credential validation policy in this task.

Keep credentials redacted and provider refresh behavior intact. Do not make KMS
depend on the storage crate and its SQL/data-plane dependencies to reuse a small
resolver. Placement and helper signatures are implementation decisions. No new
credential variable, authentication flow, provider SDK/dependency, schema change,
per-request credential parsing, or hot-path provider call is authorized.
AWS, Vault, file and memory behavior remain outside this correction.

## Ordered acceptance scenarios

1. **Existing inline credentials reach KMS.** For each existing variable in
   isolation, supply a valid service-account fixture without ambient credentials
   hiding the selection. Construct through the normal storage and KMS credential
   paths and establish that both select the same supplied identity. Use the
   existing production facade and controlled token/KMS fixtures to prove KMS can
   acquire a credential and seal/open; injecting a ready-made credential provider
   cannot prove environment loading. Capture the expected missing-input RED
   before the production change, then GREEN. This needs no live account.
2. **Precedence and existing fallback remain intact.** With both inline inputs,
   preserve storage's current base64-first selection; with neither, retain the
   existing credential-file/default chain. Pin current malformed-input behavior
   at the shared resolver so extraction does not silently change it. Keep tests
   readable, credential-free and focused; exercise both callers rather than
   testing two copied parsers.
3. **Setup guidance matches behavior.** Update the stored-secret encryption
   reference to list both existing inputs and explain that the same identity may
   serve GCS and KMS but needs separate key permissions. Retain credential-file
   setup and accepted live-cloud CI limits. Update owned runner descriptions if
   they otherwise imply only file credentials work. Regenerate documentation
   projections through their owner when required.

## Verification and handoff

Use existing wyrd-kms and cloud-enabled wyrd-storage unit/integration owners,
local token/provider fixtures, and mise runners. Record actual exact selectors
and the narrow required feature set after tests are written. Do not invent
names or require live-cloud credentials for ordinary verification. Existing
`test:kms:gcp:cloud` remains credentialed CI proof; do not broaden its permissions
or add cloud provisioning work.

Run focused affected tests, format/lints and check:deps; docs:check and generated
documentation checks apply to changed reference content. No benchmark, migration,
full journey aggregate, or repair of historical TDD order is required. Record
RED/GREEN, preserved credential precedence and verification limits. Return evidence
to the caller; do not automatically launch implementation or review.
