# FIX_REQUIRED — TASK-001, candidate 3ed5d90ff

**Findings: FIND-001-13, FIND-001-16, and owner-directed FIND-001-17. No specification revision needed.**

Ready, self-contained task:
[TASK-001-R3-portable-media-remediation.md](TASK-001-R3-portable-media-remediation.md).
Route directly to `$wyrd-implement`. Other prior findings close on source;
finding13 now concerns only explicit nonempty-media journey proof, with its
production projection implemented.

**Owner-directed scope update:** the owner subsequently instructed us to add
Agent export as one more finding in this same remediation. FIND-001-17 now
requires preservation of omitted/explicit run configuration and verification
bindings. This supersedes separate tracking; the earlier scope analysis is
retained as history in [agent-export-follow-up.md](agent-export-follow-up.md).
There are two independently validated findings plus this explicit owner addition.

## Immutable cumulative subject and authority

Repository: `/home/thorrester/Documents/GitHub/wyrd-worktrees/deployment-addressing`.
Base: `1d3b5ffa0d34f75fa5b22c9b68f6ec0fc8d0cc70`.
Candidate: `3ed5d90ff53a2ed082decd66bf7aab168a5afdcb`.
Previous candidate: `9516afcdc7d3ffb58d5f20e85c4341d0df3cef23`.
Immutable archived source: `/tmp/wyrd-task-review-3ed5d90f`.

The complete base-to-candidate range was reviewed; the R2 delta located
changed owners without replacing original-task acceptance. Subject/map,
cumulative and remediation diffs, changed paths and commits are preserved here.
Root CodeGraph exploration preceded code discovery; the archive has no index.
Candidate identity stayed fixed. The user approves unrelated adjacent work
and live changes outside this pinned subject; those approvals were respected.

Authority: current owner decisions; AGENTS/agent-rules and applicable
design/doctrine/security/Bifrost/operations/language/testing references;
`changes/active/deployment-addressing/spec.md` approved r6; original
`tasks/TASK-001-deployment-addressing.md`; both prior review verdicts/ledgers
and R1/R2 tasks with committed evidence. The approved docsite-rebuild r2
Connect structure governs the bounded gateway docs. Human finding14 retains
its original owner-directed provenance.

## Independent review and claim reconciliation

| Fresh role | Report | Result | Reconciled meaning |
|---|---|---|---|
| behavior-rev | `task-review-behavior.md` | FAIL | Proposed nonempty-media proof13 and compiler collision16. |
| invariant-rev | `task-review-invariants.md` | PASS | Original route/hash/permission/lifecycle boundaries supported; proof13/collision16 require narrower reconciliation below. |
| repo-rev | `standards-review.md` | PASS | Cumulative/R2 ownership, dependencies, actual SQL capabilities, bindings, generated consumers and rustdoc supported. |
| maintainer-rev | `maintainer-review.md` | PASS | Existing owners reused, R2 code/projection/loader fixes supported; generic Agent export recorded separately. |
| system-rev | `system-review.md` | PASS within scope | Request failure boundaries, actual loaded execution, restart, uncertain dispatch and outbox remain correct. |
| domain security/RBAC | `domain-review-security.md` | PASS | Exact grants/issuance, auth/credentials, local Vertex and offline schema trust boundaries supported. |
| domain data/tenancy/durability | `domain-review-data.md` | PASS within scope | SQL, retained attribution, authored registry storage and bundle data seams supported. Initial Agent export concern withdrawn after scope tracing. |
| domain concurrency | `domain-review-concurrency.md` | PASS | Actual-snapshot batch auth, fences/replay/cancellation, overrides and process-isolated resolver remain correct. |
| domain Prompt/Agent/SDK | `domain-review-prompt-sdk.md` | FAIL | Production fixes supported; nonempty-media proof13 remains. |
| focused followup-rev | `followup-review.md` | RESOLVED | Traced media collision across compiler/normalizer/sibling binders, later turns and message text; resolved explicit proof13 scope. |
| structured ponytail-rev | `findings-validation.md` | Two retained findings | Independently source-validated every claim/correction, prior closure and owner export scope. |

Each required discovery role used a separate fresh agent with the same subject
and map, independently expanded from source without peers' new conclusions.
`claim-comparison.md` groups source claims rather than votes. Follow-up was
required for the newly identified portable-media producer path and disagreement
over empty-only media proof. It was not an extra pass to obtain a desired count.

The production Rust→N-API→TS metadata projection is correct; broad closure
claims do not waive R2's explicit nonempty-media journey requirement. Likewise,
the new compiler identity defect differs from the already-fixed expected-kind
and payload validation causes. Existing binders correctly replace repeated
user names; correction belongs at temporary identity production, not at each
consumer. These conclusions come from independent validation, not majority
agreement. No new product/boundary decision is needed.

## Validated ledger, plainly

| ID / status / classification | Finding | Selected remediation |
|---|---|---|
| FIND-001-13 — REVISED — MISSING verification | The TS journey only checks an empty media-variable list, so it does not prove real media names survive registration and hydration. | Add named Image/Document inputs to the existing portable fixtures and assert exact nonempty names on standalone and Agent-linked paths, retaining JSON/hash/ref/text/schema proof and updating the shared Python expectation. |
| FIND-001-16 — CONFIRMED — INCORRECT | A valid authored media name can match the compiler's temporary literal name, making a fixed attachment replace the caller's input. | Make temporary literal-media identities disjoint from all authored media tokens in the complete portable content before ordinary binding, preserving valid names, repeated binding, order, kind checks and native behavior. |
| FIND-001-17 — OWNER-DIRECTED — INCORRECT export | Saving a loaded runtime Agent adds omitted defaults and loses verification bindings, changing its spec/hash. | Preserve authored configuration representation and verification bindings in the existing Agent conversion owner, keep effective runtime defaults separate, and reflect intentional edits without resetting unaffected fields. |

Exact source, reachable YAML counterexample, producer→consumer/sibling traces,
consequences, alternatives rejected and focused proof are in
`findings-validation.md`. R3 packages those diagnoses and selections without a
new compiler/binder/SDK API/harness. Its existing SDK fixture extension can
exercise mixed literal/colliding-name registration too.

## Prior-finding closure

| Prior ID | Current result and basis |
|---|---|
| FIND-001-1 | CLOSED: explicit grants survive All normalization; complete exact delegated intersection refuses unrepresentable results before signing; mixed/order/empty representable tests remain. |
| FIND-001-2 | CLOSED: Agent loader/to_spec retain route; runtime selects it before IO, fails without host caller and isolates overrides; actual loaded execution is now exercised. |
| FIND-001-3 | CLOSED: full named Anthropic/Google/Jev targets and one-shot Jev preserved; public typed Vertex guard now refuses before transport while server/native Vertex survives. |
| FIND-001-4 | CLOSED at original expected-kind cause: shared binding rejects wrong/conflicting kind before mutation. New16 is a separate temporary identity producer. |
| FIND-001-5 | CLOSED: partial schema walk removed; bundled standard declared-draft meta-validation, custom-draft pre-refusal, strict object-root and no retrieval of user references; all authoring/builders/registry share it. |
| FIND-001-6 | CLOSED: typed provider constraint reaches initial requested-target admission without breaking direct conversion or explicit authorized fallback. |
| FIND-001-7 | CLOSED: snapshot/grants prune before bounded SQL LIMIT, final audited decision uses the same snapshot. |
| FIND-001-8 | CLOSED: canonical literal plus all typed body-model fields checked for agreement before variables/schema derivation; Google model-less body preserved. |
| FIND-001-9 | CLOSED: actual admitted adapter provider remains immutable per attempt, distinct from deployment/model, shared by live/captured mapping. |
| FIND-001-10 | CLOSED: actual dispatch/replay authorizes its own routing snapshot; early claim protection, fresh no-dispatch release and uncertain fences remain. |
| FIND-001-11 | CLOSED at original source boundary: bounded base64/kind/MIME/provider/URI checks are shared before replacement; new16 does not reopen those validators. |
| FIND-001-12 | CLOSED: cumulative prior inventory and R2 changed items have substantive in-place rustdoc; no new missing-item blocker retained. |
| FIND-001-13 | PARTIAL: four-form storage/hash and production metadata projection corrected; nonempty TS media journey proof remains. |
| FIND-001-14 | CLOSED, human-directed: downloaded exact pinned Prompt resolves through existing owner; public Agent::from_card restores actual runtime, inspected then executed with only normal public host caller; same bundle runs after restart with correlated evidence. |
| FIND-001-15 | CLOSED: canonical setup applies metadata capture before request, framework page inherits it, actual framework requests read their own retained rows; server opt-in defaults unchanged. |

## Cumulative acceptance matrix

PASS denotes immutable source plus inspected supplied verification, not fresh
test execution by this review. Each failed row identifies the remaining part.

| Obligation | Implementation evidence | Verification evidence / remaining cause | Result |
|---|---|---|---|
| REQ-001 named addressing | Typed deployment/ModelRef, CallPlan and full native targets | Existing named/slash/restriction fixtures and SDK evidence | PASS |
| REQ-002 shared authority/visibility | PermissionSet, issuer, actual snapshot decision and pre-LIMIT listing | Exact meet/exchange/request/fallback/tenant/listing proof | PASS |
| REQ-003 onboarding | Full-file CLI validation, ordinary ordered PUT, existing sealing/CRUD | Malformed/duplicate/progress/reapply/single/list installed checks | PASS |
| REQ-004 external deployment | Compatible adapter, unchanged suffix, distinct credential provider and pinned screened egress | External/header/revocation evidence | PASS |
| REQ-005 literal identity | Prompt construction compares canonical/native body model before derivation | R2 typed mismatch matrix and slash/model-less positives | PASS |
| REQ-006 portable media/settings/schema | Shared compiler and standard schema/media rules; temporary media collision remains | FIND-001-16 mixed literal/binding counterexample | FAIL |
| REQ-007 native forms | Pair/mixed/body.model checks and option preservation, complete offline schema boundary | R2 schema/model negatives and existing native-option tests | PASS |
| REQ-008 authored storage/shared derivation | JSON/hash and shared projections implemented | Compiler collision16 and nonempty TS proof13 | FAIL |
| REQ-009 route/default/override | Actual Agent loader and runtime/Run route owners retain immutable effective route | Existing route/override/tool/isolation plus actual downloaded loader stage | PASS |
| REQ-010 resolution/failure | Public/local Vertex guard, compatible requested admission and explicit fallback | P1 and existing native ingress/routed failure proofs | PASS |
| REQ-011 consumer completion | Typed SDK/error/generated/server/docs surfaces implemented | Explicit TS nonempty proof remainder13 | FAIL |
| REQ-012 Workflow removed | Retired Card/runtime/client/server/SDK/UI product absent; independent Eval survives | Catalog/negative/judge/current source | PASS |
| REQ-013 installed dev setup | Dev tenant, saved auth, credential/deployment list and restart | P9 real installed server and actual loaded four-provider child | PASS |
| INV-001 tenant isolation | TenantConn/RLS and verified tenant across administration/admission/registry/capture | Actual signatures and tenant-separated tests inspected | PASS |
| INV-002 secrets | Existing sealing/bindings/SSRF pinning/header protections | Header/redaction/revocation/restart assertions | PASS |
| INV-003 no compatibility/weighted replacement | Revised contracts and retired wiring, no shim/alias | Contract/catalog/codegen evidence | PASS |
| INV-004 route/hash identity | Authored Prompt identity and stored Agent unchanged by invocation override; actual targets recorded | Route/override/hydration/capture proof | PASS |
| INV-005 shared ownership | Existing Rust server/Skald/client owners and thin SDKs | Source/dependency/projection inspection | PASS |
| INV-006 existing nonblocking outbox | Existing audit/capture staging/settlement/shutdown path retained | Source/failure-path inspection, no new writer | PASS |
| INV-007 fail-closed route/authority | No native rescue, actual target/override authorization, immutable route | Missing caller/native trap/denial/isolation evidence | PASS |
| AC-001 gateway/onboarding | Same named/shared owners across three SDKs | Original focused and R2 evidence | PASS |
| AC-002 grants/delegation/visibility | Exact intersection, effective subject and current snapshot | Permission/exchange and bounded batch/race tests | PASS |
| AC-003 external gateway | Unchanged model and ordinary auth/pricing/header path | External credential/screen/revoke proofs | PASS |
| AC-004 authored forms/derived metadata | Four-form JSON/hash and production metadata correct | Nonempty TS media metadata unproved, FIND-001-13 | FAIL |
| AC-005 Agent route/default/override | Existing runtime callbacks/tools/context and isolated route | SDK/judge/timeout/refusal and loaded Agent P9 proof | PASS |
| AC-006 negative/media/compiler proof | Earlier schema/model/kind/payload corrections present | Wrong attachment via temporary-name collision, FIND-001-16 | FAIL |
| AC-007 judges | Vala/Skald Agent and separate authorized server caller | Existing structured judge/attribution/native refusal evidence | PASS |
| AC-008 ingress/local/server parity | Named protocols, Jev one-shot and actual local Vertex refusal | P1 and native/Agent tests, server/native Vertex unchanged | PASS |
| AC-009 surfaces/docs agreement | APIs/bindings/docs and served OpenAPI agree in source | Explicit SDK proof13 incomplete | FAIL |
| AC-010 retired Workflow absence | Kind/runtime/headless/UI removed without independent Eval loss | Original negative/catalog checks and cumulative source | PASS |
| AC-011 real installed journey | Single dev/tenant, saved client, four deployments and actual downloaded Agents | P9 same bundle/auth/evidence before and after restart | PASS |
| AC-012 list/restart/convergence | Existing validated per-item onboarding and sealed key/tenant reuse | CLI progress/reapply/revocation and P9 restart | PASS |
| R1-DOC-AC-01 | Connect setup with metadata capture prerequisite | Published source and corresponding existing setup journey | PASS |
| R1-DOC-AC-02 | Actual stock/framework calls and retained-evidence success check | P10 own fresh-server rows, capture-removal mutation recorded | PASS |
| R1-DOC-AC-03 | Connect structure/indexes and rendered evidence with explicit limits | Reported docs:check/Chrome desktop/narrow iframe/theme observation | PASS on supplied evidence |
| Owner FIND-001-17 loaded Agent export | Runtime conversion currently materializes defaults and empties verified_by | Exact load/export/save/reload spec/hash and edit proof now required by owner | FAIL |
| Non-goals | No mappings/aliases/new scopes, hosted jobs, local Vertex, portable audio/video, replacement runtime/graph/transport/queue or fetch subsystem | Cumulative source and accepted adjacent changes | PASS |

## Agent export — original scope analysis, now superseded by owner instruction

Independent validation confirms generic runtime Agent export is lossy: it can
materialize omitted cap8/max_iterations10 and emit empty `verified_by`.
The conversion is wider than a concurrency default patch. Original REQ-008's
exact authored re-emission guarantee concerns Prompt forms; Agent route and
invocation guarantees preserve the stored Card/hash, which the current paths
do. Those generic converter/default/binding bodies are unchanged in the
cumulative executable diff, and no required route writer saves/re-registers
the reconstructed output. This is a source/authority scope judgment, not a
waiver merely because R2 omitted a criterion.

The owner has now expressly included this work as FIND-001-17. Correct it in
the same R3 task at the existing Agent/spec conversion owner with faithful
omitted/explicit configuration, verification binding and intentional mutation
proof. Do not change global defaults, weaken hashing or fix only capNone.
The existing loaded-Agent test's narrowed assertion still checks the actual
identities/route/provider/model and executes that instance; its documented
route-drop failure discriminates the required loader behavior. It does not
claim generic export equality. FIND-001-17 now adds that equality requirement
and the corresponding focused proof; the old narrower test alone no longer
closes the expanded owner-directed obligation.

## Verification limits and conclusion

No reviewer reran runtime tests, builds or browser inspection. Reported R2
passes, test bodies/selectors, route/capture mutation proof and rendered notes
were checked against source; they remain supplied evidence. The media collision
is established by full producer/normalizer/sibling binder/render tracing, with
an exact valid authored counterexample, not a claimed newly executed RED.

The stale Python-extension diagnosis is consistent with the current
fieldless-Bifrost error reconstruction and old binary's502 fallback. Committed
tracing/timestamps, `py:setup` rebuild and P9/P10 reruns explain the failure
without an assertion/source/timeout relaxation. `uv run` alone is not a native
build. Missing historical behavioral RED is honestly disclosed and is not
turned into a current implementation defect.

Rendered evidence records production pages at desktop/narrow iframe content
width in light/dark with attribute-toggle/iframe limits; it is not represented
as a new review execution or grounds for a speculative layout bug. No live paid
provider, release runner, artificial load or older-baseline experiment ran.
The broad integrated gate remains change-review work, not a new remediation
aggregate. Full pinned diff and review Markdown whitespace checks passed.

All mandatory reports and independent validation for the reviewed scope are
complete. Two validated acceptance gaps and one subsequent owner-directed
addition remain; R3 supplies diagnoses, chosen existing owners,
constraints, ordered proof and exact focused commands. No permission or
specification decision is pending. Review changed only its local artifacts;
it did not implement, commit, merge, push or deploy the candidate.
