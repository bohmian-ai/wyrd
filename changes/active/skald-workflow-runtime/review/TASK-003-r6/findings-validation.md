# TASK-003 r6 findings validation

## Immutable subject and validation boundary

- Repository: `/home/thorrester/Documents/GitHub/wyrd`
- Base: `58d07d7260df1f022a721e720a28ea48e5096e35`
- Candidate: `b954976648b49429f1c0950c4fa3509573885be8`
- Approved authority: `changes/active/skald-workflow-runtime/spec.md`, Revision 12
- Original task: `changes/active/skald-workflow-runtime/tasks/TASK-003-remote-client-and-public-gateway.md`
- Prior remediation authority: TASK-003 R1 through R5 and their preserved
  ledgers and verdicts

The candidate remained the stated commit throughout validation. The repository
has no `.codegraph/` directory, so navigation used the immutable Git objects and
ordinary source search. This was a strictly source-only review: no build,
compilation, test, Cargo, mise, pnpm, pytest, lint, formatting, code generation,
test listing, or verification lane was run. Recorded evidence was inspected as
historical evidence and checked against candidate source; it was not rerun.

All required r6 discovery reports and the focused follow-up were available and
complete. The behavior, invariant, standards, system, security-domain, and
concurrency-domain reviewers proposed no findings. The maintainer reviewer
proposed `MNT-R6-001`; the focused follow-up independently traced the resulting
conflict and proposed `FUP-R6-001`. No source, caller, authority, or report
needed for validation was missing.

## Proposed-finding validation

| Discovery source | Disposition | Stable ID | Source-backed decision |
|---|---|---|---|
| `MNT-R6-001` | **REVISED** | `FIND-TASK-003-15` | The public PyO3 `Workflow.run` method still opens with a false local-only summary even though the same doc block and delegated implementation cover public and external gateway routes. This is incomplete closure of the existing Python public-contract finding, not a new finding. |
| `FUP-R6-001` | **REVISED, CONSOLIDATED** | `FIND-TASK-003-15` | The follow-up correctly proves the runtime method documentation is reachable through the public package and disagrees with both shipped declarations. Its correction is consolidated with the maintainer claim at the existing PyO3 owner. |

Agreement between these two reports was not treated as proof. The validation
below independently inspected the complete method body, its shared-client
delegate, PyO3 registration, public Python re-export, both declaration
consumers, the prior finding and R5 remediation, and the governing documentation
rules.

## Final deduplicated ledger

### FIND-TASK-003-15 — finish the public Python `Workflow.run` contract repair

- **Status:** REOPENED / REVISED
- **Classification:** INCORRECT / VIOLATION
- **Discovery sources:** `MNT-R6-001`, `FUP-R6-001`
- **Violated obligation:** Revision 12 makes Python `Workflow.run` a projection
  of the shared client Workflow and requires affected Python exports and
  declarations to move with that behavior. `AGENTS.md` section 16 and
  `architecture/agent-rules.md` make accurate operation-level rustdoc part of
  implementation correctness, and Maintainer Style requires the public Python
  declaration and documentation to describe the same operation. R5's intended
  outcome was one accurate Python public contract, not merely two matching
  `.pyi` copies.
- **Exact locations:**
  `sdks/wyrd-sdk-python/src/workflow.rs:537-544`, contrasted with
  `sdks/wyrd-sdk-python/python/wyrd/stubs/agent.pyi:716-725` and
  `sdks/wyrd-sdk-python/python/wyrd/agent/__init__.pyi:717-726`.
- **Producer-to-consumer trace:** `PyWorkflow` stores the shared
  `ClientWorkflow` at `sdks/wyrd-sdk-python/src/workflow.rs:204-215`.
  `PyWorkflow::run` is in its `#[pymethods]` block and delegates directly to
  `self.inner.run(input)` at lines 559-562. The shared owner selects routes,
  may load ambient client configuration, resolves only selected external
  bindings at run start, and dispatches through Native, public Wyrd gateway, or
  external gateway dependencies at
  `crates/shared/wyrd-client/src/workflow/mod.rs:99-160` and
  `workflow/local.rs:39-110`. `workflow::register` adds this class to the
  native `agent` module at `sdks/wyrd-sdk-python/src/workflow.rs:679-688`, the
  extension entry point registers that module at
  `sdks/wyrd-sdk-python/src/lib.rs:43-52`, and the public package re-exports the
  native `Workflow` at
  `sdks/wyrd-sdk-python/python/wyrd/agent/__init__.py:7-16`. The method rustdoc
  is therefore the public runtime help text, not a dormant internal comment.
- **Contradicting source:** the runtime method begins, “Run this workflow
  against the process-local provider registry,” then immediately describes
  selected Wyrd and external gateway routes. Both shipped `.pyi` declarations
  instead begin, “Run this workflow, preparing only what its step routes
  select,” explicitly reserve the process-local registry for Native steps, and
  state that selected external secrets are read at run start. The runtime
  headline is false for the public and external route cases, and its selected-
  secret sentence omits the run-start timing now documented by both sibling
  public declarations.
- **Reachability and observable consequence:** any public Python caller can
  inspect `Workflow.run` through the re-exported native class. Runtime help can
  therefore tell the caller the operation is local-only while the typed public
  declaration correctly discloses conditional authenticated network IO,
  ambient configuration, and selected secret reads. The contradiction is on a
  materially modified public method and can mislead callers about side effects;
  it is not an unrelated old comment or a wording preference.
- **Smallest safe correction:** at the existing PyO3 method only, replace the
  opening sentence with the already-shipped stub sentence, “Run this workflow,
  preparing only what its step routes select,” and change the selected-secret
  sentence to say the secrets “are read, at run start.” Preserve the method,
  signatures, shared runtime delegation, route behavior, stubs, assembly
  pipeline, and every adjacent contract. Do not add a semantic-documentation
  checker, new test harness, generator, declaration file, setting, option, or
  runtime mechanism.
- **Focused closure proof:** statically compare the PyO3 method documentation
  with the hand-authored and assembled public declarations and with the shared
  `Workflow::run` route behavior. Use only the repository's ordinary existing
  documentation/lint, Python type, and generated-declaration drift checks for
  the touched surface; no new documentation test is justified.

### Ponytail ladder

1. **Delete:** deleting the public method documentation would violate the
   repository's mandatory documentation rule and would hide the method's
   non-obvious IO and secret-read behavior.
2. **Reuse existing repository behavior:** the correct wording already exists
   in both shipped `.pyi` declarations; reuse it at the PyO3 owner.
3. **Standard/native platform:** ordinary PyO3 method documentation already
   exposes runtime help. No additional platform mechanism is needed.
4. **Installed dependency:** no dependency is needed.
5. **Minimum correction:** two sentence-level edits in the existing doc block.

The consumer guard belongs at the PyO3 method because that item is itself the
public runtime documentation source. Moving the wording into another helper or
adding a synchronization mechanism would not repair `help(Workflow.run)` and
would introduce unsupported DRIFT.

## Prior-finding closure

| Stable finding | Validation result in candidate `b954976648b49429f1c0950c4fa3509573885be8` |
|---|---|
| `FIND-TASK-003-1` | **CLOSED.** `PyWorkflow` retains the complete `ClientWorkflow` through registered/authored loading, mutation, and `run`; the recorded public Python journey covers hostile and absent ambient client configuration. |
| `FIND-TASK-003-2` | **CLOSED.** `HttpTransport::post_native` sends once, branches on the observed `401`, awaits the existing `force_refresh` owner before body collection, propagates renewal failure, and never reconstructs or resends the model request. Recorded complete, cut-off, and pending-body cases correspond to that source ordering. |
| `FIND-TASK-003-3` | **CLOSED.** `Ingress::problem` ignores envelope message text; recognized codes use catalog title/remediation and uncoded statuses use fixed provider categories. |
| `FIND-TASK-003-4` | **CLOSED.** `read_secret_file` is private and the shared cross-module API returns `SecretString` through `read_secret_ref`. |
| `FIND-TASK-003-5` | **CLOSED.** The previously identified function-local imports remain in owning module or test-module import blocks. |
| `FIND-TASK-003-6` | **CLOSED.** Changed declarations use module-imported bare or role-specific aliased types. |
| `FIND-TASK-003-7` | **CLOSED.** The identified task-owned panic-capable helpers and tests retain substantive `# Panics` contracts. |
| `FIND-TASK-003-8` | **CLOSED.** Remote create/cancel, native POST, public gateway caller, and shared run operations document caller drop, retry/idempotency, and possible post-dispatch progress at their owners. |
| `FIND-TASK-003-9` | **CLOSED.** The focused transport source waits until the model route is observed, cancels the pending call, and asserts no resend. |
| `FIND-TASK-003-10` | **CLOSED.** `load_local_setup` performs at most one `GlobalConfig::load` and derives both mixed-route consumers from it; the R5 recorded exact caller-level proof traverses `Workflow::run_with` and the public Python retained-client rerun remains recorded. |
| `FIND-TASK-003-11` | **CLOSED.** `AuthMiddleware::force_refresh` distinguishes replay-safe transport retry from native send-once renewal for later calls. |
| `FIND-TASK-003-12` | **CLOSED.** `Workflow::into_skald` states that it discards the retained loading client and automatic shared dependency composition. |
| `FIND-TASK-003-13` | **CLOSED.** `architecture/wyrd-design.md:434-443` records the fallback-header encoding, bounds, authentication order, refusal, consumption/non-forwarding, and absent-header behavior. |
| `FIND-TASK-003-14` | **CLOSED.** The shared `Workflow::run`, `run_with`, `SelectedRoutes::dependencies`, and `resolve_binding` documentation now states the relevant ambient IO, run-start, cancellation, and partial-progress boundaries. |
| `FIND-TASK-003-15` | **REOPENED / REVISED.** The source and assembled stubs were corrected, but the reachable PyO3 runtime method retains the obsolete local-only headline and omits the shared run-start timing phrase. This is incomplete closure at the same public-contract owner and keeps the stable ID. |

## Rejected proposals and DRIFT assessment

No r6 discovery proposal was rejected; `MNT-R6-001` and `FUP-R6-001` describe
one source-supported issue and are consolidated under the prior stable ID.
The other six discovery reports proposed an explicitly empty union, which
source inspection did not contradict.

No additional mechanism, check, file, setting, or option survives validation as
a required correction. The candidate otherwise reuses established shared
Workflow, PyO3, transport/auth, configuration, secret, typed-header, and stub
assembly owners. The retained correction uses ordinary public-method
documentation and wording already shipped in the sibling declarations. A new
semantic-doc checker, runtime introspection test, generator, synchronization
layer, or declaration source would be absent from established repository and
widely used project practice and would therefore be DRIFT under the standing
human direction.

## Validation result and evidence limits

The final deduplicated ledger contains one bounded finding:
`FIND-TASK-003-15`. Its correction changes no product behavior, public API,
architecture, security contract, compatibility boundary, concurrency semantic,
resource owner, or persistent data decision. No unresolved reviewer conflict or
missing caller trace remains.

All verification references are recorded historical evidence from TASK-003 and
R1-R5. This reviewer did not execute them. The finding is established directly
by contradictory candidate source and public callers; its closure is
documentation-only and can be proved through source parity plus the existing
ordinary checks without inventing a new proof mechanism.
