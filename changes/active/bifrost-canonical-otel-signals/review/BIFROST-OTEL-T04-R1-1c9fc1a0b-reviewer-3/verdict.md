# BIFROST-OTEL-T04-R1 task review

## Verdict

`FIX_REQUIRED`

## Immutable subject

- Repository: `/Users/stevenforrester/Documents/GitHub/wyrd`
- Original task base: `442da074cb316be7f580694ba8274229561935a8`
- Prior candidate: `81eaa346e643ac6315e517041fc838c41057f7ac`
- Remediation base: `e2ebc45a6359dad784614ab238540ea85543298c`
- Cumulative candidate: `1c9fc1a0bd5b54733e241902b671f1f00345d892`
- Approved specification: `changes/active/bifrost-canonical-otel-signals/spec.md`, revision 11 as it existed at the original task base
- Original task: `changes/active/bifrost-canonical-otel-signals/tasks/04-integrated-public-journeys.md` as it existed at the original task base
- Prior verdict and remediation: `changes/active/bifrost-canonical-otel-signals/review/BIFROST-OTEL-T04-81eaa346e-reviewer-2/`

The original task acceptance is reassessed cumulatively. Changed-surface and unrelated-change review uses the user's explicit three-commit remediation range `e2ebc45a6..1c9fc1a0b`; work already integrated at the remediation base is not attributed to this remediation.

## Acceptance matrix

| Requirement, criterion, constraint, or non-goal | Implementation evidence | Verification evidence | Result |
|---|---|---|---|
| REQ-002 Gate owns authenticated routing and request identity | `gate/mod.rs:117-136,663-719` derives and routes one authenticated identity | focused Gate test | PASS |
| REQ-004 equivalent public writes preserve stored semantics and row identity | existing convergence path retained; OTLP identity now binds tenant, principal, table, logical content, and accepted correlation | Gate unit test and public replay journey | PASS |
| REQ-005 mixed requests atomically retain accepted siblings | existing projection and Scribe fence retained | independently rerun mixed OTLP journey | PASS |
| REQ-006–REQ-018 complete canonical trace/log/metric fidelity | cumulative T04 implementation unchanged by R1 except retry identity/test support | prior accepted raw and stock-SDK journey evidence | PASS |
| REQ-019–REQ-021 canonical query, stale removal, and single mapping authority | no remediation regression | prior accepted focused and journey evidence | PASS |
| REQ-022 authenticated publisher and optional Card/Run attribution | principal is bound directly; accepted `card_ref` remains in logical content and `run_id` enters the correlation digest | independently rerun two-principal/run journey | PASS |
| REQ-023 payload authorization | no remediation change | prior accepted public read journeys | PASS |
| INV-001 one canonical row per accepted signal | same-principal replay converges; distinct publisher or run attribution diverges | two-principal journey | PASS |
| INV-002–INV-006 fidelity, schema identity, signal boundaries, canonical source, and payload safety | no contrary remediation change | cumulative accepted evidence | PASS |
| INV-007 tenant isolation | authenticated tenant remains a fixed identity input and Scribe boundary | unit test plus independent security audit | PASS |
| INV-008 acknowledgement means authoritative durable rows | distinct attribution no longer shares one fence | two-principal journey through publish/read | PASS |
| INV-009 bounded processing | both digests traverse only the admitted bounded Arrow batch | source inspection and focused test | PASS |
| INV-010 existing harness/no shadow path | existing Gate, Scribe fence, trace helper, and OTLP target reused | remediation diff inspection | PASS |
| INV-011 publisher identity without mandatory Card identity | principal is token-derived; no Card inference or lookup added | source inspection and security audit | PASS |
| AC-001–AC-005 public fidelity and GenAI coverage | prior implementation preserved | prior accepted journey evidence | PASS |
| AC-006 exact partial success and accepted-subset replay | one-principal retry still converges on the accepted rows | independently rerun mixed journey | PASS |
| AC-007 durability and recovery identity | publisher and stable accepted correlation now participate without changing the Scribe fence | focused unit and public journey | PASS |
| AC-008–AC-011 schema/storage, topology, complete verification, and stock SDKs | no regression found; aggregate limit recorded truthfully | prior evidence plus reported 15/16 `verify:bifrost` and isolated Forge pass | PASS |
| Preserve public/persisted contracts and existing fence | no public field, route, schema, migration, dependency, feature, or second fence added | remediation diff inspection | PASS |
| Preserve native canonical Arrow behavior | native caller-supplied batch identity path is unchanged | source/diff inspection | PASS |
| No new mapper, collector, harness, target, or compatibility path | none added | remediation diff inspection | PASS |
| Mandatory import and bare-signature style | Gate correction passes; trace test helper and modified wrapper still use fully qualified signature types | independent standards audit | FAIL |
| Exact durable verification evidence | new journey exists and independently passes, but its implementation record abbreviates the exact command | independent standards audit | FAIL |
| Repository standards | independent standards audit | `standards-review.md` | FAIL |

## Repository-standards result

`FAIL`. The independent audit found two bounded remediation-only violations, both included below.

## Security, tenancy, and durability review

`PASS`. The fresh specialist found no material security, tenant-isolation, attribution, replay/idempotency, durability, collision, or audit issue. The same verified `AuthContext` supplies the identity, audit, Scribe principal, and tenant; Scribe remains the durable fence owner.

## Material findings

### FIND-BIFROST-OTEL-T04-7 — VIOLATION

- Violated obligation: `architecture/agent-rules.md` requires types to be imported at module scope and used as bare names in signatures.
- Location: `crates/wyrd/wyrd-testing/tests/bifrost/otlp/trace_export.rs:32-55`.
- Evidence: the new `export_traces_over_grpc_as` helper and materially modified `export_traces_over_grpc` wrapper use fully qualified `ResourceSpans` and `ExportTracePartialSuccess` types.
- Observable consequence: the changed Rust fails a mandatory repository style rule.
- Required correction: use module-top imports and bare names for those types in both changed signatures, with no behavior change.

### FIND-BIFROST-OTEL-T04-8 — VIOLATION

- Violated obligation: `AGENTS.md` §11 requires an exact runnable focused command for every specifically named Rust test in a task artifact or implementation report.
- Location: `changes/active/bifrost-canonical-otel-signals/review/BIFROST-OTEL-T04-81eaa346e-reviewer-2/BIFROST-OTEL-T04-R1-close-retry-identity-review-gaps.md:146-158`.
- Evidence: the new two-principal journey is named, but the recorded command uses `-E '...'` rather than the exact expression.
- Observable consequence: the durable implementation evidence does not reproduce the exact named journey invocation.
- Required correction: record the complete `mise exec --` Postgres-wrapped command and passing result for the named journey.

## Ponytail audit

The behavioral correction is the minimum root-cause change: it reuses the Gate identity, one existing recursive Arrow walker, the Scribe fence, and the existing OTLP journey target. No dependency, feature, schema, migration, store, trait, or compatibility path was added. The remaining corrections require only imports/signatures and an exact evidence line.

## Verification limits

- Independent Gate identity test: PASS, 1/1.
- Independent two-principal Postgres journey: PASS, 1/1.
- Independent mixed-replay Postgres journey: PASS, 1/1.
- Independent `mise exec -- cargo fmt --all --check`: PASS.
- Cumulative `git diff --check`: PASS.
- Reported `mise run fmt` and `mise run lints`: PASS.
- Reported `mise run verify:bifrost`: 15/16; the Forge publication-cancellation failure passed alone under the same Postgres lane and no `forge/` file changed in R1.
- The settled UUID representation finding was not reopened under current user authority.
- The shared branch advanced after evidence collection, but the explicitly pinned candidate commit and reviewed range remain immutable.

## Prior-finding closure

- `FIND-BIFROST-OTEL-T04-1`: CLOSED — authenticated principal and accepted correlation attribution now separate durable fences; public proof passes.
- `FIND-BIFROST-OTEL-T04-2`: WITHDRAWN — preserved as settled user authority.
- `FIND-BIFROST-OTEL-T04-3`: CLOSED for production Gate code; the separate test-helper signature violation is `FIND-BIFROST-OTEL-T04-7`.
- `FIND-BIFROST-OTEL-T04-4`: CLOSED — required remediation rustdoc is present.
- `FIND-BIFROST-OTEL-T04-5`: WITHDRAWN BY USER.
- `FIND-BIFROST-OTEL-T04-6`: WITHDRAWN BY USER.

## Remediation task

`BIFROST-OTEL-T04-R2-close-remediation-standards.md`
