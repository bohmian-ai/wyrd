---
id: TASK-002-R4
kind: remediation
status: ready
spec: SPEC-skald-workflow-runtime
spec_revision: 12
parent_task: TASK-002-cleanup
remediates: [FIND-TASK-002-11]
base: 0569b79702218600c4f9790f45cc03100d5c6f1c
reviewed_candidate: 2ff7bbfa57737ce8a7a69287e2fa462c313b1a8c
---

# Close Workflow selector validation at its existing owners

Implementation skill: `$wyrd-implement`. This is a bounded correction of the
last independently validated cumulative TASK-002 finding. Work forward from the
reviewed candidate; preserve commit `375d97e67f3affe0d5c59727ef3135b22a459140`
and the behavior it already closes.

## Authority and subject

- Approved spec: `changes/active/skald-workflow-runtime/spec.md`, Revision 12
- Original task: `changes/active/skald-workflow-runtime/tasks/TASK-002-cleanup.md`
- Prior remediation: `review/TASK-002-r2/TASK-002-R2-close-cleanup-review-gaps.md` and `review/TASK-002-r3/TASK-002-R3-close-remaining-review-gaps.md`
- Original cumulative base: `0569b79702218600c4f9790f45cc03100d5c6f1c`
- Reviewed candidate: `2ff7bbfa57737ce8a7a69287e2fa462c313b1a8c`
- Validated diagnosis: `review/TASK-002-r4/findings-validation.md`
- Governing repository authority: `AGENTS.md`, `architecture/agent-rules.md`, current design/doctrine, and applicable Rust, errors, TypeScript, testing and spec-driven-development references

The human removed FIND-TASK-002-11 from R3 after commit `375d97e67` appeared.
R4 audits that commit and finds it correct for Python, wrong-kind, versionless,
and selector-shape cases, but incomplete for the two reachable paths below.

## Issue diagnosis — FIND-TASK-002-11

Revision 12 REQ-054 and INV-007 require the three first-class SDKs to expose the
same registered Workflow loading and stable error semantics; REQ-056 requires
exact version pins; AC-030 requires invalid selectors to refuse at the loading
boundary. The selected existing contract is
`WYRD_WORKFLOW_400_INVALID_CARD_REF`, status 400, with the offending field in
`details.field`, before registry IO.

Two parts remain open:

1. `crates/shared/wyrd-semver/src/block.rs` derives `Deserialize` for
   `VersionBlock`, allowing Serde to populate its private string without calling
   `VersionBlock::parse`. A range such as `^1.0.0` can therefore inhabit the
   exact-version type. Rust Serde callers and the TypeScript native Workflow
   selector send that value through `WorkflowCards::load` to
   `/v1/cards/by-ref`; server defense-in-depth rejects it as
   `WYRD_REGISTRY_400_INVALID_CARD_SPEC`. Python correctly rejects the same
   value locally with the Workflow error and `field = version`.
2. `sdks/wyrd-sdk-ts/native/src/workflow.rs` deserializes `uid`, `space`, and
   `name` directly into domain newtypes behind one outer Serde error mapper.
   Individual invalid values receive the selected Workflow code but
   `details.field = selector`, so the public boundary loses which field failed.

The server integration refusal table at
`crates/wyrd/wyrd-server/tests/pg_workflow_registration.rs` currently proves the
first inconsistent path by constructing a ranged `VersionBlock` through Serde
and expecting the Registry error. Existing TypeScript journey assertions cover
mixed shape but not malformed individual values or exact field metadata.

## Intended correction outcome

Every public Workflow selector accepts only an exact valid `VersionBlock`.
Malformed `uid`, `space`, `name`, or `version` values refuse at the local SDK or
typed deserialization boundary with the existing Workflow error and the actual
field name. No registry request is made for those malformed inputs. Unknown,
mixed, incomplete, or otherwise invalid selector shapes continue to use
`field = selector`.

## Decision-complete recommendation

Correct the invalid state at its foundational owner: replace only
`VersionBlock`'s derived deserialization with the ordinary validated-newtype
Serde implementation that reads a string and delegates to its existing
`FromStr`/`parse`. Preserve its serialized string and schema shape and preserve
the distinct `VersionRange` and `VersionSpec` semantics. Do not add a guard in
`WorkflowCards::load`; that would leave the invalid exact-version type available
to sibling consumers and duplicate its constructor invariant.

At the existing TypeScript native Workflow selector boundary, keep the same
closed selector shape but decode its optional values as raw strings, then use
the existing `CardUid`, `SpaceName`, `CardName`, and `VersionBlock` constructors
to map value failures to `uid`, `space`, `name`, or `version`. Retain
`field = selector` for malformed JSON, unknown fields, and invalid
missing/mixed combinations. Preserve the existing
`WorkflowInvalidCardRef` catalog variant and native/public error projection.

Remove or replace the server test assertion that manufactures an invalid
`VersionBlock` and expects the Registry error. Server query parsing remains
defense in depth; no new server validation mechanism is needed.

This is standard validated-newtype Serde and foreign-runtime boundary
conversion already used elsewhere in the repository and common Rust practice.
Do not add a validator framework, dependency, parser dialect, compatibility
path, scanner, allowlist, feature flag, setting, option, file, or new check.
Any such machinery is DRIFT under the standing human direction.

## Constraints and preserved behavior

- Preserve the existing public Rust, Python, and TypeScript Workflow loading signatures and `WorkflowInvalidCardRef` wire contract.
- Preserve valid exact, prerelease, and build-metadata versions and their wire representation.
- Preserve `VersionRange` as the range owner and `VersionSpec` registration semantics.
- Preserve server-side query validation as defense in depth and all Registry errors for actual read failures.
- Preserve valid UID and exact named selection, response identity assertions, Active checks, authorization/audit, tenant isolation, lazy external reads, and exact graph hydration.
- Preserve all closed findings: provenance, registration transaction/UID fence, owner shape, runtime and SDK journeys, generated declarations, Hakari state, and patch hygiene.
- Preserve Workflow as declarative/non-principal and WyrdState as Service-rooted.

## Explicit non-goals

- No new public API, error code, selector type, transport, parser, validator framework, dependency, configuration, check, or test harness.
- No generic change to Registry, Data, or non-Workflow error ownership.
- No TASK-003/004/005 gateway, server-job, CLI, or all-route behavior.
- No compatibility alias, range acceptance for registered Workflow loading, new server guard, or duplicated consumer validation.
- No unrelated semver, SDK, registration, documentation, or formatting cleanup.

## Acceptance criteria

1. Serde construction of `VersionBlock` round-trips valid exact, prerelease,
   and build-metadata values and rejects ranges, partials, empty strings, and
   other invalid semantic versions through the existing parse invariant.
2. Rust and TypeScript cannot send a ranged or otherwise malformed
   `VersionBlock` through registered Workflow loading; the former Registry-error
   expectation is removed or replaced without weakening server defense.
3. Public TypeScript Workflow loading rejects malformed `uid`, `space`, `name`,
   and `version` locally with `WYRD_WORKFLOW_400_INVALID_CARD_REF`, status 400,
   and the exact corresponding `details.field`; invalid selector combinations
   continue to report `selector`.
4. Python's existing field-specific Workflow errors, Rust wrong-kind and
   versionless errors, valid exact/UID reads, authorization failures, inactive
   dependencies, pinning, and all three SDK journeys remain unchanged.
5. No unsupported mechanism, check, file, setting, option, dependency, or
   compatibility surface is introduced.

## Focused proof

Add one owning `wyrd-semver` unit scenario, for example
`block::tests::serde_preserves_the_exact_version_invariant`, covering valid
exact/prerelease/build round trips and invalid/range refusal. Run it exactly:

```bash
mise exec -- cargo nextest run --locked -p wyrd-semver --lib \
  -E 'test(=block::tests::serde_preserves_the_exact_version_invariant)'
```

Extend the existing TypeScript Workflow loading journey rather than creating a
new harness. Assert each malformed field's existing code/status and exact
`details.field`, plus retained mixed-selector behavior, in a way that proves
the refusal occurs without registry IO. Run its existing exact command:

```bash
mise exec -- scripts/postgres/with-test-postgres.sh -- bash -lc 'mise run db:migrate:all:inner && mise run ts:build && mise run ts:build:testing && cd sdks/wyrd-sdk-ts/wyrd && mise exec -- pnpm exec vitest run tests/integration/workflow-loading.test.ts -t "^Workflow loading workflow loading journey$"'
```

Retain the existing Rust and Python selector/journey regressions and run:

```bash
mise exec -- scripts/postgres/with-test-postgres.sh -- bash -lc 'mise run db:migrate:all:inner && mise exec -- cargo nextest run --locked -p wyrd-sdk-rust --test workflow_loading --run-ignored all -E "test(=workflow_loading_journey)"'
mise exec -- scripts/postgres/with-test-postgres.sh -- bash -lc 'mise run db:migrate:all:inner && mise run py:setup && cd sdks/wyrd-sdk-python && mise exec -- uv run python -m pytest -q -m integration tests/integration/cards/test_cards_crud.py -k test_workflow_loading_journey'
mise exec -- scripts/postgres/with-test-postgres.sh -- bash -lc 'mise run db:migrate:all:inner && mise exec -- cargo nextest run --locked -p wyrd-server --test pg_workflow_registration -E "test(=fetches_and_executes_locked_workflow_graph)"'
```

Then run the smallest complete affected lanes: `mise run test:shared`,
`mise run test:wyrd-sdk`, `mise run ts:test:unit`,
`mise run ts:test:integration`, `mise run ts:typecheck`,
`mise run ts:napi:check`, `mise run codegen:check`,
`mise run check:client-tier`, `mise run check:sdk-client-tier`,
`mise run fmt`, `mise run lints`, and explicit cumulative
`git diff --check 0569b79702218600c4f9790f45cc03100d5c6f1c <NEXT_COMMITTED_CANDIDATE>`.
Do not add a new gate or rerun an unrelated whole-repository release aggregate.

## Handoff

Route directly to `$wyrd-implement`. The next `$wyrd-task-review` must inspect
the complete original-base-to-new-candidate range, including this remediation
and every prior finding closure; it must not review only the latest fix diff.

## Implementation evidence

Owner and gap: `VersionBlock` (`crates/shared/wyrd-semver/src/block.rs`) owned
the exact-version invariant in `parse` but derived `Deserialize`. It now uses a
manual `Deserialize` that reads a `String` and calls `parse`, which is the same
pattern as the `wyrd-spec` id newtypes. The TypeScript native selector
(`sdks/wyrd-sdk-ts/native/src/workflow.rs`) now reads raw strings and builds the
selector with `CardUid::new`, `SpaceName::new`, `CardName::new`, and
`VersionBlock::parse`, in the same way as the Python `WorkflowCards.load`. The
schema output is unchanged, as `codegen:check` confirms.

| Acceptance criterion | Implementation evidence | Verification evidence | Result |
|---|---|---|---|
| 1. `VersionBlock` Serde preserves the parse invariant | `block.rs` `impl Deserialize for VersionBlock` | `block::tests::serde_preserves_the_exact_version_invariant`; `test:shared` | PASS |
| 2. Rust/TS cannot send a ranged `VersionBlock`; Registry-error row replaced | `pg_workflow_registration.rs` asserts a ranged `CardRef` does not decode; server query parsing unchanged | `fetches_and_executes_locked_workflow_graph`; `rust_journey` | PASS |
| 3. TS names `uid`/`space`/`name`/`version`; combinations report `selector` | `workflow.rs` `parse_workflow_selector`; `workflow-loading.test.ts` step 7 (outsider client, so a registry read would return 403) | TS journey; `ts:test:integration`; `ts:test:unit` | PASS |
| 4. Python/Rust behavior and the three journeys are unchanged | No change to Python or `WorkflowCards::load` | Python, Rust, and TS journeys; `test:wyrd-sdk` | PASS |
| 5. No new mechanism, check, setting, or compatibility path | Only change outside the edited files: the TS native crate now depends on workspace crate `wyrd-semver`, which the Python SDK already uses | `check:client-tier`, `check:sdk-client-tier`, `codegen:check`, `ts:napi:check`, `ts:typecheck`, `fmt`, `lints` | PASS |

Commands (all with `CARGO_TARGET_DIR=/home/thorrester/Documents/GitHub/wyrd/target`) exited 0: the five focused commands in this task, `mise run test:shared`, `test:wyrd-sdk`, `ts:test:unit`, `ts:test:integration`, `ts:typecheck`, `ts:napi:check`, `codegen:check`, `check:client-tier`, `check:sdk-client-tier`, `fmt`, `lints`, and `git diff --check 0569b79702218600c4f9790f45cc03100d5c6f1c <candidate>`. The first `codegen:check` run failed because the new struct rustdoc changed the generated schema description. I moved that prose onto the `Deserialize` impl, and the rerun passed. All non-goals stay excluded.
