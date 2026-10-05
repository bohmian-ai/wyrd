---
id: TASK-004-R1
kind: remediation
status: ready
spec: SPEC-forge-concurrent-planning
spec_revision: 11
parent_task: TASK-004
remediates: [FIND-TASK-004-1, FIND-TASK-004-2]
---

# Restore compaction contract parity across first-class SDKs

## Contract and candidates

- Approved spec: `changes/active/forge-concurrent-planning/spec.md`
- Original task: `changes/active/forge-concurrent-planning/tasks/TASK-004-compaction-defaults-and-type.md`
- Reviewed candidate/base: `7ac45dec99535c881b7a936c66623044f15d8823` / `c1508b375`

## Diagnosis

The canonical enum serializes `small_files` and `files_with_delete`, contrary
to revision 11's hyphenated public values, and canonical Rust/schema prose says
omission defaults to `full` while runtime correctly defaults to `small-files`.
Generated projections multiply the contradiction. The Rust SDK also exposes
`TableConfig` methods that accept `CompactionTypeWire` without re-exporting the
type; its claimed journey bypasses the SDK and imports `wyrd_spec` directly.

## Intended correction outcome

One canonical contract exposes exactly the approved hyphenated values, reports
omission as `small-files`, and is nameable through Rust, Python, and TypeScript
first-class SDKs without implementation-layer dependencies.

## Decision-complete recommendation

Correct serialization and default documentation at `wyrd-spec`, propagate via
the established source owners and generators, and regenerate schemas/stubs/
declarations. Do not add aliases for the unshipped underscore wire values.
Re-export the existing enum from `wyrd_client::bifrost` so the thin Rust SDK
projects it; add neither a wrapper enum nor a direct SDK dependency on
`wyrd-spec`.

## Preserved behavior and non-goals

- Preserve internal Iceberg property spelling, runtime `SmallFiles` default,
  per-table target calculation, conflict semantics, and copy-on-write `full`.
- Do not hand-edit generated artifacts or introduce compatibility aliases.

## Acceptance criteria

| Finding | Acceptance criterion |
|---|---|
| `FIND-TASK-004-1` | All public contracts accept/report hyphenated values, reject underscore spellings, and document omission as `small-files`. |
| `FIND-TASK-004-2` | Rust SDK users and journeys name the enum only through the public SDK/Bifrost surface. |

## Focused proof and broader verification

Add schema assertions and Rust/Python/TypeScript public registration coverage
for both hyphenated values and omitted default. Run codegen check, public SDK
typing/unit/journey lanes, Bifrost verification, format, lints, and diff check.

