# TASK-001 behavior discovery

Maintainer amendment, 2026-11-19: BEHAVIOR-1 and the resulting
FIND-TASK-001-3 are withdrawn. The benchmark requirement below records the
original review reasoning and creates no current work. See `verdict.md`.

Reviewed immutable base `a859dab1d20806e8ad29d9eb064585126419f5c2` to candidate
`584a5de8b97f2d5bd2d258edbdd7ef2f95d8aa82`, with
`git diff a859dab1d...584a5de8b` (26 files). The four authority edits in the
base commit were independently inspected with `git diff a859dab1d^ a859dab1d`;
they precede replacement implementation and explain the user's inclusive
14-commit, 30-file description. HEAD remained the candidate. Pre-existing edits
to TASK-002 and TASK-004 were excluded. No CodeGraph index exists.

Authority: approved SPEC-stored-secret-encryption revision 1; original
TASK-001-common-encryption-capability; root AGENTS.md; docs/AGENTS.md;
changes/spec-driven-development.md and implementation-execution.md;
agent_docs/{index,rust,errors,maintainer-style}.md; TESTING.md;
architecture/wyrd-security-posture.md Stored secrets/Tenant isolation;
architecture/wyrd-design.md Operator connections;
architecture/operations/deployment-and-release.md Configuration authority/
Stored-secret encryption; reliability-and-recovery.md Backup contract/Restore
ordering. Benchmark applicability additionally follows the current user
instruction, spec Acceptance observations, and benchmarks/{AGENTS,README}.md.

Scope is the shared capability, not Task 2's consuming-owner cutover, init,
SQL replacement, public Rotate/Revoke surfaces, or whole-store maintenance.
Security-specific credential acquisition, crypto, tenancy/context, transport,
and zeroization findings are independently assigned to domain-rev; that report
must be reconciled before accepting provider/security rows below.

## Navigation and traced consumers

- `wyrd-kms/src/lib.rs`: context and domain tags, owning facade, backend enum,
  seal/open, renewal/rewrap, trusted-key check. `envelope.rs` encodes one format.
- `config.rs`: typed backend/options, canonical env projections, validation,
  redacted diagnostics. `local.rs` reuses `wyrd_utils::secret::read_secret_file`
  and existing AES-GCM primitives; loads only configured keys.
- `aws.rs`, `gcp.rs`, `transit.rs`: reusable remote adapters;
  `remote.rs`: bounded response/client; `remote_tests.rs`: protocol assertions.
- `wyrd-crypt` adds OS-random generation and exposes its existing canonical AAD;
  `wyrd-vault` shares the existing screened transport between KV and Transit.
- There is no production `WyrdKms` consumer yet. Existing shared Vault consumers
  are Gateway's `VaultBackend` and server Operators' `OperatorKeys`.
- CI adds KMS/Vault selection and a real dev Vault lane; draft reference stays
  unpublished. No new SDK projection, schema, migration, or public wire error.

The new facade is justified because existing `wyrd_crypt::seal/open` requires a
raw KEK and cannot perform remote wrapping. Its local/backend enum structure
keeps orchestration in one owner; no duplicate durable registry or public
plaintext endpoint was introduced. Crate-local KmsError is consistent with
agent_docs/errors.md; public coded errors will be mapped at consuming owners.

## Acceptance matrix

These rows reuse TASK-001 scenarios rather than creating new requirements.
Execution evidence is in `verification.md`: this review reran all 25 library
tests, the 15 KMS tests individually, the new Vault unit test individually, real
Vault, and CI selection. Historical wider lanes are recorded in the task.

| Requirement / scenario | Implementation and assertions | Evidence / assessment |
| --- | --- | --- |
| REQ-001, REQ-002, REQ-004 / scenario 1 | `lib.rs:352-399`; `SecretKey::generate` uses OsRng; `encrypt_aad` uses fresh nonce; `lib.rs:550-607` opens both values and compares unwrapped 32-byte DEKs/nonces; second facade opens file envelope after restart | PASS for assigned facade slice. Actual server/SDK use belongs to Task 2. |
| REQ-003, REQ-004 / scenario 2 | `SecretContext::canonical` (`lib.rs:119-138`) length-prefixes tenant/platform, purpose, owner, authority count/parts and optional version; distinct payload/wrap domains; `unwrap_dek` rejects disallowed refs before provider use; `lib.rs:611-738` checks substitutions, payload/wrapped-DEK changes, format/algorithm/truncation, wrong DEK size; `remote_tests.rs:341-359` checks zero requests for disallowed reference | PASS on inspected structure and assertions; reconcile domain review for sensitive boundary. Task evidence overstates "every envelope region": tamper loop actually flips payload ciphertext and wrapped-DEK nonce; unknown-format tests separately flip magic/format/algorithm. This does not omit the assigned tamper observations. |
| REQ-001, REQ-004, REQ-005 / scenario 3 | GCP/AWS calls carry allowed key and canonical context; AWS signs with installed AwsAuthorizer; refreshed credential-provider access is asserted in `remote_tests.rs:222-338`; Transit token reread/context in `:416-508`; real Vault checks native rotation, old read, rewrap, wrong context/token; provider refusal/transport/HTTP timeout in `:341-413` | Protocol and real-Transit checks PASS. Overall provider authentication/lifecycle acceptance requires domain-rev's independent assessment; fake credentials do not by themselves prove real ambient chains or cloud scope. No live AWS/GCP required by this task. |
| REQ-002, REQ-004 / scenario 4 | `lib.rs:407-440`; `:821-904` compares old/new DEKs, nonce/ciphertext preservation on rewrap, retirement and owner mismatch; `remote_tests.rs:514-550` injects provider failure, checks errors/no replacement and recovery | PASS for assigned primitives; maintenance/owner persistence not shipped here. |
| REQ-005 / scenario 5 | `config.rs:124-168,221-331`; `local.rs:38-63`; config tests at `:451,484,639,665`; file refusal/restart/retained-key tests `lib.rs:587,676,743`; missing keys create no substitute; memory loss refuses prior envelope | PASS for implemented validation/no-fallback observations; ambient provider lifecycle reconciled with specialist. |
| Authority prerequisite / TASK-001 Existing owner and scope | Four architecture documents updated by base commit before implementation; stored-secret exception preserves unrelated storage isolation; shared KEK risk and restore retention explicit | PASS. |
| Scope, projections, setup docs | New crate remains server tier; shared crypto has no cloud dependency; providers stay behind facade; reference covers settings, auth/permissions, retained keys and is `draft: true`; no runtime cutover or SQL diff | PASS. |
| Mandatory performance proof / applicable invariant | Changed shared `VaultKv2` is called on Gateway attempt path; full clean gateway benchmark absent | FAIL — proposed BEHAVIOR-1 below. |

## Shared Vault hot path and wait inventory

This candidate touches a Gateway hot path despite the task's claim that no hot
path changed. The supported flow is `GatewayEngine::attempt`
(`crates/wyrd/wyrd-gateway/src/engine.rs:862-866`) →
`CredentialResolver::resolve` (`credential.rs:143-149`, ExternalSecret arm) →
`VaultBackend::fetch` (`vault.rs:70-75`) → changed
`VaultKv2::read_field` (`wyrd-vault/src/lib.rs:145-158`) → extracted
`Endpoint::send` (`:367-400`). The caller waits while the shared code executes.

| Dependency wait added or moved by this diff | Result |
| --- | --- |
| Vault HTTP request (`request.send().await`) and response chunks (`response.chunk().await`) | Existing one-request KV read moves from `VaultKv2::read_field` into `Endpoint::send` (`:380,391`). No extra call, provider, retry, refresh, or changed deadline. The unchanged resolver/DNS wait remains inside that same request. Existing ExternalSecret resolution is expressly excluded from the encryption replacement by spec Persistent and authority changes; this refactor preserves the supported pre-existing wait and does not introduce another external dependency. |
| PostgreSQL | None added or moved on a production hot path. |
| Scribe | None added or moved on a production hot path; `EncodingHold` changes are cfg(test). |
| Object storage | None added or moved on a production hot path; four server edits are test support/assertions. |
| New remote wrapping, credential acquisition, Vault token-file blocking read | New facade operations only; there is no production caller in this candidate. Not a new hot-path wait yet. Task 2 must qualify its consuming paths separately. |

This wait-preserving extraction changes production bytes on the existing caller
path, including making response buffers zeroizing. The mandatory benchmark
condition is "touches a hot path", not "adds an external wait". Therefore
functional tests alone cannot close the performance gate.

## Proposed blocking finding

### BEHAVIOR-1 — EVIDENCE_GAP: changed Gateway Vault path lacks mandatory benchmark

- Authority: current user preference requiring a full clean
  `mise run bench:l1:<engine>` VALID comparison for a hot-path change;
  spec Acceptance observations (`spec.md:286-289`); benchmarks/AGENTS.md and
  README Source/Records/Reporting rules.
- Exact location: `crates/shared/wyrd-vault/src/lib.rs:145-158,367-400`, reached
  from `crates/wyrd/wyrd-gateway/src/engine.rs:862-866`.
- Starting condition/trigger: configured ExternalSecret Vault credential,
  admitted Gateway call attempts an authenticated upstream invocation.
- Expected: review includes the added/moved wait inventory and a full clean
  `mise run bench:l1:gateway` run with VALID verdict, compared with the newest
  same-experiment/same-host record under benchmarks/history.
- Actual/missing proof: task verification lists functional, CI, docs, lint and
  dependency checks, but no benchmark; `verification.md` confirms no benchmark
  rerun; task says no hot path changed. `benchmarks/history/` is not mounted in
  this checkout. Neither a record nor a qualified comparison was supplied.
- Consequence/materiality: the explicit performance acceptance gate cannot be
  assessed for shared code executing while Gateway callers wait. This is an
  evidence gap, not a demonstrated latency/throughput regression.
- Smallest correction: obtain the required clean candidate run and existing
  same-host comparison through the canonical benchmark workflow; record the
  result and correct the task's no-hot-path assertion. No production rewrite
  or new benchmark machinery is requested. Preserve pre-existing user edits;
  a clean isolated checkout is sufficient if a run is needed. A dirty/smoke/
  steps run cannot close the finding.

## Four server test corrections

- `scribe_outbox.rs:1163-1233`: `EncodingHold` is cfg(test); one mutex now guards
  started/released for the one condvar, preserving parking/count/release
  behavior without macOS's two-mutex condition-variable error. Existing callers
  are the encoding hook and cancellation-capacity test. No production wait moved.
- `pg_durable_verifier_work.rs:746-753`: case-insensitive AlreadyExists is
  tolerated only while constructing the optional kind-collision fixture. The
  existing two-entry check remains (`:783-787`), and rejection/no-publication
  assertions remain for every real collision. No assertion is weakened for a
  realizable case.
- `pg_invocation_tests.rs:4686-4738`: workload charge is asserted before the
  FIFO writer releases the blocked open; storage failure still stages zero
  rows, returns charge, and the subsequent normal capture stages its row and
  returns charge. This matches object_store's fstat-sized read, which sees zero
  FIFO size; removing the doomed FIFO write preserves intended coverage.
- `pg_deferred_evidence.rs:633-668`: bounded polling observes actual returned
  Python worker, zero scratch and free model permit while Scribe remains held,
  rather than treating the earlier step-begin event as park completion. It
  additionally rechecks running status before release; waiting/resume/completion
  assertions remain. The nonempty idle-worker requirement avoids vacuous success.

Task records full `test:wyrd` 2849/2849 and `test:shared` 898/898 after these
corrections. This reviewer did not rerun those broad suites; root reran the
focused crypto/provider suite. Recorded traces/source causes explain each
correction. Directory fix's missing pre-fix diagnostician is explicitly
recorded; the task does not independently document diagnostician consultation
for the condvar fix. Those are historical process/provenance limits; do not
manufacture historical diagnosis or require production churn to repair them.

## Process and limits

Behavioral acceptance remains subject to independent provider/security review
and validation of BEHAVIOR-1. TDD compliance is VIOLATED: scenarios 2 and 3 were
implemented before RED; mutation checks strengthen assertions but do not repair
history. Directory correction's process deviation also remains visible.
Scenarios 1, 4 and 5 have recorded expected missing-behavior RED/GREEN, though
this review can reproduce only current GREEN. No implementation, commit, push,
or broad journey rerun was performed. Live GCP/AWS, server cutover, and resource
administration are not proven by these shared capability checks.
