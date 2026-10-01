# TASK-006 R7 verdict

## Verdict

**FIX_REQUIRED.** `FIND-TASK-006-13`, `FIND-TASK-006-33`, and `FIND-TASK-006-34` remain. Their corrections fit approved specification revision 44; no spec revision is required. Proposed `FIND-TASK-006-32` was rejected on independent reassessment.

## Immutable subject

- Repository: `/home/thorrester/Documents/GitHub/wyrd-vcc-t006`, branch `vcc/task-006`.
- Original cumulative base: `f8811ac5035c3aa165d34c38992f9889b3c9081f`.
- Candidate: `f3c65147e0fd828fe5c657d2871ff80f9b3d5543`.
- Authority: [approved specification revision 44](../../spec.md), [original TASK-006](../../tasks/TASK-006-continuous-eval-verifier.md), R1–R6 review and remediation, `AGENTS.md`, agent rules, and applicable architecture.
- `.codegraph/` is absent. Candidate HEAD remained unchanged during both review waves. Reviewers inspected source and recorded evidence; they did not rerun infrastructure lanes.

## Acceptance matrix

| Obligation | Implementation evidence | Verification evidence | Result |
| --- | --- | --- | --- |
| Continuous Eval, canonical outcomes, post-ACK work, bounded media, and excluded extra engines/queues | Cumulative server, Vala, SQL and journey source | Prior server, Oracle, SQL and SDK journeys; R7 Eval review | PASS within recorded limits |
| External Postgres, owner-only migration, separate serving roles, RLS readiness | Cumulative SQL/server source; R5 `SqlStore` removal | Recorded SQL, roles, migration and startup checks; R7 data review | PASS within recorded limits |
| Peer mTLS, discovery, receiver checks, ready membership, and remote Oracle/Scribe work | Cumulative peer/server source | Prior peer 9/9 and kind evidence; R7 peer review | PASS within recorded limits |
| Official-image startup and single Oracle autoscaling journey from final source (AC-034/037) | Existing scripts build and pin official image | Recorded images came from `abf963ccf`; later `f3c65147e` changes compiled storage code | **FAIL — FIND-13** |
| Production policies before workloads and edge last (R6 FIND-27) | Production guide applies existing network and mesh policies first | Fresh-namespace startup denial and edge walkthrough recorded | PASS |
| Database passwords absent from `psql`/`kubectl` arguments (R6 FIND-31) | Guides use `PGPASSWORD` and `--from-file`; `roles.sql` uses `\getenv` | Recorded argv capture, three TLS logins, migration and serving walk | PASS |
| Storage readiness correction without hidden write retries | Per-operator OpenDAL client; custom endpoint uses 2-second idle bound; writes remain single attempt | Rustfs before/after and focused test recorded; managed cloud 15/60-second bounds remain an unmeasured limit | PASS within recorded limits |
| Fast tests remain IO-free; touched Rust obeys import, type and documentation rules | New storage test binds a socket in ordinary `--lib` tests and has local imports, qualified signature type and missing panic docs | Source inspection; fmt/lints do not enforce these rules | **FAIL — FIND-33/34** |
| Official image build context excludes local SDK build products | `.dockerignore` | Recorded context reduction and startup image build | PASS |

## Independent review

| Role | Result | Material proposals |
| --- | --- | --- |
| Task implementation | FAIL | Final-source image proof; proposed managed-cloud timeout drift rejected in validation |
| Repository standards | FAIL | Image proof; socket test tier; Rust source rules |
| Deployment and client domains | FAIL | Same final-source image proof |
| Security, data, peer, Eval and storage domains | PASS | None; storage recorded cloud timeout uncertainty as a limit |
| Structured Ponytail validation | COMPLETE | Three retained findings; cloud timeout proposal rejected on reassessment |

The [validated ledger](findings-validation.md) records each source proposal, reachability, decision, exact location, consequence, correction and focused proof. R6 `FIND-TASK-006-27` and `FIND-TASK-006-31` are closed. Proposed `FIND-TASK-006-32` is withdrawn: forcing every cloud pool to two seconds would add unmeasured reconnection latency without an observed cloud failure. Prior findings other than reopened `FIND-TASK-006-13` remain closed on the inspected paths.

## Verification limits and disposition

R6 records passing startup and kind lanes on images built from `abf963ccf`, plus storage, Postgres, docs, fmt and lint checks. Those images predate the final compiled storage change. Cloud provider idle bounds and their performance tradeoffs remain unmeasured; no reviewer ran a real cloud bucket. `git diff --check` on the cumulative range passed. The peer lane was not repeated because no peer code or admission changed.

Remediation: [TASK-006-R7-final-image-and-storage-closure.md](TASK-006-R7-final-image-and-storage-closure.md).
