---
id: TASK-001-R2
kind: remediation
status: proposed
spec: SPEC-agent-prompt
spec_revision: 2
parent_task: TASK-001
remediates: [FIND-TASK-001-5]
depends_on: []
---

# Complete the two modified SQL function contracts

## Subject and authority

Use `$wyrd-implement` directly. This is documentation-only closure of the one
remaining validated finding; no behavior change or new test is required.

- Approved [spec r2](../../spec.md) and original [TASK-001](../../tasks/TASK-001-agent-prompt.md).
- Prior [R1](../TASK-001-02c06e21b-20261015/TASK-001-R1-registration-and-public-type-closure.md).
- Cumulative base: `c1aac2d132b19848ca1940c3907e1fa68a8d08a5`.
- Reviewed candidate: `a21a804bc56f039f8a407f6a5b9c4f95905bf3f0`.
- [Verdict](verdict.md) and [independent validation](findings-validation.md).

## Diagnosis and correction

FIND-TASK-001-5 remains open only in
`crates/wyrd/wyrd-sql/src/queries/cards/version_resolve.rs`. R1 fixed the
original field and six HTTP documentation omissions, but materially changed
these SQL functions without completing their documentation. AGENTS.md's Rust
documentation rule and agent rules explicitly require substantive rustdoc,
`# Errors` on fallible functions and `# Panics` for remaining panic conditions.
Green format/lint checks do not replace that source requirement.

At line 49, `latest_stable_in_line` has ordinary comments rather than rustdoc.
Document its read-only latest-stable selection on the caller-owned tenant
transaction: Active plus current-operation Pending eligibility, same Card line,
optional bounds, and bounds-generation/database/stored-version parsing errors.
Explain that cancellation publishes nothing and does not commit the transaction.

At line 101, public `resolve_version` has an Errors section but no Panics
contract for `Some(VersionSpec::Pin(_))` reaching `unreachable!` at line 113.
Document its auto/scope-only precondition and this panic. The sole production
caller already handles exact pins separately; no registration panic is alleged
and no new runtime guard, error, type or refactor is warranted.

Read the two full bodies and `service.rs::resolve_existing` to align the prose
with actual behavior. Add only these missing contracts; preserve the existing
Errors documentation, ordinary timeout comment, signatures and implementation.

## Scope and acceptance

- Only these two functions' rustdoc changes; SQL, eligibility, locking,
  version/hash/artifact arithmetic, transaction ownership and lifecycle stay intact.
- Both item contracts accurately describe their current operation and failure
  assumptions; the private helper has substantive rustdoc/Errors and the public
  resolver has the exact-pin Panics section.
- FIND-2/-3/-4 remain closed; original FIND-5 sites remain closed.
- FIND-1 remains owner-accepted risk: no lock, recheck, concurrency test or
  deletion-policy change. No unrelated documentation cleanup or new machinery.

## Focused proof and handoff

Static comparison of docs with the two bodies and sole production caller is
the proof. TDD/runtime tests are inapplicable to these comment-only additions.
Run `mise run fmt` and `git diff --check`; preserve the existing normal lint
gate. Do not add a strict workspace cargo-doc lane, rerun registration/journey
suites solely for prose, or repair unrelated private intra-doc links here.
Record the bounded diff and documentation proof against FIND-TASK-001-5.

If executable code changes prove necessary, stop and explain that different
scope rather than expanding this documentation correction. Later review keeps
the original cumulative base, R1, this task, prior findings and accepted risk.

## Implementation Evidence

| Acceptance criterion | Implementation evidence | Verification evidence | Result |
|---|---|---|---|
| `latest_stable_in_line` has substantive rustdoc and `# Errors` (eligibility, line, bounds, transaction/cancellation, error codes) | `version_resolve.rs::latest_stable_in_line` doc; timeout comment kept | static comparison with body (status/operation predicate, `push_bounds` BIGINT error, `VersionBlock::parse`, 503 map) | PASS |
| `resolve_version` documents exact-pin `# Panics` | `version_resolve.rs::resolve_version` doc; existing `# Errors` kept | static comparison with `unreachable!` arm and sole caller `service.rs::resolve_existing` (pins go to `find_card_by_ref`) | PASS |
| Docs only; FIND-1 accepted risk untouched | diff limited to two doc blocks | `mise run fmt`, `cargo clippy -p wyrd-sql --all-features`, `git diff --check` | PASS |
