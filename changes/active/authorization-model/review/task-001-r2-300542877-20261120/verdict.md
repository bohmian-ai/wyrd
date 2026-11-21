# TASK-001: PASS after R2

The cumulative backend task satisfies its assigned approved requirements. The shared authority lifecycle correction preserves cold-tenant observations, fences in-flight loads and confirmations, and removes the superseded invalidation logic. Guidance, CLI proof and checked-in journey inputs are corrected. No validated blocking finding remains.

## Immutable subject and authority

| Identity | Value |
| --- | --- |
| Repository | `/home/thorrester/Documents/GitHub/wyrd` |
| Candidate and reviewed HEAD | `300542877cd2a58cbe5f061d5075616e4cc4397b` |
| R2 base / prior candidate | `284f801159a391e92493c3072878fd644c9d173b` |
| Original reviewed candidate | `c78ad7b5241fbbfb54e85e2390c6a694787debfc` |
| Cumulative base | `eeb3bbd4f891ae88b00ce1979810eb2a11d04871` |
| Latest inspected range | `git diff 284f80115 300542877`, four commits |
| Approved contract | `SPEC-authorization-model`, revision 2 |
| Original task | `tasks/TASK-001-scoped-authorization.md` |
| Remediation | Prior review's `TASK-001-R2-close-observation-and-consumer-gaps.md` |

The candidate descends from the prior candidate; no rebase correspondence is needed. Existing dirty packet edits and untracked historical review evidence were preserved. No implementation changes or commits were made during review. [subject.md](subject.md) records navigation and review constraints.

Governing authority: root and affected nested AGENTS.md, `changes/spec-driven-development.md`, `agent_docs/maintainer-style.md`, TESTING.md, security posture principal lifecycle/authorization/current freshness/retained-parent/tenant rules, and applicable runtime identity/Card/Bifrost public-query architecture. The approved specification and task determine scope; implementation and prior findings create no additional product authority.

## Independent review assignment

One fresh remediation reviewer, `r2_closure_review`, independently checked all five R2 dispositions, the latest diff's affected callers and consumers, and continued validity of carried-forward acceptance evidence. It traced the shared concurrency/security state and production consumer paths and ran the focused Cargo checks. Its report is [findings-validation.md](findings-validation.md).

This follows the skill's single fresh remediation-reviewer default. Separate discovery/domain/system/repository/maintainer passes were not selected: no new material boundary or unresolved finding required one; unchanged areas retain the prior independent coverage. The root orchestrated, ran the omitted non-Cargo documentation checks and assembled the verdict; it did not validate its own source findings. No new blocker requires independent focused validation.

## Reconciled acceptance matrix

| Existing contract / scenario | Implementation and strongest credible proof | Result |
| --- | --- | --- |
| REQ-001 / AC-001–003 | Common scoped compilation/containment, target validation and group/custom/exact evidence carried forward; checked-in second-release inputs preserve assertions | PASS |
| REQ-002 / AC-001 | Existing local password/device anchors and invitation atomicity proof unchanged; affected local onboarding guidance corrected | PASS |
| REQ-003 / AC-002 | Workload family restriction, two-release exact root/principal/actor attribution paths and their prior proof unchanged | PASS, carried forward |
| REQ-004 / AC-003 | Shared observation/load/install/confirmation lifecycle corrected; fresh cold/warm/malformed/settled units and real-Postgres outage components pass; existing request/stream/replica consumers unchanged | PASS |
| REQ-005 / AC-002–003 | Identity-only claims, restrictive ceilings, delegation and accepted-work recovery unchanged; settled credential behavior regression checked | PASS, carried forward |
| REQ-006 / AC-001 | Prior retained/direct/publication/download/SQL/actual-peer proof remains; new scoped language approval refusal/grant/replay assertions with recorded passes; old FIND-11 claim rejected | PASS |
| REQ-007 / AC-001 | GitHub configuration isolation, provider identity and disconnected retained-read source/proof unaffected | PASS, carried forward |
| REQ-008 backend / AC-001–003 | Prior provisioning/target validation/MCP/access summary proof plus new actual CLI direct/group administration journey | PASS |
| REQ-010 / AC-001–003 | Existing owners preserved; no new schema/contracts; changed consumers/fixtures/docs now align | PASS |
| Mandatory changed-item rustdoc | Prior concrete omissions corrected; new cache generation/observation/validity items document intent and invariants | PASS |
| Mandatory public fixture rule | Four affected journeys use authored checked-in second releases/parents; production assertions preserved | PASS |

REQ-009 and new Access UI presentation remain TASK-002 scope. This is TASK-001 acceptance, not final integrated change acceptance.

## Finding closure and corrected review record

The stable dispositions and source/assertion evidence are recorded in [findings-validation.md](findings-validation.md). Earlier closed findings remain closed. FIND-2, FIND-8, FIND-12 and FIND-13 are now closed.

**FIND-TASK-001-11 is rejected as originally diagnosed.** The prior review incorrectly claimed `approver` at `All` lacked parent read. The existing binding compiler derives `executions:read` from `cards:read` at both Card and All scopes; the old tenant-wide success expectation was valid. This behavior existed in the prior candidate. The revised Python/TypeScript tests correctly exercise the narrower approved boundary: an approver restricted to the shared Verifier is refused until separately granted Repository visibility. They strengthen proof without changing production semantics. Historical reports remain preserved; this verdict corrects their diagnosis and does not count the prior assertion as a repaired production or fixture regression.

The cache correction is at the shared owner: observations exist before a snapshot; invalidation generation is captured before loads/confirmations; a single usability rule governs current revision, generation and freshness. The old present-entry timestamp-clearing loop and optional confirmation field were removed. No handler-specific workaround or alternate authorization mechanism was added.

## Verification and waits

Six focused authority tests passed freshly, including PostgreSQL-backed cases. Exact selectors, results and proof limits are recorded in the validation report. Changed CLI, language and fixture journeys were inspected against their supplied execution evidence; those journeys were not all rerun during review.

The orchestrator additionally ran `mise run docs:build` successfully (static site and Pagefind), followed by `mise run docs:a11y`: 37 pages checked, AA contrast verified in light and dark modes. This closes the two explicitly omitted documentation checks. Build output contained a Node deprecation warning and plugin timing notices, with exit 0.

The latest correction adds or moves no hot-path PostgreSQL, Scribe, object-store or Vault waits. Cache observation/generation/usability checks remain in process memory; reload retains the sanctioned permission-acquisition wait and confirmation remains background. No lock spans IO. Earlier administration/Card-mutation target lookups remain outside the defined hot paths; retained identity replaces an existing lookup; audit remains nonblocking outbox staging. Prior wait accounting carries forward.

The cache retains small observation-only entries for notifying tenants, including those never loaded on this replica. That documented memory-retention limitation does not change authorization semantics or require a new cache subsystem for this task.

Behavioral verification: **PASS**. TDD compliance reporting and benchmark qualification remain excluded by explicit maintainer instructions. No required process history is manufactured or inferred from later tests. `git diff --check` and final candidate immutability checks are recorded at handoff.

R2 is the second remediation review and passes. The unsuccessful-round convergence condition does not apply. Remaining prior issues were resolved within settled behavior; the false-positive approval diagnosis is explicitly corrected. No further remediation task is needed.

Skipped broad integrated suites and deployment checks; final whole-change acceptance and TASK-002 UI remain outstanding. No implementation, merge, push, deployment or next review is launched by this PASS.
