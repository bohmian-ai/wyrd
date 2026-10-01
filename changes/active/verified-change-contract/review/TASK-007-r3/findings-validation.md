# TASK-007 R3 Wave 2 Finding Validation

## Immutable subject

- Repository: `/home/thorrester/Documents/GitHub/wyrd-vcc-t007`
- Base: `f8811ac5035c3aa165d34c38992f9889b3c9081f`
- Candidate: `57ee8dd0b3dddc3e1da34bf543ecc566bae2e831`
- Approved specification: `changes/active/verified-change-contract/spec.md`, revision 35
- Original task: `changes/active/verified-change-contract/tasks/TASK-007-operator-connections-and-delivery.md`
- Prior reviews and remediation: `review/TASK-007-r1/`, `review/TASK-007-r2/`,
  `TASK-007-R1-operator-delivery-corrections.md`, and
  `TASK-007-R2-operator-delivery-corrections.md`
- Wave 1 inputs: `task-review.md`, `standards-review.md`,
  `domain-review-security.md`, and `domain-review-delivery.md`

The candidate remained the stated commit at the beginning and end of this
validation. CodeGraph was unavailable because the repository has no
`.codegraph/` index, so caller tracing used the candidate source and `rg`.

## Proposed-finding validation

| Wave 1 proposal | Result | Independent validation |
|---|---|---|
| `TREV-R3-001` | **REVISED** | The path is reachable: `WyrdServerConfig::load` parses `NonZeroU32`, skips all Operator-key validation for `ForgeWorker`, `build_state` always calls `attach_config_fields`, and that function always calls public `OperatorKeys::new`, whose narrowing `expect` panics above `i32::MAX`. Retain under prior `FIND-TASK-007-10`, but the proposed “skip construction” correction alone leaves the same public constructor panic reachable from its other callers. The standards proposal to validate all Operator settings for Forge contradicts the explicit role-ownership rule and `forge_worker_validation_ignores_api_only_settings`. The minimum complete correction is therefore both to leave non-API state on its unused default key owner and to make the public constructor return a typed error for unvalidated oversized input. |
| `STD-R3-001` | **REVISED** | Duplicate of `TREV-R3-001` and the still-open exact-version finding. Its constructor-error requirement is necessary; its request to validate the setting for every role is rejected because a dedicated Forge worker deliberately ignores API-only configuration. Deduplicated into `FIND-TASK-007-10`. |
| `TREV-R3-002` | **CONFIRMED** | `pager_duty::event` is the live request-body builder called by `OperatorDelivery::send`. Its rustdoc says every retry “collapses into one PagerDuty incident,” while REQ-141 says grouping is only possible and expressly disclaims exactly-once delivery. The implementation sends the correct stable key; only the source contract is wrong. Retained as new `FIND-TASK-007-13`. |
| `STD-R3-002` | **REVISED** | The hard documentation rule is violated by four candidate-added trait methods at the three cited sites. The unbounded instruction to rediscover every undocumented item in 18,600 changed lines is not a decision-complete correction. Retain the independently confirmed methods only: `HttpsOrigin::{fmt, deserialize}`, `SealedSecret::fmt`, and `OperatorDispatchQueue::default`. Retained as new `FIND-TASK-007-14`. |
| `STD-R3-003` | **CONFIRMED** | The cited candidate-added fields and signatures spell standard-library types inline despite the explicit top-level-import/bare-name rule. The change is mechanical, uses the standard library already present, and needs no abstraction or runtime proof. Retained as new `FIND-TASK-007-15`. |

The security-domain report's claim that prior `FIND-TASK-007-10` is closed is
overruled by the directly reachable non-API boot path above. The delivery-domain
report proposed no material finding and does not contradict the retained
ledger.

## Final deduplicated finding ledger

### FIND-TASK-007-10 — REVISED — INCORRECT: exact key-version handling still permits a reachable configuration panic

- **Wave 1 sources:** `TREV-R3-001`, `STD-R3-001`
- **Prior identity:** R2 `FIND-TASK-007-10` remains open. R2 closed silent
  version truncation for API-bearing roles but did not establish its required
  “before boot” refusal or no-panic property at every constructor path.
- **Violated obligation:** R2 `FIND-TASK-007-10`, AGENTS.md §4, and the Rust
  error-handling authority require external configuration to be refused rather
  than unwound. The existing role boundary also requires a dedicated
  `ForgeWorker` to ignore settings for API capabilities it does not own.
- **Exact location:**
  `crates/wyrd/wyrd-server/src/config.rs:1956-1981,2482-2503,2865-2876,3468-3493`;
  `crates/wyrd/wyrd-server/src/boot/mod.rs:1252-1322,1489-1502`;
  `crates/wyrd/wyrd-server/src/components/operators/keys.rs:248-284`.
- **Caller and reachability evidence:** `WYRD_OPERATOR_KEK_ACTIVE_VERSION` and
  TOML both deserialize into `NonZeroU32`. `WyrdServerConfig::validate` invokes
  `OperatorKeysConfig::validate` only when `role.serves_api()`. The sole normal
  process entry calls `build_state` for every role before branching to
  `run_forge_worker_process`; `build_state` unconditionally reaches
  `attach_config_fields`, which unconditionally constructs configured
  `OperatorKeys`. `OperatorKeys::new` then performs
  `i32::try_from(...).expect(...)`. Its other real callers are the test-server
  builder and server integration fixtures, all of which already consume its
  `Result`; no caller requires a panic contract.
- **Observable consequence:** a dedicated Forge process with an irrelevant
  Operator key version of `2147483648` passes configuration loading and panics
  during state construction. Direct unvalidated constructor use can trigger
  the same unwind.
- **Decision-complete correction:** preserve role ownership by attaching
  `OperatorKeys::default()` for a role that does not serve the API/Operator
  runtime, and construct the configured owner only for API-bearing roles.
  Independently replace the constructor's narrowing `expect` with a typed,
  selector-free `KeyError` returned through its existing `Result`, so every
  direct caller is safe even if validation was skipped. Keep the existing
  `OperatorKeysConfig::validate` rejection for API-bearing roles and exact
  `i32::MAX` storage. Do not broaden Forge validation, add a version wrapper,
  widen the SQL column, or change provider access.
- **Focused closure proof:** one focused configuration/build-state test proves
  an oversized Operator version is ignored without unwind for `ForgeWorker`
  and refused before provider access for an API-bearing role; one constructor
  unit test passes the oversized config directly and observes the typed error;
  the existing `i32::MAX` exact-value assertion remains green.

### FIND-TASK-007-13 — CONFIRMED — VIOLATION: PagerDuty rustdoc promises guaranteed incident collapse

- **Wave 1 source:** `TREV-R3-002`
- **Violated obligation:** REQ-141 and TASK-007's non-goals prohibit treating a
  stable PagerDuty dedup key as exactly-once delivery or guaranteed incident
  grouping.
- **Exact location:**
  `crates/wyrd/wyrd-server/src/verification/operators/pager_duty.rs:12-19`.
- **Caller and reachability evidence:** `OperatorDelivery::send` calls
  `pager_duty::event` for every live PagerDuty dispatch, then sends that body
  through `OperatorDelivery::pager_duty`. The helper correctly defaults the
  key to the dispatch ID and performs no grouping itself. The rustdoc alone
  says every retry collapses into one incident, which Wyrd cannot observe or
  ensure after Events API acceptance.
- **Observable consequence:** the permanent source contract tells maintainers
  that downstream incident grouping is guaranteed, stronger than the approved
  behavior and actual implementation boundary.
- **Decision-complete correction:** change only the two-line rustdoc claim to
  state that the stable key lets PagerDuty *potentially* group retry events and
  is not an exactly-once delivery or incident guarantee. Preserve the dispatch
  ID fallback, payload, transport, retry, and success behavior.
- **Focused closure proof:** source review against REQ-141; no runtime test,
  helper, dependency, or harness is warranted because behavior is unchanged.

### FIND-TASK-007-14 — REVISED — VIOLATION: four candidate-added trait methods lack mandatory rustdoc

- **Wave 1 source:** `STD-R3-002`
- **Violated obligation:** AGENTS.md §16 and `architecture/agent-rules.md`
  require meaningful rustdoc for every new or materially modified Rust item,
  including trait implementation methods; missing documentation is a hard
  blocker.
- **Exact location:**
  `crates/wyrd-spec/src/operator_connection.rs:135-145`;
  `crates/wyrd/wyrd-sql/src/queries/operator_connections.rs:117-122`;
  `crates/wyrd/wyrd-sql/src/queries/operator_dispatches.rs:154-157`.
- **Evidence:** the cumulative diff adds `HttpsOrigin` and its `Display` and
  custom `Deserialize` implementations, `SealedSecret` and its redacting
  `Debug`, and `OperatorDispatchQueue` and its delivery-ceiling `Default`.
  Each cited trait method has no `///` documentation. Compiler public-doc
  lints do not enforce documentation on these implementation methods.
- **Observable consequence:** the exact normalization/deserialization,
  ciphertext-redaction, and delivery-ceiling implementations omit the
  repository-required maintainer contract even though their surrounding types
  document those invariants.
- **Decision-complete correction:** add concise rustdoc directly to those four
  methods: `HttpsOrigin::fmt` documents emission of the normalized origin;
  `HttpsOrigin::deserialize` documents parse/normalization and its serde error
  conditions; `SealedSecret::fmt` documents that only key version is emitted;
  `OperatorDispatchQueue::default` documents the three-attempt, five-minute,
  45-second-lease defaults and why the lease exceeds the attempt timeout. Do
  not add wrappers, tests, or unrelated prose.
- **Focused closure proof:** source inspection of the four methods against
  AGENTS.md §16; existing format and lint lanes are the broader proof.

### FIND-TASK-007-15 — CONFIRMED — VIOLATION: candidate types bypass the mandatory import-manifest style

- **Wave 1 source:** `STD-R3-003`
- **Violated obligation:** `architecture/agent-rules.md` requires types to be
  imported at module top and used by bare name in fields, parameters, return
  types, trait implementations, and `where` clauses.
- **Exact location:**
  `crates/wyrd/wyrd-server/src/config.rs:1820-1824,1855,1864-1868,1876-1878`;
  `crates/wyrd/wyrd-server/src/components/operators/keys.rs:225-230`;
  `crates/wyrd/wyrd-sql/src/queries/operator_connections.rs:117-122`;
  `crates/wyrd/wyrd-cli/src/operator_connection.rs:118-140`.
- **Evidence:** the new code uses `std::fmt::{Debug, Formatter, Result}` through
  fully qualified paths in three `Debug` implementations,
  `std::num::NonZeroU32` in the key-config field and default signature, and
  `std::path::Path` in `read_body`, while each module already has a top-level
  standard-library import block.
- **Observable consequence:** the touched modules violate the repository's
  explicit source-shape contract and hide dependencies outside the designated
  import manifest.
- **Decision-complete correction:** add the existing standard-library types to
  each module's top-level imports and use their bare names at every cited
  candidate-added position. Alias the formatting result only where necessary
  to avoid collision with ordinary `Result`. Add no helper or suppression.
- **Focused closure proof:** source search over the cited positions plus
  `mise run fmt` and `mise run lints`; no runtime test is warranted.

## Prior-finding closure

| Prior finding | Validated R3 status |
|---|---|
| `FIND-TASK-007-1` | CLOSED — public detail, remediation, complete problem JSON, and logs are selector-free. |
| `FIND-TASK-007-2` | CLOSED — multi-tenant production verifies all active tenant keys before readiness. |
| `FIND-TASK-007-3` | CLOSED — every effective URL is screened and pinned before credential attachment. |
| `FIND-TASK-007-4` | CLOSED — the cumulative diff contains no unrelated skill-policy change. |
| `FIND-TASK-007-5` | CLOSED — production Vault requires HTTPS and token files use owner-only reads. |
| `FIND-TASK-007-6` | CLOSED — Python and TypeScript project the closed provider contracts. |
| `FIND-TASK-007-7` | CLOSED — mounted-secret reads use Tokio's blocking boundary. |
| `FIND-TASK-007-8` | CLOSED — changed production/library imports identified by R1 are module-scoped. |
| `FIND-TASK-007-9` | CLOSED — discovery and tenant work share the bounded, delivery-independent rewrap pass. |
| `FIND-TASK-007-10` | OPEN / REVISED — exact API-role validation is fixed, but non-API boot and direct construction can still unwind. |
| `FIND-TASK-007-11` | CLOSED — key configuration and owner diagnostics use redacted custom `Debug`. |
| `FIND-TASK-007-12` | CLOSED — provider wire mechanics are confined to focused private modules. |

## Recommendation

The validated ledger contains four bounded implementation findings:
`FIND-TASK-007-10`, `FIND-TASK-007-13`, `FIND-TASK-007-14`, and
`FIND-TASK-007-15`. All corrections stay within the approved specification and
reuse the existing config role gate, key owner/error path, provider module, and
standard library. The appropriate task-review outcome is `FIX_REQUIRED`; no
specification revision, public API, dependency, schema, provider, or new
abstraction is required.

## Verification limits

- This validation inspected the complete base-to-candidate diff, all Wave 1
  reports, both prior ledgers and remediation tasks, the approved revision-35
  requirements, and the full bodies and callers affected by the retained
  corrections. Generated schemas and declarations were traced to their owners
  rather than reviewed line by line.
- `git diff --check` passed, and the candidate stayed
  `57ee8dd0b3dddc3e1da34bf543ecc566bae2e831`. No Cargo, Postgres, Python,
  TypeScript, codegen, or live-provider lane was rerun in Wave 2. The candidate
  records its broader lanes as green, but none exercises the non-API oversized
  version path or enforces trait-method rustdoc/import source shape.
- The credentialed Slack/PagerDuty smoke remains gated and was not run. It is
  irrelevant to the confirmed PagerDuty documentation overclaim because the
  implementation payload and acceptance behavior are unchanged.
