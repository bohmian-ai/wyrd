# TASK-001 cumulative re-review verdict

Verdict: **PASS** — bounded task completion gate.

Open finding ledger: **empty**. Closed: **FIND-TASK-001-1**, **FIND-TASK-001-2**.

No remediation task is required. This is task acceptance, not whole-change verification or completion.

## Immutable subject and authority

- Wyrd repository: `/home/thorrester/Documents/GitHub/wyrd-worktrees/split/ambient-auth`.
- Original base: `5bcd9e4872f1376f1722922fa63b67384243be04`.
- Reviewed candidate: `0cfbf6422709cc9d3de680788c616f82f29255da`.
- Reported remediation candidate: `276f648f4`; its immediate successor, reviewed here, only corrects a related specification reference.
- Previous reviewed candidate: `824f0c491ea2686977c9da9fce03ac4ef65c01b1`.
- External example: `/home/thorrester/Documents/GitHub/wyrd-example-ts`.
- External original base: `bef9fc05ec1725070588bcd49efdc937b02b0131`.
- External reviewed candidate: `000ed1f2f587a6629a471c51aadff5e05f0c820b`.
- External previous candidate: `b283666df93a32cb73aa8c07f881d9271797dff0`.
- Approved specification: `changes/active/ambient-auth/spec.md`, revision 3.
- Original task: `changes/active/ambient-auth/tasks/TASK-001-client-http-auth.md`.
- Prior verdict/validated ledger: `changes/active/ambient-auth/review/task-001-r1/verdict.md` and `findings-validation.md`.
- Remediation/evidence: `changes/active/ambient-auth/review/task-001-r1/TASK-001-R1-complete-consumers-and-reuse-principals.md`.

Reviewed both complete original-base-to-current ranges. Remediation diffs located fixes without replacing cumulative acceptance. Root AGENTS.md, agent rules, active design/doctrine/security authority, reference router, specification/maintainer/language/testing guidance and approved packet governed the audit. CodeGraph preceded source navigation. Prior reports were hypotheses/history, not substitutes for current source.

Both candidate HEADs remained pinned and reviewed source remained unchanged. Only the new r2 review directory was authored. No implementation, task, specification, package link, build, configuration, credential store or server state was changed by this review.

## Independent review and follow-up

| Required role | Durable report | Result |
|---|---|---|
| behavior-rev | [task-review-behavior.md](task-review-behavior.md) | PASS; cumulative matrix, both findings closed. |
| invariant-rev | [task-review-invariants.md](task-review-invariants.md) | PASS; cumulative value/identity/lifecycle trace, both findings closed. |
| repo-rev | [standards-review.md](standards-review.md) | PASS; complete authority/rule coverage, including changed Rust documentation and language projections. |
| maintainer-rev | [maintainer-review.md](maintainer-review.md) | PASS; changed-owner/caller/declaration coverage and existing capability reuse. |
| system-rev | [system-review.md](system-review.md) | PASS; deployed/request failure and recovery paths, no new sibling availability defect. |
| domain-rev: authentication/security | [domain-review-security.md](domain-review-security.md) | PASS; exact-client identity, server trust/tenant boundary and secret handling. |
| Independent structured ponytail-rev | [findings-validation.md](findings-validation.md) | PASS; independently validated empty ledger and both stable prior IDs closed. |

All discovery reports were complete before validation. No new focused follow-up was required: reports had no material conflict or untraced reachable path. The prior round's ignored-package/source distinction remained explicit and was independently checked again. [claim-comparison.md](claim-comparison.md) records that decision; [subject.md](subject.md) records shared inputs and navigation. The orchestrator assembled this verdict from the independent validation rather than validating its own findings.

## Prior-finding closure and validated ledger

**FIND-TASK-001-1 — CLOSED.** External `src/agent.ts:19–20` exports its existing single WyrdClient for OpenAI; `scripts/investigate.ts:11,21–26` imports that instance and uses its httpAuth/serverUrl for MCP. The removed helper import/call is absent across tracked application consumers. Tracing, gateway Run/Card headers, model configuration, read-only tool filter and MCP connect/close remain intact. Corrected evidence names the actual investigator. Runtime/typecheck evidence is explicitly composition-scoped below.

**FIND-TASK-001-2 — CLOSED.** The one-use raw revokedApiKey helper, manual HTTP/JSON/status logic and unused value import are deleted. The sole negative test calls existing typed Principals create/revoke methods, reaching the established native/shared owner. Empty roles, service-principal revocation/reason, fresh-client ordering, placeholder key, WyrdError cause, unpinned status and unchanged-upstream assertions remain. Recorded focused proof selected 1 passing test and 11 skipped tests; owning TS static checks passed.

**Validated open ledger: `[]`.** No FIND-TASK-001-3 is allocated. The previous rejected stale-vendor committed-regression claim stays rejected. No new specification decision, remediation, compatibility alias, tracing change or packaging redesign is needed.

## Reconciled cumulative acceptance

PASS below is bounded task acceptance based on independently inspected source and attributed available proof. It does not turn unrun final aggregate lanes into passing results.

| Obligation | Implementation evidence | Verification evidence and limits | Task assessment |
|---|---|---|---|
| D1 / INV-003: one token/cache/renewal owner | Both adapters call existing accessors through native projections to shared WyrdClient/AuthMiddleware. | Full owner trace; no second exchange/cache/renewal. | PASS |
| D2 / REQ-001: TS fetch and current token/header normalization | Exact-client closure copies effective Request/init headers, strips three provider carriers, replaces Wyrd header and awaits token before fetch. | Fresh r2 exact adapter unit: 1 passed; recorded stock-client journeys. | PASS |
| D2 / REQ-001: Python dual HTTPX sync/async transport | Native getter holds actual client; async uses asyncio.to_thread; existing accessor releases GIL. | Fresh r2 Python adapter units: 3 passed; recorded provider/HTTP MCP journeys. | PASS |
| REQ-002 / AC-008: helper removal and admin preservation | No aliases/stale application references; Gateway administration runtime/types/stubs retained. | Source/export/generated parity; recorded existing gateway/export/type coverage. | PASS |
| REQ-003: credential chain and stable failures | Exact client reused; shared resolver/bearer variants and error mapping unchanged. | Recorded ambient API-key flow plus source composition for workload/login/delegation; no fresh all-source expiry matrix. | PASS within stated proof |
| REQ-004 / D6 / AC-005: token-only CLI output | Existing CLI ambient assembly and shared token owner; one newline-terminated output after acquisition. | Recorded journey spends the printed token at gateway models; historical command record lacks full mise/setup precision. | PASS within stated proof |
| AC-001: TS stock OpenAI, Anthropic, official MCP, Agents MCP | Real-server callers use httpAuth; provider and tool-result assertions retained. | Recorded gateway 12 and MCP 2 passing cases. | PASS |
| AC-002: Python stock clients and HTTP MCP | Real-server OpenAI, AsyncOpenAI, Anthropic and supported HTTPX2 MCP path use public getter. | Recorded 5 passes; direct HTTPX-family units freshly pass. | PASS |
| AC-003: placeholder requests succeed | All three credential names removed; stock clients retain placeholders; server provider credential separation unchanged. | Explicit adapter assertions and recorded successful journeys. | PASS |
| AC-004: revoked key refused before gateway | Typed existing principal lifecycle followed by fresh revoked client; acquisition precedes dispatch. | Recorded exact selector: 1 passed, 11 skipped; refusal cause and untouched upstream asserted. | PASS |
| AC-006: example one client for OpenAI and MCP | Committed shared instance and actual runnable investigator migrated completely. | Recorded composed-SDK runtime success and fresh r2 composed-SDK typecheck; candidate-only build excluded. | PASS for changed wiring |
| AC-007: static checks and owning proof | Relevant manifests/lock, test registrations and lane definitions inspected; static/focused results recorded. | Bounded task proof credible. Full TS/Python integration, CLI and local-development lanes remain unrun. | Task gate supported; full AC-007 UNPROVEN until change review |
| INV-001/002: tenant identity and secrets | Exact bound bearer; unchanged server authentication/permission/audit; no new tenant selector or secret logging. Intentional CLI stdout is specified behavior. | Source/security/system trace; no fresh tenant/log qualification. | PASS |
| Docs, reuse and non-goals | Required provider/framework/MCP/key-slot/CLI guide; generated text matches existing schema source; typed Principals reuse; dev-only required Anthropic dependency. | Cumulative source/lock/export inspection and recorded docs/codegen checks. No fallback, per-framework adapter, server or token-owner change. | PASS |

## Proof provenance and remaining limits

Fresh r2 review results include the exact TS adapter unit (1 passed, 3 unselected), Python adapter units (3 passed), successful external example typecheck against the current composed package, and clean cumulative git diff checks. Complete commands and attribution appear in the independent reports. Other recorded focused journeys/static results were inspected rather than falsely described as fresh executions. No Cargo-backed jobs, builds, installations, generation, full suites, live calls or another session's credentials were used by this re-review.

The ignored example dependency resolves to a scratch composed SDK with HEAD `531eb168940da76f494beba2a49857e1ce7561e2` and uncommitted trace-processor changes from the separate branch. Its emitted auth getter/declaration matches the reviewed implementation; the example's trace-processor import already existed at its original base. The example's successful typecheck/investigator run therefore supports the changed transport in that composition. It does not establish that this candidate alone exports the separate trace processor, a clean integrated SDK commit, or a reproducible standalone example/release.

Read-only inspection also finds the scratch uv.lock index still marked unresolved despite remediation prose saying resolved. This is an incidental provenance discrepancy outside either immutable subject, not a newly established transport defect or authorization to modify/clean the scratch worktree. The reported live run used a saved administrator login and real OpenAI calls; it is not proof of restricted-role, workload, expiry or outage behavior. No such broader claim is made.

The final whole-change owning lanes, including full TS/Python integration, CLI journeys and installed-CLI local-development proof, remain for change review. This task PASS does not certify full AC-007 completion or automatically invoke change completion. No merge, push, deploy, cleanup or implementation action was performed.
