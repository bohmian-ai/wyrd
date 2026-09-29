---
id: TASK-003-R3
kind: remediation
status: approved
spec: SPEC-verified-change-contract
spec_revision: 33
requirements: [REQ-078, REQ-104, REQ-105, REQ-112, REQ-134, AC-018, AC-019, AC-020, AC-028]
depends_on: [TASK-003-R2]
parent_task: TASK-003
remediates: [FIND-TASK-003-14]
---

# Close TASK-003 Rust Source-Shape Violations

## Authority and Immutable Subject

- Approved specification: `changes/active/verified-change-contract/spec.md`, revision 33
- Original task: `changes/active/verified-change-contract/tasks/TASK-003-binding-projection-and-runtime-activity.md`
- Prior review history: `changes/active/verified-change-contract/review/TASK-003-r1/` and `TASK-003-r2/`
- R3 verdict: `changes/active/verified-change-contract/review/TASK-003-r3/verdict.md`
- R3 validated findings: `changes/active/verified-change-contract/review/TASK-003-r3/findings-validation.md`
- Original base: `1609e102881dd55b12154248834a8aedbb7a36b2`
- Reviewed cumulative candidate: `a5a5b60f446981760ac831f64aba871582ec45e4`

Implement this remediation cumulatively. The next review reassesses the complete
original base through the new candidate and preserves all finding IDs.

## Diagnosis

### `FIND-TASK-003-14` — Candidate-added Rust items violate source-shape rules

Repository authority requires each module's dependencies to appear in its
top-level import block and requires bare imported names in struct fields,
function parameters, return types, trait bounds, and type aliases. The
cumulative candidate introduced qualified names in those positions and one
ordinary function-local import across:

- `crates/wyrd-spec/src/envelope.rs` (`Status.verification`);
- `crates/wyrd-spec/src/ids.rs` (`BindingId` field, constructor, return type,
  and deserializer bound);
- `crates/wyrd/wyrd-testing/src/server.rs` (activity helper return type);
- `crates/wyrd/wyrd-server/tests/identity_e2e.rs` (new helper UUID/time
  signatures); and
- `crates/wyrd/wyrd-server/tests/pg_card_registration_route.rs` (new response,
  time, CardRef signatures/alias and the `owner_gates` local SQL import).

Every listed item was introduced by TASK-003 and is exercised by production
Card contracts or the required identity/Card/activity journeys. Passing Clippy
does not close the issue because this is a repository-specific structural rule.

## Required Correction

Use only the existing Rust import mechanism: add the already-used types and
functions to each file's existing top-level `use` block, replace the validated
qualified field/signature/bound/alias names with their bare names, and move
`InactivityTimeout` plus `binding_activity` from `owner_gates` into the route
test's existing top-level verification-query import.

Limit the edit to the exact candidate-added positions confirmed in
`findings-validation.md`. Qualified paths used only in ordinary expressions,
inherited untouched code, and permitted `use TraitName as _;` cases are not in
scope. Do not add a helper, abstraction, dependency, lint suppression, or new
check.

This closes the finding because all new dependencies become visible in their
module manifests and every source position expressly covered by the rule uses
the imported bare type, without altering any contract or execution path.

## Constraints and Preserved Behavior

- Preserve closure of `FIND-TASK-003-1` through `FIND-TASK-003-13`.
- Preserve Rust serde/schema optionality and the TypeScript status projection,
  including optional, non-null `binding_ids`.
- Preserve binding identity, Card hydration, activity, audit, transaction, RLS,
  SDK, and journey behavior exactly.
- Do not change public wire shapes, control flow, test scenarios, fixtures,
  module boundaries, visibility, or error behavior.
- Do not introduce an import-only helper, trait, factory, wrapper, dependency,
  lint exception, or static check.

## Acceptance Criteria

| Finding | Acceptance criterion |
|---|---|
| `FIND-TASK-003-14` | Every validated candidate-added dependency is imported in its module's top-level block; every listed field, signature, bound, and alias uses the bare name; the ordinary function-local import is removed; no behavior or public contract changes. |

## Focused and Broader Proof

Inspect the final cumulative diff against the exact location table in
`findings-validation.md`, then run:

```bash
mise run fmt
mise run lints
git diff --check 1609e102881dd55b12154248834a8aedbb7a36b2..<new-candidate>
```

These commands compile and format every touched Rust surface. No new behavioral
test or harness is credible for an import-only correction; do not manufacture
one. If an existing focused test fails after the import-only edit, fix the
source-shape edit rather than changing the test or behavior.

## Implementation Evidence

Code commit: `d560c9964` (cumulative on `a5a5b60f4`).

| Acceptance criterion | Implementation evidence | Verification evidence | Result |
|---|---|---|---|
| `FIND-TASK-003-14`: validated dependencies imported at module top | `envelope.rs` (`VerificationStatus`), `ids.rs` (`Deserializer`, `Uuid`), `wyrd-testing/src/server.rs` (`DateTime`), `identity_e2e.rs` (`DateTime`, `Utc`, `Uuid`), `pg_card_registration_route.rs` (`Response`, `DateTime`, `Utc`, `CardRef`, `InactivityTimeout`, `binding_activity`) | Diff inspection against the `findings-validation.md` location table; `mise run lints` (all targets) | PASS |
| Listed fields, signatures, bound, and alias use bare names | `Status.verification`; `BindingId` field/`new`/`as_uuid`/deserializer bound; `last_authenticated_at`; `register_service`, `owner_last_authenticated`; `read_card`, `BindingActivityRow`, `live_ref`, `principal_activity`, `owner_principal`, `schedule_cursor`, `owner_gates` | Diff inspection; `mise run lints` | PASS |
| Function-local import removed | `owner_gates` no longer has a `use` | Diff inspection | PASS |
| No behavior or public contract change | Import/type-path edits only; no expression, control-flow, serde attribute, or visibility change | `mise run fmt` (no diff); `mise run lints` exit 0 | PASS |

Commands (all exit 0):

```bash
mise run fmt
mise run lints
git diff --check 1609e102881dd55b12154248834a8aedbb7a36b2..HEAD
```

Non-goals held: no helper, dependency, lint exception, or check added. Out-of-scope qualified expressions and inherited sites were left alone. The only files changed are the five listed Rust files and this packet.
