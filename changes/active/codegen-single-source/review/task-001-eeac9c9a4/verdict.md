# TASK-001 review verdict

**Verdict: FIX_REQUIRED**

**Finding:** FIND-TASK-001-1.

**Remediation:** [TASK-001-R1-preserve-typescript-response-shapes.md](TASK-001-R1-preserve-typescript-response-shapes.md), routed to `$wyrd-implement` under the existing approved specification.

## Immutable subject

- Repository: `/home/thorrester/Documents/GitHub/wyrd-worktrees/split/codegen-single-source`.
- Base: `5bcd9e4872f1376f1722922fa63b67384243be04`, parent of first implementation commit `e4210b940`.
- Candidate: `eeac9c9a45b599b1727086ba30582d3f02b3230a`.
- Scope: complete cumulative seven-commit base-to-candidate diff, not only the evidence commit.
- Authority: [approved spec revision 1](../../spec.md) and [original TASK-001](../../tasks/TASK-001-consolidate-sdk-types-and-codegen.md), under repository rules and applicable architecture.
- Initial tracked tree was clean. HEAD stayed pinned; tracked implementation, spec, and task files were not changed. Only this new review directory was written.
- No previous task-review findings or remediation packet were supplied.

## Outcome

The schema retirement, native Python envelope, authored stub consolidation,
MCP derivation, documentation cleanup, and pipeline closure satisfy their
reviewed obligations. All three language lint lanes passed independently.
The existing Python typecheck and TypeScript compiler check also passed.

The candidate nevertheless weakens TypeScript response contracts. An
always-emitted nullable Rust field becomes optional in TypeScript, and a field
skipped when absent acquires an impossible null value. Existing valid callers
then fail compilation, despite unchanged runtime JSON. The existing green
fixtures do not establish this precision; one was widened to accept the error.

The implementation's disclosed approximation does not supersede REQ-007 or
INV-003. This is one shared producer/lowering defect, with a bounded correction
within the approved owners and existing dependencies. No specification
revision or additional behavioral scenario matrix is required.

## Independent review results

| Role | Report | Discovery result | Reconciled result |
| --- | --- | --- | --- |
| Behavior | [task-review-behavior.md](task-review-behavior.md) | FAIL; BEHAVIOR-001 | REVISED into FIND-TASK-001-1 |
| Invariants | [task-review-invariants.md](task-review-invariants.md) | FAIL; INVREV-001 | REVISED into FIND-TASK-001-1 |
| Repository standards | [standards-review.md](standards-review.md) | PASS; no material proposals | No separate retained finding |
| Maintainer | [maintainer-review.md](maintainer-review.md) | PASS; no material proposals | No separate retained finding |
| System resilience | [system-review.md](system-review.md) | PASS; no material proposals | No separate retained finding |
| SDK contract domain | [domain-review-sdk-contracts.md](domain-review-sdk-contracts.md) | FAIL; SDK-001 | REVISED into FIND-TASK-001-1 |
| Fresh structured Ponytail validation | [findings-validation.md](findings-validation.md) | Independent source validation and compiler reproduction | One final shared-cause finding |

All required reports are complete. The orchestrator compared claims and checked
report completeness; the fresh validator, rather than the orchestrator,
independently established the finding and selected its correction.

## Follow-up decision

No `followup-rev` was needed. The discovery proposals identify the same
reachable contract boundary. The scoped PASS reports explicitly do not certify
lowering precision, so there is no conflicting acceptance claim to resolve.
The validator assessed the proposed correction's shared input/output cases and
dependency limitations directly. See [claims-comparison.md](claims-comparison.md).

## Validated finding ledger

### FIND-TASK-001-1 — REVISED — INCORRECT / REGRESSION

- **Discovery IDs:** BEHAVIOR-001, INVREV-001, SDK-001.
- **Obligations:** REQ-007, INV-003, INV-005, AC-006.
- **Cause/location:** `scripts/gen_ts_types.py:179–190,205–225` and the
  incomplete serialization information in `gen_schemas.rs::contracts`.
  Default deserialization schema metadata plus a `default` heuristic cannot
  accurately describe output presence and nullability.
- **Evidence:** Rust `WorkflowRun`/`WorkflowStepResult` serialize unskipped
  Option keys, including timestamps/text, as value or null. The native bridge
  and public wrapper pass this response through unchanged. Generated types
  introduce optionality. Conversely, Bifrost's skipped fingerprint/compaction
  fields admit null in the generated output. Registry, principal, credential,
  judgment, and shared layout siblings use the same projection boundary.
- **Consequence:** `const started: string | null = run.started_at` now fails
  with unexpected `undefined`; skipped non-null response fields accept null.
  Independent compiler probes reproduced TS2322 and TS2578.
- **Correction:** restore exact serialization shape at the Rust metadata and
  existing lowering owners. Reuse existing annotations for response-owned
  fields and existing `schema_with`/schema-extension support where shared
  input/output contracts need distinct projections. Preserve accepted inputs,
  opaque JSON null values, and already-correct skipped collections. Regenerate
  existing output and strengthen existing compile fixtures. Do not introduce
  field-name overrides, copied interfaces, a new dependency/framework, or
  downstream guards.
- **Closure:** the linked remediation gives the diagnosis, affected owners,
  selected correction, preserved behavior, and exact focused checks. Full
  source traces and the restriction on schemars 0.8 mechanisms are in
  [findings-validation.md](findings-validation.md).

There are no other independently retained findings. Generated namespace suffixes
were inspected against existing exports and aliases; no separate public-name
regression was established. No optional preference is bundled into remediation.

## Reconciled acceptance matrix

Detailed source/verification evidence is retained in both implementation review
matrices and the validation ledger. PASS rows distinguish source inspection and
recorded implementation evidence from newly executed checks.

| Obligation | Implementation/verification evidence | Result |
| --- | --- | --- |
| REQ-001 | Schema and fixture outputs/snapshot-only readers retired; generated UI examples and import retained; codegen evidence | PASS |
| REQ-002 | Five MCP descriptors derive actual request DTOs using the existing schema helper; metadata/permissions/dispatch unchanged; recorded focused catalog result | PASS |
| REQ-003 | Docs indexes retained, raw appendices/dead renderers removed, diagram comparison consumes transient Rust definitions; recorded docs check | PASS |
| REQ-004 | Existing native envelope retains complete Card and exact identity; empty registry aliases and validated hydrated aliases; updated real-server journey | PASS |
| REQ-005 | Callable annotation conversion, decorator, registry, invocation and delegate paths retained; recorded unit coverage | PASS |
| REQ-006 | One authored native stub tree, inline pure-Python definitions and public reexports; no generated Card/template copy/root replacement; independent Python checks | PASS |
| REQ-007 | Rust ownership and generation are present, but response precision is incorrect; independently reproduced FIND-TASK-001-1 | FAIL |
| REQ-008 | Public root/subpath declarations, native generation and compiler-emitted wrapper declarations remain | PASS |
| REQ-009 | Setup/codegen/docs/CI/rules and useful stub validation close deleted references; surviving output generation retained | PASS |
| REQ-010 | All three lint lanes and Python typecheck independently pass; direct TS compiler passes; one planned behavioral scenario retained | PASS |
| INV-001 | Existing server/client ownership, wire/runtime behavior, security, tenancy, errors and durable formats retained | PASS |
| INV-002 | Approved Python envelope break only; no compatibility model/getter or new runtime wire behavior | PASS |
| INV-003 | Single declaration ownership achieved, but surviving TypeScript response precision weakened | FAIL |
| INV-004 | Existing dependencies/features/owners reused; no new framework, transport, schema service or extractor | PASS |
| INV-005 | Snapshot-only retirement is authorized; changed fingerprint fixture accepts faulty widening | FAIL |
| AC-001 | Artifact retirement and UI example preservation established | PASS |
| AC-002 | Direct Rust MCP schemas and catalog semantics established | PASS |
| AC-003 | Docs/index/diagram closure established | PASS |
| AC-004 | Native retrieval/identity/content/alias assertions and hydrated validation established | PASS |
| AC-005 | Python canonical declarations/exports/tool behavior established | PASS |
| AC-006 | Declaration ownership resolves, but accurate Rust-to-TS response projection does not | FAIL |
| AC-007 | Independently rerun Rust, Python and TypeScript lints pass with retained scope | PASS |
| AC-008 | Pipeline/packaging references/rules and surviving generation closure established | PASS |
| Non-goals | No new capability, endpoint, Bifrost engine behavior, Card kind, migration, deployment or unrelated UI behavior | PASS |

## Verification limits and prior-finding closure

[verification.md](verification.md) records newly executed successful `lints`,
`py:lints`, `ts:lints`, and `py:typecheck`, plus the direct TypeScript compiler
body against existing built native declarations. The latter is not a fresh
native build or a rerun of all `ts:typecheck` prerequisites.

Earlier runtime/unit/integration, generation/docs, N-API, format and journey
results remain implementation evidence assessed alongside source; they are not
represented as freshly executed here. Discovery/validation also used read-only
compiler counterexamples and transient metadata inspection. No source
regeneration, fresh journey suite, or broad gate ran during this review.
The original task defers the integrated aggregate to change review.

Prior-finding closure: not applicable; this is the first review. A subsequent
review must reassess this complete base-to-remediated-candidate range and retain
FIND-TASK-001-1's identity.

**Final verdict: FIX_REQUIRED.** Lints are green; public response typing remains
incorrect until the bounded remediation and its focused proof pass.
