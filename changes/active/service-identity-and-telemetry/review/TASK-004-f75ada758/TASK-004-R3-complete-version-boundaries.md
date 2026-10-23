---
id: TASK-004-R3
kind: remediation
status: ready
spec: SPEC-service-identity-and-telemetry
spec_revision: 13
parent_task: TASK-004
remediates: [FIND-TASK-004-1, FIND-TASK-004-8, FIND-TASK-004-9, FIND-TASK-004-10, FIND-TASK-004-11, FIND-TASK-004-12, FIND-TASK-004-13, FIND-TASK-004-14, FIND-TASK-004-15]
requirements: [REQ-035, REQ-036, REQ-037, REQ-038, REQ-042, REQ-044, AC-008, AC-012, AC-013, AC-016, AC-018, AC-021, AC-022, AC-023]
depends_on: []
---

# Complete the existing table-version boundaries

Implement through `$wyrd-implement`. Keep R2's one positive TableVersion and retained native definitions; complete the nine source-validated gaps below at their existing owners. No new specification revision, public async API or retry subsystem is needed. Prior findings 2–7 remain closed and their behavior must be preserved. Batch the small documentation correction with this work.

## Subject, authority and constraints

- Root: `/home/thorrester/Documents/GitHub/wyrd-worktrees/service-identity-and-telemetry-task-004`.
- Original cumulative base: `3e7c59db7a0f5e9506273eab826875cc8b59f226`.
- Prior candidate: `c4dd61029a54c44017c63bd4ff469472c406bc13`.
- R3 reviewed candidate: `f75ada758827266ac0b25a05d6a7b530f86d845a`.
- Approved authority: [revision 13](../../spec.md) and [storage supplement](../../supplements/table-storage-and-correlation.md), [original task](../../tasks/TASK-004-shared-table-storage.md), [R2](../TASK-004-c4dd61029/TASK-004-R2-single-table-version.md), this [verdict](verdict.md) and [independently validated ledger](findings-validation.md). R1's copied-recipe remedy is superseded.
- Root/nested `AGENTS.md`, owning Bifrost/security/recovery authority, Rust/error/SDK guides and spec-driven execution apply. Use existing owners and mise commands. Do not alter unrelated task drafts or uncommitted work.

Preserve current throughput constraints: no added accepted-ID read or transaction on successful fresh ingest, no additional full-payload copy/hash pass there, no per-row version metadata or new global lock. Existing bounded mirror, grouped fence, CPU lanes, catalog and language boundary owners suffice. Keep strict payload/source-schema/order/count conflicts, authorization, tenant isolation/RLS, managed origin/time, checksums, canonical WAL coordinates, fences, pins, uncertain-result reconciliation, cancellation and resource ownership.

No benchmarks are requested now. Throughput remains explicitly unmeasured until the separately authorized matched comparison under revision 13; do not substitute SQL statement counts for performance evidence or weaken the existing 5% rule. Do not add speculative guards, a second cache/ledger/service, copied recipes, a configuration counter, public PATCH, cache broadcast, automatic history rewrite, release migration/backfill, retention TTL or tuning. A synchronous setter is allowed to require an already-described/registered binding; do not invent network IO, a row queue or a new async signature to resolve an unpromised convenience.

## Diagnoses and selected corrections

### FIND-TASK-004-1 — Recover accepted canonical input under its original definition

**Gap.** Gate creates a deterministic canonical batch ID but sends `table_version: None` (`gate/mod.rs:944–975`). `scribe/ingress.rs:232–272,577–665` selects current and prepares before the mirror. The mirror and SQL fence require the prepared version to match; neither returns the original binding for this path. Accept an OTLP export at version 1, commit a supported Bloom-only edit, and resend identical content: warm and cold retries fail at version 2. Replay suppresses the refused provisional attempt but does not supply the required live acknowledgement. This is the remaining original FIND-1 cause; native retries retaining their explicit original claim already work.

**Selected correction.** Extend the existing mirror lookup to return accepted version/original acceptance facts for no-claim requests before interpretation. A cold miss retains current provisional preparation and existing grouped fence SQL; its existing conflict read returns original binding/facts for strict retained-definition comparison on the existing bounded CPU owner outside the transaction. Roll back still-uncommitted fresh fences in that tenant group before exceptional comparison and resume only after equality. Current-contract preparation refusal uses only the existing exceptional accepted-ID acquisition boundary; an unknown ID keeps its original refusal. Retain source ownership under existing accounting rather than copying every fresh payload. Keep shard-owned completion/cancellation, canonical-attempt replay, original origin/time and full conflict proof. No preliminary lookup on successful fresh work and no new ledger.

**Boundary/proof.** Shared ingress/CPU/preprocess, committed mirror, shard completion/fence result and tenant fence projection/classification plus direct tests. Extend existing canonical and accepted-revision fixtures for exact no-claim retries after Bloom and supported grouping edits, warm/cold/restart and publication/WAL retirement. Verify one set of exact rows and original acceptance/origin, changed-input refusal, unknown-ID refusal, tenant-group rollback and unchanged fresh SQL counts. Do not require a request explicitly changing its native version claim to succeed; retain that intentional conflict behavior.

### FIND-TASK-004-8 — Validate against the selected native schema

**Gap.** `ingress.rs:232–272` pairs a current/retained native schema with `table_registration`'s creation SQL fingerprint (`bifrost_catalog.rs:1753–1787`). `execution_lanes.rs:574–595` conforms new input to that schema, then rejects it against the stale fingerprint. The existing rename/add-nullable catalog seam updates native metadata but not creation SQL; its journey only reads afterward. Consequently valid fresh current-version writes after the supported edit fail. Refreshing only SQL would instead break older retained definitions.

**Selected correction.** Remove the redundant registration-fingerprint comparison as a definition authority on the catalog-backed decode path, as R2 intended. The selected native definition owns conformance, type/nullability/reserved/value validation and field stamping. Preserve actual-source schema/payload/order/count conflict evidence and the explicitly no-catalog embedded/test seam's required contract. Keep registration for existence/UID/permission projection. Align description fingerprint consumers only where they represent that selected definition; introduce neither another fingerprint identity nor more normal-ingest SQL/layout hashing.

**Boundary/proof.** Ingress/decode context and callers, description/entry projection only as necessary, existing mixed-schema and accepted-version fixtures. After the supported stable-ID rename and nullable addition, describe and write valid new values; then retry original-version native content and retain wrong-shape/value/content refusals. Verify old/new exact row values and stable IDs.

### FIND-TASK-004-9 — Keep the accepted version in promotion evidence

**Gap.** `file_list_writer.rs:231–282,359–377` receives the version-bearing assembly key but drops it in publication facts and `PublishedHotFileIdentity`; `ScribePublishedHotFileV1` (`promotion.rs:174–243`) has no TableVersion. Forge (`scribe_promotion.rs:331–423`) infers matching schema/spec/order from current history. Two real Bloom-only versions share those IDs, so published evidence cannot identify the exact accepting definition after local evidence retires.

**Selected correction.** Carry the existing accepting version from assembly/frozen identity through publication facts and durable promotion JSON beside the existing exact DataFile evidence. At existing worker revalidation, resolve `BifrostCatalog::retained_definition(version)` and validate schema/spec/order/tuple/footer against it. Preserve independent complete SQL tuple comparison, checksum/object binding, worker terminal-failure ownership, leases/fences and ambiguous append reconciliation. Do not infer a missing version later or scan all rows. Follow approved legacy refusal when exact mapping is unavailable; no compatibility alias/backfill/new authority or additional ingest query.

**Boundary/proof.** File-list/promotion types, producers/fixtures and Forge promotion/retained-footer validation. Extend mixed-spec and corruption fixtures with Bloom-only versions sharing native IDs: old evidence still identifies its accepting version, contradictory version versus physical definition refuses before append, and intact old objects promote unchanged.

### FIND-TASK-004-10 — Keep Forge's selected output definition through commit

**Gap.** Managed policy (`policy.rs:106–126,178–196`), handoff/publication and rewrite lineage have no selected TableVersion. `layout.rs:533–565` uses constant `/data/forge/v2`, and `worker.rs:7138–7177,7234–7240` revalidates only schema on definite conflict. A Bloom-only edit leaves schema unchanged; outputs and lineage cannot state or revalidate the complete definition actually selected.

**Selected correction.** Select TableVersion once with the existing attempt's native table/policy and carry it through handoff, publication and committed snapshot/lineage. Supply its canonical decimal segment through the native core's existing recipe/output identity mechanism, replacing constant recipe-as-definition identity. Resolve retained input definitions by recorded versions. Reuse initial/retry authority acquisition to validate the retained selected output definition and its assumptions; retain them rather than silently adopting refreshed current Blooms. Do not blanket-refuse ordinary data-head movement when assumptions remain valid. Preserve native grouping/execution, delete scope/sequence, target geometry, lease/fence, no new outputs on retry, uncertain reconciliation, cleanup and sibling publication.

**Boundary/proof.** Forge path projection, managed policy/executor/handoff/evidence and worker/publication/lineage owners, existing native-core configuration and fixtures. Extend the existing Bloom rewrite and precise definite-conflict seam: assert decimal selected-version paths and durable lineage, change only Bloom policy between execution and retry, and prove retained output assumptions are revalidated without relabeling inputs or creating more output objects. Exact rows and unversioned data/snapshot commits remain correct. No second writer or metadata history service.

### FIND-TASK-004-11 — Resolve the mutable file target into version-1 metadata

**Gap.** `create_physical_table` (`bifrost_catalog.rs:1185–1238`) omits the target for `None`; `CompactionRegistration::write_properties:2017–2031` preserves that omission. `ForgeTablePolicy::extract:153–158` substitutes the live deployment default supplied by boot. Restarting with another target changes writer geometry under the same version; differently configured workers can disagree.

**Selected correction.** Pass the existing authorized resolved deployment target to the existing shared catalog creation/default owner and persist `write.target-file-size-bytes` with every omitted-target system/dynamic version-1 definition. Preserve explicit-target validation, omission ergonomics, bounds and idempotent re-registration. Later versioned edits preserve or explicitly change it; Forge reads the committed target for registered versions instead of silently applying today's deployment value. Do not freeze unrelated scheduler/lease/resource settings, invent a knob/counter, retune values, backfill old evidence or add per-batch IO.

**Boundary/proof.** Catalog config/construction handoff in server/test builders, creation property resolution and Forge policy extraction. Create under default A with no explicit target, assert native property A, extract the same definition under A and B and require A both times. New creation under B may resolve B. Extend the existing creation and declared-target policy fixtures.

### FIND-TASK-004-12 — Project exact native SDK integers

**Gap.** The wire serializer correctly emits strings, but Python native/public describe (`mod.rs:949–960`, `_client.py:1036,1294–1296,1416`) and TS native/public describe (`index.ts:7,1457–1458`) return that string unchanged. Both journeys expect `"1"`. Revision 13 separately requires SDK Python `int` and TS `bigint`, not just lossless wire text.

**Selected correction.** Keep canonical wire strings, shared Rust serde and generated JSON types. Convert directly at existing SDK description projections to exact Python int and TS bigint, with matching handwritten result types/authored stubs. Share Python sync/async projection. Never pass through JS number; retain positive signed-64 validation and existing errors. Cover sibling public description projections, not unrelated integers or future PATCH. No generic serialization framework or new server surface.

**Boundary/proof.** Existing Python projection/types/stubs and TS handwritten facade/result type plus language-owned boundary fixtures and describe journeys. Assert runtime type and exact value for 1, 9007199254740993 and 9223372036854775807; update incorrect journey expectations and typechecks. Shared malformed-wire refusal remains at the domain owner.

### FIND-TASK-004-13 — Keep the newest Python binding during describe

**Gap.** Python `handle` (`src/bifrost/mod.rs:584–607`) snapshots declaration A, releases the mutex/GIL for describe, then applies the result on FQN equality. During that IO `use_table:725–734` can successfully select already-described B at the same FQN with a different schema. A's result overwrites B, encoding later rows under the replaced declaration. This is real even with a fully resolved replacement; it is independent of unsupported versionless setter automation.

**Selected correction.** Apply an in-flight describe result only to the exact declaration/resolution state it resolved, preserving any newer selection. Reuse the complete declaration comparison pattern of shared Rust registration and the existing binding state/lock; do not equate names with selections. Where the existing operation requires resolution, use shared `resolve_table`. Keep network IO outside the binding mutex/GIL handoff, existing describe errors, producer/drain ownership and resolved hot-path behavior. No new client, service, queue or setter signature.

**Boundary/proof.** Python native handoff and a public Python threaded regression in the existing binding fixture. Delay A's describe, select already-described B with the same FQN/different schema, resume, and require B remains selected and its valid row writes/drains. Exercise actual Python/GIL lifetime rather than imitating it in Rust.

### FIND-TASK-004-14 — Document the new version proof owner

**Gap/correction.** The new parser `wyrd-spec/src/vala/ids.rs:448–450` omits Errors for real validation refusal; new tests at `:568,:582` have no rustdoc. Document parser syntax/range errors and test intent/invariants in place. Preserve behavior and assertions; no imagined Panics, helper, new check, suppression or separate style round. Original FIND-6 locations are closed. Source inspection closes this prose correction, together with normal formatting/lints for the write set.

### FIND-TASK-004-15 — Refuse a version-intended no-op at native publication

**Gap.** Pinned Iceberg SQL `a34f3885c`, `catalog.rs:529–574,1331–1408`, counts offered property updates without checking effectiveness. Native application preserves identical SetProperties. The required Bloom revision seam can submit existing `[service]` plus next version: it commits a new pointer/reference and stales an existing writer without changing the definition. R2 explicitly requires no-op semantics at this seam; this is not a request for future PATCH or an arbitrary-property guard.

**Selected correction.** At the existing native definition-publication owner, after applying a version-intended edit and before metadata write/publication, reject unchanged effective definitions without consuming version/reference. Compare existing native before/after effective schema, default spec/order and properties, excluding the version itself and snapshot/log/timestamp/publication bookkeeping. Ordered Bloom changes remain effective. Select refusal for this ineffective-edit case, retaining existing conflict/error behavior. No property registry or generic missing-bump protection: no-version data/snapshot/housekeeping commits keep their current path. Preserve expected predecessor, CAS/reference transaction atomicity, one-winner conflicts and uncertain commit reconciliation.

**Boundary/proof.** Native catalog publication and focused invalid-version/update tests, Wyrd immutable pins/lock and existing revision seam only as needed. Exact repeated Bloom declaration with next-version intent leaves pointer/version/reference count unchanged; adjacent reordered/changed cases advance once, competing valid commits retain one winner, and data commits remain unversioned. Use the fork's existing repository/mise instructions and record exact nonempty selectors. Prepare in an ordinary fork checkout, not Cargo cache; commit/push only when asked. A tested local patch alone does not close the finding until Wyrd uses the corrected immutable pin. Do not substitute a mutable branch or untracked path override.

## Acceptance and focused proof

| Finding | Direct closure observation |
| --- | --- |
| FIND-TASK-004-1 | Exact no-claim canonical retries recover original acceptance across supported definition edits and lifecycle states; changed/unknown input refuses, group rollback and fresh SQL counts are preserved. Explicit native changed-claim conflicts remain strict. |
| FIND-TASK-004-8 | Fresh current-version rows after rename/addition succeed; retained original-version rows/retries and structural/content negatives remain correct. |
| FIND-TASK-004-9 | Promotion evidence identifies/resolves its accepting version even when native IDs coincide; contradictions refuse before append. |
| FIND-TASK-004-10 | Output paths, revalidation and committed lineage retain the selected version through a Bloom-only edit/conflict retry without extra objects or lost rows. |
| FIND-TASK-004-11 | Same committed definition yields the same persisted file target under two deployment defaults; a new definition can resolve the new default. |
| FIND-TASK-004-12 | SDK descriptions return exact native integer types at small and >2^53/max values; wire stays canonical strings. |
| FIND-TASK-004-13 | Blocked describe cannot overwrite a newer already-resolved same-FQN declaration; Python row/drain behavior follows the replacement. |
| FIND-TASK-004-14 | Three named new items have useful rustdoc/error contracts; original docs closure stays intact. |
| FIND-TASK-004-15 | Identical version-intended edit is refused without new pointer/version/reference; effective edits and ordinary snapshot work preserve their specified behavior. |

Apply Red–Green–Refactor per demonstrated behavior, tracing failures and recording required independent diagnoses before changing proof to clear a failure. Use existing harnesses and exact newly added selectors once named; reject empty selections. Do not weaken assertions, skip tests, raise timeouts or hide negative flows. Keep prior FIND-2/3/4/5/7 regressions green only where this write set intersects them. Include required rustdoc as each item changes.

Existing focused selectors to extend (commands are future implementation verification, not runs performed by this review):

```bash
mise exec -- scripts/postgres/with-test-postgres.sh -- bash -lc 'mise run db:migrate:inner && cargo nextest run --locked -p wyrd-testing --test scribe -P journey --run-ignored=all -E "test(=write_read::accepted_batch_revision_survives_alteration_and_restart) | test(=write_read::mixed_runtime_specifications_preserve_rows_and_pins)"'

mise exec -- scripts/postgres/with-test-postgres.sh -- bash -lc 'mise run db:migrate:inner && cargo nextest run --locked -p wyrd-testing --test forge -P journey --run-ignored=all -E "test(=scribe_promotion::mixed_specifications_preserve_partition_and_sort_truth)"'

mise exec -- scripts/postgres/with-test-postgres.sh -- cargo nextest run --locked -p vala-bifrost-redux --test integration -P journey --run-ignored=all -E 'test(=forge::promotion::scribe_promotion_integration_rejects_physical_datafile_mismatch_before_catalog_io)'

mise exec -- scripts/postgres/with-test-postgres.sh -- bash -lc 'mise run db:migrate:inner && cargo nextest run --locked -p wyrd-testing --test oracle -P journey --run-ignored=all -E "test(=distributed::bloom_policy_versions_answer_exactly_through_compaction)"'

mise exec -- cargo nextest run --locked -p vala-bifrost-redux --lib -E 'test(=forge::managed::policy::tests::forge_table_policy_preserves_declared_target_geometry)'

mise exec -- cargo nextest run --locked -p wyrd-spec --lib -E 'test(=vala::ids::tests::table_version_round_trips_canonical_strings_beyond_two_to_the_53) | test(=vala::ids::tests::table_version_refuses_noncanonical_forms)'
```

Also run the exact extended canonical OTLP journey, SQL fresh-count/rollback/reconciliation, canonical replay and precise Forge definite-conflict tests. Confirm their current package/target/full selectors in source/manifests and record them in R3 evidence before claiming closure. Run the focused native no-op proof in the fork and record its exact commit and command; do not demand its whole test suite.

Language setup and focused file checks reuse the owning mise lanes' preparation, with all Cargo-backed commands sequential. `py:setup`, `ts:build` and `ts:build:testing` are existing build owners. After the relevant native changes, run the affected boundary unit tests plus these focused integration files using the same repository-managed Postgres lifecycle:

```bash
mise run py:setup
mise exec -- scripts/postgres/with-test-postgres.sh -- bash -lc 'mise run db:migrate:all:inner && uv run --directory sdks/wyrd-sdk-python python -m pytest -q -m "integration and not local_dev" tests/integration/bifrost/test_table_time_column.py tests/integration/bifrost/test_bifrost_e2e.py'

mise run ts:build
mise run ts:build:testing
mise exec -- scripts/postgres/with-test-postgres.sh -- bash -lc 'mise run db:migrate:all:inner && cd sdks/wyrd-sdk-ts/wyrd && pnpm exec vitest run tests/integration/table-time-column.test.ts'

mise run fmt
mise run lints
mise run check:deps
mise run check:tenant-isolation
mise run py:lints
mise run py:typecheck
mise run ts:lints
mise run ts:typecheck
```

Record an exact selector for the new Python race once introduced. If generated declarations actually change, regenerate through their owner and run `codegen:check` and the affected `ts:napi:check`; never hand-edit outputs. Keep Python authored stubs consistent. Do not add new scripts or named verification lanes for this task.

No `gate`, broad Redux aggregate, full language suites or `verify:bifrost` aggregate is required solely for R3. Full integrated verification remains change-review work. Do not run benchmarks now. Record exact RED/GREEN/check results, selected counts, immutable dependency revision, prior closure preservation and unmeasured throughput. The next `$wyrd-task-review` includes the original cumulative base, prior verdicts/ledgers, R1/R2/R3 and the remediated immutable candidate.
