# Independent invariant review — TASK-002 cumulative candidate

Overall result: **FAIL**.

## Subject and limits

Repository `/home/thorrester/Documents/GitHub/wyrd`; base
`0569b79702218600c4f9790f45cc03100d5c6f1c`; candidate
`e7d16b5bd622b9a564a49edb18239df7f209ca92`. HEAD remained the candidate.
Only this report was written. No current discovery reports were read.

Authority: approved spec Revision 12, original superseded TASK-002,
TASK-002-cleanup, prior r1 validated findings/remediation, AGENTS.md,
architecture/agent-rules.md, current Workflow/client/reference sections of
wyrd-design.md, wyrd-doctrine.mdx, reference router, spec-driven-development,
maintainer-style and TypeScript guide. Revision 12 supersedes the old public
WorkflowLoader and keyed normalization instructions. Approved authority and
skill changes in the cumulative range are not unrelated product drift.

Reviewed the cumulative changed surfaces and expanded the supplied navigation
map through the actual loader, reference visitor, exact Cards read assertions,
shared graph traversal, runtime Agent construction, Workflow lowering/run,
EffectiveSpecs sibling/external consumers, SQL dependency recheck, registration
transaction, all three SDK projections, generated declarations, example and
journey fixtures, and tests. No CodeGraph directory exists.

Independent execution evidence comes from `verification.md`: shared client
focused test 1/1; three server focused tests 3/3; Rust SDK journey 1/1;
format check and diff check pass. Broader lane and Python/TypeScript results
remain recorded implementation claims. I did not start competing build or
Postgres lanes. Passing execution cannot establish assertions absent from tests.

## Producer → state → consumer audit

* Authored files enter the existing sandboxed `wyrd_loader::load` pipeline.
  Its path projection retains Sibling provenance, fills spaces, validates the
  pure Workflow contract, and orders dependency submissions. Entry-file source
  provenance selects exactly one root in `WorkflowBodies::authored`.
* `WorkflowBodies` separates loader sibling specs from fetched registered
  envelopes. Its dependency discovery preserves Ref/Sibling, constructs Cards
  only when an external dependency exists, and uses `GraphTraversal` through
  exact Cards reads. `reads::assert_selector_identity` enforces requested kind,
  space, version and optional UID. Registered traversal follows UID-bearing
  references; runtime scope avoids artifact inventories and filesystem writes.
  Body lookup checks source and optional UID before Skald consumes a spec.
* Registered loading validates the selector first and uses the same graph
  traversal. Active-state refusal occurs before complete Workflow publication.
  The Workflow envelope retains its UID and version for run snapshots. The
  disk hydration sibling still uses Bundle scope, retaining all relationships,
  inventory reads and existing atomic workspace publication.
* Server registration inventories canonical reference slots, resolves external
  references in the tenant transaction, then uses `EffectiveSpecs` separately
  for sibling and external bodies. Workflow validation extends that existing
  owner with transitive Prompt bodies. Skald's declarative mode clears only
  runtime tool binding for validation; persisted submissions retain tool names.
  Binding validation and baseline validation share the same provenance-aware
  owner. Registration does not execute, bind runtime tools or resolve secrets.
* Preflight's exact `(CardRef, CardUid)` values survive into write registration.
  SQL rechecks identity **and expected UID** under `FOR SHARE` before reserving
  idempotency or persisting Cards/relationships. Audit append and writes retain
  their existing transaction owner. Replacement fails the whole write attempt;
  no Workflow principal or WyrdState root is added.
* Rust reexports the shared facade. Python detaches the GIL and projects it to
  the existing native Workflow on the shared runtime. Node owns Rust-native
  Workflow state across awaits and uses the existing lifecycle error envelope.
  The Node producer serializes canonical run snapshots; its public TypeScript
  consumer annotation loses the closed step/error shape (INV-R2-002).

Reuse comparison: the existing loader covers parsing/path/!file; existing
GraphTraversal covers remote closure; WorkflowBodies is a private provenance
adapter for authored and registered inputs, not another remote traversal;
Skald's existing lowering and validation cover binding/runtime rules;
EffectiveSpecs covers tenant preflight; existing SQL/audit registration covers
persistence. No replacement parser, transport, executor or cache is justified
or introduced. The facade earns its existence at the client IO boundary.

## Acceptance matrix

| Obligation | Implementation evidence | Verification evidence | Result |
|---|---|---|---|
| REQ-001 normal Workflow envelope, no second format | Existing loader and WorkflowCard conversion in workflow.rs | loader checked-in bundle and client focused test | PASS |
| REQ-002 inline/path/versioned Agent steps | canonical visitor, loader path rewrite, Skald referenced/inline lowering | loader/client/server focused tests; SDK loads | PASS |
| REQ-003 existing Agent→Prompt and binder | bodies.rs CardBodyResolver and native Prompt construction | client and registered server executions inspect downstream bound request | PASS |
| REQ-013 pure validation at authored load | wyrd-loader validate.rs invokes WorkflowSpec::validate | loader cycle assertion and raw server invalid graph tests | PASS |
| REQ-013A resolved validation | from_card_bodies/validate_card_bodies delegate Workflow::validate | extra-binding and dialect refusal tests; server sibling/external negatives | PASS |
| REQ-014 validate before registration/dispatch | EffectiveSpecs::validate_workflows before write plan; hydrated Workflow returned only after validate | server invalid request assertions show no Card/operation; client refusal count | PASS |
| REQ-024 three SDK local/registered executable path within cleanup, later server/CLI work deferred | public projections exist; run delegates existing engine | Python has no run in journey; TS only expects binding-unavailable; Rust only injected gateway | FAIL — INV-R2-001 |
| REQ-025 shared loader and automatic client refs | from_path uses loader and lazy Cards | client focus and Python/TS source assert missing/denied credentials | PASS for implementation; Rust journey proof gap in INV-R2-001 |
| REQ-028 exact registration/relationships | canonical binding plus server declarative graph validation | server proves seven Active Cards and exact Agent→Prompt/Workflow→Agent relationships | PASS; CLI apply belongs to TASK-005 |
| REQ-029 active/exact registered graph for this task | Cards traversal and extend_registered | server locked graph test pending/deleted/foreign/mismatch refusals | PASS; server-run acceptance belongs to TASK-004 |
| REQ-040 Prompt owns native request/model | Prompt::from_native and existing Agent construction | actual example request bodies and downstream binder assertions | PASS |
| REQ-052 declared tools remain registrable | declaration-only validation drops binding on copies, writes original submissions | server registered tooling Agent retains bifrost.query/cards.get | PASS for this task; server invocation later |
| REQ-054 exact public loading contracts, owning runtime tests | three SDK facades/typed Cards views and selector parsing | language journeys exercise loads; generated declarations present | PASS loading API; complete executable journey FAIL via INV-R2-001 |
| REQ-055 lazy automatic refs/no registry for local | branch on external_refs before Cards::new; existing owner resolves closure | shared test and Python/TS credential isolation | PASS implementation; Rust mixed-path proof incomplete |
| REQ-056 provenance, exact pins, active/read checks, no writes/secrets during load | separate body sources; exact Cards reads; Active checks | collision registration test and registered graph negatives | PASS implementation; language parity proof incomplete |
| REQ-057 existing owners, no Workflow principal/WyrdState root | graph owner extension and thin facade; no state/auth root changes | principal-count registration assertion and source closure | PASS |
| REQ-059 delete obsolete loader/graph/normalizer and preserve protections | old types/normalization absent; canonical visitor unchanged | source audit; loader/shared regression claims; SQL-focused proof | PASS |
| INV-002 spec IO/PyO3-free | reference.rs adds pure provenance conversion only | static diff and boundary claims | PASS |
| INV-003 declarative Cards | no clients/tools/providers stored on spec contracts | static source | PASS |
| INV-005 exact registered closure | graph exact/UID assertions and locked bodies | server and Rust run pin tests after v2 | PASS implementation; TS after-v2 proof missing |
| INV-007 one shared WorkflowRun projection | runtime/native JSON is canonical, TS annotation hides fields | no successful TS run contract assertion | FAIL — INV-R2-002 |
| INV-008 only declared tools | existing Agent::from_card resolves each declared name | unchanged tool owner; server declaration preservation | PASS within loading/registration scope |
| INV-014 only existing Prompt binder | existing runtime binder unchanged | client/server bound request assertions | PASS |
| AC-001 actual bundle local execution in each language | bundle has correct native Prompt schema and loads | no successful Python/TS example execution | FAIL — INV-R2-001; CLI proof explicitly later |
| AC-002 versioned Prompt relationships in actual bundle | three versioned Prompt files and sibling paths | server asserts all seven outcomes and relationship UIDs | PASS; actual CLI apply later |
| AC-003 registered load/run equivalence per SDK | same runtime behind each wrapper | successful execution only Rust/shared; Python/TS omit it | FAIL — INV-R2-001 |
| AC-006/AC-013 pure and resolved negative validation | loader/spec/Skald owner reuse | shared binding/dialect plus raw server sibling/external matrix | PASS for changed seam |
| AC-029 local/mixed file→run→apply→registered load→run in each language | APIs exist | incomplete executable journeys and Rust mixed authored path absent | FAIL — INV-R2-001; CLI application explicitly TASK-005 |
| AC-030 each language lazy/read/pin/provenance negative proof | correct shared producer boundaries | coverage differs by language; TS lacks newer-version pin assertion; loading collision assertions only step IDs | FAIL — INV-R2-001 |
| AC-031 removed machinery/projections/Service regression | obsolete symbols absent; Bundle scope retained | source audit, codegen/boundary/shared claims | PASS for cleanup; selected gateway config TASK-003 |
| No new parser dialect/transport/executor/provider-secret resolution/Workflow authority | changed code only composes existing owners | cumulative source/manifest audit | PASS |
| Cancellation publishes no partial Workflow and writes nothing | private loaded bodies, fallible read composition, no write call | source tracing; no cancellation test claimed | PASS |
| Native execution using existing provider configuration (cleanup §§Outcome, S5) | default run remains native-capable but checked-in proof uses WyrdGateway | no successful Native journey for these new loading surfaces | FAIL — INV-R2-001 |
| No weakened failures, skipped tests or lint suppression to hide red gates | new ignored Rust SDK test is environment-gated and explicitly executed | independent run selected 1/1; corrected Vitest selector recorded | PASS |

## Prior-finding closure

| Prior stable ID | Cumulative disposition |
|---|---|
| FIND-TASK-002-1 provenance | Corrected at both body producers: client WorkflowBodies and server EffectiveSpecs distinguish Sibling from Ref; server incompatible-body collision test passes independently. Local credentialless collision refusal exists. Full language observable-output closure still needs INV-R2-001. |
| FIND-TASK-002-2 versioned Prompts | Closed: actual acceptance bundle now has three Prompt Cards, exact sibling projection and seven-card/relationship assertions. |
| FIND-TASK-002-3 keyed normalization | Superseded correction mechanics: Revision 12 deliberately retains untagged canonical types and deletes keyed normalization rather than extending it. No second slot inventory remains. |
| FIND-TASK-002-4 UID replacement | Closed: expected UID enters SQL predicate and locks persist to commit. Independent replacement-race and fresh-request test passes. |
| FIND-TASK-002-5 rustdoc | Previously cited deleted parser/loader symbols no longer exist; retained/new focal loader, body resolver, SQL fence and fixture/test items contain substantive docs/error/panic/cancellation descriptions. This is prior-location closure, not a waiver for other touched items. |
| FIND-TASK-002-6 imports/signatures | Cited local imports and qualified helper return types corrected or deleted; loader module imports PathBuf/TempDir at top, native Prompt alias at module scope, edited helpers use imported names. |

## Proposed findings

### INV-R2-001 — MISSING — Complete Native SDK journeys and their behavioral assertions are absent

**Obligation:** TASK-002-cleanup explicitly proves Native execution through
existing provider configuration, Scenarios 3–5 and acceptance; Revision 12
AC-001/003/029/030; AGENTS.md §11. TASK-003's selected gateway/configuration
assembly is separate and is not required to supply this Native proof.

**Locations:** `sdks/wyrd-sdk-python/tests/integration/cards/test_cards_crud.py:484–565`;
`sdks/wyrd-sdk-ts/wyrd/tests/integration/workflow-loading.test.ts:41–136`;
`sdks/wyrd-sdk-rust/tests/workflow_loading.rs:15–18,244–327`;
`changes/active/skald-workflow-runtime/tasks/TASK-002-cleanup.md:311–318`.

**Evidence:** all shared acceptance fixtures declare WyrdGateway. Python's
journey never invokes local/authored/registered run. TypeScript invokes run
only to assert `WYRD_WORKFLOW_503_BINDING_UNAVAILABLE`. Rust loads the local
bundle without running it and never invokes `from_path` on the mixed authored
bundle; it runs registered graphs solely through an injected gateway. TS also
never registers team-v2 before the pinned load. Python/TS shadowed tests check
step IDs, which cannot distinguish whether the same-identity external slot
consumed the registered or sibling body. Task evidence explicitly treats these
limits as sufficient despite the replacement task's Native execution scope.

**Consequence:** green lanes can coexist with broken public Native execution,
binding/result conversion in Python/Node, or mixed-file Rust environment
resolution. The required complete SDK product outcome is not proved.

**Smallest required correction:** extend the existing three journey tests with
deterministic native provider setup through existing provider configuration,
while retaining the actual checked-in Prompt bodies. Run local and mixed
loading, apply through existing registration, load exact and UID, run again,
and assert equal named/intermediate outputs and downstream Prompt bindings.
Each language must exercise its own ambient configuration path and required
refusals. Preserve collision proof and assert observably distinct local and
registered Agent output, exact Agent/Prompt relationships, no calls during
loading/registration/refusal, and no floating after newer versions. For Rust,
configure the test process environment before runtime startup using the
repository's existing launcher/subprocess or test-owned configuration pattern;
do not use unsafe concurrent environment mutation as the reason to omit the
public path. Do not build gateway configuration here; TASK-003 remains intact.

**Closure proof:** the existing exact Rust/Python/TS journey commands must select
and pass successful native execution for these paths; retain the independently
passing server and client-focused regression tests. No new dependency,
execution abstraction or test harness is required.

### INV-R2-002 — INCORRECT — TypeScript run type erases canonical step and error fields

**Obligation:** INV-007 same WorkflowRun on every shipped surface;
TASK-002-cleanup S5 “port native result fields”; AGENTS.md §9 typed public
contracts and first-class SDK projection; TypeScript guide “must not fork field
names” and maintainer-style public typed contract.

**Location:** `sdks/wyrd-sdk-ts/wyrd/src/index.ts:1255–1273,1321–1324`.
Canonical producer: `crates/wyrd-spec/src/card/workflow.rs:723–733,775–790,811–831`;
serialization path `sdks/wyrd-sdk-ts/native/src/workflow.rs:139–142`.

**Evidence:** native run returns the Rust WorkflowRun serialized by the existing
lifecycle envelope. Its `steps` values are WorkflowStepResult (closed status,
text, structured_output, attempts, timestamps and error), and WorkflowRunError
includes details/remediation. The newly exported TypeScript interface declares
steps as `Record<string, unknown>` and its error as only `{code,message}`.
The wrapper's `lifecycleValue<WorkflowRun>` adopts this annotation without
changing the actual JSON. Successful run snapshots, including terminal errors,
are therefore reachable producers of fields the TypeScript consumer contract
cannot access. Rust and Python consume the original canonical result instead.

**Consequence:** normal typed code cannot inspect `run.steps.security.status`,
attempts or text without its own unchecked cast, and `run.error?.remediation`
and `.details` fail type checking despite being real durable result fields.
This is contract loss, not runtime serialization loss.

**Smallest required correction:** project the existing canonical WorkflowStepResult,
WorkflowStepStatus and WorkflowRunError fields in the existing TS public wrapper
and use them in WorkflowRun. Keep wire names, nullability, runtime JSON and
error envelope unchanged. Use the existing canonical Rust contract as the
source; do not add a second executor or transport. The typed input/output JSON
values should match the task's JsonValue contract instead of allowing arbitrary
non-JSON values through `unknown`.

**Closure proof:** add a small public TypeScript type-check assertion accessing
step status/attempts/text and run error details/remediation, and assert the same
fields on a successful/failed native run in the existing journey. Run
`mise run ts:typecheck` and the corrected exact journey selector. Generate
only artifacts whose owning generator is affected; never hand-edit N-API
index.d.ts.

## Verdict

**FAIL:** two proposed task-local gaps remain. The prior provenance and exact
preflight-UID defects are corrected at their owning boundaries. Source
immutability is preserved; no unresolved material product decision is needed
for the proposed corrections.
