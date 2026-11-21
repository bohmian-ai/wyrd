# TASK-001-R2 review subject

- Repository: `/home/thorrester/Documents/GitHub/wyrd`.
- Immutable candidate/initial HEAD: `300542877cd2a58cbe5f061d5075616e4cc4397b`.
- Prior candidate/remediation base: `284f801159a391e92493c3072878fd644c9d173b`.
- Original reviewed candidate: `c78ad7b5241fbbfb54e85e2390c6a694787debfc`.
- Cumulative base: `eeb3bbd4f891ae88b00ce1979810eb2a11d04871`.
- Approved contract: `SPEC-authorization-model`, revision 2, `changes/active/authorization-model/spec.md`.
- Original task: `tasks/TASK-001-scoped-authorization.md` in that packet.
- Prior review: `review/task-001-r1-284f80115-20261120/`, including R2 task and updated implementation evidence.
- Latest diff: `git diff 284f80115 300542877`, four commits. Cumulative source remains available; this range is a descendant, with no rebase correspondence needed.

Existing uncommitted design/spec/task edits and untracked prior review reports are supplied context. Preserve them. Implementation source and HEAD must remain immutable. Only review reports and a necessary remediation task belong in this new directory.

## Review navigation and ownership

One fresh `r2_closure_review` independently reviews the five outstanding findings and affected paths using prior validated evidence. It is the sole Cargo runner. New blockers require a separate fresh source validator before retention; add specialists for a material unresolved boundary, not another vote on unchanged coverage. The orchestrator assembles reports and does not validate its own findings.

| Boundary | Starting points and consumers |
| --- | --- |
| Authority lifecycle/root-cause correction | `wyrd-auth/src/authority.rs`: observation, generation, load/install/confirm, shared usability; server request and stream acquisition; cache/PG tests |
| Language approval semantics | Builtin operations and binding compiler, bootstrap fixture, retained approval parent checks; Python/TS deferred-evidence journeys and corresponding Rust proof |
| CLI access administration | CLI access command dispatcher, new real-server Card lifecycle journey; role/group/binding consumers and target read/refusal |
| Checked-in authoring inputs | Rust principal journeys, public Oracle peer journey, four fixture files and fixture documentation |
| Current onboarding guidance | Add-people and grant guides; generated served documentation; docs commands/checks |

## Constraints

The maintainer requires a root-cause correction in the shared authority owner and removal of superseded validity logic. Preserve existing auth/attribution semantics and closed findings; no new product authority is created by this review. Independently check the implementer's challenge to FIND-11's prior premise; correct the review record if its original source diagnosis was wrong.

Carry forward the explicit exclusions of benchmark qualification and TDD compliance reporting. Runtime proof, wait accounting and repository invariants remain in scope. TASK-002 UI and final integrated change review are outside this task review. No implementation, commits, pushes, deployment or automatic next review is authorized here.
