# Independent structured Ponytail validation

## Subject and evidence

Base: `526355fb8c58dae6d67d516f98dddcd5c02b0e79`.
Candidate: `202e5bab768cb998a01f04555c39f43b53c09929`.
Approved authority: `changes/active/sealing-keyring/spec.md`, revision 1.
Original task: absent by explicit human direction; the specification and cumulative commits are the review subject. Nothing has shipped, so deployed-format compatibility is not an acceptance requirement.

Read all seven discovery reports, `followup-review.md`, `claim-comparison.md`, `verification.md`, and the cumulative diff. Independently inspected the cited source, complete bodies of the functions whose documentation would change, their callers and underlying error owners, and the key-loading, boot-rewrap, persistence and credential-consumption paths. Used CodeGraph before implementation navigation. Applied the skill, AGENTS.md, agent rules, spec-driven-development and maintainer references, applicable design/server ownership, security-posture, operations configuration/recovery, and testing authority. HEAD remained the candidate; tracked source is unchanged.

## Claim dispositions

| Discovery claim | Disposition | Independent conclusion |
| --- | --- | --- |
| BH-001 | CONFIRMED | `managed.rs:161–170` materially changes the fallible test helper but omits `# Errors`. Its body forwards the documented production open error. Consolidated into FIND-sealing-keyring-1. |
| STD-001 | CONFIRMED | Both cited changed functions lack required error sections; the boot helper also incorrectly claims no IO although its resolver reads configured CA files. Consolidated into FIND-sealing-keyring-1. |
| MAINT-001 | CONFIRMED | Same two contracts and same correction boundary as STD-001, independently supported by their complete bodies and callees. Consolidated into FIND-sealing-keyring-1. |
| SYS-001 | REVISED | The two-replica failure is reachable, but a new rotation runbook is unnecessary. Delete the optional incomplete recipe from the newly added configuration row. Consolidated into FIND-sealing-keyring-2. |
| FU-001 | CONFIRMED | The existing loaded-reader prerequisite proves the failure sequence. Deleting the optional sentence is sufficient and smaller than expanding the procedure. Consolidated into FIND-sealing-keyring-2. |

The standards report's function-scoped import note is non-blocking placement feedback. It has no demonstrated runtime, security, tenancy, durability or public-contract consequence and is not a retained finding or required remediation. Empty security/data finding ledgers and runtime acceptance conclusions were checked against the relevant source paths; they do not establish executed verification.

## Validated finding ledger

### FIND-sealing-keyring-1 — Changed fallible contracts lack required error rustdoc

- Discovery IDs: BH-001, STD-001, MAINT-001.
- Status: CONFIRMED. Classification: VIOLATION.
- Violated obligation: AGENTS.md §16 and the final Rust-documentation rule in `architecture/agent-rules.md` require substantive rustdoc, including `# Errors` for every materially modified fallible Rust function and test helper. The task-review skill explicitly keeps this hard gate blocking even when the missing prose has no runtime effect.
- Exact locations: `crates/wyrd/wyrd-server/src/boot/mod.rs:1321–1356`, `attach_config_fields`; `crates/wyrd/wyrd-gateway/src/managed.rs:161–170`, `tests::open`.
- Evidence and producer/consumer trace: `build_state` calls `attach_config_fields` at line 1201. The helper propagates `OperatorKeys::for_role` errors and maps resolver and HTTP-dispatch construction failures to `ServerBootError::GatewayTransport`. `OperatorKeys::for_role` delegates to `OperatorKeys::new` for API-bearing roles; its errors include invalid Vault client configuration and an out-of-range version. `GatewayConfig::credential_resolver`, lines 2615–2652, reads a file-backed CA with `std::fs::read`, validates/builds each backend and returns `ConfigError::Invalid` on failure. Thus the helper's existing “Pure/sync (no I/O)” statement is false, and there is no `# Errors` section. The test helper delegates directly to `ManagedSecretBinding::open` then exposes the returned SecretString for assertions. That owner authenticates the envelope, parses its payload, verifies tenant/name/provider, and returns only `CredentialError::Unavailable` for the relevant failures. Its production sibling consumers are server administration's seal path and runtime `CredentialResolver::resolve`; its tests consume both successful plaintext and negative outcomes. The helper's docs omit these error conditions despite its changed signature and body.
- Consequence: the explicit documentation completion gate fails; the boot helper misstates an IO boundary and omits the configuration/construction failures that abort state composition. This finding does not claim a new runtime defect.
- Decision-complete minimal correction: add concise, accurate `# Errors` sections to these two existing functions and remove or correct the boot helper's no-IO claim. Reuse the actual contracts of `OperatorKeys::for_role`, `GatewayConfig::credential_resolver`, HTTP dispatcher construction and `ManagedSecretBinding::open`; the test helper should name `CredentialError::Unavailable` for unopenable, malformed or mismatched payloads. Preserve all code, signatures, callers, sync/async boundaries, errors, and lifecycle behavior.
- Ponytail assessment: deleting these functions would remove required composition or useful existing test behavior; a new abstraction or checker is unnecessary. Their existing lower-level contracts provide the required information. This is one documentation correction boundary, not three separate findings.
- Focused closure proof: compare both complete final rustdoc blocks with the unchanged bodies and named callees; verify error conditions and IO behavior agree. Run `mise run fmt:check`. No new runtime test or harness is justified by this prose-only correction. Batch with the substantive operational-documentation correction below.

### FIND-sealing-keyring-2 — Optional rotation recipe skips loading the next key on readers

- Discovery IDs: SYS-001, FU-001.
- Status: REVISED. Classification: INCORRECT.
- Violated obligation: REQ-007 uses the existing deployment keyring lifecycle; operator guidance must accurately project that lifecycle. `SealingKeyring` at `crates/shared/wyrd-crypt/src/lib.rs:181–191` explicitly requires adding the next key as retained on every replica before switching writers. `AuthConfig::sealing_key` at `config.rs:2333–2348` repeats that prerequisite. Supported topology includes multiple serving replicas; operations configuration authority treats encryption configuration as static state requiring restart.
- Exact location: `docs/src/content/docs/operate/deploy-wyrd-for-a-team.md:72`, the optional sentence beginning “To rotate” in the added sealing-key configuration row.
- Evidence and producer/consumer trace: configuration loaders read secret files into owned values; `build_sealing_keyring` at `boot/mod.rs:1956–1976` constructs one in-memory ring. `build_state` installs that ring and calls the existing `rewrap_sealed_secrets` before returning ready state. `SealedSecretRewrap::run/settle` walks `SealedSecretTable::ALL` and the platform secret, resealing with its write key and swapping exact previous bytes. Gateway administration is the other writer of its credential ciphertext; it uses the same process ring. A new invocation reads the tenant snapshot and `CredentialResolver::resolve` passes its sealed bytes to the process's ring through `ManagedSecretBinding::open`. Shared-ring sibling consumers include the stored human, trusted-issuer and platform client secrets already covered by the same pass. Updating a mounted file cannot change a loaded process ring.
- Reachable scenario and consequence: replicas A and B start holding K1 only. The documented procedure switches A to K2-write/K1-retained and restarts it. A's boot pass rewrites stored secrets under K2 while B still holds K1 only. B's fresh Gateway invocations cannot open those credentials and refuse before provider dispatch until B reloads an adequate key set. This is temporary loss of availability, not corruption, and requires neither an older software release nor previously shipped data. A draft/partial page does not remove the effect of its affirmative instruction about replicas.
- Decision-complete minimal correction: delete only the optional “To rotate, … at startup” sentence from the newly added configuration row. Keep the file and inline key settings, common-replica key requirement, purpose and missing-key behavior. The spec does not require a new operator rotation runbook. Existing runtime owners already support safe rotation; change none of them. Do not expand this into compatibility, coordination, readiness or key-reload machinery, or edit unrelated older guidance.
- Ponytail assessment: deletion fully preserves the requested setup documentation and removes the unsafe instruction at its source. It wins over adding a two-phase runbook. Existing source contracts establish the safe prerequisite; no replacement implementation is needed.
- Focused closure proof: inspect the remaining row for correct configuration information and absence of the incomplete recipe; run `mise run docs:check`. No live two-replica test or new harness is necessary for deletion of this optional prose. If someone elects to retain rotation instructions instead, they must include loading K2 as retained on every serving process before any K2 writer/boot pass, but that expansion is not the selected remediation.

## Reconciled acceptance assessment

| Obligation | Source assessment | Evidence limit |
| --- | --- | --- |
| Deployment sealing/opening, same envelope and boot rewrap | PASS at source: administration and resolver share auth state's ring; Gateway joins existing table inventory and exact-byte CAS. | Relevant unit and integration bodies inspected; no executed journey claim. |
| Tenant/name/provider binding and tenant isolation | PASS at source: authenticated payload contains all three and the consumer compares them; tenant SQL uses TenantConn, rewrap uses OperatorPool and tenant/identity/old-bytes CAS. | Tenant-isolation supporting check passed; negative-path bodies inspected. |
| Remove independent Gateway keyring with no compatibility | PASS: deleted configuration, key owner, state and column plumbing have matching consumer changes. | No shipped-data conversion requirement is inferred. |
| Dev key before first boot, persistence, supplied-key precedence and quickstart | PASS at source: invocation provisions once at installer root, publishes without replacement, and passes the path to existing child owner; real CLI journey drives first boot and restart. | Journey presence is proven; candidate execution and historical RED/GREEN are unestablished. |
| Secret non-disclosure | PASS at source: redacted secret types/views, opaque envelope, zeroizing encoded/decrypted payloads, path-only child configuration. | Inspected leak assertions were not executed by this validator. |
| Accurate added operational guidance | FAIL: FIND-sealing-keyring-2. | Bounded documentation correction, no runtime redesign. |
| Required changed-item rustdoc | FAIL: FIND-sealing-keyring-1. | Explicit hard-rule exception, not preference or placement feedback. |
| AC-001 execution / AC-003 gates | Not established in full. | See below; absence of execution is not an invented code defect. |

## Verification limits and verdict recommendation

`verification.md` records candidate PASS for `mise run fmt:check`, `mise run check:tenant-isolation`, `mise run check:deps` (including its one production inline-secret security test), and cumulative `git diff --check`. Full lints, codegen regeneration/check, docs build and full journey lanes have not run in this review. No historical implementation execution record was supplied. This validator ran no Cargo command or test suite, changed no source and did not execute older commits. Source presence cannot certify AC-001 execution or AC-003 completion. Do not turn this evidence limit into an unsupported implementation finding or add broad journey suites to a prose-only remediation task; final change verification retains the full acceptance obligation.

Recommend **FIX_REQUIRED** for the two independently validated bounded findings. All required discovery/follow-up reports are present and source identities remain stable; no unresolved conflict or new material decision requires BLOCKED or SPEC_REVISION_REQUIRED. Preserve both stable IDs in one remediation task. No prior remediation findings exist in this first round.

Skipped runtime and full-gate execution; acceptance execution remains unproven. The selected corrections change documentation only and introduce no new runtime or migration risk.
