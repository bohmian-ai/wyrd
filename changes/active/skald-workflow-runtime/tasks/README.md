# Workflow Revision 11 task packet

Status: READY — independent plan-readiness re-review passed on 2026-10-01:
architecture PASS, executability PASS, rehearsal PASS, 0 Critical/0 Major.
No implementation or tests have been executed by planning. Every named new selector is marked planned;
implementers must add it, prove selection, and record RED/GREEN evidence.

Use `$wyrd-implement` for each task. Run Cargo-backed verification sequentially
across shared targets. Preserve unrelated user edits. Task metadata is the
full obligation/write closure; the table below assigns a primary owner so no
obligation is left for an unspecified later task.

## Outcomes and dependency order

1. [TASK-001](TASK-001-explicit-local-runtime.md) — one buildable explicit contract/runtime/local
   Python/provider-egress outcome; no contract-only broken intermediate state.
2. [TASK-002](TASK-002-load-and-register-graphs.md) — shared loading/exact registry graph/registration
   validation; depends on TASK-001.
3. [TASK-003](TASK-003-remote-client-and-public-gateway.md) — shared remote handle/public gateway adapter and
   ingress fallback; depends on TASK-001, not server host availability.
4. [TASK-004](TASK-004-accepted-server-jobs.md) — bounded accepted server jobs/built-in reads;
   depends on TASK-001–003.
5. [TASK-005](TASK-005-cli-and-integrated-proof.md) — CLI and integrated journeys/authority closure;
   depends on TASK-001–004.

TASK-002/003 are logically independent after TASK-001 but share some client
exports; the execution harness chooses serial or coordinated editing. No task
may leave compilation to a dependent repair. No arbitrary files-only phase or
new scheduler/tool registration is authorized.

## Obligation ownership and proof

All 111 unique REQ/INV/AC IDs in the approved spec are mapped. REQ/INV rows
use their task's ordered scenarios plus relevant AC rows below; metadata in
consumer tasks preserves cross-owner closure. AC rows name concrete proof.

| Obligation | Primary task | Required proof |
|---|---|---|
| AC-001 | [TASK-002](TASK-002-load-and-register-graphs.md) | T2 S1 + T5 S2: actual bundle/native loader and Rust/CLI execution |
| AC-002 | [TASK-002](TASK-002-load-and-register-graphs.md) | T2 S2 + T5 S2: registered exact relationships and CLI apply |
| AC-003 | [TASK-002](TASK-002-load-and-register-graphs.md) | T2 S3 + T5 S2: registered local Rust/CLI |
| AC-004 | [TASK-004](TASK-004-accepted-server-jobs.md) | T4 S2/S3 + T5 S3: real client/server, lost reply and pinned graph |
| AC-005 | [TASK-001](TASK-001-explicit-local-runtime.md) | T1 S2/S3 + T5 S2: overlap and dependency readiness |
| AC-006 | [TASK-001](TASK-001-explicit-local-runtime.md) | T1 S1 + T2 S1/S2: pure and resolved negative validation |
| AC-007 | [TASK-001](TASK-001-explicit-local-runtime.md) | T1 S2 + T5 S2: colliding structured keys stay namespaced |
| AC-008 | [TASK-001](TASK-001-explicit-local-runtime.md) | T1 S2/S3 + T4 S6: partial failure and complete terminal state |
| AC-009 | [TASK-004](TASK-004-accepted-server-jobs.md) | T4 S1/S4/S6/S7: side-effect-free admission/authority/bounds |
| AC-010 | [TASK-004](TASK-004-accepted-server-jobs.md) | T4 S1/S4/S5: canonical allow/deny audit and fail-closed acceptance |
| AC-011 | [TASK-001](TASK-001-explicit-local-runtime.md) | T1 S1/S4 + T5 S2/S4: exact route names/precedence |
| AC-011A | [TASK-003](TASK-003-remote-client-and-public-gateway.md) | T1 S4 + T3 S2/S3 + T4 S5: both caller projections/native errors |
| AC-012 | [TASK-005](TASK-005-cli-and-integrated-proof.md) | T1/T3/T4 codegen/served document + T5 JSON/aggregate |
| AC-013 | [TASK-005](TASK-005-cli-and-integrated-proof.md) | T1–T5 focused owners + real Rust/CLI journeys |
| AC-014 | [TASK-004](TASK-004-accepted-server-jobs.md) | T4 S5 + T5 S2/S4: registered route boundary journeys |
| AC-015 | [TASK-004](TASK-004-accepted-server-jobs.md) | T4 S5 + T5 S4: supported five-dialect/capability matrix |
| AC-016 | [TASK-001](TASK-001-explicit-local-runtime.md) | T1 S5 + T4 S5 + T5 S4: bound egress negatives/private local |
| AC-017 | [TASK-004](TASK-004-accepted-server-jobs.md) | T4 S1/S5 + T5 S4: route immutability/credential nonexposure |
| AC-018 | [TASK-004](TASK-004-accepted-server-jobs.md) | T4 S2/S6 + T5 S3 + deployment docs: ephemeral lifecycle |
| AC-019 | [TASK-001](TASK-001-explicit-local-runtime.md) | T1 S1/S2/S6 + T3 S1 + T4 S6/S7: exact DTO/fields/codes/wait |
| AC-020 | [TASK-001](TASK-001-explicit-local-runtime.md) | T1 S3/S7 + T4 S4/S6: retries/drain/Python/tools |
| AC-021 | [TASK-004](TASK-004-accepted-server-jobs.md) | T4 S2: exact-once preparation/reservation/replay cleanup |
| AC-022 | [TASK-004](TASK-004-accepted-server-jobs.md) | T4 S6: deterministic lifecycle races/eviction/whole snapshots |
| AC-023 | [TASK-001](TASK-001-explicit-local-runtime.md) | T1 S5 + T5 S4 + boundary/regression checks: narrow egress move |
| AC-024 | [TASK-001](TASK-001-explicit-local-runtime.md) | T1 predependency compile/lint/codegen/Python proof; T5 aggregate |
| AC-025 | [TASK-004](TASK-004-accepted-server-jobs.md) | T4 S4: actual YAML Agent both tools, real data/auth/audit |
| AC-026 | [TASK-001](TASK-001-explicit-local-runtime.md) | T1 S3/S7 + T5 S4: caller custom tool/undeclared refusal |
| AC-027 | [TASK-004](TASK-004-accepted-server-jobs.md) | T4 S3/S4/S5: accepted scoped context/token expiry/live owners |
| AC-028 | [TASK-004](TASK-004-accepted-server-jobs.md) | T4 S7 + T1 S1/S6: graph/terminal reserve/sibling responsiveness |
| INV-001 | [TASK-001](TASK-001-explicit-local-runtime.md) | Runtime S1–S7 contract/bindings/routes/lifetime/local-language |
| INV-002 | [TASK-001](TASK-001-explicit-local-runtime.md) | Runtime S1–S7 contract/bindings/routes/lifetime/local-language |
| INV-003 | [TASK-001](TASK-001-explicit-local-runtime.md) | Runtime S1–S7 contract/bindings/routes/lifetime/local-language |
| INV-004 | [TASK-001](TASK-001-explicit-local-runtime.md) | Runtime S1–S7 contract/bindings/routes/lifetime/local-language |
| INV-005 | [TASK-002](TASK-002-load-and-register-graphs.md) | Loading/registration S1–S3 exact native graph |
| INV-006 | [TASK-004](TASK-004-accepted-server-jobs.md) | Server S1–S7 admission/authority/tools/lifecycle/bounds |
| INV-007 | [TASK-005](TASK-005-cli-and-integrated-proof.md) | CLI scenarios and static architecture/aggregate closure |
| INV-008 | [TASK-001](TASK-001-explicit-local-runtime.md) | Runtime S1–S7 contract/bindings/routes/lifetime/local-language |
| INV-009 | [TASK-001](TASK-001-explicit-local-runtime.md) | Runtime S1–S7 contract/bindings/routes/lifetime/local-language |
| INV-010 | [TASK-001](TASK-001-explicit-local-runtime.md) | Runtime S1–S7 contract/bindings/routes/lifetime/local-language |
| INV-010A | [TASK-001](TASK-001-explicit-local-runtime.md) | Runtime S1–S7 contract/bindings/routes/lifetime/local-language |
| INV-011 | [TASK-001](TASK-001-explicit-local-runtime.md) | Runtime S1–S7 contract/bindings/routes/lifetime/local-language |
| INV-012 | [TASK-001](TASK-001-explicit-local-runtime.md) | Runtime S1–S7 contract/bindings/routes/lifetime/local-language |
| INV-013 | [TASK-004](TASK-004-accepted-server-jobs.md) | Server S1–S7 admission/authority/tools/lifecycle/bounds |
| INV-014 | [TASK-001](TASK-001-explicit-local-runtime.md) | Runtime S1–S7 contract/bindings/routes/lifetime/local-language |
| INV-015 | [TASK-005](TASK-005-cli-and-integrated-proof.md) | CLI scenarios and static architecture/aggregate closure |
| INV-016 | [TASK-001](TASK-001-explicit-local-runtime.md) | Runtime S1–S7 contract/bindings/routes/lifetime/local-language |
| INV-017 | [TASK-001](TASK-001-explicit-local-runtime.md) | Runtime S1–S7 contract/bindings/routes/lifetime/local-language |
| INV-018 | [TASK-004](TASK-004-accepted-server-jobs.md) | Server S1–S7 admission/authority/tools/lifecycle/bounds |
| INV-019 | [TASK-004](TASK-004-accepted-server-jobs.md) | Server S1–S7 admission/authority/tools/lifecycle/bounds |
| INV-020 | [TASK-003](TASK-003-remote-client-and-public-gateway.md) | Client S1–S3 native transport/public ingress |
| INV-021 | [TASK-004](TASK-004-accepted-server-jobs.md) | Server S1–S7 admission/authority/tools/lifecycle/bounds |
| INV-022 | [TASK-004](TASK-004-accepted-server-jobs.md) | Server S1–S7 admission/authority/tools/lifecycle/bounds |
| INV-023 | [TASK-004](TASK-004-accepted-server-jobs.md) | Server S1–S7 admission/authority/tools/lifecycle/bounds |
| REQ-001 | [TASK-001](TASK-001-explicit-local-runtime.md) | Runtime S1–S7 contract/bindings/routes/lifetime/local-language |
| REQ-002 | [TASK-001](TASK-001-explicit-local-runtime.md) | Runtime S1–S7 contract/bindings/routes/lifetime/local-language |
| REQ-003 | [TASK-001](TASK-001-explicit-local-runtime.md) | Runtime S1–S7 contract/bindings/routes/lifetime/local-language |
| REQ-004 | [TASK-001](TASK-001-explicit-local-runtime.md) | Runtime S1–S7 contract/bindings/routes/lifetime/local-language |
| REQ-005 | [TASK-001](TASK-001-explicit-local-runtime.md) | Runtime S1–S7 contract/bindings/routes/lifetime/local-language |
| REQ-006 | [TASK-001](TASK-001-explicit-local-runtime.md) | Runtime S1–S7 contract/bindings/routes/lifetime/local-language |
| REQ-007 | [TASK-001](TASK-001-explicit-local-runtime.md) | Runtime S1–S7 contract/bindings/routes/lifetime/local-language |
| REQ-008 | [TASK-001](TASK-001-explicit-local-runtime.md) | Runtime S1–S7 contract/bindings/routes/lifetime/local-language |
| REQ-009 | [TASK-001](TASK-001-explicit-local-runtime.md) | Runtime S1–S7 contract/bindings/routes/lifetime/local-language |
| REQ-010 | [TASK-001](TASK-001-explicit-local-runtime.md) | Runtime S1–S7 contract/bindings/routes/lifetime/local-language |
| REQ-011 | [TASK-001](TASK-001-explicit-local-runtime.md) | Runtime S1–S7 contract/bindings/routes/lifetime/local-language |
| REQ-012 | [TASK-001](TASK-001-explicit-local-runtime.md) | Runtime S1–S7 contract/bindings/routes/lifetime/local-language |
| REQ-013 | [TASK-001](TASK-001-explicit-local-runtime.md) | Runtime S1–S7 contract/bindings/routes/lifetime/local-language |
| REQ-013A | [TASK-001](TASK-001-explicit-local-runtime.md) | Runtime S1–S7 contract/bindings/routes/lifetime/local-language |
| REQ-014 | [TASK-002](TASK-002-load-and-register-graphs.md) | Loading/registration S1–S3 exact native graph |
| REQ-015 | [TASK-001](TASK-001-explicit-local-runtime.md) | Runtime S1–S7 contract/bindings/routes/lifetime/local-language |
| REQ-016 | [TASK-001](TASK-001-explicit-local-runtime.md) | Runtime S1–S7 contract/bindings/routes/lifetime/local-language |
| REQ-017 | [TASK-001](TASK-001-explicit-local-runtime.md) | Runtime S1–S7 contract/bindings/routes/lifetime/local-language |
| REQ-018 | [TASK-001](TASK-001-explicit-local-runtime.md) | Runtime S1–S7 contract/bindings/routes/lifetime/local-language |
| REQ-019 | [TASK-001](TASK-001-explicit-local-runtime.md) | Runtime S1–S7 contract/bindings/routes/lifetime/local-language |
| REQ-020 | [TASK-001](TASK-001-explicit-local-runtime.md) | Runtime S1–S7 contract/bindings/routes/lifetime/local-language |
| REQ-021 | [TASK-001](TASK-001-explicit-local-runtime.md) | Runtime S1–S7 contract/bindings/routes/lifetime/local-language |
| REQ-022 | [TASK-001](TASK-001-explicit-local-runtime.md) | Runtime S1–S7 contract/bindings/routes/lifetime/local-language |
| REQ-023 | [TASK-001](TASK-001-explicit-local-runtime.md) | Runtime S1–S7 contract/bindings/routes/lifetime/local-language |
| REQ-024 | [TASK-005](TASK-005-cli-and-integrated-proof.md) | CLI scenarios and static architecture/aggregate closure |
| REQ-025 | [TASK-002](TASK-002-load-and-register-graphs.md) | Loading/registration S1–S3 exact native graph |
| REQ-026 | [TASK-005](TASK-005-cli-and-integrated-proof.md) | CLI scenarios and static architecture/aggregate closure |
| REQ-027 | [TASK-005](TASK-005-cli-and-integrated-proof.md) | CLI scenarios and static architecture/aggregate closure |
| REQ-028 | [TASK-002](TASK-002-load-and-register-graphs.md) | Loading/registration S1–S3 exact native graph |
| REQ-029 | [TASK-004](TASK-004-accepted-server-jobs.md) | Server S1–S7 admission/authority/tools/lifecycle/bounds |
| REQ-030 | [TASK-004](TASK-004-accepted-server-jobs.md) | Server S1–S7 admission/authority/tools/lifecycle/bounds |
| REQ-031 | [TASK-003](TASK-003-remote-client-and-public-gateway.md) | Client S1–S3 native transport/public ingress |
| REQ-032 | [TASK-004](TASK-004-accepted-server-jobs.md) | Server S1–S7 admission/authority/tools/lifecycle/bounds |
| REQ-032A | [TASK-004](TASK-004-accepted-server-jobs.md) | Server S1–S7 admission/authority/tools/lifecycle/bounds |
| REQ-033 | [TASK-004](TASK-004-accepted-server-jobs.md) | Server S1–S7 admission/authority/tools/lifecycle/bounds |
| REQ-034 | [TASK-004](TASK-004-accepted-server-jobs.md) | Server S1–S7 admission/authority/tools/lifecycle/bounds |
| REQ-034A | [TASK-004](TASK-004-accepted-server-jobs.md) | Server S1–S7 admission/authority/tools/lifecycle/bounds |
| REQ-034B | [TASK-004](TASK-004-accepted-server-jobs.md) | Server S1–S7 admission/authority/tools/lifecycle/bounds |
| REQ-034C | [TASK-004](TASK-004-accepted-server-jobs.md) | Server S1–S7 admission/authority/tools/lifecycle/bounds |
| REQ-035 | [TASK-001](TASK-001-explicit-local-runtime.md) | Runtime S1–S7 contract/bindings/routes/lifetime/local-language |
| REQ-036 | [TASK-001](TASK-001-explicit-local-runtime.md) | Runtime S1–S7 contract/bindings/routes/lifetime/local-language |
| REQ-036A | [TASK-001](TASK-001-explicit-local-runtime.md) | Runtime S1–S7 contract/bindings/routes/lifetime/local-language |
| REQ-037 | [TASK-001](TASK-001-explicit-local-runtime.md) | Runtime S1–S7 contract/bindings/routes/lifetime/local-language |
| REQ-038 | [TASK-001](TASK-001-explicit-local-runtime.md) | Runtime S1–S7 contract/bindings/routes/lifetime/local-language |
| REQ-039 | [TASK-001](TASK-001-explicit-local-runtime.md) | Runtime S1–S7 contract/bindings/routes/lifetime/local-language |
| REQ-040 | [TASK-001](TASK-001-explicit-local-runtime.md) | Runtime S1–S7 contract/bindings/routes/lifetime/local-language |
| REQ-041 | [TASK-004](TASK-004-accepted-server-jobs.md) | Server S1–S7 admission/authority/tools/lifecycle/bounds |
| REQ-042 | [TASK-001](TASK-001-explicit-local-runtime.md) | Runtime S1–S7 contract/bindings/routes/lifetime/local-language |
| REQ-043 | [TASK-001](TASK-001-explicit-local-runtime.md) | Runtime S1–S7 contract/bindings/routes/lifetime/local-language |
| REQ-044 | [TASK-005](TASK-005-cli-and-integrated-proof.md) | CLI scenarios and static architecture/aggregate closure |
| REQ-045 | [TASK-001](TASK-001-explicit-local-runtime.md) | Runtime S1–S7 contract/bindings/routes/lifetime/local-language |
| REQ-046 | [TASK-003](TASK-003-remote-client-and-public-gateway.md) | Client S1–S3 native transport/public ingress |
| REQ-047 | [TASK-001](TASK-001-explicit-local-runtime.md) | Runtime S1–S7 contract/bindings/routes/lifetime/local-language |
| REQ-048 | [TASK-001](TASK-001-explicit-local-runtime.md) | Runtime S1–S7 contract/bindings/routes/lifetime/local-language |
| REQ-049 | [TASK-001](TASK-001-explicit-local-runtime.md) | Runtime S1–S7 contract/bindings/routes/lifetime/local-language |
| REQ-050 | [TASK-004](TASK-004-accepted-server-jobs.md) | Server S1–S7 admission/authority/tools/lifecycle/bounds |
| REQ-051 | [TASK-001](TASK-001-explicit-local-runtime.md) | Runtime S1–S7 contract/bindings/routes/lifetime/local-language |
| REQ-052 | [TASK-004](TASK-004-accepted-server-jobs.md) | Server S1–S7 admission/authority/tools/lifecycle/bounds |

## Public seams and proof discipline

TASK-001 embeds native DTO/call/dependency/limits/binding shapes and runtime
ordering. TASK-002 fixes the shared loading composition and exact native resolver
precedent; TASK-003 embeds facade/HTTP/header semantics; TASK-004 fixes host
config, trusted context, service/audit seams and lifecycle ordering; TASK-005
fixes CLI/config/projection. Existing uniquely named source precedents refer to
current implementation, not removed gateway Revision 21 artifacts.

All new behavior has ordered Behavior/RED/GREEN/REFACTOR scenarios. Static
schema/generated/docs/relocation obligations use stated static/regression proof,
not manufactured failing tests. Unit/integration evidence does not substitute
for real registered Rust/CLI client→server→client journeys. No new remote
Python/TypeScript/MCP Workflow API is claimed. Python local behavior remains
language-owned, using the single engine and public package exports.

The final aggregate is `mise run gate`; the new ignored CLI Workflow journeys
are an explicit selection gap and their exact focused wrapper commands remain
separate required final proof. Do not duplicate the aggregate's component lanes.
No live cloud/model credentials or synthetic host load are needed.

## Planning limitations

This is source/manifest/mise-backed planning, not execution evidence. New test
selectors, CLI behavior, runtime memory/time bounds and sibling responsiveness
are unproven until implementation executes their named scenarios. The independent
plan-readiness review requested by the user has validated this packet for
implementation. Readiness is not evidence that implementation or tests pass.

## Readiness revision record

- 2026-10-01, PR-001 validated against the packet and approved specification:
  TASK-001 now locally states binding grammar/transitive visibility, exact
  ExtGateway trust restrictions and gateway retry codes; TASK-003 locally
  states protocol-native error field/category/remediation projection. No
  approved semantics changed.
- 2026-10-01, PR-002 validated against ResultCollector and
  RunningQueryControls: TASK-004 fixes tracked query ownership before exposing
  an abortable waiter, signal/Skald-drain/query-join/terminal ordering, bounded
  honest settlement, and real forwarded-query cancel/deadline/shutdown proof.
  TASK-001 records the cross-owner dependency. No new durable service or
  token authority is introduced. Re-review is required; this record is not
  a readiness verdict.
- 2026-10-01, independent re-review returned Ready for all five tasks and 24
  scenarios, with architecture/executability/rehearsal PASS and no blocking
  findings. Validated report:
  [r2 review](../../../../.dev/review/skald-workflow-runtime-plan-20261001-r2/review.md).
  Root then changed only task status metadata from proposed to ready and
  recorded this handoff; the reviewed behavioral contracts remain unchanged.
