# Independent findings validation — TASK-003 r4

## Subject and evidence boundary

- Repository: `/home/thorrester/Documents/GitHub/wyrd`
- Base: `58d07d7260df1f022a721e720a28ea48e5096e35`
- Candidate: `cfd60f6442e4dbdfcbc57cfec0bde494a225ae89`
- Approved specification:
  `changes/active/skald-workflow-runtime/spec.md`, Revision 12
- Original task:
  `changes/active/skald-workflow-runtime/tasks/TASK-003-remote-client-and-public-gateway.md`
- Prior remediations and validated ledgers: `TASK-003-r1`, `TASK-003-r2`, and
  `TASK-003-r3`

The human-approved 2026-10-03 addition of `pub model: ModelRef` to
`WyrdGatewayCall`, the R1 same-spec native-`401` task correction, and the
approved use of `spawn_blocking` for run-start configuration, client, and
secret reads were treated as authority. Run-start preparation remains the
required boundary because Workflow loading must resolve no execution secrets.

This fresh validation inspected the complete cumulative diff, current source,
the task and all three remediations, applicable repository and architecture
authority, all prior validated ledgers and verdicts, the implementer's recorded
evidence, and every r4 discovery report. It traced each proposed failure from
producer through live callers and sibling consumers. Agreement among discovery
reviewers was corroboration, not proof.

The review was strictly source-only. No build, compile, test, Cargo, mise,
pnpm, pytest, formatter, linter, package-manager, test-listing, or other
verification command was run. Implementer evidence is accepted only for the
source path it actually exercises; missing or source-contradicted evidence is
part of the validated finding below. CodeGraph was unavailable because this
repository has no `.codegraph/` index.

No focused follow-up report was needed: the discovery claims do not conflict,
and current source fully exposes the repeated-remediation source and both live
call chains. All required discovery reports were available.

## Disposition of every proposed finding

| Discovery source ID | Disposition | Final ID | Source-backed decision |
|---|---|---|---|
| `BEH-R4-001` | **CONFIRMED** | `FIND-TASK-003-10` | A client-less mixed-route run reads `GlobalConfig` once for external bindings and again through `WyrdClient::from_global`. |
| `INV-R4-001` | **REVISED** | `FIND-TASK-003-10` | The runtime defect is confirmed. Its missing retained-client/language rerun is required closure evidence for this same R3 correction, not a second source defect or a new stable finding. |
| `STD-R4-001` | **REVISED** | `FIND-TASK-003-10` | R3 expressly required retained-client/language coverage after changing shared `Workflow::run_with`. The missing Python rerun remains mandatory proof, but it closes the same changed run-start owner and does not justify `FIND-TASK-003-13`. |
| `MAINT-TASK-003-R4-1` | **CONFIRMED** | `FIND-TASK-003-10` | The hidden second filesystem boundary defeats the helper's stated single setup role and the selected one-snapshot correction. |
| `SYS-R4-001` | **CONFIRMED** | `FIND-TASK-003-10` | Atomic configuration replacement can compose one run from two generations or make only the second read fail. |
| `SEC-R4-001` | **CONFIRMED** | `FIND-TASK-003-10` | The two snapshots can split external credential/binding context from the Wyrd endpoint, tenant, and cache context. |
| `CONC-R4-001` | **CONFIRMED** | `FIND-TASK-003-10` | The mixed path is reachable and violates the approved coherent run-start snapshot boundary. |

No discovery proposal was rejected wholesale. Validation narrows all seven
reports to one producer defect and one stable finding. In particular, missing
language proof is not duplicated into a proof-only stable finding; it is part
of the focused closure proof for the runtime correction that changed the
shared language path.

## Validated finding ledger

### FIND-TASK-003-10 — derive mixed local dependencies from one ambient configuration snapshot

- **Status:** REOPENED / REVISED
- **Classification:** INCORRECT / VIOLATION
- **Discovery sources:** `BEH-R4-001`, `INV-R4-001`, `STD-R4-001`,
  `MAINT-TASK-003-R4-1`, `SYS-R4-001`, `SEC-R4-001`, and `CONC-R4-001`
- **Correction route:** bounded implementation and recorded focused proof

**Violated obligation.** The R3 correction for this stable finding requires
the shared Workflow preparation owner to move all task-added ambient
filesystem work to the established blocking pool, load one ambient
`GlobalConfig` snapshot when selected routes require it, and reuse the current
`GlobalConfig`, `ClientConfig`, and `WyrdClient` owners to derive both Workflow
bindings and any client-less public-gateway client. R3 also requires the
selected-local-dependencies proof and retained-client/language coverage to be
rerun and recorded. This preserves TASK-003's one shared selected-dependency
path, Revision 12's retained client context, and AGENTS.md's async and
shared-client boundaries.

**Exact locations.** The producer is
`crates/shared/wyrd-client/src/workflow/mod.rs:190-210`. Its two filesystem
paths continue through `crates/shared/wyrd-client/src/client.rs:47-54` and
`crates/shared/wyrd-client/src/config.rs:91-103`; the actual file read is
`crates/shared/wyrd-client/src/global_config.rs:71-105`. The incomplete R3
evidence is recorded at
`changes/active/skald-workflow-runtime/review/TASK-003-r3/TASK-003-R3-close-pending-renewal-and-async-context-contracts.md:120-132,148-162`.

**Producer-to-consumer proof.** `Workflow::run_with` computes
`SelectedRoutes` and passes independently computed `needs_config` and
`needs_gateway` flags plus the retained client into one blocking task. A
wholly local or file-loaded Workflow has no retained client. Per-step routing
allows that Workflow to contain both an `ext_gateway` step and a
`wyrd_gateway` step, so both flags can be true.

For `load_local_setup(true, true, None)`, lines 195-196 call
`GlobalConfig::load()` and immediately move only `.workflow` into the external
binding configuration. Lines 200-208 then call `WyrdClient::from_global()`.
That constructor calls `ClientConfig::from_global()`, which calls
`GlobalConfig::load()` again. The first snapshot therefore supplies external
origins and secret references while the second supplies the Wyrd HTTP/gRPC
endpoint, tenant, token-cache settings, and ambient credential assembly.

`SelectedRoutes::dependencies` at `workflow/local.rs:83-104` consumes these
already-split values. It cannot restore coherence: it resolves the selected
external bindings from the first value and installs the client assembled from
the second. Rust and CLI call the shared facade directly; Python's
`PyWorkflow::run` at `sdks/wyrd-sdk-python/src/workflow.rs:537-562` delegates
to the same owner, and its loaded facade retains the loading client through
`state/mod.rs:2635-2638`. The registered/retained-client branch correctly
avoids ambient client assembly, so the defect is specifically the live
client-less mixed-route path rather than every run.

**Observable consequence.** If `config.toml` is replaced between reads, one
run can resolve external gateway bindings under configuration A while sending
its governed Wyrd call under configuration B's endpoint, tenant, or cache
context. A valid first read can also be followed by a transient second-read or
parse failure that refuses the run. Even without a concurrent update, the
candidate performs the duplicate filesystem read that the approved R3
correction required the shared owner to eliminate.

**Evidence assessment.** R3 correctly proves that setup now executes on
Tokio's blocking pool, closes pending-body native renewal, and updates the two
owner contracts. Its selected-dependencies test does not exercise this
producer path: external-route cases pass an already-parsed
`LocalWorkflowConfig` directly to `SelectedRoutes::dependencies`, while the
gateway case supplies an explicit retained client. The recorded R3 command
also omits the existing public Python retained-client integration that R3
expressly required after changing `Workflow::run_with`. The earlier R1/R2
Python result predates the R3 run-start boundary and is not evidence for the
current candidate. These proof gaps belong to closure of this finding; they do
not establish a second implementation defect.

**Ponytail ladder and decision-complete minimum correction.** Delete the
second ambient configuration load from the existing `load_local_setup` stage.
When selected external bindings or a client-less selected Wyrd gateway need
ambient state, load at most one `GlobalConfig`. Derive the Workflow section and
the client configuration from that same value using the existing
`ClientConfig::from_global_with_env`, then assemble the client through the
existing `WyrdClient::with_config`. Reuse a retained Cards client without
ambient client assembly. No standard-library facility or installed dependency
is missing, and no new abstraction is justified: the repository already owns
every required constructor.

Preserve route-first laziness, no ambient read for a purely native run, no
ambient client assembly for a retained-client gateway run, selected-secret-only
resolution, current public error mapping, run-start timing, and the approved
`spawn_blocking` boundary. Do not add a cache, watcher, async configuration API,
configuration hook, setting, option, dependency, compatibility path, checker,
allowlist, new harness, timing test, or synthetic slow-filesystem fixture.

**Focused closure proof.** Static source inspection must show one
`GlobalConfig::load` feeding both consumers on the client-less mixed-route
path. Extend or reuse the existing selected-local-dependencies proof to cover
one Workflow selecting both route families without a retained client, then
rerun and record its exact focused selector. Rerun and record the already-
existing public Python integration
`test_loaded_workflow_calls_the_gateway_through_its_loading_client` through its
repository-managed setup, because it is the existing language/runtime proof
directly sensitive to the changed shared owner. Preserve the remote-client and
auth regressions required by R3. No new test file, generalized harness,
filesystem timing assertion, TypeScript-specific duplicate runtime, or
permanent check is required.

## Prior-finding closure

| Stable finding | Validation result |
|---|---|
| `FIND-TASK-003-1` | **CLOSED IN SOURCE.** Shared and Python Workflows retain the loading client through registered/authored-ref loading, authoring mutation, and run. The required post-R3 Python rerun is accounted for as closure proof under `FIND-TASK-003-10`, not as a reopened client-retention defect. |
| `FIND-TASK-003-2` | **CLOSED.** `post_native` sends one model POST, begins `force_refresh` after the known `401` status and before body collection, never replays, and has recorded pending/complete/truncated-body evidence. |
| `FIND-TASK-003-3` | **CLOSED.** Recognized native codes take message and remediation from the Wyrd catalog; arbitrary envelope text is discarded. |
| `FIND-TASK-003-4` | **CLOSED.** The plaintext file reader is private; cross-module consumers receive `SecretString` from the shared `read_secret_ref` owner. |
| `FIND-TASK-003-5` | **CLOSED.** The r1 function-local imports remain in their owning module or test-module import blocks. |
| `FIND-TASK-003-6` | **CLOSED.** The changed declarations use module-imported bare or role-specific aliased types. |
| `FIND-TASK-003-7` | **CLOSED.** The identified panic-capable new tests and helpers retain substantive `# Panics` contracts. |
| `FIND-TASK-003-8` | **CLOSED.** Remote create/cancel, native POST, public gateway calls, and shared local execution document cancellation and possible post-dispatch progress. |
| `FIND-TASK-003-9` | **CLOSED.** Recorded focused evidence cancels only after model dispatch is observed and confirms no resend. |
| `FIND-TASK-003-10` | **REOPENED / REVISED.** Blocking placement is correct, but client-less mixed routes still read two ambient configuration snapshots and the mandated retained-client/language rerun is absent. |
| `FIND-TASK-003-11` | **CLOSED.** `force_refresh` rustdoc distinguishes replay-safe retries from native send-once renewal for later calls. |
| `FIND-TASK-003-12` | **CLOSED.** `Workflow::into_skald` documents the retained client and automatic dependency-composition context it discards. |

## Rejected and narrowed alternatives

### A second proof-only stable finding

The missing post-R3 Python rerun is real and blocks acceptance, but it is the
explicit closure proof attached to the same R3 finding that changed
`Workflow::run_with`. It has no independent producer, consequence, or source
correction. Assigning `FIND-TASK-003-13` would split one correction from its
required proof and invite separate remediation rounds. It is therefore revised
into `FIND-TASK-003-10` rather than retained separately.

### New configuration consistency machinery

A cache, watcher, generation token, lock, reload protocol, async file API,
configurable snapshot option, or filesystem timing fixture is unnecessary.
The defect is two calls to an existing loader in one synchronous setup stage;
the existing borrowed-snapshot client constructor closes it directly. These
alternatives add mechanisms absent from both Wyrd's established path and
ordinary client configuration composition, so requiring them would be DRIFT.

### Downstream consistency guards

Adding equality checks or version guards in `SelectedRoutes::dependencies`,
the public gateway caller, Python, TypeScript, or Skald would duplicate state
after the split has already occurred. The invalid state is produced in
`load_local_setup`; correction belongs there. Sibling retained-client and
explicit-dependency paths already satisfy their contracts and must remain
unchanged.

## Validation completeness

Every r4 proposal was checked against current source and callers and resolved
above. The final deduplicated ledger contains one bounded finding,
`FIND-TASK-003-10`. It requires no new product, public API, architecture,
security, compatibility, cross-service, resource-ownership, persistent-data,
or concurrency-semantics decision. No additional unsupported mechanism,
check, file, setting, option, dependency, compatibility path, or style
preference survives validation.

The independently validated ledger is therefore non-empty. The candidate needs
the bounded source correction and recorded focused proof above before TASK-003
can pass.
