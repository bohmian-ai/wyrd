# Task 004 second remediation closure review

## Immutable subject and authority

Wyrd root: `/home/thorrester/Documents/GitHub/wyrd-worktrees/github-verifier-pipelines-task-004`.
Original base: `81aca5479145b2b94d5db4e0d15816d82cf40bc7`.
Prior candidate: `b5c0db2e01268d6ad033fbe7450069247edff6ee`.
Candidate: `dd2cb513ac8e4aee1316788335f33d2ca84f9393`.
Incremental range: `git diff b5c0db2e0 dd2cb513a`; full cumulative range available.
Library root: `/home/thorrester/Documents/GitHub/bohmian-code-review`.
Library remains `9046d1035538df8dfb661f826b102316ee6d0b4e`, no delta.
Original library base: `d8c772de4248ce0130f79ec3b5fefda518b1431a`.

Approved spec: `changes/active/github-verifier-pipelines/spec.md`, revision 6,
explicitly approved by the user; stale markers do not reopen approval.
Original task: same packet `tasks/004-pinned-pr-preparation.md`.
Prior verdict/ledger/remediation: `review/004-pinned-pr-preparation-r2/`:
verdict.md, findings-validation.md and 004-R2-fix-required-documentation-links.md.
FIND-004-1 closed in r2; FIND-004-2 is the sole open prior finding.
Keep source immutable. Write only this new report directory, no diffs/archives.

## Navigation and evidence

The only new source change is five module-doc lines in
`crates/wyrd-spec/src/github_connection.rs`: a blank line plus four reference
link definitions to the existing public types in crate::github_connection.
Read the complete module-doc block, corresponding four types, lib.rs public
module declaration and mise.toml check:docs runner. Other delta paths are
committed r2 review/evidence records. No wire, runtime, SQL, tests, gate flags,
dependencies or library source change is reported; verify from the actual diff.

## Specialist applicability and carry-forward

| Contract / invariant | Selected or carried evidence |
|---|---|
| FIND-004-2 / R2-1 link resolution and R2-2 bounded diff | One fresh ponytail-rev checks actual destinations, unchanged public target identity, gate result, scope and both prior closure IDs |
| Full Task004 L1-L15/W1-W9/N1/D1/D2, REQ-006/007/AC-004 and invariants | Carry the source/assertion matrices in r1's seven discovery reports and approved validator, reconciled by r2 validation; no executable or dependency change invalidates them |
| Mandatory Rust documentation rules and gates | Prior r2 standards report plus focused fresh validation/gate run; no materially new cross-tier change requires a separate standards reviewer |
| SQL persistence/tenancy/CAS/security/system/ownership | Prior data/security/invariant/system/maintainer reports, plus r2 targeted context-removal assessment; no new evidence or code delta requires renewed specialist discovery |
| UI/Python/TS/public SDK/model runtime | Inapplicable, unchanged surfaces |

Independent fresh validation must check this applicability and evidence, not
merely assume an empty ledger. If it finds a concrete coverage gap, request
targeted discovery; do not repeat the full first-round team. Any newly discovered
blocker needs separate source validation before retention. No intended verdict.
No followup discovery is selected absent a material conflict; prior rejected
runtime proposals stay rejected unless new evidence meets the reopening rule.
The two-unsuccessful-remediation threshold has not been reached; no escalation
or new expectations are warranted solely because this is the next review folder.

Known limits carry: real GCS cloud lane unrun, four nonfatal private-link
warnings are explicitly outside this correction; cleanup primitive lost-key and
collection caller preconditions remain documented limitations, not new findings.
