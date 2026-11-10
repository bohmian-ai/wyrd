# Identity and publication boundary review

Result: **FAIL** — two material proposals require independent validation.

## Subject and scope

Reviewed the immutable R3 subject recorded in `subject.md`: immediate base
`a387f2a47a145a867c8a4948966fcbd136af9961`, candidate
`37b3c2f96b1bd425bf1f0923a1950d0363c40907`, approved spec revision 4 and
`TASK-001-R3-canonical-authoring-grammar.md`. HEAD remained the candidate;
tracked source was not changed. Original task and prior security acceptance
were available as context. No other round-03 report or implementation summary
was used as evidence.

This review follows raw identity/reference values and source descriptors through
field captures, Graph, loader decoding/ordering, declaration argument dispatch,
sole Artifact publication, and existing registry binding. It does not audit
unrelated persistence, security, or executor internals. CodeGraph is absent.

Authorities read: root AGENTS; spec-driven development; maintainer style;
architecture authority map; Wyrd design's in-memory authoring,
pre-declaration/reference/registry sections; security posture's principal and
tenant trust boundaries. REQ-002/004/005/006/008 and AC-001/002/003/007/008
govern this boundary. No narrower AGENTS applies to changed production paths.

## Coverage and acceptance evidence

| Obligation | Implementation evidence | Verification evidence / limits | Result |
|---|---|---|---|
| Deferred values retain current captures and by-value identity | `Body::set/set_list/set_map/push/insert`, per-field captures, `FieldValue` conversions; ordinary `Graph::absorb` recursively visits Card captures before comparing identity and content. | Read local edit/snapshot/replacement tests, including continued ownership of a shared Artifact and conflicting Trigger definitions. No independent execution. | PASS for ordinary Cards; Artifact exception below |
| Exact external intent remains external; siblings remain explicit | Authoring CardRef converts to `{kind,name,version}`; ordinary CardDefinition converts to `{sibling: ...}`. Native CardRef drops UID as the prior authored identity contract did. Loader's canonical graph only treats explicit siblings as graph edges. | File parity and registered-reference journeys retain the comparisons; shared graph `build_ignores_external_refs` confirms its intentional identity-independent behavior. | PASS for the exercised reference paths |
| Complete validation precedes publication and uses canonical rules | `Card::decode` calls `parse_card_value`; loader `parse_value` calls existing `parse_single_envelope`, which calls `Spec::from_kind_and_value`. `declaration_input_from_cards` retains canonical validation and ordering. | Local malformed-field tests and parity journey cover ordinary Cards. Local Artifacts skip part of this boundary. | FAIL — IDPUB-001 |
| Every named Card can validate and reach the unified declaration surface | Named-kind and Verifier conversion macros adapt to `Declarable`; `Cards::declare` dispatches Input/Path/Card to existing owners. Artifact uses separate conversions and misses root adaptation. | Public inventory uses a reconstructed generic CardDefinition for Artifact, rather than the Artifact value itself (`integration/authoring.rs:1418`). | FAIL — IDPUB-002 |
| Publication preserves sole submissions, source maps, scratch and retained progress | `Cards::declare` validates first, iterates LocalArtifact values using `self.upload_artifact`, then calls `self.submit`. `upload_artifact` retains the `ArtifactUpload` object through awaited submission; its private scratch owns extraction files. Each upload has its own input/source map; saga preparation checks source/manifest one-to-one and hashes sources. | Read the Artifact journey's unchanged rerun, registered no-upload, source failure, drift, old-version selection and retained-publication assertions. These do not exercise nested local Artifact references or cross-partition conflicts. | PASS for existing simple publication; IDPUB-001 covers omitted cases |
| Existing native/path/state lifecycle and tenant authority remain intact | Native Input goes directly to `submit`; paths use unchanged loader; all branches use the same Cards engine. `WyrdState::declare_with` selects one client, declares with its Cards handle, then hands the same client to unchanged `pull_declared`. Python/TypeScript existing native/path consumers remain on those owners. | Existing path-state journey and read-only denial assertions remain. Static review confirms no alternate endpoint or tenant selector. | PASS |
| Existing server identity/permission attribution remains authoritative | Saga `submit_card_declaration` still sends authenticated `POST /v1/cards`. `declare_card_http` authorizes card write and applicable operator invocation. `service::declare_card` still validates/plans, calls `resolve_external`, and writes through existing DeclarationWriter. `resolve_external` obtains `registry_tenant_conn(caller.data_tenant_id)`; `EffectiveSpecs::resolve` uses canonical reference inventory and tenant-scoped lookup. | Prior source-backed security acceptance carries forward only for unchanged owners; fresh tracing confirms new dispatch reaches them. No new two-tenant or permission test run by this reviewer. | PASS |

Changed authoring representation, kind/Verifier producers, loader seam,
Cards/state adaptation, architecture changes, finance example and relevant
local/public/journey consumers were inspected. The main publication proof is
`local_artifacts_reuse_and_keep_versions`; registry/file/client proof is
`declarations_match_files_and_keep_refusals`. Source inspection also covered
ArtifactUpload preparation, canonical reference inventory, graph edge/root
selection, saga preparation/submission/upload and server external binding.

## IDPUB-001 — Local Artifacts are excluded from complete graph capture and validation

Classification: **INCORRECT**. Violated obligations: REQ-004 complete graph
validation through shared owners, REQ-008 captured references and active identity
conflicts, AC-002 refusal before publication, AC-007 supported Artifact fields.

Locations: `authoring/mod.rs:913–968` (`Graph::declaration/absorb`),
`authoring/kinds.rs:280–301` (`Artifact::local`) and `:318`
(`Artifact::schema_ref`).

The local Artifact branch stores an entire Artifact in `captures.artifacts`.
Graph then queues it without visiting its spec captures. Only ordinary Card
captures recurse (`mod.rs:959`). `Artifact::local` decodes and calls
`ArtifactSpec::validate`, whose complete body checks only nonblank
`artifact_kind`; only `graph.cards` enter `declaration_input_from_cards`.

Two concrete public paths expose this omitted boundary:

1. `schema = Artifact::from_path("schema", "1.0.0", schema_path)`;
   `payload = Artifact::from_path("payload", "1.0.0", payload_path).schema_ref(schema)`;
   `Audit::new("release-evidence", "1.0.0").evidence_refs([payload])`.
   The schema is captured by the field setter, but the snapshot contains only
   the payload LocalArtifact. The payload's canonical schema_ref is an external
   exact Artifact reference. Nothing publishes the captured schema. Declaration
   subsequently fails at the existing server's unresolved-external dependency
   check, even though both definitions were supplied and no new upload mechanism
   is needed: each can use the existing sole Artifact publication owner in
   dependency order. The same omission suppresses invalid/conflicting definitions
   and local publications reachable only through that field.
2. A local payload with `.schema_ref("./missing-schema.yaml")` is accepted by
   `Card::decode` as a canonical Ref::Path. Its parent's local `validate()` and
   `declaration()` pass because ArtifactSpec::validate does not inspect refs.
   The shared loader's `check_resolved_references` would refuse this exact value
   at `spec.schema_ref`. The invalid ref instead reaches server preflight during
   the upload. If another valid Artifact preceded it, that Artifact can already
   be published when a locally detectable invalid definition finally refuses.
   This is a local admission gap, not an authorization bypass: the server still
   refuses the bad ref.

The separate identity lists also never compare local Artifacts with ordinary
Artifact definitions: an Audit evidence_refs field can hold a local payload and
`Artifact::new` of the same identity with a different artifact_kind. Both remain
active, but snapshot validation does not diagnose the conflict. The local value
publishes first and the later graph receives remote spec drift. This shares the
omitted complete-graph admission boundary; it is not an additional finding.

Materiality: Artifact.schema_ref and complete graph capture are expressly
supported, including cases outside the finance story. The omitted child may be
another local Artifact with real schema bytes; this is not an invented Card kind
or a need for general automatic Model/Data uploads. Local validation success is
currently untrustworthy for this documented reference field, and publication can
start before a required local refusal.

Smallest correction boundary: make the existing authored Graph account for local
Artifacts' current captures and identities, and pass their canonical authored
refs through the existing loader validation boundary before the publication
loop. Keep local Artifact requests separate, external references external,
server-owned UID resolution, per-upload source/scratch ownership and approved
retained-publication semantics. Do not add a second reference validator or a new
registry workflow. A local Card dependency which requires a genuinely different
publication contract must be resolved against approved authority rather than
silently dropped or used to justify a new workflow.

Focused proof: snapshot a payload whose schema is another local Artifact and
assert both planned publications in dependency order, once each; replace the
schema field and assert the old capture disappears. Assert an unresolved path
and a cross-partition conflicting Artifact identity refuse before publication,
through the canonical diagnostic/duplicate families. An existing focused Artifact
journey can confirm the nested schema actually registers with exact attribution;
no new harness or broad suite is required.

## IDPUB-002 — Artifact is missing the root Card declaration/validation adaptation

Classification: **MISSING**. Violated obligations: REQ-001 all twelve named
kinds; public support table's validate() on every named Card; REQ-006 declare
accepts authored Card forms; AC-007/008 native public surface.

Locations: `authoring/kinds.rs:215–339` (Artifact definition and conversions),
`authoring/mod.rs:789–797` (generated validate), `:934–936`
(unconditional containing-graph conversion), and `cards/handle.rs:1115–1178`
(`Declarable` adapters).

All ordinary named kinds receive CardDefinition and Declarable conversions from
`card_kind!`; Verifier has its own equivalents. Artifact invokes only
`card_methods!` and implements From<Artifact> for FieldValue. There is no
From<Artifact> or From<&Artifact> for Declarable/CardDefinition. Therefore the
documented `cards.declare(&Artifact::new(...))` and local Artifact equivalents
cannot compile. Reconstructing a generic definition from `.spec()` loses the
local source descriptor and captured dependencies, so the public test's fallback
is not equivalent coverage.

Independently, a valid local `Artifact::from_path(...).validate()` necessarily
fails: generated validate converts it to FieldValue, which has only
`captures.artifacts`; Graph creates zero `authored` ordinary Cards and then
unconditionally calls `declaration_input_from_cards(&[])`. `order_cards` returns
"Cannot order an empty card tree". This happens even with a valid existing file
and metadata; construction of a parent is currently needed merely to validate
the Artifact. A source-less Artifact value validates as a sibling, demonstrating
that the missing behavior is specifically the root adaptation, not the canonical
Artifact decoder.

Materiality: this is an explicit named-kind/public operation, with no unusual
value or failure timing needed. Users cannot declare the named Artifact type at
all or independently validate a local Artifact. The public inventory's generic
reconstruction masks both gaps.

Smallest correction boundary: complete Artifact's adaptation at the existing
authoring/declaration owner and make valid Artifact-only snapshots usable for
validation/publication. Reuse sole `upload_artifact` publication and the ordinary
Artifact receipt; do not invent a containing Card, YAML bridge, or alternate
upload/saga. Preserve by-value source capture. The exact private representation
is implementation-owned.

Focused proof: public compile/use coverage must pass an Artifact value directly
to `Cards::declare`; validate a valid local Artifact without a fabricated parent
and check invalid metadata/spec refusal. A focused real Artifact declaration can
reuse the existing server/fixture and assert an Artifact root receipt. No new
transport or fault-injection harness is justified.

## Limits and rejected expansion

No Cargo, external network calls, source mutations, or runtime probes were run;
the orchestrator owns sequential verification. These are source-proven proposals,
not claims of independently reproduced failing tests. No credentials, tenancy,
wire, migration or durable lifecycle change was found in the new dispatch.

`Artifact::new(...).file(...)` also appears to serialize external intent while
capturing a sibling, but a newly declared descriptor has no uploaded file
manifest. Reachability alone does not establish a useful supported fixed-file
journey, so no separate blocker is proposed for that case.

Skipped unrelated server persistence/security internals and broad checks. The
material risk is false local validation and incomplete Artifact publication,
while the existing server authorization and tenant binding remain in force.
