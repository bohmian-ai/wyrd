# TASK-001 review verdict

**SPEC_REVISION_REQUIRED**

The candidate does not pass TASK-001. Independent validation retains five
bounded implementation/proof findings and one approved-authority conflict.
The latter requires a specification decision before a complete, decision-ready
remediation task can be issued. This review does not implement a correction or
change the approved specification.

## Immutable subject

- Repository: `/home/thorrester/Documents/GitHub/wyrd-worktrees/sdk-client-instantiation`.
- Base: `2f1db2194f5630df850b10987fe0f360946a525c`.
- Candidate: `fde7d396a3917964dc344ede2ed09046e179aa66`.
- Range: the complete eight-commit implementation range, 153 changed files;
  not merely the final formatting/evidence commits.
- Approved authority: [specification revision 9](../../spec.md).
- Original task: [TASK-001](../../tasks/TASK-001-unify-sdk-handle-instantiation.md)
  as stored at the base. The candidate appends implementation evidence without
  changing that original task's requirements.
- [TASK-002](../../tasks/TASK-002-document-default-sdk-instantiation.md) owns
  authored docsite/README guidance and generated docsite outputs. Its pending
  work is not a TASK-001 failure.
- Starting tracked tree was clean. HEAD and tracked source were checked again
  after independent validation; candidate/source remain unchanged. All writes
  in this review are confined to this new review directory.

## Required specification decision

**FIND-TASK-001-6:** AC-002 requires every in-scope handle, including Platform,
 to succeed with environment-only and saved-configuration-only credentials.
D4 and INV-001 preserve Platform's existing separate session exchange and
credential semantics, and the task forbids a new authentication path.

The existing saved file floor resolves to `ApiKey`; saved human login resolves
to `Renewable`. Platform accepts only `BearerToken` before its separate
exchange. Saved endpoints plus an environment platform credential work through
existing sources, but do not constitute saved-credentials-only success.
Private test fixture changes cannot make that literal production path succeed.

Return this exact decision to `$wyrd-spec`:

- Qualify saved-only success by existing credential-source applicability:
  tenant handles use saved credentials; Platform uses its existing ambient
  platform credential/session path; or
- Explicitly approve a persisted platform credential source and its security,
  ownership, and acceptance contract, revising the current prohibitions.

Neither choice is silently selected here. Do not add a saved source, weaken
Platform's variant check, reinterpret the tenant credentials file, or route
platform credentials through tenant exchange as bounded remediation.

## Independent reviews and validation

| Role | Report | Discovery result |
|---|---|---|
| Behavior implementation | [task-review-behavior.md](task-review-behavior.md) | FAIL; all 33 obligations mapped |
| Invariant implementation | [task-review-invariants.md](task-review-invariants.md) | FAIL; all 33 obligations mapped |
| Repository standards | [standards-review.md](standards-review.md) | FAIL; all changed language/layer groups covered |
| Maintainer | [maintainer-review.md](maintainer-review.md) | FAIL |
| System resilience | [system-review.md](system-review.md) | FAIL |
| Security/identity domain | [domain-review-security.md](domain-review-security.md) | PASS within reviewed source scope |
| Concurrency/runtime domain | [domain-review-concurrency.md](domain-review-concurrency.md) | FAIL |
| Focused follow-up | [followup-review.md](followup-review.md) | Tenant proof feasibility resolved; Platform applicability conflict identified |
| Fresh structured Ponytail validation | [findings-validation.md](findings-validation.md) | SPEC_REVISION_REQUIRED; six retained ledger items |

All required agents were separate, fresh contexts. Discovery reviewers did
not receive each other's conclusions. Every proposed finding, including
unique claims, went to the fresh validator; matching reports were not treated
as proof. The orchestrator grouped claims and wrote this verdict from the
independently validated ledger rather than validating its own findings.

### Follow-up decision

One focused follow-up was required: the discovery reports did not establish
how true Rust ambient factories could be tested without runtime environment
mutation or test child processes. It found a bounded approach through existing
runner, configuration, server-bind, tenant, and credential-fixture owners. It
also identified the separate Platform saved-only conflict. No second discovery
pass was needed; the validator checked those sources and resolved the
remaining proposal boundaries from approved authority.

See [claim-comparison.md](claim-comparison.md) and [subject.md](subject.md) for
the shared starting map, claim grouping, and review constraints.

## Validated finding ledger

Exact locations, full producer-to-consumer traces, proposal dispositions,
existing-owner correction recommendations, and focused commands are preserved
in [findings-validation.md](findings-validation.md). This table contains only
independently confirmed or revised findings.

| ID | Status / classification | Violated obligation and observable consequence | Correction boundary |
|---|---|---|---|
| FIND-TASK-001-1 | CONFIRMED; INCORRECT / REGRESSION / VIOLATION | REQ-007, INV-001, Python property-IO rule. Bifrost local binding/counters and Cards typed-view selection initialize credentials/transport before a server operation; inherited AsyncBifrost local calls can stall the event-loop thread. | Keep unopened local state and typed-view selection local, sharing the existing deferred owner; preserve postconnection state and the one client. Correct tests that currently bless local access as initialization. |
| FIND-TASK-001-2 | REVISED; MISSING verification | AC-002/004/007. Rust journeys manually resolve a client and use connect_with, bypassing public no-client factories; Python eager AsyncBifrost success is omitted. | Drive actual public factories using existing harness/runner startup configuration and bounded fixture support; extend the existing Python ambient journey. Reserve saved-only Platform applicability to FIND-6. |
| FIND-TASK-001-3 | CONFIRMED; MISSING verification | AC-003. Permission-intersection assertions do not compare server-observed subject attribution in the three language connect journeys. | Extend each representative delegated journey with an allowed operation and existing returned/retained subject attribution; preserve denial checks, with no new server/auth/audit contract. |
| FIND-TASK-001-4 | REVISED; VIOLATION of mandatory rustdoc | AGENTS §16 and REQ-010/012 API-doc closure. Changed Rust items have missing/stale failure/panic contracts, WyrdState links to a removed constructor, and first-use timing prose needs alignment. | Batch accurate documentation and existing Result propagation at the validated changed items. Withdraw accidental getter error contracts when local operations become infallible. No new checker or style-only round. |
| FIND-TASK-001-5 | CONFIRMED; REGRESSION | Migrated identity consumer closure. revoked_login_is_refused now panics expecting successful eager connection before its intended refusal assertion. | Assert the existing SavedLoginUnusable error on eager connection itself, preserving real revocation/expiry and sibling assertions. |
| FIND-TASK-001-6 | CONFIRMED; approved-authority conflict | Literal Platform saved-only AC-002 cannot succeed under preserved D4/INV-001 credential sources and the prohibition on new authentication paths. | Specification decision above; no unapproved executable correction. |

## Reconciled acceptance matrix

Results below are the validator's static source/proof assessment, not claims
that this review executed passing runtime checks. Detailed source citations
and individual test evidence remain in the independent reports and validated
ledger. `PASS` does not waive a separately identified mandatory proof gap.

| Obligation | Implementation evidence | Verification evidence / limitation | Result |
|---|---|---|---|
| REQ-001 | Shared handle factories and SDK projections | Public-surface tests inspected | PASS |
| REQ-002 | Shared connect_ambient/from_global and explicit-client paths | Direct Rust public ambient journey missing; FIND-2/6 | PASS implementation |
| REQ-003 | connected/access_token and existing transport setup | Exchange/bearer cases inspected; identity consumer FIND-5 | PASS implementation |
| REQ-004 | BifrostOptions and language option projections | Options remain optional-client; ambient proof FIND-2 | PASS implementation |
| REQ-005 | Rust async/blocking, Python sync/async, TS Promise factories | Source and boundary tests inspected | PASS |
| REQ-006 | Python argument-only constructors and eager factories | Constructor/surface tests inspected | PASS |
| REQ-007 | LazyConnection exists; local consumers initialize early | Incorrect first-use property assertions; FIND-1 | FAIL |
| REQ-008 | Successful eager/lazy handles share the same native owner | No separate post-initialization difference established | PASS |
| REQ-009 | AsyncBifrost constructor has no IO; eager factory uses to_thread | Constructor unit and lazy async journey inspected; eager proof FIND-2 | PASS implementation |
| REQ-010 | Old public constructors removed | State API docs still link removed constructor; FIND-4 | PASS API; docs FAIL |
| REQ-011 | TS private constructors; Rust eager-only public forms | Source/surface negative proofs inspected | PASS |
| REQ-012 | Executable examples and SDK docs migrated | Remaining SDK API/rustdoc closure FIND-4; TASK-002 prose excluded | FAIL TASK-001 doc closure |
| REQ-013 | Client model and language reference updates | Authority diff inspected | PASS |
| REQ-014 | One retained shared client/cache; separate Platform session client | Existing retention/renewal source; public Rust proof FIND-2 | PASS implementation |
| REQ-015 | Existing credential path before factory return; no extra bearer probe | Shared exchange-count and credential-path tests inspected | PASS |
| REQ-016 | Rust auth projection and Python auth package/registration | Removal/import tests and declarations inspected | PASS |
| REQ-017 | Shared query, async/blocking, Python and TS list projections | Three language discovery journeys inspected | PASS |
| REQ-018 | One existing compact list GET, separate describe | No fan-out/cache/aggregation in source | PASS |
| REQ-019 | Existing wire type and typed Python/TS projections reused | Stubs/declarations statically align | PASS |
| INV-001 | Server/wire/permissions unchanged; local Python behavior regresses | FIND-1; also bounds FIND-6 | FAIL |
| INV-002 | One shared resolver | No new language resolver found | PASS |
| INV-003 | Supplied/delegated client retained | Production source sound; exact attribution proof FIND-3 | PASS implementation |
| INV-004 | Existing redacted errors and credential owners retained | Security source review; no exhaustive runtime redaction rerun | PASS static |
| INV-005 | Earned foreign-runtime wrappers reuse native owners | No new transport/auth/durable implementation found | PASS |
| AC-001 | Approved public shapes, removed forms, visibility | Surface tests and compile-fail docs inspected; docs not executed here | PASS static |
| AC-002 | Actual no-client factories exist | Rust/Python eager async proof FIND-2; Platform conflict FIND-6 | FAIL / SPEC_REVISION_REQUIRED |
| AC-003 | Delegated connection source is sound | Required server-attribution comparison absent; FIND-3 | FAIL proof |
| AC-004 | Eager/lazy shared errors implemented | Public Rust missing-credential path bypassed; local trigger FIND-1/2 | FAIL proof/boundary |
| AC-005 | Bifrost constructors perform no IO | Unit source inspected | PASS static |
| AC-006 | Stubs/declarations align; most authorities/examples migrated | SDK documentation FIND-4; TASK-002 remainder excluded | FAIL TASK-001 portion |
| AC-007 | Shared retained cache and renewable source | Public Rust ambient retention missing; FIND-2. Identity consumer FIND-5 remains reachable. | FAIL proof |
| AC-008 | Existing authorized/tenant list and describe projected | Three language journey assertions inspected | PASS static |
| AC-009 | Correct list names and compact typed entries | No catalog/schema aliases or fan-out found | PASS static |
| Other task non-goals | Local loaders remain deferred; no new route/wire/permission/cache/dependency/Platform language projection | Cumulative diff and role coverage | PASS static |

## Verification and limits

The implementation appendix supplies lane-result claims, including format,
lint, typing, unit, generated/boundary/example checks and SDK journeys. No raw
command transcript was supplied. Those claims were available to reviewers but
were not allowed to override contrary source or missing assertions.

This review ran no builds, tests, generators, formatters, identity services,
or runtime test subprocesses. It inspected source, cumulative diff, actual
test bodies/selectors, setup owners, and lane definitions. `git diff --check`
and tracked-source/candidate immutability checks passed during orchestration.
The identity lane was not rerun; FIND-5 is a full source-backed failure trace,
not a claimed reproduction. Focused closure commands are recommendations in
the validated report and remain unexecuted.

The saved-login lane's different setup and the lack of direct public ambient
proof are material limits of the reported broad green SDK results. Pending
TASK-002 docs do not count against this task. The pre-existing exchange-error
mapping is not retained as a new implementation finding. No speculative new
queue, cancellation, recovery, security, or combinatorial test requirement
was accepted.

## Prior-finding closure and handoff

This is the first review of the original cumulative task; there are no prior
FIND IDs or remediation rounds to close. FIND-TASK-001-1 through -6 are newly
validated, not human-directed additions relabeled as reviewer findings.

No remediation task is issued under this SPEC_REVISION_REQUIRED verdict:
FIND-6 requires the explicit applicability/credential-source decision above.
The five bounded correction boundaries and focused proof are fully recorded
for the eventual `$wyrd-implement` handoff after specification authority is
resolved. Do not silently change the specification or implement a new saved
Platform credential path to make this review pass.

No source edits, staging, commits, merges, pushes, or deployment were performed.
