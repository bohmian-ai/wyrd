# Repository standards review — TASK-003 r3

## Review findings

### Critical

None.

### Important

#### STD-R3-001 — an observed native `401` can be cancelled before renewal

- Classification: **INCORRECT / MISSING EVIDENCE**
- Governing authority: the approved R1 native-`401` wording correction; the
  R2 remediation acceptance for `FIND-TASK-003-2`; `AGENTS.md` §§6, 11, 12,
  and 16; and the Rust async/cancellation guidance.
- Locations:
  - `crates/shared/wyrd-client/src/transport/http.rs:402-409`
  - `crates/shared/wyrd-client/src/workflow/gateway.rs:96-116`
  - `crates/shared/wyrd-client/tests/workflow_transport.rs:1039-1062`
- Issue: `post_native` observes the response status at line 402 but still
  awaits the complete response body at line 403 before it enters the `401`
  renewal branch. The outer public caller races that whole future against the
  call deadline and cancellation. A server can therefore send `401` headers
  and keep an incomplete body connection open; timeout or Workflow
  cancellation then drops `post_native` before `force_refresh` runs. The
  refused cached bearer remains available to the next call. This contradicts
  the method's own statement that renewal follows every observed `401` and the
  approved remediation requirement that an unreadable native `401` renew
  without replay.
- Evidence assessment: the recorded R2 proof uses a short body followed by
  connection close. That makes `response.bytes()` finish with an error and
  proves renewal only after a completed body-read outcome. It does not cover a
  body that remains pending until the public caller cancels or times out, so
  the evidence does not establish the stated invariant for the current source.
- Impact: a rejected renewable credential can be reused after a reachable
  native response shape, contrary to the approved authentication behavior.
  The model request is not replayed, but reactive renewal is skipped.
- Smallest correction: once the status is known to be `401`, renew through the
  existing `AuthMiddleware::force_refresh` owner before awaiting the body. Keep
  the single model POST, make renewal failure authoritative, then read and
  return the original refusal body or its existing body-read error. Extend the
  existing focused raw-loopback case so it sends `401` headers and holds the
  body pending, establishes renewal before releasing/ending the response, and
  proves one model POST with no resend. Add no setting, retry, response marker,
  transport, harness, or other mechanism.

#### STD-R3-002 — the authentication owner's renewal documentation is false for the new native caller

- Classification: **VIOLATION**
- Governing authority: `AGENTS.md` §16 and
  `architecture/agent-rules.md`, which make accurate workflow, invariant, side
  effect, cancellation, and retry rustdoc a hard acceptance requirement.
- Location: `crates/shared/wyrd-client/src/auth.rs:503-507`.
- Issue: `AuthMiddleware::force_refresh` says the HTTP/gRPC reactive-`401`
  path calls it and then retries once. TASK-003 adds a conforming HTTP native
  caller that invokes this method specifically to prepare the *next* call and
  must never retry the refused model request. The owner documentation now
  misstates the behavior of one of its production consumers.
- Impact: a maintainer following the authentication owner's contract can
  incorrectly add or preserve replay on a non-replay-safe model call, or
  misunderstand why native renewal has no current-call success path.
- Smallest correction: qualify the existing rustdoc: replay-safe HTTP/gRPC
  helpers may retry once, while native model POSTs refresh only for later calls
  and return the original refusal. This is documentation only; add no retry or
  new policy mechanism.

#### STD-R3-003 — the new consuming conversion omits its client-context side effect

- Classification: **VIOLATION**
- Governing authority: `AGENTS.md` §16,
  `architecture/agent-rules.md`, and the R1 client-context remediation.
- Locations:
  - `crates/shared/wyrd-client/src/workflow/mod.rs:34-41`
  - `crates/shared/wyrd-client/src/workflow/mod.rs:170-174`
- Issue: the shared `Workflow` explicitly retains the client that loaded its
  registered Cards so public gateway calls keep endpoint and credential
  context. Its new public `into_skald` method is documented only as “Take the
  hydrated Skald Workflow,” although consuming the wrapper necessarily drops
  that retained client and its automatic local dependency composition. The R1
  Python defect demonstrates that this is a material, non-obvious side effect,
  not an implementation detail.
- Impact: a Rust/SDK maintainer can use the public conversion while expecting
  registered-local gateway execution to preserve its loading client, recreating
  the context-loss bug already remediated at the Python boundary.
- Smallest correction: document that `into_skald` intentionally discards the
  retained client and that callers must supply explicit execution dependencies;
  direct callers that need automatic shared configuration should keep and run
  the shared `Workflow`. No wrapper, compatibility path, check, or new API is
  required.

### Suggestions

None.

## Open questions

None. The minimal Revision 12 `WyrdGatewayCall.model` amendment and the R1
same-spec native-`401` wording correction are explicit human approvals and were
applied as authority.

## Subject and review boundary

- Repository: `/home/thorrester/Documents/GitHub/wyrd`
- Base: `58d07d7260df1f022a721e720a28ea48e5096e35`
- Candidate: `0732ed92fc2907cb806ac918b09abbf6d44ff04f`
- Approved specification: `SPEC-skald-workflow-runtime`, Revision 12
- Original task: `TASK-003-remote-client-and-public-gateway.md`
- Remediation history: TASK-003 r1 and r2, including both remediation tasks,
  verdicts, validation ledgers, and implementation evidence
- Review mode: source, cumulative diff, repository authority, and recorded
  implementer evidence only. No build, compile, test, Cargo, mise, pnpm,
  pytest, formatter, linter, test-listing, or other verification command was
  run. Other TASK-003 r3 reports were not read.

## Authority coverage

| Changed surface | Applicable authority inspected | Result |
|---|---|---|
| Shared Workflow facade, local dependency composition, remote handle, gateway caller, and HTTP transport | `AGENTS.md`; `architecture/agent-rules.md`; `architecture/wyrd-design.md`; `architecture/wyrd-doctrine.mdx`; architecture constraints/patterns; Rust core, errors, testing, and implementation-execution references; Revision 12 and TASK-003/r1/r2 | Complete; findings `STD-R3-001` through `STD-R3-003` |
| Shared secret reader, gateway credential consumer, and server key consumer | repository ownership rules; `architecture/wyrd-security-posture.md`; Rust secret guidance | Complete; PASS |
| Skald route planning and `WyrdGatewayCall.model` | Skald ownership rules; approved 2026-10-03 minimal amendment; struct-centered and import rules | Complete; PASS |
| `wyrd-spec` fallback-header contract | foundation boundaries; doctrine; error and agent-harness references | Complete; PASS |
| Public OpenAI/Anthropic/Gemini ingress and served OpenAPI | server/contract rules; security posture; errors, agent-harness, and testing references | Complete; PASS |
| Python wrapper and public retained-context journey | PyO3 boundary and Python API/stub references; SDK ownership rules | Complete; PASS |
| TypeScript and Rust SDK projections; later CLI seam | shared-client ownership rules; Revision 12 task partition; TASK-005 consumer assignment | Complete; PASS for this task boundary |
| Manifests and lockfile | dependency-cost, feature, client-tier, and crate-boundary rules | Complete; PASS |
| Tests and implementation evidence | test taxonomy, runtime ownership, verification-scope rules, original/r1/r2 recorded evidence | Complete; one contradicted/missing case in `STD-R3-001` |

## Applicable rule results

| Rule | Result | Source-backed assessment |
|---|---|---|
| `wyrd-client` remains the sole SDK-facing implementation and language layers stay thin | PASS | Rust owns loading, execution composition, remote lifecycle, and public gateway transport; Python and TypeScript delegate to the shared `Workflow`; Rust SDK re-exports it |
| Skald owns reusable execution and provider routing; the server owns authentication, tenancy, policy, audit, and governed dispatch | PASS | Route state remains in `skald-workflow`; public calls enter the existing authenticated server ingress and gateway invocation owner |
| `wyrd-spec` remains synchronous, IO-free, async-free, and PyO3-free | PASS | Only the typed fallback codec/constants and approved model contract were added |
| Stateful behavior has cohesive concrete owners and justified traits | PASS | `Workflow`, `Workflows`, `PublicWyrdGatewayCaller`, `SelectedRoutes`, and the two real `WyrdGatewayCaller` implementations follow the required shape |
| Async boundaries, cancellation, and authentication renewal match their stated contracts | **FAIL** | `STD-R3-001` |
| New/materially affected Rust documentation states workflow role, invariants, side effects, and retry behavior | **FAIL** | `STD-R3-002`, `STD-R3-003` |
| Changed declaration types are imported at module top and used by bare/role-specific names | PASS | The R2 import remediation covers the cumulative changed declarations; no checker or allowlist was added |
| Secrets remain redacted, bounded, and resolved only at the owning execution boundary | PASS | Public shared reader returns `SecretString`; file metadata is checked on the open handle; selected local bindings alone resolve at run time |
| Public ingress authenticates before fallback interpretation; malformed/repeated/oversized values fail before dispatch and do not reach providers | PASS | Header decoding occurs only inside authenticated handler branches and feeds the typed fallback field |
| Portable refusal projection uses catalog/category text and does not retain raw upstream messages or arbitrary details | PASS | Recognized Wyrd codes use catalog metadata; uncoded statuses use existing provider categories; only the approved OpenAI `param` field survives |
| Public proof is placed in the owning runtime and generated artifacts are not hand edited | PASS, except the case in `STD-R3-001` | HTTP/server/Python journeys and served OpenAPI proof are in established homes; no generated schema/stub was edited |
| Dependency and feature changes stay in the narrow owners | PASS | No wildcard, profile block, client-tier SQL/cloud/DataFusion dependency, Python activation, or unapproved third-party package was added |
| No unsupported mechanism, check, file, setting, or option is introduced or required | PASS | The fallback header and Workflow config are approved contracts; the loopback HTTP fixture is an established ordinary test technique; this review requires no new checker, option, setting, allowlist, or harness |

## Verification notes

The original task and r1/r2 remediation records report the focused Workflow
transport selectors, selected local dependency proof, fallback contract proof,
gateway/server/Postgres and served OpenAPI proof, the public Python
retained-context journey, shared/Rust-SDK/TypeScript families, formatting,
lints, code generation, and client/SDK/PyO3 boundary checks. The R2 record also
reports the cut-off-body `401` and post-dispatch cancellation cases.

This review did not rerun any recorded command. Source supports the recorded
evidence for a body read that succeeds or terminates with an error and for
post-dispatch cancellation. Source contradicts the broader claim that every
observed `401` renews: the body-pending-until-cancellation case is neither
implemented nor recorded. The implementer must correct and rerun the existing
focused selector; review must not infer a pass from the connection-close case.

## Overall result

**FAIL**

The candidate otherwise conforms to the applicable ownership, contract,
security, language-boundary, dependency, test-placement, import, and
no-extra-mechanism rules. The remaining renewal-ordering defect and two hard
documentation-contract violations block repository-standards acceptance.
