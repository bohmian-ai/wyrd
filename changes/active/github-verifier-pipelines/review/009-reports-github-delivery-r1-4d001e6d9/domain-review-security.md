# TASK-009 security, RBAC and tenancy review

Result: **FAIL**.

Subject: base `d07c55158`, candidate `4d001e6d9ed3daf88bf81995510319373bd8c353`, complete three-dot diff. HEAD remains the candidate. Working-tree changes outside the review reports were excluded. No prior acceptance evidence was carried forward.

## Boundary and authority

Reviewed current installation/repository access and PR generation before provider mutations; tenant-scoped token and record access; authenticity of source used to validate report anchors; provider response/URL handling and safe errors; HTTP/MCP execution-inspection authorization. Governing authorities were root AGENTS.md, wyrd-sql/AGENTS.md, spec revision 10 REQ-005/012/013/014/016 and INV-001/005/006/007, original TASK-009 exact-source and delivery obligations, wyrd-design.md queued PR delivery section, and wyrd-security-posture.md GitHub App association, outbound credentials, execution inspection and Postgres tenant binding sections. The task-review domain and materiality requirements were applied. CodeGraph was used before source discovery.

## Source coverage

| Boundary | Source traced | Assessment |
|---|---|---|
| Current access and generation | github/delivery.rs `attempt`, `allowed`, `Write::send`; github/service.rs `installation_token`, `installation_access`, `hold_binding`, `disconnect`, `fence_and_release`, webhook removal; github/pull_requests.rs admission/observation; SQL pull_requests.rs generation/recovery reads | Tenant-bound existing access owner is reused, repository token is narrowed and slot revision is rechecked after mint; delivery introduces an intervening provider lookup before the protected write (SEC-009-1). |
| Provider transport and credentials | github/app.rs token mint, check/review lookup/create/update, `pages`, `write`, `answer`, URL-origin and bounded-body helpers | SecretString bearer tokens remain transient; permissions are contents read, pull_requests write, checks write for one repository. Pagination remains under configured API origin and uses screened requests. Failure enums retain status/reason rather than provider response bodies or tokens. |
| Canonical source versus ordinary outputs | github/delivery.rs `render`; github/presentation.rs `render_findings`, `place`, `comment`; extraction/output.rs and extraction.rs producer; verification/hosted/blocks.rs `pull_request`; verification/engines.rs `pr_source_extracted`; vala-verify blocks/engine.rs outcome assembly and Attempted::settle; blocks/agent.rs Agent output | Report schema checks canonical path/commit/range shape and rendering checks retained file/range, but delivery decides which value is retained source solely by deserialization, discarding existing server-assigned provenance (SEC-009-2). Suggestions use the exact selected anchor and COMMENT, with no source mutation. |
| Inspection authorization and tenant isolation | verification/service.rs `get_execution`, `records`; service/inspection.rs `retained`, `presented`; verification/inputs.rs delivery queries; mcp/verification.rs; SQL recovery queries | HTTP/MCP retain exact Verifier read authorization and additional cards read for PR work before exposing delivery. Background SYSTEM queries are fixed tenant/execution queries; TenantConn binds tenant; the OperatorPool path enumerates tenants only. No caller-controlled SQL or cross-tenant token source was introduced. |
| Existing proof | pg_github_delivery.rs connected rendering/inspection tests, lost-answer/cooldown tests, new-head and access-removal tests; presentation tests and report validation tests | Assertions cover ordinary invalid anchors and retries after a failed write. They do not exercise access/generation changes during successful lookup or extraction-shaped ordinary output. |

## Acceptance matrix for this domain

| Obligation | Evidence | Result |
|---|---|---|
| Tenant and one-repository token scope, no persisted/returned secret | service.rs installation_access; app.rs installation_token; typed delivery reasons and anchor problems | PASS |
| Recheck current access and generation before every actual provider write | delivery.rs:459 checks before lookup; :147-168 perform lookup followed by mutation | FAIL |
| Match every report anchor against the canonical retained source for the reviewed execution | delivery.rs:392-395 trusts any decodable output; presentation.rs:339 compares against that value | FAIL |
| Inspection uses existing read authorization and safe errors separately from verdict | service.rs get_execution and records; inspection.rs retained; inputs.rs delivery; pg_github_delivery HTTP/MCP equality assertion | PASS |
| COMMENT only, pinned reviewed commit, no automatic source changes or decision promotion | presentation.rs Review creation, comment placement, Settled::conclusion; rendering journey verdict/conclusion assertions | PASS |

## Material findings

### SEC-009-1 — INCORRECT: authorization/generation check precedes a separate remote lookup rather than the mutation

Violated obligation: REQ-014 and TASK-009 require current access and current generation before each write; INV-006 permits already-issued IO, not a later mutation issued after a completed lookup under stale authority.

Location: crates/wyrd/wyrd-server/src/components/github/delivery.rs:459-463 and :147-168.

Evidence and reachable failure: `attempt` obtains its token/checks current generation through `allowed`, then `Write::send` awaits `check_runs` or `review_bodies` (potentially multiple remote pages). `send` subsequently issues POST/PATCH with the earlier token without rechecking. Pause a successful lookup, complete a new-head activation or local tenant disconnect, then return the lookup. The next POST/PATCH is still sent. Local disconnect commits slot release and generation cancellation but does not revoke an already-issued GitHub installation token; a new head likewise leaves GitHub able to accept a Check/review on the old commit. Neither is an already-issued mutation at the time access/generation changed. The current tests stall POST /check-runs and fail that POST, proving the retry boundary only, not this gap.

Observable consequence and materiality: obsolete executions can publish another Check/review after supersession or tenant disconnection, violating the explicit stop-writing boundary. This is a normal provider-latency/concurrent disconnect or synchronize case, not an unusual token profile or hypothetical permission expansion.

Existing owners/siblings: `installation_access` already provides active selected-repository access plus slot revision checking; `allowed` already reads generation under TenantConn. Admission similarly revalidates its installation binding after provider IO. All three mutation branches in `Write::send` share this cause (Check create/update and review create).

Smallest correction: preserve lookup-before-create and retry semantics, but place the existing current-access/current-generation check after the final lookup and immediately before each actual mutation. Reuse the existing installation access/generation owner; do not add locks across remote IO, a new fence service, or a progress ledger. Existing-resource/no-write branches need no new mutation check. This closes the intervening awaited-read gap without promising atomicity for IO already issued.

Focused proof: hold a successful Check lookup, supersede the PR or disconnect the tenant locally, release lookup, and assert no subsequent POST/PATCH and a skipped reason. Exercise the same boundary before review creation; retain lost-answer retry proof. Runtime proof was requested from the review orchestrator; this report is static.

### SEC-009-2 — INCORRECT: arbitrary step JSON can become the trusted source for anchor validation

Violated obligation: TASK-009 exact-source rendering requires validation against TASK-005 retained canonical source; REQ-013 and wyrd-design.md require comments/suggestions to match retained source and the reviewed commit. INV-001/005 preserve stable accepted source identity.

Location: crates/wyrd/wyrd-server/src/components/github/delivery.rs:392-395; consumer crates/wyrd/wyrd-server/src/components/github/presentation.rs:339-361.

Evidence and reachable failure: delivery takes the first step output deserializable as `PullRequestExtraction`, without checking provenance or binding it to activation. Ordinary Code and schema-validated Agent outputs may hold arbitrary objects, including an extraction-shaped object; the engine retains them in plan-stage order. The real extraction producer stamps `CallProvenance::Extract` with `PULL_REQUEST_EXTRACT_VERSION` (hosted/blocks.rs:354-368), and the sibling `engines::pr_source_extracted` already requires successful status plus that exact provenance. Delivery ignores that existing distinction. A PR Verifier can complete with an ordinary Code/Agent step returning an extraction-shaped prior-head snapshot and a standard report anchored to that prior head, even without a genuine extractor. `place` checks the report's commit only against this ordinary object's `pr.head.sha`; the review itself is then pinned to `presented.activation.head.sha`. If the same path/range exists in the new head's diff, GitHub can accept an old-source suggestion at the new commit. A fabricated source/hunk object can similarly satisfy the local range checks. The ordinary JSON value's persistence does not turn it into the TASK-005 canonical extraction.

Observable consequence and materiality: source validation can endorse a report anchored to a different source/commit and publish the suggestion at the reviewed commit. This defeats a specifically required integrity boundary; it does not require treating a tenant author as a cross-tenant attacker. Ordinary intermediate structured outputs must remain content rather than source authority.

Existing owners/siblings: `ExecutionBinding::pull_request` is the canonical extraction producer; `engines::pr_source_extracted` demonstrates the existing server-assigned provenance check. Report parsing and source identity serve different purposes and should remain distinct.

Smallest correction: select canonical retained extraction only from successful step output carrying the exact server-owned pull-request extraction provenance, preserving its binding to the execution's accepted source. Reuse that existing discriminator/predicate; ordinary deserializable JSON must never supply source authority. If no genuine extraction exists, do not publish located findings as validated source. No new selector option, report engine, or persistence is needed.

Focused proof: complete a PR execution with a normal Code output shaped as a prior-head extraction plus a report anchored to that head; ensure it cannot produce an inline review pinned to the current activation. Also place an extraction-shaped ordinary output before a genuine extraction and assert the genuine retained source controls validation. Existing valid-extractor rendering must remain unchanged.

## Verification limits

Static review only; no Cargo, Postgres, SDK or live GitHub commands were run. Existing test assertions were inspected rather than treated as independent runtime proof. No source files or unrelated working-tree changes were edited. The two focused gaps need closure proof; no additional hardening or new infrastructure is requested.
