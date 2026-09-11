# Independent Ponytail validation

- Subject: `442da074cb316be7f580694ba8274229561935a8..81eaa346e643ac6315e517041fc838c41057f7ac`
- Result: `SIMPLIFY BEFORE SHIP`

Only finding 6 is a Ponytail complexity/scope finding. Findings 1–5 concern
correctness, architecture, repository standards, or verification and are
outside the complexity-only Ponytail review boundary.

| Finding | Result | Independent evidence |
|---|---|---|
| `FIND-BIFROST-OTEL-T04-1` | `OUT_OF_SCOPE_FOR_PONYTAIL` | The cross-principal collision path is real: `otlp_batch_id` hashes tenant/table/logical payload, while the reused logical identity excludes `principal_id`. Whether that violates the approved identity contract is a correctness/architecture decision. |
| `FIND-BIFROST-OTEL-T04-2` | `OUT_OF_SCOPE_FOR_PONYTAIL` | Hash bytes have only their version/variant bits changed; the test proves the version nibble, not UUIDv7 timestamp semantics. This is contract correctness. |
| `FIND-BIFROST-OTEL-T04-3` | `OUT_OF_SCOPE_FOR_PONYTAIL` | Qualified signature types and the function-local `sha2` import violate explicit repository rules, but add no unnecessary abstraction. |
| `FIND-BIFROST-OTEL-T04-4` | `OUT_OF_SCOPE_FOR_PONYTAIL` | Missing required rustdoc is documentation compliance, not excess complexity. |
| `FIND-BIFROST-OTEL-T04-5` | `OUT_OF_SCOPE_FOR_PONYTAIL` | The report calls `verify:bifrost` passed while recording 8/9, and the task-required aggregate gate is absent. This is verification credibility. |
| `FIND-BIFROST-OTEL-T04-6` | `UPHOLD` | The candidate adds three unrelated `py-error-refactor` files. Remove them from the T04 range and review them with their own change. |

The new helper is otherwise lean: private, single-caller, reuses the existing
digest and fence, and adds no dependency or abstraction. Ponytail neither
validates nor refutes `SPEC_REVISION_REQUIRED`; that verdict depends on the
separate correctness and architecture findings.
