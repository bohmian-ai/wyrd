# Workflow Revision 12 task packet

Status: READY — independent architecture/executability review passed. Revision 12 was
explicitly approved by the user on 2026-10-02. Production implementation is
stopped at the original TASK-002 candidate; this revision changes instructions,
not product code. Prior readiness/implementation verdicts do not approve this
revised packet. New selectors are planned and require selection/RED/GREEN proof.

Use `wyrd-implement` for implementation, `wyrd-task-review` for immutable
cumulative acceptance, and `wyrd-change-review` for integrated closure. Caller
owns scheduling/Git. Preserve the stopped implementation and prior reviews;
no reset/rebase. Superseded TASK-002 is history, not executable authority.

## Outcomes and dependency order

1. [TASK-001](TASK-001-explicit-local-runtime.md) — existing native runtime
   work carried forward. Its existing review/evidence remain candidate-bound;
   do not infer acceptance from task sequencing. Close outstanding review
   obligations before accepting a dependent implementation. Reopen affected
   invariant-bearing code if cleanup changes it.
2. [TASK-002-cleanup](TASK-002-cleanup.md) — delete duplicate machinery and
   deliver automatic file/exact registered loading with Native execution through
   Rust/Python/TypeScript; preserve registration/provenance/UID protections.
3. [TASK-003](TASK-003-remote-client-and-public-gateway.md) — shared remote
   handle, gateway caller/ingress and common local execution configuration;
   depends on TASK-001 and cleanup, not server-run route availability.
4. [TASK-004](TASK-004-accepted-server-jobs.md) — bounded accepted server jobs,
   existing server graph/validation, scoped built-in reads; depends on the above.
5. [TASK-005](TASK-005-cli-and-integrated-proof.md) — thin CLI, compiled apply,
   team reuse and three-SDK route journeys, integrated authority/proof closure.

TASK-002-cleanup must replace consumers and validation in the same complete
outcome; deletion cannot leave an insecure registration or broken client. No
new Workflow principal, public loader/hydrator, Workflow-root WyrdState, duplicate
graph/parser/executor, or per-language configuration owner is authorized.

## Obligation ownership and proof

The table maps the current specification IDs, including Revision 12 additions.
Original task metadata retains consumer closure. TASK-001 rows carry its native
runtime obligations; new language loading/configuration closure belongs to
cleanup/TASK-003/TASK-005. This is a proof plan, not executed evidence.

| Obligation | Primary task | Required proof |
|---|---|---|
| AC-001 | [TASK-002-cleanup](TASK-002-cleanup.md) | Cleanup S1–S6 + T5 S2/S5: exact automatic loading/registration across SDKs/CLI |
| AC-002 | [TASK-002-cleanup](TASK-002-cleanup.md) | Cleanup S1–S6 + T5 S2/S5: exact automatic loading/registration across SDKs/CLI |
| AC-003 | [TASK-002-cleanup](TASK-002-cleanup.md) | Cleanup S1–S6 + T5 S2/S5: exact automatic loading/registration across SDKs/CLI |
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
| AC-013 | [TASK-005](TASK-005-cli-and-integrated-proof.md) | T1–T5 focused owners + real Rust/Python/TypeScript/CLI journeys |
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
| AC-029 | [TASK-002-cleanup](TASK-002-cleanup.md) | Cleanup S1–S6: public three-SDK/registration/provenance/pinning proof |
| AC-030 | [TASK-002-cleanup](TASK-002-cleanup.md) | Cleanup S1–S6: public three-SDK/registration/provenance/pinning proof |
| AC-031 | [TASK-005](TASK-005-cli-and-integrated-proof.md) | Cleanup static audit + T5 S5/integrated declarations and shared-owner review |
| INV-001 | [TASK-001](TASK-001-explicit-local-runtime.md) | Runtime S1–S7 contract/bindings/routes/lifetime/local-language |
| INV-002 | [TASK-001](TASK-001-explicit-local-runtime.md) | Runtime S1–S7 contract/bindings/routes/lifetime/local-language |
| INV-003 | [TASK-001](TASK-001-explicit-local-runtime.md) | Runtime S1–S7 contract/bindings/routes/lifetime/local-language |
| INV-004 | [TASK-001](TASK-001-explicit-local-runtime.md) | Runtime S1–S7 contract/bindings/routes/lifetime/local-language |
| INV-005 | [TASK-002-cleanup](TASK-002-cleanup.md) | Cleanup S1–S6 + T5 S2/S5: exact automatic loading/registration across SDKs/CLI |
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
| REQ-014 | [TASK-002-cleanup](TASK-002-cleanup.md) | Cleanup S1–S6 + T5 S2/S5: exact automatic loading/registration across SDKs/CLI |
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
| REQ-025 | [TASK-002-cleanup](TASK-002-cleanup.md) | Cleanup S1–S6 + T5 S2/S5: exact automatic loading/registration across SDKs/CLI |
| REQ-026 | [TASK-005](TASK-005-cli-and-integrated-proof.md) | CLI scenarios and static architecture/aggregate closure |
| REQ-027 | [TASK-005](TASK-005-cli-and-integrated-proof.md) | CLI scenarios and static architecture/aggregate closure |
| REQ-028 | [TASK-002-cleanup](TASK-002-cleanup.md) | Cleanup S1–S6 + T5 S2/S5: exact automatic loading/registration across SDKs/CLI |
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
| REQ-053 | [TASK-001](TASK-001-explicit-local-runtime.md) | Existing native runtime/telemetry proof, candidate-bound review closure |
| REQ-054 | [TASK-002-cleanup](TASK-002-cleanup.md) | Cleanup S1–S6: public three-SDK/registration/provenance/pinning proof |
| REQ-055 | [TASK-002-cleanup](TASK-002-cleanup.md) | Cleanup S1–S6: public three-SDK/registration/provenance/pinning proof |
| REQ-056 | [TASK-002-cleanup](TASK-002-cleanup.md) | Cleanup S1–S6: public three-SDK/registration/provenance/pinning proof |
| REQ-057 | [TASK-002-cleanup](TASK-002-cleanup.md) | Cleanup S1–S6: public three-SDK/registration/provenance/pinning proof |
| REQ-058 | [TASK-003](TASK-003-remote-client-and-public-gateway.md) | T3 S4 + T5 S5: shared selected local dependencies across SDKs/CLI |
| REQ-059 | [TASK-002-cleanup](TASK-002-cleanup.md) | Cleanup S1–S6: public three-SDK/registration/provenance/pinning proof |

## Public seams, reuse, and verification

Revision 12 fixes Workflow.from_path/fromPath and Cards.workflow.load contracts
in all three SDKs with automatic lazy refs and exact selectors. Cleanup states
loading/registration ordering and old-machinery deletion. TASK-003 owns shared
local config, public gateway and remote wire contracts. TASK-004 retains scoped
server acceptance/cancellation/drain ordering. TASK-005 delegates CLI modes and
proves actual compiled apply and all three local SDK route surfaces.

Every active task has a source-backed capability/owner/callers/tests/gap/extension
map. Extending an existing owner is required unless source evidence proves it
insufficient. Public/architectural boundaries are fixed; private helpers, module
layout and fixture choices remain implementation-owned. Independent review
must challenge prescribed duplication, not merely task compliance.

TASK-005's final aggregate is `mise run gate`. Explicit selection gaps include
the new ignored Rust SDK Workflow target and ignored Workflow CLI journeys;
run their exact commands separately. Python Cards integration currently selects
one file and TS integration its directory; prove new tests are selected before
claiming aggregate coverage. Runtime tests run in their owning languages.

## Historical records

The original TASK-002 and its review reports remain in the packet as superseded
history. Earlier readiness reviews apply to their earlier revisions only.
TASK-001 retains its existing candidate-bound findings; neither this index nor
planning asserts a new implementation PASS. The [Revision 12 readiness review](../review/revision12-plan/review.md)
reports Ready: architecture PASS, executability PASS, zero Critical/Major findings.
Its [cold rehearsal](../review/revision12-plan/cold.md) and
[system review](../review/revision12-plan/system.md) are retained. One omitted
UID-replacement regression recipe was repaired before the final verdict.
This is plan readiness; production implementation and runtime proof remain pending.
