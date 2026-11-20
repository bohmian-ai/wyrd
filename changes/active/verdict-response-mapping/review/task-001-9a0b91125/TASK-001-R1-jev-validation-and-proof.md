---
id: TASK-001-R1
kind: remediation
status: proposed
spec: SPEC-verdict-response-mapping
spec_revision: 1
requirements: [REQ-001, REQ-002, REQ-005, REQ-008, REQ-010, REQ-011, REQ-012, REQ-013, REQ-014]
depends_on: [TASK-001]
parent_task: TASK-001
remediates: [FIND-TASK-001-1, FIND-TASK-001-2, FIND-TASK-001-3, FIND-TASK-001-4, FIND-TASK-001-5, FIND-TASK-001-7]
---

# Complete Jev validation boundaries and acceptance proof

## Subject and outcome

Approved contract: SPEC-verdict-response-mapping revision 1 at
`08998b9e9b6f04825f7743b877391247e0ad9621`. Original task:
`changes/active/verdict-response-mapping/tasks/TASK-001-jev-verdict-checks.md`.
Reviewed candidate: `9a0b9112523252328813041c0b11be375f0eed29`.
The implementation range is base-exclusive, nine commits beginning `5a3caa8b3`.
This is the first remediation packet.

An author must catch invalid policy values at the promised boundary, receive
structured errors, and get a check error for an unusable native Score answer.
An agent must have executable MCP proof of the same result and retained evidence.
Close the required authoring allocation evidence on the corrected candidate.
The maintainer rejected the runtime benchmark gate after review; finding 6 is
withdrawn and requires no remediation or benchmark run. The existing Jev leaf workflow and direct-Verdict checks remain intact.

The diagnosed triggers, authorities, exact locations and independent evidence
are authoritative review records in [findings-validation.md](findings-validation.md);
[verdict.md](verdict.md) provides the reconciled coverage and wait inventory.
They identify settled obligations, not additional product requirements.

## Correction boundaries

1. **Selected Score answer validity — FIND-TASK-001-1; REQ-008/010/012.**
   At Vala's existing `VerdictTransformer`, validate present confidence and
   probability metadata against the native numeric contract before accepting
   the answer. Reuse the existing unit-interval validation owner. Valid low
   confidence must still leave the authored score comparison unchanged;
   absent optional metadata stays valid. Preserve isolated terminal check-error
   handling, exact reasons, selected actual capture and one invocation per check.

2. **Typed Python construction — FIND-TASK-001-2; REQ-011/013.**
   Connect `ProbabilityTransform` construction to shared Rust self-contained
   policy validation. Invalid reversed/out-of-range/nonfinite bounds must refuse
   at construction with field/correction. Preserve optional Pydantic imports,
   dictionary-only use and incremental assembled-graph validation. Reuse the
   shared validator rather than adding Python domain rules.

3. **TypeScript error projection — FIND-TASK-001-3; REQ-011/013.**
   At the new native transform-validation boundary and public consuming setter,
   retain the canonical shared validation error through the existing Wyrd error
   mechanism. Public refusal must expose WyrdError and its existing catalog
   diagnostics. Scope the repair to changed policy behavior; do not rewrite
   unrelated authoring errors or introduce an error family.

4. **Optional-policy parity — FIND-TASK-001-4; REQ-005/013/014.**
   Align the shared AgentCheck decoder and affected SDK native projection with
   the existing optional schema/type contract. Omitted and explicitly null policy
   must both support the existing direct-Verdict workflow. Preserve malformed
   non-null refusal. Any generated projection changes come from the owner and
   generator, never hand edits.

5. **Focused authoring qualification — FIND-TASK-001-5; approved allocation constraint.**
   Record reused captures/native views and added conversions/copies across
   Prompt construction, attachment and graph operations. Supply matched
   before/after same-host profiling with existing tools for representative
   question sizes/shared reuse: allocation volume/bytes, retained memory where
   observable, relevant timing and measurement limits. A supported baseline
   can express equivalent native authoring before the new convenience API.
   This evidence gap alone requires no production change; resolve any established
   avoidable regression at its existing owner and explain necessary extra work.

6. **MCP consumer proof — FIND-TASK-001-7; REQ-005/012/013.**
   Extend the existing MCP verification journey and runtime contract owner for
   a representative Jev leaf execution and `verification.get_execution_details`
   read after existing visibility controls. Reuse its server/provider setup,
   details helper and negative/capture mechanisms. Assert the policy, canonical
   verdict/reason, permitted native answer and existing Agent/Prompt/route
   provenance; include representative concealed capture and unauthorized read
   proof without duplicating all SDK cases. Correct the original task evidence's
   claim that no MCP details tool exists. No new tool, route or envelope is needed.

## Observable closure and focused verification

Keep actual selectors/results in the original task's affected evidence entries
or append them below as remediation evidence. Confirm new test names after they
exist. Run each named case through its exact runner with repository-managed
dependency setup; no broad integrated suite is required to close a focused gap.

| Closure scenario | Findings / requirement | Narrowest credible proof and runner |
| --- | --- | --- |
| Valid Score 2 with absent or valid low confidence follows its comparison; out-of-range/nonfinite optional metadata errors. A hosted malformed answer affects only its own requirement with explicit reason and unchanged call count. | 1; REQ-008/010/012 | Vala inline transformer unit table via exact `mise exec -- cargo nextest run --locked -p vala-verify --lib -E 'test(=<confirmed-name>)'`; representative `gateway_jev` Rust hosted journey through existing Postgres wrapper and `wyrd-ai` integration target, journey profile. Reuse existing Python/TS malformed journeys for affected regression checks. |
| Public Python typed construction refuses reversed/out-of-range/nonfinite bounds before attachment; corrected construction works, mapping-only use stays available without Pydantic. | 2; REQ-011/013 | Focused public Python unit cases in `test_verdict_transform_authoring.py` and relevant optional-export case via existing pytest environment. Retain exact catalog/field assertions at the boundary where applicable. |
| Public TS constructor/setter policy refusal exposes WyrdError, exact `WYRD_SPEC_400_VALIDATION` and field/remediation, then accepts correction. | 3; REQ-011/013 | Existing TS verdict-transform-authoring unit owner via focused Vitest selector after rebuilding changed native binding with owning mise task. Assert structured fields, not just exception text. |
| Omitted and null policies decode consistently with their schema and yield unchanged direct-Verdict behavior; malformed non-null policies still refuse. | 4; REQ-005/013/014 | Shared contract unit/schema assertions plus affected public SDK constructor/setter cases. Reuse one hosted direct-Verdict path for behavioral parity where needed. Run `codegen:check` and N-API declaration check when projections change. |
| Equivalent authoring workloads have actual same-host ownership/allocation observations, timing and declared sampling limits. | 5; allocation constraint | Existing profiling tools and representative construction/attachment/graph workload. Record before/after revisions and workload equivalence. Server benchmark throughput alone is insufficient. |
| Real MCP execute/details and runtime descriptor/schema project a Jev leaf policy/result/capture/provenance with relevant concealment/denial behavior. | 7; REQ-005/012/013 | `crates/wyrd/wyrd-mcp/tests/bifrost/mcp/verification.rs`, target `mcp`, exact confirmed nextest selector with setup from `test:bifrost:journey:mcp`. Record focused GREEN and assertions; no full MCP suite merely for this task. |

Inspect `mise.toml` for owning build/setup/check tasks before running commands.
Finish affected Rust format/lints, Python format/lints/typecheck and TS
format/lints/typecheck/N-API checks, plus relevant generated-contract and
dependency checks when touched. Run Cargo-backed checks sequentially across the
shared build directory. Tests use real required dependencies; only the external
provider is mocked in hosted proof.

Preserve typed native-response Arc ownership, memory-only conversion, async
result/audit staging, tenant isolation, existing snapshots/mutation semantics,
provider refusal/retry behavior and exact versioned policy identity. Add no
dependencies, migrations, storage variants, authorization mechanism, benchmark
framework, AllTransform or Google behavior. Do not request historical RED or
production churn to repair the recorded TDD history. Implementation and another
review remain caller-initiated.
