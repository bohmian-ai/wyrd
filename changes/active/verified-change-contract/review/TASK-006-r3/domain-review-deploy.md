# Deployment domain review — TASK-006 R2 cumulative candidate

**Result: FAIL.** One material documentation/authority conflict remains. The image startup and local kind autoscaling implementation otherwise meet the deployment obligations inspected here.

## Immutable subject and boundary

- Repository: `/home/thorrester/Documents/GitHub/wyrd-vcc-t006`; remediation base `f500ea38bc749f36b3ee8d88893dcf7c0161435c`; candidate `2fbe90cd880936d8e4f25173839e4cc4fa18f0b4` (HEAD when inspected); cumulative TASK-006 base `f8811ac5035c3aa165d34c38992f9889b3c9081f`.
- Approved specification: `changes/active/verified-change-contract/spec.md`, revision 40, especially REQ-153–165 and AC-034–037. Original task: `tasks/TASK-006-continuous-eval-verifier.md`; remediation: `review/TASK-006-r2/TASK-006-R2-continuous-eval-closure.md`. Actual base-to-candidate diff inspected for the deployment files; completion prose was treated as claimed verification, not source truth.
- This review covers the official image, first-use startup, public URL routing, external database deployment, peer address injection, and local kind HPA proof. SQL privilege details and private RPC authorization are reviewed by their own domain agents.

## Authority and source coverage

| Boundary | Authorities | Source and proof inspected |
|---|---|---|
| Published image, nginx, BFF, setup and migration boot | `AGENTS.md` §§2, 9, 11–12; `architecture/agent-rules.md`; `architecture/wyrd-design.md`; `architecture/wyrd-doctrine.mdx`; `architecture/operations/deployment-and-release.md`; `architecture/v1/04-surfaces/deployment.md`; spec REQ-153–159, AC-034–035 | `docker/official/Dockerfile`, `extras/entrypoint.sh`, nginx template, `scripts/server/test-startup.sh`, `wyrd-client/tests/startup_image_journey.rs`, BFF root load/upstream and self-hosting docs. Claimed `mise run test:server:startup` pass. |
| Public Rust/Python/TypeScript endpoint derivation | `AGENTS.md` client ownership and journey rules; spec REQ-153, AC-034; `architecture/references/languages/testing-workflows.md` | Shared `wyrd-client` config/transport/facade, Python and TypeScript wrappers and unit tests, image journey through nginx. Claimed startup, Python and TypeScript journey passes. |
| Peer-enabled kind deployment and autoscaling | `AGENTS.md` Bifrost and journey rules; `architecture/bifrost-design.md`; `architecture/operations/deployment-and-release.md`; `architecture/operations/reliability-and-recovery.md`; spec REQ-162–165, AC-036–037; R2 task's required local kind proof | `deploy/kubernetes/kind/{infra,migrate,wyrd,load}.yaml`, `mise.local.toml.example`, `scripts/server/test-kind-autoscale.sh`, `startup_image_journey.rs::kind_seed/kind_join_read`; claimed successful kind run and `test:server:peer` 9/9. |
| Peer trust and rollout documentation touched by the deployment change | `AGENTS.md` §2, `architecture/wyrd-security-posture.md`, `architecture/operations/deployment-and-release.md`; spec REQ-160, REQ-164–165, AC-037 | Actual changed architecture files, server ticket deletion and R2 task decision to synchronize security/protocol docs. |

## Result by obligation

| Obligation | Evidence | Result |
|---|---|---|
| One image serves Rust, live Node BFF, HTTP routes, MCP, health, and public gRPC on 50051 | Dockerfile and entrypoint run all three processes; nginx forwards the named routes verbatim to Rust and gRPC to distinct internal port 50053; startup script asserts real routes, BFF readiness, MCP stream, and SDK gRPC through nginx. | PASS |
| External Postgres, one-off migrate, idempotent setup, restart persistence | Startup script supplies owner URL only to migrate, serves with app/platform URLs, tests schema refusal and recoverable retry, setup idempotence, production restart, and Card/artifact/Bifrost reads after restart. | PASS for deployment path; SQL role audit is separate. |
| Public SDK URL resolution and optional gRPC override | `ClientConfig::from_global_with_overrides` derives endpoint from effective HTTP URL; Python and TypeScript delegate to shared constructor; Rust image journey performs a real derived-port gRPC write and explicit override. | PASS |
| Runtime peer address and shared object storage in local kind template | Both StatefulSets inject `status.podIP` then `WYRD_PEER_ADDRESS=$(POD_IP):50052`, mount distinct PVCs, point to shared S3-compatible store, and use shared TLS secret. Script checks distinct live registrations. | PASS |
| HPA scaled by successful analytical read rate, capped at two, with remote execution | Adapter selects Wyrd's status-200 `POST /v1/query` counter; HPA uses only Pods custom metric at 10/s, max two. Script checks baseline below target, HPA desired two and `SuccessfulRescale`, new pod readiness, peer mTLS refusal/admission, and anchor Oracle counter increase while new Scribe has zero local Oracle executions. Traffic job has rate/concurrency/duration/deadline bounds and is cleaned up. | PASS within local proof; live run evidence is recorded in the R2 task. |
| Deleted ticket trust model and future rollout guidance agree | Changed deployment and security authorities still require signed peer tickets and ticket-specific manifest/replay behavior after implementation and spec removed them. | FAIL — DEPLOY-1 |

## Finding

### DEPLOY-1 — VIOLATION: deployment authorities still require deleted peer tickets

**Violated obligation.** Approved REQ-160 removes signed purpose tickets, keyrings, and nonce replay state; REQ-164 forbids an old/new ticket compatibility shim. The R2 remediation task explicitly requires synchronizing security and protocol architecture documents to the shared-certificate mTLS model. `AGENTS.md` names the current architecture as authority, so these contradictory operating instructions are material.

**Location and evidence.** `architecture/wyrd-security-posture.md:35` still says replica traffic requires mTLS **plus** a signed peer ticket; lines 249–267 still require ticket signature, `kid`, replay checks, rotation, and ticket rejection audit. `architecture/operations/deployment-and-release.md:33–34` still requires the signed ticket contract; lines 170–172 and 189–190 still put peer-ticket claims and versions in the release fingerprint/manifest. Both documents are changed in the candidate, while the server ticket mechanism is deleted.

**Observable consequence.** An operator or future implementer following the active security and release authority would provision nonexistent ticket keys or require a release manifest field for a removed protocol. A release gate built from this guidance could reject the actual peer-mode image or reintroduce the exact replay-capacity path this task removes.

**Testable correction.** Update these two existing authorities to describe the approved dedicated-CA/shared-leaf mTLS boundary and receiver-side context checks. Remove the ticket-specific trust, audit, rotation, fingerprint and manifest instructions; preserve the applicable peer wire/schema/object compatibility and migration rollout requirements. A focused documentation search plus `mise run docs:check` is sufficient proof; no new mechanism or test harness is needed.

## Verification limits

This is a static audit. I did not rerun the costly Docker/kind lanes; the tracked R2 task records the kind run and all listed gates as passing. The kind query proves remote Oracle work and a successful fused read of data written on separate Scribes. It does not independently prove that those rows were still unpublished at the time of the cross-pod tail read; the separate peer journey covers the explicit cross-process tail path, subject to its own domain review. The kind script's anchor is fixed `all` and the autoscaled template is `scribe`, consistent with the current single ready Forge coordinator design.
