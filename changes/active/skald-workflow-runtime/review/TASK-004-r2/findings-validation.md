# Structured Ponytail Validation — TASK-004 R2

## Immutable subject

- Repository: `/home/thorrester/Documents/GitHub/wyrd`
- Base: `b90335e0991438e8c28b65ed9c0c4cd80f9b0d56`
- Candidate: `e86831e5ac784028f8022cc3faeee1c22b12c665`
- Approved specification: `changes/active/skald-workflow-runtime/spec.md`, revision 13
- Original task: `changes/active/skald-workflow-runtime/tasks/TASK-004-accepted-server-jobs.md`
- Prior verdict and ledger: `changes/active/skald-workflow-runtime/review/TASK-004-r1/{verdict.md,findings-validation.md}`
- Remediation task: `changes/active/skald-workflow-runtime/review/TASK-004-r1/TASK-004-R1-close-accepted-job-gaps.md`
- Review mode: complete cumulative diff, candidate source, governing authority,
  every R2 discovery report, the focused follow-up, and recorded evidence only.
  No build, test, Cargo, or mise command was run.

The candidate identity was checked before and after validation and remained the
requested commit. `.codegraph/` is absent, so immutable Git objects and
repository source were used directly.

## Validation method and completeness

Every discovery proposal was checked against the candidate source, the complete
base-to-candidate range, its owning item and callers or consumers, applicable
authority, and the recorded evidence. The behavior and provider-domain reports'
same Prompt-example claim was validated once and deduplicated. The standards
and query-domain claims were traced independently despite the maintainer report
not proposing them. The focused follow-up resolved all discovery conflicts;
none remains unresolved.

The correction ladder was applied to every retained finding. Each correction is
either deletion of stale material or a direct edit using an existing repository
shape. No correction adds a parser, compatibility reader, release protocol,
transport, lifecycle owner, repository check, setting, option, dependency, or
test harness. The deleted Oracle graph-drain polling and supervisor idle refusal
remain deleted; grant-stream close remains follower release; the leader does not
await a follower acknowledgement; and the approved foreign-tenant fixture limit
is not reopened.

## Proposal-by-proposal validation

| Discovery proposal | Validation | Final disposition |
|---|---|---|
| `BEH-004-R2-001`, `DOMAIN-PROVIDER-1` | Revision 13 requires every example to use the adjacent `provider`/`body` envelope. The three canonical Prompt examples still use the removed untagged form, while `ProviderRequest`, generated schema, and the executable examples require and demonstrate the tagged form. | **CONFIRMED**, deduplicated as `FIND-TASK-004-15`. |
| `STD-004-R2-001` | The cited types occur in changed struct fields, parameters, return types, or trait bounds. `architecture/agent-rules.md` expressly requires module-scope imports and bare type names in those positions, including test code; its inline-test exception permits `use super::*`, not qualified interfaces. | **CONFIRMED** as `FIND-TASK-004-16`. |
| `STD-004-R2-002` | Revision 13 expressly assigns provider-tagged requests and Vertex proof to TASK-004, and the remediation records that implementation, but both active task front matters and TASK-004's current authority link still say Revision 12. The r1 immutable-input entry correctly remains historical Revision 12 evidence. | **REVISED** as `FIND-TASK-004-17` to preserve immutable r1 review records and the remediation's historical input line while correcting only current active authority metadata. |
| `QSET-R2-001` | `AnalyticalGraphLifecycle` documents a retryable release acknowledgement and reserve/release transport even though grants are stream handles, `settle` releases followers by dropping them, and no release RPC, acknowledgement, retry timer, or status RPC exists. The owning description contradicts the implementation and the settled human decision. | **CONFIRMED** as `FIND-TASK-004-18`. |

The invariant, maintainer, system-resilience, concurrency/lifecycle, and
security/tenancy reports proposed no additional findings. Their PASS results do
not displace the source-supported proposals above.

## Prior-finding closure

`FIND-TASK-004-1` through `FIND-TASK-004-14` remain **CLOSED**. Candidate source
and the recorded remediation evidence place their corrections at the diagnosed
owners: reservation and blocking-work tracking; original-deadline query
settlement and pod-loss recovery; real tenant boundaries; declaration rustdoc
and exact tool schemas; pinned/captured authority; idempotency, tool-negative,
gateway, lifecycle, and graph-bound journeys; and accepted-job architecture and
security authority. None is reopened by the four new documentation/source-shape
findings below.

## Final deduplicated finding ledger

### FIND-TASK-004-15 — CONFIRMED — MISSING: canonical Prompt examples use the removed untagged request form

- **Discovery sources:** `BEH-004-R2-001`, `DOMAIN-PROVIDER-1`.
- **Violated obligation:** Revision 13 requires every fixture and example to
  move to the serde-adjacent `ProviderRequest` form, with no compatibility
  reader or alias.
- **Location:** `changes/active/skald-workflow-runtime/spec.md:519-599`,
  specifically the `request` mappings beginning at lines 531, 555, and 579.
- **Producer-to-consumer evidence:** `ProviderRequest` derives adjacent-tagged
  deserialization at `crates/skald/skald-spec/src/request.rs:18-59`; the
  generated Prompt schema requires `provider` and `body` at
  `crates/wyrd-spec/schemas/prompt_spec.json:3899-3920`; and the corresponding
  executable examples under `examples/workflows/code-review/prompts/` already
  use `provider: open_ai_chat_completion` with the native request under
  `body`. The three specification examples instead place `model` and
  `messages` directly beneath `request`, so the sole current parser rejects
  them before registration or execution.
- **Observable consequence:** a user copying any canonical example from the
  approved specification receives a Prompt deserialization failure even though
  the prose presents it as the current Workflow bundle shape.
- **Decision-complete correction:** update only those three examples to the
  existing tagged OpenAI Chat form and clarify the adjacent sentence only as
  needed to say that the native body remains unchanged inside the envelope.
  Reuse the checked-in executable examples. Do not add a compatibility reader,
  migration, alias, parser, option, check, or new fixture.
- **Focused closure proof:** source comparison must show all three snippets use
  the same `request.provider`/`request.body` shape as the executable examples.
  The existing recorded loader/example and codegen lanes remain broader proof;
  no bespoke documentation check is warranted.

### FIND-TASK-004-16 — CONFIRMED — VIOLATION: changed Rust interfaces use qualified type paths

- **Discovery sources:** `STD-004-R2-001`; confirmed by the focused follow-up.
- **Violated obligation:** `architecture/agent-rules.md` requires types to be
  imported in the module's top-level `use` block and used by bare name in
  fields, parameters, return types, trait bounds, and `where` clauses.
- **Location:** changed interfaces at
  `crates/wyrd/wyrd-server/src/components/workflow/host.rs:409`;
  `crates/wyrd/wyrd-server/src/components/workflow/runs.rs:60,70,72,77,88,272,404,422,521-525,540,542,559,647`;
  `crates/wyrd/wyrd-server/src/query/collect.rs:332,359-361,371,594,627`;
  `crates/wyrd/wyrd-server/src/state.rs:2168,2387`;
  `crates/wyrd/wyrd-server/tests/pg_workflow_runs.rs:95,97,309,386`;
  `crates/wyrd/wyrd-testing/src/server.rs:486,3853`;
  `crates/wyrd/wyrd-testing/src/bifrost/cluster.rs:145,217,889`;
  `crates/wyrd/wyrd-testing/tests/bifrost/oracle/peer_cluster.rs:107,153`;
  and `crates/wyrd/wyrd-testing/tests/bifrost/oracle/workflow.rs:301,317,451,462,473,496,508`.
- **Evidence and reachability:** these are fields and callable interfaces on
  the new Workflow owner, shared collector, state composition, and the real
  server/Oracle journey fixtures. They are compiled and called by the accepted
  Workflow routes and recorded journeys; they are not dormant or illustrative
  code. Existing top-level import blocks provide the repository's required
  dependency manifest, while paths such as `blake3::Hash`,
  `tokio::time::Instant`, `crate::oracle::RunningQueryControls`,
  `wyrd_spec::DataTenantId`, `reqwest::Client`, and `url::Url` bypass it in the
  cited interfaces.
- **Observable consequence:** the cumulative candidate violates an explicit
  repository source-shape rule and obscures the changed modules' dependency
  surface despite recorded formatting and lint success.
- **Decision-complete correction:** add the cited types to each existing
  module-scope import block and use bare names at the affected interfaces.
  Alias only real collisions, such as standard versus Tokio `Instant` or watch
  sender/receiver names. Preserve all behavior. Do not add a scanner, lint,
  allow attribute, configuration knob, or repository check.
- **Focused closure proof:** direct source inspection of the affected
  interfaces plus the repository's existing format/lint and affected test
  lanes. This mechanical source correction needs no new behavior test.

### FIND-TASK-004-17 — REVISED — VIOLATION: active TASK-004 authority still identifies Revision 12

- **Discovery sources:** `STD-004-R2-002`; narrowed by the focused follow-up.
- **Violated obligation:** the task contract in
  `architecture/references/languages/spec-driven-development.md` requires an
  active task to identify the approved specification revision governing its
  current scope. Revision 13 expressly assigns its provider-tagged request and
  Vertex proof to TASK-004.
- **Location:**
  `changes/active/skald-workflow-runtime/tasks/TASK-004-accepted-server-jobs.md:6,435`;
  `changes/active/skald-workflow-runtime/review/TASK-004-r1/TASK-004-R1-close-accepted-job-gaps.md:6,106,127-128,159-168`.
  The remediation's `Immutable review inputs` entry at line 17 is historical
  Revision 12 evidence and is not erroneous.
- **Evidence and reachability:** `spec.md:21-42,2717-2721` makes Revision 13
  approved TASK-004 authority. The remediation's implementation record says
  Revision 13 was added to that remediation and records its implementation and
  proof, yet the current task front matter, authority link, and remediation
  front matter still advertise Revision 12. These active files are the inputs
  future implementers and reviewers are instructed to follow.
- **Observable consequence:** the active packet presents two governing
  revisions for one cumulative implementation, allowing a later reader to
  follow Revision 12 and omit or reject the Revision 13 contract.
- **Decision-complete correction:** update TASK-004's front matter and current
  authority link to Revision 13. Update the R1 remediation front matter to
  Revision 13 and add one concise note that Revision 13 extended the
  remediation after the r1 review. Preserve line 17's clearly historical
  Revision 12 immutable input and do not relabel or rewrite the r1 verdict,
  ledger, or discovery reports. Add no successor metadata mechanism, check,
  file, setting, or compatibility path.
- **Focused closure proof:** source review must show one unambiguous current
  Revision 13 authority chain while every immutable r1 review artifact still
  accurately identifies the Revision 12 candidate it reviewed.

### FIND-TASK-004-18 — CONFIRMED — DRIFT: the lifecycle owner documents a nonexistent follower release-ack protocol

- **Discovery sources:** `QSET-R2-001`; confirmed by the focused follow-up.
- **Violated obligation:** the approved lifecycle releases a follower by
  closing its participant grant stream; the leader neither sends nor awaits a
  release acknowledgement. Materially changed Rust documentation must describe
  the actual owner and invariant, and unsupported bespoke mechanisms must be
  deleted rather than preserved as implied requirements.
- **Location:**
  `crates/vala/vala-bifrost-redux/src/oracle/analytical.rs:2492-2505`.
- **Producer-to-consumer evidence:** the stale type documentation says an
  unacknowledged release must be retried and invokes a detached timer, second
  registry, and status RPC; the `transports` field says it issues reserve and
  release operations. In source, `AnalyticalParticipantGrants` owns stream
  handles (`analytical.rs:2256-2266`), `AnalyticalGraphLifecycle::settle`
  performs `drop(grants)` and does not wait for followers
  (`analytical.rs:2689-2784`), `ParticipantGrant` states there is no release
  request to send or retry (`oracle/dispatcher.rs:611-642`), and the public
  settlement/query-stream docs say follower cleanup may finish afterwards
  (`analytical.rs:7777-7786`; `oracle/query_stream.rs:783-795`). `transports`
  is consumed only by participant reservation.
- **Observable consequence:** the owning type advertises the opposite protocol
  from its implementation and approved contract, inviting restoration of a
  release RPC, acknowledgement wait, retry timer, poll, or status mechanism
  that Wyrd neither implements nor authorizes.
- **Decision-complete correction:** replace the obsolete acknowledgement,
  retry, timer, registry, and status-RPC narrative with the existing ownership
  contract: the lifecycle retains participant grant streams; dropping them is
  leader-side release; followers settle their own graphs asynchronously; and
  the transport directory is used for reservation/admission. Add no release
  request, acknowledgement, retry, timer, polling loop, option, setting, probe,
  or new test.
- **Focused closure proof:** source review of the corrected owner and field
  rustdoc against `AnalyticalParticipantGrants`, `ParticipantGrant`, and
  `AnalyticalGraphLifecycle::settle`. Existing scheduled and forwarded
  journeys remain the behavioral proof.

## Validated ledger summary

| Stable ID | Status | Classification |
|---|---|---|
| `FIND-TASK-004-1` through `FIND-TASK-004-14` | CLOSED | Prior remediation findings |
| `FIND-TASK-004-15` | CONFIRMED | MISSING |
| `FIND-TASK-004-16` | CONFIRMED | VIOLATION |
| `FIND-TASK-004-17` | REVISED | VIOLATION |
| `FIND-TASK-004-18` | CONFIRMED | DRIFT |

No retained correction requires a new material product, public API,
architecture, security, compatibility, cross-service, concurrency,
resource-ownership, or persistent-data decision. All four are bounded source or
active-packet corrections under approved Revision 13; no
`SPEC_REVISION_REQUIRED` condition is present.

## Verification limits and blockers

- No build, test, Cargo, or mise command was run, per the review instruction.
- Recorded evidence was treated as a claim and checked against the named source
  and assertions. It credibly closes prior runtime findings but cannot make the
  four source-visible defects above pass.
- No required report, authority, diff, or source was missing. The required
  validation pass is complete, and no unresolved disagreement or blocker
  remains.
