# TASK-004 behavior review — FAIL

Subject: base `dfd19aa129830696d68cf8d4a09c144805847345`, committed candidate
`1beb1aa1980cb03495dfb0778a353604670ba935`, authorized immutable WIP tree
`226fdafc56bacc707b9167d1887be9b72f5e6d53`. First replacement-task review;
no acceptance carried forward. Compared cumulative base→tree and HEAD→tree
separately. CodeGraph exploration preceded source navigation. Directly inspected
source files cited below match captured blobs by Git hash. No source edits,
builds, tests, other reviewer reports, or implementation summary were used.

Authority: revision24 scoped GitHub amendment; original installable-App task,
public-contracts browser/tenant operations, TASK005/006 dependency boundaries,
execution-state supplement; root/nested AGENTS, workflow, maintainer style,
architecture authority map, Card authoring and security authorization sections,
and TESTING.md. This report owns behavior and task scope; security, data,
resilience and repository specialists independently assess their assigned seams.
Static PASS below means inspected source/assertions support that bounded property;
it does not claim tests ran against this WIP tree. The orchestrator supplied
`mise exec -- cargo check --locked -p wyrd-server --features test-support` PASS
against hash-matching captured source; compilation does not close the behavioral gaps.

## Acceptance matrix: all 44 ledger rows, grouped by team outcome

Each O-ID occurs once. Partial outcomes are FAIL rather than silently dropping
unfinished parts. These are coverage groups, not separate findings per O-ID.

| Obligations | Source and assertion evidence | Result |
|---|---|---|
| O-01, O-02, O-03, O-07, O-09, O-22 | `config.rs::GitHubAppConfig`; `github/mod.rs` config/signing/screened transport and explicit installation/OAuth origins; `service.rs::prove_installation` required permissions; operator registration guide; connection journey asserts configured redirects, unconfigured refusal, safe fields. No new quota/limits API in cumulative diff. | PASS, static; no rerun |
| O-05, O-06, O-17, O-18, O-19, O-23, O-39 | `service.rs` connect/setup/callback/selection/confirm and SQL setup/selection owners. Rust journey asserts separate PKCE exchange, replay/foreign actor/tenant/expiry refusals, outside selection 400, selected 101 allowed and PR-enabled, claim conflict, reconnect preserving old access, API-key selection/confirm refused. This is credible server-flow evidence, independent of missing UI. | PASS, bounded server flow |
| O-04, O-20, O-36, O-37, O-38, O-40, O-41 | No `/github/setup` or tenant settings/GitHub UI; callback redirects there. GitHub-first Rust assertion only checks Location (journey lines 635–646). Python/TS/CLI/MCP connection projections and requested HTTP field/status parity proof absent. Suspension/missing permission are asserted, but pending-approval restart and full normal browser journey are not. Typed existing HTTP declarations and Rust client are partial evidence only. | FAIL — BEH-01 |
| O-08, O-21, O-32, O-33 | SQL claim cardinality and disable/disconnect transitions exist; Rust journey asserts disabled claim retained and released claim reusable. `service.rs::deactivate` and `change_repository` merely commit control-row changes; no affected active-work cancellation. Worker ignores installation/access events. No publication principal/write/terminal-ACK path to assess. Thus full obligations exceed proven connection-state behavior. | FAIL — BEH-02 |
| O-10, O-43 | Cargo.toml/Cargo.lock pin `e52c9fee…`; server is the production consumer. Rust journey asserts configured GHES origins. Scope records clean external descendant. No library protocol execution was supplied to this reviewer; external implementation/protocol coverage remains subject to orchestrator evidence, not a new implementation finding. | FAIL for complete acceptance proof; pin/connection half supported |
| O-11, O-12 | Repository target wire arm and pinning exist; repository journey asserts Card raw subject shape, enqueue 202, both/neither 400 without provider calls, repository grant denial, base/head/ref from GitHub. SQL binding test asserts branch identity/revision/retirement. Worker then repins during enqueue, so its adopted config/Check can disagree with actual execution subject. | FAIL — BEH-05; O-12 itself supported |
| O-13, O-14, O-15, O-16 | Shared SourceTree and local declaration staging are present. WIP `trusted_configuration/adopt` consume pinned base; bindings reconcile digest and retire removed selections. Configuration journey currently proves only manual repository target behavior, not imports/bindings/invalid config. Cloud-backed artifact imports explicitly refuse; readiness is checked only per member during subsequent enqueue. | FAIL — BEH-03/04; runtime configuration-Check delivery also BEH-02 |
| O-24, O-25, O-26, O-27, O-28, O-29 | WIP HMAC ingress persists to delivery SQL before 202; claim/lease and bounded due listing exist. Worker handles selected PR actions and branch deletion, explicitly settles rerequests/push/installation events without their required effects. No shared PR ordering/supersession authority is applied before adoption/admission. Required event and lease-takeover journey selectors absent. | FAIL — BEH-02 |
| O-30, O-31, O-34, O-35 | Worker records SQL Checks, but no remote create/update/reconciliation consumer; SQL Check module only inserts/lists. No retained-state reader to drive conclusion transitions, delivery inspection or retry. No Check delivery/revocation journey. | FAIL — BEH-02 |
| O-42 | Separate tenant connect and operator configuration guides exist. Their separation is correct; tenant guide currently directs users to unimplemented UI (BEH-01). | PASS for separation only |
| O-44 | No tenant App-key surface, custom login, extraction pipeline, source-write/comment workflow, or Workflow/Repository Card added. Reuse constraints around new delivery loop/loader orchestration remain for maintainer/system review; full no-partial-suite and no-new-scheduler constraints are not established. | FAIL for complete non-goal acceptance — BEH-03 and specialist coverage |

The recorded connection test result at an earlier candidate is a claim with
inspectable assertions, not WIP verification. `repository_verification::
trusted_target_configuration_reuses_shared_declaration` currently does not load
`wyrd.toml`, import its graph, reconcile bindings or receive a Check. Its name
cannot discharge Scenario2. The SQL `branch_bindings_keep_revise_and_retire_per_target`
test proves binding math, not the complete PR journey.

## Material proposals

### BEH-01 — MISSING: neither installation entry point completes through the product

Obligations O-04/36–41; REQ024/032 and public-contracts Browser onboarding.
Locations: `github/routes.rs:36,494–520`, `service.rs:564–567`, UI route tree;
Python/TS public packages, CLI and MCP have no GitHub connection capability.

Wyrd-first callback redirects to `/t/{tenant}/settings/github`; GitHub-first
Setup without state redirects to `/github/setup?installation_id=…`. Neither
route exists in the candidate. The Rust journey checks the latter redirect and
stops there. An admin following Connect GitHub guide steps1–3 cannot sign in,
choose a tenant/principals and confirm verified repositories through the normal
flow; headless users outside Rust/HTTP cannot start/manage it either.

Correction boundary: complete the existing UI/BFF/login integration over the
Rust control operations and thin shared-client projections. Do not create a
second OAuth implementation or permit an API key to complete human confirmation.
Keep connection work separate from execution/result UI (TASK006). Closure proof:
both real browser entry points reach an active selected repository, plus the
existing surface-specific focused start/status/control and denial proof. Fix the
guide alongside the flow so it only promises available controls.

### BEH-02 — MISSING: saved PR work never delivers the promised Check lifecycle

Obligations O-08/21/24–35; task Scenarios3–4 and REQ027/028/030.
Locations: WIP `worker.rs:116–137,157–213,233–315`;
`github_checks.rs:21–58,130–172`; `service.rs::deactivate/change_repository`.

A valid signed opened PR can adopt bindings, enqueue and insert a `queued` SQL
Check. There is no consumer that reads retained selected Bifrost execution state
and calls provider Check create/update; configuration failures likewise only
insert a SQL `completed/action_required` row. The ordinary happy path therefore
never shows a Check on GitHub. Rerequest/push/installation-access events hit the
explicit catch-all `Ok(())` and are durably marked finished. Close/draft events
do not cancel work; disable/disconnect/revoke only change control rows. These
are one unfinished repository-to-Check journey, not separate findings for every
ledger promise. Claim settlement alone is not admission/effect fencing.

Correction boundary: finish the approved repository lifecycle over existing
queue/claim/admission/cancellation and delivery/effect owners, with shared PR
authority for ordinary/rerequest work and current connection/principal authority.
Remote delivery reads retained accepted state, preserves ACK/ambiguity/terminal
monotonicity and never executes verification. Reuse generic library Check APIs;
do not introduce another verification scheduler or weaken revocation. Focused
proof must observe one real remote Check for a no-input Verifier, configuration
failure Check, intentional rerequest identity versus redelivery, supersession,
revocation of manual/automated work and lost-response delivery recovery. This is
substantial unfinished approved work, not a small patch to an App setup task.

### BEH-03 — INCORRECT: suite validation occurs after earlier members are admitted

Obligations O-15; Scenario2; REQ035/038 no partial-suite dispatch.
Locations: `automation.rs:493–550`, `worker.rs:281–315`, existing
`verification/service.rs:723–736,954–1007`.

`adopt` only reads each exact Card, with an explicit comment deferring input
compatibility to admission. Then `dispatch` calls `enqueue` sequentially.
A suite with ready ordinary Verifier A followed by active drift/window Verifier
B passes Card reads; A is staged, then B is refused by `VerificationControl::resolve`
as unsupported. Thus invalid configuration has already dispatched its valid
subset. The same issue applies to per-Verifier run grants checked by enqueue.
This is a normal authored suite error with real work/cost, explicitly prohibited.

Correction boundary: reuse the existing verifier resolution/readiness and
permission decision before any suite member is admitted; do not build a second
readiness definition in `adopt`. Preserve existing execution behavior for ordinary
missing evidence. Proof: A followed by an unsupported/not-runnable B admits zero
executions and produces the single configuration failure outcome. This bounded
correction is needed independently of completing the remote delivery loop.

### BEH-04 — INCORRECT: local definition imports require LocalFs storage

Obligations O-13; Scenario2 transitive artifact/Prompt/Agent imports; root AGENTS
shared deployment behavior. Location: `automation.rs:422–440::declare_stage`.

For any declaration upload plan other than LocalFs, the new code returns
unavailable("importing repository artifacts needs the local storage backend").
A team using supported cloud object storage cannot activate the same trusted
artifact-backed definitions; retry eventually settles the event unprocessed.
The approved capability includes these imports and SaaS/self-hosted equivalence.

Correction boundary: use the existing server storage backend owner to write the
ordinary authorized upload destination and retain normal upload completion and
Card completion validation; do not add another upload service/client protocol.
Proof: equivalent local graph activation through a non-LocalFs upload plan,
including interrupted upload retry, dependency order and publisher attribution.
The maintainer/storage review should identify the exact existing backend method;
this report does not prescribe a new wrapper.

### BEH-05 — INCORRECT: configuration/Check pin and admitted execution can diverge

Obligations O-11, task exact pinned base/head and REQ036/037.
Locations: `worker.rs:180–185,281–315` and
`verification/service.rs:723–736`.

The worker fetches PR pin S1, adopts S1 base config and derives its idempotency
key and Check record from S1. `dispatch` passes the unpinned RepositoryTarget to
ordinary enqueue, which fetches PR again as S2. A normal synchronize or base
update between requests means execution uses S2 while binding configuration and
Check head remain S1. A second binding can even see a third PR revision. Exact
subject evidence no longer proves what the Check reports.

Correction boundary: reuse ordinary queue admission with the server's already
trusted immutable pin, or refuse/restart if movement is detected before any
member is admitted. Keep caller-supplied commit pins untrusted; manual execute/
enqueue must retain their existing server pinning. Do not merely rewrite the
Check head after execution, since the selected config is also affected. Proof:
script GitHub PR movement between initial pin and admission; zero mixed-pin work,
or every execution, binding revision and Check consistently uses the same pin.

## Task-scope recommendation requested by the user

The task really bundles two capabilities. Its title promises installation and
PR Checks; Scenario1 is onboarding, while Scenarios2–4 are an entire repository
verification lifecycle (trusted declaration, branch binding, durable admission,
execution identity, cancellation, remote effect delivery and recovery). These
requirements are approved: deleting them as checklist overkill would change the
promised outcome. The WIP source confirms the breadth, crossing shared loader,
queue subject, runtime, SQL controls, storage, public clients and delivery.

Recommend a focused **connect and manage repositories** task: operator config
and generic OAuth extension/pin; both browser entry points; existing tenant/
principal selection; verified selection/confirmation; connection and repository
read/disable/reconnect/disconnect controls on all promised surfaces; separate
operator/team docs; and focused browser/client journeys. Security, current
installation/repository/principal authority, tenant cardinality and retained
history remain mandatory. Keep any already-supported active-work fencing in
its existing owner; connection state changes must supply the revocation handoff
that the runtime consumer will honor. Do not claim active runtime cancellation
proved when no such runtime journey has run.

Make **a PR produces its configured Check** the next dependency-owning task,
carrying every Scenario2–4 obligation and relevant Scenario5 inspection surface:
trusted registered/local definitions and full graph support; complete-suite
validation; branch revision/removal; durable ingress and PR generations;
shared ordering/leases; exact pins; ordinary execution; independent Check
publication and failure inspection; access removal/cancellation; replay and
recovery. Prove the no-input ordinary Verifier end to end before TASK005 adds
pull_request extraction. TASK005 must depend on the runtime task as well as
connection, retain maintenance consumption, and must not inherit missing event
or Check work silently. TASK006 retains execution UI/combined education; it
currently explicitly relies on TASK004 for connection UI, so update that link.

This is a task decomposition through `wyrd-plan` under unchanged approved
behavior, not permission to declare TASK004 complete now. If the desired product
change is to ship only setup and abandon or weaken PR-to-Check/security promises,
that requires explicit spec revision and human approval. No new product decision
is needed merely to sequence the two cohesive outcomes honestly.

Skipped: build/test/codegen execution and independent library protocol reruns;
no earlier implementation acceptance was assumed. Risk: the captured WIP cannot
be accepted as either the complete onboarding product or PR-to-Check capability.
