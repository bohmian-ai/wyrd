# Round-2 acceptance matrix

## Evidence key

- **R1** — complete round-1 acceptance matrix and task closure at commit
  `079794988`, reviewing target `395a3ad91`.
- **F** — follow-up source diff from completed target `ac19e6bbe` to round-2
  target `3d2295f1a`.
- **E** — evidence table in
  `changes/active/skald-workflow-runtime-followups/task.md`.
- **A** — current Wyrd design, doctrine, security posture, Bifrost design, and
  completed record.

The round-2 review rechecked the complete base-to-target subject. Unchanged
obligations retain the implementation and verification evidence detailed in
R1; every obligation is listed below. Obligations affected by the remediation
also cite F/E/A explicitly.

## Required behavior

| Obligation | Implementation evidence | Verification evidence | Result |
|---|---|---|---|
| REQ-001–REQ-025 (including REQ-013A) | R1; unaffected owners remain present | R1; F regression review | PASS |
| REQ-026 | Ambient-only Workflow CLI in F; A records the human decision | E rows 6–7 | PASS |
| REQ-027 | Signal result handling and stable error projection in F | E row 9 | PASS |
| REQ-028–REQ-034C (including REQ-032A, REQ-034A/B/C) | R1; server lifecycle owners unchanged | R1; F regression review | PASS |
| REQ-035–REQ-041 (including REQ-036A) | R1; Revision 14 provider owners unchanged | R1; F regression review | PASS |
| REQ-042 | Standard external-gateway egress and redaction remain; bespoke success scan deleted in F | E row 10; R1 route evidence | PASS |
| REQ-043–REQ-059 | R1; media attribution/docs/OpenAPI corrections in F strengthen existing behavior without changing contracts | E rows 2, 4, 5, 12; R1 | PASS |

## Invariants

| Obligation | Implementation evidence | Verification evidence | Result |
|---|---|---|---|
| INV-001–INV-011 (including INV-010A) | R1; shared owners and Revision 14 schema/destination split unchanged | R1; F regression review | PASS |
| INV-012 | Sensitive headers and error/decode redaction remain; success scanner deleted without replacement | E row 10; A security review | PASS |
| INV-013–INV-023 | R1; accepted-job, deadline, authority, tenancy, and bounded-state owners unchanged | R1; F/A regression review | PASS |

## Acceptance criteria

| Obligation | Implementation evidence | Verification evidence | Result |
|---|---|---|---|
| AC-001–AC-011A (including AC-011A) | R1; original contract/runtime/SDK/server journeys unchanged | R1; F regression review | PASS |
| AC-012 | Served document asserts all three Workflow operations in F | E row 12 | PASS |
| AC-013–AC-014 | R1; unchanged | R1 | PASS |
| AC-015 | Correct effective-provider media refusal in F; provider protocol matrix unchanged | E row 2; R1 | PASS |
| AC-016–AC-031 | R1; scanner removal retains standard negative controls, CLI corrections retain journeys, and no original owner regressed | E rows 6–10; R1; F regression review | PASS |

## Expensive-to-reverse constraints

| Obligation | Implementation evidence | Verification evidence | Result |
|---|---|---|---|
| C01 One Workflow Card; C02 Agent-only steps; C03 explicit dependencies/bindings; C04 namespaced outputs | R1; unchanged | R1 | PASS |
| C05 process-local async lifecycle; C06 terminal failure snapshots; C07 closed route set; C08 Prompt-owned provider/request semantics | R1 plus F provider-attribution/doc corrections | R1; E rows 2, 4, 5 | PASS |
| C09 immutable stored route; C10 server Native refusal; C11 capability-scoped SDK/CLI surfaces; C12 two server tools | R1 plus A ambient-only Workflow CLI decision | R1; E row 6 | PASS |
| C13 process-local idempotency; C14 pinned graph/live gateway governance; C15 exact V1 DTO/client interfaces; C16 one async engine | R1; unchanged | R1 | PASS |
| C17 shared lower egress policy | Standard egress retained; bespoke response scan removed | R1; E row 10 | PASS |
| C18 tracked preparation/atomic acceptance; C19 cohesive contract-consumer boundary; C20 immutable per-call context and Revision 14 wire contract | R1; unchanged | R1; F regression review | PASS |
| C21 whole-step retry/deadline composition; C22 Skald-only drain/gateway settlement; C23 captured authority without token retention; C24 bounded graph/snapshot preparation | R1; unchanged | R1; A regression review | PASS |

## Non-goals

| Obligation | Implementation evidence | Verification evidence | Result |
|---|---|---|---|
| NG01 no Python/TypeScript/MCP remote lifecycle; NG02 no compatibility/implicit binding; NG03 no durable queue/cross-replica recovery; NG04 no persisted run payload | R1; unchanged | R1 | PASS |
| NG05 no dynamic graph/loop/nesting; NG06 no expression language/branching; NG07 no Workflow-owned gateway policy; NG08 no gateway credential administration | R1; F removes rather than adds credential machinery | R1; E row 10 | PASS |
| NG09 no Gateway Card/route registry; NG10 no Tool Card/uploaded code/plugin runtime; NG11 no direct Prompt/MCP steps; NG12 no raw provider-response wire contract | R1; unchanged | R1 | PASS |
| NG13 no second executor/runtime/poll subsystem; NG14 no new egress crate/resolver/per-run lock/queue/distributed coordination | F adds no mechanism and preserves the existing owners | R1; F/A regression review | PASS |

## Constraints and stated non-functional obligations

| Obligation | Implementation evidence | Verification evidence | Result |
|---|---|---|---|
| Security, tenancy, credential, SSRF, audit, and token-free accepted authority constraints | R1 and A; remediation changes no authorization or tenancy owner | R1; F/A regression review | PASS |
| Bounded concurrency, cancellation, retry, total deadline, query-deadline binding, retention, and shutdown constraints | R1; remediation does not touch these owners | R1; source comparison | PASS |
| Rust/Python/TypeScript first-class local journeys, Rust/HTTP/CLI server lifecycle, Cards registration, and TypeScript package cone | R1; test import-only changes preserve journeys | R1; E rows 6 and 8 | PASS |
| No compatibility alias, bespoke check, setting, option, scanner replacement, polling, acknowledgement, or parallel owner | F deletes obsolete mechanisms and introduces none | Direct source/diff inspection | PASS |
