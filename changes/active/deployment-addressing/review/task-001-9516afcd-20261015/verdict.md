# FIX_REQUIRED — TASK-001, candidate 9516afcdc

**Retained findings:** FIND-001-3, FIND-001-5, FIND-001-8, FIND-001-13,
FIND-001-14 (human-directed coverage), and new FIND-001-15.
**Specification revision required: no.**

Ready, self-contained remediation:
[TASK-001-R2-deployment-addressing-remediation.md](TASK-001-R2-deployment-addressing-remediation.md).
Route directly to `$wyrd-implement`. Review has not implemented, committed,
merged, pushed or deployed anything.

## Immutable subject and authority

Repository: `/home/thorrester/Documents/GitHub/wyrd-worktrees/deployment-addressing`.
Original base: `1d3b5ffa0d34f75fa5b22c9b68f6ec0fc8d0cc70`.
Candidate: `9516afcdc7d3ffb58d5f20e85c4341d0df3cef23`.
Previous reviewed candidate: `ef2fe8bda2071f7b32cb6803b7f9f4599f0d4183`.
Read-only archived source: `/tmp/wyrd-task-review-9516afcd`.

The review covers the complete original-base-to-candidate range, not just the
R1 delta. `cumulative.diff`, `remediation.diff`, changed paths, commits,
`subject.md` and `navigation.md` are preserved here. The archive has no
CodeGraph directory; root CodeGraph exploration preceded source navigation.
The pinned candidate remained unchanged. The owner permits live branch edits
outside this subject and explicitly approves adjacent unrelated changes; those
approvals were honored rather than treated as scope violations.

Authority: current owner decisions, AGENTS.md/agent-rules and applicable
architecture/reference rules; approved `changes/active/deployment-addressing/spec.md`
revision 6; original `tasks/TASK-001-deployment-addressing.md`; prior review's
verdict, findings, approval resolution and TASK-001-R1 with appended evidence.
The approved `developer-docsite-rebuild` revision 2 and its Connect structure
govern the bounded gateway docs addition. Review reports identify the applicable
security, SQL, Bifrost, SDK, testing, operations and maintainer references.

## Independent review results and reconciliation

| Fresh independent role | Preserved report | Result | Reconciled outcome |
|---|---|---|---|
| behavior-rev | `task-review-behavior.md` | FAIL | Schema5, native model8 and TS metadata13 independently retained. |
| invariant-rev | `task-review-invariants.md` | FAIL | Schema5 and actual Agent-loader14 proof retained. |
| repo-rev | `standards-review.md` | FAIL | Shared schema5 retained; prior rustdoc inventory corrected, no new rustdoc round. |
| maintainer-rev | `maintainer-review.md` | FAIL | Schema5 and loader14 retained; unproven stale-batch concern rejected. |
| system-rev | `system-review.md` | FAIL | Loader14 retained; existing batch/recovery boundaries preserved. |
| domain-rev security/RBAC | `domain-review-security.md` | PASS | Exact delegation/normalization and gateway trust/credential boundaries supported closed. |
| domain-rev data/tenancy/durability | `domain-review-data.md` | PASS within domain | Storage/hash, tenant SQL, provider capture and restart data seams pass; this does not waive TS introspection or actual loader proof. |
| domain-rev concurrency | `domain-review-concurrency.md` | PASS | Batch actual-snapshot admission, pre-LIMIT pruning and route isolation supported closed. |
| domain-rev Prompt/Agent/SDK | `domain-review-prompt-sdk.md` | FAIL | Schema5, local Vertex3, native model8, loader14 and docs capture15 retained. |
| focused followup-rev | `followup-review.md` | RESOLVED | Resolved conflicting SDK/task assumptions from approved authority and existing runtime owners. |
| structured ponytail-rev | `findings-validation.md` | Six bounded items retained | Independently source-validated all proposals, rejected unsupported claims, deduplicated and selected corrections. |

Every required role has a separate fresh agent and report; discovery agents
did not receive peers' new conclusions. `claim-comparison.md` groups claims by
cause. A focused follow-up was required because several reports accepted TS
JSON/hash-only proof under a lower private task assumption, and because
hydrated invocation was being treated as equivalent to actual Agent loading.

The follow-up and independent validation resolve both without new product
decisions: REQ-008 already requires TS derived introspection, so extend the
existing thin projection; the owner did not select Python for the bundle
stage, so use the existing public Rust Agent loader/caller instead of adding
a Python host API. R2 explicitly corrects those R1 assumptions. Current TS
connect is synchronous and is not a docs defect. A real Card-scope403 satisfies
finding14's representative denied request; it need not be a new RBAC case.

The public direct local Vertex path and direct native Prompt constructor are
reachable siblings missed by broader closure claims, not new features. Their
specific source chains control over those closure claims. No proposed finding
is retained merely because multiple reviewers agreed. Only the independently
confirmed/revised ledger below determines this verdict.

## Validated findings, explained plainly

| ID / status | Finding in one sentence | Selected remediation |
|---|---|---|
| FIND-001-3 CONFIRMED | A local Rust Agent can still dispatch actual Vertex through the public caller despite the required pre-IO refusal. | Guard the typed provider in PublicWyrdGatewayCaller before transport; preserve Google deployments named `vertex` and the separate server/native Vertex paths. |
| FIND-001-5 REVISED | Invalid output schemas still register because the shared checker ignores many standard keywords. | Replace the partial walk with bundled offline standards meta-validation at the same shared owner, update the existing JSON Schema dependency and its two test consumers, and prove native/portable/builder/registry refusal. |
| FIND-001-8 CONFIRMED | Direct native Prompt construction can render a different request-body model than the literal model it validated. | Enforce one matching literal typed request/canonical model before variable derivation in Prompt construction; retain model-less Google and content rendering. |
| FIND-001-13 REVISED | TypeScript preserves Prompt JSON/hash but does not expose the required derived variables or output schema. | Project the existing Rust-derived metadata through both current N-API hydrated Prompt paths and typed TS surfaces; extend the four-form journey without storing derived fields. |
| FIND-001-14 REVISED, human-directed | The new bundle journey executes a replacement Agent rather than the runtime loaded through the public Agent loader. | Extend the same installed-CLI journey with the existing Rust Agent loader and public gateway host dependency, execute that loaded instance for all four providers, then rerun the same downloaded bundles after restart. |
| FIND-001-15 CONFIRMED, new | The setup guide promises recorded gateway calls but omits the capture-policy command required to record them. | Apply existing metadata-only capture before the first documented request and prove actual framework calls' retained rows, keeping opt-in defaults and existing capture owners. |

Full violated obligations, exact locations, producer/consumer evidence,
consequences, selected correction boundaries and focused closure proof are in
`findings-validation.md` and the self-contained R2 task. No repeated per-SDK
guards, replacement runtime/graph/transport, remote schema retrieval, new
capture defaults or local Vertex support is prescribed.

## Prior-finding closure

| Prior finding | Result at this candidate | Source/proof basis |
|---|---|---|
| FIND-001-1 | CLOSED | Shared grant subsumption preserves explicit gateway grants beside All; complete exact intersection refuses unrepresentable exchanges before signing; real exchange and normalization regressions cover both orders and representable mixtures. |
| FIND-001-2 | CLOSED at original route-loss cause | Agent from_card/to_spec retain route; both runtime entry owners select it before IO; missing caller cannot use native credentials. Combined actual-loader proof remains human14. |
| FIND-001-3 | PARTIAL | Named Anthropic/Google/Jev target projection and Jev one-shot execution corrected; public actual Vertex local sibling remains. |
| FIND-001-4 | CLOSED | Portable media kinds retained; mismatch/conflict rejected before mutation, native placeholder behavior preserved. |
| FIND-001-5 | OPEN | Shared schema language validator accepts malformed known keywords and nested schema locations. |
| FIND-001-6 | CLOSED | Typed Prompt provider reaches requested-target compatibility admission; direct translation and explicit fallback semantics remain. |
| FIND-001-7 | CLOSED | Snapshot/grants prune visible SQL rows before bounded LIMIT; final audit decision uses the same snapshot. |
| FIND-001-8 | PARTIAL | Authored/builder canonical identities corrected; direct typed request-body model bypass remains. |
| FIND-001-9 | CLOSED | Admitted actual provider carried separately from deployment/model into retained/live attempt facts, including fallback. |
| FIND-001-10 | CLOSED | Actual dispatch/replay always authorizes its owned routing snapshot; fresh undispatched release and uncertain fencing remain; deterministic replacement regression exists. |
| FIND-001-11 | CLOSED for diagnosed media boundary | Shared source binder enforces existing size/base64/kind/MIME/URI rules before replacement and is reused by literal compilation. |
| FIND-001-12 | CLOSED | Standards inventory/source review covers original 103 functions and related modules/associated items/constants plus remedial additions; substantive rustdoc present, no missing-item claim retained. |
| FIND-001-13 | PARTIAL | Four-form authored JSON/server hash and Rust/Python derived proof corrected; TS derived projection/proof remains. |
| FIND-001-14 | PARTIAL, owner-directed | Real registration/download/fresh saved auth/upstream headers/evidence/restart added; actual Agent loader remains bypassed. Original human-directed provenance retained. |

## Reconciled cumulative acceptance matrix

PASS means immutable source plus inspected supplied verification supports the
obligation; it does not mean this review reran those tests. A FAIL identifies
the remaining part, not a claim that all related implementation is absent.

| Obligation | Implementation evidence | Verification evidence / remaining gap | Result |
|---|---|---|---|
| REQ-001 named deployment/model identity | Deployment/ModelRef contracts, CallPlan, ingress and capture owners | Named ingress and same-adapter target assertions | PASS |
| REQ-002 access/delegation/visibility | PermissionSet/issuer, current-snapshot decide and bounded SQL listing | Exact meet/refusal, exchange, pagination and batch replacement regressions | PASS |
| REQ-003 credential-first single/list onboarding | CLI complete-file validation/ordered put, existing management/sealing | Installed malformed/duplicate/missing-credential/reapply/progress checks | PASS |
| REQ-004 external gateway | Existing compatible adapter, separate credential provider and screened transport | External unchanged-model, auth/header/revocation evidence | PASS |
| REQ-005 literal provider/model | Authored/builders corrected; direct Prompt constructor retains renderable body target | FIND-001-8 counterexample absent from current regression | FAIL |
| REQ-006 portable content/settings/schema | Shared compiler/media/settings complete except schema checker | FIND-001-5 accepted malformed standard keyword | FAIL |
| REQ-007 native forms/options | Pair/body/option owners preserved; native schema and raw-model acceptance gaps | FIND-001-5/8 | FAIL |
| REQ-008 authored preservation/derived shared compilation | Authored JSON/hash preserved; shared Rust derivation exists | Schema5 and missing TS projection13 | FAIL |
| REQ-009 required route/default/override | Agent retains route, shared invocation immutable and authorized | Original route behavior supported; added combined loader proof tracked human14 | PASS for original contract |
| REQ-010 execution resolution/failures | Named ingress, initial target matrix, explicit fallback and server/native contexts | Public local Vertex bypass, FIND-001-3 | FAIL |
| REQ-011 consumer completion | Derive-backed contracts/errors/SDK/server paths | Schema5, TS13, published docs15 | FAIL |
| REQ-012 Workflow removal | Retired Card/runtime/client/server/SDK/UI wiring removed; Eval remains | Retired capability negative/catalog checks and surviving Agent/judges | PASS |
| REQ-013 installed CLI setup | Existing dev tenant/credentials/deployments/saved client/restart path | Original T13 supported; owner14 extends its proof below | PASS for original setup |
| INV-001 tenant isolation | TenantConn/RLS, verified tenant snapshots/credentials/registry | Tenant-separated gateway/registry checks and actual capabilities inspected | PASS |
| INV-002 secrets | Existing managed bindings/endpoint screening/header separation | Header/redaction/restart/revocation assertions | PASS |
| INV-003 no compatibility/weighted/retired surfaces | Revised typed contracts and removed wiring | Removed-field/kind negative and catalog/codegen evidence | PASS |
| INV-004 identities and immutable effective route | Authored/hash/route/capture owners mostly corrected | Direct request-body model disagrees with canonical identity, FIND-001-8 | FAIL |
| INV-005 shared Rust ownership | Existing server/Skald/client/SDK boundaries retained | Independent standards/owner/dependency inspection | PASS |
| INV-006 nonblocking existing outbox | Existing decision/capture ScribeOutbox path unchanged | Source path, no new injected failure execution claimed | PASS |
| INV-007 no hidden fallback/escalation | Gateway fail-closed/authorized overrides retained | Renderable native request-model target, FIND-001-8 | FAIL |
| AC-001 three-SDK named gateway/onboarding | Existing shared CRUD/runtime/credential paths | Original focused admin/inference/journey evidence | PASS |
| AC-002 exact access/delegation/tenant visibility | Shared algebra/issuer and actual admission/listing | Normalization, real exchange, bounded pagination/race source and recorded passes | PASS |
| AC-003 external gateway | Existing unchanged target/credential/auth pipeline | External and screened/revoked credential regressions | PASS |
| AC-004 three-SDK authored forms/introspection | Four-form JSON/hash corrected | TS missing derived variables/schema, FIND-001-13 | FAIL |
| AC-005 Agent default/override/native/gateway/external | Existing loops/contexts/overrides/denial/cancellation paths supported | Original route proof supported; explicit owner14 actual-loader stage incomplete | FAIL for extended proof |
| AC-006 validation/settings/media/native negative proof | Compiler/binder corrections present | Schema5 and native model8 | FAIL |
| AC-007 surviving Agent judges | Vala/Skald/shared server caller, terminal native refusal and attribution | Existing judge/inference route and structured-result evidence | PASS |
| AC-008 native ingress/local-server boundary | Anthropic/Google/Jev projection and Jev one-shot corrected | Actual local Vertex still reaches public transport, FIND-001-3 | FAIL |
| AC-009 docs/contracts/SDK parity | Served OpenAPI/generated/error/binding paths mostly align | TS13 and canonical docs/evidence15 | FAIL |
| AC-010 retired Workflow absent | Registrable kinds, runtime routes, SDK/UI/catalog removal | Negative runtime/catalog checks, independent Eval preserved | PASS |
| AC-011 installed CLI/fresh client/extended Agent stage | Original direct setup/evidence and downloaded bundle restart present | Actual Agent loader bypassed, owner FIND-001-14 | FAIL for extension |
| AC-012 validation/convergence/restart | Whole-file validation/progress and saved tenant/key restart | Original list/restart supported; same downloaded actual-Agent rerun still human14 | FAIL for extended proof |
| R1-DOC-AC-01 | Published Connect setup exists | Omitted required capture command, FIND-001-15 | FAIL |
| R1-DOC-AC-02 | Complete stock-client/framework inference examples/tests exist | Framework retained-row proof missing, FIND-001-15 | FAIL |
| R1-DOC-AC-03 | Connect publication/index/docs/light-dark automated checks recorded | Manual narrow/desktop/theme inspection incomplete; no layout defect observed | NOT ESTABLISHED |
| Non-goals | No new scopes, mappings, external bypass, portable audio/video, hosted jobs, Workflow substitute or replacement platform | Approved adjacent changes accepted; actual local Vertex violation is FIND-001-3, not permission to add support | FAIL only for local Vertex boundary |

## Verification and practical limits

Implementation's final-head format/lint/codegen/dependency/Python typing/TS
lint/docs/OpenAPI and focused GREEN results are preserved in the R1 packet.
Reviewers checked source/selector/assertion strength rather than treating
summary PASS as proof. Compile-only RED and post-fix test additions remain
honestly labeled; no retrospective RED, older-baseline run, blame investigation
or current-behavior finding was manufactured from that history.

This review ran `git diff --check` successfully on the full pinned range and
review artifacts. It did **not** rerun behavioral tests/builds, paid providers,
release ARM jobs, fault/load experiments or the integrated aggregate gate.
The latter remains change-review work, not a broad remediation requirement.
Existing independent source reports cover the full task and sensitive seams.

For the missing visual evidence, the orchestrator started the existing docs
dev command and attempted the computer-use browser. It reported no browser
available and an empty browser/app inventory; the dev process was stopped.
No rendered layout/code-copy inspection was possible. This is a concrete
verification limit, not evidence of broken layout, and is included in the
same bounded docs correction rather than a separate speculative finding.

No user decision is pending. All retained items have selected correction
owners, preserved behavior, focused commands and acceptance evidence in R2.
The existing source remains unchanged by review. Task completion is withheld
until a subsequent immutable cumulative review establishes closure.
