# TASK-004 r2 focused follow-up review

## Subject and scope

- Repository: `/home/thorrester/Documents/GitHub/wyrd`
- Base: `b90335e0991438e8c28b65ed9c0c4cd80f9b0d56`
- Candidate: `e86831e5ac784028f8022cc3faeee1c22b12c665`
- Approved specification: `changes/active/skald-workflow-runtime/spec.md`, Revision 13
- Original task: `changes/active/skald-workflow-runtime/tasks/TASK-004-accepted-server-jobs.md`
- Remediation task: `changes/active/skald-workflow-runtime/review/TASK-004-r1/TASK-004-R1-close-accepted-job-gaps.md`
- Review mode: source, cumulative diff, applicable authority, and completed discovery reports only. No build, test, Cargo, or mise command was run.

`.codegraph/` is absent, so the review used repository source and Git. The
candidate identity was checked before and after inspection and remained the
requested commit.

## Conflict 1: bare-type import rule

**Resolution: RESOLVED. `STD-004-R2-001` is source-supported. The maintainer
review's lack of a finding does not override the explicit repository rule.**

`architecture/agent-rules.md` requires module-scope imports and bare type names
in struct fields, parameters, return types, trait bounds, and `where` clauses.
Its only test-related exception permits `use super::*` inside an inline test
module; it does not exempt test fields or signatures from the bare-type rule.

Every location cited by `STD-004-R2-001` is in the cumulative changed surface:

| Surface | Diff status and inspected interfaces | Assessment |
|---|---|---|
| `components/workflow/host.rs` | New file; `external_bindings` returns `wyrd_spec::ids::CredentialBindingName` at `:409` | Governed; violation is real. |
| `components/workflow/runs.rs` | New file; fields and signatures use `watch::Receiver`/`watch::Sender`, `blake3::Hash`, `tokio::time::Instant`, `std::sync::atomic::AtomicBool`, and `tokio::sync::Notify` at the cited locations | Governed; violations are real. `std::time::Instant` is already imported, so the Tokio instant needs a clear alias. |
| `query/collect.rs` | New file; test-support return/field types and collector fields use qualified `QueryStreamStall`, `RunningQueryControls`, `DataTenantId`, and `RequestId`; `charge` qualifies `serde::Serialize`; `project_columns` qualifies `arrow::datatypes::Schema` | Governed; violations are real. Qualified enum variants and local expression paths are outside this finding. |
| `state.rs` | The cumulative diff adds the `WorkflowRuns` field and `ServerWorkflowConfig` parameter at `:2168` and `:2387` | Governed; violations are real. Older qualified interfaces elsewhere in this file are not broadened into this task finding. |
| `tests/pg_workflow_runs.rs` | New external journey; new fields/signatures use `watch::Sender`, `axum::http::Uri`, and `reqwest::Client` at the cited locations | Governed; violations are real. External journey placement does not create a source-shape exception. |
| `wyrd-testing/src/server.rs` | The cumulative diff adds the `ServerWorkflowConfig` field and builder parameter at `:486` and `:3853` | Governed; violations are real. |
| `wyrd-testing/src/bifrost/cluster.rs` | The cumulative diff adds `gateway_provider_root` and its builder surface using `url::Url` at `:145`, `:217`, and `:889` | Governed; violations are real. Other older qualified interfaces in the file are outside this task finding. |
| `tests/bifrost/oracle/peer_cluster.rs` | The cumulative diff adds the gateway-root start/launch parameters using `url::Url` at `:107` and `:153` | Governed; violations are real. Existing qualified `QuerySlot` internals are not part of this change. |
| `tests/bifrost/oracle/workflow.rs` | New external journey; new fields/signatures use qualified `reqwest`, `secrecy`, `tempfile`, `tokio::sync::watch`, and `url` types at `:301`, `:317`, `:451`, `:462`, `:473`, `:496`, and `:508` | Governed; violations are real. |

The smallest compliant correction is import-only source cleanup: add each type
to the existing module-scope import block and use a bare name in the affected
interface. Use aliases only for real collisions or clarity, for example
`Blake3Hash`, `WatchReceiver`, `WatchSender`, and `TokioInstant`. Do not add a
source scanner, lint, allow attribute, configuration option, or new test. The
existing compiler/format/lint lanes plus direct source review are sufficient
closure mechanisms.

## Conflict 2: Analytical follower-release documentation

**Resolution: RESOLVED. `QSET-R2-001` is source-supported as documentation
DRIFT; it requires deletion/correction of prose only, never a release
acknowledgement mechanism.**

The owner documentation at
`crates/vala/vala-bifrost-redux/src/oracle/analytical.rs:2492-2505` says an
unacknowledged release must be retried, mentions a detached timer, second
registry, and status RPC, and describes `transports` as issuing reserve and
release operations. The runtime has none of those operations:

- `AnalyticalGraphLifecycle::reserve` and `reserve_round` use `transports` only
  for `reserve_graph` (`analytical.rs:2803-2936`).
- `AnalyticalParticipantGrants` owns the participant streams; dropping it closes
  them and requires no acknowledgement (`analytical.rs:2256-2266`).
- `AnalyticalGraphLifecycle::settle` performs `drop(grants)` and explicitly does
  not wait for follower cleanup (`analytical.rs:2689-2757`).
- `ParticipantGrant` is the stream handle and documents that there is no
  release request to send, retry, or lose
  (`oracle/dispatcher.rs:611-642`).
- `AnalyticalAttemptOwnership::settle` and the query-stream settlement owner say
  follower cleanup may complete after leader settlement returns
  (`analytical.rs:7777-7786`; `oracle/query_stream.rs:783-795`).

The stale type/field prose existed in the base, but the cumulative candidate
materially changes this lifecycle's cleanup semantics and updates its method and
consumer documentation. The owning lifecycle is therefore a materially changed
review surface, and leaving its primary description contradictory is not
unrelated pre-existing debt.

The minimum correction is to replace the obsolete acknowledgement/retry/status
RPC narrative with the implemented ownership contract: the lifecycle retains
participant grant streams, dropping them is leader-side release, followers
settle their own graphs asynchronously, and the transport directory is used for
reservation/admission. Add no release RPC, acknowledgement, retry, timer,
polling loop, setting, probe, or new test. This follows the standing direction
by deleting implied bespoke machinery rather than resurrecting it.

## Conflict 3: Revision 13 task authority

**Resolution: RESOLVED. `STD-004-R2-002` identifies a real authority ambiguity,
but its correction should distinguish current task metadata from historical r1
review evidence.**

The task contract in
`architecture/references/languages/spec-driven-development.md` requires an
active task to identify the approved spec revision from which its current scope
is derived. Revision 13 is approved at `spec.md:1-4` and expressly assigns the
provider-tagged request change and server-internal Vertex proof to TASK-004 at
`spec.md:21-42` and in the revision history at `:2717-2721`. The active task
still says `spec_revision: 12` and links “Approved Revision 12”
(`TASK-004-accepted-server-jobs.md:6,435`). The remediation front matter also
says Revision 12, while its implementation record says Revision 13 was added to
that remediation and records its proof
(`TASK-004-R1-close-accepted-job-gaps.md:6,106,127-128,159-168`). That is an
ambiguous active authority chain.

The minimal correction is:

1. update the active TASK-004 task metadata and current authority link to
   Revision 13;
2. update the active TASK-004-R1 remediation front matter to Revision 13 and add
   one concise note that Revision 13 extended the remediation after the r1
   review; and
3. retain the remediation's `Immutable review inputs` entry at line 17 as the
   historical Revision 12 input for candidate `96e993a...`.

Do not rewrite `review/TASK-004-r1/verdict.md`,
`findings-validation.md`, or any r1 discovery report: their Revision 12 labels
correctly describe that immutable prior review. No new metadata mechanism,
repository check, compatibility path, or extra setting is needed. If the
maintainer prefers the repository's existing late-revision addendum pattern,
a compact Revision 13 addendum that explicitly identifies the same active task
and current revision is also compliant, but it must replace—not duplicate—the
ambiguous current authority. Retrospectively relabeling the prior verdict is not
compliant.

## Other discovery claim noted, not revalidated

The behavior and provider-contract reviewers independently report the same
stale Revision 13 Prompt examples at `spec.md:519-599` (`BEH-004-R2-001` and
`DOMAIN-PROVIDER-1`). Per this follow-up's assigned scope, that corroborated
claim was not revalidated or changed here and remains for the independent
validation pass.

## New findings

None. This follow-up confirms the import-rule and lifecycle-documentation
claims and narrows the Revision 13 metadata correction so current authority is
unambiguous without altering immutable historical review artifacts.

## Follow-up result

**RESOLVED**

