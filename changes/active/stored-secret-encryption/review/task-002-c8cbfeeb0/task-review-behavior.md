# TASK-002 primary behavior review

## Subject and conclusion

Base `451aa0c4d332af3bf8f334ea9eb1bc9d0679a34a` → candidate
`c8cbfeeb0d5cf3a4fa63bb386cbf460f009ead24`, approved
SPEC-stored-secret-encryption revision 1, original
`tasks/TASK-002-standardize-secret-owners.md`.

No blocking finding is proposed in this reviewer's assigned boundary. Initialization,
consuming examples, setup/configuration removal, provider test wiring and the assigned
cutover requirement mapping have credible source and recorded execution evidence.
Acceptance of security/concurrency and Gateway lifecycle remains subject to the selected
independent specialist reports and fresh validator; this is not the integrated verdict.

The diff was inspected directly. No `.codegraph/` exists. Source was not edited and
this reviewer ran no Cargo command. Review artifacts are the only writes.

## Scope and authority

Reviewed the approved spec independently of its scenario evidence, the task, root and
applicable docs/Python/SQL AGENTS, the task-review skill, spec-driven-development,
TESTING, maintainer style, Rust and error guides. Applicable architecture is the
stored-secret and tenant isolation security owner, deployment configuration authority,
backup/restore authority and Operator design. Initial SQL edits have the explicit
unshipped-schema authorization in the spec; no upgrade/backfill promise is inferred.

Primary coverage includes real init and its persisted key, server/harness composition,
three language examples across restart, common provider tests/CI, documentation and
removal of obsolete configuration/readers. The domain specialist owns detailed auth,
platform and Operator context, SQL CAS/authority and remote IO boundaries; the system
specialist owns Gateway snapshots and external waits. Shared owner simplicity and
repository boundaries were considered within the primary scope.

## Requirement/scenario coverage

This matrix records the original requirement/scenario references; it adds no obligations.
PASS here means the primary-covered part is supported, with assigned specialist details
explicitly pending reconciliation.

| Requirement / original scenario | Implementation and inspected assertions / execution evidence | Primary result |
| --- | --- | --- |
| REQ-001; scenarios 1–3, 6 | `boot/mod.rs::build_kms` creates one facade and composition supplies the same Arc to auth, Gateway and Operator owners. Auth `pg_resolvers`, Gateway `ManagedSecretBinding` and Operator `OperatorSecrets` route through it. CLI init starts the actual binary; server harness constructs the production facade rather than an independent deterministic sealing path. | PASS; detailed consumer security belongs to domain/system |
| REQ-002; scenarios 1, 3 | KMS seal/open remains the passed TASK-001 shared implementation; no new cipher owner appears. CLI assertion reads common-envelope bytes from Gateway and Operator rows, excludes both secrets, and checks rerun rewrites neither row. Root reports a current combined KMS/crypt unit rerun of 21/21. | PASS for shared implementation reuse and integration |
| REQ-003–004; scenarios 3–4, 7 | Each owner supplies facade context; live-facade test helper reopens a stored envelope with a separately constructed facade and refuses another owner. Auth, platform, Gateway and Operator consumer assertions are present at the real server/Postgres tier; substitution/concurrency details are assigned to specialists. | PASS for primary provider/projection coverage; specialist reconciliation required |
| REQ-005; scenarios 1–3, 7 | `init.rs::local_kms_key` persists a generated owner-only temporary file with `persist_noclobber`, reuses existing key, and child commands override KMS projections with one file backend. `init_keeps_one_stored_secret_key_for_every_owner` checks mode, byte reuse, actual Gateway calls, Operator create/list and secret-free output across rerun. Restart journey observes actual Operator delivery with the same stored token. | PASS; root reproduced exact init 1/1 |
| REQ-008; scenarios 3–4 | Owners preserve typed redacted views; actual init and restart journeys assert token absence from outputs, CLI test checks key/plaintext absence, platform test checks redacted config response and actual token request. Shared protected buffers are carried from accepted TASK-001. SQL/authority races and transaction scope belong to domain. | PASS for primary response/composition proof; domain reconciliation required |
| REQ-009; scenarios 2, 5 | Python runner starts real installed init, creates the connection once, declares the Service, records slow input, observes failed verdict plus delivered dispatch, restarts saved deployment and observes a second authenticated delivery. It runs the actual Python, Rust and TypeScript dispatch programs. Gateway lifecycle and wait assertions belong to system. | PASS for restart journey; system reconciliation required |
| REQ-012; scenarios 2, 6 | Old `SealingKeyring`, raw Operator key source/config and env projections are deleted; source sweep finds no remaining production caller or compatibility reader. Remaining historical Gateway nonce schema is dropped by its authorized format migration. Startup scripts, server composition, fixture builder and docs name canonical KMS settings. No new external package in Cargo.lock. | PASS |
| REQ-003–005; scenario 7 | One automatically discovered integration target collects real Vault and separately selected GCP/AWS modules. Common cloud helper loads production env configuration, constructs two real facades, seals/opens fixed value and refuses foreign owner. Ordinary lanes skip external checks; selected lanes explicitly include and select the provider. Vault retained assertions cover native rotation, context refusal and bad token. | PASS with accepted unavailable-account execution limit |

## Important proof paths

- `crates/wyrd/wyrd-cli/tests/pg_tests.rs:1866` runs real CLI initialization,
  creates Gateway and Operator credentials, verifies `kms/local` mode and saved key,
  drives the authenticated Gateway before/after re-init in another working directory,
  and verifies stored envelopes and credentials remain byte-identical.
- `sdks/wyrd-sdk-python/tests/integration/test_local_development.py:1381`
  uses real initialized deployment and an external receiver. It stores the connection
  once, fails a scheduled Verifier, requires the exact `failed`/`delivered` program
  output, checks one received bearer-authenticated failure body, and repeats after
  stopping/restarting init. Parametrization runs each of three checked-in programs;
  it does not seed another connection for the second phase.
- `identity_e2e.rs::auth_client_secrets_open_through_vault_transit_across_restart`
  drives confidential human sign-in and workload exchange through the actual server,
  observes encrypt/decrypt requests and repeats both uses after `restart_bound`.
- `platform_admin_e2e.rs::platform_sign_in_opens_its_client_secret_through_vault_transit_across_restart`
  configures platform OIDC through its authority surface, checks redaction, drives
  sign-in and proves decryption again after restart without resealing.
- `wyrd-kms/tests/integration/main.rs:28` uses `KmsConfig::apply_env` and
  `WyrdKms::new(..., true)`. It cannot pass missing or wrong selected backend by
  skipping. `mise.toml:251`–266 select one provider module including ignored external
  tests; `gate:rust:journeys` retains the real Vault lane. No additional Cargo target
  registration is needed: Cargo discovers `tests/integration/main.rs` natively.
- `kms-integration-cloud.yml` follows the existing storage workflow's main-only,
  environment, setup action and concurrency conventions. AWS uses the existing OIDC
  action; GCP JSON is written under umask 077 and passed by the file form the existing
  loader accepts. Neither job provisions or destroys a key.

## Documentation and scope

The published stored-secret reference covers common settings, trusted key references,
file/memory/cloud/Transit behavior, provider permissions and provisioning/authentication
steps. Team setup, add-people and troubleshooting project the same canonical names.
The failed-verification guide embeds the actual journey-run programs and YAML. Its
`draft: true` is deliberately retained for the maintainer's publication decision;
REQ-012 requires runnable proof, not a separate publication action. The reference is
published and present in generated LLM indexes. No new authentication mechanism,
SDK crypto, registry, table, compatibility shim, or independent cache is introduced.
Resource admin additions and consolidated wrapping maintenance remain TASK-003/004;
this review does not require them early. Existing maintenance safety is a domain
specialist boundary, not waived by that deferral.

## Execution and process limits

Original task records name focused runs, assertion results and broad checks tied to
this candidate's commits; source assertions above were independently inspected.
Root additionally reports KMS/crypt 21/21 (nextest
`f75f94ea-d423-409c-b62b-11a69629d56b`). The task's scenario-1 prose gives an
inaccurate target/selector (`--test pg_tests`); the runnable owning mise lane correctly
uses `--test cli` with `pg_tests::...`. Root reproduced that exact test under the
repository PG wrapper: 1/1, nextest `1258304f-d826-443f-ab03-6c02bcf42896`. This
spelling issue does not establish a production or assertion defect; current root
execution supersedes the shorthand. Root also reproduced real dev Vault 1/1 via
`mise run test:kms:vault`, nextest `7556e942-e391-4983-a890-6ca442ff90ca`; exact
commands and results are in this review directory's `verification.md`.

Live AWS/Cloud KMS and Linux-only production startup are unrun here, as explicitly
accepted by the user. Their workflow/source wiring was reviewed; successful live
provider or image execution is not claimed. Full journey aggregates, codegen/docs,
format and lints were not rerun by this reviewer; their recorded implementation
results remain reported evidence. No benchmark run is assigned by the task or current
root AGENTS; no benchmark result is claimed. Gateway wait inventory is assigned to
system review.

TDD compliance is **VIOLATED** for scenarios 2 and 3, as openly recorded. Later
passing journey assertions prove current behavior and cannot manufacture historical
RED. Scenario 1 records expected missing-key RED; scenario 5 records 502-versus-503
RED. Verification-only scenarios correctly record no artificial RED. The recorded
race-test draft initially expected 409 although existing reprepare preserved disable;
its assertion correction and required diagnostic history should remain visible in
final process notes, separately from the source-backed behavioral finding set.

## Findings

None proposed. No optional cleanup, new backend/authentication promise, historical
RED repair, benchmark obligation, or new production machinery is requested.
