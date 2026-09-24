---
id: TASK-007-R3
kind: remediation
status: ready
spec: SPEC-verified-change-contract
spec_revision: 35
requirements: [REQ-141, REQ-147, AC-029, AC-031]
depends_on: []
parent_task: TASK-007
remediates: [FIND-TASK-007-10, FIND-TASK-007-13, FIND-TASK-007-14, FIND-TASK-007-15]
---

# TASK-007-R3 — Close Remaining Operator Delivery Findings

## Authority and immutable subject

- Approved spec: `changes/active/verified-change-contract/spec.md`, revision 35
- Original task: `changes/active/verified-change-contract/tasks/TASK-007-operator-connections-and-delivery.md`
- Review ledger: `changes/active/verified-change-contract/review/TASK-007-r3/findings-validation.md`
- Base: `f8811ac5035c3aa165d34c38992f9889b3c9081f`
- Reviewed candidate: `57ee8dd0b3dddc3e1da34bf543ecc566bae2e831`

## Outcome

Finish TASK-007 by making exact key-version construction non-panicking across
role boundaries, correcting the PagerDuty deduplication contract, and bringing
the confirmed new Rust declarations into the repository's documentation and
import style. Preserve all working connection, delivery, security, tenancy,
durability, SDK, retry, and provider behavior.

## Diagnoses and required corrections

### `FIND-TASK-007-10` — exact key-version handling can still panic

R2 rejects active versions above `i32::MAX` for API-bearing roles and preserves
accepted values exactly, but that validation is intentionally skipped for a
dedicated `ForgeWorker`, which does not own API-only Operator settings. Normal
process startup still builds shared state for every role, and the state
attachment unconditionally constructs configured `OperatorKeys`.
`OperatorKeys::new` narrows the external `NonZeroU32` with `expect`, so a Forge
configuration containing `2147483648` passes configuration loading and panics
during state construction. The public constructor is independently reachable
from test/server builders and accepts the same unvalidated input through its
existing `Result` signature.

Preserve role ownership: a role that does not serve the API/Operator runtime
must keep the unused default key owner rather than construct from irrelevant
Operator configuration. API-bearing roles continue to validate and construct
the configured owner. Independently replace the constructor's narrowing
`expect` with a selector-free typed `KeyError` through its existing `Result`,
so direct unvalidated callers cannot unwind. Keep the existing API-role
configuration rejection and exact `i32::MAX` value. Do not broaden Forge
validation, widen the SQL column, add another version type, or access a
provider for a role that does not own this capability.

### `FIND-TASK-007-13` — PagerDuty documentation overclaims deduplication

The PagerDuty event builder correctly uses the dispatch ID as the default
stable dedup key, and delivery correctly treats provider HTTP acceptance as
the success boundary. Its new rustdoc nevertheless says every retry collapses
into one incident. REQ-141 permits PagerDuty to group repeated trigger events
but explicitly rejects an exactly-once or guaranteed incident-grouping claim.

Change only that rustdoc to say the stable key lets PagerDuty potentially
group retry events and is not an exactly-once delivery or incident guarantee.
Preserve the payload, key fallback, transport, retry, and success behavior. No
runtime test or new abstraction is warranted for this prose-only correction.

### `FIND-TASK-007-14` — four trait methods lack mandatory rustdoc

The cumulative candidate added four trait methods without the documentation
required by AGENTS.md and `architecture/agent-rules.md`:

- `HttpsOrigin::fmt` must document emission of the normalized origin;
- `HttpsOrigin::deserialize` must document parsing/normalization and its serde
  error conditions;
- `SealedSecret::fmt` must document that debug output exposes only the key
  version, never ciphertext or wrapped key material; and
- `OperatorDispatchQueue::default` must document the three-attempt,
  five-minute, 45-second-lease defaults and why the lease exceeds the attempt
  timeout.

Add concise rustdoc directly to those four methods. Do not add wrappers,
tests, generated prose, or unrelated documentation churn.

### `FIND-TASK-007-15` — candidate declarations bypass module import manifests

Candidate-added declarations use fully qualified `std::fmt`,
`std::num::NonZeroU32`, and `std::path::Path` types in fields, signatures, and
trait implementations. Repository authority requires module-top imports and
bare type names in those positions.

At the cited owners in `config.rs`, `components/operators/keys.rs`,
`wyrd-sql/src/queries/operator_connections.rs`, and
`wyrd-cli/src/operator_connection.rs`, add the existing standard-library types
to the module import blocks and use bare names in every validated position.
Alias the formatting result only where needed to avoid colliding with ordinary
`Result`. Add no helper, suppression, dependency, or broad mechanical rewrite.

## Constraints and preserved behavior

- Preserve all provider contracts, routes, permissions, audit transactions,
  forced RLS, encryption/AAD, key selectors, persistence schema, retry
  ceilings, deadlines, SSRF ordering, and delivery success boundaries.
- Preserve API-bearing production readiness and exact active-version behavior,
  including acceptance of `i32::MAX` and rejection of `i32::MAX + 1` before
  provider access.
- Preserve the existing role boundary: Forge does not validate, construct, or
  access API-only Operator key configuration.
- Preserve the selector-free public/internal key error boundary and redacted
  `Debug` behavior.
- Preserve Python, TypeScript, HTTP, CLI, MCP, OpenAPI, and generated contracts;
  no public wire or schema change is needed.

## Non-goals

- No new provider, public API, error code, version type, SQL schema, key source,
  scheduler, cache, transport, dependency, trait, registry, or compatibility
  path.
- No redesign of shared server state, Operator delivery, rewrap scheduling, or
  role validation.
- No guaranteed/exactly-once PagerDuty delivery or incident grouping.
- No documentation sweep beyond the four independently confirmed methods and
  no import rewrite beyond the validated candidate-added positions.

## Acceptance criteria

| Finding | Required closure |
|---|---|
| `FIND-TASK-007-10` | An oversized Operator version is ignored without unwind or provider access for `ForgeWorker`, is refused as a typed pre-boot configuration error for an API-bearing role, and returns a typed selector-free error when passed directly to `OperatorKeys::new`; `i32::MAX` remains exact. |
| `FIND-TASK-007-13` | PagerDuty rustdoc describes possible provider grouping and explicitly disclaims exactly-once delivery/incident guarantees, with no wire or runtime change. |
| `FIND-TASK-007-14` | The four named trait methods have concise rustdoc covering their normalization, error, redaction, or default-limit invariants. |
| `FIND-TASK-007-15` | Every validated candidate-added field/signature/trait implementation uses module-top standard-library imports and bare type names; no suppression or helper is introduced. |

## Focused and broader proof

Add the smallest focused proof for `FIND-TASK-007-10`: one configuration or
build-state test covering the dedicated Forge role and API-bearing rejection,
plus one constructor unit test proving direct oversized input returns the typed
error without unwind. Retain the existing exact-maximum assertion. Record exact
`mise exec -- cargo nextest run --locked` selectors for each named Rust test.

The remaining findings require source inspection rather than new runtime
tests. Run `mise run fmt` and `mise run lints`, plus the narrow existing
server/config/key tests covering exact-version validation and the relevant
crate-family tasks for the touched server, SQL, spec, and CLI owners. Run
`git diff --check`. No codegen, SDK, or journey lane is required unless the
implementation unexpectedly changes a public contract or behavior.

Route this task directly to `$wyrd-implement`.
