# TASK-003 r6 focused follow-up review

## Immutable subject and question

- Base: `58d07d7260df1f022a721e720a28ea48e5096e35`
- Candidate: `b954976648b49429f1c0950c4fa3509573885be8`
- Conflict: `MNT-R6-001` says the PyO3 `Workflow.run` documentation retains an
  obsolete local-only headline, while the behavior, invariant, and standards
  reviews accepted the R5 public-documentation closure.
- Review boundary: source and recorded evidence only. No build, compilation,
  test, Cargo, mise, pnpm, pytest, lint, formatting, code-generation, or other
  verification command was run.

The candidate remained the stated commit throughout this follow-up.

## Source path inspected

- `sdks/wyrd-sdk-python/src/workflow.rs:204-215,537-562` — public PyO3 class
  ownership and the exported `run` method documentation and delegation.
- `sdks/wyrd-sdk-python/src/lib.rs:31-43` and
  `sdks/wyrd-sdk-python/src/workflow.rs:683-689` — registration of
  `PyWorkflow` into the native `wyrd._wyrd.agent` module.
- `sdks/wyrd-sdk-python/python/wyrd/stubs/agent.pyi:716-739` — hand-authored
  source declaration.
- `sdks/wyrd-sdk-python/python/wyrd/agent/__init__.pyi:717-740` — assembled
  public declaration.
- `crates/shared/wyrd-client/src/workflow/mod.rs` and
  `crates/shared/wyrd-client/src/workflow/local.rs` — delegated route selection,
  retained/ambient Wyrd client use, and selected external-binding preparation.
- `changes/active/skald-workflow-runtime/spec.md:272-286,2178-2180,2640-2654`
  and `tasks/TASK-003-remote-client-and-public-gateway.md:136-147,220-259` —
  approved route meanings, shared local execution, and Python consumer closure.
- `review/TASK-003-r5/TASK-003-R5-close-proof-and-contract-documentation.md:89-102,104-110,128-140,163-172,212-220` — R5 diagnosis,
  intended outcome, prescribed correction, acceptance criterion, and recorded
  closure evidence.
- `AGENTS.md:712-727` and
  `architecture/references/languages/maintainer-style.md:186-207` — accurate
  operation documentation and Python typed-contract parity rules.

## Evidence resolving the conflict

1. The runtime declaration is internally contradictory. Its headline is still
   `Run this workflow against the process-local provider registry`
   (`workflow.rs:537`), but the same doc comment immediately says selected Wyrd
   gateway steps call the server with a retained or ambient client and selected
   external-gateway steps use configured bindings (`workflow.rs:539-543`). The
   method then delegates to `ClientWorkflow::run` (`workflow.rs:559-562`), whose
   route-selected path performs exactly those non-Native operations. Under the
   approved route definitions, only a `Native` route calls the native
   process-local provider directly; `WyrdGateway` and `ExtGateway` are network
   routes. The unqualified headline is therefore materially false, not merely
   less detailed.

2. The repaired declarations expose the mismatch rather than curing it. Both
   `.pyi` files now begin `Run this workflow, preparing only what its step
   routes select`, distinguish Native, Wyrd-gateway, and external-gateway
   behavior, and state that selected secrets are read `at run start`. The PyO3
   owner retains the old headline and says only that secrets “are read,” without
   the run-start boundary. The type/signature surface is aligned, but the
   runtime method documentation and the shipped declarations do not describe
   one contract consistently.

3. The runtime text is reachable. `PyWorkflow` is a `#[pyclass]`, `run` is in
   its `#[pymethods]` block, and `register` adds that class to the public native
   `agent` module. The Rust doc comment is therefore the method help text on the
   runtime Python object, independently of the `.pyi` documentation used by
   editors and type checkers. A user can receive the stale first sentence from
   runtime introspection while receiving the corrected route-selected sentence
   from the public stub.

4. R5 explicitly prescribed editing the hand-authored stub because its
   diagnosis assumed the PyO3 owner already stated the complete route-selected
   and run-start contract. It did not separately name the owner's opening
   sentence as an edit. That omission does not make the candidate acceptable:
   R5's intended outcome requires the Python public declaration to accurately
   describe the implemented contract, and its `FIND-TASK-003-15` acceptance
   criterion requires the source declaration and assembled stub to describe
   the same Native, public-gateway, retained/ambient-client, and selected-secret
   behavior as the PyO3 owner. The recorded R5 closure compared the two stubs
   but did not resolve the owner's contradictory headline or missing timing.
   More importantly, `AGENTS.md` independently makes accurate documentation of
   every materially modified Rust method part of implementation correctness.
   Thus this is incomplete closure of the existing public-contract finding,
   not a new product requirement.

5. The user/maintainer consequence is concrete but documentation-only:
   `help(Workflow.run)` can lead a caller to treat the operation as local-only
   even though route selection can read ambient client configuration, resolve
   selected secrets, and perform authenticated network IO. It also creates two
   public descriptions that can drift independently during later maintenance.
   No runtime defect was found in this focused path.

## Proposed finding

### FUP-R6-001 — incomplete closure of the Python `Workflow.run` contract

- **Classification:** `INCORRECT` / `VIOLATION`; incomplete closure of prior
  `FIND-TASK-003-15` rather than a new runtime behavior gap.
- **Violated obligation:** materially modified method documentation must
  accurately explain operation and side effects, and Python runtime and typed
  declarations must describe the same route-selected contract.
- **Exact location:**
  `sdks/wyrd-sdk-python/src/workflow.rs:537-544`, contrasted with
  `sdks/wyrd-sdk-python/python/wyrd/stubs/agent.pyi:716-725` and
  `sdks/wyrd-sdk-python/python/wyrd/agent/__init__.pyi:717-726`.
- **Evidence:** the runtime headline says the whole Workflow runs against the
  process-local provider registry, while the method and its delegate can select
  public or external gateway routes. The runtime text also omits that selected
  external secrets are read at run start; both repaired declarations state the
  route and timing boundaries correctly.
- **Observable consequence:** runtime Python help understates configuration,
  secret-read, and network-IO side effects and disagrees with the shipped typed
  declaration.
- **Smallest ordinary correction:** replace only the PyO3 method's opening
  sentence with the existing stub wording, `Run this workflow, preparing only
  what its step routes select`, and add `at run start` to its selected-secret
  sentence. Preserve the method, signatures, runtime behavior, stubs, assembly
  pipeline, and every adjacent contract. Static source comparison plus the
  already-established ordinary documentation/code-generation verification is
  sufficient closure; add no new test harness, semantic-doc checker, setting,
  option, generator, or documentation file.

## Resolution

**RESOLVED.** `MNT-R6-001` is source-supported. The other reports correctly
observed that the hand-authored and assembled stubs now align with each other,
but their R5 closure assessment did not include the still-exported PyO3 runtime
headline. The retained issue is a bounded documentation correction at the
existing owner and introduces no unsupported mechanism or DRIFT.
