# Scoped repository standards review — Task 004 remediation

Result: **FAIL**.

## Immutable subject and scope

Wyrd `81aca5479145b2b94d5db4e0d15816d82cf40bc7` →
`b5c0db2e01268d6ad033fbe7450069247edff6ee`; remediation delta from
`359a19f41597b8cfdc673373409b0d96e8298447`. Library context:
`d8c772de4248ce0130f79ec3b5fefda518b1431a` →
`9046d1035538df8dfb661f826b102316ee6d0b4e`, prior
`d380f57fa53429eaa4f3e7272ce651e1825a8b38`.

Revision 6 approval is accepted. This specialist investigates only the newly
reported red `check:docs` gate, its governing rules, affected documentation
coverage, and smallest required correction. Prior full review evidence carries
for unchanged runtime and cross-tier rules; this is not another full audit or
independent closure review of all FIND-004-1 items. The incremental source diff
contains rustdoc changes, the library manifest/lock update and review records;
no runner or gate policy changes. The fresh closure reviewer owns substantive
changed-item documentation accuracy. No implementation source was changed.

## Authority and surface coverage

| Surface | Applicable authority | Evidence / result |
|---|---|---|
| Reported Rust documentation failure | Root AGENTS.md:188–190: never circumvent a gate; “A red gate is yours to fix whoever turned it red” | FAIL: named managed gate independently exits 101; no preexisting exemption applies |
| Documentation acceptance | Root AGENTS.md Code rustdoc rule; agent_docs/rust.md Rustdoc; remediation task's focused owning documentation proof | Changed/private/test/durable-item coverage requires source review; public rustdoc build alone is insufficient. No weakening or new checker proposed |
| Exact runner and enforced lint | mise.toml:816–824; crates/wyrd-spec/src/lib.rs:8–9 | `check:docs` runs `RUSTDOCFLAGS="-D missing_docs -D rustdoc::broken_intra_doc_links" cargo doc --locked --no-deps --all-features -p wyrd-spec -p wyrd-auth-issue -p wyrd-auth-verify`; crate itself denies broken links |
| Public module documentation links | crates/wyrd-spec/src/github_connection.rs:4–7; lib.rs:20 | FAIL: four bare link names do not resolve in this public-doc build; each target exists in the public module |
| Task-sized verification and non-expansion | TESTING.md:65–89; changes/spec-driven-development.md task/remediation scope; remediation task focused proof | Use existing documentation command; no runtime suite, broad gate aggregate, new checker or unrelated cleanup required |
| Docs-site gate distinction | mise.toml:1226–1240 and :1537 | `docs:check` generates/builds/tests/checks the docs site. It is distinct from `check:docs`; running it cannot substitute for the failing Rust-doc command |
| Owning nested rules | Root AGENTS.md and agent_docs/index.md directory authority map | No nested AGENTS governs wyrd-spec/github_connection.rs. Docs-site, Python, TypeScript and SQL directory rules are not triggered by the required link correction |

The architecture authority map was read. This correction changes only Rust
link destinations, so no product, wire, security, storage or deployment
architecture section requires a contract change. Existing target types and
wire definitions must stay unchanged.

## Source-validated finding

### REPO-R2-001 — required Rust documentation gate remains red

Classification: **VIOLATION**. Mandatory gate, not a runtime regression claim.

Locations: `crates/wyrd-spec/src/github_connection.rs:4`, `:6`, `:7`;
`mise.toml:816–824`; enforcement also at `crates/wyrd-spec/src/lib.rs:9`.

The public module introduction links `GithubConnectResponse`,
`GithubSetupQuery`, `GithubCallbackQuery` and `GithubConnectionView` using
unqualified destinations. The managed rustdoc invocation reports “no item
named … in scope” for all four. Their actual public declarations are in the
same module at lines 31, 118, 132 and 94 respectively; the module is publicly
declared at `lib.rs:20`, and these types are not root reexports. Thus the
problem is unresolved destinations in module documentation, not missing wire
contracts, unavailable dependencies, tests or runtime behavior.

Independent execution: `mise run check:docs`, exit **101**, with four fatal
unresolved-link diagnostics for exactly those names and `could not document
wyrd-spec`. The command was run after the orchestrator confirmed no other
Wyrd Cargo operation was running, respecting TESTING.md shared-target rules.
The orchestrator independently observed the same failure.

Observable consequence: Wyrd's existing managed Rust documentation gate cannot
complete on the immutable candidate. Root AGENTS.md:188–190 explicitly makes a
red gate actionable regardless of who introduced it. The remediation evidence's
claim that Task 004 leaves these links alone because they belong to another
task cannot waive that rule. The review skill likewise preserves explicit
mandatory-gate violations. This is a required correction, with no need to
reopen spec approval or audit unrelated repository debt.

Smallest safe correction: retain the four readable labels and prose, and set
their rustdoc destinations explicitly to
`crate::github_connection::GithubConnectResponse`,
`crate::github_connection::GithubSetupQuery`,
`crate::github_connection::GithubCallbackQuery`, and
`crate::github_connection::GithubConnectionView`. All targets already exist
and are public. This changes only three existing documentation lines (or their
wrapping); no added imports, root reexports, aliases, helper, dependency,
configuration, public API or runtime code is needed. A local qualified link
preserves navigation and fixes the producer of the invalid destination. Removing
the lint or excluding the module would weaken the existing gate and is not a
valid smaller correction.

Focused closure: verify the documentation-only diff, then rerun
`mise run check:docs` and require exit 0. Do not require full journeys or the
whole `gate` aggregate for this static correction. Carry this as one bundled
gate finding; do not split four links into four rounds.

## Coverage limits and non-findings

`check:docs` covers three selected public crates; it does not build the changed
server/storage documentation or integration-test helper docs. Its result does
not prove FIND-004-1 closed. Those changed-item obligations remain covered by
the fresh closure reviewer and the existing focused owner documentation proof.

The run also printed nonfatal private-link warnings for auth-issue's
`Self::sign_access_token`, human_connection's `Self::validate`, and oidc's
`SHA256_HEX_PATTERN`. They are a different lint (`private_intra_doc_links`),
not the four fatal diagnostics. The existing runner does not deny that lint;
this scoped review proposes no warning sweep or stronger gate policy. No
source mutation, formatter, runtime test, or broad re-audit was performed.
