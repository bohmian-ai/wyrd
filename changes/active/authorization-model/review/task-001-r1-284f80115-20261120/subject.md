# TASK-001-R1 review subject

- Repository: `/home/thorrester/Documents/GitHub/wyrd`.
- Cumulative base: `eeb3bbd4f891ae88b00ce1979810eb2a11d04871`.
- Prior reviewed candidate: `c78ad7b5241fbbfb54e85e2390c6a694787debfc`.
- Remediation base (prior review-time HEAD): `3139c08cb0b7c0194583a66af3ab31bef28dc14f`.
- Candidate and initial worktree HEAD: `284f801159a391e92493c3072878fd644c9d173b`.
- Approved contract: `SPEC-authorization-model`, revision 2, `changes/active/authorization-model/spec.md`.
- Original task: `tasks/TASK-001-scoped-authorization.md` in that packet.
- Prior review and remediation: `review/task-001-c78ad7b52-20261120/`, including `TASK-001-R1-scoped-authorization-corrections.md`.

Review reads `git diff 3139c08cb 284f80115` and retains the cumulative range as context. `git diff --quiet c78ad7b52 3139c08cb -- crates sdks scripts examples mise.toml docs` succeeded: the later governing-document commits did not alter those implementation surfaces. No rebased patch correspondence is necessary.

The packet has existing uncommitted documentation/evidence changes, and the prior review directory is untracked. These are supplied review inputs, not permission to commit. The candidate's implementation must remain unchanged during review. Only new review reports and a remediation task may be written here.

## Navigation and assignment

One fresh remediation reviewer, `r1_closure`, checks closure of all ten stable findings and affected callers against the approved requirements and prior validated evidence. It owns focused executable checks; Cargo runs must not overlap. New blockers require a separate fresh source validator. The orchestrator assembles the verdict and does not validate its own findings.

| Boundary | Navigation starting points |
| --- | --- |
| Observed authority and repeated acquisition | `wyrd-auth/src/authority.rs`; auth SQL snapshot; request/stream consumers; authority unit/PG and replica tests |
| Provisioning and scope targets | `wyrd-auth/src/roles.rs`; principals routes/access; `vala-sql` catalog target lookup; principal/browser integration tests |
| Scoped Card mutation and retained reads | Card service/routes; storage routes; verification inspection; Rust principal and durable execution journeys |
| MCP and served guidance | Server MCP principals/catalog/guide; MCP client journeys; identity/admin/verification guides and language consumers |
| Actual peer visibility | `wyrd-testing/tests/bifrost/oracle/distributed.rs`; existing peer fixture; retained-parent producers and Oracle enforcement |

The reviewer assesses whether additional expertise is necessary for material new boundaries. This is focused remediation review, not renewed discovery across unchanged TASK-001 behavior.

## Explicit review constraints

The maintainer removed benchmark obligations from this packet; do not run or require them. Preserve the prior explicit exclusion of TDD compliance reporting. Functional evidence, scoped hot-path wait accounting, tenant separation, and attribution remain required. TASK-002 UI implementation is outside this review. The four open notes supplied in the re-review request must be assessed against existing approved obligations, not treated as new requirements automatically.

Review does not implement, commit, push, deploy, or launch the next workflow phase.
