# TASK-001 R2 independent remediation review

Recommendation: **PASS**. Four R2 issues close through the implementation and proof. The fifth, FIND-TASK-001-11, is rejected as an incorrect prior diagnosis; the new language tests additionally exercise the intended scoped approval boundary. No validated blocking finding remains.

## Immutable subject and method

- Candidate: `300542877cd2a58cbe5f061d5075616e4cc4397b`.
- Remediation base/prior candidate: `284f801159a391e92493c3072878fd644c9d173b`.
- Original candidate: `c78ad7b5241fbbfb54e85e2390c6a694787debfc`.
- Cumulative base: `eeb3bbd4f891ae88b00ce1979810eb2a11d04871`.
- Approved contract: SPEC-authorization-model revision 2; original TASK-001 and supplied R1/R2 remediation records.
- Actual correction diff: `git diff 284f80115 300542877`, four commits, with cumulative source/prior source-backed acceptance available.

Fresh reviewer `r2_closure_review` independently checked the five R2 items, latest changed producers/consumers, assertions and supplied execution evidence. CodeGraph preceded code tracing. Read root and applicable docs/Python/TypeScript AGENTS.md, task-review skill, workflow, maintainer guidance, TESTING.md, applicable security authorization/freshness/lifecycle authority, Wyrd's single auth plane and Bifrost public boundary. Existing uncommitted packet/evidence edits are supplied inputs, not reviewed production edits or authority to commit them.

This is focused remediation review. Prior unchanged acceptance evidence is retained where the correction leaves its implementation and proof dependencies intact. The only runtime owner changed is the authority cache; its request, stream, confirmation and installation paths were traced together. The CLI and language changes are proof, the Card changes are authored fixture inputs, and the documentation changes remove contradictory advice and untested duplicate snippets. There is no new SQL capability, migration, public wire contract or principal model.

One fresh remediation reviewer suffices under the skill's remediation procedure: the identified shared-state correction is bounded and traced through its existing owner, no new security boundary or unresolved failure topology remains, and no new proposed blocking finding requires another independent validator. Prior full-task specialists remain applicable to unchanged boundaries. Extra reviewers would repeat that accepted coverage.

## Stable finding dispositions

| Stable finding | Current disposition | Basis |
| --- | --- | --- |
| FIND-TASK-001-1 | CLOSED, carried forward | R1 provisioning-administration correction and real-server refusal/atomicity proof are unchanged. |
| FIND-TASK-001-2 | CLOSED | R2 fixes absent-entry observation and in-flight unknown invalidation at AuthorityCache; details below. |
| FIND-TASK-001-3 | CLOSED, carried forward | Scoped Card mutation owners unchanged; second-release test input now checked in, with assertions preserved. |
| FIND-TASK-001-4 | CLOSED, carried forward | Retained direct identity/attribution correction and deletion/read proof unchanged. |
| FIND-TASK-001-5 | CLOSED, carried forward | Tenant-bound exact target validation and invitation/account atomicity correction unchanged. |
| FIND-TASK-001-6 | CLOSED, carried forward and regression checked | Credential settlement remains unchanged; fresh same-second and real-Postgres outage component tests pass with the new lifecycle. |
| FIND-TASK-001-7 | CLOSED, carried forward | Existing typed MCP adapters/handler reuse and their recorded journey proof unchanged. |
| FIND-TASK-001-8 | CLOSED | Local User/invitation/password/device guidance corrected; untested duplicate SDK snippets removed; fresh build/accessibility checks supplement recorded docs proof. |
| FIND-TASK-001-9 | CLOSED, carried forward | Concrete original rustdoc omissions remain corrected; new lifecycle items document the invariant. |
| FIND-TASK-001-10 | CLOSED, carried forward | Actual public multi-pod peer filtering assertions remain; only the other-parent Card input changes to checked-in fixtures. |
| FIND-TASK-001-11 | REJECTED / prior diagnosis corrected | At both the prior candidate and current candidate, compile_binding derives execution read from Card read at All. The old tenant-wide approver positive tests were valid. New Verifier-scoped tests independently strengthen the approved parent-visibility proof. |
| FIND-TASK-001-12 | CLOSED | New real-server CLI journey executes direct/group grants, inspection, removal and non-admin refusal, with observable Card reads. |
| FIND-TASK-001-13 | CLOSED | Both new runtime YAML rewrites and two same-branch sibling rewrites are replaced by checked-in Cards; exact family/version/peer assertions remain. |

## Closure traces and correction of the prior record

### FIND-TASK-001-2 — shared lifecycle fix and superseded logic removal

Authority: REQ-004 and security posture Current authority and freshness. The root issue was that invalidation state belonged only to already-installed snapshots, although notifications and loads run independently.

`wyrd-auth/src/authority.rs:470-516` now holds an optional snapshot, highest observed tenant revision and confirmation generation under the existing cache lock. `observe` at `:684-691` creates an observation-only entry when the tenant has never loaded, so a notification received during its first load is retained. `notify` at `:698-711` advances a common generation for an unknown/malformed notification. It no longer loops over currently installed tenants clearing an optional timestamp.

`reload` at `:594-611` captures the generation before opening the tenant read and passes it to `install`. `install` at `:656-679` preserves the highest observed revision, refuses installation across an unknown-invalidation generation, retains a newer usable snapshot when an older load finishes, and returns only state accepted by the common validity rule. `confirm` at `:719-760` similarly captures generation before its read; unchanged-revision confirmation updates freshness only under that same generation and matching installed revision. A changed revision is observed before the awaited reload. A failed read does not refresh the confirmation time.

`Entry::usable` at `:500-507` is the one serving predicate: a snapshot exists, its revision covers observations, its confirmation generation is current, and its successful confirmation is within the five-minute bound. Both ordinary request authorization (`authorize` -> `acquire` -> `cached`) and stream admission/revalidation (`:801-845`) consume that same owner/predicate. No handler-specific workaround or parallel grant rule was added.

The diff removes the installed-entry-only get_mut observation, the malformed-notification clearing loop, `confirmed_at: Option<Instant>` and the old alternate usable implementation. Remaining timestamp, snapshot and revision state each serve a distinct required part of the invariant; credential settlement and restrictive ceilings remain unchanged. The `wyrd-server/src/test_support.rs:334-335` direct installation caller is adapted to the generation-bearing API. The generation pattern reuses the existing gateway cache approach without adding another lifecycle service.

New deterministic tests establish a first load of N cannot serve after N+1 was observed and an unknown invalidation fences both cold and warm pending loads without a numeric fence hiding the issue. Existing warm observation, settled-token and real-Postgres outage proof remain. Fresh executions are recorded below. This fixes the shared cause and removes stale logic as requested.

### FIND-TASK-001-8 — guidance and duplicate snippets

`docs/src/content/docs/operate/add-people.md:9,182-198` now distinguishes optional provider sign-in from local Users, documents explicit-scope invitations and username/password creation, and connects local browser sign-in to CLI device approval. The old no-password/no-invitation and human-Service-key advice is removed. The invitation output/password-stdin descriptions match the existing CLI owner `principal/user.rs:62-105`.

`operate/grant-and-inspect-permissions.md:41` replaces three manually maintained SDK snippets with a short pointer to existing Principals/MCP calls while retaining the CLI grant/show/revoke instructions. Deleting duplicate snippets was explicitly permitted by R2 and avoids new example machinery. The generated `docs/public/llms-full.txt` follows the source change. Recorded docs generation/test/command/link results are supplied for this candidate. The orchestrator additionally ran docs build and accessibility successfully in this review, closing the implementation's stated omissions.

### FIND-TASK-001-11 — previous claim rejected, scoped boundary proved

The previous review inspected the approver's operation bundle but missed the common binding compiler. `wyrd-runtime/src/builtin_roles.rs:86-89` includes cards:read and verifier:approve. `wyrd-spec/src/auth/access.rs:319-327` adds executions:read at the same scope whenever cards:read applies at a Card scope **or All**. `git show 284f80115:crates/wyrd-spec/src/auth/access.rs` confirms this was already true in the prior reviewed candidate; no R2 production change created it.

Therefore an approver bound at All already has tenant-wide parent visibility and can correctly approve the Repository execution. The earlier assertion that the old Python/TypeScript positive tests must fail was incorrect. Retract that diagnosis; do not describe it as a production bug or fixed stale-test regression. Prior reports remain historical records; this correction governs the current disposition.

The new Python `test_deferred_evidence.py:154-194` and TypeScript `deferred-evidence.test.ts:156-181` journeys instead create approver at the shared `Verifier/signoff` family, assert exact RBAC refusal without Repository visibility, bind viewer at `Repository/web` through public Principals, then approve/replay the same execution and preserve actor attribution. Viewer/editor refusal remains. These tests match REQ-006/008 and the existing Rust scoped case, without changing role semantics or production checks. Their supplied candidate-bound language test passes are credible execution evidence; this reviewer did not rerun those language suites.

### FIND-TASK-001-12 — real CLI administration proof

`wyrd-cli/tests/card_lifecycle.rs:3433-3670` boots the existing real-server fixture, creates an ungranted target through the CLI, refuses the target's self-grant with exact RBAC code and no created binding, then executes administrator access grant/show/revoke. Assertions inspect returned binding identity/scope/source and show the target can read only the covered Verifier family after normal propagation. Revocation removes that read using the same existing token.

The same journey executes put-role/create-group/group grant/add-member/remove-member/delete-group/delete-role/roles through the real in-process CLI dispatch, observes group source and the other family's data visibility, and proves removal. It uses existing CLI/harness ownership rather than a mock adapter. The reported C5 pass closes this proof gap; it does not imply there was a CLI production defect to repair.

### FIND-TASK-001-13 — authored fixture inputs

`principal_roles.rs:916,1178,1314` and the public peer journey `distributed.rs:2088` now reference checked-in second-parent/second-release Cards. The four added files are `declare_and_pull/support-model-2.yaml`, `repository_declaration/code-review-2.yaml`, and `scoped_peer_reads/{other-reply,no-refund-promise}.yaml`. The Model reuses its existing binary artifact; the other Service references its checked-in Verifier. The two additional sibling rewrites removed from the same branch had the identical rule violation and reuse these fixtures; this adds no capability or production behavior.

The latest diff leaves the declaration/upload/completion/deletion, exact-versus-family, SQL hidden-parent and actual peer participation assertions intact. Supplied C2a/C2b results establish the four affected journey passes. No runtime Card authoring framework replaces the removed rewriting.

## Reconciled acceptance matrix

The single reconciled requirement matrix is recorded in [verdict.md](verdict.md#reconciled-acceptance-matrix). Stable finding dispositions above preserve correction history and supporting evidence.

## Hot-path waits and retained limit

R2 adds or moves **no PostgreSQL, Scribe, object-storage or Vault wait**. Warm acquire remains a short synchronous memory lookup; generation capture and validity checks use the existing short mutex. Miss/invalidation reload retains its sanctioned tenant-bound authority statement; background confirmation retains its revision statement and changed-state reload. No mutex spans awaited IO. Streams use the same acquisition owner, and audit staging remains nonblocking. Test fixture installation and language/CLI journeys add no serving behavior.

Observation-only entries persist for notifying tenants even if this process never loads their grants. The source documents this existing cache-owner memory tradeoff. It retains one small marker per notifying tenant rather than forgetting an in-flight invalidation; no requirement in this packet demands a different eviction policy. Do not turn it into an unrelated blocker or invent new cache infrastructure.

## Fresh execution and evidence limits

Reviewer execution, candidate unchanged:

```bash
mise exec -- cargo nextest run --locked -p wyrd-auth --lib -E 'test(=authority::tests::a_first_load_never_serves_after_a_newer_revision_is_observed) | test(=authority::tests::an_unknown_invalidation_refuses_a_load_begun_before_it) | test(=authority::tests::an_older_load_never_serves_after_a_newer_revision_is_observed) | test(=authority::tests::a_settled_token_stops_reloading_in_its_issuance_second)'
```

Exit 0, **4/4 PASS**, run `fb619623-0050-46d5-91ce-f730051c3104`, 0.005s test time. These deterministic assertions directly cover the two R2 races, the existing warm observation case and same-second credential settlement.

```bash
mise exec -- scripts/postgres/with-test-postgres.sh -- cargo nextest run --locked -p wyrd-auth --lib -E 'test(=authority::pg_tests::an_outage_serves_only_within_the_bound_then_refuses_as_unavailable) | test(=authority::pg_tests::a_settled_token_needs_no_reload_during_an_outage)'
```

Exit 0, **2/2 PASS**, run `8c5be885-b217-4eaa-a026-a875ed80bc7f`, 0.806s test time. Repository-managed Postgres was available. The first test establishes confirmed snapshot availability inside the outage bound, expiry refusal and cold-cache refusal; the settled-token test asserts memory refusal for an unknown principal without repeat compilation. It is not itself a successful authenticated human outage journey. The separate inspected saved-login/unit proof remains applicable.

Orchestrator execution: `mise run docs:build` exit 0 (static site/Pagefind, 37 pages), then `mise run docs:a11y` exit 0 (37 pages, contrast AA in light/dark themes). No source modification was reported; warnings were Node/plugin deprecations/timing guidance.

Supplied R2 task evidence was checked against actual assertions: C1 records all 11 authority tests; C2a three Rust SDK journeys; C2b the real multi-pod peer journey; C3/C4 two tests per deferred-evidence language file; C5 CLI administration plus existing family read; C6 docs generation/tests/commands/links. Supplied relevant Rust/language checks passed. Those results are implementation evidence, not all fresh reviewer reruns. Builds and plausible source were not substituted for missing journey proof.

`git diff --check` passed. Final HEAD remained `300542877cd2a58cbe5f061d5075616e4cc4397b`. No implementation, architecture, Git identity, commit, push or deployment changes were made. Benchmark qualification and TDD compliance reporting remain excluded by explicit maintainer instruction.

## Verdict and convergence

Behavioral verification: **PASS**. There are no retained findings and no remediation task is required. Original FIND-11 is corrected/rejected, not counted as a successfully fixed regression. All other stable findings are closed, with unchanged evidence carried forward as qualified above.

The shared cache cause is resolved and superseded handling removed. There is no second unsuccessful remediation round to justify an expanded investigation; neither completion nor further work is inferred from a round count. Return the task verdict to the caller. Do not launch TASK-002 implementation or final change review.

Skipped broad suites, fresh language/CLI/peer journey reruns, excluded process/qualification assessment and implementation. Remaining limit: observation markers consume memory proportional to notifying tenant count; no unresolved acceptance defect was identified.
