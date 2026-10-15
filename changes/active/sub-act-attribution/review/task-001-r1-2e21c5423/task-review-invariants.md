# TASK-001 / R1 independent invariant review

**Result: PASS. Proposed material findings: none.**

## Subject and method

- Repository: `/home/thorrester/Documents/GitHub/wyrd-worktrees/sub-act-attribution`.
- Original base: `b22718f2cf5da0ebcb7f26bc376157e5eec7768f`.
- Cumulative candidate: `2e21c5423036460d5da2ca22a51efde63635e352`.
- Authority: approved `changes/active/sub-act-attribution/spec.md` r6, original TASK-001, prior verdict and validated FIND-1–9 ledger, and TASK-001-R1. R5 supersedes FIND-3; r6 explicitly accepts the local registered Workflow Card check. The original task's older revision label and older implementation evidence do not override these decisions.
- Read the task-review skill and immutable subject/common map. Used CodeGraph before code navigation, then cumulative diff and actual producer/consumer bodies. No current peer report or conclusion was received. The remediation diff located closure changes; acceptance remained original base to candidate.
- Applied AGENTS §§2–12, 15–16, agent rules, specification workflow, maintainer style, reference router/testing guidance, and the applicable identity, Workflow, Verifier, audit, ingest and telemetry portions of Wyrd design, doctrine, security posture and Bifrost design. Inspected manifests and canonical mise task definitions; this review depends on no newly assumed dependency behavior.
- HEAD was rechecked as the candidate and tracked source remained clean. Only this assigned report was written. No source, task, previous review, Git identity, or approved specification was edited.

PASS below means inspected source plus credible reported verification supports the obligation. It does not mean this reviewer independently executed runtime checks. Original V1–V11 and R1 P1–P15 identify the exact commands in their task records.

## Values, owners, siblings and lifecycle

### Accepted request to protected consumer

`wyrd-runtime/src/request_context.rs:55–108` projects the verified top-level subject and last initiator-first actor without using actor history as permission input. `AcceptedAuthority::new` copies the already effective Principal and verified chain, explicitly dropping credential identity. It does not parse a token or resolve fresh grants. `server/src/components/auth/caller_extractor.rs` preserves the verified Principal/chain together at HTTP/MCP entry; `Caller::accepted` restores the stored authority, and `executed_by_system` changes only execution attribution.

Both direct Task and direct Eval go through `components/verification/service.rs::execute` and `verification/direct.rs` using that caller with SYSTEM execution attribution. Queued Eval builds `Caller::accepted(&run.authority, system)` in `verification/eval.rs:200–207`; it no longer recovers the observation writer. `components/gateway/workflow.rs::ServerWyrdGatewayCaller::call` still enters `GatewayInvocation`, whose admission evaluates the retained effective Principal. `AdmittedCall` freezes both that Principal and the pair; `gateway/ledger.rs` uses the Principal for limits/budgets and records the pair on accounting. SYSTEM never supplies permissions. Missing trusted judge caller has no ambient credential fallback.

### Queue authority and origin identity

`wyrd-sql/src/queries/verifier_runs.rs:584–629` keeps the row pair separate from accepted authority: manual acceptance uses the verified pair; background acceptance uses owner/SYSTEM while retaining owner authority. Manual enqueue takes pair-scoped advisory locking and lookup (`:1200`); registration's existing lookup/conflict owner and unique constraint likewise include both identities. Replay returns existing work without replacing its authority, after server admission. Background binding/source/occurrence keys and Workflow subject ownership remain distinct.

The shared insert and observation bulk insert serialize authority alongside their pair. Claim (`:1569`, `ClaimedRunRow::into_claimed` at `:2226`) reads that JSON; retry, lease renewal, release and settlement update lifecycle fields without rewriting it. Run views select status/pair/result/error, not authority. Cause trace context remains in the defining migration and typed claim. Neither trace nor row payload creates authority.

`BackgroundAuthority::resolve` (`verification/authority.rs:232`) reads active SYSTEM and calls the existing tenant issuer's `recover` on the same tenant transaction. It returns no accepted run when an identity cannot resolve; SQL/connection failure remains retryable. Its two production acceptance consumers are scheduler and observation sink. The scheduler advances/refuses one locked occurrence under its existing cursor and metric semantics. `ObservationRunSink::write` (`observations.rs:103–145`) logs tenant/binding/owner-Card identity for every missing/inactive/foreign owner or missing SYSTEM, consumes that nonretryable binding refusal, and continues healthy sibling bindings. It does not fail the shared server, retry the whole slice for an invalid owner, or invent an audit decision.

### Internal reads, result writes and server spans

`SystemReadAuthority::resolve` retains the existing credentialless SYSTEM principal with only input-table UID grants. `serving(&AcceptedAuthority)` (`authority.rs:176`) now adds the accepted subject and chain through the existing Oracle context fields. Drift and Eval both call this producer. `AuthorizedQueryContext` serialization carries these fields to peer Oracle; `oracle/mod.rs::query_audit_event` selects served subject/SYSTEM without consulting the chain for permission. The caller gains no general input-table grant.

Direct result staging (`components/verification/service.rs:538`) uses the original caller pair; queued result construction (`verification/runner.rs:731`) uses run subject/SYSTEM. Both use the same result builder and Scribe outbox. Managed `card_uid` is the Card judged; `verifier_card_uid` is separate, with origin links left in their own fields. `ScribeSink` groups frames by tenant, destination, pair, request and schema (`scribe_outbox.rs:669`), binds the pair into the content-derived batch ID, and forwards it unchanged locally or over private authenticated capture peer ingress. Retrying an identical slice retains the same identity. Audit content is not overwritten by the transport sentinel.

`wyrd-telemetry/src/export.rs::TenantRoutes::rebind` changes the operation subtree's route on the same bounded export owner, while `ServerWyrdGatewayCaller` binds that route from its trusted caller. Sampling/linked-trace eligibility remains the prerequisite's decision; trace IDs and parents never supply the pair. Request spans keep A/A or A/B; gateway judge spans get A/SYSTEM.

### Inventory creation and ingest

The named Card, service-account, API-key, operator-connection and Bifrost-catalog inserts now receive the pair. Their update/upsert branches preserve the first creator. `TenantProvisioning` forwards its platform operator pair through the existing SYSTEM SQL function and `BuiltinTables::ensure_tenant`; startup `reconcile`, fixture seeding, defining SQL backfill and reserved audit-owner creation explicitly pass `Attribution::unresolved()`. SQL conflict handling and the catalog cache/locked registration owner retain first creation values. These are the r5-approved lookup exceptions, not tenant authority grants.

Gate's verified `AuthContext::attribution` supplies both HTTP/canonical and gRPC/native Scribe frames. Scribe admission retains tenant/Card checks; decode reserves the pair before stamping, and managed arrays write both non-null values (`scribe/execution_lanes.rs:541,1084,1228`). Policy-aware projection keeps audit's own pair as content and applies managed stamping only where the existing Observation policy requires it. Variant boundaries remain. Claim sampling changes only the existing stratum column to subject. The SDK reservation copy in `wyrd-queue` and shared client/table declarations agree.

API-key completion now uses the verified pair and `audit_delegation_chain` at `wyrd-auth/src/issue_api_key.rs:155`; the sole route passes its verified chain. Sibling direct issuance producers remain direct. General absent/malformed/unverifiable bearer failures remain unaudited under r5; existing exchange-refusal audit producers use the unresolved sentinel when no verified identity exists. Platform payload identity stays distinct from reserved audit storage.

### Workflow and minimum machinery

Registered local hydration returns the root metadata UID through the existing `CardGraphHydrator::load_workflow`; shared `Workflow` retains it and `SelectedRoutes::dependencies` reuses `PublicWyrdGatewayCaller::with_subject(None, card_uid)`. Authored/YAML Workflows retain no fabricated registered UID. The existing gateway requested-Card check refuses out-of-scope Card-bound callers; hosted preparation retains its existing captured-authority path and Workflow correlation. No Workflow run ID is substituted for an application Run.

The added value/context machinery earns its scope: `Attribution` expresses one pair; `AcceptedAuthority` is required durable credential-free captured authority; `RunAuthority` distinguishes accepted row actor from execution authority; `BackgroundAuthority` retains and reuses the existing issuer; served-subject/operation-route changes extend existing Oracle/telemetry owners. No second resolver, executor, permission engine, graph store, queue, audit writer, loader, retry system, token issuance path, dependency or Cargo feature was introduced. Outbox simplification removes the former result-only attribution envelope rather than keeping competing branches. R1 fixes invalid state at its existing producer, including the shared read-history producer, instead of adding repeated downstream guards.

## Complete obligation matrix

Paths below are repository-relative; `server/` abbreviates `crates/wyrd/wyrd-server/src`, and SQL/runtime/Bifrost owner names identify the full paths discussed above.

| Requirement, acceptance criterion, constraint or non-goal | Implementation evidence | Verification evidence | Result |
|---|---|---|---|
| REQ-001 non-null, server-derived pair on inventory | Defining SQL, `Attribution`, Gate frame, managed arrays, audit content | Original V4/V5/V9, SQL lane; R1 P1/P2/P5 | PASS |
| REQ-002 direct/delegated/background/judge/Workflow/creation identities | Caller, RunAuthority, direct/queued result owners, Workflow preparation, provisioning and sentinel entry points | V1/V2/V5/V6; R1 P1–P8/P11/P15 | PASS |
| REQ-003 accepted sub authority, intersection, SYSTEM grants nothing, subject accounting | Effective Principal retained into GatewayInvocation and LedgerCall; narrow SYSTEM reads separate | V1/V2; P7 asymmetric/further-hop denial; P10 recovered denial | PASS |
| REQ-004 Card roles and local/hosted Workflow correlation | Result/run/binding projections; retained registered Workflow UID; null application Run | P3 direct/delegated and out-of-scope refusal; hosted regression; V7/V11 joins | PASS |
| REQ-005 named SQL/Bifrost schemas and creator immutability | Defining declarations, insert/update/read owners; audit policy remains content-owned | V4/V5/V9, SQL, codegen; P1/P2 | PASS |
| REQ-006 public contracts, SDK projections, reserved pair, no aliases | `wyrd-spec` wire fields/constants; shared client and language projections; queue reservations | V5/V9, Python/TS checks, codegen; P12 served OpenAPI | PASS |
| REQ-007 active tokenless tenant SYSTEM and correct creator | Existing provisioning SQL/wrapper, fixture seed; caller/sentinel explicit | V2 pre-fixture active SYSTEM assertion; P1/P2 | PASS |
| REQ-008 durable frozen authority/history, per-call checks, observable refusal, no credential/public snapshot | Queue insert/claim/retry; Caller::accepted; BackgroundAuthority; observation diagnostic; serving context | V3/P9; P4/P6/P8/P10 and shared read-context test | PASS |
| REQ-009 pair-scoped registration/manual replay; background/Workflow semantics preserved | SQL locks/lookups/indexes; no accepted-authority update; unchanged origin/Workflow keys | V4 exact replay/conflict/identity scope; queue lifecycle and P9 | PASS |
| REQ-010 authenticated audit/history, platform storage separation, approved sentinel, no pre-identity bearer event, nonblocking path | Audit builders, IssueApiKey, Oracle read context, exchange producers, outbox | V6/V11; P5/P6/P11; existing outbox loss/refusal proof | PASS |
| REQ-011 every realtime hop and further delegation retains subject/effective authority | Existing verified-token intersection passed through Caller/direct/Eval/gateway; no reconstructed subject grant | P7 further delegation and both asymmetric denials; P10 durable protected consumer | PASS |
| INV-001 only verified/owner/approved-sentinel sources | Caller/AuthContext and explicit r5 creator entry points; no client pair input | V5/V9 reservation; P1/P2/P11 | PASS |
| INV-002 SYSTEM actor only | RunAuthority/background, executed_by, input-read attribution; SYSTEM creator no longer self | V1/V2; P1/P2/P6/P8 | PASS |
| INV-003 unchanged tenant isolation | TenantConn/RLS owner and SYSTEM lookup; per-tenant queue/read/catalog boundaries | SQL isolation, tenant check; P4 foreign owner refusal | PASS |
| INV-004 origin links never principal substitutions | Typed run/Card/binding/source/trigger fields; separate pair | V1/V2/V4/V7; P3/P8 | PASS |
| INV-005 trace only observability | Trusted bind/rebind and prerequisite route eligibility | Telemetry operation-subtree test; V7/V10/V11 | PASS |
| INV-006 pair never authority/Card/resource grant | Existing permission evaluator and requested-Card checks remain; served-subject attribution only | P3 out-of-scope/no provider; P4/P7/P10; shared narrow-read test | PASS |
| INV-007 creator/accepted authority immutable | Conflict/update projections omit pair replacement; lifecycle SQL does not rewrite snapshot | V3/V4/P9; P1/P2/P10 | PASS |
| AC-001 scheduled real run/result owner/SYSTEM | Scheduler → RunAuthority → runner/results | P8 real scheduled Custom Drift (supported schedule origin) | PASS |
| AC-002 observation owner over writer, owner permission/cost | Observation bindings resolve owner; Eval consumes owner authority; ledger uses owner Principal | V2/P4 permitted owner and denied owner, different writer, one provider request | PASS |
| AC-003 direct gateway/verify and audit pair across languages | Caller/capture/result/audit and common client owner | V1/V5/V6/V7/V9/V10; affected language realtime journeys | PASS |
| AC-004 delegated gateway/audit A/B | Last verified actor projection, common audit/capture | V1/V5/V6 and P3 | PASS |
| AC-005 both reserved fields refused in each SDK | Shared queue/table reservations plus server decode | V5/V9 real write/flush/read/refusal paths | PASS |
| AC-006 complete old-name disposition | No live old writer aliases; remaining baseline `verifier_uid` identity keys and locals are not attribution; migration filename comment is historical | Original inventory, cumulative source search and codegen | PASS |
| AC-007 defining schemas only, empty DB, deleted incremental migration | No newly added migration; trace context folded into defining verifier run table | Original `test:sql`; P1/P2 fresh databases | PASS |
| AC-008 fixture active SYSTEM | `wyrd-dev-fixtures/src/pg.rs` existing seed owner | V2/P4 reads seeded SYSTEM before fixture provisioning | PASS |
| AC-009 narrow write-set proof, real gateway/language journeys, boundary/schema/docs checks | Existing task map; R1 adds omitted seams and served contract/example proofs | V1–V11 and owner/static/language checks; P1–P15, docs/codegen/deps/tenancy reported green | PASS |
| AC-010 request pair versus judge pair spans; result/Card joins | operation route binding and managed Card role projection | V7/V10/V11, telemetry unit proof | PASS |
| AC-011 local/hosted direct/delegated Workflow Card and null Run; out-of-scope refusal | Shared Workflow UID transport and unchanged requested Card check; hosted attributed caller | P3 local assertions plus refusal/no extra upstream call; hosted proof; Python/TS P15 | PASS |
| AC-012 actor lacks permitted subject model → denial | Retained intersection reaches gateway | P7 C-for-A denial and provider count | PASS |
| AC-013 retry/restart snapshot, grant edits/expiry cannot widen, A/SYSTEM judge | Queue JSON restore into actual protected GatewayInvocation | P9 retained public queue/retry proof plus P10 actual claim → protected gateway after verified token expiry/grant edit | PASS |
| AC-014 replay identities/conflicts/unchanged authority and independent origin keys | Pair-scoped existing replay owners; unchanged origin keys | V4, P9 storage/retry | PASS |
| AC-015 platform pair, unresolved exchange sentinel, unaudited bad bearer, nonblocking audit | PlatformAuthorization, exchange/audit builders, existing authentication refusal path, sole outbox | P11/V6, workload wrong-audience audit, platform integration and outbox tests | PASS |
| AC-016 every writer/read projection, HTTP/gRPC, spans, subject sampling/Variant unchanged | Gate frame, Scribe arrays/peer, SQL creates and audit/read fixes; policy-aware columns | V5/V9/V7/V10/V11; P1/P2/P5/P6; gRPC/peer/managed owner tests | PASS |
| AC-017 owner/SYSTEM fail acceptance observably, no foreign fallback, narrow reads/history, immutable queue, Operator rechecks | BackgroundAuthority + observation diagnostic, existing scheduler skip, serving context and lifecycle SQL | P4 missing/inactive/foreign owner and missing SYSTEM with healthy siblings; P6/P8/P9/P10 | PASS |
| AC-018 original subject every hop, both asymmetric failures, further delegation cannot restore, no ambient authority | Direct/queued callers retain effective Principal; gateway remains protected owner | P7 direct/delegated Task/Eval success, both negatives and D-for-C-for-A negative; trusted-context boundary and P10 | PASS |
| AC-019 authenticated platform creators; callerless sentinel; first pair immutable | Provisioning → SYSTEM wrapper + BuiltinTables/catalog; autonomous entries explicit | P1 persisted operator/sentinel/restart/reserved owner; P2 backfill/seed/retry | PASS |
| No new migration/backfill compatibility mechanism, permission/role/token policy, dependency/feature/check, queue/store/writer | Cumulative source/manifests; defining SYSTEM backfill is existing and r5 explicitly approves its sentinel input | Diff/static inspection; codegen/dependency checks reported | PASS |
| Preserve Variant policy, sampling probability, tenant/Card restrictions, cancellation/leases, operational lineage, Operator rechecks and Workflow lifetime/idempotency | Existing owners retained; only attribution column/stratum/context inputs change | Source comparison plus affected owner/journey evidence | PASS |
| Preserve prerequisite trace continuity, links, sampled eligibility, bounded nonblocking export and timeline | Existing telemetry owner extended; cause context retained; no trace authority inference | V7/V10/V11 and existing export tests | PASS |

## Prior stable finding closure

| Prior ID | Current disposition and source evidence | Closure proof inspected |
|---|---|---|
| FIND-TASK-001-1 | CLOSED under approved r5. SYSTEM defining function takes creator pair; callerless backfill supplies sentinel; provisioning forwards real pair to SYSTEM and built-ins; reconcile/fixtures explicitly unresolved; upserts preserve first pair | P1/P2 assertions read persisted creation values and retry/restart behavior |
| FIND-TASK-001-2 | CLOSED under approved r6. Registered UID retained at shared hydration → Workflow → SelectedRoutes → existing public gateway subject; Card check unchanged | P3 now asserts exact UID/null Run for direct/delegated paths and stable out-of-scope refusal with no provider call; hosted and P15 regressions |
| FIND-TASK-001-3 | SUPERSEDED by approved r5 REQ-010/AC-015, not silently implemented. General failed bearer authentication remains unchanged; exchange auditing remains | P11 preserves missing/malformed/forged no-audit behavior and verified allowed/denied/platform pairs |
| FIND-TASK-001-4 | CLOSED. `ObservationRunSink::write` records binding-specific refusal for both optional-owner absence and resolver refusal, continues authorized bindings | P4 removes/moves/suspends owner rows, captures exact refused owner-Card set, verifies two healthy runs, then missing SYSTEM/no new runs |
| FIND-TASK-001-5 | CLOSED. Route passes verified chain to `IssueApiKey::audit`; shared pair/history projection populates credential detail, without secret fields | P5 matches retained completion event by issued key ID, creation pair and history, and checks secret absence |
| FIND-TASK-001-6 | CLOSED. `SystemReadAuthority::serving` takes complete accepted authority; both Drift and Eval pass it; Oracle carries it in existing serialized context | P6 real delegated queued Drift read A/SYSTEM with [B]; shared narrow grants + JSON round-trip proof covers Eval's same producer |
| FIND-TASK-001-7 | CLOSED. Required proof now exercises supported real scheduled Drift, multi-hop realtime denial, identity refusals, and claimed snapshot into the real protected gateway after grant edit/expiry | P4/P7/P8/P9/P10 inspected; P12 served OpenAPI, P13/P14 changed support-desk examples, docs check reported |
| FIND-TASK-001-8 | CLOSED for enumerated items. Registration query rustdoc describes absence/conflict/error/transaction contracts, ParsedCardRow conversion and replay have Errors sections, named new tests have panic contracts | Cumulative documentation diff/body comparison; fmt/lints reported green |
| FIND-TASK-001-9 | CLOSED. Wyrd design and docsite distinguish direct caller-pair results from queued subject/SYSTEM; Bifrost peer prose describes the universal pair and audit content/transport separation | Compared docs to direct stage_result, queued result, capture, proto and peer/outbox bodies; docs check reported green |

## Findings and verification limits

No material MISSING, INCORRECT, DRIFT, VIOLATION or REGRESSION proposal remains in this invariant review. No new FIND ID is requested. Closure of prior findings is based on current source and actual assertion paths, not the remediation's PASS labels. The short-lived-token proof polls until authentication actually refuses the initiating token; it does not treat an arbitrary sleep as expiry. The protected-consumer seam is appropriate supporting proof because public queued manual work supports Drift, which has no judge; no queued/scheduled Eval capability was invented to manufacture a test.

The reported RED/GREEN diagnoses were checked against source. Owner-role removal corrects the fixture's broad built-in grant rather than weakening production authorization. The P10 User subject correction matches the existing Card-scope boundary. The scheduled test retains the existing database delay and does not loosen timing to force a pass. R6's allowed-caller fixture change accompanies a preserved negative out-of-scope assertion. No new runtime failure was generated or diagnosed here.

Skipped runtime/build/check reruns and live external infrastructure; all execution results remain reported evidence. Final integrated change review still owns the complete affected capability gates and broad language suites.
Risk: this PASS is a static acceptance assessment of the immutable candidate, not an independent runtime or deployment attestation.
