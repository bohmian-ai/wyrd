# TASK-006 R6 verdict

## Verdict

**FIX_REQUIRED.** Reopened `FIND-TASK-006-13` and `FIND-TASK-006-27`, plus new `FIND-TASK-006-31`, remain. All three corrections fit approved specification revision 44; no new spec decision is needed.

## Immutable subject

- Repository: `/home/thorrester/Documents/GitHub/wyrd-vcc-t006`.
- Original cumulative base: `f8811ac5035c3aa165d34c38992f9889b3c9081f`.
- Candidate: `3f93886489a1d95be1a3eb2382fe059bb9856988` (`vcc/task-006`); final code parent `895477da1ced790ded96e21a83e2cf31c3185ca1`.
- Authority: [approved specification revision 44](../../spec.md), [original TASK-006](../../tasks/TASK-006-continuous-eval-verifier.md), R1–R5 review packets and remediation, `AGENTS.md`, agent rules and applicable architecture.
- `.codegraph/` is absent. Candidate HEAD remained unchanged through both review waves. An unrelated untracked storage example is outside this immutable subject.

## Acceptance matrix

| Obligation | Implementation evidence | Verification evidence | Result |
| --- | --- | --- | --- |
| Continuous Eval, media, canonical outcomes, post-ACK work, durable results and non-goals | Cumulative Eval/server/Vala source and prior task reviews | Recorded server, Oracle, SQL and SDK journeys; Eval domain review | PASS within recorded limits |
| External Postgres roles, one-off owner migration, RLS readiness and lease; R5 FIND-20 | `SqlStore` removed; owner-backed `OperatorPool` only in migrate; existing lease retained | Focused migration test, `test:sql`, startup and pool allowlist recorded passing; data review | PASS |
| Peer mTLS, discovery, receiver checks and ready membership | Cumulative private peer paths; no R5 peer change | Prior peer 9/9 and kind run; peer review | PASS within recorded limits |
| R5 FIND-28/29/30: verified external database TLS, CLI second-tenant setup and bounded Oracle load | Guides, catalog TLS, kind load Job | R5 wrong-CA/host walk, dev guide walk, held-read cap and kind run | PASS on those behaviors |
| Official image startup and one Oracle autoscaling journey (AC-034/037; FIND-13) | Existing image scripts pin and check image ID | Recorded image predates a later compiled server change | **FAIL: FIND-13** |
| Protected production edge-to-pod path from first rollout (FIND-27) | Existing NetworkPolicy and strict mesh policy in guide | Walk tests only final state; guide starts pods before applying policies | **FAIL: FIND-27** |
| Startup guides include safe database credential setup | Dev/production guide commands and `roles.sql` | Source shows password-bearing `psql` and `kubectl` arguments | **FAIL: FIND-31** |
| R5 client transport, routing, `dist` profile and approved adjacent deletions | Shared client TLS roots, HTTPRoutes, release profile; obsolete manifests/config removed | Focused client/catalog checks, docs, codegen, format/lints and relevant review reports | PASS within recorded limits |

## Independent review

| Role | Result | Material proposals |
| --- | --- | --- |
| Task implementation | FAIL | Final-source image proof |
| Repository standards | FAIL | Policy application order; database credential arguments |
| Security domain | FAIL | Same two security boundaries |
| Deployment domain | FAIL | Policy application order |
| Data, peer, Eval and client domains | PASS | None |
| Structured Ponytail validation | COMPLETE | Three retained, deduplicated findings |

The [validated ledger](findings-validation.md) gives source IDs, reachability, exact locations, consequence, selected correction and focused proof. Prior findings `1–12`, `14–26`, and `28–30` remain closed on the inspected paths; `13` and `27` reopen for distinct final-source and first-rollout gaps.

## Verification limits and disposition

This was a static cumulative audit. Reviewers did not rerun Docker, kind, Postgres or SDK journeys. R5 records passing `test:sql`, `test:server:startup`, `test:server:kind`, focused transport/catalog tests, `docs:check`, codegen, formatting and lints; `test:server:peer` was not rerun because R5 did not change peer behavior. Those results do not prove a final-source image, protection during initial production rollout, or password-free operator command arguments. `git diff --check` for the cumulative range passed.

Remediation: [TASK-006-R6-startup-proof-and-credentials.md](TASK-006-R6-startup-proof-and-credentials.md).
