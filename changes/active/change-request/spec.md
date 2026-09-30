---
id: SPEC-change-request
revision: 1
status: draft
---

# Change Requests

## Objective and user value

Give maintainers, developers, reviewers, and agents one durable record of a
proposed code change: what it intends, which exact commits it contains, where
it reaches in the code, which Claims must hold, and which Verifiers judged
them. A Change Request works for any connected repository. A library published
to a package registry and a registered, deployed Service get the same
workflow; registered Cards add context when they exist and are never required.

Two journeys define this change and must be reproducible end to end:

- **Library.** A maintainer of a library repository (for example `pydantic`)
  opens a PR and a Change Request for it. The repository's `wyrd.toml`
  requires a code-review Verifier. Wyrd pins the PR's exact commits, attaches
  impact and change Evidence, runs the required Verifier on every new head,
  publishes the outcome as a GitHub Check Run, and closes the Change Request
  when the PR merges. Nothing follows the merge.
- **Service.** A developer of a registered Service repository opens a PR and a
  Change Request. The repository's `wyrd.toml` requires code review and an
  LLM-judge Eval. The pre-merge flow matches the library journey, plus the
  Change Request shows the registered Cards whose code the change reaches.
  After merge, the deploy pipeline registers a new Service Card version from
  the deployed commit. Wyrd associates every Change Request that version
  contains. The Service's continuous Verifiers (for example PSI drift) keep
  running against the Card version, and their results belong to that Card, not
  to the Change Request.

## Scope

- Change Request, subject, revision, Claim, and Evidence contracts, durable
  state, lifecycle, and APIs.
- Repository-declared Claims and Verifier requirements in `wyrd.toml`.
- A GitHub App integration: installation linking, signed webhooks, repository
  reads, and Check Run publication, for github.com and GitHub Enterprise
  Server.
- Change-time Verifier runs on the existing generic Verifier runtime.
- The `code_review` Verifier implementation and change-context input for the
  existing `eval` implementation.
- Impact Evidence produced by `bohmian-code-review`'s impact facade.
- Populated, persisted Card `metadata.origin` and the association between
  merged Change Requests and the Card versions that shipped them.
- HTTP, MCP, CLI, and Rust, Python, and TypeScript SDK surfaces.
- Authority updates.

## Non-goals

- Discussions, Comments, mentions, approvals, and overrides (the Review surface
  of `SPEC-wyrd-ui-foundation` REQ-064). They follow as a separate change on
  top of this record.
- Carrying a result forward to a new revision. Every revision is judged fresh.
- UI implementation. `SPEC-wyrd-ui-foundation` replaces its mock view models
  with these contracts in its own change.
- Merge authority. Wyrd never merges, blocks, or approves a PR itself; a
  repository makes the Check Run a required status check if it wants a gate.
- Posting inline PR review comments to GitHub.
- GitLab, Bitbucket, and other providers.
- Opening Change Requests automatically for every PR.
- Routing post-deploy Verifier results to a Change Request.
- Policy-driven requirements, CI-test, external review-system, Python, MCP,
  API, and Workflow Verifier implementations.
- Checking out, building, or executing repository code on the server.

## Dependencies

- `SPEC-verified-change-contract` (approved): Verifier Cards, the generic
  VerificationRuntime, `wyrd.verifier_runs`, and `vala.verification.results`.
  This change extends them; it adds no second runtime or result table family.
- `bohmian-code-review` `SPEC-impact-map` (draft, revision 1): the impact
  facade, symbol keys, entry-point roles, coverage, and per-language accuracy
  gate. This change consumes its released, pinned crates and implements the
  object-storage and snapshot-store traits its pipeline requires.
- Skald Agent execution as already used by Eval `llm_judge`.

## Definitions

- **Change Request**: a tenant-owned, provider-neutral record of one proposed
  change. It carries title, intent, owners, lifecycle, subjects, Claims,
  Evidence, Verifier runs, and history. It is not a Card.
- **Lifecycle**: `draft`, `open`, or `closed`; a closed Change Request records
  `completed` or `cancelled`.
- **Subject**: one member of a Change Request naming a connected repository
  and either a GitHub PR (`pull_request`) or an explicit base/candidate commit
  pair (`commits`). A Change Request may hold several subjects from one or more
  repositories.
- **Revision**: the immutable set of exact base and candidate commits of every
  subject at one moment, numbered from 1. A new revision is created whenever a
  subject's candidate commit changes or a subject is added or removed. Claims,
  Evidence, and runs attach to a revision, so every judgment names the exact
  code it judged.
- **Base commit**: for a `pull_request` subject, the merge base of the PR head
  and its target branch at revision time, which is the base GitHub diffs
  against. For a `commits` subject, the stated base.
- **Claim**: a statement that must be established for a revision, with one or
  more Verifier Cards that judge it, a `run` mode (`automatic` or `manual`),
  and `required` or advisory status.
- **Repository Claim**: a Claim declared in a `wyrd.toml` `[change]` table.
- **Author Claim**: a Claim added to one Change Request through the API.
- **Claim resolution**: `pending`, `satisfied`, or `not_satisfied` for one
  Claim on one revision.
- **Evidence**: immutable, source-free data attached to a revision. The initial
  kinds are `impact` (one per subject) and `change_context` (one per
  revision).
- **Change run**: a Verifier run whose subject is a Change Request revision and
  Claim rather than a Card.
- **Repository link**: the binding of one GitHub App installation to exactly
  one tenant, which makes that installation's repositories connected
  repositories of the tenant.
- **Shipped-in association**: the derived relation between a merged Change
  Request and the first Card version whose origin commit contains its merge
  commit.

## Requirements

### Change Request record

- **REQ-001**: A Change Request MUST be creatable through HTTP, MCP, CLI, and
  all three SDKs with a title, optional intent, optional owners, and zero or
  more subjects. It starts in `draft`. It moves to `open` only when it has at
  least one subject. Draft saves are allowed while incomplete.
- **REQ-002**: A `pull_request` subject MUST be resolved from the provider at
  add time and at every revision: repository identity, PR number, head
  commit, target branch, and merge base. A `commits` subject MUST verify both
  commits exist in the repository. An unknown repository, unconnected
  repository, missing commit, or inaccessible PR MUST fail with a structured
  Wyrd error and create no subject.
- **REQ-003**: A PR MUST belong to at most one Change Request that is not
  `closed`. Adding it to a second MUST fail with a conflict error naming the
  existing Change Request.
- **REQ-004**: Revisions MUST be immutable and ordered. Creating a revision
  MUST freeze each subject's base and candidate commits, the resolved Claims,
  and the exact Verifier Card UIDs and versions. Results for an earlier
  revision become stale; they remain readable and never count toward the
  current revision.
- **REQ-005**: A Change Request MUST close `completed` automatically when every
  `pull_request` subject has merged, recording each merge commit. It MUST
  close `cancelled` automatically when every `pull_request` subject is closed
  unmerged. Any other mix leaves it open for an explicit close. An explicit
  close or reopen MUST be authorized and audited. A Change Request with only
  `commits` subjects closes only explicitly.
- **REQ-006**: Every Change Request MUST expose an ordered history of revision
  creation, subject changes, run creation and settlement, Check Run
  publication, lifecycle transitions, and shipped-in associations, with actor
  (principal or provider event) and time.

### Repository requirements

- **REQ-007**: `wyrd.toml` MUST accept a `[change]` table declaring
  repository Claims:

  ```toml
  [[change.claims]]
  id = "code-review"
  statement = "The change passes code review"
  verifiers = [{ name = "code-review", version = "1" }]
  run = "automatic"      # automatic | manual; default automatic
  required = true        # default true
  ```

  `id` is unique within its file. Each verifier entry is a Verifier CardRef
  without `kind`; `space` falls back to `wyrd.toml` defaults. Unknown keys
  MUST fail parsing, consistent with the existing file.
- **REQ-008**: For each revision, the server MUST read requirements at each
  subject's **base** commit, never the candidate. For every changed file it
  applies the nearest ancestor `wyrd.toml` within the repository, using the
  same discovery rule the CLI uses. The revision's repository Claims are the
  union over all changed files, identified by (repository, file path, `id`).
  A PR that adds, edits, or deletes a `wyrd.toml` cannot change the
  requirements it is judged by.
- **REQ-009**: Each Verifier reference MUST resolve against the tenant
  registry when the revision is created and be pinned to an exact UID. An
  unresolvable reference, or a `run` mode the Verifier implementation cannot
  honor, MUST leave that Claim `pending` with a visible configuration error.
  It MUST NOT be silently dropped. An invalid `wyrd.toml` MUST do the same for
  every Claim it would have declared.
- **REQ-010**: An author MAY add Author Claims (required or advisory) to a
  Change Request with `changes:write`. Author Claims MUST NOT remove, replace,
  or downgrade a repository Claim.

### Evidence

- **REQ-011**: Each revision MUST receive one `impact` Evidence per subject.
  It is the projection returned by the `bohmian-code-review` impact facade for
  the subject's exact base and candidate commits, stored with its format
  version, coverage, per-language resolution summary, and truncation facts.
  Languages the facade reports as not analyzed or below its accuracy bar MUST
  remain visible as such.
- **REQ-012**: When registered Card versions in the tenant carry an origin in
  the same repository, `impact` Evidence MUST also record which of those Cards'
  origin paths contain reached modules. When none match, the Evidence is
  complete without them. No Change Request operation requires a Card other
  than the Verifier Cards its Claims name.
- **REQ-013**: Each revision MUST receive one `change_context` Evidence. It is a
  versioned, source-free document with title, intent, Claims, and per subject:
  repository, commits, PR number and title, changed-file paths with status and
  line counts, and the impact summary.
- **REQ-014**: Evidence MUST be immutable, content-addressed, tenant-scoped,
  and stored in object storage through `wyrd-storage`, with its identity,
  kind, digest, and size in Postgres. Evidence MUST NOT contain repository file
  contents or diffs.

### Change runs

- **REQ-015**: When a revision is created on an `open` Change Request, the
  server MUST enqueue one change run for each (Claim, Verifier) of every
  `automatic` Claim. A `manual` Claim runs only through an authorized manual
  request. Opening a draft creates its first runs. A run for a stale revision
  that has not started MUST be cancelled.
- **REQ-016**: `wyrd.verifier_runs` MUST gain a change origin whose frozen
  subject is (Change Request, revision, Claim) instead of a Card. The Card
  subject columns are null for change runs, and existing Card-subject runs
  are unchanged. Change runs use the existing claim, lease, retry, timeout,
  settlement, concurrency ceilings, and PostgreSQL clock. They never dispatch
  Operators.
- **REQ-017**: `vala.verification.results` MUST carry nullable Change Request,
  revision, and Claim identity. `subject_card_uid` becomes nullable and is
  null exactly when the Change identity is present. Completed change runs MUST
  write the common result. Existing Drift and Eval semantics are unchanged.
- **REQ-018**: At settlement, the server MUST record each change run's verdict
  against its Claim in Postgres and recompute that Claim's resolution in the
  same transaction:
  - `satisfied` when every pinned Verifier's current-revision run completed
    `passed`;
  - `not_satisfied` when any completed `failed` or `inconclusive`;
  - otherwise `pending`.

  A run that ends `errored`, `timed_out`, or `cancelled` leaves the Claim
  `pending` and marks it as needing attention. The Bifrost result stays the
  authoritative verdict record.
- **REQ-019**: `POST /v1/verification/runs` MUST accept a `change_claim`
  target (Change Request, revision, Claim, Verifier). It requires `evals:run`
  plus `changes:read` on the Change Request, is audited, and reuses the
  existing `Idempotency-Key` contract. A manual run is valid only for the
  current revision. It is how `manual` Claims run and how any Claim reruns.

### Verifier implementations

- **REQ-020**: `VerifierImplementation` MUST gain `code_review` with an
  `agent` (`InlineableRef<AgentSpec>`) and `fail_on` severity (`blocking`,
  `major`, or `minor`; default `blocking`). It is valid only for change runs.
  Registration MUST reject binding it through `verified_by`.
- **REQ-021**: A `code_review` run MUST review every changed hunk of every
  subject in the revision:
  1. Source is fetched transiently through the `bohmian-code-review` pipeline,
     which ranks and packs related context.
  2. The Agent runs one structured-output turn per bounded batch of changed
     files.
  3. It returns findings with path, candidate-side line range, severity,
     title, and body. Findings from all batches are unioned.
  4. The verdict is `failed` when any finding meets `fail_on`, and `passed`
     otherwise.

  The verdict is `inconclusive`, with a named reason, when:
  - a single file's changed hunks cannot fit a turn;
  - the server's per-run turn ceiling is reached; or
  - the Agent's output fails the findings schema after its retries.

  Findings MUST be written to a new Bifrost detail table
  `vala.code_review.result_findings`, joined to the common result on
  (`data_tenant_id`, `result_id`).
- **REQ-022**: An `eval` Verifier MUST accept a change run whose input record
  is the revision's `change_context` document plus a transient `patches` field
  with each changed file's unified diff. Existing Eval tasks, `llm_judge`,
  scoring, and `vala.eval.result_items` apply unchanged. Patches and any
  captured context derived from them MUST NOT be persisted.

### GitHub App

- **REQ-023**: GitHub App credentials (app ID, private key, webhook secret)
  MUST come from deployment configuration through the deployment secret
  provider, one App per configured host (github.com or a GitHub Enterprise
  Server). Tenants never supply App secrets. Installation tokens are minted
  server-side, held in memory until expiry, and passed to the
  `bohmian-code-review` pipeline.
- **REQ-024**: A tenant administrator with `repositories:write` MUST link an
  installation through the App's setup flow. The server MUST prove the
  installing GitHub user can access that installation (user-to-server
  authorization, then confirming the installation is in that user's
  installations) and bind the request to the Wyrd session with a
  single-use state value. An installation links to exactly one tenant. A
  second tenant's claim MUST fail. Link, unlink, and uninstall MUST be
  audited, and an uninstall webhook MUST unlink it.
- **REQ-025**: `POST /v1/providers/github/webhooks` MUST:
  - verify `X-Hub-Signature-256` before parsing;
  - bound the payload size;
  - deduplicate on `X-GitHub-Delivery`;
  - resolve the tenant only through the installation link;
  - acknowledge within GitHub's delivery timeout by durably enqueuing work.

  The response MUST be the same for an unknown or unlinked installation, and
  it creates no tenant state. A webhook payload is only a hint: the worker
  MUST re-read PR state from the GitHub API before changing a subject or
  revision.
- **REQ-026**: The server MUST converge PR subjects without relying on webhook
  delivery. Each PR subject is refreshed on its next webhook, on an explicit
  refresh request, and by a bounded periodic reconcile of open `pull_request`
  subjects. A deployment without public webhook ingress works through refresh
  and reconcile alone.
- **REQ-027**: GitHub API calls per revision MUST depend on the number of
  changed files and subjects, never on the number of commits in the PR. This
  follows `bohmian-code-review`'s endpoint-only model and archive fallback.
- **REQ-028**: For every current `pull_request` subject head, the server MUST
  maintain one Check Run and update it only when its conclusion or summary
  changes:
  - `success` when every required Claim is `satisfied` (or none exist);
  - `failure` when any required Claim is `not_satisfied`;
  - `in_progress` while a required Claim has an active run;
  - `action_required` when a required Claim is `pending` with no active run.

  The summary lists Claims, their resolutions, and a link to the Change
  Request. Advisory Claims never change the conclusion.
- **REQ-029**: Webhook-driven and reconcile-driven transitions evaluate no
  principal permission. They MUST be recorded in Change Request history as
  provider events, not as authorization audit. Every human or agent operation
  MUST be authorized and audited through the canonical path.

### Card origin and shipped-in association

- **REQ-030**: The CLI and SDK register path MUST populate
  `metadata.origin` from the local Git repository:
  - normalized repository identity from the fetch remote;
  - `HEAD` commit;
  - the apply-root path relative to the repository root;
  - `dirty`.

  The server MUST persist origin with the Card version. A change of origin
  commit MUST produce a new Card version even when the spec is unchanged.
  Registration outside a Git repository omits origin, as today.
- **REQ-031**: When a Card version is registered with a clean origin in a
  connected repository, the server MUST associate each merged Change Request
  whose merge commit is contained in that origin commit and was not contained
  in the previous version's origin commit. It MUST also have changed at least
  one file under the origin path. The derivation MUST use a bounded, paginated
  comparison between the two origin commits, not one call per Change Request.
  It MUST work for merge, squash, and rebase merges. For a first version, only
  Change Requests merged after the repository was linked are considered.
  Truncation MUST be recorded, never guessed around.
- **REQ-032**: The association MUST be readable from both sides: a Change
  Request lists the Card versions it shipped in, and a Card version lists the
  Change Requests it contains. Post-deploy Verifier results stay on the Card
  version. The Change Request never copies, aggregates, or re-resolves them.

### Surfaces and permissions

- **REQ-033**: Wire request and response types MUST live in `wyrd-spec` with
  schemas and stable `WyrdError` codes. `wyrd-client` MUST project one Changes
  capability to Rust, Python, and TypeScript. It covers:
  - create, get, list (filter by lifecycle, repository, needs attention, Card
    version), update, open, close, and reopen;
  - add and remove a subject, refresh a subject, and add an Author Claim;
  - get Evidence and list shipped-in associations.

  Manual runs use the existing Verification capability.
- **REQ-034**: MCP MUST expose `changes.get`, `changes.list`, and
  `changes.get_evidence` as read tools. `changes.create`, `changes.update`,
  `changes.add_subject`, `changes.add_claim`, and `changes.refresh` are write
  tools requiring an explicit scope.
- **REQ-035**: The CLI MUST provide `wyrd change` commands for create (from a
  PR URL or a commit pair), get, list, open, close, refresh, and run.
- **REQ-036**: Permissions MUST be:
  - `changes:read` for reads;
  - `changes:write` for create, update, lifecycle, subjects, and Author
    Claims;
  - `repositories:read` and `repositories:write` to list and link
    installations and repositories.

  Runs reuse `evals:run`. Every evaluation MUST be audited, allowed and
  denied alike.
- **REQ-037**: Change Request, subject, revision, Claim, Evidence, repository
  link, and association state MUST live in the `wyrd` Postgres schema, with
  migrations and typed operations owned by `wyrd-sql`. Every table carries
  `data_tenant_id` with forced RLS and is reached only through `TenantConn`.
  Only webhook tenant resolution and the periodic reconcile's work discovery
  use explicit `OperatorPool` reads.

### Authorities

- **REQ-038**: Before completion, update:
  - `architecture/wyrd-design.md`: Change Requests, `wyrd.toml [change]`,
    change runs, the `code_review` implementation, origin persistence and
    versioning, shipped-in association, and the GitHub App.
  - Doctrine rule 7 in `wyrd-design.md` and `wyrd-doctrine.mdx`: it MUST state
    that a source-provider App's signed events and its Check Run publication
    are permitted provider coordination, distinct from observation data,
    which Wyrd still never receives by push or writes externally.
  - `architecture/wyrd-security-posture.md`: webhook ingress, installation
    linking, and App credentials.
  - Public docs for the CLI, SDKs, and MCP.

## Invariants

- **INV-001**: The server never checks out, builds, or executes repository code.
  Source is read through provider APIs, held transiently, and never
  persisted, including in Evidence, results, and captured Eval context.
- **INV-002**: Requirements are read at the base commit. A candidate cannot
  change what judges it.
- **INV-003**: A Change Request never depends on a subject Card. Libraries and
  Services follow the same path; Cards only add context and the shipped-in
  association.
- **INV-004**: Results belong to what they judged. Change-run results name a
  revision and Claim. Card-subject results name a Card version and are never
  attributed to a Change Request.
- **INV-005**: Every judgment names exact commits and an exact Verifier Card
  version. A stale result never counts for the current revision.
- **INV-006**: An installation, and therefore each repository, belongs to at
  most one tenant. No provider event can create or reveal state in another
  tenant.
- **INV-007**: Execution status, verdict, and Claim resolution stay
  independent. Only completed runs have verdicts.
- **INV-008**: Provider API cost is independent of commit count.

## Expensive-to-reverse decisions

- A Change Request is a Postgres record, not a Card kind.
- The revision is the unit of judgment, and it is created on every candidate
  change.
- Requirements live in `wyrd.toml [change]`, are read at the base commit, and
  are unioned by nearest-ancestor file.
- Change runs extend `wyrd.verifier_runs` and `vala.verification.results`
  rather than adding a parallel runtime or result family.
- Claim resolution is Postgres control state, derived at settlement.
- `code_review` is an in-server Agent-backed implementation, and
  `vala.code_review.result_findings` is its new detail table.
- Evidence is source-free object-storage data with Postgres identity.
- The GitHub App uses deployment-owned credentials, installation-to-tenant
  linking, and webhooks as hints with API re-reads, plus reconcile.
- The Check Run is the only way a repository gates merge on Wyrd.
- Origin is persisted, and an origin commit change creates a new Card
  version. The shipped-in association is derived from origin commits.
- New permissions: `changes:read`, `changes:write`, `repositories:read`, and
  `repositories:write`.
- `bohmian-code-review` becomes a pinned Git dependency of the server-tier
  Change Request owner only. Client-tier crates never depend on it or on
  tree-sitter.

## Acceptance criteria

User journeys run against `WyrdTestServer`, repository-managed Postgres, a local
fake GitHub (REST, archives, signed webhook delivery, and Check Run capture),
and the local development model provider. They run in the gated journey lanes
for Rust, Python, and TypeScript, with MCP where noted.

- **AC-001 Library journey**:
  1. Link an installation and connect a repository that has no registered
     Cards and a root `wyrd.toml` requiring `code-review`.
  2. Open a PR, create and open a Change Request for it from each SDK, and
     reach it through MCP.
  3. Observe revision 1 with pinned commits, `impact` Evidence whose entry
     points are `exported_api`, `change_context` Evidence, one code-review run
     with findings, the Claim resolution, and a Check Run conclusion.
  4. Push a commit. Observe revision 2, the stale revision-1 result, a fresh
     run, and an updated Check Run.
  5. Merge. Observe `closed/completed` with the merge commit.
  6. Register nothing; there is no shipped-in association.
- **AC-002 Service journey**:
  1. Use the same flow for a repository with a registered Service whose
     origin is in it, and a `wyrd.toml` requiring `code-review` (automatic)
     and an Eval `llm_judge` Claim (manual).
  2. Observe `impact` Evidence naming the Service, and the manual Claim
     `pending` with `action_required` until an authorized manual run
     resolves it.
  3. After merge, register a new Service version from a later commit. Observe
     the shipped-in association from both sides, including a second Change
     Request merged in the same interval.
  4. Run the Service's scheduled PSI Verifier. Observe its result on the
     Service version and absent from the Change Request.
- **AC-003 Requirements and negative flows**:
  - A PR that edits `wyrd.toml` is judged by the base file.
  - A monorepo change is judged by the union of two nested files.
  - An unresolvable Verifier reference or an invalid file shows a
    configuration error on the Claim.
  - An Author Claim cannot downgrade a repository Claim.
  - A second open Change Request for the same PR is refused.
  - An under-privileged token is refused and audited for create, write,
    run, and link.
- **AC-004 Webhook and installation security**:
  - Bad signature is rejected before parsing.
  - A duplicate delivery is processed once.
  - Unknown and unlinked installations get an identical response and no
    state.
  - A forged payload with a head the API does not report creates no
    revision.
  - Linking an installation the GitHub user cannot access fails.
  - A second tenant's claim fails.
  - Uninstall unlinks.
  - With webhooks disabled, refresh and reconcile reach the same revision.
- **AC-005 Provider cost**: a 100-commit PR and a 1-commit PR with the same
  endpoint files make identical GitHub API call counts per revision. Check Run
  writes occur only on conclusion or summary changes.
- **AC-006 Verifier behavior**:
  - `code_review` covers every changed hunk across batches.
  - An oversized file, the turn ceiling, and invalid output each yield
    `inconclusive` with their reason.
  - Findings join their result.
  - `code_review` bound through `verified_by` is rejected at registration.
  - The Eval change input receives patches, and no patch appears in any
    persisted table or object.
- **AC-007 Runtime reuse**:
  - Change runs survive restart through lease reclaim.
  - Stale-revision runs that have not started are cancelled.
  - Change runs never dispatch Operators.
  - Existing Drift and Eval journeys pass unchanged.
- **AC-008 Origin**:
  - Register from a Git repository and observe persisted origin.
  - A new commit with an unchanged spec creates a new version.
  - A dirty origin is persisted but never associated.
  - The association is correct for merge, squash, and rebase merges, and for
    a path-filtered monorepo.
  - Comparison truncation is recorded.
- **AC-009 Contracts**: schemas and generated stubs regenerate cleanly. The
  served OpenAPI document includes the new operations. MCP runtime tests
  cover the new tools. Error codes are catalogued.
- **AC-010 Authorities**: REQ-038 updates are merged, and the scoped gates
  pass.

## Open material decisions

- **Confirm `code_review` is in-server.** This draft makes code review an
  Agent-backed Verifier implementation that runs inside Wyrd over
  `bohmian-code-review` context. The alternative is an external review
  service (for example CodePal) that returns the portable result contract.
  In-server keeps the journeys reproducible under `WyrdTestServer` and reuses
  Skald. External keeps review logic out of Wyrd, but needs an external-Verifier
  contract first.

## Authority links

- `AGENTS.md`, `architecture/agent-rules.md`
- `architecture/wyrd-design.md` (doctrine rules 3, 7, 21; Trigger; Workspace
  config)
- `architecture/wyrd-doctrine.mdx`
- `architecture/wyrd-security-posture.md`
- `changes/active/verified-change-contract/spec.md` (REQ-061–063, 078–079,
  085, 115, 119, 135–137, 145–146, 152)
- `changes/active/wyrd-ui-foundation/spec.md` (Change Request definitions,
  REQ-060–065, REQ-099)
- `../bohmian-code-review/changes/active/impact-map/spec.md` (REQ-013, 019,
  020)
- `crates/wyrd-spec/src/origin.rs`, `crates/wyrd-spec/src/card/verifier.rs`,
  `crates/wyrd/wyrd-sql/src/queries/verifier_runs.rs`

## Revision history

- **Revision 1 (2026-09-29, draft)**: Initial specification from the library
  and Service journeys. Key corrections made during design review:
  - post-deploy results belong to the Card, not the Change Request;
  - `verified_by` stays reserved for principals, and repository requirements
    live in `wyrd.toml`;
  - provider webhooks via a GitHub App are adopted;
  - the Change Request to Card association is derived from origin commits,
    because production observations carry no commit.
