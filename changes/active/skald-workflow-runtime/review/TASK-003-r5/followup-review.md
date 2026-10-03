# Focused follow-up — TASK-003 r5 mixed-route snapshot proof

**Resolution: RESOLVED.** The R4 implementation closes the two-snapshot
runtime defect in source, but its Rust regression and recorded command are
narrower than R4 required. The required caller-to-owner closure proof is
missing. This remains closure of stable `FIND-TASK-003-10`; it is not a new
finding.

## Immutable subject and review boundary

- Repository: `/home/thorrester/Documents/GitHub/wyrd`
- Base: `58d07d7260df1f022a721e720a28ea48e5096e35`
- Candidate: `5084b0e5b30fe79fdb2468ad7d0fa3a9d2106069`
- Approved specification:
  `changes/active/skald-workflow-runtime/spec.md`, Revision 12
- Original task:
  `changes/active/skald-workflow-runtime/tasks/TASK-003-remote-client-and-public-gateway.md`
- Focused remediation:
  `changes/active/skald-workflow-runtime/review/TASK-003-r4/TASK-003-R4-use-one-run-start-config-snapshot.md`

The human-approved `WyrdGatewayCall.model` amendment, native-`401` wording
correction, and run-start `spawn_blocking` decision were treated as authority.
This follow-up resolves only the disagreement over R4's mixed-route proof. It
was strictly source-only: no build, compile, test, lane, Cargo, mise, pnpm,
pytest, formatter, linter, typecheck, codegen, test listing, or other
verification command was run.

## Conflicting claims

1. `task-review-behavior.md`, `task-review-invariants.md`, and
   `system-review.md` report that
   `workflow::tests::mixed_routes_use_one_config_snapshot` bypasses the actual
   client-less mixed-route Workflow path and that R4 did not record its
   mandated exact selector/count.
2. `domain-review-concurrency.md` accepts the helper-level fixture plus the
   recorded five-test selector as sufficient closure evidence.

## Full path inspected

The inspected producer-to-consumer path was:

1. R4's diagnosis, acceptance criteria, focused-proof requirement, and
   implementation record at
   `TASK-003-R4-use-one-run-start-config-snapshot.md:36-72,74-98,118-159,162-178`.
2. The prior validated closure contract at
   `review/TASK-003-r4/findings-validation.md:57-160,181-188`, which assigns
   implementation and focused proof to stable `FIND-TASK-003-10`.
3. `Workflow::run_with` at
   `crates/shared/wyrd-client/src/workflow/mod.rs:131-149`, which obtains
   `SelectedRoutes`, derives `needs_config` and `needs_gateway`, retains the
   loading client if present, invokes `load_local_setup` inside
   `spawn_blocking`, then supplies the returned values to
   `SelectedRoutes::dependencies`.
4. `load_local_setup` and `local_setup_from` at
   `workflow/mod.rs:179-240`.
5. `SelectedRoutes::{of,needs_gateway,needs_config,dependencies}` and selected
   secret resolution at
   `crates/shared/wyrd-client/src/workflow/local.rs:30-138`.
6. The existing selected-route fixture and the new R4 fixture at
   `workflow/mod.rs:582-598,600-758,783-826`.
7. The original TASK-003 selected-dependency implementation record at
   `tasks/TASK-003-remote-client-and-public-gateway.md:297-307` and its general
   evidence requirement at `:262-274`.

## Evidence and resolution

### The source correction is present

The current source establishes the production invariant:

- `Workflow::run_with` derives both need flags from the Workflow's resolved
  routes and passes them with the optional retained client to one blocking
  `load_local_setup` call (`workflow/mod.rs:131-146`).
- `load_local_setup` contains one conditional `GlobalConfig::load` and passes
  that owned value to `local_setup_from` (`workflow/mod.rs:191-202`).
- For a client-less selected Wyrd gateway, `local_setup_from` builds the client
  through `ClientConfig::from_global_with_env(&global)` and
  `WyrdClient::with_config`, while the selected external-binding configuration
  comes from that same value's `global.workflow` field
  (`workflow/mod.rs:215-239`). The old independently reloading
  `WyrdClient::from_global` path is absent here.
- A retained client takes precedence, and two false need flags use
  `GlobalConfig::default` rather than reading ambient configuration.

Thus the implementation defect itself is closed in source. There is no basis
for configuration-generation machinery, a timing fixture, a hook, a checker,
or another setting; requiring one would violate R4's explicit non-goals and
the standing DRIFT rule.

### The new Rust fixture proves only the extracted helper contract

`mixed_routes_use_one_config_snapshot` parses one in-memory `GlobalConfig`,
sets an ambient API-key credential, and directly calls
`local_setup_from(global, true, true, None)` (`workflow/mod.rs:790-825`). Its
assertions prove that this helper returns the parsed `review-gateway` binding
and a Wyrd client using the same value's `http_url`.

That is useful lower-level proof, but it does not satisfy the whole R4 closure
contract. The test constructs no `Workflow`; contains no steps selecting both
route families; does not call `SelectedRoutes::of`; supplies the two need flags
manually; does not call `Workflow::run_with`; does not enter the approved
`spawn_blocking`/`load_local_setup` boundary; and does not pass the resulting
setup through `SelectedRoutes::dependencies`. Consequently it can remain green
if route discovery, need-flag wiring, the run-start owner call, or consumption
of either returned value regresses.

The pre-existing `selected_local_dependencies_use_shared_config` test does not
fill this gap. Its external route invokes `run_selected`, which calls
`SelectedRoutes::dependencies` with an already parsed `LocalWorkflowConfig`
and no gateway (`workflow/mod.rs:586-598,629-715`). Its Wyrd-gateway case is a
separate Workflow carrying an explicitly retained client and calls
`Workflow::run_with` with `needs_config == false`
(`workflow/mod.rs:716-758`). Neither case is one client-less Workflow selecting
both `ext_gateway` and `wyrd_gateway`.

R4 is explicit rather than ambiguous on this point. Its focused-proof section
requires extending or reusing the existing selected-local-dependencies test
"to exercise one Workflow selecting both route families without a retained
client" (`TASK-003-R4...md:130-134`). The helper-only fixture is therefore
narrower than the required proof even though it corroborates the corrected
helper implementation.

### The recorded command does not satisfy the mandated evidence record

R4 mandates the exact focused command selecting
`workflow::tests::selected_local_dependencies_use_shared_config` and its
selected count (`TASK-003-R4...md:134-139,157-159`). The implementation record
instead contains one broad union expression over Workflow unit tests plus two
integration tests and records only `5 passed`
(`TASK-003-R4...md:170-177`). The Workflow regex plausibly includes the new
helper test, so the aggregate record supports that the helper-level assertions
passed; it does not supply the mandated exact selector/count, and passing that
helper cannot substitute for the absent mixed-Workflow path.

The exact Python retained-client selector is separately recorded with
`1 passed`; that closes R4's retained-client language proof but does not repair
the missing client-less mixed-route Rust proof.

## Proposed finding

### FU-TASK003-R5-1 — MISSING: R4's required mixed-route Workflow closure proof is absent

- **Stable ID:** `FIND-TASK-003-10` (continued closure evidence), not a new
  `FIND-TASK-003-*` ID.
- **Violated obligation:** R4 requires source proof plus a test that exercises
  one client-less `Workflow` selecting both route families through the shared
  run-start owner, and requires the prescribed exact selector/count to be
  recorded (`TASK-003-R4...md:122,128-139,157-159`).
- **Exact location:**
  `crates/shared/wyrd-client/src/workflow/mod.rs:582-598,600-758,783-826` and
  `changes/active/skald-workflow-runtime/review/TASK-003-r4/TASK-003-R4-use-one-run-start-config-snapshot.md:168-177`.
- **Evidence:** the new test calls `local_setup_from` with manually supplied
  booleans; the older test separates an already-configured external route from
  a retained-client Wyrd route; the evidence record substitutes a five-test
  union for the mandated exact command/count.
- **Observable consequence:** the recorded proof can stay green while the live
  Workflow-to-route-selection-to-run-start wiring no longer produces or
  consumes both halves of the single snapshot. The runtime source is currently
  correct, but acceptance of the repeatedly remediated invariant is not
  regression-protected as required.
- **Smallest testable correction:** extend or replace the existing shared
  selected-local-dependencies proof so one client-less mixed-route `Workflow`
  reaches the actual `Workflow::run_with` run-start path and demonstrates use
  of both the selected external binding and Wyrd client from the same ambient
  snapshot. Record the exact focused selector and selected count required by
  R4. Reuse the existing Workflow fixtures and owners; add no production
  mechanism, hook, timing assertion, fixture system, or permanent check.

## Why this is not a new stable finding

R4 itself is a remediation solely for `FIND-TASK-003-10`, and its prior
validated ledger expressly treats required focused proof as part of closing
that same producer defect rather than as a separate proof-only finding
(`findings-validation.md:149-160,181-188`). The missing path test has no
independent runtime producer or behavioral consequence beyond leaving the
one-snapshot correction unclosed. Preserve `FIND-TASK-003-10`.

