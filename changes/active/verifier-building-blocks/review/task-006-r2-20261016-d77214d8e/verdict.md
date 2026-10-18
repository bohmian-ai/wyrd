# TASK-006 R2 verdict

**FIX_REQUIRED — FIND-TASK-006-4 remains partially open.** The independent validator confirms one bounded reference-consumer defect. Findings 1–3 are withdrawn or closed as described below. This review does not accept TASK-006 or its R1 remediation.

## Immutable subject

Repository: `/home/thorrester/Documents/GitHub/wyrd-verifier-building-blocks`.

Candidate source was isolated at detached `/tmp/wyrd-task006-review-d77214d8e`. Reviewed inclusive span `f711c6834..d77214d8e`; exclusive base `b323327f4cbb690628303d3a6b5249a67af50c9b`; candidate `d77214d8eb119fc26ff114f4ef4a403bc3d3e426`. The first implementation commit is included. Prior candidate `380663d83f3d120d3833ea807d29b7444929d392`; its fix diff located remediation but did not replace the cumulative audit.

Authority: approved `changes/active/verifier-building-blocks/spec.md` revision11; original `tasks/TASK-006-verification-vocabulary-cutover.md`; prior review/validation and `review/task-006-20261016-380663d83/TASK-006-R1-vocabulary-and-consumer-corrections.md`; repository/architecture authority mapped in the reports. Spec11 and committed trailing packet notes supersede historical scan requirements. The repository-wide vocabulary scanner and AGENTS exception were explicitly withdrawn by the owner. This verdict requires neither to return.

Candidate source stayed pinned and clean. Original-tree uncommitted packet amendments and owner-amendment-r11.md were excluded and untouched. Separately integrated local-dev-setup planning is unrelated collaborator context. Reports contain no diff, patch or source archive.

## Independent review and claim reconciliation

| Required fresh role | Report | Discovery result |
|---|---|---|
| Behavior implementation | task-review-behavior.md | FAIL, residual prior finding4 |
| Invariant implementation | task-review-invariants.md | FAIL, INV-R2-4 |
| Repository standards | standards-review.md | FAIL, STD-R2-1 |
| Maintainer | maintainer-review.md | FAIL, MAINT-R2-001 |
| System resilience | system-review.md | PASS, empty proposed ledger |
| Security/tenancy domain | domain-review-security-tenancy.md | PASS, empty proposed ledger |
| Data/durability domain | domain-review-data-durability.md | PASS, empty proposed ledger |
| Contracts domain | domain-review-contracts.md | FAIL, CONTRACT-R2-4 |
| UI/reference domain | domain-review-ui.md | FAIL, residual finding4 with wider proposed scope |
| Focused follow-up | followup-review.md | RESOLVED, narrowed reference dependencies |
| Independent structured Ponytail validation | findings-validation.md | FIX_REQUIRED, one REVISED residual finding4 |

All reports are complete. Root orchestrated subject, routing and packaging; the fresh validator independently adjudicated every proposal against source. Agreement was not treated as proof. `claims-comparison.md` records grouping and the conditional follow-up decision.

Follow-up was necessary because the UI proposal extended into generic Workflow inventory/dispatch cleanup while other reports limited correction to evaluation-specific references. It traced identities, direct sections and current-reference authority. Independent validation retained the obsolete evaluation panels and direct consumers, rejected mandatory C-01/C-14/Service generic Workflow cleanup, and excluded E-07's unrelated review-loop link. No new finding ID or specification decision was needed. Rejected proposals are not optional remediation advice.

## Validated ledger and prior-finding closure

| Stable ID | Status at this candidate | Validated conclusion |
|---|---|---|
| FIND-TASK-006-1 | Repository-scan portion WITHDRAWN; remaining served-contract portion CLOSED | Spec11 removes permanent scanning. The actual served OpenAPI assertion now detects expanded product names as well as abbreviations/routes while preserving ordinary prose and faithful standard projections. |
| FIND-TASK-006-2 | CLOSED | Rust accessor docs describe its pure VerifierSpec; Python/TypeScript describe their Card envelope and actual spec.blocks access. |
| FIND-TASK-006-3 | CLOSED | Bounded new/extracted/adapted rustdoc and panic contracts corrected, including new served helper/constant/test. Unchanged moved tests/import-only callers remain excluded. |
| FIND-TASK-006-4 | REVISED / INCORRECT, residual OPEN | Current product-reference C-08/E-07/E-17 still prescribe the retired evaluation definition/task/score/result ontology; direct E-04/E-07/E-16/EM-02 section consumers continue it. Named panel/golden retirement, supported Card links and mobile collision are closed portions. |

**FIND-TASK-006-4 exact boundary:** under `crates/wyrd/wyrd-server/wyrd-ui/brand/renders/product/`, README:3–7/167–175/524–528/584–588 and directly affected ledger/count entries; cards.svg:1 C-08-light/dark; cards/experiment/outputs.svg:1 E-17 and E-16; cards/experiment/runs.svg:1 E-04/E-07; cards/experiment/mobile.svg:1 EM-02.

The reference README retains route/IA/state authority and has active UI-foundation consumers. C-08 promises score-task/judge-threshold stages and evaluation records in Observe; E-17 declares old Workflow/tasks/judges/gates and canonical Evaluation-run inspection; E-07 repeats that task model and direct section links. The runtime owners and Observe routes supplying this product were deleted, while current VerifierSpec has inputs/steps/requirements/decide/context_capture. Deferred Experiment implementation does not turn positive current design instructions into historical evidence. The consequence is a current implementation reference instructing a maintainer or agent to recreate the deleted product, not a claim that the running app currently renders these SVG panels.

The validated minimal correction is deletion of C-08/E-17 and their positive entries, removal of E-07's obsolete task/aggregate/output/Observe instructions and direct Evaluations-section navigation in E-04/E-07/E-16/EM-02, with affected counts reconciled. Preserve other Experiment content, ordinary metrics, unrelated Workflow debt and all runtime behavior. No scanner, replacement UI, workspace, API, migration or compatibility machinery is required.

The full source trace, every proposal disposition, exact correction and narrow proof are in `findings-validation.md`. The self-contained implementation handoff is **TASK-006-R2-retire-evaluation-reference-model.md**, routed to `$wyrd-implement`.

## Reconciled cumulative acceptance

| Obligation | Independently validated implementation and proof | Result |
|---|---|---|
| REQ-001–016 / AC-001–008 regression boundaries | Retained helper bodies and engine/control-flow/import consumers compared with base; prior exact P9/P10 and engine evidence support move. No earlier task acceptance granted. | PASS within TASK006 regression scope |
| REQ-017 / AC-009 current-consumer agreement | Corrected SDK docs/generated ownership agree; current reference definition/result contradiction remains. | FAIL, finding4 only |
| REQ-018 obsolete machinery retirement | CLI/Eval contracts/scenario/scoring/store/resources removed; retained live closure uses vala-verify/verification. Current reference still teaches old machinery. | Runtime PASS; reference FAIL, finding4 |
| REQ-019 / INV-001/002/009 | Existing engine retained; contracts stay pure; GenAI type/field expansion preserves standard event names/content/validation and storage owners. | PASS |
| REQ-020 / AC-012 removed access | Closed permission parser/serde refuses stale grants; actual issuance rejects corrupt role; real negative surface paths retain current controls. | PASS with recorded proof limits |
| REQ-021 / AC-015 bounded development retirement | Claims removed before only vala.eval tasks; demands already dropped; shared namespace authority/list/query/write refusal preserves current evidence and tenancy. | PASS; claim-delete branch unseeded |
| REQ-022 / AC-011 and INV-008 | Approved scanner withdrawal honored; ordinary/external/GenAI vocabulary preserved. Current positive reference closure incomplete. | FAIL, finding4 only |
| INV-003/004 security/tenancy/audit | Existing TenantConn/RLS, accepted-authority and nonblocking outbox owners preserved; no new accepted legacy grant path. | PASS changed scope |
| INV-005–007 runtime/resource semantics | No new runtime; retained walk/JoinSet/deadline/capacity/drop/failure/citation owners. | PASS changed scope |
| AC-010 proof integrity | Focused GREEN records distinguished from missing historical RED, unseeded branch and deferred full qualification; R1 corrects ts:format:check record. | Credible bounded proof, no fresh aggregate certification |
| AC-013 retained real-time/background/drift/results | P9/P10 and retained assertions support the move; P10 proves client reconnect, not server crash recovery. | PASS task evidence scope |
| AC-014 generated/served/MCP contracts | Served assertion/helper and actual MCP catalog proof retained; generated source/stubs correspond. | PASS; finding1 remaining portion closed |
| AC-016 docs/UI references | Runtime old routes and named prior boards retired; residual current evaluation-model references remain. | FAIL, finding4 only |
| Required bounded rustdoc and SDK shape docs | Actual material items and returned shapes independently checked. | PASS; findings2–3 closed |
| Non-goals | No new lifecycle/parser/transport/compatibility/history/GC/naming framework required. Unrelated planning and generic Workflow debt excluded. | PASS |

## Verification and limits

The committed original task records P1a–P10 exact GREEN, supporting engine49/spec758/permission27/migration3, real Rust/Python/TypeScript/MCP/CLI negatives and current controls, GenAI roundtrips/canonical event and client reconnect, plus language format/lint/type/NAPI/codegen/deps/tenant/docs/examples/UI178. R1 records exact served OpenAPI and principals integration22, fmt/lints/ts:format:check/ts:lints, XML/offline render checks, then deps/docs after scanner removal. These are supplied execution records, not reviewer reruns.

Fresh review used source comparisons, actual consumer traces and current offline SVG renders/pixel inspection. Current C-08/E-17 retain the residual model in both themes; SM-01/M-02 separate labels close the prior overlap. Renders use fallback fonts and cover named artboards, not runtime interaction or the entire package. No fresh builds/tests/gates/generators/live calls/migrations/external-example rebuilds were run. The pinned source was clean and cumulative diff whitespace checks passed.

P2 historical RED was not captured; P3 expiration-claim deletion is unseeded. Stale stored grants retain ordinary OAuth public WYRD_SPEC_500_INTERNAL; retired SQL matches a never-existing namespace with QUERY_INVALID_SQL. These limits did not establish additional required defects. Full SQL/Bifrost/Verifier journeys remain integrated change-review work. Nothing shipped: no extra compatibility/history conversion or recovery machinery is justified.

TASK-005's latest committed independent verdict remains FIX_REQUIRED, despite later implementation fixes. Its prerequisite PASS is unestablished; this bounded review neither accepts it nor reopens unrelated findings. TASK-006 also remains unaccepted due to finding4. No task status, source, authority, Git history, push, merge or deployment was changed. A later independent cumulative task review must assess the reference-only remediation before acceptance.
