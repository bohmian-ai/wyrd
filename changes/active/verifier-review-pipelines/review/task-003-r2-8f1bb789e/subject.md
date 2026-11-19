# TASK-003 R2 immutable review subject

Repository `/home/thorrester/Documents/GitHub/wyrd-worktrees/code-review-workflow`.
Prior candidate `f74a576348cb9805ec7a2ef0e1c0bde3dd07928d`.
Candidate `8f1bb789e4d1ad4f481a9cefba2a0eddd82b7ef9`.
Original task base `e6fe5fbdd1905c6627d6a95bb3042a9cfda4d355`.
Review diff `git diff f74a576348cb9805ec7a2ef0e1c0bde3dd07928d 8f1bb789e4d1ad4f481a9cefba2a0eddd82b7ef9`.
One linear commit; three Python test files; 35 additions/38 deletions.
Cumulative original task range remains available at the base/candidate above.

## Immutable authorities and context

Read .agents/skills/wyrd-task-review/SKILL.md, root AGENTS, SDK-local Python
AGENTS, spec-driven-development, maintainer style and applicable testing rules.
CodeGraph first because `.codegraph/` exists; fall back to targeted source if
Python symbols are not indexed. All commands through mise. The overall spec
remains draft only for unrelated TASK007 policy; approved baseline18 plus
approved amendments including22 governs this task.

Original task: ../../tasks/TASK-003-generic-orchestration.md.
Initial verdict/ledger: ../task-003-384a0def7/{verdict,findings-validation}.md.
Prior R1 verdict/ledger and focused assessment:
../task-003-r1-f74a57634/{verdict,findings-validation,followup-review}.md.
R2 task/evidence: ../task-003-r1-f74a57634/TASK-003-R2-python-provider-hold-ownership.md.
Uncommitted authority/evidence bytes pinned in authority-sha256.txt.
Keep implementation, authorities, prior reports and HEAD unchanged. Only current
reports may be written in this directory; no source/diff archives. Pre-existing
dirty skills and untracked software-factory packet are outside scope. Verify
HEAD/source and fingerprints at conclusion; a changed candidate blocks review.

## Round scope and navigation

Second remediation review. Prior validated FIND-003-1..4 are CLOSED; carry those
source/assertion-backed rows and unaffected original acceptance, checking this
diff/dependencies for invalidation. Review only FIND-003-5 closure and changed
Python provider paths. Do not restart original engine/Rust/MCP/TS/state/security
discovery. Prior acceptance is substantive source evidence in original/R1
reports, not a permission to ignore changed dependencies.

| Obligation / changed owner | Producers, consumer and proof to inspect |
|---|---|
| R2.1 no new mutable global | support.py HOLDS deletion, conftest/test imports deletion; only new hold state is per-Receiver default_factory field |
| R2.2 same fixture owns test and handler state | Receiver.hold/release/await_release; serving closure calls await_release on its own received instance; session fixture yields it; test receiver hold/release calls |
| R2.3 ordinary/sibling endpoints | serving/delayed serving/watching, gateway upstream and independent Receiver construction; default empty instance gates; new await placement applies to each handler but no global lookup |
| R2.4 preserved public proof | Six SPECULATED rows, public invocation milestones, observed provider arrival, final status/call/manifest/retained/read-authority assertions |
| R2.5 wake/cleanup | Event captured in await_release snapshot, release pop+set, successful decision case observes arrived then releases and completes; finally uses same release. No separate forced-failure/parallel test requirement. |
| R2.6 scope and rules | Only support.py, conftest.py, test_verifier_orchestration.py. No production/native/public contracts/other languages/deps/old Event changes. Python top-level tests, typed existing dataclass owner, appropriate local docs. |

This map is navigation, not coverage or conclusions. Verify all relevant bodies
and callers. User completion summary is not proof; R2's evidence table supplies
claims/commands for inspection. Docstring wrapping after supplied runtime run
is source-inspected; orchestrator reruns focused selector at exact candidate.

## Specialist applicability recorded before discovery

- Select one fresh remediation ponytail closure reviewer, owning
  remediation-review.md. Its narrow scope includes Python rules, every R2
  obligation, producer/threaded consumer/sibling fixture lifetime, unchanged
  behavioral assertions and prior closure carry-forward.
- Select a separate fresh final ponytail validator after closure review is
  complete, owning findings-validation.md. It independently validates source,
  materiality, role/carry-forward coverage and empty or nonempty ledger. Any new
  blocker discovered by it needs separate focused source validation.
- Additional repo-rev omitted: three files in one test-only Python owner, no
  broad cross-tier or language change. Default closure/validator inspect local
  no-global-state rule and all touched Python docs/types/tests. R1 standards
  and focused Python authority assessment carry unaffected coverage.
- Concurrency/invariant/persistence/security/system specialists omitted: no
  production permits/budgets/SQL/accepted state/grants/WAL/deployment change.
  Existing Event matching/wait/set semantics move to the existing fixture owner;
  no new concurrency policy or coordination protocol is introduced. Closure
  reviewer traces actual handler placement and sibling effects; prior original
  and R1 domain/system source evidence carries unchanged boundaries.
- Maintainer/provider/UI specialists omitted: existing Receiver extended, old
  standalone global helpers deleted, no new abstraction/transport/provider wire
  protocol/UI. Complete fixture reuse/owner check stays in default reviewer.

Validator independently assesses these choices and obtains any missing required
review, without recreating the first-round team or expanding expectations.
No intended verdict is supplied. Only orchestrator runs tests sequentially;
reviewers request exact commands if needed and do not run Cargo or mutate code.
No synthetic host load, parallel-test hardening, teardown redesign, old-global
cleanup, worker/lease/replica APIs or all-language timing matrix is authorized.
