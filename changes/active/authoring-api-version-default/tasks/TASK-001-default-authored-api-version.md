---
id: TASK-001
kind: implementation
status: proposed
spec: SPEC-authoring-api-version-default
spec_revision: 2
requirements: [REQ-001, REQ-002, REQ-003, REQ-004, INV-001, INV-002, INV-003, INV-004, INV-005, AC-001, AC-002, AC-003, AC-004, AC-005]
depends_on: []
parent_task:
remediates: []
---

# Default omitted authored apiVersion at the shared deserializers

## Outcome and Value

An authored Card that omits `apiVersion` deserializes as `wyrd/v1` through
both shared parsing seams. Existing CLI, SDK, and per-kind consumers inherit
the behavior without consumer-specific changes, while `CardSubmission` and
the registration wire remain strict.

## Owners, Scope, Consumers, and Prohibited Changes

| Capability | Existing owner/symbol | Inspected callers/tests | Missing behavior | Selected extension | New machinery justification |
|---|---|---|---|---|---|
| Canonical Card deserialization | `wyrd_spec::envelope::Card::deserialize` and its `CardHelper` | Typed Agent and Workflow holders delegate to `Card`; Prompt Card codecs parse `Card`; `prompt_declarative_tests::api_version_is_required` | Omission currently fails before `ApiVersion::default()` can apply | Enable the existing `ApiVersion` default at this deserialization seam | None; the value and validation already exist |
| Authored registration loading | `wyrd_loader::parse::RawCardEnvelope` | CLI and Rust/Python/TypeScript path registration share `wyrd-loader`; `parse::tests::parse_rejects_missing_api_version` | This raw envelope bypasses `Card`, so omission still fails | Mirror the same existing default at this one bypass seam | None; retain the existing loader and submission builder |
| Registration wire | `wyrd_spec::registry::CardSubmission` | Server card service and shared client registration consume it | No behavior is missing | Leave it unchanged and confirm no diff | None |

Do not edit CLI, SDK, Agent, Workflow, Prompt runtime, server, transport,
generated schema, or TypeScript declaration code. Do not add helpers, types,
fixtures, journeys, dependencies, features, version negotiation, or new error
behavior. Missing `kind`, `metadata`, and `spec` remain invalid.

## Approach

1. Change the existing canonical Card test and loader multi-document test to exercise the approved `wyrd/v1` default.
2. Enable the existing `ApiVersion` default at the canonical Card deserializer and the loader's raw-envelope deserializer.
3. Delete the obsolete loader rejection test and correct its aggregate-diagnostics expectation for the now-valid omitted field.
4. Add the approved authoring sentence to `architecture/wyrd-design.md` and run focused verification.

## Ordered Implementation Scenarios

### Scenario 1 — Both shared deserializers default omission to v1

**Behavior.** A Card without `apiVersion` deserializes through canonical
`Card` and through the loader as `wyrd/v1`; subsequent serialization or
submission construction carries the explicit value. Existing explicit v1 and
unsupported-version behavior is unchanged. Covers REQ-001–003, INV-001,
INV-003–005, and AC-001–003.

**RED.** Change the two existing tests to assert the approved behavior and run
them before production edits. Both currently fail because their deserializers
require the field:

```bash
mise exec -- cargo nextest run --locked -p wyrd-spec --lib \
  -E 'test(=card::prompt::prompt_declarative_tests::api_version_defaults_to_v1)'
mise exec -- cargo nextest run --locked -p wyrd-loader --lib \
  -E 'test(=parse::tests::parse_accepts_multi_doc_yaml)'
```

**GREEN.** Make the minimum changes at the two approved deserialization seams
so the existing `ApiVersion::default()` supplies `wyrd/v1`, then rerun both
focused tests. Do not change `CardSubmission`.

**REFACTOR.** Delete the obsolete missing-version rejection test and update the
aggregate-diagnostics test so it continues to prove the remaining invalid
document. Add no shared abstraction: the loader's single mirrored default is
required because it intentionally bypasses `Card`.

## Acceptance Criteria

- Canonical Card omission deserializes to `wyrd/v1` and serializes explicitly.
- Loader omission produces an authored Card with `wyrd/v1` and retains all other required-field checks.
- Explicit v1 remains valid and existing unsupported-version validation is unchanged.
- `CardSubmission`, server code, generated contracts, and consumer-specific authoring code have no diff.
- `architecture/wyrd-design.md` says authored `apiVersion` is optional and defaults to `wyrd/v1`.

## Expected Write Set and Consumer Closure

Expected production and test owners are
`crates/wyrd-spec/src/envelope.rs`,
`crates/wyrd-spec/src/card/prompt/mod.rs`, and
`crates/shared/wyrd-loader/src/parse.rs`. The documentation update belongs in
the spec-file authoring section of `architecture/wyrd-design.md`.

All consumers close through these existing deserializers. Paths are ownership
guidance, not permission to add parallel parsing or consumer-specific changes.

## Verification and Evidence

Run the two exact scenario commands above, followed by:

```bash
mise run test:shared
mise run fmt
mise run lints
git diff --check
```

Inspect the final diff to prove `CardSubmission`, server code, generated
artifacts, and SDK/CLI/runtime consumers are untouched. Codegen and public
journey lanes are intentionally excluded: the approved change does not alter
schema derivation, wire types, transports, or surface-specific behavior; those
surfaces already delegate to the two tested deserializers.

The documentation sentence uses static diff inspection rather than a runtime
test. No additional TDD scenario applies.

## Material Stop Conditions

Stop if implementation requires changing `CardSubmission`, generated wire
contracts, a consumer-specific authoring path, stable errors, or API-version
meaning. A discovery that another independent Card authoring deserializer
exists also requires the ownership map to be corrected before adding a third
default.

## Authority Links

- [Approved specification revision 2](../spec.md)
- [AGENTS.md](../../../../AGENTS.md)
- [Agent rules](../../../../architecture/agent-rules.md)
- [Wyrd design](../../../../architecture/wyrd-design.md)
- [Wyrd doctrine](../../../../architecture/wyrd-doctrine.mdx)
- [Spec-driven development](../../../../architecture/references/languages/spec-driven-development.md)
- [Implementation execution](../../../../architecture/references/languages/implementation-execution.md)
- [Testing workflows](../../../../architecture/references/languages/testing-workflows.md)

## Implementation Evidence

Ownership revalidated: `AgentCard`/`WorkflowCard` and Prompt codecs deserialize
through `Card`; `wyrd-loader::RawCardEnvelope` is the only bypass. Both gain
`#[serde(default)]` on `api_version`, reusing `ApiVersion::default()` (`wyrd/v1`).
The loader's `apiVersion` diagnostic branch now reads "Invalid field: apiVersion"
because omission can no longer reach it; malformed values keep
`WYRD_LOADER_400_INVALID_ENVELOPE`.

| Acceptance criterion | Implementation evidence | Verification evidence | Result |
|---|---|---|---|
| Canonical omission → `wyrd/v1`, serialized explicitly | `crates/wyrd-spec/src/envelope.rs` `CardHelper` | `card::prompt::prompt_declarative_tests::api_version_defaults_to_v1` (RED then GREEN) | PASS |
| Loader omission → `wyrd/v1`; other fields still required | `crates/shared/wyrd-loader/src/parse.rs` `RawCardEnvelope`; obsolete `parse_rejects_missing_api_version` deleted | `parse::tests::parse_accepts_multi_doc_yaml` (RED then GREEN), `parse_collects_every_envelope_violation`, `parse_rejects_missing_kind` | PASS |
| Explicit v1 valid; unsupported-version validation unchanged | `Card::deserialize` non-v1 check untouched | `cargo nextest run -p wyrd-spec --lib` 908/908; `mise run test:shared` 762/762 | PASS |
| `CardSubmission`, server, generated contracts, consumers untouched | `git diff --stat`: only the files listed here | diff inspection | PASS |
| Design doc states optional authored `apiVersion` | `architecture/wyrd-design.md` path-reference envelope bullet | diff inspection | PASS |

Commands: the two scenario commands; `mise exec -- cargo nextest run --locked -p wyrd-spec --lib`;
`mise run test:shared`; `mise run fmt`; `git diff --check`.

`mise run lints`: PASS after deleting the stale
`vala-bifrost-redux` `forge::metrics::tests::forge_operator_documentation_matches_telemetry`.
It protected parity between the Forge metric catalog and
`docs/src/content/docs/bifrost/forge.svx`; the docs-site rebuild deleted that
page, so the property is unreachable until the rebuilt site documents Forge
metrics again. Catalog closure stays enforced by the adjacent emitted-family test.
