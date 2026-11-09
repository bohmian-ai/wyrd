# Independent remediation validation — TASK-001

Recommendation: **FIX_REQUIRED** for the remaining mandatory documentation portion of **FIND-TASK-001-3**. **FIND-TASK-001-1** and **FIND-TASK-001-2** are closed. No new finding, runtime regression, specialist gap, or specification decision was established.

## Immutable subject and review scope

Repository: `/Users/stevenforrester/Documents/GitHub/wyrd`. Original base `5234961d51798581a026834cfffd5efaadc226b4`; prior candidate `790da3369c1b0107765db234d72c1ae0c4680446`; cumulative candidate `c0a06eacdc70249e9c32e44b7670314cf1aefb3a`. Reviewed the actual prior-to-current Git diff, with cumulative original-task context. HEAD matches the candidate and the tracked worktree is unchanged. The three packet SHA-256 values match `subject.md`. No CodeGraph index exists and no nested directory rule applies to the changed paths.

Read the task-review skill, root contributor instructions, approved revision 3, original TASK-001, prior verdict/validation and eight prior discovery reports, TASK-001-R1 and its recorded evidence, the relevant contributor guides and owning client/in-memory-authoring/reference/Verifier contracts. Implementation evidence was treated as a claim, not acceptance authority. No user completion summary was used.

This is a remediation audit: closure, the complete fix diff, current capture producers and all attachment/conversion consumers, changed local and public proof, and cumulative changed-item documentation. Unchanged graph validation, registry/upload/pull owners, security, persistence and execution evidence carry forward from review-01. They were not comprehensively rediscovered. Only this report is written; no Cargo, source mutation, packet edit, commit or push was performed by this validator.

## Specialist applicability and coverage

The round's single fresh ponytail reviewer is sufficient. All eight prior discovery reports and the prior independent reconciliation are preserved; there is no missing selected report. Carry-forward is based on their producer-to-consumer evidence, not merely unchanged filenames.

| Contract / possible specialist | Independent selection decision |
|---|---|
| Current capture, snapshots and identity; invariant/maintainer | Fresh closure inspection covers the complete Binding and AgentCall bodies, their constructors and sibling writers, `into_parts`, Action/Check, Step/Requirement/Verifier, Agent/Service attachment and compose, Graph and publication descriptor consumption. New fields partition existing local owned values; no new shared state, graph algorithm, concurrency or lifecycle owner is introduced. Prior invariant/maintainer coverage remains applicable. No additional discovery is needed. |
| Complete public native meaning and slots; behavior | New SDK proof and canonical engine test close the previously named missing cases; no production schema/validator/public-type implementation is added. Source construction and comparison were inspected independently. No new comprehensive behavior review is needed. |
| Cross-tier rules and rustdoc; repo | Prior standards authority mapping carries forward. New Vala code is one inline test using existing engine helpers; the additional client paths contain literal documentation/link repairs. Fresh review checks the actual changed docs and new private/test items. Remaining mandatory documentation is retained below; a fresh broad standards reviewer would add no missing boundary coverage. |
| Failure propagation, scratch, publication and hydration; system/persistence | Production Cards/state/upload/Graph/loader bodies are unchanged by the fix. The descriptor producer correction feeds their existing loop. Prior system and persistence reports cover scratch ownership, per-request source maps, failure/retained publication and exact durable resolution. Fresh review checks cancellation prose against these unchanged bodies. No new recovery or persistent-data decision requires a specialist. |
| Permissions, tenancy, fixed selections, exact subject; security | Prior security report traces both entry points to authenticated server checks and subject binding. The fix does not change those paths, reference meaning, transport or authorization. Current-slot capture does not replace their authority. Carry forward; no fresh security review needed. |
| Planning, concurrency and requirements barriers; concurrency | Canonical planner and production engine are unchanged. New engine test covers supplied-null admission through existing conform/Scope, not scheduling. Prior concurrency report and exact regression/dependent-output journey remain applicable. No fresh concurrency review needed. |

No material claim conflict or new untraced failing runtime path warrants follow-up discovery. The remaining issue is incomplete closure of an existing mandatory finding, not a new finding self-validated by this reviewer.

## Prior finding closure

### FIND-TASK-001-1 — CLOSED

`authoring/mod.rs:431–505` now separates Binding's additive Verifier/Operator captures from the current Trigger capture. `set_runs_on` replaces both its canonical reference and its owned contribution. `into_parts` joins only the current Trigger with retained additive contributions. All three attachment sites use it: Agent `add_verifier` at `:649`, Service `add_verifier` at `:715`, and component `compose` at `:749`.

`authoring/verifier.rs:508–584` retains the Agent target's capture separately and stores one workspace contribution per canonical destination. A file replacement inserts that destination's new capture; a Scope replacement removes that destination's old capture. `into_parts` joins current destinations in workspace path order. Both `Action::agent` at `:369` and `Check::agent` at `:451` call this owner. CodeCall source ownership remains separate and unchanged. Additive Step/Requirement/Verifier insertions still capture their transferred values.

Graph at `mod.rs:253` still deduplicates equal current captures and refuses unequal current identities; its conflict policy was not weakened. `ArtifactFile::dependencies` still captures only a local source. Cards `declare_authored` at `cards/handle.rs:364–388` consumes exactly the resulting `Declaration.artifacts` and uses the existing upload owner; state calls that same Cards operation. Therefore removing an obsolete descriptor at its producer removes the unwanted IO cause without a downstream upload guard.

Inspected the entire `replacements_keep_only_current_dependencies` test at `authoring/tests.rs:910`: direct versus edited same-identity Trigger, sibling-to-registered replacement, genuine two-current-Trigger conflict, missing obsolete file to Scope, file to another file, and one replaced destination of two sharing the policy. It asserts canonical/value/declaration equality or the exact remaining descriptor set, not merely successful construction. Existing snapshot/conflict tests remain unchanged except docs. Fresh review-02 verification passes all four owning authoring tests. The implementation's RED and mutation histories are recorded history, not independent reruns.

The correction is appropriately at the existing producers. It adds no graph traversal, upload owner, retry, global cache, public wrapper or lifecycle. No affected attachment site was left using the old transfer mechanism.

### FIND-TASK-001-2 — CLOSED

The offline public SDK test `integration/authoring.rs:1359` constructs through `wyrd_ai` paths and compares native specs against canonical decoded forms. No new dependency, public helper or projection was required.

| Previously missing inventory / proof | Source and independent assessment |
|---|---|
| All ComparisonOperator values | `operators` at `:1271` enumerates the current 56 canonical forms, including parameterized forms; the public test decodes each as the public typed operator, forwards it through assert/each/trace, and compares canonical requirements. The existing local primitive proof supplies non-equality condition forwarding and stage metadata. |
| Remaining registrable native Specs | Public test's kind table constructs canonical `Spec` values, wraps them in `CardDefinition::new`, and compares kind/spec and one-submission declaration. Existing SDK `Spec` and `authoring` reexports provide usable construction without an outer wire envelope or duplicate contract hierarchy. Prompt/Artifact/Agent/Verifier paths and prior Service/native parity supplement the table. |
| Source slots and source-reference forms | Public `add_slot` accepts canonical deserialization of both `verifiers.<name>.inputs.<slot>` and `verifiers.<name>.requirements.<id>.outcome`; group order/reference/check fields are compared and validated. This is a concrete usable public canonical InputSlot construction route. The approved Rust requirement accepts existing schema/slot representations and does not require a new SourceText constructor merely because serde constructs the canonical source form. |
| Trigger/Operator binding fields | Public binding construction compares `runs_on`, `on_failure`, and `on_verdict_change` canonical fields, covering sibling, inline and registered reference intents across those positions. It checks actual submitted kinds, so losing sibling captures cannot pass solely through equal canonical references. |
| Required versus nullable | Public authored slots compare required-strict/default-required, required-nullable, optional-strict and optional-nullable schemas to canonical output; `InputSlot::required()` separately checks requiredness. No conversion conflates these fields. |
| Explicit null execution / omission | New `vala-verify` engine test at `blocks/engine/tests.rs:1055` admits supplied null in both nullable requiredness forms and refuses both strict forms with `EngineError::InputInvalid` naming the slot. Canonical `engine.rs:1361` validates supplied JSON against its schema independently of requiredness. Existing `missing_and_optional_evidence_resolve_honestly` at test `:904` establishes missing-required inconclusive and omitted optional null/child-missing behavior. Both tests freshly pass in review-02. |

The public test plus retained local proof cover the named handoff obligation; this is not a requirement to run each operator remotely or add typed wrappers for every serde-backed canonical value. No additional product or helper work is prescribed.

### FIND-TASK-001-3 — PARTIALLY CLOSED; residual retained

The substantive documentation corrections are present: local tests/helpers, new conversion methods, Prompt and capture fields, SDK Story fields/methods, construction/read/assertion helpers and `verify_both` have meaningful docs and the added error/panic sections. New producer helpers and proof tests are documented. Verifier planning and `add_depends_on` now agree with explicit plus inferred canonical dependencies.

Cards cancellation prose describes retained completed Artifacts, uncertain in-flight upload state, no rollback and fresh-call retry under ordinary registry rules. State authored methods and `pull_declared` describe retained declaration, removed temporary directory and caller bundle/staging behavior. I compared the unchanged complete bodies and prior system trace rather than reading the added prose as runtime proof. The three ancillary client doc-link repairs and transport link edits are documentation only and belong to the prescribed doc-gate closure; no runtime cleanup entered that diff.

However, five original-task SDK helpers still omit their applicable panic contracts. This is within the existing mandatory changed-item finding and TASK-001-R1's cumulative prose pass, not a new style preference or new product requirement. Exact residual evidence follows in the stable ledger.

## Reconciled acceptance matrix

| Requirement / acceptance / constraint | Implementation and verification evidence | Result |
|---|---|---|
| REQ-001, AC-007 complete shared/public primitives | New public native-family proof plus retained local canonical proof; public selector freshly passes | PASS |
| REQ-002/008, AC-002 edits, snapshots, dedup and current captures | Producer correction and full regression above; four exact local tests freshly pass | PASS |
| REQ-003/007, required independent of nullable | Authored canonical table + native null/schema/omission mapping; two engine selectors freshly pass | PASS |
| REQ-004 planning/references/concurrency/dependent output | Runtime implementation unchanged; carry review-01 concurrency evidence and journeys; corrected docs match planner | PASS |
| REQ-005/006, AC-003 publication, exact references, preparation/source isolation/bypass/retention | Current descriptor closure above; carry source-backed review-01 system/persistence evidence and real Artifact journey | PASS |
| REQ-006/008, AC-001 Cards receipt/state hydration and file parity | Local parity freshly passes; runtime owners unchanged; carry prior real parity/state regression | PASS |
| REQ-007, AC-004 two-component outcome and attribution matrix | Carry prior behavior/security reports and fresh review-01 component/dependent journeys; no changed consumer invalidates their proof | PASS |
| INV-001–005, thin SDKs/non-goals | No manifest, wire/schema, endpoint, auth, server runtime, migration or foreign projection change; prior domain/standards coverage remains applicable | PASS |
| Mandatory changed-item rustdoc gate | Most prior locations corrected; five SDK helper panic sections remain absent | FAIL: FIND-TASK-001-3 |

UID/reference semantics remain reconciled exactly as prior validation: durable server binding pins the exact tenant/kind/name/version before persistence; this does not claim that the outgoing client graph transmits the upload result's UID. No changed source or authority reopens that accepted issue.

## Final retained stable ledger

### FIND-TASK-001-3 — REVISED, VIOLATION: remaining SDK helper panic contracts

**Discovery sources:** original REPO-001 and M-002; prior independent FIND-TASK-001-3. **Status:** REVISED to the remaining portion; no new finding ID. **Violated obligation:** root AGENTS changed-item rustdoc gate and `agent_docs/rust.md` Rustdoc require applicable `# Panics` on new/materially changed items, expressly including private/test helpers. TASK-001-R1 requests the complete cumulative helper pass.

**Exact source and failing documentation scenarios**, all in `sdks/wyrd-sdk-rust/tests/integration/authoring.rs`:

- `release_access` at `:322` and `summarized_access` at `:328` accept a caller-provided version and forward it to `access_verifier` at `:337`. That owner immediately calls `Verifier::new(..., version).expect(...)` at `:338` and now documents its panic on refusal. The two wrapper docs do not carry that contract; an invalid exact version demonstrates the path without any server.
- `baseline` at `:451` builds `finance_release`, which calls report/access construction and `fact_step`; the latter reads the checked-in source via `CodeCall::from_file(...).expect(...)` at `:198`. A missing/unreadable fixture panics instead of returning a Service. Baseline has no panic section. This is the same fixture-read panic already documented on `fact_step`, not speculative allocator or hidden dependency behavior.
- `outcome` at `:530` forwards caller-supplied `id` to `verifier_building_blocks::requirement` at `:229`. That existing owner explicitly panics if the result has no matching requirement, and documents it. The new wrapper omits this panic contract.
- `policy_published` at `:794` forwards caller-supplied `version` to `card_ref` at `:445`, whose serde decode explicitly expects a valid exact reference. Invalid caller version therefore panics before its later network `.is_ok_and`; the function's `bool` does not turn this construction failure into false. Its docs omit the panic contract already documented on `card_ref`.

**Consequence and materiality:** the explicit all-visibility documentation gate remains unsatisfied. These are concrete input/fixture/missing-assertion panic paths in task-introduced helpers; no production correctness or security failure is alleged. Public cargo doc/clippy do not check this private helper contract, so their green results cannot waive it.

**Smallest correction / burden:** finish this existing documentation correction with short applicable `# Panics` sections on these five wrappers, reusing or linking the documented callee contract where sufficient. Preserve signatures, runtime bodies, fixtures and assertions. No new abstraction, helper, public surface, test, checker, dependency or broad prose rewrite is needed. Deletion would require changing existing callers and provides no smaller safe closure than the documentation.

**Focused closure proof:** source inspection of these five helper docs against the cited existing bodies/callees, plus diff whitespace/format as appropriate. Existing green runtime tests remain sufficient because the required correction is prose only. Do not repeat journeys, engine tests, broad discovery or the whole specialist team. The instruction against style-only rounds does not waive this explicit still-open gate; this is completion of the already requested mandatory correction, with no optional wording work or expanded style audit.

## Verification and recommendation

`verification.md` records seven freshly passing focused tests (four local authoring, public SDK construction, native null and omission), warning-free client/SDK cargo doc, fmt, scoped all-target clippy and range whitespace check. All test results were considered with their actual assertions/source. The cached SDK linker compact-unwind warning is recorded; the selected test passed. Implementation RED/GREEN/mutation histories were not independently repeated.

Carry forward prior real journeys, bounded concurrency, existing state declaration and lower-owner source evidence. No cross-platform execution, new cancellation fault injection or live model-quality evaluation was performed. No runtime risk was found in the bounded fix; the remaining limitation is the explicit helper-documentation gate.

Recommend **FIX_REQUIRED** with only the consolidated residual FIND-TASK-001-3 correction above. No specification revision or additional specialist is needed. This is the first remediation review, so the two-unsuccessful-round convergence requirement is not triggered. Findings 1 and 2 must stay closed in the next focused review unless a later change supplies material contrary evidence.
