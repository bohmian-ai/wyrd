# TASK-006 R4 verdict

## Verdict

**FIX_REQUIRED.** Six bounded findings remain: reopened `FIND-TASK-006-19` and `FIND-TASK-006-20`, plus new `FIND-TASK-006-23` through `FIND-TASK-006-26`. The other R3 findings close. The user's correction to Oracle autoscaling is recorded in approved specification revision 42.

## Immutable subject

- Repository: `/home/thorrester/Documents/GitHub/wyrd-vcc-t006`.
- Original cumulative base: `f8811ac5035c3aa165d34c38992f9889b3c9081f`.
- R3 remediation base: `2fbe90cd880936d8e4f25173839e4cc4fa18f0b4`.
- Candidate: `58cabb529b93da366959db796ac3b596a6c6c1e6`.
- Approved specification: `changes/active/verified-change-contract/spec.md`, revision 42 (AC-037 corrected by the user after review).
- Original task: `changes/active/verified-change-contract/tasks/TASK-006-continuous-eval-verifier.md`.
- Prior reviews: `changes/active/verified-change-contract/review/TASK-006-r1/`, `TASK-006-r2/`, and `TASK-006-r3/`.
- Remediation reviewed: `changes/active/verified-change-contract/review/TASK-006-r3/TASK-006-R3-startup-trust-closure.md`.

The implementation candidate remained at the stated commit during both waves. The post-review AC-037 correction changed only the specification and R4 review artifacts.

## Acceptance matrix

| Obligation | Implementation evidence | Verification evidence | Result |
| --- | --- | --- | --- |
| Original continuous Eval model, activation, media, durable results and prior findings 1–12 | Cumulative Eval owners; R3 did not materially change them | Recorded server 22/22 and Oracle 28/28; Eval domain audit | PASS |
| REQ-153/155, AC-034/038 and R3 FIND-13: official image startup, production profile, provenance | Official Docker recipe; startup/kind scripts record immutable local image IDs and source commits | Recorded startup and kind passes; validator checked image-input ancestry | PASS for image pinning |
| AC-037: original kind HPA scales Oracle on successful Oracle-executed reads/sec | Existing kind HPA scales Scribe on HTTP ingress while anchor Oracle executes reads | Recorded kind pass proves Scribe scale only | **FAIL: `FIND-TASK-006-26`** |
| REQ-160/161/162, AC-036 and R3 FIND-14/15/16/22: peer trust, audit, discovery and readiness | Updated authorities, TLS admission, peer authority and listener lifecycle | Peer 9/9, focused TLS/audit/join checks; security and peer reviews | PASS |
| REQ-156/157/158, AC-035 and R3 FIND-17/18/19/20: migration lease, exact roles and effective SQL security | One bounded migration lease, role checks and policy/grant readiness | SQL/startup lanes; data and standards reviews | **FAIL: `FIND-TASK-006-19`, `-20`** |
| Repository Rust import and signature rules; R3 FIND-21 | Documented setup writer; peer transport and activation source | Format/lints pass but do not enforce these source rules | **FAIL: `FIND-TASK-006-23`, `-24`** |
| Complete local development and development Kubernetes startup documentation | `local-development.svx`, `kubernetes-development.svx`, Docker/overview pages | Manual local walkthrough and docs check; deployment review | **FAIL: `FIND-TASK-006-25`** |
| Complete production Kubernetes startup, credentials, peer TLS and Oracle read autoscaling documentation | `kubernetes-production.svx`, Oracle target and telemetry, kind reference manifests | Docs check and source comparison; deployment review | **FAIL: `FIND-TASK-006-26`** |
| Task non-goals and prior closures outside the six retained gaps | Complete cumulative diff and R3 increment | Wave 1 domain and Wave 2 validation | PASS |

## Review results

| Reviewer | Result | Material proposal |
| --- | --- | --- |
| Task implementation | FAIL | Image proof and production guide |
| Repository standards | FAIL | Raw pool API, function import, qualified signatures |
| Security domain | PASS | None |
| Data domain | FAIL | Wrong-column RLS policy passes readiness |
| Peer/concurrency domain | PASS | None |
| Deployment/docs domain | FAIL | Local kind image path and production metrics path |
| Eval domain | PASS | None |
| Structured Ponytail validation | COMPLETE | Six retained; image rerun proposal rejected; production guide proposals merged |

The authoritative [validated ledger](findings-validation.md) gives each finding's Wave 1 source IDs, exact location, reachable consequence, selected correction and focused proof. `FIND-TASK-006-19` and `-20` preserve their existing IDs. The new findings use `-23` through `-26`. No rejected proposal is packaged as optional work.

After the user's correction, `FIND-TASK-006-26` requires the **original and only** local kind journey, plus the production example, to autoscale Oracle on successful Oracle-executed reads per second. The prior Scribe scale pass does not close AC-037.

## Prior-finding closure and verification limits

Findings 1–18 and 21–22 remain closed. Finding 19 reopens because the policy check accepts any lower-case column instead of `data_tenant_id`; finding 20 reopens because `MigrationLease::acquire` added a new public `&PgPool` library signature. The recorded startup and kind images were built before the evidence commit, but no image input changed after each relevant source commit. A run from the evidence commit would create a new evidence commit without changing the tested image. The review accepts the recorded image-ID pinning mechanism under revision 42; the revised Oracle kind journey still needs its own run and image record.

This was a static audit of source, complete diff, prior artifacts and recorded verification. Reviewers did not rerun Docker, kind, Postgres or broad journeys. The passing lanes do not exercise a wrong-column policy mutation, the incomplete Kubernetes guide branches, or the revised Oracle scaling criterion. `git diff --check 2fbe90cd8..58cabb529` passed.

Remediation: [TASK-006-R4-sql-and-deployment-closure.md](TASK-006-R4-sql-and-deployment-closure.md).
