# TASK-002 second remediation review scope

Repository: `/Users/stevenforrester/Documents/GitHub/wyrd`.
Original base: `15f939fc917b22efa5e5e4d12fd1f97be807a0a7`.
R1 candidate: `5ef2981e66068e8ee5f96d3be15000eea9cbd94f`.
Prior candidate: `a80afd68e47d992e35f20931984d96524b2e7dbf`.
Candidate: `793912475795951d440b8d072c99a9957ad18464`.
Review incremental `git diff a80afd68e 793912475`; cumulative `15f939fc9...793912475` is available as acceptance context. Requested commits `48a9bc98b` and `793912475` are direct descendants of the prior candidate; no rebasing. Initial tracked source is clean; existing untracked TASK-001-R2/TASK-002-R1/R2 reports remain untouched. No CodeGraph index exists. Candidate and source must remain immutable.

## Authority and bounded acceptance context

Read original approved spec revision 2 and TASK-002; prior R1/R2 verdicts, findings validations and selected reports; R1 lifecycle remediation; and `../TASK-002-R2/TASK-002-R2-recovery-documentation.md`, including its appended evidence. Root AGENTS, spec-driven development, maintainer/Rust guides and applicable testing/Wyrd lifecycle/Bifrost/security authority remain unchanged. No nested AGENTS applies to the three changed Rust paths.

This round covers only remaining FIND-TASK-002-3 closure and the four-file incremental diff: `RunEvidence::pin` docs, storage `upload_init/file_upload_init/init_upload` docs, existing sweeper-test purpose/Panics, and remediation packet/evidence. Confirm no executable/test-body changes. Trace each replacement claim against its existing producer/consumer functions, but do not reopen accepted runtime behavior or expand into unrelated wording.

Carry forward R2's independently validated closure of FIND-1/2, complete prior item inventory acceptance except the specifically remaining entries, and its fresh five-server/three-sweeper/two-SQL/path/two-setup test evidence. R1's unaffected surface/SDK/CLI/MCP/HTTP/tenant/permission/audit/ACK/inventory/validation/non-goal acceptance carries forward through R2. No changed dependency/authority/runtime item is identified that invalidates that evidence. TASK-003's SQL elimination/metadata/canonical readback work remains excluded. Provider-emulator sweeper abort and real-GCS multipart remain disclosed limits, previously assessed as non-blocking here.

## Common navigation map and selected reviewer

| Remaining claim | Existing source to trace |
|---|---|
| One plan precedes serial transfer; unreturned/later/current states and retry | `RunEvidence::pin`, shared `WyrdStorageClient` transfer/completion, HTTP/MCP `VerificationControl::evidence`, initialization commits and completion terminal writes |
| Open versus Failed sweep eligibility and provider-expiry limits | storage `upload_init/file_upload_init/init_upload`, backend-init failure recorder, pending persistence, SQL `expired_uploads_batch`, existing `Sweeper::abort_one` |
| Changed test purpose/Panics | `pg_sweeper::pg_tests::tick_sweeps_expired_uploads_and_reaps_idempotency_rows` unchanged body and fixture/assertions |

One fresh independent ponytail-rev checks this bounded closure, incremental source, carry-forward acceptance, exact mandatory docs and writes `findings-validation.md` here. Map is a starting point, not a conclusion. No specialist/discovery team is selected for documentation-only changes. If a new material blocker is discovered, obtain separate focused source validation before retaining it; no reviewer validates its own new finding.

Available evidence: remediation records successful `fmt:check`, `lints` and whitespace checks, with source comparison instead of manufactured RED/runtime tests. Orchestrator will record fresh static checks. No runtime, cloud, emulator, codegen or full user-journey rerun is warranted without executable changes. If this second remediation review is unsuccessful, include the skill's convergence assessment before prescribing another round; do not expand expectations or repeat broad discovery.
