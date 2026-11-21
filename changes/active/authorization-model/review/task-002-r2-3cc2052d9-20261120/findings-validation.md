# TASK-002 R2 independent validation

Recommendation: **FIX_REQUIRED**. Both assigned behavioral gaps are closed.
The changed production dependency owner still lacks its mandatory clean lint
qualification. This retained blocker originated with the Oracle specialist and
was independently source-validated here; no blocker originated with the validator.

## Immutable subject and method

Repository `/home/thorrester/Documents/GitHub/wyrd`; branch
`task/authorization-model/TASK-002`. Direct remediation range:
`ff42598100b0edabad5cad2272a3c7735f2eef14` →
`3cc2052d977c94f52a11548f887ea548c2d6f297`. Cumulative original base:
`300542877cd2a58cbe5f061d5075616e4cc4397b`.

Approved authority is the supplied working-tree SPEC-authorization-model
revision 2, not the older committed packet. Its SHA256 is
`c27e81cab3368de9a27d93f37fddf620155f107768c80a263a7f937d043f2d9a`;
original TASK-002 SHA256 is
`00479aabdaf43a4212dd0e2f66c8d2ea8fe953d9f01f4521c02cac0341c1a127`.
Both match the prior immutable review subject. The maintainer's dirty packet
edits are preserved and excluded from implementation review.

This fresh reviewer read the assigned approved requirements independently,
the original task, R1 verdict/validation, R2 remediation and acceptance evidence,
the direct diff, changed assertions, fixture/deployment producer and affected
consumer paths. CodeGraph preceded code exploration; it returned the policy
declaration helper but did not locate the anonymous journey, so focused source
reads completed that coverage. Applicable root instructions, task-review/UI
skills, workflow, maintainer style, TESTING, architecture authority map,
security retained-data/peer rules and Bifrost Variant SQL rules informed review.

## Prior finding dispositions

| Stable finding | Disposition | Independent source/evidence assessment |
| --- | --- | --- |
| FIND-TASK-002-1 | CLOSED | The Alice journey now runs `serverTest({ githubApp: true, perTest: true, verificationRuntime: true })`, declares the existing deferred `ci-gate`, and binds it to both Repositories with `policies/gated.toml`. Real PR deliveries create retained executions; public `verify.Evidence.publish` correlates separate CI evidence to each repository/head. Admin controls require two produced results, evidence rows and execution outputs. Alice consumes the payments verdict, evidence identifier/value and requirement outcome and sees one count per table. Populated ledger counterparts are absent from selected rows, later result/evidence pages and execution/evidence point predicates. |
| FIND-TASK-002-3 | CLOSED | The actual native physical-plan codec now executes a masked scoped plan locally and after serialization for row selection, Variant key read, key predicate and whole Variant. The whole-column assertion requires `arrow.parquet.variant` locally, identical peer/local Field metadata/type/nullability, and identical array data. The dependency pin and peer fingerprint revision move together. |
| FIND-TASK-002-2 | Remains CLOSED | No status lookup, credential issuance, suspension, revoke owner, SQL lifecycle or Access production code changes in this range. R1's independently validated lifecycle and suspension proof carries forward. |

### Connected producer and consumers

`ui-pages.integration.test.ts:2374–2378` owns the fresh reviewing deployment;
the support `serverTest` forwards `verificationRuntime` to the real native test
server (`sdks/wyrd-sdk-ts/wyrd/tests/support/server.ts:260–285`).
`ui-signals.integration.ts:18–24` declares the fixture policy through public
Cards from a repository checkout. `fixtures/cards/deferred_evidence/ci-gate.yaml`
requires the deferred CI value's status to equal green. The six-line new policy
binds that exact Verifier to pull-request work; no fabricated result/evidence
insertion or alternate enforcement path was introduced.

The journey (`ui-pages.integration.test.ts:2653–2940`) preserves the UI-created
invitation, Alice's password login, saved family binding and matching Access
summary, real local device approval and redeemed CLI credential. Her ambient
TypeScript client, Python process and MCP authenticated transport all reuse that
login; admin uses an explicit separate client. New checks at `:2804–2841`
consume actual payments outputs and populated ledger negative controls. CLI
reads the published payments evidence and completed member (`:2843–2850`),
Python reads its passed result (`:2852–2859`), and MCP reads its result and member
with ledger member refusal (`:2861–2869`). Browser navigation opens the exact
completed payments member, still refuses its independently protected Verifier
definition, and hides ledger in lists and point navigation (`:2871–2900`).
The remediation expressly permits the narrow populated-output seam rather than
multiplying each result/evidence assertion across every surface; the browser's
completed-member assertion is not represented as an evidence-value assertion.

The subsequent grant/removal cycle continues using the same browser, CLI and
SDK identities. Existing tests for sensitive fields and the row-mask's local
negative paths remain unchanged. No production UI, new API, generated contract,
schema, migration, runtime or grant resolver is added. This is the smallest
existing-owner correction to the missing connected proof.

### Peer proof inspected independently

`oracle/codec.rs:755–873` uses production plan encode/decode, replacing only the
source leaf with the existing extension provider and requiring exactly consumed
source mapping. `verification_session` (`oracle/mod.rs:5207–5290`) supplies a
payments row, an other-repository row and a row with no provable parent, each
with distinct Variant contents. Family-scoped execution read therefore has
meaningful populated hidden-row controls. The result-id query and all Variant
queries explicitly expect one payments result, rather than merely comparing
two potentially incorrect empty outputs. The whole Variant compares full Field
and ArrayData, so a metadata-only loss cannot pass as value parity.

The shared mask remains in `restrict_verification_rows` (`oracle/mod.rs:3933`)
and the changed dependency is its existing physical-cast codec owner. No new
Wyrd mask, Variant function, serializer wrapper or policy path is introduced.
Prior local null-mask and sensitive-content proof remains applicable: the
remediation changes serialization of the explicit target Field, not visibility
semantics. This in-process codec test does not claim real multi-server routing,
transport, object-store or distributed recovery proof.

## Execution provenance and process history

Supplied R2 acceptance evidence binds the Alice selector to `c1991bc4b`, with
one passed production BFF/server/Postgres journey and clean `mise run ui:check`.
The later commit changes only the peer test and dependency pin; its recorded
959 passing `vala-bifrost-redux --lib` tests, exact focused codec selector and
Wyrd fmt/lints cover that dependency seam. The report also supplies fork cast
tests (29) and proto tests (286). These are implementer execution records,
independently checked against assertions and selected source, not fresh reviewer
runtime observations. Direct reviewed-range `git diff --check` was rerun clean.
No zero-selected run is treated as proof.

TDD is separate from behavioral acceptance. The new Alice work is an evidence
correction with no exposed production defect, so no manufactured RED is required.
The supplied peer RED describes the exact missing extension metadata before its
codec fix, followed by passing targeted and regression tests; it is credible
supplied RED/GREEN, not independently witnessed history. The original Alice
test-first violation and unverified original RED history remain recorded as in
R1; accepting corrected behavior does not erase them.

Added/moved hot-path waits: PostgreSQL **none**, Scribe **none**, object storage
**none**, Vault **none**. The patch changes in-memory plan serialization; test
setup is not a new production wait. Benchmark qualification is expressly
excluded by the maintainer and approved packet; no measured-performance claim
is made. Original integrated Bifrost 64/66 remains a carried-forward integrated
change limitation, not a green aggregate or a reason to reopen unrelated work.

## Specialist reconciliation and retained finding

The selected [Oracle report](task-review-oracle.md) is present. It independently
traces the external dependency's PhysicalCastNode/physical expression and peer
fingerprint boundary, including the fetched exact revision, protobuf projections,
explicit-nullability reconstruction and native follower consumers. Its behavioral
closure conclusion agrees with the source proof inspected here. No additional
security/lifecycle, broad repository, abstraction or recovery reviewer is
warranted: those owners are unchanged, prior independent coverage remains valid
and this reviewer covers affected UI consumers and residual rules.

### FIND-TASK-002-4 — EVIDENCE_GAP: mandatory changed-owner lint is not qualified

**Source proposal:** Oracle ORA-R2-1, **CONFIRMED**, classified as the missing
mandatory qualification rather than an asserted production correctness defect.

**Authority:** R2 remediation lines 111–114 explicitly require affected-owner
format/lint checks. The changed codec owner is the DataFusion fork, whose
`AGENTS.md:12–27` requires a warning-free
`cargo clippy --all-targets --all-features -- -D warnings` as a minimum before
committing. The user excludes benchmarks explicitly; reporting fork clippy as
not clean is a disclosed limit, not an instruction waiving that check. Wyrd's
task-review acceptance rule preserves mandatory gates.

**Location:** Wyrd `Cargo.toml:300–310`, `Cargo.lock` and
`oracle/codec.rs:32` adopt production dependency revision
`1a0f1af0bc6ce686e5fd25aaee000bd88f6ca17a`; the required owner gate is
`/home/thorrester/Documents/GitHub/datafusion-scalar-snapshot/AGENTS.md:27`.

**Starting conditions and trigger:** completing the peer Variant proof exposes
the loss in the previously pinned physical cast codec. The implementation
corrects that production owner and adopts the resulting fork commit in Wyrd.

**Expected qualification:** the changed owner has clean required lint evidence
at the revision Wyrd uses. **Actual evidence:** the maintainer's supplied review
summary explicitly says the fork's clippy was not run clean. Recorded fork unit
tests and Wyrd workspace fmt/lints establish their respective properties; Wyrd
lints do not establish all-target/all-feature dependency-owner lint qualification.
No clean fork result is supplied. The validator inspected the exact owner
instruction independently and reconciled the task's affected-owner requirement.
No warning or runtime defect is inferred from absent qualification.

**Consequence:** both behavioral repairs have credible proof, but required
qualification of the adopted production correction remains incomplete, so the
task cannot PASS. This is available but inadequate proof, not missing access
that would make the review BLOCKED.

**Smallest testable correction:** obtain a clean required owner lint result for
the exact pinned revision in its applicable environment, and record command,
revision and outcome. If actual failures require source correction, correct them
at that owner, preserve codec semantics, update Wyrd dependency pins/fingerprint
together and repeat the exact focused codec proof and affected checks. Evidence
alone can close this finding if no code fix is necessary. Do not rewrite commits
to simulate precommit history, reconstruct RED or change working production code
solely to create a new history. No full integrated suite or benchmark obligation
is added: the R2 packet expressly limits runtime proof and excludes those suites;
the retained lint is its explicitly applicable affected-owner check.

This second unsuccessful remediation review has converged on owner qualification,
not another runtime redesign. The original connected-output omission is closed;
the fix-induced codec compatibility omission is closed. The remaining finding is
a newly exposed mandatory check at the codec correction's real owner, not an
expanded product expectation or an allegation that the fixes regressed behavior.

Skipped runtime reruns, broad suites, screenshots, benchmarks and deployment.
No production/authority edits, commit or push. Required fork lint remains
unqualified; real multi-server execution and measured performance are not claimed.
