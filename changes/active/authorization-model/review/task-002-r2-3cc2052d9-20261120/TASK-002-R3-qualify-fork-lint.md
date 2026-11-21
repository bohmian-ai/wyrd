---
id: TASK-002-R3
kind: remediation
status: proposed
spec: SPEC-authorization-model
spec_revision: 2
requirements: [REQ-006]
original_task: TASK-002
findings: [FIND-TASK-002-4]
---

# Qualify the production codec owner's lint gate

The produced-review journey and peer Variant proof now satisfy their behavioral
closure scenarios. The remaining gap is the production DataFusion fork's
mandatory lint gate. Finish that qualification without reopening accepted Access
or Oracle behavior.

## Subject and diagnosed gap

Approved contract: supplied working-tree SPEC-authorization-model revision 2,
`changes/active/authorization-model/spec.md`; original task:
`changes/active/authorization-model/tasks/TASK-002-access-ui.md`.
Reviewed base: `ff42598100b0edabad5cad2272a3c7735f2eef14`.
Reviewed candidate: `3cc2052d977c94f52a11548f887ea548c2d6f297`.
Prior remediation:
`../task-002-r1-ff4259810-20261120/TASK-002-R2-prove-produced-reviews-and-peer-variants.md`.
Validated finding: FIND-TASK-002-4 in [findings-validation.md](findings-validation.md).

REQ-006's peer correction lives in the existing DataFusion physical cast codec,
fork revision `1a0f1af0bc6ce686e5fd25aaee000bd88f6ca17a`, pinned by this Wyrd
candidate. R2 requires affected-owner format/lint checks. The fork's `AGENTS.md`,
Before Committing, explicitly requires `cargo fmt --all` and
`cargo clippy --all-targets --all-features -- -D warnings` to pass. The supplied
implementation records no clean fork clippy run. Wyrd's clean lint result does
not qualify the dependency owner's own code. This is a missing mandatory-gate
result, not an assertion that the cast repair is wrong.

## Smallest correction boundary

Obtain a clean result for the exact pinned fork revision using the existing
owner commands, through `mise exec --`, sequentially with other Cargo work:

```bash
mise exec -- cargo fmt --all -- --check
mise exec -- cargo clippy --all-targets --all-features -- -D warnings
```

Run in the fork checkout, recording commit identity, exact command, exit status
and actual diagnostics. Preserve the source immutable while qualifying it.
Formatting check establishes the required formatting property without mutating
the reviewed revision. Do not treat a narrower package lint or Wyrd dependency
build as the required fork lint result.

If the command demonstrates failures, diagnose them before proposing a code
correction. Correct only what is necessary at its existing owner; do not add
allowances, weaken gates or waive failures as pre-existing. If a corrected fork
revision is necessary, update Wyrd's existing dependency pins/lockfile and
`ORACLE_DATAFUSION_REVISION` together, preserving fingerprint refusal for an
incompatible peer. Return the changed candidate and affected focused evidence
for maintainer-initiated review. Evidence-only closure requires no new code.

## Observable closure

| Finding / authority | Required observation | Narrow proof |
| --- | --- | --- |
| FIND-TASK-002-4 / R2 affected-owner checks and fork Before Committing | The exact pinned production codec owner's required lint completes successfully with no warnings, and formatting is clean | Existing fork fmt check and exact all-target/all-feature clippy command above, with revision-bound execution evidence<br>**Evidence.** Fork checkout `~/Documents/GitHub/datafusion-scalar-snapshot`, branch `wyrd-production`, HEAD `1a0f1af0bc6ce686e5fd25aaee000bd88f6ca17a`, clean tree before and after. `mise exec rust@1.98.1 cmake@3.26.4 -- cargo fmt --all -- --check`: exit 0, no output. `mise exec rust@1.98.1 cmake@3.26.4 -- cargo clippy --all-targets --all-features -- -D warnings` (clippy 0.1.98): exit 0, no warnings, `Finished dev profile`. **Mechanical deviation:** the tool versions are named explicitly. The bare `mise exec --` form, run from the Wyrd session, exited 101 before linting: the shell's inherited `RUSTUP_TOOLCHAIN=1.99.0` overrode the fork's `rust-toolchain.toml` pin (1.98.1), and `snmalloc-sys` could not build because no `cmake` version is selected in the fork directory. Both are tool selection, not source; the lint flags and scope are unchanged. No file changed in either repository. **Behavior: PASS. TDD: NOT_APPLICABLE** (evidence only). |
| REQ-006, if correction changes the fork pin | Existing peer proof still preserves whole Variant Field metadata and values, key read/filter results and row exclusions; pin/fingerprint remain aligned | Existing focused fork cast/codec tests and `mise exec -- cargo nextest run --locked -p vala-bifrost-redux --lib -E 'test(=oracle::codec::tests::a_masked_verification_plan_executes_identically_on_a_peer)'`, plus affected owner checks<br>**Not triggered:** the fork revision and the Wyrd pin are unchanged, so the R2 peer proof stands as recorded. |

Preserve closed FIND-TASK-002-1/-2/-3 and their existing evidence. No new journey,
account fixture, permission model, migration, API, codec wrapper or full integrated
suite is required. Benchmark qualification remains expressly excluded. Do not
reconstruct historical RED or claim a later clean lint rewrites precommit/TDD
history. Record any new external waits if code changes; none are expected for
evidence-only qualification. Return evidence to the caller; no implementation of
another task, automatic review, commit, push, merge or deployment is authorized.
