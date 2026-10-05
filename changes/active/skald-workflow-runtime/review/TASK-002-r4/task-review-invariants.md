# Independent invariant review — TASK-002 R4 cumulative candidate

Overall result: **FAIL**.

## Subject and review limits

- Repository: `/home/thorrester/Documents/GitHub/wyrd`
- Base: `0569b79702218600c4f9790f45cc03100d5c6f1c`
- Candidate: `2ff7bbfa57737ce8a7a69287e2fa462c313b1a8c`
- Authority: approved `spec.md` Revision 12, `TASK-002-cleanup.md`, the R2 and
  R3 remediation tasks, prior validated ledgers and verdicts, `AGENTS.md`,
  `architecture/agent-rules.md`, current Wyrd design/doctrine, and the applicable
  language references.

The candidate remained `HEAD` before this report was written. The repository has
no `.codegraph/` directory. I independently expanded `subject.md` through the
loader, authored-body projection, exact Cards reads, graph traversal, Skald
hydration and Prompt binding, server preflight, registration write transaction,
SQL relationship locking, all three SDK boundaries, generated declarations, and
the owning journey tests. No other R4 discovery report was read. No build or test
was run, as assigned; verification below is source-inspected candidate evidence
and the immutable implementation evidence supplied with R2/R3.

The human standing direction was applied: the candidate adds no material
mechanism, check, file, setting, or option that lacks an established repository
or ordinary ecosystem analogue. `spawn_blocking`, Vitest's per-test timeout,
Hakari generation, typed newtype validation, and public HTTP inspection in an
integration journey are all established/native mechanisms. I found no DRIFT
that warrants remediation, and recommend no new scanner, allowlist, harness,
configuration switch, compatibility path, or abstraction.

## Producer-to-sink invariant trace

| State / invariant | Producer and transitions | Consumers and outcome |
|---|---|---|
| Authored graph provenance | `wyrd_loader::load` resolves sandboxed paths to canonical `Sibling` values and retains authored external `Ref` values; `WorkflowBodies::authored` stores sibling Agent/Prompt bodies separately; `external_refs` follows only sibling bodies and sends external refs to the existing Cards traversal. | `WorkflowBodies::body` serves `Sibling` only from the local map and `Ref` only from registered envelopes. `CardBodyResolver` receives the original discriminator. Same-identity local and registered bodies therefore cannot substitute for each other. |
| Registered closure identity | `reads::get_response` resolves exact/UID selectors and asserts the response identity; `GraphTraversal` follows server-derived UID-bearing relationships, rejects cycles/unsafe aliases/untyped legacy-only edges, and Runtime scope avoids artifact inventory IO. | `WorkflowBodies::extend_registered` requires Active state; body lookup checks identity and optional UID; Skald receives only the locked Workflow/Agent/Prompt bodies. Later versions do not float and failed loading publishes no partial Workflow. |
| Registration validation authority | `EffectiveSpecs::resolve` inventories canonical reference slots under one tenant connection and maintains distinct sibling/external stores. `validate_workflows` follows transitive Agent/Prompt references and delegates pure/resolved validation to Skald without binding tools. | The preflight returns exact `(CardRef, CardUid)` pairs; registration executes no provider/tool and resolves no execution secret. Invalid graphs return before the write transaction. |
| Durable UID fence | Preflight pairs enter `RegistrationPlan`; `RegistrationWriter::write` appends audit, calls `recheck_active_card_refs`, then binds and persists. SQL requires exact identity, expected UID, and Active state under `FOR SHARE`. | A lifecycle change or replacement UID cannot be bound after a different body was validated. The lock survives through relationship insertion and commit; refusal rolls back audit/bookkeeping/cards/relationships together. |
| Root version intent | `graph_ready_submissions` creates validation-only holders for omitted/scoped fresh root versions; original submissions remain in the request hash, replay seed, registration plan, and version allocator. | Declarative Workflow validation can run before allocation without converting the durable authored intent. Omitted/scoped roots still allocate and reload through the normal owner. |
| Runtime ownership | The shared client `Workflow` facade owns client IO composition; loader filesystem work and entry canonicalization execute in Tokio's installed blocking pool; Cards owns registry traversal; Skald owns lowering, validation, binder, and execution. | Rust reexports the facade. Python detaches the GIL and uses the shared runtime. N-API owns the native Workflow across awaits and TypeScript projects the canonical snapshot. No second parser, graph traversal, validator, transport, cache, or executor exists. |
| Workflow selector errors | Python and TypeScript parse untyped public selector fields; shared `WorkflowCards::load` checks version presence and Card kind before traversal. Commit `375d97e67` maps incomplete/mixed/wrong-kind cases to `WorkflowInvalidCardRef`. | Ordinary malformed shape/kind cases now converge, but an invalid exact version can still inhabit `VersionBlock` through derived deserialization and escape to the server in Rust/TypeScript. This leaves the prior selector invariant partially open (INV-R4-001). |

## Acceptance matrix

| Requirement, criterion, constraint, or non-goal | Implementation evidence | Verification evidence | Result |
|---|---|---|---|
| REQ-001/002: one normal Workflow Card with inline/path/versioned Agent steps | Existing envelope and Skald action types; loader path projection; no alternate YAML wrapper. | Checked-in seven-Card code-review fixture and loader/client journey assertions. | PASS |
| REQ-003/040, INV-014: Prompt remains request/model/binder owner | `CardBodyResolver` constructs existing Agent/Prompt values and delegates the existing Prompt resolver/binder; no renderer or provider schema added. | Three language journeys assert distinct reviewer text and final bound output. | PASS |
| REQ-013/013A/014: pure and resolved validation at earliest informed boundaries | Loader invokes pure validation; client hydration and `EffectiveSpecs::validate_workflows` delegate resolved validation to Skald before publication/write. | Loader cycle, binding/dialect, raw registration, no-dispatch and no-partial-write assertions are present. | PASS |
| REQ-024/054 and INV-007: exact Rust/Python/TS authored and registered-local APIs over one runtime | Shared facade/view; Rust reexports; Python `from_path`/typed Cards view; N-API/TS async projections. | Owning runtime journeys cover public load/run paths and generated declarations. Malformed version error parity fails below. | FAIL — INV-R4-001 |
| REQ-025/055: shared loader, lazy ambient client, zero registry IO when fully local | `Workflow::from_path` runs loader first and constructs `Cards` only when `external_refs` is nonempty. | Local success without credentials plus missing/denied/deleted external dependency flows in the language journeys. | PASS |
| REQ-028, AC-002: declarative composite registration with exact relationships | Existing registration plan/writer and reference binding; stored specs and relationship rows use the same exact UIDs. | PG proof plus strengthened Rust/Python/TS assertions cover Workflow→three Agents and each Agent→Prompt at both layers. | PASS |
| REQ-029/056, INV-005: Active, exact, authorized graph; provenance and no dependency floating | Exact Cards reads, UID relationship traversal, Active checks, separate provenance stores, tenant-scoped server resolution. | Missing/denied/inactive/mismatch/foreign/ref collision and after-v2 runs are covered. | PASS |
| REQ-052/INV-008: tools remain declarations at registration and bind only in execution environment | `validate_card_bodies` clears tools only on validation copies; persisted specs remain unchanged; runtime uses the existing registry. | Built-in declarations register and local explicit/default execution paths remain covered. | PASS |
| REQ-057/059, AC-031: reuse owners, remove obsolete machinery, preserve protections | Old WorkflowLoader/WorkflowGraph/keyed normalization is absent; existing loader, hydrator, EffectiveSpecs, writer, visitor, and Skald owners are used. | Source/diff audit and retained loader/Service/boundary checks in implementation evidence. | PASS |
| INV-002/003: pure PyO3-free contracts and declarative Cards | `wyrd-spec` changes are synchronous data/error/reference contracts only; live clients/tools stay outside Cards. | Source/manifests and recorded PyO3/client-tier checks. | PASS |
| AC-001/003: actual bundle and registered graph execute locally with equivalent binding/output semantics | Same hydrated Skald Workflow backs each SDK. | Rust/Python/TS journeys execute local, mixed, shadowed, exact, UID, and after-v2 paths. | PASS |
| AC-006/013: stable refusal before provider/write side effects | Shared validation and fail-before-write/load composition. | Focused loader/client/server negatives and recorded relevant broad lanes. | PASS except malformed exact-version code parity in INV-R4-001 |
| AC-029: each language proves exact relationships and no floating | R3 adds structured receipt-to-spec/outbound comparisons for all three Agent/Prompt pairs without runtime changes. | Existing exact three language journey commands reportedly pass. | PASS |
| AC-030: each language proves lazy credentials, negative selectors/dependencies, collision, no dispatch; server retains no-write/UID-race/no-principal proof | Existing shared owners and new Workflow selector catalog error. | Required branches are present, but malformed exact-version selectors do not retain the selected Workflow error in Rust/TS. | FAIL — INV-R4-001 |
| Async filesystem isolation and cancellation boundary | `Workflow::from_path` moves loader plus canonicalization together into `tokio::task::spawn_blocking`; later registry IO remains async. | Source inspection and recorded shared/Rust/Node journeys. | PASS |
| Generic non-Workflow Python selector error semantics | Shared generic parser now constructs `WyrdError::Validation`; Workflow retains its specialized variant; Data body errors remain DataValidation. | Parameterized unit coverage and genuine Data validation regression are present. | PASS |
| Generated state and patch hygiene | Hakari feature union regenerated by sanctioned command; prior report's EOF blank removed. | Recorded `check:workspace-hack` and explicit cumulative `git diff --check` pass. | PASS |
| No Workflow principal/WyrdState root, secret resolution, registration-time execution, server-run Python/TS lifecycle, MCP surface, or TASK-003/004/005 implementation | No such owner or surface appears in the executable diff; later task boundaries remain separate. | Source/cumulative diff audit and PG principal/no-dispatch assertions. | PASS |

## Prior-finding closure

| Stable finding | R4 invariant disposition |
|---|---|
| FIND-TASK-002-1 | Closed. Client and server body producers retain Ref/Sibling provenance through the consumer lookup; public collision runs observe both bodies. |
| FIND-TASK-002-2 | Closed. The actual bundle contains exact-version Prompt Cards and both Workflow→Agent and Agent→Prompt reference/relationship layers. |
| FIND-TASK-002-3 | Superseded and closed under Revision 12. Canonical untagged reference types/visitor remain; removed keyed normalization was not rebuilt. |
| FIND-TASK-002-4 | Closed. Expected preflight UID participates in the write-time Active row lock and is the UID later bound/persisted. |
| FIND-TASK-002-5/6 | Closed at the corrected boundaries. Current loading/native items have substantive contracts and imports remain module-scoped. |
| FIND-TASK-002-7 | Closed. Every owning language now asserts successful Native behavior, provenance, after-v2 pins, and the complete exact relationship graph. |
| FIND-TASK-002-8 | Closed. TypeScript accepts only recursive JSON input and projects the complete canonical run/step/error snapshot. |
| FIND-TASK-002-9 | Closed. Hydrator, EffectiveSpecs, and RegistrationWriter are cohesive owners without relocating transaction/audit/lifecycle behavior. |
| FIND-TASK-002-10 | Closed. Graph-only holders validate omitted/scoped roots while original durable version intent reaches allocation. |
| FIND-TASK-002-11 | **Partially open.** Commit `375d97e67` closes incomplete, mixed, wrong-kind, and normally malformed Python/TS fields, but Rust/TypeScript can still pass a deserialized invalid exact version to a server read and receive a registry error. See INV-R4-001. |
| FIND-TASK-002-12 | Closed. Generic Python registry identity failures use request Validation; true Data body validation and Workflow-specific selectors keep their owners. |
| FIND-TASK-002-13 | Closed. Synchronous filesystem work is isolated on Tokio's standard blocking pool, with truthful abandonment semantics. |
| FIND-TASK-002-14 | Closed by sanctioned Hakari regeneration; no exemption or new check was introduced. |
| FIND-TASK-002-15 | Closed. The cumulative patch whitespace failure was removed without changing historical report content. |

## Proposed finding

### INV-R4-001 — INCORRECT — FIND-TASK-002-11 remains open for malformed exact versions in Rust and TypeScript

**Violated obligation.** Revision 12 REQ-054 and INV-007 require the same public
Workflow loading/error semantics; AC-030 requires wrong selectors to be refused
at the loading boundary. The validated R3 FIND-TASK-002-11 specifically requires
malformed Workflow UID/space/name/**version** fields to produce
`WYRD_WORKFLOW_400_INVALID_CARD_REF` at the owning boundary without registry IO.
The public contract accepts an exact semantic version, not a version range.

**Exact locations.** `crates/shared/wyrd-semver/src/block.rs:13-45` derives
`Deserialize` directly on the private-string `VersionBlock`, bypassing
`VersionBlock::parse`; `crates/shared/wyrd-client/src/workflow.rs:167-190` checks
only missing versions and wrong kinds; `sdks/wyrd-sdk-ts/native/src/workflow.rs:76-96`
deserializes the public JSON selector into `CardRef`; and
`crates/shared/wyrd-client/src/cards/reads/get.rs:67-85` then sends that version
to `/v1/cards/by-ref`. The reachable candidate regression assertion is
`crates/wyrd/wyrd-server/tests/pg_workflow_registration.rs:790-813`, which
constructs `version: "^1.0.0"` through Serde and expects
`WYRD_REGISTRY_400_INVALID_CARD_SPEC` only after a server read. R3 implementation
evidence explicitly records the corresponding Rust/TypeScript behavior as an
unfixed observation.

**Producer-to-consumer evidence.** A normal `VersionBlock::parse("^1.0.0")`
fails because this type stores concrete semver versions. Derived deserialization,
however, writes the inner string without calling that constructor, creating a
state the type's own API says is invalid. TypeScript's public
`cards.workflow.load({space,name,version:"^1.0.0"})` reaches exactly that path.
`parse_workflow_selector` sees a successfully deserialized `CardRef`,
`WorkflowCards::load` sees a present Workflow version, and the generic Cards read
makes network IO. The server eventually rejects the invalid path/query version as
`WYRD_REGISTRY_400_INVALID_CARD_SPEC`. Python uses `VersionBlock::parse` and
returns `WYRD_WORKFLOW_400_INVALID_CARD_REF` locally, so the same public input has
different error ownership and IO behavior. Rust can reach the same state through
public Serde deserialization of its public selector/reference types.

**Observable consequence.** Agents and callers branching on the selected
Workflow error instead receive a registry-spec error in two first-class
languages, and malformed input performs an authenticated registry request despite
the no-IO selector-refusal contract. Commit `375d97e67` therefore does not fully
close the prior stable finding. This is not speculative: the server integration
test intentionally exercises and pins the path.

**Required testable correction.** Restore the existing validated-newtype
invariant at its producer using ordinary Serde newtype validation: deserialize
`VersionBlock` through its existing `FromStr`/`parse` contract rather than
writing the private string directly. Keep the current Workflow boundary mapping
of TS raw-field decode failures to `WorkflowInvalidCardRef`; typed Rust should no
longer be able to construct this invalid value through normal public
deserialization. Update the existing Rust/TypeScript Workflow selector negatives
so `^1.0.0` is refused locally with the selected Workflow code, status, and
`details.field = "version"`, and prove no request occurs. Preserve valid exact
versions, prerelease/build pins, generic non-Workflow request Validation, server
defense in depth, and every existing Cards read behavior. Do not add a second
version validator, scanner, allowlist, option, compatibility shim, or test
harness; validated deserialization of a private-field newtype is the standard
mechanism already implied by `VersionBlock::parse`.

## Review finding summary

### Critical

None.

### Important

- `[crates/shared/wyrd-semver/src/block.rs:13]` Derived deserialization bypasses
  the concrete-version newtype constructor, leaving FIND-TASK-002-11 incomplete
  for Rust/TypeScript Workflow selectors. Impact: malformed exact versions make
  registry IO and return a different stable error than Python. Recommended fix:
  validate at `VersionBlock` deserialization and retain the current Workflow
  boundary error mapping and focused negative journeys.

### Suggestions

None. Optional refactors and unrelated hardening are outside this acceptance
review.

## Open questions

None. The approved exact-version contract and existing validated constructor
resolve the correction boundary without a new product, architecture, security,
compatibility, concurrency, resource-ownership, or persistent-data decision.

## Verification notes

No commands were run by this reviewer. Inspected evidence records the focused
loader/client/server and three SDK journeys, generic Python selector unit proof,
workspace-hack, codegen, type/native checks, relevant format/lint/boundary lanes,
and cumulative diff hygiene as passing. Those results credibly close the prior
findings listed above, but they do not close INV-R4-001 because the current server
test asserts the inconsistent registry error and no Rust/TypeScript local malformed
version/no-request assertion exists.
