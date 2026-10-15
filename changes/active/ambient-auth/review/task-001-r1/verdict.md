# TASK-001 review verdict

Verdict: **FIX_REQUIRED**

Findings: **FIND-TASK-001-1**, **FIND-TASK-001-2**

Remediation: [TASK-001-R1-complete-consumers-and-reuse-principals.md](TASK-001-R1-complete-consumers-and-reuse-principals.md), routed to `$wyrd-implement`.

## Immutable subject

- Wyrd repository: `/home/thorrester/Documents/GitHub/wyrd-worktrees/split/ambient-auth`.
- Base: `5bcd9e4872f1376f1722922fa63b67384243be04`.
- Candidate: `824f0c491ea2686977c9da9fce03ac4ef65c01b1`.
- External example: `/home/thorrester/Documents/GitHub/wyrd-example-ts`.
- External base: `bef9fc05ec1725070588bcd49efdc937b02b0131`.
- External candidate: `b283666df93a32cb73aa8c07f881d9271797dff0`.
- Approved specification: `changes/active/ambient-auth/spec.md`, revision 3.
- Original task: `changes/active/ambient-auth/tasks/TASK-001-client-http-auth.md`.

Reviewed both complete cumulative ranges. No implementation, task, spec, generated source, manifest, dependency link, or lockfile was changed during review. Only the review directory was written. Both HEADs remained pinned; source worktrees remained unchanged. The ignored local example SDK was inspected as an environment input, not treated as immutable candidate content.

## Independent review results

| Required role | Report | Discovery result | Reconciliation |
|---|---|---|---|
| behavior-rev | [task-review-behavior.md](task-review-behavior.md) | FAIL, BEH-001 | Confirmed in FIND-TASK-001-1. |
| invariant-rev | [task-review-invariants.md](task-review-invariants.md) | FAIL, INV-001 | Same missing external consumer; consolidated in FIND-TASK-001-1. |
| repo-rev | [standards-review.md](standards-review.md) | FAIL, STD-001 | Rejected as a committed dependency regression. Local typecheck failure remains an honest proof limitation. Changed Rust rustdoc passes. |
| maintainer-rev | [maintainer-review.md](maintainer-review.md) | FAIL, MAINT-001/002 | MAINT-001 revised/consolidated in FIND-TASK-001-1; MAINT-002 confirmed as FIND-TASK-001-2. |
| system-rev | [system-review.md](system-review.md) | PASS, no findings | No new request-to-process failure propagation or renewal owner introduced. |
| domain-rev: security/authentication | [domain-review-security.md](domain-review-security.md) | FAIL, SEC-001 | Same missing external MCP migration; consolidated in FIND-TASK-001-1, no bypass finding. |
| followup-rev | [followup-review.md](followup-review.md) | RESOLVED | Distinguishes tracked application source from ignored local SDK contents. |
| ponytail-rev | [findings-validation.md](findings-validation.md) | FIX_REQUIRED | Independently validated every discovery ID and produced the two-finding final ledger. |

The orchestrator compared claims and assembled this verdict from independent validation; it did not validate its own findings. The original navigation and immutable inputs are in [subject.md](subject.md); comparison and the reason for follow-up are in [claim-comparison.md](claim-comparison.md).

Follow-up was required because discovery reports differed on whether the stale external SDK was a committed regression. Immutable external objects contain no vendor package or gitlink; local package bytes are ignored. Consequently STD-001 is rejected rather than retained as optional advice. Its actual failed compiler check is still a proof limit. The tracked MCP consumer is independently proven and remains a finding.

## Reconciled acceptance matrix

Source-supported passes below are corroborated by the validator's full owner/caller inspection and the task's recorded focused results, with fresh executions explicitly identified. They do not claim that unrun aggregate lanes passed.

| Obligation | Implementation evidence | Verification evidence | Result |
|---|---|---|---|
| D1, INV-003: existing credential/cache/renewal owner | Both language token accessors reach shared `WyrdClient::access_token` and `AuthMiddleware::bearer`. | Complete owner tracing; no second credential state or exchange. | PASS |
| D2, REQ-001: TS client-owned fetch, per-request token and three-header removal | `wyrd/src/index.ts:1569–1575`. | Fresh exact gateway adapter unit: 1 passed; URL/init and Request inputs, placeholders, freshness. | PASS |
| D2, REQ-001: Python dual-httpx sync/async transport, off-loop renewal | Native getter holds exact client; complete `_http_auth.py` flows and `asyncio.to_thread`. | Fresh Python adapter file: 3 passed; source verifies async dispatch off loop. | PASS |
| REQ-002: remove helpers without aliases | Main SDK/docs residual search and runtime/stub diff; gateway administration preserved. | Independent source/export inspection. External stale consumer is evaluated separately below. | PASS for SDK removal |
| REQ-003: credential chain and stable errors | Shared resolver/bearer branches unchanged, exact client bound, errors propagate before send. | Recorded ambient API-key journey plus shared-owner inspection; workload/saved-login journeys not freshly run. | PASS within stated proof limits |
| REQ-004, D6, AC-005: CLI token plus newline usable at gateway | New CLI dispatch and existing ambient client construction. | Recorded focused CLI journey checks stdout and spends token at GET /v1/models; no fresh CLI execution. | PASS within stated proof limits |
| AC-001: TS OpenAI, Anthropic, official MCP, Agents MCP | Real-server gateway and MCP tests use only httpAuth. | Recorded gateway file 12 and MCP file 2 passes; fixtures/assertions inspected. | PASS |
| AC-002: Python stock clients and HTTP MCP | New real-server OpenAI, AsyncOpenAI, Anthropic, HTTP MCP flows use http_auth. | Recorded file 5 passes; HTTPX-family units freshly pass. MCP journey uses supported httpx2. | PASS |
| AC-003: placeholder keys removed | Units assert all three header removals; stock clients keep placeholders. | Fresh adapter units and recorded successful journeys. | PASS |
| AC-004: revoked key refused at exchange | Test constructs a fresh client after revocation, asserts WyrdError cause and no upstream request. | Recorded focused pass; setup duplicates existing owner. | PASS behavior; FIND-TASK-001-2 for DRIFT |
| AC-006, D3: one example client for provider and MCP | OpenAI migrated, but tracked investigator imports/calls wyrdFetch. | Immutable external source proves failure with current SDK; local typecheck also fails on stale ignored dependency. | FAIL — FIND-TASK-001-1 |
| AC-007: gates and owning verification | Recorded format/lint/codegen/type/docs and focused journey evidence. | Full TS/Python integration, CLI and local-development lanes explicitly deferred. | UNPROVEN in full; final aggregates belong to change review |
| AC-008: retain gateway administration | Native Gateway/public administration runtime/types/stubs retained; only auth helper removed. | Existing exports/surface/type fixture and gateway journey results recorded, source independently inspected. | PASS |
| INV-001/002: tenant identity and secrets | Exact bound client, verified server authority, no new production secret logs; intentional CLI stdout only. | Security/system/source reviews; no fresh tenant/log suites claimed. | PASS within stated proof limits |
| Docs and non-goals | Required framework/provider/MCP/key-slot/CLI guide; no API-key fallback, framework-specific helper, server change, or duplicate token cache. | Complete cumulative diff and recorded docs check. | PASS |
| Reuse-first owner constraint | New revoked-key support duplicates principal routes/JSON/status handling already owned by Principals. | Validator traced helper, sole test, typed/native/shared owner and sibling consumers. | FAIL — FIND-TASK-001-2 |

## Validated finding ledger

### FIND-TASK-001-1 — MISSING: finish the external MCP consumer migration

Sources: BEH-001, INV-001, SEC-001, revised MAINT-001. Independently CONFIRMED.

External `scripts/investigate.ts:9,21–27` is tracked, runnable via `npm run investigate`, and still imports/calls removed `wyrdFetch`. It imports the migrated gateway but cannot reuse its unexported application client. With the reviewed SDK that named import fails, violating AC-006. Reuse the client already constructed by `src/agent.ts` for MCP, remove the stale helper reference, preserve tool restrictions/lifecycle, and correct the task's inaccurate no-MCP claim. Supply the built reviewed SDK through the existing ignored local file dependency and record passing example typecheck plus focused MCP execution. Do not redesign packaging or commit an SDK snapshot.

### FIND-TASK-001-2 — DRIFT: delete duplicate principal setup

Source: MAINT-002. Independently CONFIRMED under the skill's deletion/reuse threshold.

`sdks/wyrd-sdk-ts/wyrd/tests/support/server.ts:113–143::revokedApiKey` adds one-use raw principal creation/revocation HTTP, JSON casts and manual status/error handling. The existing typed `Principals` facade and shared Rust owner implement that complete workflow. Delete the helper and use those typed operations in its sole negative-test caller, preserving the exact revocation semantics and all rejection assertions. No production principal owner or transport change is required.

The complete source/caller evidence, rejected-claim disposition, smallest correction and exact commands are in [findings-validation.md](findings-validation.md). These are bounded implementation corrections under approved r3; no spec revision is required. No optional style notes were promoted.

## Verification, prior closure and limits

Fresh validator executions:

- Exact TS adapter unit via mise: 1 passed, 3 unselected.
- Python adapter unit file via mise: 3 passed.
- External example typecheck via mise: failed TS2339 at `src/agent.ts:20` against an ignored stale SDK. This is not proof of a committed stale package.

Both cumulative diffs also passed `git diff --check` during orchestration. Complete commands/results and practical limits are preserved in findings-validation.md. No installs, generation, full suites, live provider execution, new infrastructure or implementation changes were performed.

No prior task-review FIND ledger or remediation round exists. The earlier plan-readiness concern about deleting `wyrd.gateway` was addressed by human-approved r3 and is closed by actual preserved runtime/type/stub source. It is not reopened.

Full aggregate verification remains for change review; the new remediation requires only the focused commands covering its write set. External runtime proof needs the example's configured server/provider. An unavailable environment must be recorded, never converted to a pass. Review does not implement, commit, merge, push, deploy, or complete the active change.
