# TASK-002 R2: FIX_REQUIRED

Alice now reads actual produced payments review outputs while populated ledger
outputs stay hidden. Masked whole Variants retain their Field and values through
the production peer codec. Both prior behavioral findings close. The remaining
blocker is the changed production dependency owner's mandatory clean lint proof.

## Immutable subject and scope

Repository `/home/thorrester/Documents/GitHub/wyrd`, branch
`task/authorization-model/TASK-002`.
Base `ff42598100b0edabad5cad2272a3c7735f2eef14`;
candidate `3cc2052d977c94f52a11548f887ea548c2d6f297`.
Reviewed direct remediation commits `c1991bc4b` and `3cc2052d9`; cumulative
original base `300542877cd2a58cbe5f061d5075616e4cc4397b` remains available.
Ancestry is preserved. Source and supplied authority identities are recorded in
[subject.md](subject.md). Approved working-tree spec revision 2 and original
TASK-002 hashes match R1; the maintainer's dirty packet was preserved.

Scope: R2 closure of FIND-TASK-002-1/-3, affected consumers and dependency owner,
with unchanged source-backed acceptance carried from R1. Governing references:
REQ-006/AC-001, original TASK-002, R2, root AGENTS, spec-driven workflow,
maintainer style/Rust/UI rules, TESTING, security posture retained-data visibility
and peer identity, Bifrost Variant storage/SQL and distributed execution.
The external production fork's AGENTS Before Committing governs its correction.

## Independent assignments

Fresh ponytail-rev independently checked closure, connected UI producer and
browser/CLI/SDK/MCP consumers, prior acceptance, repository rules, proof and
the specialist's proposed finding in
[findings-validation.md](findings-validation.md).
Fresh Oracle specialist traced the fork production correction, fetched/pinned
identity, cast metadata/nullability, protobuf/projections, peer consumers and
fingerprint in [task-review-oracle.md](task-review-oracle.md).
Root orchestrated and assembled this record without discovering or validating
findings. The specialist discovered the only new blocker; ponytail-rev separately
source-validated it. No reviewer validated its own new finding.

Additional lifecycle/security, invariant, repository, maintainer and recovery
reviewers were omitted because those production owners are unchanged; prior
independent coverage remains applicable. The selected Oracle specialist covers
the material dependency boundary and ponytail-rev covers residual rules and the
changed journey. Both selected reports are present.

## Reconciled acceptance matrix

| Existing requirement / scenario | Source, assertions and executable evidence | Result |
| --- | --- | --- |
| FIND-TASK-002-1 / REQ-006, AC-001 produced review outputs | Alice journey `ui-pages.integration.test.ts:2653–2940`, reviewing deployment, real ci-gate bindings and public Evidence publication for both parents. Admin sees two result/evidence/output rows; Alice reads payments verdict, CI evidence value and requirement outcome. Supplied focused production journey: one passed. | PASS; closed |
| REQ-006 rows/counts/pages/point reads and connected identity | Alice sees one row per produced-data table; ledger execution/evidence predicates and later pages return none. Existing UI-created grant, device credential and browser/CLI/TS/Python/MCP consumers remain connected; independent Verifier definition access stays refused. | PASS |
| FIND-TASK-002-3 / REQ-006 and Variant wire authority | `oracle/codec.rs:755–873` compares local/production-roundtripped whole Variant Field and ArrayData, key read/filter and exact visible rows with populated hidden/unparented controls. Supplied focused test and 959 crate units pass. | PASS; closed |
| Production owner and cross-build consistency | Exact fork/fetched revision `1a0f1af0bc6ce686e5fd25aaee000bd88f6ca17a`; physical cast target metadata/nullability survives native codec, generated projections align; all Wyrd pins and fingerprint agree. Supplied fork cast 29/proto 286 pass. | PASS for behavior/source correspondence |
| R2 affected-owner checks / fork mandatory lint | Wyrd fmt/lints are supplied clean; the production fork's explicitly mandatory all-target/all-feature clippy has no clean result. Wyrd lints do not qualify dependency-owner code. | FAIL; FIND-TASK-002-4 |
| FIND-TASK-002-2 / REQ-008 and principal lifecycle | No production lifecycle/Access code changes here; R1 suspension, readable metadata, active-only issuance/authentication, grants/keys and refusal proof remains applicable. | PASS; remains closed |
| Unchanged original TASK-002 obligations | Carry forward R1's REQ-002/003/007/008/009 authentication, runtime-family, administration, GitHub separation, safe copy/current-session gain/loss and docs/projection proof within its recorded limits. Changed producer/consumer and dependency seams inspected above. | PASS within prior limits |

## Retained finding and bounded handoff

**FIND-TASK-002-4 — EVIDENCE_GAP, mandatory owner gate:** recorded once with
authority, location, trigger, missing proof, consequence and correction in
[findings-validation.md](findings-validation.md). No new behavioral defect or
data leak is asserted. The clean Wyrd lint result cannot substitute for the
explicit production fork gate. The maintainer excluded benchmarks, not lint.

One cohesive follow-up:
[TASK-002-R3-qualify-fork-lint.md](TASK-002-R3-qualify-fork-lint.md).
Obtain revision-bound clean owner lint evidence; correct/re-pin only if actual
failures require code. No historical RED reconstruction or mandatory production
churn is prescribed.

This is the second unsuccessful remediation review, but neither prior gap remains
incomplete. R2 closed both and exposed a dependency-owner qualification gap at
the production correction site. It is neither a demonstrated regression nor an
expansion of product behavior. The next scope is the single owner gate, preserving
accepted journeys and Variant behavior.

## Verification and process

Fresh review check: direct-range `git diff --check` clean. Reviewers inspected
actual assertions and revision correspondence, relying on supplied focused
execution records rather than rerunning cold Cargo builds or production journeys.
Supplied results: Alice one pass/UI check clean at `c1991bc4b`; focused peer
selector and 959 crate tests pass at the final repair; fork cast 29/proto 286;
Wyrd format/lints clean. Fork lint remains unqualified. Earlier integrated Bifrost
64/66 remains a change-review limitation, not a green result.

Added/moved hot-path waits: **PostgreSQL none; Scribe none; object storage none;
Vault none**. The production change only preserves Field metadata in memory
during existing plan serialization. No new client/pool/runtime, lock across await,
audit wait or migration. Benchmark qualification is expressly excluded by the
maintainer; no performance acceptance is claimed. The production codec in-process
proof covers the identified defect; no real multi-server journey was run or claimed.

Behavioral closure of FIND-TASK-002-1/-3 is **PASS**; overall task acceptance is
**FIX_REQUIRED** for FIND-TASK-002-4. TDD remains separate: original Alice
test-first violation and unverified original RED history persist. R2 Alice is an
evidence-only correction; peer RED/GREEN is credible supplied execution evidence,
not independently witnessed history. Later acceptance cannot erase process history.

Skipped runtime reruns, broad suites, screenshots and benchmarks. No source or
maintainer authority changes, commits, push, merge or deployment. Remaining
acceptance risk is the production fork's unproved mandatory clean lint result.

The independent bounded comprehension pass of R3 found no unsupported scope or
contradiction: evidence-only qualification, conditional owner correction and
preserved invariants are clear. Actual fork source corrections would require a
session with write access to that external checkout. Final HEAD, authority/task
hashes and maintainer tracked dirty-file set match the established subject.
