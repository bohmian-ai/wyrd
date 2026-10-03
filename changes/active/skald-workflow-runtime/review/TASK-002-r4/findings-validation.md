# Independent structured Ponytail validation — TASK-002 R4

Validation is complete. One bounded portion of prior stable
`FIND-TASK-002-11` remains open. The final ledger contains no new finding ID.
The generated/type/boundary proof proposal is rejected because candidate-bound
R4 verification supplies every cited lane.

## Immutable subject and authority

- Base: `0569b79702218600c4f9790f45cc03100d5c6f1c`
- Candidate: `2ff7bbfa57737ce8a7a69287e2fa462c313b1a8c`
- Candidate/`HEAD` identity was rechecked before and after source inspection.
- Approved specification: `changes/active/skald-workflow-runtime/spec.md`,
  Revision 12
- Original task: `changes/active/skald-workflow-runtime/tasks/TASK-002-cleanup.md`
- Remediation inputs: R2 and R3 tasks, every prior verdict and validation
  ledger, and the cumulative candidate including
  `375d97e67f3affe0d5c59727ef3135b22a459140`

Applied `AGENTS.md`, `architecture/agent-rules.md`, the applicable design,
doctrine, spec-driven-development, maintainer-style, error, Rust, TypeScript,
and testing authority, plus the human standing direction against unsupported
bespoke machinery. The repository has no `.codegraph/` directory, so existing
repository navigation and direct source/caller tracing were used.

This validation read the complete cumulative diff and every R4 discovery,
domain, comparison, follow-up, subject, and verification report. It then
independently inspected the cited producers, full consumer bodies, sibling
version/reference owners, public SDK projections, server parsing boundary, and
tests. No implementation was edited and no command was rerun by this
validator; `verification.md` is candidate-bound execution evidence produced by
the orchestrator.

## Proposal-by-proposal validation

| Proposal or conflicting closure claim | Disposition | Source-backed resolution |
|---|---|---|
| `BEH-R4-001`: TypeScript maps malformed individual selector fields to `details.field = "selector"` | **CONFIRMED**, consolidated into prior `FIND-TASK-002-11` | `WorkflowSelectorJson` deserializes `uid`, `space`, and `name` directly into validated newtypes, while the one `serde_json::from_str` failure mapper at `sdks/wyrd-sdk-ts/native/src/workflow.rs:76-84` always emits `selector`. The public projection preserves those details. Invalid individual values are reachable through `WorkflowCards.load`, and the existing TS journey checks only a mixed shape and only its code. |
| `INV-R4-001`: derived `VersionBlock` deserialization admits a range, allowing Rust/TypeScript registry IO | **CONFIRMED**, consolidated into prior `FIND-TASK-002-11` | `VersionBlock` is the exact-pin type, its public constructor uses `semver::Version::parse`, and `VersionRange` separately owns read-side ranges. Nevertheless, derived `Deserialize` writes the private string directly. The new TS selector path deserializes its raw version through `CardRef`, then `WorkflowCards::load` checks only presence/kind and `reads::get_response` sends the invalid value. The server test at `pg_workflow_registration.rs:790-813` proves the path is reachable and currently pins the later registry error. |
| `FUP-R4-001`: both selector defects are parts of prior `FIND-TASK-002-11` | **REVISED** and retained | The follow-up correctly combines the defects under the existing stable ID and correction owner. Its recommendation is narrowed below so proof stays with existing owners and does not add a validator, harness, or duplicated catalog assertion. |
| Domain/maintainer/system closure claim that malformed selectors all fail before registry IO | **REJECTED for malformed exact versions only** | Python calls `VersionBlock::parse` and refuses `^1.0.0` locally. Rust Serde and the TS `CardRef` conversion can create the invalid `VersionBlock`; the shared read then performs authenticated IO and returns `WYRD_REGISTRY_400_INVALID_CARD_SPEC`. The broader graph, runtime, security, durability, recovery, and maintainability conclusions remain valid. |
| `REPO-R4-01`: final codegen, typing, N-API, client-tier, SDK-tier, and PyO3 gates are absent | **REJECTED** | R4 `verification.md` records PASS on the immutable candidate for `codegen:check`, `py:typecheck`, `ts:typecheck`, `ts:napi:check`, `check:client-tier`, `check:sdk-client-tier`, and `check:pyo3-scope`, as well as the transaction-coupling check. Missing evidence in the earlier R3 implementation record is no longer a candidate completion gap. No rerun, new gate, check, setting, or allowlist is warranted. |
| Empty registry-durability, tenancy/security, workflow-runtime, maintainer, and resilience ledgers | **VALIDATED except for the selector qualification above** | Exact dependency UIDs, lifecycle locks, transaction/audit ownership, provenance, lazy client construction, blocking-pool ownership, language projections, and recovery retain their established owners. No separate finding or remediation is supported. |

Every discovery proposal is accounted for. Agreement among reviewers was not
treated as proof, and a PASS report did not override the reachable selector
path. Conversely, the standards review's earlier evidence gap is not retained
after the exact required lanes passed on the reviewed candidate.

## Producer-to-consumer validation

### Exact-version invariant

`crates/shared/wyrd-semver/src/block.rs:13-43` defines `VersionBlock` as a
private-string exact semantic version. `parse`, `FromStr`, `semver`,
`is_pin`, `sort_versions`, and downstream `expect("VersionBlock invariant")`
calls all rely on that invariant. `VersionRange` is the distinct installed
type for `^`, `~`, wildcard, and comparator syntax, while `VersionSpec` already
uses ordinary custom Serde parsing for its pin/scope wire contract. There is no
approved use in which a range inside `VersionBlock` is valid.

The derived `Deserialize` implementation is the lone producer that bypasses
the constructor. This defect predates the base commit, but the candidate's new
public TypeScript Workflow selector routes a user-provided string through it:

1. `WorkflowCards.load` serializes the public selector for the native call.
2. `parse_workflow_selector` reads `version` as a raw string.
3. It builds JSON and deserializes that JSON into `CardRef`.
4. `CardRef.version: VersionBlock` accepts `^1.0.0` without `parse`.
5. `WorkflowCards::load` sees a present Workflow version and enters the graph
   hydrator.
6. `cards/reads/get.rs:67-85` sends the invalid version to
   `/v1/cards/by-ref`.
7. The server's ordinary query parser reparses it correctly and returns
   `WYRD_REGISTRY_400_INVALID_CARD_SPEC`.

The exact server integration refusal row constructs the same invalid value via
Serde and observes that registry error. Python instead calls
`VersionBlock::parse` at its boundary and returns
`WYRD_WORKFLOW_400_INVALID_CARD_REF`, `details.field = "version"`, before IO.
Rust callers can reach the invalid state through the public Serde contract of
`CardRef`, even though normal typed construction cannot do so.

Adding another guard to `WorkflowCards::load` would leave an invalid public
newtype available to every sibling consumer and duplicate its constructor.
Validated deserialization at the newtype producer is the smallest standard
correction and matches Wyrd's identifier newtypes and `VersionSpec`. It is also
normal Serde practice; it needs no new dependency or mechanism.

### TypeScript field ownership

`sdks/wyrd-sdk-ts/native/src/workflow.rs:53-99` uses a closed
`WorkflowSelectorJson`, which appropriately rejects unknown and mixed shapes.
However, its `uid`, `space`, and `name` fields deserialize directly into domain
newtypes before the shape match. Any value error is therefore collapsed by the
single outer mapper to `details.field = "selector"`. The same collapse applies
to the currently invalid version only after the nested `CardRef` conversion.
The native/public error projection copies `details` unchanged, so no later
consumer can recover the offending field.

The boundary already owns conversion from untyped JavaScript strings. Keeping
the closed raw shape and invoking the existing `CardUid`, `SpaceName`,
`CardName`, and `VersionBlock` constructors there is the direct correction.
`selector` remains appropriate for malformed JSON, unknown fields, and invalid
field combinations; value failures name their actual field. This is a normal
foreign-runtime boundary conversion, not a second validator.

## Final deduplicated stable ledger

### FIND-TASK-002-11 — REVISED — INCORRECT: malformed Workflow selector validation is still inconsistent in Rust and TypeScript

**Discovery sources.** `BEH-R4-001`, `INV-R4-001`, and `FUP-R4-001`.

**Violated obligation.** Revision 12 REQ-054 and INV-007 require the three
first-class SDKs to project the same registered Workflow loading and stable
error semantics; REQ-056 requires exact version pins; AC-030 requires wrong
selectors to be refused at the loading boundary. Prior validated
`FIND-TASK-002-11` selected `WYRD_WORKFLOW_400_INVALID_CARD_REF`, status 400,
and field-specific details for malformed Workflow selector fields, before
registry IO. Commit `375d97e67` implements that outcome for Python and for the
shared wrong-kind/versionless checks, but not for all TypeScript field errors
or a Serde-created invalid exact version.

**Exact locations.** The invalid-state producer is
`crates/shared/wyrd-semver/src/block.rs:13-43`. The TypeScript field collapse
and nested `CardRef` conversion are
`sdks/wyrd-sdk-ts/native/src/workflow.rs:53-99`. The shared consumer is
`crates/shared/wyrd-client/src/workflow.rs:167-190`, and the IO sink is
`crates/shared/wyrd-client/src/cards/reads/get.rs:67-85`. The reachable
incorrect-registry-error assertion is
`crates/wyrd/wyrd-server/tests/pg_workflow_registration.rs:790-813`.

**Evidence and consequence.** A TypeScript selector with an invalid UID,
space, or name fails locally with the selected code but reports the whole
`selector`, unlike Python's exact field. A range such as `^1.0.0` bypasses
`VersionBlock::parse` through derived Serde in Rust and TypeScript, performs a
registry read, and returns `WYRD_REGISTRY_400_INVALID_CARD_SPEC`; Python
returns the Workflow-owned error locally with `field = "version"`. Callers and
agents therefore receive inconsistent stable error ownership and remediation,
and malformed input crosses an IO boundary that the selected contract says it
does not cross.

**Decision-complete minimum correction.** Preserve the existing public
selector types, `WorkflowInvalidCardRef` catalog variant, Cards transport,
server query validation, and all non-selector read errors.

1. At the source of the invalid state, replace only `VersionBlock`'s derived
   `Deserialize` with the ordinary validated-newtype implementation that reads
   a string and delegates to its existing `FromStr`/`parse`. Keep
   serialization and schema shape unchanged. Do not add a Workflow consumer
   guard or alter `VersionRange`/`VersionSpec` semantics.
2. At the existing TypeScript native boundary, deserialize the same closed
   selector shape as optional raw strings and convert each present value using
   the existing domain constructor, mapping failures to `uid`, `space`,
   `name`, or `version`. Keep `selector` for malformed JSON, unknown fields,
   and missing/mixed combinations.
3. Remove or replace the server journey assertion that manufactures an
   invalid `VersionBlock` through Serde and expects the registry error. Server
   query parsing remains defense in depth; no new server check is required.

This correction reuses established repository and ecosystem mechanisms. Do
not add a validator framework, schema package, compatibility path, scanner,
allowlist, feature flag, option, setting, new check, or second error catalog.
Any such addition would be DRIFT under the human standing direction.

**Focused closure proof.** In the owning `wyrd-semver` tests, prove that
Serde round-trips valid exact, prerelease, and build versions and rejects a
range/invalid value. Through the existing public TypeScript Workflow test
surface, cover malformed `uid`, `space`, `name`, and `version`, plus the mixed
shape, and assert the existing Workflow code/status and exact `details.field`;
exercise the malformed cases without a usable registry connection so the
result itself proves local refusal. Retain the existing Rust wrong-kind and
versionless assertions and Python malformed-field journey. Run the existing
focused language journeys plus the ordinary codegen/type/N-API lanes affected
by the source change. Do not introduce a new harness or duplicate catalog
title/remediation assertions already generated and checked by the existing
error machinery.

## Prior-finding closure

| Stable finding | Validated R4 disposition |
|---|---|
| FIND-TASK-002-1 | Closed: sibling and registered provenance remain separate through both client hydration and server effective-body resolution. |
| FIND-TASK-002-2 | Closed: the actual bundle and stored graph contain exact Workflow-to-Agent and Agent-to-Prompt references. |
| FIND-TASK-002-3 | Superseded and closed under approved Revision 12; removed keyed normalization was not recreated. |
| FIND-TASK-002-4 | Closed: preflight UID, exact identity, and Active status are rechecked and locked through the write transaction. |
| FIND-TASK-002-5 and -6 | Closed at their documentation/import owners. |
| FIND-TASK-002-7 | Closed: all three owning language journeys inspect the complete exact spec-reference and relationship graph without adding a production API. |
| FIND-TASK-002-8 | Closed: TypeScript uses the recursive JSON input and complete native run snapshot contract. |
| FIND-TASK-002-9 | Closed: hydrator, `EffectiveSpecs`, and `RegistrationWriter` remain cohesive existing owners rather than duplicate orchestration layers. |
| FIND-TASK-002-10 | Closed: graph-only holders do not replace original version intent in hashing, replay, or allocation. |
| FIND-TASK-002-11 | **Partially open and retained above.** The included human-selected commit closes the owning error for Python, wrong-kind, versionless, and selector-shape cases, but not TypeScript value attribution or invalid-version Serde/IO. |
| FIND-TASK-002-12 | Closed: generic Python registry selectors use request Validation while genuine Data validation and Workflow-specific selectors retain their owners. |
| FIND-TASK-002-13 | Closed through Tokio's installed blocking pool with no custom runtime or cancellation machinery. |
| FIND-TASK-002-14 | Closed by sanctioned Hakari regeneration; the existing workspace-hack gate passes. |
| FIND-TASK-002-15 | Closed: explicit cumulative diff hygiene passes. |

## Human-direction and verification assessment

The cumulative candidate otherwise uses established repository owners and
ordinary ecosystem mechanisms: Serde/domain newtypes, Tokio `spawn_blocking`,
PostgreSQL transactions and row locks, existing Cards transport and
authorization, generated declarations, Hakari, and normal owning-language
tests. No unsupported check, file, setting, option, parser dialect, retry
layer, cache, compatibility shim, or framework warrants another DRIFT finding.

Candidate-bound R4 verification credibly closes the standards evidence
proposal and the previously repaired generated-state/hygiene findings. Green
journeys do not close `FIND-TASK-002-11`, because the current server test
asserts the inconsistent range behavior and no public TypeScript test checks
individual field details or pre-IO range refusal.

Final retained ledger: **FIND-TASK-002-11** only.
