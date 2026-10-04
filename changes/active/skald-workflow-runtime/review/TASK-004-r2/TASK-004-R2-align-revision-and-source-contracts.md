---
id: TASK-004-R2
kind: remediation
status: ready
spec: SPEC-skald-workflow-runtime
spec_revision: 13
parent_task: TASK-004
remediates: [FIND-TASK-004-15, FIND-TASK-004-16, FIND-TASK-004-17, FIND-TASK-004-18]
---

# Align Revision 13 examples, task authority, Rust interfaces, and Oracle lifecycle documentation

Implementation skill: `$wyrd-implement`.

## Immutable review inputs

- Approved spec: `changes/active/skald-workflow-runtime/spec.md`, Revision 13
- Original task: `changes/active/skald-workflow-runtime/tasks/TASK-004-accepted-server-jobs.md`
- Prior remediation: `changes/active/skald-workflow-runtime/review/TASK-004-r1/TASK-004-R1-close-accepted-job-gaps.md`
- Reviewed base: `b90335e0991438e8c28b65ed9c0c4cd80f9b0d56`
- Reviewed candidate: `e86831e5ac784028f8022cc3faeee1c22b12c665`
- Validated diagnosis: `changes/active/skald-workflow-runtime/review/TASK-004-r2/findings-validation.md`

## Outcome

Make the active TASK-004 packet and source describe the implementation that is
already approved and present: canonical Prompt examples use Revision 13's
provider-tagged request envelope, current task authority identifies Revision
13, changed Rust interfaces use repository-required imported bare type names,
and Oracle lifecycle documentation states the approved grant-stream-close
release model. Preserve all runtime behavior and all prior finding closures.

## Diagnoses and required corrections

### Tagged Prompt examples (`FIND-TASK-004-15`)

Revision 13 removed untagged `ProviderRequest` deserialization and requires all
examples to use serde adjacent tagging. The three canonical Prompt snippets in
`changes/active/skald-workflow-runtime/spec.md:519-599` still place `model` and
`messages` directly under `request`. `ProviderRequest` and the generated Prompt
schema require `request.provider` and `request.body`, while the corresponding
checked-in code-review Prompt examples already use the correct OpenAI Chat form.
Copying the specification snippets therefore fails before registration or
execution.

Update only those three snippets to the existing tagged OpenAI Chat shape and,
if necessary, clarify the adjacent prose that the provider-native body remains
unchanged inside the tagged envelope. Reuse the checked-in executable examples.
Do not add a compatibility reader, migration, alias, alternate parser, fixture,
option, setting, or documentation check.

### Rust interface source shape (`FIND-TASK-004-16`)

New and materially changed fields, parameters, return types, and trait bounds
use qualified paths in the Workflow owner, shared query collector, server state,
and server/Oracle journey fixtures. `architecture/agent-rules.md` requires types
to be imported in the module-level `use` block and referenced by bare name in
those interface positions; the rule applies to test code and has no exception
for qualified fixture interfaces.

Correct the interfaces listed in the validated ledger across:

- `crates/wyrd/wyrd-server/src/components/workflow/host.rs`;
- `crates/wyrd/wyrd-server/src/components/workflow/runs.rs`;
- `crates/wyrd/wyrd-server/src/query/collect.rs`;
- `crates/wyrd/wyrd-server/src/state.rs`;
- `crates/wyrd/wyrd-server/tests/pg_workflow_runs.rs`;
- `crates/wyrd/wyrd-testing/src/server.rs`;
- `crates/wyrd/wyrd-testing/src/bifrost/cluster.rs`;
- `crates/wyrd/wyrd-testing/tests/bifrost/oracle/peer_cluster.rs`; and
- `crates/wyrd/wyrd-testing/tests/bifrost/oracle/workflow.rs`.

Add each cited type to the existing top-level import block and use its bare
name. Use an alias only for a real collision, such as standard versus Tokio
`Instant` or distinct sender/receiver types. This is a behavior-neutral source
correction. Do not add a scanner, lint, allow attribute, configuration knob, or
repository check.

### Active Revision 13 authority (`FIND-TASK-004-17`)

The approved specification expressly assigns provider-tagged requests and the
internal Vertex proof to TASK-004, and the r1 remediation record says that work
was implemented there. The current TASK-004 front matter and authority link,
and the remediation front matter, still advertise Revision 12. This gives a
future implementer or reviewer two different current authorities.

Update TASK-004's front matter and current authority link to Revision 13.
Update the R1 remediation front matter to Revision 13 and add one concise note
that Revision 13 extended the remediation after the r1 review. Preserve the
remediation's `Immutable review inputs` entry identifying Revision 12 as the
historical authority reviewed in r1. Do not relabel or rewrite the r1 verdict,
finding ledger, discovery reports, or their candidate identities. Do not add a
successor metadata mechanism, compatibility path, setting, check, or duplicate
authority file.

### Oracle follower-release documentation (`FIND-TASK-004-18`)

`AnalyticalGraphLifecycle` documentation in
`crates/vala/vala-bifrost-redux/src/oracle/analytical.rs:2492-2505` describes a
release acknowledgement that must be retried and says the transport directory
issues reserve and release operations. The implementation has no release RPC or
acknowledgement: participant grants are stream handles, the leader releases
followers by dropping them, and followers settle their own graphs
asynchronously. Other corrected owner and query-stream documentation already
states that behavior.

Replace the obsolete acknowledgement, retry, timer, registry, status-RPC, and
reserve/release narrative with the implemented ownership contract: the
lifecycle retains participant grant streams; dropping them is the leader-side
release; followers settle asynchronously; and the transport directory is used
for participant reservation/admission. Add no release request,
acknowledgement, retry, timer, polling loop, option, setting, probe, or test.

## Preserved behavior and non-goals

- Preserve every runtime, wire, schema, provider, gateway, Workflow, query,
  tenancy, authorization, audit, deadline, cancellation, and shutdown behavior.
- Preserve closure of `FIND-TASK-004-1` through `FIND-TASK-004-14`.
- Keep the deleted Oracle graph-drain polling, supervisor idle refusal,
  reserved-byte poison, and mechanism-specific tests deleted.
- Follower release remains grant-stream close. The leader does not await a
  follower release acknowledgement; existing journeys may wait for follower
  cleanup before asserting baselines.
- Preserve the approved foreign-tenant harness limit; do not provision a second
  tenant's gateway credentials merely to run a model step.
- No compatibility reader, migration, new parser, release protocol, lifecycle
  owner, repository check, setting, option, dependency, fixture, or test harness.
- Do not rewrite immutable r1 review reports or their historical Revision 12
  subject.

## Acceptance criteria

| Finding | Closure criterion |
|---|---|
| `FIND-TASK-004-15` | All three canonical Prompt snippets use `request.provider: open_ai_chat_completion` and place the unchanged native request under `request.body`. |
| `FIND-TASK-004-16` | Every interface cited by the validated ledger uses a module-imported bare type name, with aliases only for real collisions and no behavior change. |
| `FIND-TASK-004-17` | Current TASK-004 and remediation authority identify Revision 13 unambiguously, while immutable r1 reports and the remediation's historical input retain their accurate Revision 12 context. |
| `FIND-TASK-004-18` | The Analytical lifecycle owner and transport field describe stream-close release and asynchronous follower settlement, with no implied release RPC, acknowledgement, retry, timer, or status protocol. |

## Focused and broader proof

The documentation and metadata corrections close through direct source review:

- compare all three Prompt snippets with the checked-in executable code-review
  Prompt examples and the generated Prompt schema;
- inspect the current/historical revision chain across the spec, TASK-004,
  remediation, and immutable r1 reports;
- inspect the corrected Analytical owner documentation against
  `AnalyticalParticipantGrants`, `ParticipantGrant`, and
  `AnalyticalGraphLifecycle::settle`; and
- inspect every cited Rust interface and its module-level imports.

After the behavior-neutral Rust source edits, run the existing repository
format and lint lanes and the affected recorded Workflow, query, and Oracle
test lanes using their repository-prescribed commands. Run the existing
code-generation check to confirm the specification-only example edits do not
alter generated contracts. Do not add a new test or repository check: existing
type checking, format/lint lanes, focused owner/journey tests, and direct source
review are the appropriate proof.

## Implementation evidence

Commits: `2dc630dfa`, `946a68633`, `3ef4700a3`.

| Acceptance criterion | Implementation evidence | Verification evidence | Result |
|---|---|---|---|
| `FIND-TASK-004-15` | `spec.md` security, correctness, and final-reviewer Prompt snippets use `request.provider: open_ai_chat_completion` with the unchanged native request under `request.body`; the lead-in sentence names the envelope | Source comparison with `examples/workflows/code-review/prompts/*.yaml` and `crates/wyrd-spec/schemas/prompt_spec.json`; `mise run codegen:check` | PASS |
| `FIND-TASK-004-16` | Module-level imports and bare names at every cited interface in `workflow/{host,runs}.rs`, `query/collect.rs`, `state.rs`, `tests/pg_workflow_runs.rs`, `wyrd-testing/src/{server,bifrost/cluster}.rs`, `wyrd-testing/tests/bifrost/oracle/{peer_cluster,workflow}.rs`; one alias, `TokioInstant`, for the real collision with `std::time::Instant`; test-support-only imports are gated by the same `cfg` as their users | `mise run fmt`; `mise run lints` (includes the non-test-support `wyrd-server` bin lane); `WYRD_TEST_PACKAGES=wyrd-server mise run test:wyrd` (683 passed); `mise run test:bifrost:journey:oracle` (43 passed) | PASS |
| `FIND-TASK-004-17` | TASK-004 `spec_revision: 13` and `[Approved Revision 13]` link; R1 remediation `spec_revision: 13` plus one note that Revision 13 extended it after the r1 review; the R1 historical "revision 12" input and every r1 review report unchanged | Source review of the revision chain | PASS |
| `FIND-TASK-004-18` | `AnalyticalGraphLifecycle` rustdoc: `reserve` returns grant streams held until `settle`, dropping them is the leader-side release, followers settle asynchronously and the leader never waits; `transports` documented as reservation/admission only; stale "retained release" wording removed | Source review against `AnalyticalParticipantGrants`, `ParticipantGrant`, `AnalyticalGraphLifecycle::settle`; `mise run lints` | PASS |

Non-goals held: no runtime, wire, schema, or test-assertion change; no new check, setting, option, dependency, fixture, parser, or release protocol; `git diff --check` clean.
