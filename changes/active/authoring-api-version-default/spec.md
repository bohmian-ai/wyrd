---
id: SPEC-authoring-api-version-default
revision: 2
status: approved
---

# Default omitted Card `apiVersion` to `wyrd/v1`

## Objective and user value

Card authors may omit `apiVersion` from YAML or JSON. Wyrd interprets the
omission as `wyrd/v1` at the two shared Card-deserialization seams, so every
existing authoring consumer receives the behavior without surface-specific
changes.

## Requirements

- **REQ-001** Deserializing a Card without `apiVersion` through the canonical
  `wyrd-spec` Card deserializer produces a Card whose API version is
  `wyrd/v1`.
- **REQ-002** The shared loader applies the same default when it reads its raw
  authored envelope, because that envelope builds `CardSubmission` values
  without deserializing through `Card`.
- **REQ-003** Serializing either result includes the explicit
  `apiVersion: wyrd/v1` value.
- **REQ-004** Documentation describes `apiVersion` as optional in authored
  Card files and defaulting to `wyrd/v1`.

## Invariants

- **INV-001** The default is owned by Card deserialization in `wyrd-spec` and
  mirrored only by the shared loader's raw authored-envelope deserializer.
  CLI, SDK, and per-kind authoring entry points do not implement their own
  defaults.
- **INV-002** `CardSubmission` remains strict: raw HTTP and gRPC registration
  requests must carry an explicit `apiVersion`, and generated wire schemas and
  client types continue to require it.
- **INV-003** An explicit unsupported API version continues to be refused by
  the existing validation path. This change adds no new local-before-network
  guarantee or error contract.
- **INV-004** Missing `kind`, `metadata`, or `spec` remains invalid. No other
  envelope field gains a default.
- **INV-005** The meaning of omission is permanently pinned to `wyrd/v1`; it
  is not configuration, negotiation, or “latest version” selection.

## Scope and non-goals

In scope: the canonical Card deserializer, the loader's one raw-envelope
deserializer, the existing Card default regression test, removal or correction
of the obsolete loader rejection test, and one authoring-doc sentence.

Not in scope:

- edits to CLI, Rust SDK, Python SDK, TypeScript SDK, Agent, Workflow, Prompt,
  or other consumer-specific authoring entry points;
- changes to `CardSubmission`, server request handling, generated wire
  schemas, OpenAPI, TypeScript declarations, or stable errors;
- version negotiation, a second API version, or defaults for other fields;
- new cross-surface journeys, fixtures, or duplicate content-identity tests.

## Expensive-to-reverse decisions

- **D-001 Shared deserialization boundary.** Optional authored `apiVersion` is
  canonical Card-deserialization behavior in `wyrd-spec`. The shared loader
  mirrors it only because its raw envelope intentionally bypasses `Card` while
  retaining artifact declarations. Consumers inherit the behavior from those
  two seams.
- **D-002 Strict registration wire.** `CardSubmission` does not default the
  field. Server registration and generated wire contracts remain explicit.
- **D-003 Pinned meaning.** Omission always means `wyrd/v1`, including after a
  future API version exists.

## Acceptance criteria and evidence

- **AC-001** The existing canonical Card test demonstrates that omitting
  `apiVersion` deserializes as `wyrd/v1` and serializes explicitly.
- **AC-002** The loader no longer rejects an otherwise valid authored envelope
  solely because `apiVersion` is absent; its obsolete missing-version rejection
  test is removed or changed to reflect the default.
- **AC-003** Existing explicit-version inputs remain valid, and existing
  unsupported-version validation remains unchanged.
- **AC-004** `CardSubmission` source and generated wire contracts are
  unchanged.
- **AC-005** `architecture/wyrd-design.md` states that authored files may omit
  `apiVersion`, which defaults to `wyrd/v1`.

## Open material decisions

None.

## Authority

- `architecture/wyrd-design.md` — Card envelope and spec-file authoring.
- `architecture/wyrd-doctrine.mdx` — shared Card contract and first-class SDK
  projection.
- `AGENTS.md` §2 and §9 — `wyrd-spec` contract ownership and explicit wire
  contracts.
- Commit `583a1d399` — current strict-deserialization behavior being narrowed.

## Revision history

| Revision | Status | Change |
| --- | --- | --- |
| 1 | superseded | Placed the default at every client authoring boundary and required duplicated cross-surface proof. |
| 2 | approved | Moves the default to the two shared deserialization seams and removes consumer-specific work and redundant acceptance obligations; explicitly approved by Steven on 2026-10-13. |
