# TASK-002 R4 focused follow-up: Workflow selector validation

**Result: RESOLVED.** The discovery conflict is source-resolvable. Commit
`375d97e67f3affe0d5c59727ef3135b22a459140` establishes the selected Workflow
error for the cases it handles, but it does not close all of prior
`FIND-TASK-002-11`. The behavior and invariant proposals identify two reachable
parts of the same public selector obligation: TypeScript loses individual-field
attribution, and `VersionBlock` deserialization admits invalid concrete versions
that Rust and TypeScript send to the registry. The reports that marked this
boundary closed did not trace those producers through the actual read path.

## Immutable subject and path inspected

- Base: `0569b79702218600c4f9790f45cc03100d5c6f1c`
- Candidate/HEAD before and after inspection:
  `2ff7bbfa57737ce8a7a69287e2fa462c313b1a8c`
- Authority: Revision 12 `spec.md` REQ-054, REQ-056, INV-007 and AC-030;
  `TASK-002-cleanup.md`; prior R3 validated `FIND-TASK-002-11`; R2/R3
  remediation tasks.
- Error contract: `crates/wyrd-spec/src/error.rs:3114-3130`.
- Concrete-version type and deserialization:
  `crates/shared/wyrd-semver/src/block.rs:12-70`.
- Comparable existing validated-newtype mechanism:
  `crates/wyrd-spec/src/ids.rs:9-64`.
- Shared Workflow selector/read path:
  `crates/shared/wyrd-client/src/workflow.rs:148-190`,
  `crates/shared/wyrd-client/src/cards/hydrate/graph.rs:324-385`, and
  `crates/shared/wyrd-client/src/cards/reads/get.rs:39-85`.
- Python boundary: `sdks/wyrd-sdk-python/src/state/mod.rs:2570-2638`.
- TypeScript native and public boundaries:
  `sdks/wyrd-sdk-ts/native/src/workflow.rs:53-99`,
  `sdks/wyrd-sdk-ts/native/src/lib.rs:147-179`, and
  `sdks/wyrd-sdk-ts/wyrd/src/index.ts:1195-1243`.
- Server defense and reachable regression fixture:
  `crates/wyrd/wyrd-server/src/components/cards/routes.rs:116-157,654-675`
  and `crates/wyrd/wyrd-server/tests/pg_workflow_registration.rs:790-813`.
- Owning journey negatives:
  `sdks/wyrd-sdk-python/tests/integration/cards/test_cards_crud.py:640-653`,
  `sdks/wyrd-sdk-ts/wyrd/tests/integration/workflow-loading.test.ts:186-203`,
  and `sdks/wyrd-sdk-rust/tests/workflow_loading.rs:266-297`.

The repository has no `.codegraph/` directory, so source and callers were
traced directly. Per the assignment, no build or test command was run.

## Resolving evidence

### 1. Python rejects malformed selector fields locally with the selected contract

`PyWorkflowCards::load` converts `uid`, `space`, `name`, and `version` using the
existing domain constructors. Every conversion maps to
`WorkflowInvalidCardRef`, status 400, and the exact field in `details.field`;
mixed or incomplete shapes use `field = "selector"`
(`state/mod.rs:2608-2633`). Only after that match succeeds does line 2636 call
the shared async loader. A range such as `^1.0.0` fails
`VersionBlock::parse` at lines 2624-2626 and therefore performs no registry IO.
This part of the domain-review claim is correct.

### 2. TypeScript locally rejects malformed identifiers but misattributes them

`WorkflowSelectorJson` types `uid`, `space`, and `name` as validated domain
newtypes. Their custom Serde implementations reject invalid values, but the
single `serde_json::from_str` error is mapped by one closure whose details are
always `{ "field": "selector" }` (`native/src/workflow.rs:76-84`). Thus malformed
UID, space, or name values receive the correct code and status before IO, but
not the selected field-specific metadata. Unknown, mixed, and incomplete shapes
properly belong to `selector`; individual value failures do not. The native
error projection preserves that details object (`native/src/lib.rs:167-179`),
so the public `WorkflowCards.load` cannot recover the lost field. The TypeScript
journey asserts only the mixed-selector code and does not exercise malformed
individual fields or details (`workflow-loading.test.ts:186-195`). The behavior
proposal is therefore confirmed.

### 3. Rust and TypeScript admit invalid concrete versions and perform registry IO

`VersionBlock::parse` accepts only `semver::Version::parse` values and documents
the type as a concrete semver version (`block.rs:12-43`). Unlike Wyrd's identifier
newtypes, however, `VersionBlock` derives `Deserialize` on its private `String`
field (`block.rs:13-15`). Derived newtype deserialization writes the inner string
without calling `parse`, so strings such as `^1.0.0` can inhabit a type whose
public constructor rejects them.

This invalid state is reachable in both disputed paths:

- TypeScript first accepts `version` as `String`, then constructs `CardRef`
  through `serde_json::from_value` (`native/src/workflow.rs:65-95`); that
  deserialization accepts the invalid `VersionBlock`.
- Rust exposes Serde-deserializable `CardRef` and can pass the resulting value
  through public `CardSelector::exact`. The server integration fixture does
  exactly that via its Serde-based `card_ref` helper, then calls
  `workflows.load` with `^1.0.0` and expects the registry error
  (`pg_workflow_registration.rs:790-813`).

`WorkflowCards::load` checks only a missing version and wrong kind
(`wyrd-client/src/workflow.rs:167-186`). It therefore enters
`CardGraphHydrator`, and `reads::get_response` sends the invalid version on
`/v1/cards/by-ref` (`reads/get.rs:67-85`). The server reparses the query using
the valid `FromStr` path and returns `WYRD_REGISTRY_400_INVALID_CARD_SPEC`
(`routes.rs:662-672`). This is authenticated registry IO and a different stable
error owner from Python's local `WYRD_WORKFLOW_400_INVALID_CARD_REF`. The R3
implementation evidence also records this exact behavior as an open observation.
The invariant proposal is therefore confirmed.

### 4. Relationship between the proposals and the prior finding

These are not competing diagnoses. Both arise in the public TypeScript
selector conversion, while invalid Rust construction additionally exposes a
foundational newtype invariant hole. They belong in one retained
`FIND-TASK-002-11` correction because they violate the same already-selected
outcome: malformed Workflow selector fields are refused before registry IO with
the Workflow code and the offending field. They do not justify a new stable
finding ID.

The candidate correctly closes versionless and wrong-kind Rust selectors,
mixed/incomplete TypeScript and Python shapes, and Python malformed field
conversion. It does not close TypeScript field attribution or invalid-version
deserialization. Accordingly, the domain reports' broader statement that all
malformed SDK selectors use typed validation before network IO is too broad.

## Proposed retained finding

### FUP-R4-001 / prior FIND-TASK-002-11 — INCORRECT: malformed Workflow selector validation remains inconsistent

- **Violated obligation:** Revision 12 REQ-054/INV-007 require the same public
  Workflow loading and stable error semantics; REQ-056 requires exact version
  pins; AC-030 requires wrong selectors to refuse at the loading boundary. The
  previously validated finding selected
  `WYRD_WORKFLOW_400_INVALID_CARD_REF`, status 400, field-specific details, and
  no registry read for malformed fields.
- **Exact locations:**
  `crates/shared/wyrd-semver/src/block.rs:13-43` and
  `sdks/wyrd-sdk-ts/native/src/workflow.rs:53-95`, with the reachable IO sink at
  `crates/shared/wyrd-client/src/cards/reads/get.rs:67-85`.
- **Observable consequence:** malformed UID/space/name inputs in TypeScript say
  only `field = selector`; malformed versions in Rust/TypeScript can issue a
  registry request and return `WYRD_REGISTRY_400_INVALID_CARD_SPEC`, while
  Python returns the selected Workflow error locally with `field = version`.
- **Smallest standard correction boundary:** restore the `VersionBlock`
  private-newtype invariant by deserializing through its existing
  `FromStr`/`parse` implementation, matching the repository's ordinary custom
  Serde pattern for validated identifier newtypes. At the existing TypeScript
  selector boundary, deserialize the closed shape as raw optional strings and
  use the existing `CardUid`, `SpaceName`, `CardName`, and `VersionBlock`
  constructors so each conversion maps to its own field; retain `selector` for
  malformed JSON, unknown fields, and invalid field combinations. This reuses
  established mechanisms and requires no validator framework, dependency,
  scanner, allowlist, compatibility option, setting, file, or new check.
- **Focused closure proof:** extend the existing TypeScript Workflow loading
  test with malformed `uid`, `space`, `name`, and `version` inputs, asserting
  code, status, title/remediation, exact `details.field`, and no request; retain
  the mixed-shape case. Add/extend the owning `VersionBlock` Serde unit proof
  for valid exact/pre-release/build round trips and invalid/range refusal, then
  update the existing Rust/TypeScript range negative so it no longer reaches
  the registry. Preserve server-side parsing as defense in depth and preserve
  all valid Cards read behavior.

No new product, public API, architecture, security, concurrency, resource
ownership, or persistent-data decision is needed. The correction uses the
standard validated-newtype and explicit boundary-conversion patterns already
present in the repository and common Rust/Serde practice; introducing parallel
validation machinery would be DRIFT under the human standing direction.

## Follow-up disposition

**RESOLVED.** Retain and consolidate both source-backed proposals under prior
`FIND-TASK-002-11`. The candidate closes only part of that finding; the
field-attribution and invalid-version/no-IO portions remain reachable and
required. No additional proposal was discovered.
