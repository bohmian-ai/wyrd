# TASK-006 R5 verdict

## Verdict

**FIX_REQUIRED.** Five bounded findings remain: reopened `FIND-TASK-006-20` and new `FIND-TASK-006-27` through `-30`. Approved specification revision 43 permits the required production transport correction without another spec decision.

## Immutable subject

- Repository: `/home/thorrester/Documents/GitHub/wyrd-vcc-t006`.
- Original cumulative base: `f8811ac5035c3aa165d34c38992f9889b3c9081f`.
- Candidate: `6ce9f9bb7e2dea288a1b78081346551d896cb7a8`.
- Authority: `changes/active/verified-change-contract/spec.md`, approved revision 43; original `changes/active/verified-change-contract/tasks/TASK-006-continuous-eval-verifier.md`; prior R1–R4 verdicts, ledgers, and remediation tasks; `AGENTS.md` and applicable architecture.
- Reviewed range: the complete base-to-candidate diff, including the R4 changes. Candidate HEAD remained unchanged through both review waves. `.codegraph/` is absent.

## Acceptance matrix

| Obligation | Implementation evidence | Verification evidence | Result |
| --- | --- | --- | --- |
| Original continuous Eval execution, post-ACK enqueue, media, canonical evidence, and non-goals | Cumulative Vala Eval/server implementation | Recorded server 22/22 and Oracle 28/28; Eval domain review | PASS within recorded limits |
| Official image and local immutable image-ID proof, startup, public gRPC, restart, policy-hook removal (AC-034/038) | Official Docker image, startup journey, deleted hook and R4 public gRPC certificate check | Recorded startup and auth/client journeys | PASS |
| External Postgres owner migration, serving roles, RLS and readiness (AC-035) | `SqlStore::migration_lease`, role bootstrap, exact policy expression check | Recorded focused migration/policy tests and `test:sql` | PASS for runtime behavior; **FAIL** redundant handle and raw-pool API (`FIND-TASK-006-20`) |
| Peer discovery, mTLS, receiver validation and readiness (AC-036) | Private peer listener, authority, membership and tail paths | Recorded peer 9/9 and kind proof; peer/security domain review | PASS |
| One kind journey scales Oracle on successful Oracle-executed reads and dispatches a remote read (AC-037) | Oracle Deployment/HPA and load Job | Recorded Oracle 1→2 scale and peer dispatch | **FAIL** load concurrency bound (`FIND-TASK-006-30`) |
| Three startup guides with complete development and production credential and transport setup | Self-hosting guides and R4 walkthrough | Recorded guide walk and `docs:check` | **FAIL** production ingress, Postgres TLS, and development credential examples (`FIND-TASK-006-27`–`-29`) |
| R4 source-style corrections and remaining explicit non-goals | Top-level imports, bare error type, cumulative diff | Source review; recorded fmt/lints | PASS |

## Independent review results

| Reviewer | Result | Material proposals |
| --- | --- | --- |
| Task implementation | FAIL | Production gateway transport |
| Repository standards | FAIL | Raw pool conversion; production gateway transport |
| Security domain | FAIL | Gateway transport; Postgres TLS examples; credential-bearing exec command |
| Deployment domain | FAIL | Gateway transport; ineffective kind concurrency cap |
| Data domain | PASS | None |
| Peer/concurrency domain | PASS | None |
| Eval domain | PASS | None |
| Structured Ponytail validation | COMPLETE | Five retained findings; four gateway proposals merged |

The [validated ledger](findings-validation.md) contains the source IDs, classifications, reachable paths, exact locations, smallest corrections and focused proofs. It reopens `FIND-TASK-006-20`; prior findings `1–19` and `21–26` remain closed on inspected paths. No finding requires changing edge-terminated public TLS, peer mTLS, or another approved product decision.

After the review, the user directed deletion of `SqlStore` and use of `OperatorPool` with the one-off database-owner credential for migration. The [R5 task](TASK-006-R5-production-startup-closure.md) records that correction. It supersedes the narrower FIND-20 remedy in the original Wave 2 report; it does not change the finding or the other four corrections.

## Verification limits

This was a static cumulative audit. Reviewers did not rerun Docker, kind, Postgres or full SDK journeys. R4 records passing `test:sql`, `test:server:startup`, `test:server:kind`, `test:server:peer`, focused regressions, `docs:check`, fmt, lints and diff checks. Those runs establish the reported boot, Oracle HPA and peer behavior, but do not prove encrypted ingress-to-pod transport, certificate verification for the three external Postgres logins, or the kind load cap. The production guide walk used a local kind cluster with documented substitutions.

Remediation: [TASK-006-R5-production-startup-closure.md](TASK-006-R5-production-startup-closure.md).
