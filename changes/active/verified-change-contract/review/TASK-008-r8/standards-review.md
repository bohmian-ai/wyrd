# Repository standards review

## Immutable subject and scope

- Candidate: `c4bc77a5c877c508191dc606b3cd3bb78047dc29`
- Remediation range: `ea0ed46fa..c4bc77a5c877c508191dc606b3cd3bb78047dc29`
- Approved authority: `changes/active/verified-change-contract/spec.md`, revision 57
- Original task: `changes/active/verified-change-contract/tasks/task-008-closeout.md`
- Prior finding and remediation: `FIND-TASK-008-CLOSEOUT-17` and
  `review/TASK-008-r7/TASK-008-CLOSEOUT-R6-close-final-audit-handoff.md`
- User-directed scope: closure of `FIND-TASK-008-CLOSEOUT-17` and regressions
  introduced by this range. Earlier passed code was used only to trace owners
  and consumers. `FIND-TASK-008-CLOSEOUT-13` and the full default
  `bench:capacity` qualification remain deferred.

The candidate remained at the stated commit during this review. `.codegraph/`
is absent, so repository navigation used the source tree directly.

## Authority coverage

| Changed surface | Governing authority inspected | Coverage |
|---|---|---|
| `Queue::poll` in `crates/wyrd/wyrd-testing/src/bin/capacity/evidence.rs` | `AGENTS.md` §§4–6, 11–12, 15–16; `architecture/agent-rules.md` Rust structure, documentation, async, SQL-boundary, audit, and test-integrity rules; `architecture/references/architecture/patterns.md` canonical Rust composition and audit pattern; `architecture/references/languages/rust-core.md`; `architecture/references/domain/analytical-operations-reliability.md`; revision-57 `REQ-171`; `architecture/bifrost-design.md` read-audit/publication boundary | Complete for the materially changed Rust workflow |
| Expanded Postgres/live-server handoff proof in `capacity/evidence.rs` | `AGENTS.md` §§11 and 16; `architecture/agent-rules.md` Postgres-test placement, exact-test command, no gate circumvention, and rustdoc rules; `architecture/references/languages/testing-workflows.md`; `TESTING.md`; R6 `AC-R6-2` and `AC-R6-3` | Complete for the changed test body and its owning lane |
| R6 remediation front matter and appended implementation evidence | `AGENTS.md` §§12 and 14; `architecture/references/languages/spec-driven-development.md` task lifecycle and command precision; `architecture/references/languages/implementation-execution.md` execution record, verification, and diff audit; `.agents/skills/wyrd-implement/SKILL.md` implementation handoff | Complete for the changed task artifact |
| Audit pending-to-staging handoff consumed by the benchmark | `AGENTS.md` audit decisions; `architecture/agent-rules.md` one audit path/non-blocking audit rules; `architecture/wyrd-security-posture.md` audit integrity; `architecture/bifrost-design.md` read audit and terminal contract; producer `wyrd-server/src/oracle/query_audit.rs`, caller `capacity/step.rs::Deployment::drain` | Complete enough to establish that the range preserves the governing audit boundary rather than redefining it |

No Python, TypeScript, public contract, generated artifact, manifest,
dependency, feature, schema, migration, production audit writer, publisher,
timeout, SLO, or report surface changed in the remediation range.

## Applicable rule results

| Applicable rule | Source evidence | Result |
|---|---|---|
| Materially changed stateful Rust behavior remains on its concrete owner (`AGENTS.md` §5; `agent-rules.md`; `rust-core.md`) | `Queue::poll` remains an inherent method on the dependency-owning `Queue`; `Deployment::drain` remains the repetition/deadline owner. No helper, trait, dependency, or alternate owner was added (`evidence.rs:211-353`; `step.rs:383-423`). | PASS |
| Async is limited to real IO composition (`AGENTS.md` §6; `rust-core.md`) | The added second `Queue::backlog` call directly awaits PostgreSQL IO. The test awaits the public Oracle request, database lock/commit, metrics handoff, and publisher. | PASS |
| Every materially modified Rust item has substantive rustdoc, errors, panics, and cancellation/partial-progress notes where applicable (`AGENTS.md` §16; `agent-rules.md`) | `Queue::poll` documents the two scrape/query pairs, why zero is accepted, its error cases, and cancellation behavior (`evidence.rs:307-331`). The expanded test documents the new crossed-commit phase plus `# Errors` and `# Panics` (`evidence.rs:557-590`). | PASS |
| Audit remains non-blocking and uses the single canonical staging/publisher path (`AGENTS.md`; `agent-rules.md`; `wyrd-security-posture.md`; `bifrost-design.md`) | The range changes only benchmark observation and its proof. The producer still commits staging before decrementing `audit_outbox_pending` (`oracle/query_audit.rs:155-215`), and the proof continues through the existing `AuditPublisher`. No audit semantics or persistent owner changed. | PASS |
| A terminating benchmark poll must not accept a false zero during the pending-to-staging handoff (revision-57 `REQ-171`; R6 remediation) | The empty path is now source-ordered `S1 -> Q1 -> S2 -> Q2`; the returned value combines `S2` with `Q2` (`evidence.rs:339-352`). The expanded public Oracle/Postgres proof releases the held commit during `S2`, waits for pending to reach zero, and proves `Q2` observes the unpublished staging row until publication (`evidence.rs:725-787`). This closes the repository-standards aspect of `FIND-TASK-008-CLOSEOUT-17`. | PASS |
| Postgres/live-server behavior stays behind the environment gate and uses repository-managed proof (`AGENTS.md` §11; `agent-rules.md`; `testing-workflows.md`) | The expanded proof remains under `mod pg_tests`, retains its pre-existing descriptive `#[ignore]`, starts `WyrdTestServer`, uses public `Bifrost`, and is recorded with the Postgres wrapper and exact nextest selector (`evidence.rs:470-790`; R6 task evidence). No test was newly disabled or weakened. | PASS |
| Verification evidence must use exact selectors, the owning environment, relevant broader lanes, formatting/lints, and diff audit (`AGENTS.md` §§11–12; `spec-driven-development.md`; `implementation-execution.md`) | The appended record names the exact Postgres handoff selector, two exact adjacent selectors, the full capacity target with ignored tests, fixed-port pair, release-server selection, server journey, formatting, lints, and `git diff --check`. This review independently confirmed `cargo fmt --all -- --check` and range `git diff --check` pass. The explicitly deferred full default benchmark is not claimed. | PASS |
| An implemented task handed to independent review uses the defined `review` lifecycle state (`AGENTS.md` §14; `spec-driven-development.md`, Task contract and Review and remediation) | The changed R6 front matter uses `status: implemented` even though the defined task states are `proposed`, `ready`, `in_progress`, `review`, `approved`, and `superseded`, and the appended evidence routes this implementation to review (`TASK-008-CLOSEOUT-R6-close-final-audit-handoff.md:1-14,164-213`). | **FAIL — `STD-R8-001`** |
| The range introduces no unrelated executable, contract, dependency, or generated-artifact drift | The executable diff is limited to the second durable read and its focused proof; the other changed file is the required remediation execution record. `git diff --check` is clean. | PASS apart from `STD-R8-001` |

## Material finding

### `STD-R8-001` — VIOLATION — remediation task uses an undefined lifecycle state

- **Violated rule:** `AGENTS.md` §14 binds active Wyrd work to the
  spec-driven workflow. `architecture/references/languages/spec-driven-development.md`
  defines task states as `proposed`, `ready`, `in_progress`, `review`,
  `approved`, and `superseded`; completed implementation awaiting independent
  task review is `review`. `$wyrd-implement` returns `IMPLEMENTED` as its
  execution result, but that result is not an additional task-front-matter
  state.
- **Location:**
  `changes/active/verified-change-contract/review/TASK-008-r7/TASK-008-CLOSEOUT-R6-close-final-audit-handoff.md:4`.
- **Evidence:** this range changes the header from `status: ready` to
  `status: implemented`, appends completed implementation and verification
  evidence, and submits the immutable candidate to `$wyrd-task-review`.
- **Consequence:** the active task packet no longer uses the repository's
  closed lifecycle vocabulary, so workflow consumers and later completion
  review cannot interpret the remediation state under the authoritative task
  contract.
- **Testable correction:** change only this remediation header to
  `status: review`; retain the implementation evidence and all Rust/test
  changes. Static inspection plus `git diff --check` closes the finding.

No other material repository-rule finding was identified in the user-directed
range. In particular, this review does not reopen earlier accepted source and
does not treat deferred `FIND-TASK-008-CLOSEOUT-13` as a failure.

## Overall result

**FAIL**

The Rust/test remediation conforms to the applicable repository rules and the
range closes the standards dimension of `FIND-TASK-008-CLOSEOUT-17` without an
identified executable regression. The range nevertheless introduces one
bounded workflow-rule violation, `STD-R8-001`, in the changed remediation task
metadata.
