# Public contracts — approved revision 7

These shapes make [spec.md](spec.md) reviewable. Existing CardRef, File,
execution, receipt, typed permission and problem+json contracts remain canonical.
The user approved these shapes with specification revision 3 on 2026-11-10.
Revision 4 changes installation storage. Revision 5 selects the documented
GitHub.com App flow. Revision 7 records approved execution/receipt semantics
below; public shapes remain unchanged.
JSON objects are closed unless an existing referenced type says otherwise.

## Execution and receipt semantics — revision 7

Direct and queued work use the same memory-owned engine under spec REQ-017.
Necessary preparation/cold loads remain supported. Live step/publication success
and direct results do not certify retained evidence. Validated extraction values
feed Prompt/workspace consumers without a persistence ACK; recovery uses exact
actually accepted evidence. No new response fields or retry-cache contract.

Queued computation releases compute resources at terminal selection. Separately
bounded background custody retains state and renews the claim until canonical
terminal evidence is accepted and token-fenced group settlement completes. Full
tracking pauses new claims. A fully blocked execution also releases compute before
its parked-checkpoint ACK; background custody retires that delivery after ACK.
Positive deferred-evidence and approval receipts retain durable acceptance semantics.
Process death may lose unaccepted progress/results; preserve accepted identities,
spend and original deadline without promising conservation of unpersisted spend.
Bifrost contracts and external upload completion remain unchanged.

## Repository and declaration

A Repository uses the ordinary Card envelope with kind Repository. Its spec is:

| Field | Type and meaning |
|---|---|
| provider | `github` in this change |
| host | Canonical configured GitHub web host |
| repository_id | Positive decimal provider ID string; durable identity |
| verified_by | Normal array of singular VerificationBinding entries |

Full name, display URL, access state, selected version and binding status are
server-managed projections, not required author boilerplate. Initial metadata
name comes from repository name; provider rename preserves the established family.

TOML uses `[repository.metadata]` and `[repository.spec]` only for overrides.
Provider identity fields may be omitted when inferred from origin and verified
server-side. `[repository.verified_by]` contains verifier array and optional
runs_on as approved; the array expands into normal singular bindings. Existing
metadata defaults/labels/annotations retain their normal merge semantics.

`runs_on = {kind = "pull_request"}` is a closed Trigger activation. It does not
accept arbitrary event names. Referenced Trigger Cards use the same activation.

## GitHub connection

Use GitHub.com’s documented GitHub App installation and OAuth flow with standard
state, PKCE and installation confirmation. Production provider endpoints are
`https://github.com` and `https://api.github.com`; development may use local
protocol mocks.

Operator configuration supplies App ID, slug, web/API origins, private key,
webhook secret, OAuth client ID/secret and exact public URLs through existing
configuration/secret providers. Setup and callback are separate Rust-owned
protocol endpoints. Connection transactions are one-use, actor/tenant/browser
bound and expire through the existing authorization-transaction lifecycle.
User OAuth tokens are transient and redacted. Revalidate installation/user repository access before
association; callback parameters alone prove nothing.

Use one connection flow under /v1/integrations/github. The provider setup and
OAuth callback complete verified association; do not require a second manual
confirmation workflow or a separate disable/claim-management feature:

| Operation | Result |
|---|---|
| POST /v1/integrations/github/connect | Bounded authorization transaction and GitHub URL |
| GET /v1/integrations/github | Redacted connection/access view |
| DELETE /v1/integrations/github | Disconnect/release after fencing affected work |

Connection requests carry no tenant ID, App keys or arbitrary provider host.
The current authenticated tenant and configured host supply those. Repository
selections contain verified provider repository IDs. Current GitHub installation
access and tenant-selected access must both cover a repository. Adding access
never creates a Repository Card or bindings. Disconnect, provider removal and
suspension stop new admission/IO and cancel affected unfinished execution.

Views contain configured host/App identity, installation ID when verified,
connection state, accessible selected repository IDs/names and safe timestamps.
No codes/tokens/secrets, signing material or another tenant's claim are returned.
Read/manage use normal authorization through existing permission owners;
callback state never substitutes for current authorization. Do not introduce a
parallel permission system or assume new permission categories are required.

## Pull request extraction

`extract` accepts exactly one of existing `from` or new
`extractor: pull_request`. Neither/both is invalid. The PR form reads queued
activation's pinned source; the caller cannot inject installation credentials.

| Output field | Exact responsibility |
|---|---|
| format | `pull_request` |
| pr.repository | Exact Repository CardRef plus canonical host and provider repository ID |
| pr.number, url, title, body | Positive PR number and source metadata; absent body is empty text |
| pr.state, draft | open/closed and boolean |
| pr.base | Branch ref and pinned commit |
| pr.head | Source repository ID, branch ref and pinned commit, including forks |
| pr.merge_base | Verified merge-base commit for the complete PR diff |
| review.mode | full/incremental |
| review.comparison_base | Commit used for active comparison |
| review.previous | Null, or retained execution reference identifying the previous usable review; report obtained through ordinary execution reads |
| review.fallback_reason | Null for incremental; first_review, history_unavailable, history_rewritten or policy_changed for full fallback |
| changes.base, head | Active comparison commit pins |
| changes.files | Active-comparison changed files described below |
| review.full_pr | Retained complete merge-base→head comparison using the same file structure; separate from active changes so normal changes binding does not inject both diffs |
| context | Existing library ReviewContext, unchanged, including its format/extraction versions, identity, PR/base/head/tree, selected spans, omissions, not_analyzed, budget and coverage |

Each changed file supplies current path, old path or null, added/modified/deleted/
renamed/type_changed status, before/after blob IDs or null and text diff hunks.
Each hunk supplies old/new starting lines and counts and the patch text. Retained
before/after source is available to validate locations/replacements; binary,
unavailable and excluded source is explicitly identified, never invented as empty
text. Required source absence or provider truncation must be reconstructed from
verified objects or fail acquisition; reported context-selection omissions are
not automatically extraction failures. An empty active diff is valid evidence,
not an automatic passed verdict. No source clone is required.

changes contains the active comparison only. review.full_pr retains the complete
comparison in the same extraction JSON for workspace/tool access; it is not
implicitly added to changes or the library context. The existing approved
bindings to pr, changes and context therefore stay focused on active changes.

The independent library returns pinned complete and active comparisons with
supporting context selected for the active comparison. It does not own Wyrd's
policy/baseline choice or review findings. Accepted evidence outlives library
context-record expiry under Wyrd's retention rules.

Normal bindings remain:

```yaml
inputs:
  context: steps.extract-pr.output.context
  changes: steps.extract-pr.output.changes.files
workspace:
  /inputs/pr.json: steps.extract-pr.output
```

The Prompt must declare exactly those placeholders. JSON mounts do not implicitly
expand into a source tree. Full evidence can remain in workspace while a caller
binds only selected fields; normal model-input limits still apply.

## Standard code review report

`response_format: code_review` expands to this generated schema. Ordinary code
outputs with the same envelope are validated by the GitHub consumer. This is
optional, not a new Verifier kind or a restriction on custom output schemas.

```text
CodeReviewReport {
  format: "code_review", version: 1,
  summary: string,
  coverage: { complete: boolean, limitations: string[] },
  findings: Finding[]
}
Finding {
  id: string,                         # unique within the report
  severity: critical | high | medium | low,
  confidence: high | medium | low,
  title: string, explanation: string,
  locations: SourceAnchor[], evidence: SourceAnchor[],
  remediation: null | {
    explanation: string,
    suggested_edit: null | { location: SourceAnchor, replacement: string }
  }
}
SourceAnchor { path: string, commit: string, start_line: positive integer,
               end_line: positive integer }
```

All fields are present; unavailable remediation/edit is null and unlocated
findings use empty arrays. Source paths are canonical repository-relative paths;
line ranges are inclusive, ordered and within retained source for that exact
commit. No fabricated line 1. Suggestions replace exactly the declared retained
range. Invalid source/edit content is refused for provider rendering with a safe
visible delivery error, never silently modified into a different finding.

Critical means catastrophic impact; high serious failure; medium bounded material
failure; low limited actionable impact. Confidence high means directly supported,
medium credible but not fully confirmed, low tentative. Neither encodes verdict.
Existing execution/provider structured-output and publication budgets apply; the
report is not forced into the separate 64-KiB Verdict.data envelope. Large reports
may not be silently truncated into a clean review. Finding IDs survive authored
aggregation and delivery retries; there is no hidden semantic identity model.

The final successful accepted output of each logical step is eligible; each
retained fanout item is an output, not a time-based winner. Only a final retained
repeat output counts. Discarded speculation and failed attempts do not count.
More than one code_review envelope is an ambiguity error even if a consolidator
consumed the earlier envelopes. Intermediate reviewers should return Findings
under ordinary schemas, reserving the final envelope for their reporting step.

## Deferred input publication

Add `deferred: true` to json/media slots; omitted/false preserves current input
behavior. Samples and grouped source inputs retain their existing paths. Missing
deferred input blocks actual consumers; it does not mark unrelated requirements
inconclusive. Exact type/format/schema validation occurs before input acceptance.

An active binding with a deferred input subscribes to that named evidence.
Subscription does not require a running execution. Publication is an input
supply, not a run trigger. Policy updates do not rewrite admitted executions.

```shell
wyrd evidence publish ci_results --file results.json
wyrd evidence publish ci_results --file results.json --repository <git-remote-url> --commit <head-sha>
```

The first form infers repository from the checkout remote and commit from Git.
The second supplies only missing or different source identity, for example when
CI checks out a merge revision instead of the reviewed head. SDK capability is
`client.evidence.publish(name, value, repository=None, commit=None)`. Reuse
existing typed JSON/File/Artifact input handling and upload completion.

The server resolves and authorizes the repository through the tenant's connected
repository identity. Match repository, exact source commit and input name within
the authenticated tenant. Repository means the stable provider repository already
identified by host/repository_id in Repository spec; it is not a new CardRef kind.
Evidence is about source, not review-policy version. Publishing never requires
selecting a Repository Card version or a specific Verifier/execution. Executions
retain their exact Card/Verifier/policy pins for judging and lineage.

Raw HTTP uses POST /v1/verification/evidence/publish after upload completion:

```text
{ repository: {host, repository_id}, tags: {commit: CommitSha}, input: SlotName,
  value: existing JSON or completed media InputDeclaration }
→ { evidence_id, repository: {host, repository_id}, tags: {commit}, input,
    status: accepted | replayed, matched_subscriptions: integer, recorded_at }
```

CLI/SDK resolve the remote to this existing registered repository identity;
client-supplied Git metadata is never permission. tags is limited to commit for
this change. No group_id, caller retry key or arbitrary filter language.

Wyrd supplies compatible, authorized subscribers. Identical matching identity
and content replays accepted evidence; conflicting replacement is refused.
Validate declared slot type/format/schema before acceptance. File identity uses
verified digest/format; JSON uses existing canonicalization. No new evidence
permission categories are prescribed: extend the existing evidence/subject
access checks for this operation, and do not grant unrelated execution or Card
mutation rights.

| Situation | Behavior |
|---|---|
| Matching execution waiting | Accept input and queue continuation of the same execution |
| Matching execution running or queued | Accept input; normal scheduling releases dependent work |
| Existing subscriber, no execution | Retain evidence for that subscriber under normal retention; a later matching execution can consume it |
| No eligible subscriber | Explicit unmatched result; no retained publication for hypothetical future subscribers |
| Completed/cancelled/superseded execution | Never reopen it |
| Subscriber added later | No automatic historical replay |

These are the subscription rules. Do not add public reactivation/rollback modes,
subscription-generation IDs or a second subscription-management API. Track which
subscribers accepted publication internally as needed for durable delivery;
implementation choices must preserve the table above without changing it.
Evidence may reach eligible executions with different pinned review policies
when they concern the same repository/commit and subscribe to that input.

A positive receipt means durable evidence acceptance, not computation or GitHub
completion. Already transferred but unpublished bytes follow normal cleanup.
Execution initialization and recovery check accepted evidence as well as new
notifications; missing notification must not lose accepted input. No new
PostgreSQL tables or payload mirrors.

## Human approval

```yaml
action:
  kind: approval
  message: Approve the assessment before continuing.
  expires_after_seconds: 3600
```

Expiry is selected once with database coordination time, bounded by the original
execution deadline. Inspection adds pending approval occurrences with approval_id,
step/item/iteration identity, message, expiry and pending/approved/rejected/expired
state. Approval IDs identify logical occurrences, not computation attempts.

POST /v1/verification/executions/{execution_id}/approvals/{approval_id}
accepts `{decision: approved | rejected, reason: string | null}`.
A retry of the same actor/decision/reason returns the same receipt; a conflicting
or non-pending occurrence returns 409. Expiry is server-selected, not a request.
Result is `{approval_id,status,actor_id,decided_at,reason}`; expired has null actor.
No payload can expand the execution's accepted authority.

Extend the existing Verifier permission with a distinct approve action and exact
Verifier scope, alongside read/run. Use normal authentication and authorization:
the authenticated user must be authorized to approve this pending decision.
Do not require fresh interactive login, a five-minute authentication window or
a special approval-proof mechanism. Credential type alone does not replace or
add to the normal authorization decision. CLI, SDK and MCP use the same check.

Approval receipt follows durable decision acceptance. The same execution resumes
through queue admission. A standard requirement helper passes only approved;
authors may branch on rejection/expiry. Repeated/fanout approvals have distinct
occurrences and speculative/obsolete occurrences cannot authorize current work.

## Errors, inspection and delivery

Use derive-backed WyrdError/problem+json. Exact new catalog entries distinguish
invalid input (400), failed authentication/authorization (401/403), unknown accessible
subject/execution/approval (404), version/selection/evidence/decision conflict
or no subscription (409), unavailable provider/authoritative state (503) and
ordinary bounded admission refusal (429). Error details expose safe field/reason
identifiers, not evidence content, tokens or foreign tenant identity.

Execution inspection adds waiting status with deferred input names/pending
approval occurrences and a separate GitHub delivery projection. Waiting is
nonterminal; waiting branches can coexist with running branches. Existing direct
and queued callers retain status/result semantics otherwise. Read APIs remain
available through CLI/SDK/MCP/HTTP with the existing verifier-read authorization.

Check name is stable per Repository/Verifier binding; external_id names the exact
execution. PR review comments use COMMENT, exact reviewed commit and stable
finding identity. Retain enough provider delivery progress to reconcile ambiguous writes;
provider IDs are reused instead of duplicating delivery. Reuse valid existing comments; if an anchor moves, preserve
history and link replacement rather than silently changing its source location.
Provider-side annotation batching respects its documented limit and append
semantics. No transport retry may blindly append after an unknown outcome.
