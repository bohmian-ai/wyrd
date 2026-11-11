---
id: TASK-002
kind: implementation
status: proposed
spec: SPEC-sdk-authoring-ergonomics
spec_revision: 5
requirements: [REQ-005, REQ-006, REQ-007, INV-001, INV-002, INV-003, AC-003, AC-004, AC-005, AC-006]
depends_on: [TASK-001]
direct_request: 2026-11-11
---

# Reduce client copies and verify the savings

## Outcome and Value

Fix unnecessary object copying and allocation across public authoring and
WyrdState interfaces in Rust, Python and TypeScript. Prioritize repeated state
access and runtime operations. Demonstrate the reductions with before/after
macOS Instruments Allocations traces.

The maintainer approved this internal performance follow-up and its macOS
measurement method on 2026-11-11. Specification revision 5 continues to govern
the existing public behavior. Deliver the fixes and measurement evidence in
this one task.

## What Changes and What Must Stay True

Audit definition construction, capture, editing, validation and declaration;
state loading/hydration, typed and general access, Agent/Prompt navigation;
and state-backed run, verification and evidence operations. Cover the shared
Rust owner and both language adapters. Record each affected path's necessary
copies and removed duplication in the task evidence.

Preserve definition snapshots, replacement behavior, complete validation,
exact references, version selection, hashing/replay and Artifact publication.
Preserve Rust borrows and shared state/run handles, Python holder identity and
garbage collection, and TypeScript independent mutable return values. Keep
run IDs, subjects, input ownership, errors, tenant boundaries, async lifetimes,
writer shutdown and cleanup behavior intact.

Public JSON export may require an owned copy. Existing Arc/Bytes handle clones
may already be cheap. Preserve those contracts and verify already-efficient
paths without forcing changes.

Measurement uses Xcode's process instrumentation. Keep profiling workloads in
test/example code and retain normal production allocators. Allocation counters,
profiling hooks, custom allocators, a report parser and a benchmark framework
are excluded. Existing graph, loader, state, index and serialization owners
remain responsible for their workflows.

## Approach

1. Build symbolized production SDK artifacts and record the fixed workloads
   below before changing code.
2. Remove the demonstrated capture, retained-body and serialization copies at
   their existing owners, following the scenarios below.
3. Run focused ownership/lifetime regressions and real SDK journeys.
4. Repeat the identical Instruments recordings and document the allocation
   reductions, remaining necessary copies and any unaffected efficient paths.

## Ordered Implementation Scenarios

### Scenario 1 — Authoring retains distinct snapshots without repeated subtrees

**Behavior.** Build a dependency chain and a graph with shared descendants.
Capture and declare them without retaining complete descendant payloads at
every flattened ancestor or full spec bodies solely for target matching.
Later source edits and field replacements retain their current behavior.

**RED.** Add a focused ownership/retention regression in the existing native
authoring test owner that exposes the repeated descendant storage. Confirm
and record its actual exact selector before running it. Record the baseline
authoring allocation stacks and bytes using the measurement procedure below.
Existing snapshot tests supply the compatibility controls.

**GREEN.** Remove the duplicated ownership and intermediate materializations
within shared capture/declaration. Make the new regression and
[authoring preservation](#authoring-preservation) pass and demonstrate reduced
allocation work at the affected stacks.

**REFACTOR.** Reuse existing matching, traversal, loader validation and
publication. Preserve metadata needed for conflicts, files and fresh snapshots.

### Scenario 2 — State hydration and access avoid redundant payload copies

**Behavior.** Load a graph and repeatedly select its Cards, Agents and Prompts.
Reduce duplicated retained envelopes/holders and repeated internal projection
or serialization. Requested owned output remains independent. Existing Rust
shared handles and Python holder reuse remain efficient.

**RED.** Add a focused regression at the affected state or SDK projection
owner, with its exact selector recorded before execution. Show the redundant
retention or projection in the baseline Instruments call stacks. Keep the
existing identity, returned-object isolation and lifetime assertions.

**GREEN.** Remove the demonstrated copies through the current state/index and
binding owners. Pass [state preservation](#state-preservation) and affected
SDK tests, then show reduced allocations for identical state workloads.

**REFACTOR.** Reuse state-owned immutable data and typed navigation. Preserve
fresh mutable exports and release objects according to their existing lifetimes.

### Scenario 3 — State-backed runtime operations reuse loaded data

**Behavior.** Open runs, select Card views and resolve verification/evidence
inputs without copying hydrated graphs or redundantly converting payloads.
Fresh runs and shared-invocation views keep their established subjects and
lifetimes.

**RED.** Where the audit demonstrates redundant runtime work, add the smallest
regression at that owner and record its exact selector before running it.
Capture the corresponding baseline allocation stack. Already-shared or
necessarily owned paths use verification-only proof and require no artificial
RED or production change.

**GREEN.** Remove demonstrated redundant lookup or conversion work while
preserving input validation, serialization and async ownership. Run
[runtime preservation](#runtime-preservation) and the focused SDK journeys.
Compare the repeated-operation allocation stacks before and after.

**REFACTOR.** Reuse existing indexes, shared handles and evidence conversion
owners. Keep required output copies and native/foreign-runtime ownership.

## Acceptance Criteria

| Obligation | Required observation |
|---|---|
| Maintainer request, all SDKs | The authoring and state paths above are audited in Rust, Python and TS, with fixes for demonstrated redundancy and evidence for necessary remaining copies. |
| Maintainer request, measured savings | Matched Instruments recordings show reduced allocation count and allocated bytes at each optimized clone/serialization path. The same workload's process totals and retained bytes are also reported, with any increase explained and resolved when caused by the change. |
| INV-001/003 | Existing capture, graph, loader, state and projection owners perform the work. Repeated subtree retention and unnecessary full-body matching/projection copies are removed. |
| INV-002, AC-003/005 | Snapshot, validation, exact-reference, declaration and publication behavior stays correct through focused native tests and existing SDK journeys. |
| REQ-005/006/007, AC-004/006 | Rust lifetime/borrow behavior, Python identity/GC, TS returned-object independence and state-backed scoped verification remain correct on their public surfaces. |
| Measurement integrity | Native allocations are included, stack symbols resolve to the affected owners, baseline/candidate workloads match, and traces and before/after numbers are retained as task evidence. |

## Implementation Reference

Revalidated at `ff4883e5d`. Reuse these owners and inspect their affected callers.

| Owner and consumers | Known copy issue and correction boundary |
|---|---|
| `crates/shared/wyrd-client/src/authoring/mod.rs`, Captures/CardDefinition/Body, Graph::absorb/declaration, Declaration::bind_published; Cards::declare and SDK definition adapters | Nested definitions derive deep Clone. Flattening clones descendants and retains their original nested captures. Declaration retains complete target submissions beside input and repeatedly decodes remaining specs during publication. Reduce these copies inside current snapshot, matching and validation owners. |
| `crates/shared/wyrd-loader/src/{lib,order}.rs` and `crates/wyrd-spec/src/graph/mod.rs`; native authoring and server consumers | Validation/ordering clone and serialize specs. Remove avoidable client intermediates while preserving the shared graph and validator and their server behavior. |
| `crates/shared/wyrd-client/src/state.rs`, HydratedStateIndex::assemble, hydrate_typed_cards, bound_verifier; observe/verify callers | State/run clones already share their index. Hydration retains envelopes and cloned typed holders. Repeated binding lookup builds temporary keys/collections. Reduce demonstrated retention and dispatch costs without replacing state ownership. |
| Python SDK `src/{state,observe,verify}` and public stubs; hydration, authoring and state tests | Captured native definitions and initial hydrated holders are copied. Repeated holder access already uses clone_ref. Preserve holder identity, owned JSON exports and Pydantic/dict input semantics. |
| TS SDK `native/src/{authoring,cards,lib,verification}.rs`, `wyrd/src/{definitions,index,native}.ts`; authoring/observe/state consumers | get projects a Card for dispatch before projecting it again; Agent lookup projects Agent and Prompt data repeatedly. Lifecycle result conversion creates a full JSON Value and then a string. Remove redundant projection/serialization while retaining isolated public values and safe N-API conversion. |

## Verification and Evidence

### Instruments measurement

Use **macOS Instruments Allocations through xcrun xctrace**. Retain debug symbols
for the normal Rust client binary and native Python/Node extensions so allocation
stacks resolve to Wyrd functions and source. Use production SDK builds, including
a production Python wheel and the normal TS addon, without the test-server
extension. Build artifacts before recording.

Use short public-SDK workloads in the existing test/example owners, with fixture
and server preparation outside the measured process. Add only the small workload
entrypoints needed to isolate the client. Each workload exercises one operation
family and checks its result. Keep its inputs and iteration count identical
between baseline and candidate.

| Workload in each SDK | Fixed comparison |
|---|---|
| Authoring capture and declaration | Chains of 8 and 32 Cards, plus 32 parent references sharing one child; 64 KiB prompt/schema payload. |
| Offline hydration | The same prepared Service/Agent/Prompt bundle before and after, including aliases sharing exact Cards. |
| Warm state access and runtime views | Hydrate once, then perform 1,000 identical typed lookups, Agent/Prompt navigations and run/Card-view operations. |
| Evidence conversion, when changed | The same named JSON/model/file inputs and operation count through the affected public call. Keep any server outside the profiled client process. |

For each SDK/workload, record a baseline trace and a candidate trace with the
same build profile, tool/runtime versions, fixture and call-tree selection.
Launch the actual client executable through this verified command form.
`CLIENT` is the absolute Rust binary, isolated Python interpreter or Node
executable; following arguments select its workload.

```bash
mise exec -- xcrun xctrace record --template Allocations --no-prompt \
  --output /tmp/wyrd-before.trace --launch -- CLIENT WORKLOAD_ARGUMENTS
mise exec -- xcrun xctrace record --template Allocations --no-prompt \
  --output /tmp/wyrd-after.trace --launch -- CLIENT WORKLOAD_ARGUMENTS
```

Use distinct filenames per SDK/workload. Open the traces in Instruments and
compare the allocation call tree, total allocation count, total allocated bytes,
persistent bytes and transient bytes. Record both process totals and the selected
Wyrd allocation stacks, with the same selection boundaries. Repeat recordings
when runtime noise prevents a clear comparison. Save traces outside the source
tree and put the before/after table and trace locations in implementation evidence.

A claimed optimization must show a clear allocation reduction at its affected
stack with preserved behavior. A lower allocation-backed clone/serialization
cost plus the ownership regression proves the relevant copy reduction.
Allocations into Python/V8 pools and copies into existing buffers have different
visibility. Label those limits and claim only the measured savings. Exact
universal copied-byte totals and automated peak-live heap figures are excluded.

### Authoring preservation

```bash
mise exec -- cargo nextest run --locked -p wyrd-client --lib -E 'test(=authoring::tests::offline_edits_preserve_snapshots) or test(=authoring::tests::field_edits_defer_validation_and_preserve_snapshots) or test(=authoring::tests::replacements_keep_only_current_dependencies) or test(=authoring::tests::declarations_match_file_authored_tree)'
mise exec -- uv run --directory sdks/wyrd-sdk-python python -m pytest -q tests/unit/cards/test_authoring.py
mise exec -- pnpm --dir sdks/wyrd-sdk-ts/wyrd exec vitest run tests/unit/authoring.test.ts
```

### State preservation

```bash
mise exec -- cargo nextest run --locked -p wyrd-client --lib -E 'test(=state::tests::typed_accessors_return_native_rust_cards) or test(=state::tests::aliases_for_same_card_resolve_same_address)'
mise exec -- uv run --directory sdks/wyrd-sdk-python python -m pytest -q tests/unit/state/test_hydration.py::test_duplicate_aliases_return_identical_python_object tests/unit/state/test_hydration.py::test_hydrated_objects_survive_gc tests/unit/state/test_hydration.py::test_state_interface_cycle_is_collected
```

### Runtime preservation

```bash
mise exec -- cargo nextest run --locked -p wyrd-client --lib -E 'test(=observe::tests::scoped_views_share_one_invocation_and_keep_their_subjects) or test(=observe::tests::each_run_is_its_own_invocation)'
mise exec -- uv run --directory sdks/wyrd-sdk-python python -m pytest -q tests/unit/state/test_observe_surface.py
mise exec -- pnpm --dir sdks/wyrd-sdk-ts/wyrd exec vitest run tests/unit/observe.test.ts
```

### Focused SDK journeys

Extend the existing component verification journey where needed to exercise
affected authored state loading, repeated navigation and scoped execution.

```bash
mise exec -- scripts/postgres/with-test-postgres.sh -- bash -lc "mise run db:migrate:all:inner && mise exec -- cargo nextest run --locked -p wyrd-ai --test integration -P journey --run-ignored=all -E 'test(=authoring::multi_component_service_verifies_each_subject)'"
mise exec -- scripts/postgres/with-test-postgres.sh -- bash -lc 'mise run db:migrate:inner && mise exec -- uv run --directory sdks/wyrd-sdk-python python -m pytest -q -m integration tests/integration/cards/test_authoring_journey.py::test_multi_component_service_verifies_each_subject'
mise exec -- scripts/postgres/with-test-postgres.sh -- bash -lc 'mise run db:migrate:inner && mise exec -- pnpm --dir sdks/wyrd-sdk-ts/wyrd exec vitest run tests/integration/authoring.test.ts -t "^multi_component_service_verifies_each_subject$"'
```

Confirm actual selectors for new tests before RED and record their exact focused
commands. Run format/lints and SDK typing/declaration/package parity for the final
write set, with codegen/dependency checks when their sources or edges change.
Run Cargo commands sequentially. Documentation/generated parity and unchanged
efficient behavior use static or verification-only proof. Finish with
`git diff --check`. Full SDK suites run at integrated change review.

## Material Stop Conditions

Return to specification authority for required changes to public SDK signatures,
snapshot/isolation guarantees, wire or persisted contracts, validation, identity,
tenant/permission boundaries, concurrency or resource lifecycle semantics, or a
new dependency/Cargo feature. Existing-owner sharing, borrowing, indexing and
serialization choices remain implementation-owned.

## Authority Links

[Approved specification](../spec.md),
[workflow](../../../spec-driven-development.md),
[implementation execution](../../../implementation-execution.md),
[testing](../../../../TESTING.md),
[client and Run authority](../../../../architecture/wyrd-design.md#client-model),
[authoring authority](../../../../architecture/wyrd-design.md#in-memory-authoring),
[Rust rules](../../../../agent_docs/rust.md),
[Python rules](../../../../sdks/wyrd-sdk-python/AGENTS.md),
[TypeScript rules](../../../../sdks/wyrd-sdk-ts/AGENTS.md).

## Implementation Evidence

Commits: `24c346a37` (Scenario 1 capture sharing), `fba7fcf63` (Prompt compiled
once per decode; authored form lent by request fingerprint), `07781e405`
(graph submissions decoded once; loader validation without spec clones),
`881777f51` (shared hydrated envelopes, precomputed Agent Prompt keys, Python
`Arc<Card>` envelopes), `4ed25f54b` (plain Chat turn text moved),
`b5a83cfe9`/`66cb3383f` (TS native results serialized directly from borrowed
projections), `a22b57c7f` (fingerprint regression), `60ce9ff07` (debug view),
`67b421735` (stale export assertion narrowed to its intent).

### Instruments totals (All Heap Allocations, total bytes / allocation count)

Release builds with `CARGO_PROFILE_RELEASE_DEBUG=line-tables-only`, identical
workloads, fixture and runtimes. Traces:
`$TMPDIR/claude-501/.../scratchpad/traces/<sdk>-<workload>-{before,after}.trace`.

| Workload | Rust before → after | Python before → after | TS before → after |
|---|---|---|---|
| authoring chain8 | 45.8 MB / 16,058 → 2.46 MB / 5,268 | 58.0 MB / 31,791 → 13.9 MB / 19,591 | 99.3 MB / 84,652 → 52.4 MB / 70,234 |
| authoring chain32 | 61.5 MB / 84,400 → 3.85 MB / 17,784 | 82.3 MB / 126,053 → 14.2 MB / 32,753 | 132.2 MB / 205,033 → 54.3 MB / 85,933 |
| authoring shared | 364.4 MB / 136,478 → 4.78 MB / 35,931 | 389.1 MB / 161,204 → 15.3 MB / 52,088 | 443.6 MB / 223,516 → 56.5 MB / 107,535 |
| hydration | 4.55 MB / 9,706 → 4.54 MB / 9,693 | 11.38 MB / 22,291 → 11.35 MB / 22,119 | 53.0 MB / 72,625 → 53.2 MB / 72,674 |
| access typed ×1000 | 4.55 MB / 9,707 → 4.54 MB / 9,694 | 11.89 MB / 38,306 → 11.86 MB / 38,134 | 95.7 MB / 543,871 → 63.9 MB / 113,895 |
| access agent ×1000 | 4.80 MB / 17,707 → 4.54 MB / 9,694 | 11.77 MB / 34,310 → 11.74 MB / 34,138 | 110.7 MB / 758,888 → 63.6 MB / 103,861 |
| access run ×1000 | 4.84 MB / 19,707 → 4.83 MB / 19,694 | 11.77 MB / 34,306 → 11.74 MB / 34,134 | 59.0 MB / 103,014 → 59.1 MB / 102,984 |

Persistent bytes are unchanged per workload (Rust ≈147–156 KB, Python 1.08–1.22
MB, TS ≈230–239 KB): the savings are transient. The first Python access-agent
baseline was a short import (1,440 persistent allocations); two re-recordings
with the same before wheel gave 2,390 / 34,310 and replace it. The TS
hydration +0.17 MB / +49 allocations is within V8 run noise and touches no
changed path; the first after chain8 Rust trace was an aborted launch and was
re-recorded twice with identical results.

### Affected stacks (in-process counting, 64 KiB Prompt payload, one Agent)

`declaration_input_from_cards`: 13.35 MB → 6.10 MB (`inline_prompt` compiles
once instead of three times) → 3.80 MB (`Prompt::new` slot scan and JSON text
without intermediate copies) → 2.35 MB (authored form lent by fingerprint, no
recompile) → 681 KB (graph decode once) → 548 KB (loader validation visits the
spec in place) → ≈7.3× payload (Chat text moved). `Prompt::authored()` now
allocates nothing for an unchanged request.

| Kind | Parse | Declare |
|---|---|---|
| Prompt / Agent | 5.1–5.2× | 7.3–7.4× |
| Model / Data | 1.2× | 3.3× |
| Verifier | 1.2–1.6× | 3.3× |

Necessary remaining copies: output serialization (1×), typed decode (1×), the
server-shared graph clone of borrowed submissions (1×), the decode `Value`
clone (serde copies strings even from a borrowed decoder), and for Prompts the
retained request plus authored form (2×, by design), the placeholder JSON
scan (1×), and `PortableContent` untagged buffering (1×; a manual deserializer
would change error text, a validation-contract change). TS returns fresh JSON
per call by contract; Rust state/run clones already share one `Arc` index;
Python holder reuse already used `clone_ref`. Run-view operations (Rust run
+10 allocations/iteration) keep their per-run subject and ID ownership and
were verified, not changed.

Measurement limits: allocations inside Python object pools and V8 heaps are
not malloc-visible; deep stacks hit Instruments' call-stack limit, so
per-stack figures come from the in-process count and process totals from
Instruments. No exact universal copied-byte totals or peak-live figures are
claimed.

### Diagnosis

- **Symptom:** the focused `multi_component_service_verifies_each_subject`
  journeys failed in all three SDKs (and TS `local_artifacts_and_declaration_parity`).
- **Evidence:** step `policy`: "failed to spawn monty worker: …/target/debug/wyrd-server: No such file or directory".
- **Cause:** the task's focused commands do not build the server binary that the
  Python sandbox spawns; the journey lanes do (`cargo build -p wyrd-cli -p wyrd-server --bins --tests`).
- **Fix site:** none in source; after building the binaries all three journeys pass.

### Acceptance

| Acceptance criterion | Implementation evidence | Verification evidence | Result |
|---|---|---|---|
| All SDKs audited, demonstrated redundancy fixed | commits above; shared Rust owner plus Python/TS adapters | totals table; per-kind table | PASS |
| Measured savings, totals and retained bytes | Instruments before/after traces | totals table; persistent unchanged | PASS |
| INV-001/003 owners reused, repeated retention removed | `authoring/mod.rs`, `graph/mod.rs`, `wyrd-loader`, `state.rs`, `prompt.rs` | `authoring::tests::captures_share_descendants_and_graphs_borrow_them`, `authoring::tests::shared_children_join_once_and_plain_graphs_keep_no_targets`, `authoring::tests::authored_form_is_lent_only_while_the_request_is_unchanged` (fails with the fingerprint guard removed) | PASS |
| INV-002, AC-003/005 behavior preserved | — | authoring/state/runtime preservation commands; `wyrd-spec`, `wyrd-skald-spec`, `wyrd-loader`, `wyrd-client` lib (1206 passed); three focused journeys plus TS `declare-and-pull` (19 passed) | PASS |
| REQ-005/006/007, AC-004/006 surfaces | Python `Arc<Card>` envelopes; TS borrowed projections with identical keys and order | Python hydration identity/GC tests; `test_observe_surface.py` (69 passed); TS unit (87 passed), `ts:napi:check` | PASS |
| Measurement integrity | release + line tables, signed debuggable clients | matched before/after traces retained | PASS |

Checks: `mise run fmt`, `mise run lints`, `py:format`, `py:lints`,
`py:typecheck`, `ts:lints`, `ts:typecheck`, `ts:napi:check`, `git diff --check`.
No public signature, wire, persisted, validation or dependency change; non-goals
(counters, hooks, custom allocators, benchmark framework) excluded; scratch
profiling files removed from the tree.
