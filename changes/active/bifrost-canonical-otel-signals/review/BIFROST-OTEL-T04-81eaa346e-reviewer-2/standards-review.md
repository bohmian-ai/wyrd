# Repository-standards review

## Subject

- Base: `442da074cb316be7f580694ba8274229561935a8`
- Candidate: `81eaa346e643ac6315e517041fc838c41057f7ac`
- Result: `FAIL`

The subject remained immutable throughout this review.

## Authority coverage

| Changed surface | Applicable authority | Result |
|---|---|---|
| `vala-bifrost-redux/src/gate/mod.rs` production ingest identity | `AGENTS.md` §§2–6, 9–12, 15–16; `architecture/agent-rules.md`; `architecture/wyrd-design.md`; `architecture/bifrost-design.md` table/row identity and durability; security posture; architecture constraints/patterns; Rust core; Vala, telemetry, OLAP, and reliability references | FAIL — STD-1 through STD-3 |
| Gate unit coverage | `AGENTS.md` §§11, 16; `architecture/agent-rules.md`; Rust and testing references | FAIL — STD-3; otherwise correctly inlined, synchronous, and credential-free |
| `tables/logs/mod.rs` test correction | `AGENTS.md` §§11, 16; named-field rule; Rust, testing, and telemetry references | PASS — validates by name and proves server restamping |
| `wyrd-testing/tests/bifrost/otlp/negative.rs` | `AGENTS.md` §11; `TESTING.md`; both Bifrost test READMEs; testing, telemetry, OLAP, and reliability references | PASS — uses the existing public OTLP journey with real server and Postgres |
| T04 implementation report | Spec-driven development, implementation execution, and testing workflows | FAIL — STD-4 |
| Added `py-error-refactor` spec/tasks | `AGENTS.md` planning/PyO3/error rules; spec-driven development; errors, PyO3, and Python references | PASS for document structure; task relevance is assessed by the acceptance reviewer |

## Applicable-rule results

- PASS: Vala/Bifrost remains the production owner; no dependency, feature,
  migration, public contract, or generated artifact changed.
- PASS: tenant and logical table scope enter the digest, and existing Scribe
  WAL/fence code remains the duplicate-decision owner.
- PASS: the helper is synchronous and deterministic; `dispatch_canonical`
  remains async because it awaits Scribe IO.
- PASS: replay proof uses the existing tier-one OTLP journey and has focused
  unit coverage.
- FAIL: Bifrost requires an actual UUIDv7 batch identity.
- FAIL: new Rust violates mandatory import and signature style.
- FAIL: new and materially changed Rust items lack required rustdoc.
- FAIL: the execution record calls a failed aggregate invocation a pass.

## Material repository-rule findings

### STD-1 — Batch identity is not UUIDv7

- Rule: `architecture/bifrost-design.md` requires `wyrd_batch_id` to be UUIDv7.
- Location: `crates/vala/vala-bifrost-redux/src/gate/mod.rs:111`.
- Evidence: the first 48 bits come from SHA-256. Setting only the version and
  variant bits makes `Uuid::get_version()` report `SortRand`, but it does not
  make the first 48 bits the UUIDv7 Unix-millisecond field.
- Consequence: persisted row identity claims UUIDv7 while violating its time
  semantics; the unit test checks only the version nibble.
- Testable correction: use the approved batch-identity representation and prove
  both its required format and retry convergence. If UUIDv7 and deterministic
  retry identity cannot share an authoritative timestamp, resolve that durable
  identity decision in the approved specification first.

### STD-2 — Mandatory import/signature style is violated

- Rule: `architecture/agent-rules.md` requires module-top imports and bare
  imported type names in signatures.
- Location: `crates/vala/vala-bifrost-redux/src/gate/mod.rs:101`.
- Evidence: the signature uses fully qualified `DataTenantId`, `RecordBatch`,
  and `Uuid`; `sha2` is imported inside the function.
- Consequence: the module dependency surface is hidden and repository Rust
  style is violated.
- Testable correction: move the imports to the module import block and use bare
  type names.

### STD-3 — Required Rust documentation is incomplete

- Rule: `AGENTS.md` §16 and `architecture/agent-rules.md` require meaningful
  rustdoc for every new or materially changed Rust item, including private test
  modules and panicking helpers.
- Locations: `gate/mod.rs:627`, `gate/mod.rs:1963`, and `gate/mod.rs:1975`.
- Evidence: materially changed `dispatch_canonical` has no workflow/error/retry
  rustdoc, the new test module has no module rustdoc, and its panicking `batch`
  helper lacks `# Panics`.
- Consequence: hard `BLOCK_BEFORE_MERGE` repository-rule failure.
- Testable correction: document the changed durable workflow and retry/error
  behavior, the test module, and the fixture panic, or remove the panic.

### STD-4 — Verification record contradicts itself

- Rule: `implementation-execution.md` requires exact, truthful verification
  evidence.
- Location: `changes/active/bifrost-canonical-otel-signals/tasks/04-integrated-public-journeys.md:784`.
- Evidence: prose says `mise run verify:bifrost` passed, while the table records
  that invocation as 8/9 with `journey:oracle` failed. Only a separate leaf
  rerun passed 28/28.
- Consequence: the aggregate cannot be treated as green.
- Testable correction: either rerun the aggregate uncontended to a pass or
  record it as failed and retain the leaf rerun only as limited follow-up
  evidence.

## Verification limits

Reported `fmt`, `lints`, focused tests, OTLP 9/9, and uncontended Oracle 28/28
passed. This review independently reran
`gate::otlp_batch_id_tests::derived_identity_is_stable_per_tenant_table_and_payload`
successfully and confirmed `git diff --check` is clean. No single
`verify:bifrost` invocation is recorded as passing, and `mise run gate` was not
run.
