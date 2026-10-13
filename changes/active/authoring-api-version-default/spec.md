---
id: SPEC-authoring-api-version-default
revision: 1
status: draft
---

# Default `apiVersion` when authors omit it

## Objective and user value

Card authors stop writing `apiVersion: wyrd/v1` on every Card. Wyrd has one
Card API version, so the field repeats a fact the tooling already knows. The
canonical envelope, wire contract, and stored Cards stay explicit, so this is
an authoring convenience, not a loosening of the contract.

Commit `583a1d399` made `apiVersion` required everywhere, including authored
YAML, by removing `#[serde(default)]` from the canonical `Card` envelope. This
change restores the authoring convenience at the authoring boundary only and
keeps the canonical envelope strict.

## Requirements

- **REQ-001** A Card authored as YAML or JSON and read by a Wyrd client
  authoring path is accepted without `apiVersion` and treated as `wyrd/v1`.
  Authoring paths include, at minimum: the CLI (`wyrd plan`, `wyrd apply`),
  `Cards` registration from a path in the Rust, Python, and TypeScript SDKs,
  composite path references followed by the loader, and the Python SDK
  constructors that load an authored Card file or text (`Agent.from_yaml`,
  `Workflow.from_yaml`, `PromptCard.from_path`, and equivalents that read an
  author-written envelope).
- **REQ-002** An omitted `apiVersion` always means `wyrd/v1`. It never resolves
  to the newest supported version, a configured default, or a server-chosen
  value.
- **REQ-003** An authored `apiVersion` that is present and is not `wyrd/v1` is
  refused with the existing stable error for an invalid envelope, before any
  network call.
- **REQ-004** Every Card that leaves the client carries an explicit
  `apiVersion: wyrd/v1`. The wire `CardSubmission`, HTTP and gRPC request
  bodies, and server responses are unchanged.
- **REQ-005** Content identity does not depend on whether the author wrote
  `apiVersion`. The same Card with and without an authored `apiVersion: wyrd/v1`
  produces the same spec hash and the same registration outcome
  (deduplicated, not a new version or a `WYRD_REGISTRY_409_SPEC_DRIFT`).
- **REQ-006** Documentation and generated authoring guidance describe
  `apiVersion` as optional for authored Cards, defaulting to `wyrd/v1`, and
  required on the wire.

## Invariants

- **INV-001** The canonical `wyrd_spec::envelope::Card` and the wire
  `CardSubmission` keep `apiVersion` required. The server-side and wire JSON
  Schemas continue to list it as required.
- **INV-002** The server does not infer `apiVersion`. A raw HTTP or gRPC
  submission without it is refused as it is today.
- **INV-003** A missing `kind`, `metadata`, or `spec` is still refused.
  Defaulting applies to `apiVersion` only.
- **INV-004** Durable behavior stays server-owned; the default is a pure
  client-side authoring normalization with no IO.

## Scope and non-goals

In scope: client authoring paths in the shared loader and SDKs, their tests,
user-journey coverage for an omitted `apiVersion`, and the documentation and
generated artifacts that describe authored Cards.

Not in scope:

- making `apiVersion` optional on the wire, in server handlers, or in stored
  Cards;
- introducing a second API version or any version negotiation;
- defaulting any other envelope field;
- removing `apiVersion` from existing fixtures, examples, or docs. Existing
  files stay valid; examples may omit it once this ships.

## Expensive-to-reverse decisions

- **D-001 Boundary.** The default lives at the client authoring boundary, not
  on the canonical envelope. Restoring `#[serde(default)]` on `Card` is
  rejected because it would make the wire contract and every deserializer of
  stored Cards lenient.
- **D-002 Pinned meaning.** Omitted means `wyrd/v1` permanently. If a future
  `wyrd/v2` ships, files without `apiVersion` keep their v1 meaning, and v2
  authors must write the field. This keeps every file written today stable
  across that future change.
- **D-003 No wire change.** Generated wire schemas, OpenAPI, and the error
  catalog are unchanged, so no client in any language needs regeneration to
  stay compatible.

## Acceptance criteria and evidence

- **AC-001** `wyrd plan` and `wyrd apply` accept a composite whose Cards omit
  `apiVersion`, and the registered Cards read back with `apiVersion: wyrd/v1`.
  Evidence: a CLI user-journey test against `WyrdTestServer`.
- **AC-002** Registration from a path succeeds with an omitted `apiVersion` in
  each first-class SDK (Rust, Python, TypeScript). Evidence: one user-journey
  test per SDK, or an extension of each SDK's existing registration journey.
- **AC-003** Registering the same Card once with and once without an authored
  `apiVersion` deduplicates. Evidence: a journey or integration test asserting
  the second registration creates no new version.
- **AC-004** An authored `apiVersion: wyrd/v2` is refused locally with the
  invalid-envelope error and makes no network call. Evidence: a loader unit
  test plus one SDK-surface negative test.
- **AC-005** A raw wire submission without `apiVersion` is still refused.
  Evidence: the existing `api_version_is_required` envelope test stays and
  passes, plus a server integration check if one does not already exist.
- **AC-006** The Python authoring constructors in REQ-001 accept an omitted
  `apiVersion`. Evidence: Python unit tests through public `wyrd` modules.
- **AC-007** `mise run codegen:check` passes with no wire-schema change, and
  the docs that describe authored Cards say the field is optional.

## Open material decisions

None.

## Authority

- `architecture/wyrd-design.md` (Card envelope; composite path references
  require a full card envelope, line ~1870, which this revision narrows to
  "`apiVersion` optional")
- `architecture/wyrd-doctrine.mdx`
- `AGENTS.md` §2 (Card envelope), §9 (server owns durable behavior)
- Commit `583a1d399` (made `apiVersion` required at every layer)

## Revision history

| Revision | Status | Change |
| --- | --- | --- |
| 1 | draft | Initial specification. |
