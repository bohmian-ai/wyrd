# Security domain review — FAIL

Subject: base `dfd19aa129830696d68cf8d4a09c144805847345`, committed head `1beb1aa1980cb03495dfb0778a353604670ba935`, immutable WIP tree `226fdafc56bacc707b9167d1887be9b72f5e6d53`. Generic library inspected at clean `e52c9fee50c94063fc32b8cd9b9d1234e107ae20`. No prior acceptance carried forward. Findings: **SEC-01, SEC-02, SEC-03**.

## Boundary and evidence

Reviewed root/SQL AGENTS, task-review skill, workflow, maintainer guide, security authority (accepted authority, browser sessions, RBAC, revocation and tenant SQL), approved revision-24 GitHub amendment, public contracts and TASK-004. CodeGraph was the initial code navigation. Source inspected against the captured tree; the two untracked automation/worker files' object hashes match its blobs. External protocol checks use [GitHub user-token documentation](https://docs.github.com/en/apps/creating-github-apps/authenticating-with-a-github-app/generating-a-user-access-token-for-a-github-app), [installation-token documentation](https://docs.github.com/en/apps/creating-github-apps/authenticating-with-a-github-app/generating-an-installation-access-token-for-a-github-app) and [webhook signature documentation](https://docs.github.com/en/webhooks/using-webhooks/validating-webhook-deliveries).

| Contract | Source/proof inspected | Result |
| --- | --- | --- |
| Operator-owned App credentials and outbound policy | `config.rs:2330-2520`, GitHub owner construction/JWT/ScreenedTransport; secret-file resolver, redacted Debug, pinned screened HTTP/no redirects; library app/token/OAuth transport | PASS, static |
| Browser, actor, tenant and installation proof | GitHub `service.rs:331-844`, `routes.rs:489-658`; setup SQL locks/DB expiry/spent state; browser middleware current-session resolution and same-origin mutation guard; library `app.rs:319-368,472-540` | PASS for implemented server stages; complete browser UI journey remains outside this security conclusion |
| Scoped grants and source attribution | runtime `Permission::validate/repository`, spec scope coverage; declaration/storage authorization and publisher attribution; actual acquisition order | FAIL: SEC-03 |
| Shared signed ingress resolves verified tenant claim before durable save | `automation.rs:569-617`, SQL `github_deliveries.rs:17-36,100-139`; library `webhook.rs:58-67` checks raw-body HMAC through constant-time MAC verification | PASS for implemented ingress; downstream revocation handling fails SEC-02 |
| Repository reads/cancel require their own grant, independent of shared Verifier | execute/enqueue admission versus inspection `retained/authorized/cancel` and all inspection consumers | FAIL: SEC-01 |
| Removal/disable/revocation stops admitted work, preserving history | connection/repository mutation SQL; delivery worker lifecycle-event match; queue claim, target cache and existing cancellation/fencing owner | FAIL: SEC-02 |
| Trusted paths stay at pinned target commit, never host filesystem/PR-head config | automation `trusted_configuration`, loader `SourceTree` and tree-backed `PathSandbox::resolve/file`; library commit/tree/blob APIs | PASS for inspected path/commit boundary; source authorization separately fails SEC-03 |

`github_app_connection.rs` asserts replay/expiry/actor/browser refusal, selection-subset refusal, tenant claim conflict, disable retaining claim and disconnect releasing it. `repository_verification.rs:205-213` asserts submit denial without repositories:run; it does not cover the missing read/cancel grants. No Cargo, server, live GitHub/GHES or browser execution was performed. Source assertions are evidence inspected, not claimed test runs.

## SEC-01 — Repository execution inspection and cancellation bypass repository grants

**INCORRECT; material authorization boundary.** Public contracts “Execution authorization” and spec REQ-026 / execution inspection require repositories:read for repository status/progress/publications/downloads, and repositories:run for cancellation, alongside the exact Verifier grant.

**Location:** `crates/wyrd/wyrd-server/src/components/verification/service/inspection.rs:287-315,333-378,687-711`. `authorized` checks only Verifier action/UID; `retained` similarly checks only verifier:read. All invocation/publication/content/download readers and `service.rs:244-289::get_execution` consume that authorization. `cancel` calls `authorized` then mutates the queue. No repository check exists in either shared owner.

**Reachable scenario/consequence:** One team uses the same registered Verifier for repositories A and B. A tenant caller with that Verifier's read/run grants and repository A grants, but no repository B grants, presents a known B execution ID. The retained path releases B's result/publication content; the cancellation path cancels its unfinished execution. IDs are selectors, not authorization. This becomes a regression when the new repository subject arm enters existing inspection handlers.

**Smallest correction/proof:** Extend these existing shared inspection authorization owners to derive the exact repository from the accepted subject (or queued frozen subject before an accepted identity exists), and authorize/audit the required repository action before returning data or cancelling. Retained-only executions must retain this rule after queue cleanup; Card behavior remains unchanged. No new permission system, role, grant store or per-endpoint policy is needed. Add one focused server journey with shared Verifier/two repositories proving denied B status/publication/download/cancel, allowed A, and retained-only B denial.

## SEC-02 — Access revocation changes settings but leaves admitted work eligible

**MISSING; material execution authority boundary.** Spec REQ-024/025 and TASK-004 Scenario 4 explicitly require disable/disconnect/repository removal or revocation to stop affected unfinished manual and automated executions while preserving ordinary retained reads and pending terminal delivery.

**Location:** GitHub `service.rs:854-872::deactivate`, `1052-1094::change_repository`, SQL `github.rs:435-455::deactivate_connection` and revoke/reconcile SQL at `199-234`; `worker.rs:209-212` settles installation/access events through the default no-op arm.

**Reachable scenario/consequence:** Admit a repository execution, then disable/disconnect its connection or revoke its repository before queue claim. These operations update only connection/repository settings. `VerifierExecutions::CLAIM_SQL` (`verifier_executions.rs:422-466`) still claims it; runner resolution (`runner.rs:488-512`) and `VerifierCards::resolve` resolve the Verifier without consulting the connection or repository. The run therefore continues under its frozen authority. Running work is likewise not fenced by those mutations. Signed suspension/removal deliveries are accepted durably but then settled without changing current access or cancelling work. Existing principal-revocation cancellation does not help: no principal is revoked in these scenarios.

**Smallest correction/proof:** Extend the existing repository/connection mutation and installation reconciliation owners to cancel the exact affected repository executions through the existing `VerifierExecutions` cancellation/fencing and pending-delivery mechanism, committing revocation with its cancellation control effect. Reuse that path for authenticated installation/access events and explicit reconciliation. Do not cancel the whole publishing/execution principal, because it can serve unaffected repositories. Preserve installation-token and current allowlist checks at subsequent IO, terminal evidence and terminal-delivery ACK obligations; no new scheduler or authority snapshot system is warranted. Focused proof should hold pending and running manual/automatic work, apply each revocation source, and assert cancellation/fenced commits with an unaffected repository still progressing.

## SEC-03 — Trusted configuration is acquired and imported without publisher source access

**INCORRECT; material least-privilege boundary.** Spec REQ-026 and repository declaration requirement (`spec.md:866-871`) require the configured publishing principal's source access as well as normal Card/artifact write authorization. TASK-004 Scenario 2 requires current exact source/declaration/run permission before respective IO.

**Location:** `automation.rs:485-502::adopt`, `100-181::trusted_configuration`, `305-343::declare_local`, `401-451::declare_stage`; `service.rs:1156-1225::repository_client` accepts a tenant/key rather than an authorized caller.

**Reachable scenario/consequence:** An enabled repository selects local YAML/artifacts. Its publishing principal remains active with cards:write, but loses/never holds repositories:read for that repository. A signed PR event reaches the installed worker (`verification/mod.rs:558-565`): `adopt` reads the repository tree/blobs before resolving the publisher; `declare_stage` subsequently checks cards:write/operator invoke and imports the acquired graph under that publisher. No publisher repository read check occurs. Installation/allowlist access is substituted for a Wyrd permission, allowing the source to enter the registry after source authority has been removed. The executor's later repositories:run admission check cannot authorize this earlier publishing operation.

**Smallest correction/proof:** Resolve the current publishing caller in the existing adoption owner and authorize/audit exact repositories:read before acquiring source for local declaration. Carry that already-owned authorization through the existing configuration/declaration workflow; keep publisher and executor grants separate and preserve the registered-selection path's intended rights. Do not add persistent GitHub-user grants, refresh services or organization-owner checks. A focused fake-provider/server case should deny this publisher while retaining cards:write, asserting no source blob acquisition/import/admission; exact source grant then permits the same graph.

## Scope recommendation and limits

Keep the connection task centered on installing the deployment App, proving the current browser/admin/user access, confirming repositories, and managing/revoking the connection. Move trusted graph acquisition/import, repository execution authorization and PR-to-Check processing together to the existing repository-verification task boundary, retaining SEC-01–03 and their proof with that owner. This is sequencing of already approved work through wyrd-plan, not permission to ship repository execution without its security checks or to weaken revocation guarantees.

Skipped: runtime/build validation and unrelated wait/approval policy. Risk: the reviewed incomplete repository path currently crosses real repository authorization and revocation boundaries.
