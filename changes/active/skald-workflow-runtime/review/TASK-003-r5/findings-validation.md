# TASK-003 r5 structured finding validation

## Immutable subject and validation boundary

- Repository: `/home/thorrester/Documents/GitHub/wyrd`
- Base: `58d07d7260df1f022a721e720a28ea48e5096e35`
- Candidate: `5084b0e5b30fe79fdb2468ad7d0fa3a9d2106069`
- Approved authority: `changes/active/skald-workflow-runtime/spec.md`, Revision 12
- Original task: `changes/active/skald-workflow-runtime/tasks/TASK-003-remote-client-and-public-gateway.md`
- Cumulative remediation authority: TASK-003 R1 through R4 and their validated ledgers

The candidate was the repository `HEAD` when validation began and remained the
stated commit at final inspection. The human-approved minimal
`WyrdGatewayCall.model: ModelRef` amendment, the R1 native-`401` wording
correction, and the approved run-start `spawn_blocking` boundary were applied
as authority. Configuration, client, and selected-secret reads remain at run
start because Workflow loading must read no execution secret.

This was a strictly source-only validation. No build, compilation, test, lane,
Cargo, mise, pnpm, pytest, formatter, linter, typecheck, code-generation,
diff-check, or other verification command was run. The actual cumulative diff,
current source, governing documents, task packets, prior ledgers, recorded
evidence, all r5 discovery reports, and the focused follow-up were inspected.
The repository has no `.codegraph/` directory, so ordinary source navigation
was used.

## Proposal disposition

| Discovery proposal | Disposition | Stable finding | Validation result |
|---|---|---|---|
| `BEH-TASK003-R5-1` | **CONFIRMED** | `FIND-TASK-003-10` | R4 expressly requires one client-less mixed-route `Workflow` to exercise the actual selected-route/run-start path. The new fixture supplies `(true, true, None)` directly to `local_setup_from` and therefore assumes rather than proves the route-selection and owner wiring. |
| `INV-R5-001` | **CONFIRMED** | `FIND-TASK-003-10` | The production source now has one snapshot, but the required producer-to-consumer regression and exact recorded selector/count remain absent. This is unfinished closure of the existing finding, not a new runtime defect. |
| `SYS-R5-001` | **CONFIRMED** | `FIND-TASK-003-10` | The helper case proves pure derivation from a supplied snapshot, not the reachable run path whose recovery/configuration invariant R4 required. No new recovery mechanism is warranted. |
| `FU-TASK003-R5-1` | **CONFIRMED** | `FIND-TASK-003-10` | The follow-up correctly resolves the discovery conflict: source correction is present; direct closure proof and its prescribed evidence record are not. |
| `STD-TASK-003-R5-01` | **CONFIRMED** | `FIND-TASK-003-13` | Revision 12 `INV-015` explicitly requires the Wyrd gateway architecture authority to record the authenticated fallback header. The implementation and served operations do, but the active architecture does not. |
| `STD-TASK-003-R5-02` | **CONFIRMED, CONSOLIDATED** | `FIND-TASK-003-14` | The new async `Workflow::run`, `SelectedRoutes::dependencies`, and `resolve_binding` omit cancellation/partial-progress behavior required by AGENTS.md and `agent-rules.md`. Consolidated with `MNT-001` because one run-start documentation boundary owns all four corrections. |
| `MNT-001` | **CONFIRMED, CONSOLIDATED** | `FIND-TASK-003-14` | `Workflow::run_with` says ambient configuration is loaded only for `ext_gateway`; source also loads it for a selected client-less `wyrd_gateway`. This is a concrete contradiction, not a wording preference. |
| `MNT-002` | **CONFIRMED** | `FIND-TASK-003-15` | The changed Python runtime can perform authenticated public-gateway IO and selected external-secret reads, but the shipped `Workflow.run` stub still describes the old process-local-only contract. |

The concurrency and security reviewers' acceptance of the corrected runtime
does not contradict the retained ledger: no runtime split-snapshot, auth,
tenancy, cancellation, or credential defect remains. Their conclusion that
`FIND-TASK-003-10` was fully closed conflicts only with R4's explicit proof
contract and is resolved by the focused follow-up and the source trace below.

## Validated finding ledger

### FIND-TASK-003-10 — complete the required mixed-route Workflow proof

- **Status:** CONFIRMED (prior stable finding remains open for closure proof)
- **Classification:** MISSING
- **Discovery sources:** `BEH-TASK003-R5-1`, `INV-R5-001`, `SYS-R5-001`, `FU-TASK003-R5-1`
- **Violated obligation:** TASK-003 R4 requires static proof of one snapshot and
  a focused test in which one client-less `Workflow` selects both
  `ext_gateway` and `wyrd_gateway` through the actual run-start owner. It also
  requires the exact named selector and selected count to be recorded. The
  standing review direction treats missing or contradicted evidence as a
  finding to rerun.
- **Exact locations:**
  `crates/shared/wyrd-client/src/workflow/mod.rs:131-149,179-240,582-598,600-758,783-826`
  and
  `changes/active/skald-workflow-runtime/review/TASK-003-r4/TASK-003-R4-use-one-run-start-config-snapshot.md:118-159,162-178`.
- **Producer-to-consumer evidence:** `Workflow::run_with` is the live producer
  of `SelectedRoutes`, `needs_config`, and `needs_gateway`, and it calls
  `load_local_setup` on the approved blocking boundary. `load_local_setup`
  now performs at most one `GlobalConfig::load`, and `local_setup_from` derives
  both `global.workflow` and the client configuration from that value. The
  source defect is therefore corrected. The new
  `mixed_routes_use_one_config_snapshot` test constructs no `Workflow`, calls
  neither `SelectedRoutes::of` nor `Workflow::run_with`, bypasses
  `load_local_setup`, and manually supplies the two flags. The older named
  test exercises an already-configured external-only Workflow and a separate
  retained-client gateway Workflow. Neither covers the required client-less
  mixed route. R4 records a five-test union, not its prescribed exact selector
  and selected count.
- **Observable consequence:** route discovery, need-flag propagation, the
  blocking owner call, or consumption of either returned dependency can
  regress while the helper-only test remains green. The corrected runtime is
  presently source-sound, but the repeatedly remediated invariant lacks the
  acceptance proof R4 made mandatory.
- **Decision-complete minimum correction:** reuse the existing Workflow/config/
  gateway fixtures and extend or replace the named selected-dependencies proof
  so one client-less mixed-route `Workflow` reaches the shared `run_with`
  run-start path and demonstrates that both selected external bindings and the
  public-gateway client come from its single ambient snapshot. Preserve the
  current production correction, retained-client precedence, native-only
  laziness, and selected-secret-only behavior. Add no production hook, timing
  assertion, cache, generation token, setting, fixture system, checker, or new
  dependency.
- **Focused closure proof:** record the exact R4 command for
  `workflow::tests::selected_local_dependencies_use_shared_config` and its
  selected count, plus the already-required public Python retained-client
  journey. Source inspection must still show one `GlobalConfig::load` feeding
  both mixed-route consumers.

The correction uses existing owners and test infrastructure and requires no
new behavior or expensive-to-reverse decision.

### FIND-TASK-003-13 — synchronize the authenticated fallback header into active architecture

- **Status:** CONFIRMED
- **Classification:** VIOLATION
- **Discovery source:** `STD-TASK-003-R5-01`
- **Violated obligation:** Revision 12 `INV-015` requires the Wyrd gateway
  architecture authority and generated public operations to record the
  authenticated fallback header before implementation completes. AGENTS.md
  makes `architecture/wyrd-design.md` the active design authority for public
  and internal contracts.
- **Exact locations:** implemented contract at
  `crates/wyrd-spec/src/gateway/policy.rs:126-184` and
  `crates/wyrd/wyrd-server/src/components/gateway/ingress.rs:189-230`; missing
  authority in `architecture/wyrd-design.md`'s gateway contract.
- **Producer-to-consumer evidence:** `GatewayFallbackOverride` serializes the
  `wyrd-gateway-fallback` value and the authenticated public ingress consumes
  and validates it for the existing protocol routes. Served OpenAPI describes
  it. The active design and security authorities contain no equivalent header
  contract, despite the spec's explicit synchronization invariant.
- **Observable consequence:** the source-of-truth architecture does not state
  the encoding, bounds, authentication order, consumption/non-forwarding
  boundary, rejection behavior, or absent-header compatibility of a shipped
  security-relevant request surface.
- **Decision-complete minimum correction:** update the existing gateway section
  of `architecture/wyrd-design.md` to record the implemented optional
  authenticated `wyrd-gateway-fallback` contract: unpadded base64url over JCS
  UTF-8 `GatewayFallbackOverride`, 8 KiB encoded and 4 KiB decoded limits,
  authentication before parsing, invalid-request refusal before dispatch,
  consume-without-forwarding behavior, and unchanged tenant-policy behavior
  when absent. Do not add another design file, checker, setting, compatibility
  path, or runtime mechanism.
- **Focused closure proof:** source inspection must show the active architecture,
  typed contract, ingress, and served operation describe the same boundary.

This is synchronization of already approved behavior, not a specification
revision.

### FIND-TASK-003-14 — make the shared run-start documentation accurate and complete

- **Status:** REVISED
- **Classification:** INCORRECT / VIOLATION
- **Discovery sources:** `STD-TASK-003-R5-02`, `MNT-001`
- **Violated obligation:** AGENTS.md section 16 and
  `architecture/agent-rules.md` require every new or materially modified Rust
  item, including private async helpers, to document relevant side effects,
  cancellation, and partial progress. Maintainer guidance also prohibits a
  public contract that contradicts its implementation.
- **Exact locations:**
  `crates/shared/wyrd-client/src/workflow/mod.rs:99-130` and
  `crates/shared/wyrd-client/src/workflow/local.rs:72-138`.
- **Producer-to-consumer evidence:** `Workflow::run` delegates to `run_with` and
  can dispatch a model call, but documents no drop/partial-progress boundary.
  `run_with` documents dispatch cancellation, yet says configuration is loaded
  only for `ext_gateway`; `load_local_setup` also loads it for a selected
  client-less `wyrd_gateway`. `SelectedRoutes::dependencies` resolves selected
  bindings sequentially, and `resolve_binding` starts blocking secret reads.
  Dropping either future can follow earlier completed reads, and a currently
  running `spawn_blocking` read can finish after the async future is dropped.
  Their rustdoc records errors but not those applicable cancellation facts.
- **Observable consequence:** Rust callers are told the wrong ambient IO
  trigger, and maintainers cannot determine from each async owner which reads
  may have completed or outlived cancellation. That obscures the exact
  run-start boundary deliberately established by R3/R4.
- **Decision-complete minimum correction:** correct `run_with` to name both
  ambient configuration triggers: selected external bindings, and a selected
  public Wyrd gateway without a retained loading client. Give `Workflow::run`
  the same applicable local-drop/already-dispatched contract as `run_with`.
  Document on `dependencies` and `resolve_binding` that prior reads may have
  completed and an already-started blocking read may finish after drop, while
  no model dispatch occurs during dependency preparation. Preserve all runtime
  behavior and the approved blocking boundary; add no cancellation mechanism,
  option, checker, or test harness.
- **Focused closure proof:** static inspection of each operation-local rustdoc
  plus the repository's ordinary documentation/lint evidence. No behavioral
  test is needed because the correction changes no runtime semantics.

The two discovery claims share one owning run-start documentation boundary and
are consolidated rather than creating parallel remediation findings.

### FIND-TASK-003-15 — update the shipped Python `Workflow.run` contract

- **Status:** CONFIRMED
- **Classification:** MISSING / VIOLATION
- **Discovery source:** `MNT-002`
- **Violated obligation:** Revision 12 requires affected Python exports and
  stubs to move with the shared Workflow behavior, and AGENTS.md section 8
  requires Python-visible changes to regenerate cleanly from their source
  declaration.
- **Exact locations:** runtime documentation at
  `sdks/wyrd-sdk-python/src/workflow.rs:537-562`; stale source declaration at
  `sdks/wyrd-sdk-python/python/wyrd/stubs/agent.pyi:716-734`; generated public
  declaration at
  `sdks/wyrd-sdk-python/python/wyrd/agent/__init__.pyi:717-735`.
- **Producer-to-consumer evidence:** `PyWorkflow` now owns the shared
  `wyrd_client::Workflow`, retains a loading client, and delegates `run` to
  that owner. Its runtime docs state that selected `wyrd_gateway` routes can
  perform authenticated server IO and selected `ext_gateway` routes resolve
  configured secrets at run start. The hand-authored source stub still says
  only that the method runs against the process-local registry and that
  dependencies merely order steps. `assemble_stubs.py` copies that stale
  source into the public package stub, so a green drift check cannot establish
  semantic parity with an unchanged source declaration.
- **Observable consequence:** Python users and maintainers cannot discover from
  the shipped typed surface that `Workflow.run()` may read ambient
  configuration, resolve selected execution secrets, call the public Wyrd
  gateway, or reuse the loading client's endpoint and credential context.
- **Decision-complete minimum correction:** update the hand-authored
  `python/wyrd/stubs/agent.pyi` source documentation to match the existing
  PyO3 owner: native routes use the process registry; public gateway routes use
  the retained or ambient Wyrd client; external routes resolve only selected
  configured secrets at run start. Regenerate the public stub through the
  existing `assemble_stubs.py` owner. Do not hand-edit the assembled output or
  add another generator, checker, declaration layer, or runtime behavior.
- **Focused closure proof:** the existing code-generation drift check and
  Python public typecheck, with source inspection that the source and assembled
  declarations match the runtime contract.

## Prior-finding closure

| Stable finding | Validation result |
|---|---|
| `FIND-TASK-003-1` | **CLOSED.** Shared and Python Workflows retain the loading client through load, mutation, and run; the recorded Python journey covers hostile and absent ambient configuration. |
| `FIND-TASK-003-2` | **CLOSED.** One native model POST is sent; renewal begins on known `401` before body collection, no replay occurs, and renewal failure has precedence. |
| `FIND-TASK-003-3` | **CLOSED.** Recognized codes use catalog-owned title and remediation; provider-controlled text is discarded. |
| `FIND-TASK-003-4` | **CLOSED.** The plaintext file reader is private and cross-module consumers receive `SecretString`. |
| `FIND-TASK-003-5` | **CLOSED.** The identified function-local imports remain removed. |
| `FIND-TASK-003-6` | **CLOSED.** Changed declarations use imported bare or role-specific aliased types. |
| `FIND-TASK-003-7` | **CLOSED.** The previously identified panic-capable items retain substantive panic contracts. |
| `FIND-TASK-003-8` | **CLOSED.** The exact remote create/cancel, native POST, public caller, and `run_with` operations identified in r2 document their post-dispatch cancellation boundaries. `FIND-TASK-003-14` covers different newly added operations and the later R4 documentation contradiction. |
| `FIND-TASK-003-9` | **CLOSED.** Recorded focused evidence cancels after model dispatch and observes no resend. |
| `FIND-TASK-003-10` | **OPEN FOR REQUIRED PROOF.** The runtime producer is corrected and the Python retained-client rerun is recorded; the mandated client-less mixed-route Workflow proof and exact selector/count are absent. |
| `FIND-TASK-003-11` | **CLOSED.** Authentication-owner rustdoc distinguishes retrying transport consumers from native send-once renewal for later calls. |
| `FIND-TASK-003-12` | **CLOSED.** `Workflow::into_skald` documents the loading-client and automatic dependency-composition context it discards. |

## Ponytail and DRIFT assessment

The retained corrections stop at existing owners: the selected Workflow test,
the active gateway architecture section, existing Rust rustdoc, and the
existing Python source-stub/assembly pipeline. They require no new abstraction,
trait, option, configuration mechanism, cache, watcher, generation token,
fixture system, check, dependency, or compatibility surface.

`local_setup_from` is not independently retained as a finding: it separates a
single filesystem read from deterministic derivation and is used by the
run-start owner and its focused lower-level proof. Deleting it would not close
the missing caller-path proof, while demanding generation tracking or a timing
hook would add nonstandard machinery expressly forbidden by R4. The fallback
header, `Workflows` facade, public gateway caller, shared secret reader, and
language projections are approved capabilities or established repository
owners rather than speculative extension points. No additional qualifying
DRIFT was found under the standing rule.

## Validation completeness

Every r5 proposal was checked against its cited source, callers or generated
consumer, and governing authority. The mixed-route conflict was resolved by
the required focused follow-up. The final deduplicated ledger contains
`FIND-TASK-003-10`, `FIND-TASK-003-13`, `FIND-TASK-003-14`, and
`FIND-TASK-003-15`. All four have bounded corrections within approved behavior.
None requires a new product, public API, architecture choice, security model,
compatibility contract, concurrency semantic, resource owner, or persistent
data decision; `SPEC_REVISION_REQUIRED` is not indicated.
