# R3 correction amended at maintainer direction

The source finding and FIX_REQUIRED verdict remain unchanged. The maintainer
rejected the earlier retained-future polling recommendation because it preserves
two cleanup-draining capabilities. This is a human-directed correction to the
selected remediation, not a new finding or a new FIND identifier.

## Evidence-backed causal chain

- Cleanup can be discarded when authored timeout cancels an Agent-owned wait.
- The wait can belong to the Agent because both it and the engine call
  `ToolCalls::settle` (`agent.rs:422`, `engine.rs:686`).
- Both can drain because cloneable `BlockCall.tool_calls` exposes the same
  draining API as the engine's ToolCalls owner (`blocks.rs:85–104`,
  `tools.rs:103–190`). The child can take its parent's completion obligation.
- That capability duplication exists because one handle combines registration,
  retry-spanning evidence/accounting and invocation completion. The engine
  actually owns timeout/retry/result decisions, so completion belongs there.

Stopping reason: this is a concrete existing-owner repair boundary. No new
service or lifecycle framework is justified. Safer polling treats cancellation
of the duplicated child drain, rather than removing that ownership transfer.

## Revised selection and independent source check

The existing R2 validator performed a bounded read-only source reassessment
without another discovery round. It confirmed the smallest structural correction:
retain one non-clone owning invocation capability with the engine; give
executors a cloneable registration/accounting/recording capability with no
settlement extraction or drain operation. Exact private type/field shape remains
implementation-owned. Engine action and check callers finish cleanup before
retry, Busy/error/result classification or record publication. Capture execution
outcome before cleanup: an earlier terminal/Busy stop retains that classification
after release, while retryable timeout cases apply only if execution was still
pending at its authored deadline. Cleanup time does not overwrite an already
completed execution outcome. Remove the Agent
child drain and do not add another guard or polling fallback.

`Engine::judge` currently relies on the Agent wait (`engine.rs:768–793`), so it
must receive the same caller-owned completion behavior. Hosted Blocks only
forwards engine calls (`hosted/blocks.rs:197–222`). Action/CheckExecutor docs
(`blocks.rs:170–190`) define low-level one-attempt execution, not another
independent lifecycle owner. Direct repository executor users are test seams:
retain explicit caller-owned settlement in those helpers or use existing engine
fixtures. Do not silently lose their cleanup proof.

The validator confirmed these conclusions through a follow-up message; it did
not edit source, run tests or add findings. The earlier validator report remains
historical evidence; its retained-future polling recommendation is superseded
by this amendment and the revised TASK-001-R3. The validated failure trace,
materiality and nine prior closures stand.

No implementation is changed or claimed. No new tests ran for this amendment.
The next proof checks exclusive capability ownership, held-work timeout/retry
and action/check completion, preserving whole-run/no-result/queue cancellation
non-goals. No specification behavior changes.
