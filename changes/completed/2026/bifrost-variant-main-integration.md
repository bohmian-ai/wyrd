# Integrate Variant Bifrost with the Wyrd product

- Change ID: `SPEC-bifrost-variant-main-integration`, approved revision 5.
- Completed: 2026-10-14.
- Final reviewed target: not supplied; no final-change-review PASS is claimed.
- Completion target: `06a89286943a64c83eb588562b308c3d35663d66`.
- Delivery reference: integration merge `46dc7e41b`; release reference not supplied.
- Completion authority: human-approved closeout extending the explicit completion override on 2026-10-14.

## Intent and integrated behavior

The rewritten Bifrost is integrated with Wyrd's verification, security,
observation, audit, gateway, first-class clients and agent surfaces, so callers
can emit, retain and query attributable evidence through one product.
Variant source `cfe21dc9007d5d22d9ae4411be07fb5504b0cc05` is the Bifrost
implementation authority; main base `8a8813a7458b7c98707c7b4f9b9d403f604ee0e6`
supplies the surrounding product behavior.

The integration preserves approved exceptions through source-owned seams:
Card-UID observation attribution, one nonblocking Scribe outbox for audit and
internal results/captures, Scribe routing independent of Oracle, role-owned
heartbeats and one membership poller, ten built-in tables, ordered typed query
parameters, verified-principal entrances and SQL capabilities, no-wait Oracle
graph release, and typed verification evidence. Forge table operations use the
active task claim as their one table authority, including manifest work and
terminal settlement.

## Lasting decisions and constraints

Revision 3 makes the Variant source authoritative inside Bifrost scope and
retains only approved main exceptions, adapted to source owners. Older
concurrent-planning and reader-pin mechanisms are not retained merely because
they appeared in earlier packets. Nothing had shipped: Bifrost DDL is
fresh-only, with no implicit retained-data deletion or compatibility migration.
Non-Bifrost identity and product ownership remain independent.

Revision 4 incorporates the single active Forge-task table claim. Revision 5
consolidates storage on the installed `object_store` client family for S3,
GCS, Azure and local storage, retaining Bifrost's policy/governor wrapper and
artifact storage's own handle responsibilities. Backend-neutral multipart
contracts replace S3-specific names and GCS resumable uploads; GCS XML
multipart includes server completion, abort and sweeper cleanup. Presigning,
HEAD verification, encryption checks, replay re-mint and operation metrics
remain required. The old cloud SDK/OpenDAL dependencies are removed.

Pinned forks use the common `wyrd-production` branch while manifests retain
immutable revision pins. Source capacity workloads keep their entrypoints and
report shapes inside the consolidated capacity build. Benchmark parity and
integrated acceptance are separate from a successful compile.

## Acceptance evidence and limits

| Obligation family | Retained evidence |
|---|---|
| Source-authoritative integration and product exceptions | Approved source/main pins, per-file dispositions, owner mappings and task records document the integration choices. Historical research is not a competing contract or a final acceptance verdict. |
| Storage consolidation and wire closure | Implemented storage task records passing handle/dispatcher tests, dependency-tree cleanup, codegen, formatting, lint and dependency-boundary checks. |
| Backend behavior | Recorded RustFS, Azurite and GCS single-PUT emulator journeys passed. Recorded real-GCS qualification passed multipart, abort, re-mint, signing and end-to-end cases. Azure user-delegation qualification is not established by account-key emulator tests. |
| Benchmarks and consumer completeness | Source workloads are retained with the one capacity build; recorded merged query reports and profiling context exist. Final geometry, complete parity runs and an integrated acceptance gate were deferred to change review and are not asserted here. |

GCS emulators do not implement XML multipart; real GCS remains the proof lane
for that capability. Recorded task passes do not imply all 27 reconciled
obligations or every historical suspicious integration hunk received a final
independent PASS. The owner accepts closeout without rerunning or inventing
that final evidence. Current architecture is authoritative for runtime
behavior; this record is historical integration context, not a release claim.

## Current authority, owners and tests

- [Bifrost design](../../../architecture/bifrost-design.md), [Wyrd design](../../../architecture/wyrd-design.md), [security posture](../../../architecture/wyrd-security-posture.md), and [Variant history](bifrost-variant.md).
- [Bifrost storage](../../../crates/vala/vala-bifrost-redux/src/storage/mod.rs), [shared cloud storage implementation](../../../crates/wyrd/wyrd-storage/src/cloud.rs), and [storage contract](../../../crates/wyrd-spec/src/storage).
- [Server outbox](../../../crates/wyrd/wyrd-server/src/scribe_outbox.rs), [Forge](../../../crates/vala/vala-bifrost-redux/src/forge/mod.rs), and [shared Bifrost client](../../../crates/shared/wyrd-client/src/bifrost/mod.rs).
- [Bifrost journey tree](../../../crates/wyrd/wyrd-testing/tests/bifrost), [storage owner tests](../../../crates/wyrd/wyrd-storage/tests), and [verification/benchmark lanes](../../../mise.toml).
