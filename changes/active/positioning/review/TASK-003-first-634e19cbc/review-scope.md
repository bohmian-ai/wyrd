# TASK-003 first review

Repository: `/Users/stevenforrester/Documents/GitHub/wyrd`.
Base: `5775c3795ebd6557d080b34d55dd2a67b8dbad63`.
Candidate: `634e19cbc517293ed2458f30ce73401714f3447d`.
Diff: `git diff 5775c3795...634e19cbc` (complete first-review range).
Approved authority: `changes/active/positioning/spec.md`, revision 5, committed at candidate.
Original task: `changes/active/positioning/tasks/TASK-003-runnable-verification-tutorials.md`, working-tree revision 5, pinned Git content hash `6846032793910170e76c0cc0ec9de2f50cb6ad6f`, SHA-256 `9299f0945b3a0ae1e8f2c3641d3bd44255f41b3338ca1e6381af3e61b02b6fb9`. The content hash was computed without writing a Git object; read the working file and verify its hash, rather than using `git show` on that hash.
The user explicitly identified the uncommitted task revision and appended evidence; this read-only content is the review input. The committed task still references revision 4. No staging, commit, or source edits are authorized. Recheck HEAD and task hash before verdict; a change invalidates the subject.

## Round and evidence

First review: full task scope; no prior TASK-003 review or acceptance carried forward.
Verification claims and selector repairs are appended under Implementation Evidence in the pinned task. They are claims to assess, not a verdict. Reviewers must distinguish reported runs from independently executed checks. Source is read at the recorded commits; unrelated dirty positioning/mockup/TASK-004 files and verification-model packet are excluded. No `.codegraph/` exists, so ordinary repository navigation applies.

## Starting navigation map

| Outcome / contract | Producers and owners | Consumers and proof |
|---|---|---|
| Verdict precedes release, all non-pass/error holds | `examples/finance-release/api.{py,rs,ts}`, existing finance declarations and provider harness | request tutorial; `examples/tests/finance_release.rs`, Python `test_docsite_quickstart.py`, TS `finance-release.test.ts` |
| Actual work and sufficient original trace | `examples/refund-agent/refund_agent.{py,rs,ts}`, public telemetry, WyrdState, trace check owners | agent tutorial; `examples/tests/refund_agent.rs`, Python `test_verifier_building_blocks.py`, TS `refund-agent.test.ts`; compare existing support-agent/building-block workflows |
| Server schedule, native window judgments | `examples/scheduled-check/checkout/*.yaml`, `record.*`, existing Trigger/binding/scheduler and Bifrost submit | `results.*`, schedule tutorial; `examples/tests/scheduled_check.rs`, Python `test_scheduled_drift_alerts_operator.py`, TS `scheduled-check.test.ts` |
| Attributable inspection, visibility and tenant refusal | `examples/refund-agent/investigate.*`, existing Execution details and Bifrost results | investigation tutorial and above refund-agent journeys; real unknown/foreign-tenant reads |
| Published procedures, canonical routes and disposition | four verify guides, verifier reference, response draft link, reset dispositions | CodeFromFile, indexes test and generated `docs/public/llms*.txt`; docs rules and Task 4 handoff |
| Example installation and execution | `examples/Cargo.toml`, `examples/package.json`, Cargo/pnpm locks | installed SDK and existing SDK/example journey infrastructure |

This map is a starting point; each reviewer independently expands it against source and callers.

## Specialist applicability (before discovery)

| Contract or invariant | Selection | Source-backed reason |
|---|---|---|
| Full four-outcome acceptance, recovery, guide usability | behavior-rev | Mandatory first review; task scenarios 1–4 and all 47 diff paths |
| Cross-language rules, changed Rust rustdoc, manifests, generated content | repo-rev | Rust/Python/TS runnable programs, journeys, docs and manifests all materially change; root and docs/Python/TS AGENTS apply |
| New example orchestration and harness ownership/reuse | maintainer-rev | New Reader structs, subprocess execution, telemetry lifecycle, polling and examples must be compared with existing support-agent/finance/schedule owners; task forbids duplicate runtime/framework machinery |
| Original trace completeness, retained evidence attribution, tenant refusal and public grants | domain-rev (evidence) | INV-002/003 and scenarios 2/4 explicitly require proving actual work, native insufficiency and isolated retained reads; unchanged trace and Execution dependencies must be inspected, not assumed |
| Complex shared state/lifecycle | invariant-rev omitted | No server/SDK runtime implementation changes; examples call existing public owners. Behavior reviewer and evidence domain cover example telemetry shutdown/flush, windows and visibility. Reassess if source shows new shared lifecycle semantics |
| Material deployed failure propagation/recovery | system-rev omitted | Diff adds example programs/test journeys and publications, with no deployed server, migration, health, lease or runtime changes. Behavior reviewer covers example failures and the recorded sleep diagnosis; reassess if runtime effect is found |
| Security/RBAC implementation, persistent data/concurrency semantics | separate domain specialists omitted | No auth, tenancy implementation, SQL schema, storage or concurrency implementation change. Evidence domain explicitly covers tutorial grants and cross-tenant reads against existing owners; no changed persistent semantics |

Reports: `task-review-behavior.md`, `standards-review.md`, `maintainer-review.md`, `domain-review-evidence.md`; after discovery, fresh independent validator writes `findings-validation.md`. Follow-up only for a material unresolved path/conflict or separate validation of a validator-origin blocker.

## Relevant authorities

Root AGENTS; nested docs/Python/TS AGENTS; `changes/spec-driven-development.md`; `agent_docs/index.md`, `rust.md`, `maintainer-style.md`, applicable errors/agent-surfaces guidance; `TESTING.md`; docs PRODUCT and CONTRIBUTING. Architecture owners: wyrd-design Verifier/execution, runtime identity, Trigger and client model; bifrost-design durability/visibility and public surfaces; security-posture permission model and tenant/data isolation. Read only applicable sections.

No engine hot-path implementation is changed in the recorded diff. The review must check this boundary; benchmark requirements apply if that assessment is invalidated.

## Execution continuity

A server restart interrupted the first discovery agents before they wrote reports. HEAD and task content hash were rechecked unchanged, and the existing verification record was preserved. Fresh replacement behavior, standards, maintainer and evidence reviewers completed the assigned reports independently; no interrupted-agent conclusion is carried forward. A separate fresh ponytail validator receives the completed reports. Source and the original task remained unchanged throughout.
