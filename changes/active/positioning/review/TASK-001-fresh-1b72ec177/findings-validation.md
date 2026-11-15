# Independent findings validation: FIX_REQUIRED

## Subject, authority and independence

Repository: `/Users/stevenforrester/Documents/GitHub/wyrd`.
Immutable full subject:
`443a79337fe33dfbfe9ccc64e01d10369d469131...1b72ec1774f68a8eefc6f4f25304dfc5aee1248f`.
The maintainer requested a new full review. No prior acceptance or closure
conclusion is carried forward. Prior stable IDs were consulted only to preserve
identity for the same outstanding causes; superseded findings are not imported.

Read the task-review skill, this round's scope and claim comparison, all four
discovery reports and verification report, candidate approved specification
revision 5 and original TASK-001, root/docs/SDK rules, Rust and maintainer guides,
and applicable client, Artifact, Verifier/workspace, security and Bifrost
projection authorities. The explicit current user direction to omit example
error/formatting scaffolding takes precedence over older task mechanics.
Revision 5 expressly authorizes the enumerated SDK additions. Neither can be
reintroduced as an old review blocker. Publication/tag availability remains a
delivery prerequisite rather than a task acceptance finding.

The validator inspected source rather than accepting agreement or reports as
proof. No new blocker was discovered by validation. No additional focused
review is needed: the only disagreement is whether an acknowledged coverage
gap violates an explicit gate, which the approved authority resolves directly.
HEAD remained the candidate and tracked source remained unchanged.

## Independently validated specialist applicability and coverage

| Contract | Validated coverage |
|---|---|
| Complete reader/application/task acceptance | Fresh behavior report covers the full task and its REQ/INV/AC matrix, native negative paths, navigation and figures. Required and present. |
| Cross-tier rules | Fresh standards report covers changed Rust client/spec/examples, Python bindings/stubs, TS projections/types, docs/tooling and version manifests. Required and present. |
| New abstraction/ownership seams | Fresh maintainer report compares model-schema helpers with the former Python owner, shared Target projection, all result constructors, and the existing motion/highlighting owners. Required and present. Validator inspected the schema helpers, projection and component-local clock to corroborate the ownership conclusions. |
| Evidence selection, exposure and historical attribution | Fresh evidence report traces extraction, exact publication refs, workspace grants/read, fixtures, public details and each taught language. Required and present. |
| Shared lifecycle/invariants | Separate invariant specialist remains inapplicable: the new schema/projection/result work transforms local values; BeatLoop owns one component's timer/observer/listener and cleans each up. No server cache, lease or shared coordination owner changes. Local lifecycle is covered by behavior and maintainer; exact identity by evidence. |
| System failure/recovery | Separate system specialist remains inapplicable: no supervisor, process boundary, deployment, server executor/transport or recovery owner changes. Native typed errors remain with existing SDK/server owners. Manifest changes outside client code align workspace versions. |
| Other sensitive domains | No changed permission evaluator, RLS/SQL, credential, persistence engine or concurrent coordination owner requires a separate security/tenancy/data/concurrency review. The evidence specialist covers the affected workspace trust boundary through existing authorization and storage owners. |

The four selected reports are complete for their stated scopes. The absence of
a fresh browser run is an explicit verification limit, not an omitted runtime
specialist. No new owner or replacement workflow requiring another specialist
was found in the assigned source/diffs.

## Proposal decisions

| Discovery ID | Decision | Stable ID | Independent determination |
|---|---|---|---|
| BEH-001 | CONFIRMED | FIND-TASK-001-7 | The sequential guide leaves a false destination input in place before promising a passing update result. |
| EVID-001 | CONFIRMED | FIND-TASK-001-4 | Explicit actual-program update/history proof is absent on two taught first-class SDK paths. This retains the prior cause's identity but is established freshly. |
| ST-001 | CONFIRMED | FIND-TASK-001-6 | The newly introduced live module item has no attached rustdoc under an explicit mandatory gate. This retains the documentation cause's identity, without importing prior evidence. |

No proposal is rejected or converted to optional advice. The documentation
omission is batched with the behavioral/proof corrections; it does not justify
a standalone style-only remediation round.

## Validated ledger

### FIND-TASK-001-7 — Restore supplied facts before the policy-update step

- **Status / discovery / classification:** CONFIRMED / BEH-001 / INCORRECT.
- **Obligation:** SPEC REQ-006 and AC-005; a runnable numbered Quickstart must
  reproduce its stated update result. Docs AGENTS requires clear observable
  success for each guide step.
- **Location:** `docs/src/content/docs/get-started/quickstart.md:117` and
  `:143–157`; generated full agent text consumes the same guide.
- **Source proof:** Step 3 says to set recipient authorization false, then
  destination restriction false instead. Even granting that “instead” restores
  authorization, it leaves restriction false. Step 4 only reads stored rows.
  Step 5 replaces the PDF and reruns without resetting restriction. Each
  language program constructs its requests from those edited facts. All three
  finance declaration owners require `inputs.destination.restricted == true`.
  `vala-verify/src/operators.rs:97` performs Equals using `json_equals`;
  `blocks/engine.rs:1256–1273` maps the false comparison to Failed; the full
  `decide` body at `:1363–1387` makes any Failed force overall Failed under All.
  A passing revised-policy judgment cannot override the destination failure.
- **Observable consequence/materiality:** An ordinary reader following the
  numbered guide gets failed external-audit and regulator requests, contradicting
  the displayed passed results at the defining policy-update observation. This
  is a normal supported path, not malformed-input hardening.
- **Smallest correction and burden:** At the shared source guide, explicitly
  restore both `authorized` and `restricted` to true after the negative exercise
  and before the update. Regenerate the coupled agent text with the existing
  generator. One local instruction corrects every language; no runtime changes,
  guard, helper, reset script, dependency or new test harness is warranted.
  Deleting the negative exercise or weakening the assertion would lose required
  behavior and is rejected.
- **Closure proof:** Inspect the corrected numbered sequence and regenerated
  content. The restored state must match the existing policy-update journey's
  original/revised inputs while independent negative-case proof remains intact.
  The bounded missing language journeys below can exercise this same restored
  state without adding a separate documentation harness.

### FIND-TASK-001-4 — Prove the published update on Python and TypeScript

- **Status / discovery / classification:** CONFIRMED / EVID-001 / MISSING.
- **Obligation:** SPEC AC-005 explicitly requires the actual-program journeys
  to reproduce policy reuse and the brief update follow-on. REQ-005/AC-004
  require changed exact selections and preserved old attribution. Root AGENTS
  requires the real journey on every shipped surface; original TASK-001
  Scenario 3 explicitly calls for extending Python/TS stories for reuse and
  exact identities. This is an acceptance-proof gate, not a claimed runtime bug.
- **Location:** Python `test_docsite_quickstart.py:103–152,214–228`; TypeScript
  `tests/integration/finance-release.test.ts:68–102`; published follow-on in
  Quickstart `:141–160`.
- **Producer-to-consumer evidence:** Each actual program declares a versionless
  local policy through the existing Artifact owner, which selects an exact
  policy file for the Verifier and Service. The common guide replaces that PDF
  and promises regulator failure becomes pass under new exact versions. Python's
  complete `run_quickstart` and repeat test run only the original policy. Its
  revised-PDF test explicitly declares version 0.1.0 and expects SPEC_DRIFT;
  it never reruns the versionless actual program after replacement. TypeScript's
  entire two-test file copies only the original PDF and never reads execution
  details. The Rust revision test at `examples/tests/finance_release.rs:489–600`
  does replace the PDF, asserts changed regulator output, checks old/new
  Artifact/Verifier/Service selection and reads an old execution after update.
- **Sibling alternatives checked:** Older Python/TS authoring journeys prove
  explicit versioning on different multi-Verifier graphs. Python's revised file
  appends `% revision 2`; it does not change the policy judgment. These and Rust's
  shared-owner proof do not reproduce this published Python/TS auto-versioned
  actual-program procedure. Existing SDK execution details and registry reads
  already expose all necessary identities; no API addition is needed.
- **Observable consequence/materiality:** Five Python and two TS green tests
  do not establish the explicitly required update/history workflow on two of
  the three first-class paths. The reviewer is not inferring a broken server,
  but the required journey gate is incomplete. Behavior discovery's description
  of this as a limitation and evidence discovery's blocker classification agree
  on source facts; AC-005 and the every-surface gate resolve the classification.
- **Smallest correction and burden:** Extend the existing actual-program journey
  owner in each missing language: retain original execution identity, replace
  the PDF with the already committed revised asset, rerun the actual program,
  assert the regulator outcome changes and exact policy/Verifier/Service refs
  advance, then inspect an original execution to prove it still names the old
  selection. Reuse ordinary Cards/Execution APIs and existing bounded test-side
  inspection helpers. Do not change normal example output, production behavior,
  verification waits or language SDK contracts for test convenience. Two bounded
  additions to established journey owners are sufficient; a separate framework,
  dependency or full-suite rerun per remediation is unnecessary.
- **Closure proof:** Run only the extended Python and TS finance journey selectors
  on their existing real server/Postgres/local-provider fixtures, plus narrow
  lint/type checks for their test write set. The proof must fail if the revised
  extraction is not read, the new policy leaves regulator requests failed, or
  old execution identity retargets. Keep the explicit-version refusal tests.

### FIND-TASK-001-6 — Document the new finance declaration module

- **Status / discovery / classification:** CONFIRMED / ST-001 / VIOLATION.
- **Obligation:** Root AGENTS explicitly says every new/changed Rust item has
  rustdoc and missing rustdoc blocks merge. `agent_docs/rust.md:233–241` applies
  the rule regardless of visibility. The skill expressly preserves this gate.
- **Location:** `examples/finance-release/finance_release.rs:4`, newly added
  `mod finance_service;`; module source `finance_service.rs:1` starts with an
  import and has no inner module docs.
- **Independent evidence and consequence:** The full base-to-candidate diff
  introduces the module item. Crate docs at the binary top attach to the crate;
  struct/function docs inside finance_service attach to those items. Neither
  attaches to the new module. This live declaration owner is compiled by the
  actual reader program, so the explicit documentation merge gate remains unmet.
  No runtime consequence or speculative maintenance harm is invented.
- **Smallest correction and burden:** One concise attached module rustdoc
  sentence describing its declaration/input role, either outer on the module
  item or inner in its source. Deleting the module would remove required example
  behavior; adding machinery, checks or tests is unnecessary. Preserve existing
  item docs and all behavior.
- **Closure proof:** Static inspection of attached module docs and the narrow
  Rust formatting check. Batch with the other corrections; no runtime test is
  needed for this sentence.

## Reconciled fresh acceptance matrix

| Obligation | Fresh implementation evidence | Verification evidence | Result |
|---|---|---|---|
| REQ-001/007, AC-006: exact opening, proof before infrastructure, languages and navigation | README/Introduction and homepage source; source-backed tabs and six-section links | Docs source/render tests, full 102 docs tests, build/link/a11y; behavior report covers entry paths | PASS |
| REQ-002/004, AC-001/002: synthetic inputs, typed result/reasons and application decision | Four releases, supplied purpose/recipient/destination, verify_release typed results and API pass/hold; current user intentionally omits scaffolding | Rust 3/Python 5/TS 2 journeys; independent negatives and uncertainty in existing tests | PASS under current user direction |
| REQ-003, AC-003: Service/Verifier/three checks, gateway Agent reads PDF | All three declarations plus YAML; .extract and exact extracted path; shared owner reuse | Real invocation fixtures require actual policy-read content; Python YAML parity and three-language journeys | PASS |
| REQ-005, AC-004: policy reuse, substantive update and immutable identities | Versionless publication uses existing exact-ref lifecycle; public details and stored query; Rust update/history implementation | Rust revision journey passes; Python/TS defining follow-on proof absent | FAIL for required cross-surface proof: FIND-4 |
| REQ-006, AC-005: complete reproducible setup and follow-on | Gateway/provider setup, release pins, files/output and numbered guide | Existing actual-program passes; sequential guide state contradicts step 5 and Python/TS follow-on absent | FAIL: FIND-7 and FIND-4 |
| REQ-007/AC-006 locked figures and motion | Source-backed API flow, Gateway only on Agent, separate delayed capture; four workflows and BeatLoop cleanup/controls | SSR pass/hold tests, build/a11y, historical task browser observations (not freshly repeated) | PASS within disclosed verification limits |
| REQ-008, AC-007: bounded inventory/handoff, shared source and editorial parity | Task dispositions, PRODUCT/package copy, setup-check consumers, llms generator-owned output | Docs parity/index/render tests; generated source relationship inspected | PASS except propagation required for FIND-7 correction |
| INV-001: approved additions only; existing owners | Rev5 model/schema/input helpers and predicates extend current SDK owners; no new durable workflow | Fresh schema/predicate/unit/type checks in verification report; maintainer source tracing | PASS |
| INV-002/003: synthetic supplied evidence, native semantics, separate retention | Purpose binding, disclosed policy provider exposure, exact selection and public details; no new result persistence owner | Journeys/readback source and existing typed error/uncertainty checks | PASS implementation; missing language proof included in FIND-4 |
| INV-004–006/non-goals | Existing routes, source generators, no new discovery platform or unsupported performance promise | Source/standards/maintainer reports and narrow docs checks | PASS |
| Mandatory changed-item Rust documentation | Retained types/fields/helpers documented, new module item undocumented | Direct diff and attached-doc inspection | FAIL: FIND-6 |

## Verification limits and final recommendation

The orchestrator's `verification.md` records fresh exact commands and results,
including real finance journeys (Rust 3, Python 5, TS 2), docs 102/build/AA checks,
language lint/types and focused schema/result tests. This validator did not
repeat those runtime checks. Green counts do not supply an absent scenario.
Full-range whitespace output is recorded; intentional Markdown hard breaks
and the extra TS EOF line do not establish a retained material finding.

The added/moved wait inventory is empty: changed Rust SDK schema/input/result
code is local preparation/projection, and the named server verification,
gateway, Oracle and Scribe runtime owners are unchanged. Publication/extraction
uses the pre-existing declaration owner. No hot-path benchmark exception is
asserted. No fresh browser, live provider or release publication is claimed.

**Recommendation: FIX_REQUIRED**, with the three findings above consolidated
into one bounded remediation task. All corrections fit approved revision 5 and
existing owners. No new product, architecture, public API, security, resource,
concurrency or persistent-data decision is necessary. No prior closure judgment
or convergence assessment is imported into this user-requested fresh review.

Skipped: independent runtime reruns and fresh browser/live-provider verification.
Risk: the sequential update guide and mandatory cross-language proof/docs gates
remain incomplete despite the existing green checks.
