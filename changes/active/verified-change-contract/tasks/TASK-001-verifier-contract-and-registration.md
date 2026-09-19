---
id: TASK-001
kind: implementation
status: proposed
spec: SPEC-verified-change-contract
spec_revision: 32
requirements: [REQ-045, REQ-046, REQ-047, REQ-056, REQ-090, REQ-091, REQ-092, REQ-093, REQ-094, REQ-102, REQ-103, REQ-109, REQ-110, REQ-111, REQ-113, REQ-114, REQ-116, REQ-120, REQ-143, REQ-144, INV-001, INV-006, INV-012, INV-013, INV-014, AC-004, AC-018, AC-021, AC-022]
depends_on: []
---

## Outcome and Value

Authors use one registrable `Verifier` Card with a closed Drift or Eval
implementation and attach it through `verified_by`. Registration, loading,
relationships, schemas, CLI, SDKs, and MCP agree on that model; the unshipped
Drift/Eval Card registrations, publication routing, Eval pull protocol, old
tables, and alert-router crate no longer survive as alternate paths.

## Owners, Scope, Consumers, and Prohibited Changes

`wyrd-spec` owns Card kinds, Verifier/binding/Trigger/Operator contracts,
validation, identity newtypes, errors, and generated schemas. Reuse its one
canonical reference visitor so loader resolution, UID pinning, auth scope, and
relationships cannot drift. `wyrd-loader`, `wyrd-client`, `wyrd-cards`, the
registry service, CLI, SDK bindings, MCP catalog, and the SQL Card-kind
constraint are consumers. Vala's existing Drift/Eval specs and engines remain
payload owners.

Do not add compatibility aliases, a binding Card, a Verifier DAG, a second
reference walker, speculative future implementation variants, or a TypeScript
CardKind abstraction solely for symmetry. Keep `wyrd-spec` IO-, async-, and
PyO3-free. Generated files are regenerated, never hand-edited.

## Approach

1. Replace registrable Drift/Eval with the closed Verifier contract while
   preserving the approved implementation payloads and validations.
2. Replace verification `publishes_to` fields with `verified_by` on Service,
   component occurrences, and standalone Agent; extend the canonical visitor.
3. Make composite registration resolve, authorize, UID-pin, validate, and
   derive relationships for effective Verifier, Trigger, and Operator refs.
4. Remove the unshipped old Card, route, table, CLI/client, crate, schema, and
   check surfaces rather than adapting them.
5. Regenerate public artifacts and synchronize the named architecture
   authorities to the approved 15-kind Verifier model.

## Ordered Implementation Scenarios

### Scenario 1 — Strict Verifier envelopes replace Drift and Eval Cards

**Behavior.** Real Drift and Eval YAML register as `kind: Verifier` with
exactly one `implementation.kind`. Old Card kinds, future kinds, unknown
fields, invalid Drift method/signal/profile pairs including SPC+Metric, and
secret-bearing payloads fail before persistence.

**RED.** Extend contract and loader coverage with accepted Verifier YAML and
each refusal. The current kind catalog accepts Drift/Eval and has no Verifier,
so the accepted and rejected cases fail in opposite directions.

**GREEN.** Add the minimum closed contract, reuse `DriftSpec` and `EvalSpec`,
remove the approved Drift variants/fields, and update the Card-kind persistence
constraint through a new migration.

**REFACTOR.** Delete obsolete kind-specific branches once every consumer is
exhaustive over the new catalog; do not retain adapters.

### Scenario 2 — One binding shape resolves every supported authoring form

**Behavior.** Service-, component-, and standalone-Agent `verified_by` accepts
path/CardRef Verifiers and path/CardRef/inline Triggers and Operators. The
server UID-pins all effective refs, traverses nested refs, derives relationships,
and rejects unresolved, wrong-kind, unauthorized, cross-tenant, duplicate
Verifier, duplicate Operator, and Workflow-on-failure cases atomically.

**RED.** Add real composite-registration cases for the three binding locations
and all reference forms. They currently fail because only `publishes_to`
verification routing exists.

**GREEN.** Extend the existing reference inventory and composite registration
pipeline; use the same effective spec for inline and referenced definitions.

**REFACTOR.** Remove publication-only verification validators and duplicate
walkers after the canonical visitor proves complete.

### Scenario 3 — Retired paths are unreachable

**Behavior.** Registration, CLI loaders, shared client state, Python and
TypeScript projections, and MCP expose Verifier only. `/v1/eval/runs`, old
client/CLI pull mechanics, `vala.eval.runs`, `vala.eval.assertions`,
`vala.drift_alerts`, and `vala-core::alert_router` have no production or build
references. Registration alone activates no work.

**RED.** Add regression assertions that old registrations/routes/table
catalog entries are refused or absent and Verifier consumers work. Existing
surfaces expose the retired paths.

**GREEN.** Delete the obsolete surfaces and update consumers to the one new
contract. Preserve reusable local Eval engine behavior without its server pull
protocol.

**REFACTOR.** Delete the alert-router-only schema check with its retired
property; retain shared security types that still have consumers.

### Scenario 4 — Public artifacts and authorities describe one model

**Behavior.** OpenAPI, JSON schemas, generated language types, `AGENTS.md`,
Wyrd design/doctrine/security, Bifrost design, and vocabulary references expose
15 native Card kinds with Verifier, `verified_by`, internal SYSTEM identity,
and no competing Drift/Eval registration story.

**RED.** Run generated-contract and stale-authority checks; current artifacts
advertise the old model.

**GREEN.** Change source generators and authoritative prose, regenerate, and
remove only checks whose guarded retired surface is now unreachable.

**REFACTOR.** Keep implementation detail in the change architecture links;
the permanent authorities state only lasting contracts.

## Acceptance Criteria

- `AC-004`, `AC-018`, `AC-021`, and the retirement portion of `AC-022` pass.
- The canonical visitor covers every new reference slot and no old routing
  field remains as an alias.
- Only Drift and Eval appear as Verifier implementations; neither is a Card
  kind.
- Existing Drift/Eval engines remain reusable and no future executor is added.

## Expected Write Set and Consumer Closure

Likely owners include `crates/wyrd-spec/src/{envelope,card,refs,graph,error}.rs`,
the loader, registry/card service, `wyrd-cards`, shared client state, SDK/CLI/MCP
projections, `wyrd-sql` Card-kind migration/tests, generated contract sources,
workspace membership/checks, and the architecture authorities named by
`REQ-114`. Paths guide discovery; they are not a private allowlist.

## Verification and Evidence

Run the new focused tests by their exact names after they exist. Confirmed
regression commands include:

```bash
mise exec -- cargo nextest run --locked -p wyrd-spec --lib -E 'test(=refs::completeness_tests::visitor_covers_every_slot_in_the_migration_table)'
mise exec -- cargo nextest run --locked -p wyrd-spec --lib -E 'test(=graph::composition::tests::publication_validation_rejects_duplicate_targets)'
mise exec -- cargo nextest run --locked -p wyrd-loader --lib -E 'test(=validate::tests::validate_rejects_duplicate_component_publication_targets)'
mise exec -- cargo nextest run --locked -p wyrd-cli --test cli -E 'test(=loader::end_to_end::load_end_to_end_reference_tree)'
mise run test:shared
mise run test:cards:unit
mise run test:cards:integration
mise run test:cli:journey
mise run test:wyrdstate:journey
mise run test:wyrd
mise run test:sql
mise run codegen:check
mise run check:client-tier
mise run check:pyo3-scope
mise run fmt
mise run lints
git diff --check
```

Add Python/TypeScript unit, typing, and integration lanes when their public
projections change.

## Material Stop Conditions

Stop for specification authority if implementation requires a compatibility
Card/route, another Verifier implementation, a different binding identity or
reference model, a new permission, or changes to approved Drift/Eval semantics.

## Authority Links

- `changes/active/verified-change-contract/spec.md`
- `changes/active/verified-change-contract/architecture/verification-control-flow.html`
- `architecture/wyrd-design.md`
- `architecture/wyrd-doctrine.mdx`
- `architecture/wyrd-security-posture.md`
- `architecture/references/languages/spec-driven-development.md`
- `AGENTS.md`

