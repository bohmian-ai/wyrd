# Invariant Review: TASK-002

## Immutable subject

- Repository: `/home/thorrester/Documents/GitHub/wyrd`
- Base: `0569b79702218600c4f9790f45cc03100d5c6f1c`
- Candidate: `e165360b1264d3628b13b02c41567c047bf96930`
- Approved authority: `changes/active/skald-workflow-runtime/spec.md`, Revision 11
- Original task: `changes/active/skald-workflow-runtime/tasks/TASK-002-load-and-register-graphs.md`
- Candidate stability: `HEAD` resolved to the candidate before focused verification.

## Review Findings

### Critical

None.

### Important

#### INV-REV-001 — INCORRECT: local sibling bodies can satisfy authored external refs

- Violated obligation: REQ-025; INV-005; the task's requirement that authored refs use Cards exact reads and that missing registry access fail clearly; `architecture/wyrd-design.md` lines 1615-1624, which require `Sibling` and authored `Ref` provenance to remain distinct even when their identities match.
- Exact location: `crates/shared/wyrd-client/src/workflow_loader.rs:95-102`, `:221-230`, `:238-263`, and `:340-347`; the same identity-only lookup is used for Prompts at `:376-385`.
- Evidence: `load_file` inserts every path-loaded Agent and Prompt into one `HashMap<CardRefIdentity, ...>`. `missing` then treats any `Ref` or `Sibling` with that identity as satisfied, because `as_card_ref()` erases the variant and the map key contains only kind/space/name/version. `GraphResolver` performs the same identity-only lookup. Therefore a Workflow containing a path-loaded Agent and an authored external Agent `ref` with the same identity hydrates both steps from the local body; no registry client or exact Cards read is required. The same collision is reachable transitively for Agent Prompt refs.
- Observable consequence: a caller can request a registered dependency but execute a different disk body carrying the same declared identity. With no registry client, the load succeeds instead of returning `WYRD_REGISTRY_422_UNRESOLVED_DEPENDENCY`; with a client, the exact registered UID/body is never checked. This breaks the authored reference trust boundary and exact-graph invariant.
- Required testable correction: preserve `Sibling` versus external `Ref` provenance through graph composition and resolution so every external ref always performs and consumes its exact Cards read, while a sibling consumes only the path-loaded body. Add one focused local-loading test with a path and a ref sharing kind/space/name/version but containing observably different Agent or Prompt bodies; prove the ref fails without a client and, with a client, uses the registered body rather than the sibling. The correction belongs at the shared graph/body owner, not as repeated caller guards.

#### INV-REV-002 — MISSING: the checked-in acceptance bundle has no exact Prompt Card identities

- Violated obligation: AC-002 (`spec.md:2262-2263`) and TASK-002 Scenario 2 (`TASK-002-load-and-register-graphs.md:100-104`), which require the same checked-in code-review bundle to register locked Agent and Prompt refs/relationships in dependency order.
- Exact location: `examples/workflows/code-review/agents/security.yaml:8-10` and the equivalent `prompt.inline` fields in `correctness.yaml` and `final-reviewer.yaml`; proof at `crates/wyrd/wyrd-server/tests/pg_workflow_registration.rs:318-387`.
- Evidence: all three checked-in Agents embed anonymous inline Prompts. Registration consequently produces four Cards, and the acceptance test explicitly asserts only three Workflow relationships, all `Agent` (`pg_workflow_registration.rs:322`, `:381-384`). The separate temporary tooling fixture at `:389-426` proves that a different one-step graph can lock one Prompt ref; it does not establish the required identities or relationships for the same code-review YAML bundle named by AC-002.
- Observable consequence: the shipped code-review bundle cannot preserve, inspect, deactivate, or fetch exact Prompt Card versions because those Cards and identities do not exist. The claimed end-to-end exact Agent/Prompt relationship proof for the acceptance artifact is absent.
- Required testable correction: make the checked-in code-review bundle's reviewer Prompts separate versioned Prompt Cards referenced through the existing Agent `path`/ref model, then have the existing real registration journey assert those Prompt outcomes, UID-bearing Agent-to-Prompt refs, and stored exact relationships for that same bundle. Reuse the loader and registration mechanisms already exercised by the temporary tooling graph; do not add another loader or fixture-only contract.

### Suggestions

None.

## Acceptance matrix

| Requirement, acceptance criterion, constraint, or non-goal | Implementation evidence | Verification evidence | Result |
|---|---|---|---|
| REQ-001: one normal Workflow Card/YAML model | Checked-in `workflow.yaml`; shared loader feeds `Workflow::from_card_with_agent_resolver` | `tests::load_explicit_workflow_bundle`; `workflow_loader::tests::hydrate_local_workflow_graph` | PASS |
| REQ-002: Agent steps support inline/path/versioned ref | Keyed forms normalize in `wyrd-loader::parse`; path refs become siblings; `WorkflowLoader` fetches absent identities | Local bundle path test and external-ref refusal case | PASS |
| REQ-003, INV-014, REQ-040: existing native Prompt and binder remain authoritative | Example native `request` bodies; graph hydration uses Skald Agent/Prompt resolvers and runtime validation | Local run proves declared values reach the final Prompt | PASS |
| REQ-013: pure graph validation at load | `wyrd-loader::validate_card` calls `WorkflowSpec::validate` | Checked-in bundle cycle mutation is refused | PASS |
| REQ-013A, REQ-014: resolved validation at local load, registration, and registered load | `WorkflowGraph::{hydrate,validate}`; `EffectiveSpecs::validate_workflows` | Local extra-binding/route cases and PG registration matrix | PASS |
| REQ-024: Rust local file and registered-local journeys in this task slice | Public `WorkflowLoader::{load_file,load_registered}` | Local unit journey and PG registered-local journey source | PASS |
| REQ-025: path is disk-local; ref requires exact registry access | `fetch_missing` calls `read` only for identities absent from the shared body map | Unique-identity no-client refusal passes, but path/ref identity collision bypasses the read | FAIL (INV-REV-001) |
| REQ-028: declarative composite registration, no execution | Existing Cards registration plus pre-write Workflow validation | PG registration journey records no provider execution | PASS |
| REQ-029 and INV-005: registered graph is active and exactly pinned | Cards exact selector asserts optional UID; loaded Workflow retains root identity | PG journey covers newer-version non-floating, UID mismatch, foreign, pending, and deleted refs | PASS for registered loading; authored-ref collision fails the broader exact-reference invariant (INV-REV-001) |
| REQ-052 and INV-008: preserve tool names; registration does not reject declared built-ins | Registration validation disables tool binding only for declarative validation; runtime hydration binds caller resolver | Temporary tooling graph preserves `bifrost.query` and `cards.get` | PASS |
| INV-002: pure contracts remain synchronous and in `wyrd-spec` | Loader invokes existing `WorkflowSpec::validate`; no IO moved into `wyrd-spec` | Static dependency/diff inspection | PASS |
| INV-003: Workflow Cards stay declarative | Client handle owns Cards IO and tools; `WorkflowGraph` owns pure bodies only | Static source inspection | PASS |
| AC-001: actual code-review bundle parses and executes through existing binder | Checked-in Workflow plus three Agent files | Both focused tests passed | PASS for Rust portion scoped to TASK-002; CLI closure is assigned to TASK-005 |
| AC-002: same checked-in bundle stores exact Agent and Prompt Card versions/relationships | Bundle uses three inline Prompts; registration asserts four Cards and Agent-only relationships | Different temporary tooling bundle proves one Prompt ref, not the acceptance bundle | FAIL (INV-REV-002) |
| AC-003: registered code-review graph executes locally equivalently | `load_registered`; exact reads; shared Skald runtime | PG test compares outputs and step keys with authored external-ref load | PASS for Rust portion scoped to TASK-002; CLI closure is assigned to TASK-005 |
| AC-006: negative contract boundaries dispatch nothing | Pure/resolved validators and pre-write server validation | Local binding/route/cycle cases and PG binding/output/graph/route matrix | PASS for task-selected cases |
| AC-013: focused loader, registration, and client evidence | Loader/client unit tests and real client/server PG target | Two focused unit selectors independently rerun; PG source and recorded task evidence inspected, not rerun here | PASS with verification limit below |
| Non-goals: no registration during load, secret/endpoint resolution, Skald registry dependency, WyrdState root, second executor/schema | Diff keeps IO in `wyrd-client`/server and makes only Skald documentation changes | Static complete-diff and manifest inspection | PASS |

## Producer-to-sink invariant trace

- Identity and space: the loader resolves path targets relative to the containing file, inherits space, and emits `Sibling` exact identities; server binding replaces sibling/external refs with UID-bearing refs before persistence. Registered reads assert kind/space/name/version and optional UID.
- Specs and validation: pure Workflow validation occurs in `wyrd-loader` and `WorkflowGraph::new`; resolved Agent/Prompt validation occurs through the existing Skald resolver seam before local execution or registration persistence.
- Lifecycle and tenancy: registered reads use tenant-authenticated Cards APIs and reject non-active dependencies; registration resolution uses `TenantConn` and Active-card lookup. No cross-tenant or lifecycle bypass was found in the registered path.
- Tools and dispatch: declarative registration clears names only in cloned validation bodies, while executable hydration preserves names and binds the caller registry. Invalid graph cases reach neither tool nor provider dispatch.
- Reference provenance: this is lost when `Ref` and `Sibling` bodies enter the identity-only `WorkflowGraph` map, producing INV-REV-001.

## Open Questions

None. Both findings are resolvable within the approved behavior and existing owners; no specification decision is needed.

## Verification Notes

- Passed: `CARGO_TARGET_DIR=/home/thorrester/Documents/GitHub/wyrd/target mise exec -- cargo nextest run --locked -p wyrd-loader --lib -E 'test(=tests::load_explicit_workflow_bundle)'` (1 passed).
- Passed: `CARGO_TARGET_DIR=/home/thorrester/Documents/GitHub/wyrd/target mise exec -- cargo nextest run --locked -p wyrd-client --lib -E 'test(=workflow_loader::tests::hydrate_local_workflow_graph)'` (1 passed).
- Passed: `git diff --check` for the immutable base-to-candidate range.
- Not independently rerun: the Postgres target and broader `mise` lanes. Their source and task-recorded results were reviewed; this report does not upgrade those records into an independent rerun.
- Residual proof gap: no test covers a local `Sibling` and authored external `Ref` sharing one exact identity, and the checked-in acceptance bundle does not contain Prompt Cards.

## Overall result

**FAIL** — two bounded task violations remain: external-ref provenance can be shadowed by a same-identity local sibling, and the same checked-in code-review bundle required by AC-002 does not register exact Prompt Cards/relationships.
