# TASK-006 R4 task implementation review

## Immutable subject

- Repository: `/home/thorrester/Documents/GitHub/wyrd-vcc-t006`.
- Cumulative base: `f8811ac5035c3aa165d34c38992f9889b3c9081f`.
- Prior R3 candidate: `2fbe90cd880936d8e4f25173839e4cc4fa18f0b4`.
- Candidate: `58cabb529b93da366959db796ac3b596a6c6c1e6`.
- Authority: approved `changes/active/verified-change-contract/spec.md` revision 41; original `tasks/TASK-006-continuous-eval-verifier.md`; R1, R2, and R3 verdicts and ledgers; ready R3 remediation task; `AGENTS.md`, `architecture/agent-rules.md`, `architecture/references/languages/spec-driven-development.md`, and applicable Wyrd, Bifrost, security, deployment, and testing authorities.
- Diff reviewed: cumulative base-to-candidate change and R3 candidate-to-current remediation. The recorded test results were inspected, not rerun in this static review.

## Acceptance matrix

| Obligation | Implementation evidence | Verification evidence | Result |
| --- | --- | --- | --- |
| Original Eval engine, post-ACK enqueue, sampling, trace, terminal results, provider media, authorization, and explicit non-goals | Cumulative Eval/server/SQL owners remain as examined in R1–R3; R4 remediation does not edit those paths | R2/R3 recorded server 22/22, Oracle 28/28, SQL, Python 38, TypeScript 18, and negative media journeys; current record says server 22/22 and Oracle 28/28 | PASS within recorded limits |
| Prior findings `FIND-TASK-006-1`–`-12` stay closed | No R4 change to Eval behavior; R3 removed the raw-pool fixture forwarder and added real-server negative media records | R2/R3 ledgers and recorded journeys | PASS |
| `FIND-TASK-006-13`, AC-034/037: official image built from the **reviewed commit**, immutable local image ID recorded, same bytes run in both image journeys | `scripts/server/test-startup.sh:103-115` and `scripts/server/test-kind-autoscale.sh:105-136` build and pin IDs; the R3 task evidence records startup source `290224775` and kind source `300b4bd98`, neither the candidate `58cabb529` | Recorded startup/kind passes exercise those earlier source commits; no pass from this immutable candidate is recorded | **FAIL — TASKREV-R4-01** |
| `FIND-TASK-006-14`: active peer trust authorities describe dedicated-CA mTLS and receiver-owned checks | `architecture/wyrd-security-posture.md`, `architecture/operations/{deployment-and-release,runbooks}.md`, `architecture/bifrost-design.md` updated | Source inspection; `docs:check` recorded pass | PASS |
| `FIND-TASK-006-15`: wrong same-CA peer client identity refused before body poll | `wyrd-server/src/grpc/mod.rs:161-184` checks first peer leaf via `wyrd_tls::certificate_has_dns_name` | Recorded peer-listener negative test and 9/9 peer lane | PASS |
| `FIND-TASK-006-16`: unsigned pre-binding peer claims cannot select a tenant audit chain | `wyrd-server/src/oracle/peer_authority.rs:117-185,200-250,376-495` uses unverified system-chain append before binding | Five focused authority tests and existing durable-audit evidence recorded; absence of foreign row is not directly tested end-to-end | PASS within recorded proof limit |
| `FIND-TASK-006-17`: one bounded owner-session migration lease spans Wyrd, Vala, validation | `wyrd-sql/src/lib.rs:48-214`, `wyrd-server/src/main.rs:185-223` | Focused competing-migrator test and SQL lane recorded | PASS |
| `FIND-TASK-006-18`: exact app/platform serving logins | `wyrd-sql/src/postgres.rs:97-131`, operator login and role checks in `schema_check.rs` | Wrong-login fixture and SQL lane recorded | PASS |
| `FIND-TASK-006-19`: migration and serve refuse missing policy or excessive grants | `wyrd-sql/src/schema_check.rs` policy and privilege checks shared by migration/serving; `scripts/server/test-startup.sh:135-148` mutates both | Startup and SQL lanes recorded | PASS |
| `FIND-TASK-006-20`: no new reusable raw-pool SQL handle | `SchemaCheck` now uses `OperatorPool`; owner session remains in `MigrationLease` | `check:from-pools-allowlist` and SQL lane recorded | PASS |
| `FIND-TASK-006-21`: setup writer documents partial writes/errors | `wyrd-server/src/main.rs` `Tee<W>::write` and `flush` rustdoc | Source inspection; format/lints recorded | PASS |
| `FIND-TASK-006-22`: peer roles become ready after listener starts | `app/server.rs:723-754`, `state.rs:1690-1715`, boot reservations remain unready | Held-listener RED/GREEN join test and peer lane recorded | PASS |
| Local development guide: required inputs, external durable Postgres/storage, owner migrate, setup, client path, restart/recovery | `docs/.../self-hosting/local-development.svx` | Recorded manual walkthrough and `docs:check` | PASS |
| Development Kubernetes guide: owner-only Job, serving Secrets, volume, routes, setup, restart, optional peer inputs | `docs/.../self-hosting/kubernetes-development.svx` | Source/manifests comparison and `docs:check`; no real-cluster deployment recorded | PASS within documented proof limit |
| Production Kubernetes guide: complete startup with fixed anchor, autoscaled Scribe, peer certificates, credentialing, recovery | `docs/.../self-hosting/kubernetes-production.svx:146-243` defines only the anchor StatefulSet, says Scribe is “the same StatefulSet” with edits, and immediately waits for an unapplied `wyrd-scribe` | `docs:check` proves links/rendering; local kind manifests prove a different, complete example; no walkthrough of this guide on a real cluster | **FAIL — TASKREV-R4-02** |
| Prohibited additions: no tickets, peer API keys, policy hook, embedded Postgres, second discovery registry, generic rollback or production use of kind threshold | Deletions and current config/docs checked against cumulative diff | R3 recorded startup/peer/kind and source inspection | PASS |

## Proposed material findings

### TASKREV-R4-01 — INCORRECT: image evidence is not from the immutable candidate

**Violated obligation:** Approved revision 41's pre-release AC-034/037 condition and R3 `FIND-TASK-006-13` correction require the official image built from the reviewed commit, pinned by immutable local image ID, for both journeys.

**Location and evidence:** The current scripts explicitly label a build with `git rev-parse HEAD` (`scripts/server/test-startup.sh:103-115`; `scripts/server/test-kind-autoscale.sh:105-136`). The committed R3 task evidence at `TASK-006-R3-startup-trust-closure.md:93-94,129-131` identifies the successful startup build as `29022477593d049f2ab55f40d163b283d832a3cb` and the kind build as `300b4bd985134b260fcd26efcee71ce437a9cfe8`. The reviewed candidate is `58cabb529b93da366959db796ac3b596a6c6c1e6`, after both runs.

**Observable consequence:** The audit can verify those two earlier official image IDs, but cannot verify that the immutable candidate's official image boots and autoscale-joins. This is a proof gap, not a claim that the earlier image failed.

**Required testable correction:** Run both existing journeys after the candidate code and documentation are committed, record their source commit and image IDs, and establish that the source identity is the immutable candidate sent for review. Keep published-digest proof deferred to release as approved. No new test or image publication is needed.

### TASKREV-R4-02 — MISSING: production guide does not supply a runnable Scribe deployment

**Violated obligation:** The R3 task's explicit complete production Kubernetes startup journey, including autoscaling and peer certificate example, must let an operator bring up both the fixed `all` anchor and autoscaled `scribe` template in order.

**Location and evidence:** `docs/src/content/docs/self-hosting/kubernetes-production.svx:154-230` supplies only the `wyrd-core` ServiceAccount, Service, and StatefulSet manifest. Lines 232-236 ask the reader to derive a second StatefulSet by changing its name, selector, target and sizing; lines 238-243 immediately run `kubectl rollout status statefulset/wyrd-core` and `statefulset/wyrd-scribe`, with no command applying either production manifest. The HPA at lines 292-304 targets that absent Scribe StatefulSet. The local kind manifest is a separate test artifact with local-only values, so it cannot silently complete the production walkthrough.

**Observable consequence:** Following the guide as written leaves no `wyrd-scribe` to roll out or autoscale. The promised production peer topology and credentialed first-use request cannot be completed from the documented sequence.

**Required testable correction:** Provide a concrete production `wyrd-scribe` manifest or an exact, safe derivation from the existing anchor example, including its distinct selector, `WYRD_TARGET=scribe`, persistent claim, shared storage/peer TLS/serving Secrets, and sizing, then give the apply command before both rollout waits. Walk those steps against the resulting manifests and keep kind-only thresholds out of production. No new deployment controller or architecture is needed.

## Result and limits

**FAIL.** The two findings above are bounded acceptance/proof gaps. The R3 correction code appears to close findings 14–22 on the inspected paths. This review did not rerun Docker, kind, Postgres, or the broad journeys; it used the committed evidence and source. No additional optional improvement or unrelated debt is proposed.
