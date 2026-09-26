# TASK-003 R3 Wave-2 Findings Validation

## Immutable Subject

- Original base: `1609e102881dd55b12154248834a8aedbb7a36b2`
- Cumulative candidate: `a5a5b60f446981760ac831f64aba871582ec45e4`
- Approved specification: `changes/active/verified-change-contract/spec.md`, revision 33
- Original task: `changes/active/verified-change-contract/tasks/TASK-003-binding-projection-and-runtime-activity.md`
- Prior review and remediation: complete `TASK-003-r1` and `TASK-003-r2` verdicts, validated ledgers, and remediation tasks
- Candidate `HEAD` before validation: `a5a5b60f446981760ac831f64aba871582ec45e4`
- CodeGraph: unavailable because `.codegraph/` is absent

The complete original-base-to-candidate diff, every R3 Wave-1 report, the
applicable repository authorities, and the complete bodies and callers of each
reported location were inspected. The candidate remained immutable during
validation.

## R3 Source-Finding Dispositions

| R3 source ID | Disposition | Validated result |
|---|---|---|
| `TREV-003-R3-001` | **REJECTED** | `binding_ids` is optional in the authoritative Rust serde contract and generated schemas. The TypeScript projection matches that contract, and the current Card hydration path still emits the property whenever it emits TASK-003 verification state. `FIND-TASK-003-9` remains closed. |
| `STD-003-R3-001` | **CONFIRMED** | Every listed violation is candidate-introduced and outside the rule's two exceptions. Retained as new `FIND-TASK-003-14`; the correction is only top-level imports and bare names in the existing fields, signatures, bounds, and aliases. |

### `TREV-003-R3-001` validation

The proposed reopening conflicts with the governing contract and is therefore
rejected:

- Approved `REQ-134` requires a bound Service or standalone Agent Card read to
  expose `card.status.verification.binding_ids`; it does not redefine the
  complete `VerificationStatus` wire object so that the property is universally
  required whenever that object exists.
- `VerificationStatus.binding_ids` is owned by
  `crates/wyrd-spec/src/card/verifier.rs:205-209`. Its
  `#[serde(default, skip_serializing_if = "Vec::is_empty")]` accepts omission as
  an empty vector and omits the property when empty. The generated Card and
  GetCardResponse schemas accordingly contain no `required` entry for
  `binding_ids` and reject `null` when the property is present.
- All three Card GET producers call the same complete `hydrate_card` body at
  `crates/wyrd/wyrd-server/src/components/cards/service.rs:268-338`. That body
  constructs `verification` only from a non-empty binding list, so the current
  TASK-003 runtime always includes a non-empty `binding_ids` property when it
  emits verification state. This runtime narrowing does not erase the broader
  shared serde/schema contract.
- `Cards.get` and `WyrdState.card` both return the exported TypeScript `Card`.
  `Card.status`, `CardStatus.verification`, and
  `VerificationStatus.binding_ids?: readonly string[]` project the Rust/schema
  nullability and omission rules exactly. The existing journey reaches the real
  SDK-to-server path and reads the property without a cast.
- The R2 remediation wording is subordinate to the approved specification and
  owning contract. Its requested “readonly string list” fixes the value type;
  it cannot override the Rust serde owner and generated schemas by making an
  omittable wire property mandatory.

Requiring `binding_ids` in TypeScript would make that SDK stricter than the
language-agnostic wire contract. No correction is warranted.

### `STD-003-R3-001` validation

The complete listed set survives provenance and exception checks:

| Location | Candidate provenance, caller, and rule result |
|---|---|
| `crates/wyrd-spec/src/envelope.rs:476` | The candidate adds the `Status.verification` field as `Option<crate::card::verifier::VerificationStatus>`. It is reached by shared Card serialization, schema generation, and every hydrated Card read. The type belongs in the module import block and must be bare in the field. |
| `crates/wyrd-spec/src/ids.rs:234,247,257,290-294` | The entire `BindingId` item is new and is consumed by projection, activity, status, schemas, and public tests. Its tuple field, constructor parameter, return type, and `Deserializer` bound use qualified names in positions explicitly covered by `architecture/agent-rules.md`. Existing qualified expressions elsewhere in the file are not part of this finding. |
| `crates/wyrd/wyrd-testing/src/server.rs:1613-1616` | `last_authenticated_at` is new and is called by the Rust observation journey at `sdks/wyrd-sdk-rust/tests/observe_run.rs:532,582`. Its qualified `chrono` return type violates the bare-signature rule. |
| `crates/wyrd/wyrd-server/tests/identity_e2e.rs:517,555-558` | `register_service` and `owner_last_authenticated` are new helpers called by the workload `jwt-bearer` journey. Their `uuid` and `chrono` signature types are qualified. The file is an integration-test module, but the test-module exception permits its own top-level imports; it does not permit qualified signatures. |
| `crates/wyrd/wyrd-server/tests/pg_card_registration_route.rs:2791-2796,2811-2814,3481,3496-3499,3521-3524,3538-3541,3555-3560` | These helpers and the alias are new and are exercised throughout the Card-status, exact-reference, and runtime-activity journeys. `Response`, `DateTime`, `Utc`, and `CardRef` appear qualified in signatures or the alias, and `owner_gates` contains an ordinary function-scoped import. Neither permitted exception applies: it is not a test submodule's top-level import and is not `use TraitName as _;` for one generic function. |

No inherited or unchanged source is retained. Fully qualified paths used only
inside expressions are also excluded; the finding is limited to the new
fields, signatures, trait bound, type alias, and ordinary function-local import
enumerated above.

## Prior-Finding Closure

| Finding | R3 disposition | Reassessment evidence |
|---|---|---|
| `FIND-TASK-003-1` | **VERIFIED CLOSED** | Operator-bearing registration evaluates and transactionally audits `operators:invoke`; Operator-free registration spends only `cards:write`, and denial leaves no registration state. |
| `FIND-TASK-003-2` | **VERIFIED CLOSED** | The shared CardRef lookup accepts exactly one of at most two active matches; API-key, workload, delegation, and fixture callers retain fail-closed ambiguity handling. |
| `FIND-TASK-003-3` | **VERIFIED CLOSED** | `GREATEST(last_authenticated_at, $2)` preserves monotonic activity, and cursor arming remains null-only. |
| `FIND-TASK-003-4` | **VERIFIED CLOSED** | Effective schedules must parse and produce a future occurrence before registration writes. |
| `FIND-TASK-003-5` | **VERIFIED CLOSED** | `$owner` is non-null, reserved against component aliases, and enforced for standalone Agent owners. |
| `FIND-TASK-003-6` | **VERIFIED CLOSED** | Verification SQL APIs use `PrincipalId`, `BindingId`, and `CardUid`; invalid stored UUID versions are refused at the typed boundary. |
| `FIND-TASK-003-7` | **VERIFIED CLOSED** | `BindingProjector` remains the cohesive transaction-scoped freeze/projection owner; no redundant repository, trait, or factory was introduced. |
| `FIND-TASK-003-8` | **VERIFIED CLOSED** | Real API-key and workload exchanges, exclusions, lifecycle gates, A/B versions, shared replicas, request-driven renewal, and observation no-touch evidence remain present. |
| `FIND-TASK-003-9` | **VERIFIED CLOSED** | TypeScript exports the complete Card status projection and reads it without a cast. Optional `binding_ids` is compliant with Rust serde and generated schemas; the current owner-Card runtime emits it whenever verification state exists. |
| `FIND-TASK-003-10` | **VERIFIED CLOSED** | Served OpenAPI resolves `Card -> Status -> verification -> VerificationStatus -> binding_ids` and proves UUID-array items. |
| `FIND-TASK-003-11` | **VERIFIED CLOSED** | The cumulative Rust additions retain substantive rustdoc and applicable error/panic documentation. The new import/signature violation is a distinct repository rule, not a documentation regression. |
| `FIND-TASK-003-12` | **VERIFIED CLOSED** | The shared `TenantConn` lookup has no manual tenant predicate or tenant bind and preserves active exact-one selection under forced RLS. |
| `FIND-TASK-003-13` | **VERIFIED CLOSED** | The R1 report changed only by removal of trailing whitespace, and the cumulative `git diff --check` command exits zero. |

## Final Validated Finding Ledger

### `FIND-TASK-003-14`

- **Wave-1 source ID:** `STD-003-R3-001`
- **Status:** CONFIRMED
- **Classification:** VIOLATION / RUST SOURCE SHAPE
- **Authority:** `architecture/agent-rules.md` requires imports to live in the
  module's top-of-file dependency block and imported bare names in struct
  fields, function parameters, return types, trait bounds, and type aliases.
  `AGENTS.md` makes those rules mandatory for new and materially modified Rust.
- **Exact locations:** `crates/wyrd-spec/src/envelope.rs:476`;
  `crates/wyrd-spec/src/ids.rs:234,247,257,290-294`;
  `crates/wyrd/wyrd-testing/src/server.rs:1613-1616`;
  `crates/wyrd/wyrd-server/tests/identity_e2e.rs:517,555-558`;
  `crates/wyrd/wyrd-server/tests/pg_card_registration_route.rs:2791-2796,2811-2814,3481,3496-3499,3521-3524,3538-3541,3555-3560`.
- **Evidence:** Every listed item or field was added by the cumulative
  candidate. Complete-body and caller tracing shows the production contract
  types are reached by Card hydration, schema generation, binding projection,
  and activity reads; the test helpers are reached by the identity, Card, and
  runtime-activity journeys. The qualified names occur in positions the rule
  expressly covers, and `owner_gates` uses an ordinary function-local import.
  Neither permitted import exception applies. Passing Clippy does not enforce
  this repository-specific rule.
- **Observable consequence:** The candidate violates a hard repository
  acceptance boundary and hides dependencies outside the required module
  manifests. Runtime behavior remains correct, but the task cannot be approved
  with these new source-shape violations.
- **Decision-complete minimal correction:** Add only the already-used types to
  each existing module's top-level import block and replace the listed
  qualified field/signature/bound/alias names with their bare names. Move
  `InactivityTimeout` and `binding_activity` from `owner_gates` to the existing
  top-level `wyrd_sql::queries::verification` import; import `BindingId` there
  as well if its existing qualified expression is made bare. Do not change
  control flow, public wire shape, runtime behavior, test structure, or add a
  helper, abstraction, dependency, lint suppression, or new check.
- **Existing mechanism to reuse:** The five modules' existing top-level `use`
  blocks and Rust's ordinary imports. No new mechanism is needed.
- **Preserved behavior:** Rust serde/schema optionality, TypeScript status
  optionality, stable UUIDv7 binding identity, Card hydration, principal
  activity semantics, tenant/RLS boundaries, all journey assertions, and every
  prior finding closure remain unchanged.
- **Focused closure proof:** Inspect the corrected diff to confirm every listed
  dependency is top-level and every covered source position uses a bare name;
  run `mise run fmt`, `mise run lints`, and the cumulative
  `git diff --check 1609e102881dd55b12154248834a8aedbb7a36b2..<new-candidate>`.
  These compile and format every changed Rust surface; no new behavioral test
  or harness is credible for an import-only correction.

## Specification Revision Decision

`SPEC_REVISION_REQUIRED` is not indicated. `FIND-TASK-003-14` is a bounded,
behavior-preserving source correction fixed entirely by existing repository
authority. The final validated ledger contains only `FIND-TASK-003-14`.
