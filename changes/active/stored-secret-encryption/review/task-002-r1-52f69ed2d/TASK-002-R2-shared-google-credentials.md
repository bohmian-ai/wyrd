---
id: TASK-002-R2
kind: remediation
status: ready
spec: SPEC-stored-secret-encryption
spec_revision: 1
parent_task: TASK-002
requirements: [REQ-005, REQ-012]
depends_on: [TASK-002-R1]
remediates: []
---

# Share Google credential resolution between storage and KMS

## Outcome and provenance

An administrator using `GOOGLE_ACCOUNT_JSON_BASE64` or
`GOOGLE_APPLICATION_CREDENTIALS_JSON` for GCS can enable Cloud KMS using that
same configured identity, after granting it the necessary KMS permissions.
No credential-file conversion or second credential copy is required.

This task is created by the maintainer-invoked TASK-002-R1 `wyrd-task-review`.
The maintainer explicitly requested this addition in the current review. It is
**user-requested scope**, not a fifth independently discovered finding. The four
original findings retain their IDs and closure history; `remediates` is empty
because this task packages the explicit addition. The pending request is recorded
in [the prior review note](../task-002-c8cbfeeb0/gcp-credential-parity-addendum.md).

Contract: [approved spec revision 1](../../spec.md), REQ-005 existing supported
credential sources and REQ-012 consistent deployment setup, supplemented by the
current explicit maintainer instruction. Original task:
[TASK-002](../../tasks/TASK-002-standardize-secret-owners.md).
Prior candidate `c8cbfeeb0d5cf3a4fa63bb386cbf460f009ead24`; reviewed candidate
`52f69ed2da3846733be0d314c6627c95d308e8dd`; cumulative base
`451aa0c4d332af3bf8f334ea9eb1bc9d0679a34a`.
See this directory's findings-validation.md and verdict.md for review disposition.

## Gap and correction boundary

`wyrd-storage/src/factory/mod.rs::service_account_json` resolves Wyrd's two JSON
credential inputs. `cloud.rs::CloudStore::gcs` applies them to the provider builder.
`wyrd-kms/src/gcp.rs::GcpKms::new` uses the provider's environment builder directly
and omits that resolution. A storage deployment using only one of these inputs
can therefore authenticate storage while KMS fails or selects an ambient identity.

Give both callers one authoritative implementation of the existing resolution,
using the smallest appropriate existing shared owner. Do not copy the parser or
make KMS depend on storage and its SQL/data-plane dependencies. Private helper
placement/signatures are implementation-owned; ordinary workspace dependency
edges must preserve repository boundaries. No new provider SDK, credential
framework or crate just to share these few lines is required.

Preserve current input behavior: a readable base64 variable is considered first;
successful decoding supplies JSON to the existing service-account parser. A
malformed base64/UTF-8 payload currently yields no resolved JSON and does not try
the raw-JSON input. When that variable is absent/unreadable, raw JSON is considered.
With no resolved explicit JSON, retain the existing provider credential chain.
Do not add a new failure/precedence policy as part of parity.

Reuse existing provider parsing, refresh and redaction. Credential selection stays
at client construction. AWS, Vault, local backends, encryption envelopes, tenant
binding and R1 fixes remain intact. No migration, new auth mode, credential variable,
per-request client creation or new caller-waited provider operation is authorized.

## Ordered closure scenarios

1. **Both inputs work through both callers.** In turn, supply each existing variable
   alone with a valid test service-account identity and no ambient identity hiding
   selection. Use production storage/KMS credential-construction paths. Establish
   that both select the supplied identity; use the existing local token/provider
   fixtures and production facade to prove KMS credential acquisition and seal/open.
   Injecting an already-created credential provider alone cannot prove env loading.
   Capture expected missing-behavior RED before changing production, then GREEN.

   Evidence — KMS: `wyrd-kms/src/remote_tests.rs::gcp_kms_signs_with_either_inline_google_credential`
   builds `WyrdKms::new` (production facade, `GcpKms::new`) under `temp_env` with each
   variable alone, every other Google source and `HOME` cleared, seals and opens against
   the local `FakeKms(GCP)` fixture, and asserts every request's bearer JWT `iss` is the
   supplied identity (fresh RSA keys from `wyrd_dev_fixtures::google`). RED before
   production change (cmd K): `seals: Unavailable { backend: GcpKms, failure: Credential }`
   — KMS ignored the variable and fell through to the metadata server. GREEN after
   `GcpKms::new` applied `wyrd_utils::secret::google_service_account_json`.
   Storage: `wyrd-storage/src/factory/mod.rs::tests::gcs_signer_selects_the_configured_google_identity`
   runs production `build_signer` against a local GCS endpoint and asserts the boot-probe
   bearer `iss` for each variable alone (cmd S). Storage already resolved both inputs, so
   this pins existing behavior across the move; no RED applies. PASS.
2. **Existing selection remains stable.** With both variables carrying different
   valid identities, prove base64 wins for both callers. Pin existing malformed-input
   behavior at the shared resolver and preserve raw-JSON/provider error redaction.
   With neither inline input, retain the existing file/default credential path.
   Keep this proof focused; no new identity or credential-refresh mechanism.

   Evidence — both set, base64 wins: KMS `gcp_kms_keeps_google_credential_precedence`
   (RED in cmd K on the same `Credential` failure, then GREEN) and storage
   `gcs_signer_selects_the_configured_google_identity` (`both` case). Neither inline input:
   the KMS test points `GOOGLE_APPLICATION_CREDENTIALS` at a key file and asserts that
   identity (file chain retained). For storage this case is not observable through a local
   endpoint, because `CloudStore::gcs` deliberately leaves an endpoint without an inline
   key unsigned (`cloud.rs`, `with_skip_signature(key.is_none())`); the production
   no-endpoint path is unchanged in the diff (resolver `None` leaves `from_env` alone),
   and it is the same `object_store` builder the KMS file case proves. Malformed input at
   the shared resolver: `wyrd-utils/src/secret.rs::tests::google_service_account_json_does_not_fall_back_past_a_malformed_base64_variable`
   (non-base64 and non-UTF-8 → `None`, raw ignored), and
   `google_service_account_json_prefers_the_decoded_base64_variable` (each alone, both,
   neither); these replace and extend the removed storage-local resolver test (cmd U).
   Redaction: `gcp_kms_redacts_an_unusable_inline_google_credential` asserts the fixed
   `KmsError::Config { setting: "backend", reason: "could not load GCP workload credentials" }`
   with no echo of a canary key; storage
   `gcs_signer_redacts_an_unusable_inline_google_credential` asserts the same canary is
   absent from the `build_signer` error. PASS.
3. **Setup guidance names the supported inputs.** Update the stored-secret reference
   and any affected runner description to explain both inputs and reuse of the
   storage identity with separate KMS permission. Retain file-based setup and the
   accepted live-cloud CI limit. Update generated documentation through its owner.

   Evidence — content review: `docs/src/content/docs/reference/stored-secret-encryption.md`
   `gcp_kms` authentication row lists both inline variables then the ADC chain; the
   `gcp_kms` setup section keeps the service-account and key-file routes, adds both inline
   variables, states storage and KMS share the identity, that the KMS role must be granted
   separately, and the base64 precedence and malformed-value rule. `mise.toml`
   `test:kms:gcp:cloud` description names all accepted credential inputs; the
   `kms-integration-cloud` workflow keeps its file-based live proof unchanged.
   `docs/public/llms-full.txt` regenerated by `mise run docs:generate`; `mise run docs:check`
   passes. PASS.

## Verification and handoff

Use existing wyrd-kms and cloud-enabled wyrd-storage test owners, local token/KMS
fixtures and relevant shared-owner tests. Unit tests protect resolution/precedence;
production-constructor integration proof protects caller wiring. No new public API
or journey aggregate is required for this credential-construction correction.
Confirm actual test names and record exact nextest selectors after adding tests,
using the narrow required features and mise environment. Do not invent selectors
or require a live cloud account to prove input selection.

Run focused affected tests, format/lints and check:deps; docs:check and the owning
projection check apply when documentation changes. Keep `test:kms:gcp:cloud` as
existing live CI provider proof; add no cloud provisioning or authentication work.
No benchmark or schema change is assigned. Preserve original TASK-002 scenarios
2/3 TDD violations and R1's separate process history. Return evidence to the caller;
implementation does not automatically start another review, commit or push.

## Implementation evidence

Status `IMPLEMENTED`. Behavioral verification PASS. TDD PASS for scenario 1 and the
caller precedence case of scenario 2 (KMS RED observed before production change);
storage and resolver cases pin existing behavior. Original TASK-002 scenario 2/3 TDD
violations and R1 history are unchanged.

Owner and reuse: the existing resolver moved verbatim from
`wyrd-storage/src/factory/mod.rs::service_account_json` to
`wyrd-utils/src/secret.rs::google_service_account_json`, the shared reader for environment
secrets that `wyrd-kms` already depends on; `wyrd-storage` gains the `wyrd-utils` edge,
so KMS takes no storage, SQL or data-plane dependency. `GcpKms::new` applies it with
`with_service_account_key` exactly as `CloudStore::gcs` does; selection stays at client
construction, with no new caller-waited operation. Test fixture:
`wyrd-dev-fixtures/src/google.rs` generates a fresh in-process RSA service-account key
(no committed private key) via a new `aws-lc-rs` workspace dependency already in the
graph through `object_store`/rustls; `workspace-hack` regenerated by `cargo hakari generate`.

Commands:

- K: `mise exec -- cargo nextest run --locked -p wyrd-kms --lib -E 'test(/remote_tests::gcp_kms_/)'` — RED 0/2, GREEN 3/3.
- S: `mise exec -- cargo nextest run --locked -p wyrd-storage --features emulator --lib -E 'test(/factory::tests::gcs_signer_/)'` — 2/2.
- U: `mise exec -- cargo nextest run --locked -p wyrd-utils --lib -E 'test(/secret::tests::/)'` — 2/2.
- Owner suites: `-p wyrd-utils -p wyrd-kms -p wyrd-dev-fixtures --lib` 34/34; `-p wyrd-storage --features emulator --lib` 59/59.
- `mise run fmt`, `mise run lints`, default-feature `cargo clippy --tests -D warnings` on the four crates, `mise run check:deps`, `mise run check:workspace-hack`, `mise run docs:check`, `git diff --check`: pass.

Limits: live Cloud KMS selection stays proven by `test:kms:gcp:cloud` in CI (accepted
limit); storage's credentials-file case is proven by source inspection as above.

