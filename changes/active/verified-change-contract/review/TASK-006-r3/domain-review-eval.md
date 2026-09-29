# TASK-006 R3 Eval Domain Review

**Subject:** cumulative TASK-006 base `f8811ac5035c3aa165d34c38992f9889b3c9081f` through immutable candidate `2fbe90cd880936d8e4f25173839e4cc4fa18f0b4`; R2 remediation range `f500ea38bc749f36b3ee8d88893dcf7c0161435c..2fbe90cd880936d8e4f25173839e4cc4fa18f0b4`.

**Boundary:** acknowledged Eval observations, post-ACK activation, frozen record and trace reads, existing Eval engine and judge, media resolution, canonical items and summary, terminal result/dispatch behavior, and the Scribe tail reads used by the real journey.

## Authority and source coverage

| Authority | Source inspected for this boundary |
|---|---|
| `AGENTS.md` §§9–12; `architecture/agent-rules.md`; `architecture/references/languages/spec-driven-development.md` | Journey priority, negative paths, bounded media, immutable task review, and required evidence. |
| Approved `spec.md` revision 40, especially AC-027 and REQ-159; original `TASK-006`; R1/R2 verdicts and validated findings; `TASK-006-R2` | Existing continuous Eval outcome, previous `FIND-TASK-006-1`–`12`, and the full server journey's tail replay diagnosis. |
| `architecture/wyrd-design.md`; `architecture/bifrost-design.md`; `architecture/references/domain/{evaluation,telemetry-observations}.md`; change-local `architecture/verifier/eval.md` | Eval terminal and evidence semantics, server-owned verification, fused observation reads. |
| `crates/wyrd/wyrd-testing/tests/bifrost/server/eval_verification.rs`; `crates/wyrd/wyrd-server/src/verification/eval.rs` | Full real SDK/server/provider matrix and the unchanged bounded tenant-media resolver. |
| `crates/wyrd/wyrd-server/src/grpc/scribe_tail.rs`; `crates/vala/vala-bifrost-redux/src/scribe/tail_rpc.rs`; R2 diff | Tail ticket deletion, retained query-owned fence validation, and removal of the capacity/replay coupling affecting Eval reads. |

## Result by obligation

| Obligation | Source evidence | Verification evidence | Result |
|---|---|---|---|
| AC-027 valid image reaches the existing judge natively and yields canonical results | `eval_verification.rs:697-741,787-832,918-934`; `eval.rs:1101-1152` | Recorded exact terminal matrix passes; provider capture requires two image-base64 requests with no private URI. | PASS |
| `FIND-TASK-006-12`: unbound, cross-tenant, unsupported MIME, and oversized media fail as execution errors, without result, dispatch, or provider request | `eval_verification.rs:708-765,832-843,918-934`; `assert_unresulted` at `:464-495` queries persisted results and checks dispatch count; `eval.rs:1101-1147` enforces MIME, tenant, and size before encoding | Recorded exact terminal matrix passes; full server journey reports 22/22. The cross-tenant case already satisfied the inaccessible-or-cross-tenant alternative in the prior validated ledger. | PASS |
| `FIND-TASK-006-11`: no extra raw superuser-pool forwarding surface | Three journey callers now use `server.pg_fixture().superuser_pool()` in the R2 diff; the forwarder is deleted. | Recorded focused Eval tests and `check:from-pools-allowlist` pass. | PASS |
| REQ-159 and R2 tail failure: ordinary Eval reads do not exhaust a global one-time nonce cache or falsely audit a fresh read as replay | R2 removes Scribe tail purpose tickets and `ScribeTailAuthority`; `scribe_tail.rs:78-162` delegates typed requests to local Scribe state; `tail_rpc.rs` retains bounded query-owned fences. The journey uses fused reads at `eval_verification.rs:359-374`. | Recorded full server journey passes 22/22 with the original 200 ms trace poll, and reports no false replay audit. | PASS |
| Earlier Eval obligations and `FIND-TASK-006-1`–`10` remain closed | Their engine, activation, record, trace, capture, and result owners are not materially changed by this R2 diff; the existing matrix, replay, authority, and stable-error tests remain in the server journey. | Recorded server, Oracle, Python, and TypeScript journey lanes pass; R2 findings validation had individually closed these findings. | PASS |

## Findings

None.

## Verification limits

This is a static domain audit. I inspected the candidate and the R2 diff, and relied on the recorded exact and broader lane outcomes rather than rerunning long environment-owning journeys. Security of mTLS and peer receiver authorization, SQL roles/migration, image boot, and Kubernetes scaling belong to the other domain reviews. The candidate HEAD remained `2fbe90cd880936d8e4f25173839e4cc4fa18f0b4` during inspection.

## Overall result

**PASS** for the Eval domain. Prior Eval findings are closed and no new material Eval finding was identified.
