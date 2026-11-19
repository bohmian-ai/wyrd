---
id: TASK-004-R2
kind: remediation
status: complete
spec: SPEC-github-verifier-pipelines
spec_revision: 6
parent_task: TASK-004
remediates: [FIND-004-2]
---

# Restore the required Rust documentation gate

## Subject and authority

Approved spec: `changes/active/github-verifier-pipelines/spec.md`, revision 6,
explicitly approved by the user; no approval prerequisite remains.
Original task: `changes/active/github-verifier-pipelines/tasks/004-pinned-pr-preparation.md`.
Previous remediation: `changes/active/github-verifier-pipelines/review/004-pinned-pr-preparation-approved/004-R1-complete-changed-item-rustdoc.md`.
Wyrd root: `/home/thorrester/Documents/GitHub/wyrd-worktrees/github-verifier-pipelines-task-004`.
Original base: `81aca5479145b2b94d5db4e0d15816d82cf40bc7`.
Prior candidate: `359a19f41597b8cfdc673373409b0d96e8298447`.
Reviewed candidate: `b5c0db2e01268d6ad033fbe7450069247edff6ee`.
Library candidate: `9046d1035538df8dfb661f826b102316ee6d0b4e` in
`/home/thorrester/Documents/GitHub/bohmian-code-review` (no library change needed).
Validated finding: FIND-004-2, discovery REPO-R2-001. FIND-004-1 is closed.

## Diagnosis and intended outcome

The managed `mise run check:docs` fails with exit 101. The four fatal errors
come from bare module-doc links in `crates/wyrd-spec/src/github_connection.rs`
at lines 4, 6 and 7: GithubConnectResponse, GithubSetupQuery,
GithubCallbackQuery and GithubConnectionView. The corresponding public types
already exist at lines 31, 118, 132 and 94, respectively, in the public
`crate::github_connection` module. No missing public contract or runtime
implementation is involved.

The existing runner (`mise.toml:816–824`) builds wyrd-spec and the auth
issue/verify crates with missing-doc and broken-link denial; wyrd-spec also
denies broken links in lib.rs. Standards discovery and fresh independent
validation each reproduced exactly this fatal failure. Root AGENTS explicitly
requires fixing a red gate whoever introduced it; attribution to Task 002 does
not waive the gate. The needed correction is one bounded documentation edit,
not an unrelated documentation sweep.

## Selected correction

Keep the readable labels and prose, and qualify only the four link destinations:

| Label | Destination |
|---|---|
| GithubConnectResponse | crate::github_connection::GithubConnectResponse |
| GithubSetupQuery | crate::github_connection::GithubSetupQuery |
| GithubCallbackQuery | crate::github_connection::GithubCallbackQuery |
| GithubConnectionView | crate::github_connection::GithubConnectionView |

Correct these links at the existing documentation producer. This preserves
navigation and reuses the actual public types; imports, root reexports, aliases,
new types/checkers, configuration or dependencies are unnecessary. Preserve
all wire definitions, runtime code, warning policies, gate scope, tests, SQL,
existing docs closure and the reviewed library pin. Do not delete or weaken
the gate. No private-link warning sweep is authorized by this correction.

The reported lost-returned-key cache-removal limitation is not a finding and
must not produce a cleanup queue, tombstone, API or lifecycle redesign here.

## Acceptance and focused proof

FIND-004-2 closes when all four module links resolve to the existing public
types and `mise run check:docs` exits 0. Inspect the diff to establish it changes
only documentation destinations/wrapping, and run `mise exec -- git diff --check`.
No new test, runtime suite, full user journey, aggregate gate, library revision
or migration is required for this static correction. Record the exact results.

Route directly to `$wyrd-implement`; review does not implement, commit or push.
The next review checks this incremental correction and FIND-004-2 closure,
carrying forward FIND-004-1 closure and unaffected technical evidence.

## Obligations

| ID | Promise (contract source) | Planned proof |
|---|---|---|
| R2-1 | The four `github_connection` module-doc links resolve to the existing public types, keeping labels and prose (Selected correction; FIND-004-2) | `mise run check:docs` exits 0 |
| R2-2 | The diff only adds link definitions for those labels; no wire, runtime, gate, test, SQL or pin change (Selected correction; Acceptance) | diff audit; `mise exec -- git diff --check` |

## Evidence

Wyrd commit `fd5b7966e` (parent `b5c0db2e0`). No library change.

| Obligation and required cases | Implementation evidence | Proof location | Verification evidence | Result |
|---|---|---|---|---|
| R2-1: four module links resolve | reference-style definitions under the existing module prose map each unchanged label to `crate::github_connection::<Type>` | `crates/wyrd-spec/src/github_connection.rs:11-14` | `mise run check:docs`: exit 0. RED: with the file restored from `b5c0db2e0`, exit 101 with `unresolved link to` `GithubConnectResponse`, `GithubSetupQuery`, `GithubCallbackQuery`, `GithubConnectionView` | PASS |
| R2-2: documentation destinations only | the diff adds one `//!` blank line and four `//!` link definitions; prose, wire types, runtime, gate scope, tests, SQL and the library pin are unchanged | `git show fd5b7966e` | `-U0` diff audit; `git diff --check b5c0db2e0..2cde86b7f`: clean; `cargo fmt --all -- --check`: clean | PASS |

## Completion

Complete following the [Task 004 PASS verdict](../004-pinned-pr-preparation-r3/verdict.md) for Wyrd candidate
`dd2cb513ac8e4aee1316788335f33d2ca84f9393` and library revision
`9046d1035538df8dfb661f826b102316ee6d0b4e`. FIND-004-1 and FIND-004-2
are closed; no findings remain.
