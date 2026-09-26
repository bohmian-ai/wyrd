# Domain Review: Public Contracts and SDK Projections

## Reviewed boundary

- Immutable subject: base `c8bb490ad814c0c7770cac33ed7779897ff776e4`, candidate `935cc6324d213414402a1d73d6eeec475965fbcb`.
- Scope: cumulative shared Rust Drift/Eval/generic projections; Python and TypeScript authoring boundaries; public SDK types and errors; and the Rust, Python, and TypeScript real-server journeys, with focused review of the R4 `strictJson` snapshot and Eval media correction.
- Authority: approved specification revision 32, original TASK-002, `run_api.md`, R1-R4 remediation artifacts, `AGENTS.md`, and `architecture/agent-rules.md`.
- The repository has no `.codegraph/` directory, so inspection used direct source, immutable diffs, and `rg` caller tracing. AI co-author trailers are explicitly allowed and outside this review.

## Authority and source coverage

| Concern | Source and proof inspected | Result |
|---|---|---|
| One shared canonical projection with thin language boundaries | `wyrd-client/src/observe/{mod,drift,eval}.rs`; Python `src/observe/mod.rs`; TypeScript `src/index.ts`; canonical `EvalRecordObservation` and `MediaRef` | PASS |
| R4 single-read TypeScript serialization | Complete `strictJson` body and all four callers (`mediaJson`, Drift, Eval context, generic record); new accessor tests | PASS |
| Closed Eval media descriptor projection | `EvalMediaRef`, `MEDIA_KEYS`, complete `mediaJson`, descriptor-key tests, and canonical Rust media decoding | PASS for each descriptor; FAIL for the containing caller media array (`DC-R5-001`) |
| Drift/Eval/generic row contracts and writer/subject correlation | `Run::correlation`, `Observe::{drift,eval,record}`, fixed-row projections, exact schema checks, queue fixed-size-binary conversion, and all three journey read-backs | PASS |
| Eval session/media plus explicit and active trace identity | Rust, Python, and TypeScript journey inputs, stored-row read-back, and span-without-trace/malformed-media refusals | PASS |
| Fixed startup, describe caching, reserved/unknown refusal, and stale-schema fencing | Three SDK journeys, shared owner tests, and `pg_bifrost_e2e` evidence retained by TASK-002 | PASS |
| Public language shapes and stable errors | Python exports/stubs, TypeScript readonly aliases/declarations, `WyrdError` projection, and corrected task evidence | PASS |

## Prior-finding closure

| Finding | Result | Evidence |
|---|---|---|
| `FIND-TASK-002-4` | **OPEN / REOPENED** | The R4 change correctly snapshots ordinary context/feature/row objects and array elements and validates every original media descriptor, but it calls `media.map(...)` before `strictJson`, so unsupported own properties on the original media array still disappear before validation. |
| `FIND-TASK-002-13` | CLOSED | Each SDK journey persists and reads back required Eval session/media and fixed-width trace/span identity and proves invalid-pair and malformed-media refusal. |
| `FIND-TASK-002-14` | CLOSED | The journeys retain fixed-table startup refusal and cache-reuse evidence, with real stale-fingerprint fencing in the Bifrost integration test. |
| `FIND-TASK-002-16` | CLOSED | `EvalMediaRef` and `EvalOptions` remain readonly type aliases. |
| `FIND-TASK-002-17` | CLOSED | The evidence and implementation consistently use `WYRD_SPEC_400_VALIDATION`. |

## Proposed finding

### `DC-R5-001` — Eval media arrays still lose own caller data before strict validation

- **Proposed stable ID:** reopen `FIND-TASK-002-4`.
- **Classification:** INCORRECT.
- **Violated obligation:** REQ-124, REQ-129, `run_api.md` lines 124-130, and the cumulative approved remediation require unsupported TypeScript observation values and silent JavaScript omission to fail before native queue admission; R3 specifically established that own non-element array data must not disappear.
- **Exact location:** `sdks/wyrd-sdk-ts/wyrd/src/index.ts:1205-1220`; missing containing-array cases in `sdks/wyrd-sdk-ts/wyrd/tests/unit/observe.test.ts:233-259`.
- **Evidence and reachability:** `mediaJson` invokes `media.map(...)` first and passes only the new mapped array to `strictJson`. A public built-SDK `Observe.eval` probe supplied three ordinary arrays containing one valid descriptor and, respectively, an own enumerable `extra` property, an own symbol property, and an own non-enumerable property. All three calls succeeded and native received the same truncated JSON, `[{"id":"page","kind":"document","uri":"s3://x"}]`. The descriptor-level loop cannot see array-owned keys, and the current test covers properties on descriptor objects only.
- **Observable consequence:** a caller can receive successful Eval admission while the persisted media value omits own data supplied on the media array, leaving the Rust boundary unable to detect that the TypeScript SDK changed the accepted input.
- **Required testable correction:** At the existing `mediaJson` boundary, apply the same canonical-element-only own-key rule to the original media array before mapping descriptors, reusing or narrowly factoring the existing `strictJson` array-key check; preserve the single-read descriptor projection, one serializer, current wire names, and Rust-owned durable validation.
- **Focused closure proof:** Add named, symbol, and non-enumerable own-property cases on the media array itself, assert `WYRD_SPEC_400_VALIDATION` and zero native calls, retain the existing accepted getter-backed descriptor assertion, then run the exact `observe.test.ts` target plus the TypeScript unit, typecheck, and integration lanes.

## Verification limits

- `pnpm exec vitest run tests/unit/observe.test.ts` passed 10/10 at the candidate, but its media cases cover descriptor properties rather than properties on the containing media array and therefore do not exercise `DC-R5-001`.
- The user reports TypeScript unit/integration/typecheck, format, lint, and diff checks passing. `verify:bifrost` was still running when review began; this is recorded as a verification limit and is not the basis of the finding.
- The reviewed TypeScript source and test paths matched `935cc6324d213414402a1d73d6eeec475965fbcb`, but during Wave 1 the branch advanced to `4642bab9440b8b84220d8e3edac83655d8ec9cac` with a committed change to `crates/wyrd/wyrd-testing/src/server.rs`; under the immutable-subject rule, the review cannot issue an acceptance verdict for the moving candidate.
- No optional API redesign, generalized validation framework, new dependency, or additional cross-language behavior is proposed.

## Overall result

**BLOCKED.** The candidate changed during Wave 1. Inspection of the originally assigned commit independently found `DC-R5-001`, which would keep `FIND-TASK-002-4` open, but the required immutable cumulative subject no longer matches the branch and must be re-established before a verdict.
