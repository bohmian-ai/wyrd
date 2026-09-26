# Domain Review: Public Contracts and SDK Projections

## Reviewed boundary

- Immutable subject: base `c8bb490ad814c0c7770cac33ed7779897ff776e4`, candidate `b56560e511918efbdd84d8756b13100fc381eda0`.
- Scope: shared Rust Drift/Eval/generic observation projection, Python and TypeScript runtime boundaries, public generated/hand-authored SDK surfaces, stable error projection, active and explicit trace identity, and the Rust/Python/TypeScript real-server journeys.
- Prior closure checked: `FIND-TASK-002-4`, `FIND-TASK-002-13`, `FIND-TASK-002-14`, `FIND-TASK-002-16`, and `FIND-TASK-002-17`, including the R1-R3 remediation contracts and evidence.
- The repository has no `.codegraph/` index, so inspection used immutable diffs, direct source reads, and `rg` caller/use tracing. AI co-author trailers are user-authorized and outside this review.

## Authority and source coverage

| Concern | Governing authority | Source and proof inspected | Result |
|---|---|---|---|
| One shared observation implementation and idiomatic SDK projections | `AGENTS.md` §§2-3, 7-9; `wyrd-design.md`; `wyrd-doctrine.mdx`; TASK-002 | `wyrd-client/src/observe/{mod,drift,eval}.rs`; Python `src/observe/mod.rs`; TypeScript `wyrd/src/index.ts` and native `cards.rs`; public Python exports/stubs and TypeScript declarations | PASS |
| Strict TypeScript input preservation before native admission | REQ-124; `run_api.md` TypeScript and input-boundary contracts; R1/R2/R3 remediation for `FIND-TASK-002-4` | `strictJson`, all four callers, `mediaJson`, and `observe.test.ts` | **FAIL (`DC-R4-001`)** |
| Canonical Drift/Eval projection and writer/subject split | REQ-118, REQ-121, REQ-123, REQ-124, REQ-129; `run_api.md`; telemetry observation identity | `Run::correlation`; `Observe::{drift,eval,record}`; canonical record construction; fixed rows; three SDK journey read-backs | PASS |
| Eval session/media and explicit/active trace identity | REQ-129; AC-026; telemetry observation identity; R2 `FIND-TASK-002-13` | Rust explicit IDs and media; Python absent/active/explicit cases; TypeScript real active OTel span through N-API; persisted fixed-width ID read-back and invalid-pair/media refusals | PASS |
| Fixed-table startup refusal, cached description, and stale-schema fence | REQ-127, REQ-128; AC-025; R2 `FIND-TASK-002-14`; testing workflow | Per-table startup failure in all three SDK journeys; dynamic/fixed describe counts; `assert_stale_writer_is_fenced` real client/server proof | PASS |
| Stable cross-language errors | `AGENTS.md` §4; errors reference; R3 `FIND-TASK-002-17` | Catalog-backed Rust errors; Python mapper; TypeScript `WyrdError`; journey/unit code assertions; corrected task evidence | PASS |
| Public language typing and generated surfaces | TypeScript guide; Python API/stub and PyO3 references; R3 `FIND-TASK-002-16` | Readonly TypeScript aliases and built declaration; Python native registration, public exports, generated stubs, and task-recorded codegen/typecheck results | PASS |
| Real SDK journey tier | `AGENTS.md` §11; testing workflow; AC-025/AC-026 | `sdks/wyrd-sdk-rust/tests/observe_run.rs`; Python `test_observe_journey.py`; TypeScript `observe-run.test.ts`; task-recorded `verify:bifrost` 9/9 and language integration lanes | PASS |

## Prior-finding closure

| Finding | Result | Evidence |
|---|---|---|
| `FIND-TASK-002-4` | **OPEN / REOPENED** | The R3 change correctly rejects hidden and symbol-keyed plain-object properties and non-element array properties for Drift, Eval context, and generic records, but `mediaJson` constructs a fresh known-field object before calling `strictJson`, so extra own media-descriptor properties still disappear before validation. |
| `FIND-TASK-002-13` | CLOSED | Each real SDK journey persists required Eval options and exact trace/span identity, reads them back, and proves span-without-trace and malformed-media refusal before an extra row appears. |
| `FIND-TASK-002-14` | CLOSED | All three SDK journeys prove fixed-table preflight failure and cached lookup reuse; the real Bifrost integration test proves stale flush and registration fingerprint refusals with no stale row stored. |
| `FIND-TASK-002-16` | CLOSED | `EvalMediaRef` and `EvalOptions` are exported readonly object type aliases with unchanged fields and built declarations. |
| `FIND-TASK-002-17` | CLOSED | The R2 evidence, implementation, and focused assertion now consistently name `WYRD_SPEC_400_VALIDATION`. |

## Proposed finding

### `DC-R4-001` — Eval media can still lose own caller data before strict validation

- **Proposed stable ID:** reopen `FIND-TASK-002-4`.
- **Classification:** INCORRECT.
- **Violated obligation:** REQ-124 and `run_api.md` require the TypeScript boundary to reject unsupported values that serialization would silently omit or coerce; the validated R1 correction explicitly routes Eval media through the one strict serializer, and the R2/R3 correction requires own observation data not to disappear before native admission.
- **Exact location:** `sdks/wyrd-sdk-ts/wyrd/src/index.ts:1184-1196`; missing media-specific cases in `sdks/wyrd-sdk-ts/wyrd/tests/unit/observe.test.ts:134-188`.
- **Evidence:** `mediaJson` maps each caller-supplied descriptor to a fresh object containing only `id`, `kind`, `uri`, and optional `media_type`, then passes that replacement to `strictJson`. An own enumerable extra property, symbol key, or non-enumerable property on the original descriptor is therefore discarded before `Reflect.ownKeys` runs. The R3 refused-input table passes its shapes as Drift features, Eval context, and generic rows, while its only media case puts a symbol in the known `uri` field; it never exercises own descriptor keys. This path is reachable from public `Observe.eval` and reaches the native Eval call with truncated media JSON.
- **Observable consequence:** a TypeScript caller can supply a media descriptor containing unsupported own data and receive successful queue admission even though the persisted canonical media differs from the supplied value, contradicting the SDK's strict-boundary contract.
- **Required testable correction:** Reuse `strictJson` and the existing `WYRD_SPEC_400_VALIDATION` path; validate each original media descriptor before or while performing the required `mediaType` to `media_type` projection, rejecting own keys outside the declared `EvalMediaRef` fields and every symbol or non-enumerable own key. Preserve absent optional `mediaType`, the existing canonical snake-case wire shape, and Rust-owned durable media validation. Add focused root/nested descriptor cases proving no native call; add no serializer, dependency, or Rust duplicate.
- **Focused closure proof:** Extend the existing TypeScript observation unit test with Eval media descriptors carrying an extra enumerable key, a symbol key, and a non-enumerable key, assert `WYRD_SPEC_400_VALIDATION` and zero native calls, then run the exact `observe.test.ts` target, `mise run ts:test:unit`, `mise run ts:typecheck`, and `mise run ts:test:integration`.

## Verification limits

- This review did not rerun the recorded test matrix. TASK-002 records `verify:bifrost` 9/9, the focused TypeScript test, TypeScript unit/type/integration lanes, the shared observation and fixed-binary tests, format, lint, and diff checks as passing on the code-bearing remediation candidate; candidate `b56560e5` adds only the final task evidence after those changes.
- The green tests do not cover `DC-R4-001`; the gap is before the tested strict serializer input and is directly visible from the full public caller body.
- No optional redesign, declaration preference, or additional cross-language validation is proposed.

## Overall result

**FAIL.** One reachable TypeScript contract defect remains: `DC-R4-001`, reopening `FIND-TASK-002-4`. The other requested prior contract and journey findings are closed, and the correction stays within the approved task without a specification revision.
