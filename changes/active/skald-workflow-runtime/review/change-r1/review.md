# Skald workflow runtime final change review

## Verdict

**PASS**

No critical bug is present on the primary user path. Target
`395a3ad91a1dfa0706bc8c08c8dc3100dce3d78d` satisfies approved
`SPEC-skald-workflow-runtime` Revision 14 at the human-directed acceptance
threshold. The supplied base
`7d96c30066425e0cde2290842d5801307843283d` is the target's merge-base with
`main`.

The review was source- and recorded-evidence-only. It ran no build, test, lint,
format, code-generation, package, server, or executable verification command.
No required reviewer was unavailable.

## Immutable subject

- Branch: `wyrd/skald-workflow-runtime/complete`
- Base: `7d96c30066425e0cde2290842d5801307843283d`
- Target: `395a3ad91a1dfa0706bc8c08c8dc3100dce3d78d`
- Specification: `changes/active/skald-workflow-runtime/spec.md`, approved
  Revision 14
- Tasks: TASK-001 through TASK-005, including all recorded remediation tasks
- Delivery reference: not supplied
- Review mode: complete base-to-target diff, surrounding source, current
  architecture, task reviews, lead dispositions, and recorded evidence

`.codegraph/` is absent, so the review used Git, `rg`, and direct source
reading. The worktree was clean and `HEAD` remained the target until this
review output was written.

## Material findings

None under the human direction that only a critical bug on the primary user
path blocks this change.

## Known deferred follow-ups

These are visible, accepted, and non-blocking. They do not authorize a bespoke
check, setting, file, option, harness, compatibility path, or other mechanism
that is absent from established Wyrd practice and comparable widely used
projects.

| Finding | Class | Current consequence | Disposition |
|---|---|---|---|
| `FIND-TASK-004-17` | VIOLATION | Historical task text names Revision 13. | Human-deferred; no runtime effect. |
| `FIND-TASK-004-24` | REGRESSION | Unsupported Vertex media is labelled Google in one error. | Human-deferred; error attribution only. |
| `FIND-TASK-004-25` | REGRESSION | Provider is absent from an unused cache-key constructor. | Human-deferred; no production caller. |
| `FIND-TASK-004-26` | VIOLATION | Rustdoc has stale schema/destination wording. | Human-deferred; documentation only. |
| `FIND-TASK-004-27` | VIOLATION | One construction seam lacks required `# Errors` rustdoc. | Human-deferred; documentation only. |
| `FIND-TASK-005-1` | DRIFT | `--server` is refused with an authored `--file`; ambient/configured endpoint selection works. | Human-deferred; explicit override edge only. |
| `FIND-TASK-005-2` | VIOLATION | Invalid modes read `--input-file` before refusal. | Human-deferred; failure-order edge only. |
| `FIND-TASK-005-3` | VIOLATION | Two test helpers import `PermissionsExt` inside functions. | Human-deferred; test-source placement only. |
| `FIND-TASK-005-4` | VIOLATION | Ctrl-C listener-install failure is reported as interruption. | Human-deferred; OS failure is outside the primary path. |
| `CHANGE-R1-DRIFT-001` | DRIFT | `skald-providers/src/clients/external.rs` serializes successful responses and substring-scans them for secret-header values. This bespoke reflection detector is not a normal provider-client control and can false-refuse valid output. | Non-blocking. A later simplification may remove it while preserving standard error/log redaction; do not replace it with another special mechanism. |
| `CHANGE-R1-INCORRECT-001` | INCORRECT | The approved TypeScript example shows `new Cards()`, while the shipped API requires `Cards.connect()`. | Non-blocking public-ergonomics documentation mismatch; registered loading itself is journey-proven. |
| `CHANGE-R1-MISSING-001` | MISSING | The served-OpenAPI test does not specifically assert `/v1/workflow-runs*`, so AC-012's route-specific proof is incomplete even though the routes and annotations share one `OpenApiRouter`. | Non-blocking evidence limit; ordinary route/OpenAPI assertion is sufficient if closed later. |

TASK-001 also records two non-blocking harness limits: its new teardown test is
postcondition proof rather than a pre-fix-failing regression, and implicit drop
can hang on a single-thread Tokio runtime when live governed work needs that
blocked runtime. Explicit async shutdown avoids the latter; neither is a
critical primary-user-path failure.

## Task-review closure

| Task | Reviewed closure at this target | Result |
|---|---|---|
| TASK-001 | The last formal verdict required the Bifrost abort fence. Fresh target-bound closure confirms `settle_lifecycle` keeps graceful work bounded, awaits `Bifrost::abort()` without the expired deadline, and records settlement afterward. Recorded 3/3 focused tests and broader lanes are credible. | PASS |
| TASK-002 | `TASK-002-r5` is a cumulative PASS. Later integration reuses the same Cards hydrator/Skald owners, and final SDK/CLI journeys exercise its consumers. | PASS |
| TASK-003 | The last formal verdict required corrected Python runtime help. Fresh target-bound closure confirms the PyO3 method, both declarations, and shared run-start behavior agree; only rustdoc changed. | PASS |
| TASK-004 | `TASK-004-r6` findings are explicitly accepted by `lead-disposition.md` under the supplied human direction. Revision 14 runtime and evidence remain intact. | PASS with known deferred follow-ups |
| TASK-005 | `TASK-005-r1` findings are explicitly accepted by `lead-disposition.md` under the supplied human direction. Target differs from the verified candidate only by review/disposition Markdown. | PASS with known deferred follow-ups |

## Evidence key

- **E1 Contract:** `crates/wyrd-spec/src/card/workflow.rs`, public errors, and
  generated Workflow schemas.
- **E2 Runtime:** `crates/skald/skald-workflow/src/{plan,workflow,run,route,bodies,workflow_surface}.rs`.
- **E3 Loading/registration:** `wyrd-client` Cards hydration and Workflow
  facade plus server Card resolution/registration.
- **E4 Remote/gateway client:** `wyrd-client/src/workflow/{remote,gateway}.rs`,
  shared transport, and public gateway ingress.
- **E5 Server lifecycle:** `wyrd-server/src/components/workflow/{host,runs,routes,tools}.rs`,
  gateway adapter, query collection, and server configuration.
- **E6 CLI/SDK journeys:** `wyrd-cli/src/workflow.rs`, compiled CLI journeys,
  and Rust/Python/TypeScript Workflow journeys.
- **E7 Provider contract:** `skald-spec` request/prompt/response types, Agent
  dispatch, server gateway projection, and the route protocol matrix.
- **E8 Telemetry:** deleted `skald-observer` and semantic tracing in Skald
  Agent/Workflow.
- **E9 Authority:** current Wyrd design, doctrine, security posture, Bifrost,
  deployment, recovery, and public Workflow documentation.
- **V1–V5:** recorded TASK-001 through TASK-005 implementation evidence. V5
  records a green `mise run gate` at `b88102317`, followed by the three exact
  ignored compiled-CLI journeys, Rust/Python/TypeScript journeys, docs check,
  and clean patch hygiene. The reviewed target changes only review Markdown
  after the TASK-005 candidate.

## Acceptance matrix — required behavior

`FAIL` below means an explicitly accepted, non-critical deviation or evidence
gap under the human-directed threshold; it is not a blocking verdict.

| Obligation | Implementation evidence | Verification evidence | Result |
|---|---|---|---|
| REQ-001 | E1–E3 | V1, V2 | PASS |
| REQ-002 | E1–E3 | V1, V2 | PASS |
| REQ-003 | E1, E2, E7 | V1, V4 | PASS |
| REQ-004 | E1, E2 | V1 | PASS |
| REQ-005 | E1, E2 | V1 | PASS |
| REQ-006 | E2 | V1 | PASS |
| REQ-007 | E1, E2 | V1 | PASS |
| REQ-008 | E1, E2 | V1 | PASS |
| REQ-009 | E1, E2 | V1 | PASS |
| REQ-010 | E2 | V1 | PASS |
| REQ-011 | E1, E2 | V1 | PASS |
| REQ-012 | E1, E2 | V1 | PASS |
| REQ-013 | E1, E2 | V1 | PASS |
| REQ-013A | E1–E3 | V1, V2 | PASS |
| REQ-014 | E2, E3, E5 | V2, V4 | PASS |
| REQ-015 | E2, E5 | V1, V4 | PASS |
| REQ-016 | E2, E5 | V1, V4 | PASS |
| REQ-017 | E2, E5 | V1, V4 | PASS |
| REQ-018 | E2, E5 | V1, V4 | PASS |
| REQ-019 | E2, E5 | V1, V4 | PASS |
| REQ-020 | E1, E2, E5 | V1, V4 | PASS |
| REQ-021 | E1, E2, E5 | V1, V4 | PASS |
| REQ-022 | E2, E5 | V1, V4 | PASS |
| REQ-023 | E2, E5 | V1, V4 | PASS |
| REQ-024 | E3, E6 | V2, V5 | PASS |
| REQ-025 | E3 | V2, V5 | PASS |
| REQ-026 | E6 | V5; `FIND-TASK-005-1` | FAIL — accepted deferred edge |
| REQ-027 | E6 | V5; `FIND-TASK-005-4` | FAIL — accepted deferred OS-error edge |
| REQ-028 | E3, E6 | V2, V5 | PASS |
| REQ-029 | E3, E5 | V2, V4 | PASS |
| REQ-030 | E4, E5 | V3, V4 | PASS |
| REQ-031 | E4 | V3, V5 | PASS |
| REQ-032 | E5 | V4 | PASS |
| REQ-032A | E5, E9 | V4 | PASS |
| REQ-033 | E5 | V4 | PASS |
| REQ-034 | E5 | V4 | PASS |
| REQ-034A | E5 | V4 | PASS |
| REQ-034B | E5, E9 | V4, V5 | PASS |
| REQ-034C | E5 | V4 | PASS |
| REQ-035 | E2, E7 | V1, V4 | PASS |
| REQ-036 | E1, E2 | V1, V4 | PASS |
| REQ-036A | E1, E4 | V1, V3 | PASS |
| REQ-037 | E2 | V1, V4 | PASS |
| REQ-038 | E2, E7 | V1, V4 | PASS |
| REQ-039 | E2, E7 | V1, V4 | PASS |
| REQ-040 | E1, E2, E7 | V1, V4 | PASS |
| REQ-041 | E3, E5 | V4 | PASS |
| REQ-042 | E2, E4, E5 | V1, V4 | PASS |
| REQ-043 | E2, E4, E5 | V1, V4; deferred label issue | PASS |
| REQ-044 | E5–E7 | V4, V5 | PASS |
| REQ-045 | E1, E5 | V1, V4 | PASS |
| REQ-046 | E4 | V3, V5 | PASS |
| REQ-047 | E2 | V1, V5 | PASS |
| REQ-048 | E2, E5 | V1, V4 | PASS |
| REQ-049 | E2, E4 | V1, V5 | PASS |
| REQ-050 | E5, E9 | V4, V5 | PASS |
| REQ-051 | E1–E7 | V1–V5 | PASS |
| REQ-052 | E5 | V4 | PASS |
| REQ-053 | E8 | V1, V5 | PASS |
| REQ-054 | E3, E6 | V2, V5 | PASS |
| REQ-055 | E3 | V2, V5 | PASS |
| REQ-056 | E3 | V2, V5 | PASS |
| REQ-057 | E1, E3 | V2, V5 | PASS |
| REQ-058 | E3, E4, E6 | V3, V5 | PASS |
| REQ-059 | E2, E3 | V2, V5 | PASS |

## Acceptance matrix — invariants

| Obligation | Implementation evidence | Verification evidence | Result |
|---|---|---|---|
| INV-001 | E1–E3, E5 | V1–V5 | PASS |
| INV-002 | E1 | V1, V5 | PASS |
| INV-003 | E1, E3 | V1, V2 | PASS |
| INV-004 | E2–E4 | V1–V5 | PASS |
| INV-005 | E3, E5 | V2, V4, V5 | PASS |
| INV-006 | E5 | V4, V5 | PASS |
| INV-007 | E1, E4, E6 | V3–V5 | PASS |
| INV-008 | E2, E5 | V1, V4 | PASS |
| INV-009 | E1, E2, E5 | V1, V4 | PASS |
| INV-010 | E2, E4, E5 | V1, V4 | PASS |
| INV-010A | E2, E4, E5 | V1, V4 | PASS |
| INV-011 | E1, E2, E7 | V1, V4 | PASS |
| INV-012 | E2, E4, E5 | V1, V4 | PASS |
| INV-013 | E5, E9 | V4, V5 | PASS |
| INV-014 | E1, E2 | V1 | PASS |
| INV-015 | E9 | V5 docs evidence | PASS |
| INV-016 | E2, E3, E6 | V1–V5 | PASS |
| INV-017 | E2, E4 | V1, V5 boundary evidence | PASS |
| INV-018 | E5 | V4 | PASS |
| INV-019 | E5 | V4 | PASS |
| INV-020 | E2, E4, E5 | V1, V3, V4 | PASS |
| INV-021 | E2, E4, E5 | V1, V4 | PASS |
| INV-022 | E5, E9 | V4 | PASS |
| INV-023 | E2, E5 | V1, V4 | PASS |

## Acceptance matrix — acceptance criteria

| Obligation | Implementation evidence | Verification evidence | Result |
|---|---|---|---|
| AC-001 | E3, E6 | V2, V5 | PASS |
| AC-002 | E3, E6 | V2, V5 | PASS |
| AC-003 | E3, E6 | V2, V5 | PASS |
| AC-004 | E4–E6 | V4, V5 | PASS |
| AC-005 | E2 | V1 | PASS |
| AC-006 | E1–E3 | V1, V2 | PASS |
| AC-007 | E2 | V1 | PASS |
| AC-008 | E1, E2, E5 | V1, V4 | PASS |
| AC-009 | E5 | V4 | PASS |
| AC-010 | E5 | V4 | PASS |
| AC-011 | E1, E2, E7 | V1, V4 | PASS |
| AC-011A | E4, E5 | V3, V4 | PASS |
| AC-012 | E1, E5 | Codegen evidence passes; served-route assertion absent | FAIL — accepted verification gap |
| AC-013 | E1–E7 | V1–V5 | PASS |
| AC-014 | E3–E6 | V4, V5 | PASS |
| AC-015 | E6, E7 | V4, V5; deferred Vertex label | PASS |
| AC-016 | E2, E4–E6 | V1, V4, V5 | PASS |
| AC-017 | E5, E6 | V4, V5 | PASS |
| AC-018 | E5, E6, E9 | V4, V5 | PASS |
| AC-019 | E1, E4, E5 | V1, V3, V4 | PASS |
| AC-020 | E2, E5 | V1, V4 | PASS |
| AC-021 | E5 | V4 | PASS |
| AC-022 | E5 | V4 | PASS |
| AC-023 | E2, E4 | V1, V5 boundary evidence | PASS |
| AC-024 | E1, E2 | V1, V5 | PASS |
| AC-025 | E5 | V4 | PASS |
| AC-026 | E2, E6 | V1, V5 | PASS |
| AC-027 | E5 | V4 | PASS |
| AC-028 | E5 | V4 | PASS |
| AC-029 | E3, E6 | V2, V5 | PASS |
| AC-030 | E3, E6 | V2, V5 | PASS |
| AC-031 | E2–E4, E6 | V2, V3, V5 | PASS |

## Acceptance matrix — expensive-to-reverse constraints

| Obligation | Implementation evidence | Verification evidence | Result |
|---|---|---|---|
| C01 One Workflow Card | E1–E3 | V1, V2 | PASS |
| C02 Agent-only steps | E1, E2 | V1 | PASS |
| C03 Explicit dependencies and bindings | E1, E2 | V1 | PASS |
| C04 Namespaced and explicit outputs | E1, E2 | V1 | PASS |
| C05 Process-local async server lifecycle | E5, E9 | V4, V5 | PASS |
| C06 Runtime failures return terminal runs | E1, E2, E5 | V1, V4 | PASS |
| C07 Closed `LlmRoute` | E1, E2 | V1 | PASS |
| C08 Prompt owns provider/request semantics | E1, E2, E7 | V1, V4 | PASS |
| C09 Stored route is immutable | E3, E5 | V4 | PASS |
| C10 Server rejects Native | E5 | V4, V5 | PASS |
| C11 Capability-scoped SDK/CLI surfaces | E3, E4, E6 | V2–V5 | PASS |
| C12 Two built-in server tools | E5 | V4 | PASS |
| C13 Process-local idempotency | E4, E5 | V4, V5 | PASS |
| C14 Pinned graph and live gateway governance | E3–E5 | V4, V5 | PASS |
| C15 Exact V1 DTO/client interfaces | E1, E4 | V1, V3, V4 | PASS |
| C16 One async engine | E2 | V1, V5 | PASS |
| C17 Shared lower egress policy | E2, E4 | V1, V5 | PASS |
| C18 Tracked preparation and atomic acceptance | E5 | V4 | PASS |
| C19 Cohesive contract-consumer boundary | E1, E2, E7 | V1–V5 | PASS |
| C20 Immutable per-call gateway context and Revision 13/14 wire contract | E4, E7 | V3–V5 | PASS |
| C21 Whole-step retry/deadline composition | E2, E5 | V1, V4 | PASS |
| C22 Skald-only drain and gateway settlement ownership | E2, E4, E5 | V1, V4 | PASS |
| C23 Captured authority without token retention | E5, E9 | V4 | PASS |
| C24 Bounded graph and snapshot preparation | E2, E5 | V1, V4 | PASS |

## Acceptance matrix — non-goals

| Obligation | Implementation evidence | Verification evidence | Result |
|---|---|---|---|
| NG01 No Python/TypeScript/MCP remote lifecycle | E3–E6 | Source review, V5 | PASS |
| NG02 No compatibility or implicit-binding path | E1–E3 | Source review, V1, V2 | PASS |
| NG03 No durable queue or cross-replica recovery | E5, E9 | Source review, V4 | PASS |
| NG04 No persisted WorkflowRun payload | E5 | Source review, V4 | PASS |
| NG05 No dynamic graph/loop/nesting system | E1, E2 | Source review, V1 | PASS |
| NG06 No expression language or branching | E1, E2 | Source review, V1 | PASS |
| NG07 No Workflow-owned gateway policy | E2, E4 | Source review, V3, V4 | PASS |
| NG08 No gateway credential administration | E2, E4, E5 | Source review, V4 | PASS |
| NG09 No Gateway Card or route registry | E1, E2 | Source review, V1 | PASS |
| NG10 No Tool Card/uploaded code/plugin runtime | E1, E2, E5 | Source review, V4 | PASS |
| NG11 No direct Prompt or MCP steps | E1, E2 | Source review, V1 | PASS |
| NG12 No raw provider response wire contract | E1, E7 | Source review, V4 | PASS |
| NG13 No second executor/runtime/poll subsystem | E2–E4 | Source review, V1, V3 | PASS |
| NG14 No new egress crate/resolver/per-run lock/queue/distributed coordination | E2, E4, E5 | Boundary/source review, V5 | PASS |

## Cross-task seam and regression review

- Authored and registered loading use the existing `CardGraphHydrator`; no
  parallel graph store, parser, traversal, validator, or executor remains.
- `Workflow` in `wyrd-client` is a thin facade over resolved Skald state and an
  optional existing client; language SDKs delegate to that owner.
- `WorkflowRunHost` owns server preparation/execution composition, while
  `WorkflowRuns` owns bounded in-memory lifecycle, admission, idempotency,
  retention, and tracked tasks. State locks are not held across IO.
- Built-in Cards, Bifrost, and gateway calls re-enter their existing
  authorization/audit owners under captured token-free authority.
- Bounded query collection is shared by MCP and Workflow tools rather than
  duplicated. Query tools bind the prepared run deadline once; a shorter
  `deadline_ms` wins.
- Revision 14 retains one request variant per wire schema, optional
  `Prompt.provider`, Vertex as GenerateContent with destination `vertex`, and
  local public-gateway Vertex refusal.
- The selected human decisions on Oracle lifecycle, follower release, the
  foreign-tenant harness, attempt 1, cancelled interruption, SDK registration,
  TypeScript packaging, and dev-only wiremock allowlists remain intact.

## Verification limits

- This review intentionally ran no executable verification and relies on the
  recorded candidate-bound evidence plus direct immutable source comparison.
- Deterministic local upstreams are the approved proof class; no live cloud or
  provider credential lane is required.
- The TASK-003 documentation remediation records format, codegen, and diff
  checks but not its requested lint or Python typecheck commands. Direct source
  comparison is dispositive for the documentation-only correction.
- AC-012's served Workflow route assertion is missing as recorded above.
- The task-review chain's post-remediation closure for TASK-001 and TASK-003 is
  supplied by fresh target-bound subreviews in this final review. TASK-004 and
  TASK-005 closure is supplied by the explicit human lead dispositions.

## Completion payload

- Intent: ship one declarative, explicit-binding Skald Workflow runtime for
  local Rust/Python/TypeScript/CLI execution and bounded accepted server runs.
- Externally observable behavior: authored and registered loading, exact graph
  pinning, explicit DAG input/output semantics, Native/WyrdGateway/ExtGateway
  routing, common WorkflowRun snapshots, Rust/HTTP/CLI server lifecycle, and
  Revision 14 provider wire semantics.
- Lasting constraints: one Skald async engine; shared client and Cards owners;
  declarative non-principal Workflow Cards; process-local bounded server jobs;
  captured token-free authority; live per-call authz/audit; shared egress
  policy; no compatibility or durable queue.
- Material decisions: the 24 expensive-to-reverse constraints above and all
  explicit human decisions supplied with this review.
- Deviations: only the known deferred follow-ups and verification limits above.
- Delivery reference: not supplied.

This PASS immediately routes to `$wyrd-complete`.
