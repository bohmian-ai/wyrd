# TASK-001 / R2 cumulative independent review

**FIX_REQUIRED**

## Immutable subject and authority

- Repository: `/home/thorrester/Documents/GitHub/wyrd-worktrees/split/agents-trace-processor`.
- Original base: `dac4c340f835676adc29f3dcc01f98e1d3d51465`.
- First reviewed candidate: `692d390dc40e43c6e1d7e25c7850d5c826d26a9e`.
- Previous candidate: `295446005b18604030bf376de6209098a28886f7`.
- Current candidate: `d088519ea846aba8b2a77c24b2af2491ad2b3936`.
- Authorized main merge: `1b769759dfd92a82e60082fe80460166bdd860f8`,
  importing `cb29781ab6f633700f9e47befddb1f325fd8b732`.
- Approved [spec r2](../../spec.md), [original task](../../tasks/TASK-001-framework-agnostic-telemetry.md).
- [Original verdict](../TASK-001-r2-692d390dc-20261015/verdict.md),
  [original validation](../TASK-001-r2-692d390dc-20261015/findings-validation.md),
  [R1 task/evidence](../TASK-001-r2-692d390dc-20261015/TASK-001-R1-telemetry-contract-closure.md).
- [Previous verdict](../TASK-001-R1-295446005-20261015/verdict.md),
  [previous validation](../TASK-001-R1-295446005-20261015/findings-validation.md),
  [R2 task/evidence](../TASK-001-R1-295446005-20261015/TASK-001-R2-python-overlap-and-docs.md).

The complete original-base-to-candidate diff is the subject. The R2-only and
imported-main contrasts are navigation aids, not replacement acceptance bases.
Imported main is authorized. Its telemetry/publisher/ingest/query/lifecycle and
proof seams are inspected; this verdict does not independently certify every
unrelated imported feature. Current authority defines publisher as verified
subject/current actor and existing implicit OTLP identity separates publishers;
no native explicit-batch-ID or new delivery contract is introduced.

## Independent review results

Every discovery role, focused follow-up and final validator was a separate fresh
agent, with its own report. Discovery reviewers received the same immutable
subject/map and applicable authority, without current peer conclusions or an
intended verdict. The root orchestrated, ran verification and wrote final
artifacts from the independently validated ledger; it did not validate its own
findings. All required reports are complete.

| Role | Report | Discovery / validation result |
|---|---|---|
| Behavior acceptance | [task-review-behavior.md](task-review-behavior.md) | PASS |
| Invariant acceptance | [task-review-invariants.md](task-review-invariants.md) | PASS |
| Repository standards | [standards-review.md](standards-review.md) | FAIL |
| Maintainer | [maintainer-review.md](maintainer-review.md) | PASS |
| System resilience | [system-review.md](system-review.md) | PASS |
| Security/tenancy | [domain-review-security.md](domain-review-security.md) | FAIL |
| Concurrency | [domain-review-concurrency.md](domain-review-concurrency.md) | PASS |
| OTEL integration | [domain-review-otel.md](domain-review-otel.md) | FAIL |
| Durability/lifecycle | [domain-review-durability.md](domain-review-durability.md) | FAIL |
| Focused replay/identity follow-up | [followup-review.md](followup-review.md) | RESOLVED |
| Fresh structured Ponytail validation | [findings-validation.md](findings-validation.md) | FIX_REQUIRED |

[Claim comparison](claim-comparison.md) grouped the three actor proposals into
one cause rather than treating agreement as proof. Follow-up was required by
an unexercised merged identity path and the semantic uncertainty about re-export
under a different actor. It traced public authorized delegation, both transports,
all signal dispatches, pre-stamp projection, stamps, in-memory/durable fences
and sibling native/outbox identities. Existing authority resolves this within
approved behavior. It additionally found the obsolete attribution readback;
root's traced execution confirmed that separate test failure. The validator
reconciled all proposals and ordinary-path PASS claims with these narrower gaps.

## Prior finding closure

All eight previous stable findings are **CLOSED**, supported by the independent
validator's source/evidence matrix:

| ID | Closure evidence |
|---|---|
| FIND-TASK-001-1 | Local TS callback types; clean packed root consumer works without optional peers |
| FIND-TASK-001-2 | Concrete framework procedures/readback and complete TS exporter prerequisites; isolated emitted module loads |
| FIND-TASK-001-3 | Real live-flush readback before continuation; active-span and post-shutdown global-provider export |
| FIND-TASK-001-4 | Safe late Responses usage, zero counts and per-counter fallback retained |
| FIND-TASK-001-5 | Bounded Python stock-provider wait, one in-flight operation and native drain on timeout |
| FIND-TASK-001-6 | TS post-import same-state recheck precedes synchronous installation/publication |
| FIND-TASK-001-7 | Python successful call requires a covering operation started after entry, within one deadline |
| FIND-TASK-001-8 | Per-state GIL-aware setup gate preserves OnceLock/GC, retry and other-owner refusal |

None is reopened merely because the candidate contains another defect.

## Validated retained findings

| Stable ID | Classification | Consequence and selected smallest correction |
|---|---|---|
| FIND-TASK-001-9 | REGRESSION | Identical OTLP content for one subject through different actors shares an implicit batch fence and can lose the later actor's attribution; bind the verified subject/actor pair in the existing Gate-derived identity |
| FIND-TASK-001-10 | VIOLATION | The new Rust settlement test lacks its mandatory Panics rustdoc section; add that section with assertions unchanged, bundled with this remediation |
| FIND-TASK-001-11 | MISSING | Python's missing-credential journey matches human text instead of the public error code; expose the actual structured exception code across its existing child boundary and assert exactly `WYRD_CLIENT_401_NO_CREDENTIALS` |
| FIND-TASK-001-12 | REGRESSION | Existing Rust OTLP attribution journey fails querying removed `principal_id`; migrate query/extraction/assertions/rustdoc to subject and actor, preserving replay/Run checks |

The full ledger in [independent validation](findings-validation.md) supplies
source locations, reachability, every discovery disposition, all relevant
callers/siblings, approved obligations, correction constraints and focused
proof. FIND-9 is source-established; no dynamic actor-collision test has been
executed. FIND-12 is independently reproduced twice and is not misrepresented
as runtime proof of FIND-9. FIND-11 alleges missing proof, not a broken resolver.
FIND-10 is an explicit required-rustdoc omission, not an optional wording round.

## Reconciled acceptance matrix

The validator provides the full source/proof matrix. This compact table includes
all 21 original obligation IDs; partial working behavior stays recognized.

| Obligation | Result | Evidence or retained boundary |
|---|---|---|
| REQ-010 | PASS | Ordinary optional bridges, hierarchy/timing/start scope/late allowlisted metadata and callback lifecycle |
| REQ-011 | FAIL | Generic/native framework integration works; actor-aware publisher retention fails at shared OTLP identity: 9 |
| REQ-012 | PASS | Same-state setup/retry, explicit provider refusal/cleanup and application-owned configuration preserved |
| REQ-013 | PASS | Bounded covering flush, native drain/error precedence, active/global provider continuation |
| REQ-014 | PASS | Standard OTLP/context/sampling/content controls and optional Card/Run correlation |
| REQ-015 | PASS | Concrete current docsite procedures, complete prerequisites and readback |
| REQ-016 | PASS | Actual three Python/one TS framework matrix and owner/capability selection; credential-code proof separately mapped |
| REQ-017 | FAIL | Ambient/shared-token/gateway implementation works; exact Python credential-negative proof missing: 11 |
| INV-001 | FAIL | Authorization/tenant boundary retained; implicit OTLP identity omits actor: 9 |
| INV-002 | FAIL | Production scope/non-goals respected; Rust OTLP preservation proof is red: 12 |
| INV-003 | PASS | Application exceptions and payload-free metadata/diagnostics retained |
| INV-004 | PASS | Managed/owned provider ownership explicit; Run has no provider lifecycle |
| AC-001 | PASS | Real Python3/TS model/tool activity and authenticated readback |
| AC-003 | FAIL | Required Rust docs and existing affected Rust journey: 10,12 |
| AC-007 | PASS | Bridge fidelity/start identity, optional declarations and safe supplied usage |
| AC-008 | PASS | Idempotent setup/retry and owned-provider settings/export |
| AC-009 | PASS | Live readback, honest bounded failures/native drain, active/global survival |
| AC-010 | FAIL | Ordinary framework scope/error/stream cases pass; identical content from distinct actors can converge: 9 |
| AC-011 | PASS | Sampling/content, denied persistence and tenant isolation |
| AC-012 | PASS | Runnable current docsite recipes and coherent lasting authority |
| AC-013 | FAIL | Ambient/gateway/fresh tokens work; Python subprocess exact no-credential assertion missing: 11 |

No new framework runtime/registry/discovery, monkey-patching, Pydantic/ADK
adapter, Rust framework integration, auth subsystem, bespoke export queue,
public payload/schema/API or global-provider shutdown redesign entered the task.
Owner-directed dependency floor/dev changes and main merge remain authorized.

## Verification and limits

Fresh root execution in [verification.md](verification.md):

- Cumulative diff whitespace: PASS.
- Focused Python setup/owned/lifecycle/deadline selectors: 16 passed.
- TS telemetry unit file: 5 passed.
- Existing packed generic runtime/strict consumer: PASS without optional peers.
- Documented OTEL entrypoint dependency closure/import: PASS using existing installed packages in a cleaned temporary project.
- Real Python framework file: 13 passed, no skips.
- Real TS framework file: 5 passed, no skips.
- Existing exact Rust OTLP publisher-attribution journey: selected 1, failed 1 twice; migration passed each time. Debug rerun proves removed-column SQL planning failure after accepted ingest/Scribe publication.

The Rust run rebuilt its affected dependency cone and test binary. No concurrent
Cargo, source changes, production credentials or synthetic load were used.
Broad static/type/generated/docs/whole-unit lanes remain historical committed
evidence, not fresh review aggregate execution. Dynamic actor-collision closure
and exact Python code proof remain remediation work. Existing stock Python
export-refusal logging, TS upstream missing usage and best-effort abrupt-exit
loss limits remain as approved/documented.

## Handoff

One decision-complete remediation:
[TASK-001-R3-otlp-publisher-and-proof.md](TASK-001-R3-otlp-publisher-and-proof.md).
Route directly to `$wyrd-implement`; then independently review the original
cumulative task plus all remediation packets. No spec revision or unresolved
material decision is needed. A red affected check cannot be excused by its
merge provenance or by passing framework cases.

Source, specification, existing tasks, index and pinned HEAD stayed immutable.
Only this new review packet was written; no fix, staging, commit, merge, push
or deploy occurred. Broad aggregates and unrelated imported-main auditing were
skipped; the retained actor/proof/documentation gaps block completion.
