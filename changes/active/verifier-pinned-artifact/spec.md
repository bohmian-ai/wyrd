---
id: SPEC-verifier-pinned-artifact
revision: 2
status: draft
---

# Verifier-pinned Artifact evidence

## Objective

A client service ingests and parses documents, verifies the parsed JSON against
its client's policy document, continues its own work, and returns. Upload the
policy once as an ordinary Artifact Card and pin its file in a Verifier version.
Each judgment then identifies the exact policy version and bytes without the
service sending the policy on every call. The policy document is evidence, not
a new Wyrd Policy contract or application runtime.

## Rechecked repository facts and gaps

Source locations below were checked against this checkout on 2026-10-17.

| Existing owner and evidence | Reuse and missing behavior |
|---|---|
| `crates/wyrd-spec/src/registry/submission.rs:49`, `:74`; `crates/shared/wyrd-loader/src/manifest.rs:28`, `:93` | `CardSubmission.artifacts` carries relative paths, base64 SHA-256, length, and optional MIME type; the loader hashes local files. The manifest hash is separately BLAKE3 (`crates/wyrd-spec/src/registry/submission.rs:24`). These are distinct hashes. |
| `crates/wyrd/wyrd-server/src/components/cards/service.rs:551`, `:723`, `:937`; `crates/wyrd/wyrd-storage/src/tenant_path.rs:9` | Registration, upload plans, and storage already own publication at `<tenant>/cards/<card_uid>/<relative_path>`. No replacement upload workflow is needed. |
| `crates/wyrd-spec/src/card/verifier/mod.rs:132`, `:303`; `crates/wyrd-spec/src/refs/mod.rs:283`, `:343` | Media slots currently have formats, size, and requiredness, but no Artifact pin. `CodeSource::Artifact { card: Ref, path }` supplies the reference precedent; the walker currently visits code Artifacts, not input slots. |
| `crates/wyrd/wyrd-server/src/components/cards/resolve.rs:142`, `:565`; `crates/wyrd-spec/src/graph/canonical.rs:16` | Registration resolves tenant-local identities and binds UIDs before deriving relationships. It needs to include pinned media references and validate their stored file inventory. It does not resolve floating versions. |
| `crates/wyrd/wyrd-server/src/verification/hosted/evidence.rs:192`, `:255`, `:335` | The shared reader authorizes/audits the Card read, selects inventory by path, checks slot format/size and deployment limits, and verifies byte length/SHA-256. It does not read `ArtifactSpec.artifact_uris` or `integrity`. Registration needs the corresponding metadata preflight. |
| `crates/wyrd/wyrd-server/src/verification/hosted/evidence.rs:79`, `:420`, `:458`; `crates/wyrd/wyrd-server/tests/integration/pg_hosted_artifacts.rs:90` | Extraction and citation validation already consume Artifact declarations; the existing hosted Artifact fixture registers files with explicit manifest MIME types. Automatic directory manifests leave MIME absent (`crates/shared/wyrd-loader/src/manifest.rs:57`); pinning must use supported stored file MIME metadata, not infer it from ArtifactSpec. |
| `crates/shared/wyrd-client/src/verify/mod.rs:314`; `crates/wyrd/wyrd-server/src/components/verification/service.rs:450`, `:585` | The shared client projects caller inputs; direct execution consumes `request.inputs`, and enqueue stages them. Neither supplies Verifier-owned slots or refuses their override today. |
| `crates/wyrd-spec/src/verification/execution.rs:81`, `:197`; `crates/vala/vala-verify/src/blocks/engine.rs:223`; `crates/wyrd/wyrd-server/src/verification/results.rs:302` | Artifact declarations have only CardRef/path. Outcomes echo the evidence map and existing result/input serialization carries it. Enrich that declaration for all Artifact evidence rather than inventing separate pinned-policy provenance. |
| `crates/wyrd-spec/src/card/verifier/validate.rs:244`, `:878`; `crates/wyrd/wyrd-server/src/components/cards/resolve.rs:39`; `crates/wyrd/wyrd-server/src/verification/runner.rs:396` | **Conflict with unconditional scheduled applicability:** samples cannot coexist with media, Window Verifiers allow only drift checks and no preparation steps, schedules require samples, and scheduled dispatch starts with an empty evidence map. This also matches the current design's Verifier/Trigger sections; see OPEN-001. |

## Scope, invariants, and boundaries

- **INV-001 — Existing publication.** The policy is an ordinary Artifact Card,
  registered and uploaded through the existing manifest/upload lifecycle before
  registering the referencing Verifier. No new Card kind or upload API.
- **INV-002 — Immutable identity.** Stored pins are exact, UID-bearing,
  tenant-local Artifact references. No `latest`, version ranges, URI fallback,
  or re-resolution to a newer version. Policy changes require a new Artifact
  version and a new Verifier version; existing results remain attributable to
  their original versions.
- **INV-003 — Existing authority.** A pin or `verifier:run` grant grants no
  Artifact read permission. Registration metadata reads and execution reads
  require the applicable Card read authority. Queued work preserves accepted
  caller/delegation authority; scheduled work uses its binding owner's accepted
  authority. SYSTEM execution adds no permission. Tenant separation and
  blocking permission checks with non-blocking audit remain unchanged.
- **INV-004 — Existing bounds and failures.** Pins count toward existing file
  and combined input limits. No increase in limits, execution deadlines, or
  retention guarantees. Missing, unauthorized, corrupt, or unreadable pinned
  evidence never silently disappears, falls back to caller evidence, or feeds
  a pass. Existing execution-error/inconclusive distinctions remain in force.

Pure declaration/schema/error contracts belong to `wyrd-spec`; registration
and authorized file access stay with the existing server owners; Vala keeps
judgment orchestration. Rust, Python, and TypeScript project the shared
`wyrd-client` behavior. There is no new service, transport, or persistence
owner. Existing Scribe result recording and durability semantics are retained.

Non-goals: Agent/Prompt attachment fields; `cards.get` returning file bytes;
reading `ArtifactSpec.artifact_uris`/`integrity`; evidence retention or historical
backfill; new extraction formats; larger size limits; general scheduling of
Run Verifiers unless separately resolved under OPEN-001.

## Requirements

**REQ-001 — Author and register a pin.** A `media` input slot may additionally
declare `artifact: { card: <Ref>, path: <relative path> }`, mirroring the
existing code Artifact source. `path` is required for this new slot field,
uses the existing relative artifact path rules, and names one file even when
the Artifact has only one. `card` must name kind `Artifact`. Other slot types
do not accept this field. Existing unpinned slots retain their behavior.

```yaml
inputs:
  parsed: { type: json }
  policy:
    type: media
    formats: [pdf]
    artifact:
      card: { kind: Artifact, name: client-policy, space: clients, version: "1.0.0" }
      path: policy.pdf
```

Registration resolves the reference through the existing resolver and checks
the completed, stored artifact inventory for that exact Card/path. Its MIME
must identify an accepted existing media format, and its length must fit
`max_bytes` when set and the deployment's individual file limit. Absent or
unsupported MIME is a format mismatch; the ordinary manifest supports explicit
`content_type: application/pdf`. Missing paths, wrong kind, malformed paths,
format/size mismatch, unavailable uploads, or unauthorized targets refuse
registration before persisting the Verifier. Validation errors identify the
slot and offending field using existing structured Wyrd errors; authorization
and tenant-not-found responses preserve existing disclosure rules.

**REQ-002 — Derive provenance from the reference.** Include
`spec.inputs.<slot>.artifact.card` in the canonical reference inventory so
resolution, hashing, stored/read contracts, and the Verifier → Artifact
relationship follow the same path as code Artifact references. Authors never
maintain a second relationship list.

**REQ-003 — Supply and protect execution evidence.** For `verify` and Run
`submit`, the service supplies only invocation-specific inputs such as
`parsed`. The server adds each pinned slot as an `InputDeclaration::Artifact`
before evidence admission and judgment; the pin is supplied even when the
slot has `required: false`. `required` does not make a configured pin optional
or overridable. All configured pins are admitted, including unused ones.

A caller supplying a pinned slot is refused even if its value equals the
pin: `WYRD_VERIFICATION_400_INPUT_INVALID`, with structured `field:
inputs.<slot>` and remediation to omit it. SDKs reject before file hashing,
upload, or execution IO; HTTP/MCP enforce the same rule server-side before
execution or enqueue staging. Queued retries use the exact accepted Verifier
pin, never a later version, and existing idempotency behavior is preserved.
Scheduled applicability has the same intended semantics, subject to OPEN-001.

**REQ-004 — Reuse evidence consumption.** Supply pins to the existing
authorized reader, `extract`, Agent workspace, native media, citation
validation, and evidence echo as ordinary Artifact declarations. Existing
references such as `inputs.policy` keep their meaning. Page citations retain
the original slot name and validated PDF page locator. The server verifies
the stored bytes against the inventory's size and SHA-256 before they are
used; registration metadata checks do not replace execution checks.

**REQ-005 — Record byte identity for all Artifact evidence.** Add `sha256`
to Artifact declarations using the existing `Sha256Hex` lowercase 64-hex
representation. Every newly produced result with resolved Artifact evidence,
whether caller-supplied or Verifier-pinned, records the server-resolved exact
CardRef (including version/UID), explicit selected path, and inventory digest
in `outcome.evidence`. The digest identifies original file bytes, not extracted
text, the Card blob, or the manifest hash. Reads must check against that
identity. Returned results, execution-status results, and retained results
preserve it through the existing serialization path.

The new digest field is optional when decoding existing requests and historical
records; absence in an old result means unrecorded, not a computed attestation.
The server fills it for new results from authoritative metadata. A supplied
digest is never trusted: mismatch with the selected inventory is refused with
a field-specific input validation error. No historical result is rewritten.

**REQ-006 — Surface parity.** The declaration, omission/override rule, digest,
and structured errors are available consistently through Rust/Python/TypeScript
SDKs and HTTP/MCP, including hydrated Verifiers and generated contracts. Keep
the existing `verify`/`submit`, execute/enqueue/status, and MCP tool contracts;
add no policy-specific invocation operation.

## Acceptance criteria and user journeys

The primary evidence is real client → real `WyrdTestServer` + repository-managed
Postgres → client journeys, per AGENTS.md §11. Provider responses may be
controlled locally; registration, uploads, authorization, execution, and result
reads must use real production paths. Integration/unit proof supports these
journeys and does not replace their negative flows.

| ID | Required observable journey and evidence |
|---|---|
| AC-001 | In each Python, Rust, and TypeScript SDK: register/upload an ordinary Artifact containing `policy.pdf` with PDF MIME; register a Verifier pinning it and bind it to the service subject; hydrate and call `verify` repeatedly with parsed JSON only. A policy-conforming judgment cites a validated policy page and returns the exact policy version/UID/path/SHA-256. Exercise extraction and an Agent workspace over the same pin. No per-call policy upload occurs. Covers REQ-001–006. |
| AC-002 | Drive equivalent HTTP and MCP workflows (MCP discover → act → observe), then `submit` → poll result. Confirm the same policy bytes, page citations, evidence identity, and retained result identity. Retries/replay preserve the selected policy and existing execution idempotency. Covers REQ-003–006. |
| AC-003 | Through all five surfaces, supply the pinned slot, including an identical Artifact declaration: get the field-specific validation refusal before execution/staging; SDK rejection causes no upload. Omit an unpinned required slot separately and retain existing missing-evidence semantics. Covers REQ-003, REQ-006. |
| AC-004 | Registration through public paths refuses a path missing from the manifest, invalid relative path, wrong-kind reference, unsupported/missing MIME, format mismatch, over-slot size, and over-deployment file size. No Verifier version or relationship is persisted on refusal. Covers REQ-001–002. |
| AC-005 | A principal allowed to invoke the Verifier but denied the Artifact read cannot use the pin, directly or queued; a foreign-tenant pin cannot resolve. No bytes or valid policy citations escape. Corrupt stored length/digest cannot produce a passing judgment. Assert structured error/terminal behavior and non-blocking audit decisions. Covers INV-003–004, REQ-004–005. |
| AC-006 | Register policy v2, then a new Verifier version pinning v2. Invoking the old Verifier still uses v1; old returned and retained results still identify v1 and its digest. The new Verifier identifies v2 and its digest. Caller-supplied, unpinned Artifact evidence also gains the digest; old digest-less records still decode. Covers INV-002, REQ-005. |
| AC-007 | After OPEN-001 is resolved, prove scheduled executions supply and authorize the same pinned declarations and record their exact identity without caller values. Retain all schedule-mode restrictions not explicitly superseded by that decision. Covers REQ-003–005. |

Existing nearby journey evidence lives at
`sdks/wyrd-sdk-rust/tests/integration/verifier_building_blocks.rs:442`,
`sdks/wyrd-sdk-python/tests/integration/test_verifier_building_blocks.py:273`,
and `sdks/wyrd-sdk-ts/wyrd/tests/integration/verifier-building-blocks.test.ts:292`;
these currently pass policy files per invocation, not via a Verifier slot pin.
Implementation evidence must include the owning format/lint checks, relevant
card/server/Vala/shared-client tests and language journeys, `codegen:check`,
served OpenAPI proof, and runtime MCP schema/behavior proof. Existing iteration
lanes include `test:cards:integration`, `test:bifrost:journey:drift`,
`py:test:integration`, `ts:test:integration`, and `test:principals:integration`
in `mise.toml`. Select final verification under AGENTS.md §11 after the write
scope is known; this draft creates no tasks and reports no executed tests.

## Open material decisions

**OPEN-001 — Resolved: pins in every activation.** Pinned slots are constants
of the exact Verifier version and are valid in verify, submit, schedule and
group_end alike. Every activation supplies and authorizes the same pinned
Artifact declarations, protects them from override and records their identity.
Scheduled applicability depends on the
[verifier-slot-sources specification](../verifier-slot-sources/spec.md), which
removes the Run/Window split and defines activation-specific slot supply and
the ordinary engine walk. Earlier schedule-mode caveats in this draft defer
to that specification; retaining drift-only checks and no preparation steps
is not the selected outcome. No open material decisions remain in this spec;
the dependent slot-sources draft and its group-source decision remain separate.

## Authority and revision history

Authorities: [AGENTS.md](../../../AGENTS.md) §§2–3, 9, 11, 14;
[agent rules](../../../architecture/agent-rules.md);
[Wyrd design](../../../architecture/wyrd-design.md) §§Doctrine, Verifier,
Trigger, Registry lifecycle, Reference forms;
[doctrine](../../../architecture/wyrd-doctrine.mdx); and the routed references
[evaluation](../../../architecture/references/domain/evaluation.md),
[spec-driven development](../../../architecture/references/languages/spec-driven-development.md),
[testing workflows](../../../architecture/references/languages/testing-workflows.md),
[errors](../../../architecture/references/languages/errors.md), and
[agent harness](../../../architecture/references/languages/agent-harness.md).
The implementation must reflect the additive media pin and digest contract in
the owning architecture/reference inventory; scheduled authority changes await
OPEN-001. This draft grants no approval or implementation authority.

| Revision | Date | Status | Change |
|---|---|---|---|
| 1 | 2026-10-17 | draft | Initial owner-directed narrow extension; all Artifact declarations gain digest provenance; scheduled conflict recorded as OPEN-001. |
| 2 | 2026-10-18 | draft | Owner resolved OPEN-001: pins are valid in every activation; scheduled applicability depends on verifier-slot-sources. Other requirements unchanged; approval outstanding. |
