# Approved runtime isolation and GitHub integration

Status: approved design decisions. Exact contracts/implementation remain to specify.

The user approved the runtime/GitHub decisions and subsequent suspension
recommendations. Waiting must not consume active
worker capacity. Exact authoring/wire and durable-transition contracts remain
to specify; implementation is not implied by this approval.

Precedence: the later [nonblocking execution correction](approved-nonblocking-execution.md)
(spec revision 7, task 010) supersedes every engine ACK-before-progress or
checkpoint-before-compute-release prerequisite below. Engine transitions complete
in live memory; persistence, ACK waits and queue retirement are background custody.

## Direct and queued capacity

- Direct and queued verification have separate execution/admission capacity.
- Each path has its own workspace and Python worker capacity. Model-call admission
  also distinguishes direct and queued work; separating only the runner semaphore
  is insufficient.
- No borrowing between pools initially. Queued work waits when its own capacity
  is full and cannot consume capacity reserved for direct verification.
- Resumed queued work stays in the queued path. Both use the same engine
  implementation and standard tenant/authority/evidence behavior.
- Required proof: hold queued work at its configured capacity and demonstrate
  that direct verification can acquire its reserved resources and execute.

This locks capacity isolation, not a claim that shared CPU, Postgres, gateway or
provider infrastructure can never contend. Stronger process/deployment isolation,
operator configuration and downstream resource reservations remain to specify.

## GitHub results

Support both Check summaries/annotations and PR review comments, including
suggested edits when supplied and valid. Use the optional standard review report
with severity, evidence and remediation. Both delivery surfaces use the same
retained report and exact reviewed source identity; provider delivery retries do
not rerun verification or change its verdict.

App permissions must cover these selected delivery surfaces. Report selection and malformed/ambiguous report behavior are locked in
[approved report delivery](approved-report-delivery.md). Exact comment identity,
batching, reconciliation and updates/resolution across commits remain to specify. There is no automatic authorization
to commit fixes, approve merges or broaden Wyrd authority.

## Operator App configuration and tenant installation

Use the agreed conventional install-once/add-repositories flow.
Preserve one active installation per tenant and one
tenant per installation; do not add multi-installation tenant management.

1. The operator registers the deployment's GitHub App in GitHub's App settings
   and configures its deployment URLs, permissions and events. SaaS operators
   own their shared deployment App; self-hosted operators register/configure
   their own App. Store App credentials in existing deployment secret providers,
   never Cards or TOML. Exact configuration fields/URLs remain to specify.
2. A tenant connects/installs that configured App and completes verified tenant
   association through existing Wyrd login and the provider authorization flow.
   A callback installation_id alone is insufficient proof.
3. The tenant selects repositories for that installation. Adding repositories
   updates that same connection. Wyrd learns and records/synchronizes authorized
   repository access; this does not itself declare a versioned Repository Card
   or create Verifier bindings.
4. `wyrd declare wyrd.toml` resolves the connected repository, registers its native
   Repository Card and dependencies and derives/selects its review bindings.
5. Removing repository access stops affected admission/source IO and cancels
   affected unfinished work, retaining Cards and evidence. Unrelated repositories
   continue normally.

### Approved storage amendment — revision 4


`wyrd-sql` owns one dedicated `wyrd.github_installations` table for tenant identity
(`data_tenant_id`), configured host, verified installation ID, selected accessible
repositories, suspension and revision, with connection identity and safe timestamps
as needed. Postgres enforces one associated installation per tenant and unique
`(host, installation_id)` across tenants, including concurrent claims. Suspension
retains ownership. Enable and force standard tenant RLS. Keep
`auth_human_connections`, its OIDC constraints and queries unchanged. An
installation grants repository access, while that table defines tenant OIDC login
trust; sharing it would require nullable OIDC fields and provider filtering in
security-sensitive login queries. The dedicated table avoids that coupling. Reuse
`auth_login_state` for temporary actor/tenant/browser-bound connection transactions
with explicit flow separation: neither callback may consume the other's state,
and stale callbacks cannot restore a disconnected or replaced connection. One
narrow SECURITY DEFINER installation-to-tenant lookup routes verified webhooks,
with pinned search_path and body-scoped operator elevation; subsequent work uses
TenantConn and normal authorization. Routing grants no permission. Store no App
keys or user/installation tokens in these rows. Private columns and generation
mechanics remain implementation-owned.

This is the only new table exception; queues, waits and evidence retain their
existing owners. Product login and public integration contracts are unchanged.

Both SaaS and self-hosting share the same code path. Tenants do not create Apps
or upload operator credentials. Implement conventional verified callbacks,
connection inspection and disconnect/revocation through existing owners. Do not
add a separate disable/claim-management workflow. UI remains
outside this change.

The old App permissions must be updated for the newly approved review-comment
surface: Contents read, Pull requests write and Checks write (Metadata read is
implicit). Publishing suggestions does not authorize source commits.

The user approved revision 5 to use GitHub.com’s conventional App installation
and OAuth flow. Use its documented state, PKCE and installation confirmation.
Enterprise Server compatibility and provider capability negotiation are outside
this change. Production uses GitHub.com’s web and API endpoints; local protocol
mocks serve development and tests.

## Webhook ingress

A webhook is another way to stage an ordinary queued verification run.

1. Verify and durably accept the delivery.
2. Resolve the authorized connected repository from provider identity.
3. Read the selected registered Repository Card version and its PR bindings.
4. Freeze the exact policy/source identity for each logical review.
5. Stage through existing verification submission/queue/runner/engine owners.
6. Recover interrupted handoffs and deduplicate without multiplying executions.

Do not fetch live review policy from the PR branch, run a DAG inside the HTTP
request, or create a separate GitHub verification engine/queue. Provider metadata
does not grant tenant access or authority. Supersession uses the approved
completed-review baseline and current-generation rules.

## Direct execution eligibility

A Verifier containing pull_request extraction, deferred-input suspension or
human approval requires queued execution, including manual invocation. Reject direct invocation before
any step executes; never implicitly convert it to a queued request or run a
partial DAG first. Existing direct-eligible Verifiers retain their direct path.

## Durable waits and resumption

Current scope is deferred evidence inputs and separate human approval, as locked
in [the deferred-input record](approved-deferred-inputs.md). The initial generic
timer/external wait actions were superseded and are not implementation scope.
No new PostgreSQL tables are allowed.

- Persist the pending logical dependency/approval and accepted state before
  parking. Resume the same execution, not a replacement; do not replay settled
  predecessors or replenish spent budgets/retries.
- A blocked branch holds no active block capacity. Independent ready branches
  continue. Once none are active/runnable, release invocation-owned execution,
  workspace, Python and provider resources immediately. Bounded background custody
  then persists the parked state, waits for its ACK, retires the current queue
  delivery and releases the claim permit and renewing lease. A crash before that
  ACK can repeat unpersisted work. (Superseded wording: "persist and ACK the parked
  checkpoint" before releasing resources.)
- Ordinary in-flight tool/model IO remains bounded active work. Resource release
  is not a rule to drop permits on every language-level await.
- Accepted deferred evidence or human decision stages continuation through
  existing queue admission. Reacquire queued capacity and restore accepted state.
- Continuations identify the exact tenant/execution and target slot or logical
  approval occurrence. Validate input schemas. Deferred input may arrive before
  its consumer starts; approval requires its exact pending occurrence.
- Identical idempotent replay returns the accepted receipt. Conflicting decisions
  or input replacements are refused. Cancelled/superseded/terminal executions
  reject new input; old approval cannot authorize a new PR head.
- Approval requires normal authentication, current subject authority and the
  approval permission. The user subsequently rejected recent interactive login
  and the proposed five-minute cutoff. No special approval-proof flow is needed.
  The decision never expands execution authority.
- Rejection is a recorded decision, not an engine crash or automatic cancellation.
  The standard helper fails its requirement; the DAG may branch explicitly.
  Completion of an approval step does not itself mean approval.
- Approval expiry is bounded by the original execution deadline. Missing deferred
  evidence also expires under that deadline; absence is never a pass. Suspension
  does not reset deadlines. Operator ceilings must accommodate intended approvals.
- Cancellation, permission revocation and PR supersession apply while parked.

Exact approval authoring/results, continuation API/proof, accepted-decision ACK,
queue handoff and restart recovery remain to specify under existing Bifrost and
Postgres owners. No cross-store atomic transaction, SQL payload mirror, quorum,
general signal buffer or event-subscription service is selected.
