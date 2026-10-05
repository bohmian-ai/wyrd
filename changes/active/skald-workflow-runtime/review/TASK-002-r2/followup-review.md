# Focused registration version-intent follow-up

**RESOLVED** — the newly exposed path supports the proposed
`REG-DUR-R2-001` regression. This is discovery evidence for independent
validation, not a task verdict.

Base: `0569b79702218600c4f9790f45cc03100d5c6f1c`.
Candidate: `e7d16b5bd622b9a564a49edb18239df7f209ca92`, confirmed unchanged.
Scope: omitted/scoped Workflow root submission through public registration,
graph projection, declarative preflight and durable version resolution. The
other discovery reports' general registration PASS claims do not contradict
this specific path: their pinned-root proof does not select either intent.

## Authority reconciliation

`architecture/wyrd-design.md:1730–1737` explicitly preserves `None`, `Scope`
and `Pin` as distinct authored registration intents and assigns their
resolution to the server. The contract itself agrees:
`crates/wyrd-spec/src/envelope.rs:149–177` permits omitted and scoped versions
before registration, and requires a pin after registration;
`Metadata::resolved_pin` at line 216 deliberately returns `None` for both
pre-registration intents. These are not malformed or unsupported payloads.

Revision 12 REQ-028 (`spec.md:1887`) and cleanup Scenario 2 preserve the
existing composite registration path and add declarative resolved validation.
REQ-056's exact loading and REQ-029's exactly versioned server execution govern
registered reads/execution. Neither changes authored registration into a
pinned-only operation. Exact dependency identities are a separate requirement:
an omitted/scoped **root** with inline bodies or pinned sibling dependencies
needs no version-range reference resolution.

The prior review's provenance and UID findings require validating the exact
dependency bodies eventually persisted. They do not require allocating the
Workflow root's durable version in preflight. A temporary root version used
only for declarative validation does not choose an external dependency or
change the body being validated.

## Producer-to-sink evidence

| Stage | Source and observed behavior |
|---|---|
| Public input | `components/cards/routes.rs:408–451`, `register_card_http`, accepts typed `CreateCardRequest`, checks the existing permissions/idempotency header, and forwards the body to `service::register_card`. There is no root pin requirement. |
| Request validation | `service.rs:567–585,1427–1466` hashes the authored request and validates typed specs/bindings/artifacts. `validate_request` does not reject omitted/scoped root metadata. |
| Pure graph projection | `service.rs:1012–1022` calls `graph_ready_submissions` only inside `plan_registration_graph`. `wyrd-spec/src/graph/mod.rs:86–119` clones submissions, replaces non-pin metadata with graph-only `0.0.0`, and leaves authored submissions unchanged. A singleton Workflow has one node/root; `graph/composition.rs:93–94` imposes no Service-root condition on it. |
| Preflight input | `service.rs:581,996–1006` passes the **original** submissions to `resolve_card_references`. Graph-projected metadata does not flow into this call. |
| Dependency validation | `resolve.rs:46–85,192–222,565–635` accepts a root with inline Agent/Prompt bodies: no external identity or sibling target is needed. A scoped/omitted root is simply absent from the pinned sibling cache, which is correct because it is not a dependency target. |
| New failure | `resolve.rs:430,509–523` clones the authored metadata into an envelope and immediately calls `WorkflowCard::from_envelope`. `wyrd-spec/src/card/workflow.rs:1001–1010` requires `resolved_pin()` and returns `Workflow Card envelope missing resolved version pin` for both intents. This happens before Skald body validation or durable writes. |
| Existing durable owner | `service.rs:1093–1104,1142–1192,1355–1423` selects the original authored submission, binds exact dependencies, locks the version line and calls `resolve_existing`. Its non-pin branch delegates to `wyrd-sql/src/queries/cards/version_resolve.rs:89–159`: omitted versions seed/bump/deduplicate absolute latest; scoped versions seed/bump/deduplicate inside the prefix line. The new refusal prevents both branches from being reached. |

A concrete reachable case is the existing valid one-step Workflow schema with
an inline Agent containing an inline native Prompt, explicit space/name, and
no `metadata.version`; changing only that root field to `"1"` exercises the
second case. No path-loaded target, external registry fixture, credential
resolution or provider dispatch is required to establish either failure.
Alternatively, the existing checked-in bundle retains all six dependency pins
while changing only the Workflow root's version intent.

## Proposed finding

**FOLLOWUP-R2-001 / REG-DUR-R2-001 — REGRESSION: authored Workflow root version
intents are rejected before server-owned version resolution.**

Changed location: `crates/wyrd/wyrd-server/src/components/cards/resolve.rs:430`.
Violates the three registration intents in the active design, REQ-028 and
cleanup Scenario 2's preservation of existing composite registration.
Observable result: fresh omitted/scoped Workflow registration returns a
Workflow-validation error instead of a server-resolved exact Card identity.
All clients entering the shared public route are affected. Replaying a
previously committed request can still return its stored response because
replay precedes preflight; that does not make fresh registration work.

Smallest correction boundary: keep declarative Workflow preflight on the
existing `EffectiveSpecs` owner, and reuse the existing graph-only metadata
projection for its transient root holder before the typed-envelope conversion.
Retain original submissions for request hashing, replay, sibling provenance
and persistence. Do not replace authored metadata in the request, run version
allocation outside its locked write transaction, change exact dependency
identities, or weaken the public/registered `from_envelope` invariant.

`submission_card` in `resolve.rs` has only this preflight caller. Its namesake
in `service.rs` serves persistence and must continue preserving authored
metadata. Projecting all preflight submissions into the sibling cache would
be the wrong correction: a placeholder must not turn an unpinned submission
into a real exact sibling target.

## Sibling consumers and preservation

Read the complete changed server resolution/service bodies and cumulative
diff, Skald body hydration/validation and its caller, graph projection/build/
root/composition owners, metadata/version contracts, SQL version resolver,
route, relevant client hydration and current registration proof. The affected
helper's single caller was checked against all `from_envelope` consumers.

`WorkflowBodies::authored` and `CardGraphHydrator::load_workflow`
(`wyrd-client/src/cards/hydrate/workflow.rs:47–75,193–215`) also use the typed
holder; registered loading must retain exact root identity. The existing
`WyrdState` holder consumer (`state.rs:1290`) is a separate Service-root
hydration path. No general relaxation of `WorkflowCard::from_envelope` is
needed for this registration correction. Authored local loading semantics are
outside this focused finding.

`EffectiveSpecs::{validate_bindings,validate_baselines,load,body}` retain
provenance-separated submitted/registry bodies. Its original pinned sibling
cache and external UID list should stay unchanged. Skald's
`validate_card_bodies` and `from_card_with_agent_resolver` validate spec and
resolved bindings without registry IO or durable allocation; the root version
is retained only as runtime metadata in this discarded validation holder.
Write-time expected-UID recheck, row locks, auditing, transaction atomicity,
dependency binding and Workflow's non-principal behavior need no alteration.

## Focused closure proof and limits

Extend the existing `registers_only_valid_explicit_workflow_graphs` journey in
`pg_workflow_registration` with omitted and scoped Workflow roots while leaving
dependencies inline or pinned. Assert the returned server-resolved exact
version (fresh omitted seed `0.1.0`, fresh `"1"` scope seed `1.0.0`) and reload
the registered Workflow. Include an invalid resolved binding under these
intents and assert no registration operation, Card or relationship is
persisted; preserve the existing standalone authorization audit on refusal.
Keep existing provenance, stale UID and lifecycle-lock regression assertions.

Run the existing exact command with the managed environment:

```bash
mise exec -- scripts/postgres/with-test-postgres.sh -- bash -lc 'mise run db:migrate:all:inner && mise exec -- cargo nextest run --locked -p wyrd-server --test pg_workflow_registration -E "test(=registers_only_valid_explicit_workflow_graphs)"'
```

No test execution or runtime reproduction is claimed in this follow-up. The
current journey's pinned-root fixture (`pg_workflow_registration.rs:370–391`)
cannot exercise the two deterministic `resolved_pin()` failures. No expensive
lane was started; no source, test, generated artifact or other report was
edited. No new product, public API, persistence or concurrency decision is
required to restore the existing server-owned registration intents.
