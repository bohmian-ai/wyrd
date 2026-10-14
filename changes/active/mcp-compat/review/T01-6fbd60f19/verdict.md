# T01 task review verdict

**SPEC_REVISION_REQUIRED**

The installed native Google ADK toolset forwards dotted model declarations. The candidate makes the provider-name assertion pass through a custom copied-tool renaming subclass, while approved r2 requires adapter-free interoperability and the original task explicitly stops on this naming incompatibility. Wire tool names remain unchanged; the finding concerns the custom adaptation needed to satisfy the model-facing contract. Review cannot authorize that new compatibility policy. See FIND-T01-3 in the independently validated ledger.

## Immutable subject

- Repository: `/home/thorrester/Documents/GitHub/wyrd-worktrees/split/mcp-compat`.
- Base: `84c2af51b42c62edf2c596b5b600cadf9094b61a`, the approved r2/task commit immediately before implementation.
- Candidate: `6fbd60f19163ea10689029982b809a158dbc4987`.
- Approved spec: `changes/active/mcp-compat/spec.md`, revision 2.
- Original task: `changes/active/mcp-compat/tasks/T01-framework-journeys.md` at the base; the candidate's appended implementation record is evidence, not revised authority.
- Reviewed range: the complete base-to-candidate diff, including all implementation/evidence commits. The latest commit alone was not the subject.
- Source stayed immutable. Only this review directory was added; no source edits, commits, merge, push, or deployment occurred.

## Reconciled acceptance matrix

The [independent validation](findings-validation.md) contains the full source-level matrix and producer-to-consumer traces. This table records the orchestrator's reconciliation from that validated evidence.

| Obligation | Implementation and verification evidence | Result |
|---|---|---|
| Protocol 2026-07-28; zero-TTL private catalog; stateless endpoint | Existing handler/session owners retained; fresh reference journey passes; recorded TTL sensitivity experiment | PASS |
| Full catalog schema roots and SDK output acceptance, owned once by Rust | Gateway tools remain included in Rust schema check; exact catalog test and reference guide validation pass; no handwritten client schema-root loop | PASS |
| TS reference client proves full administrator catalog | Test checks effect-hint categories without required server-derived name equality | FAIL: FIND-T01-1 |
| OpenAI converts all tools with strict conversion enabled and invokes guide under provider-valid names | Native SDK qualification/strict configuration, count equality and converted tool call; fresh framework lane passes | PASS |
| Pydantic AI converts all tools and invokes guide under provider-valid names | Native renamed toolset, complete name comparison and scripted model loop; fresh lane passes | PASS |
| ADK converts/invokes every advertised tool through permitted integration; model names satisfy regex | Happy path passes only with custom renaming subclass; installed native prefixing preserves dots | FAIL: FIND-T01-3 |
| Every reader sees no write tool | All three real framework reader catalogs verified; shared gateway descriptor filter matches existing operation permissions | PASS |
| Hidden writes surface actual Wyrd refusal through each framework | OpenAI and Pydantic invoke Wyrd and assert RBAC code; ADK stops at local undeclared-name lookup | FAIL: FIND-T01-2 |
| INV-001 tenant isolation and INV-002 existing authentication/authorization/audit/secrets | Verified caller and existing operation owners remain authoritative; hidden names still resolve for authorization; independent security audit found no bypass | PASS, with ADK proof gap recorded separately |
| INV-003 no replica affinity | No transport/session production changes; catalog remains request-local | PASS |
| AC-001 real server and each public framework path, offline model execution | Real managed server and public native clients used; ADK adapted positive/local negative path does not fulfill complete contract | FAIL: FIND-T01-2/3 |
| AC-002 no repeated hand-written schema-root checks | Removed TS loops; Rust remains owner | PASS |
| AC-003 dedicated lane and owning checks | Fresh framework/reference/catalog tests pass; implementation record reports formatting/lints/typechecks/codegen/docs/MCP journey passes | PASS with verification limits below |
| Framework packages isolated from runtime/default development/fast lane | Separate group and explicit-directory lane; new lane wired into aggregates | PASS |
| TS trimmed/renamed and JS framework dependency removed | Complete source, manifest and lock diff reviewed | PASS |
| Shared server root fix and documentation parity | Gateway visibility fixed at existing catalog owner; dispatch permissions unchanged; public visibility docs match | PASS except unauthorized ADK recipe below |
| No public wire rename, older protocol, resources/prompts/sampling, new authorization path, or host credentials | None introduced in cumulative production diff | PASS |
| No Wyrd-specific MCP adapter | Custom ADK identity rewrite and documented recipe are necessary to the claimed result | FAIL: FIND-T01-3 |
| Required rustdoc for new/materially changed Rust items | Four changed items omit their panic contracts | FAIL: FIND-T01-4 |

## Independently validated findings

| ID | Classification | Consequence and required disposition |
|---|---|---|
| FIND-T01-1 | MISSING, CONFIRMED | TS full-catalog proof accepts a subset. Compare complete sorted names against an independent server-derived administrator list using native request facilities; preserve SDK output validation and keep schema-root checks in Rust. |
| FIND-T01-2 | MISSING, REVISED | ADK's reader test never reaches Wyrd's denied-call consumer. Reuse public ADK `McpTool` and a viewer-authenticated `MCPSessionManager` with an administrator-discovered descriptor; assert the Wyrd RBAC code and continued native read use. Preserve reader discovery filtering and existing server authorization. |
| FIND-T01-3 | VIOLATION / DRIFT, CONFIRMED | Custom ADK model-name rewriting conflicts with r2. Route to `wyrd-spec` for an explicit decision on ADK naming/compatibility or allowed adaptation. Do not strengthen the adapter, rename wire tools, remove ADK proof, or weaken the regex under unchanged r2. |
| FIND-T01-4 | VIOLATION, CONFIRMED | Three new gateway descriptor projections and the materially changed catalog unit test omit AGENTS §16 panic contracts. Add concise rustdoc for the existing invariant/assertion panics without runtime changes; batch with substantive corrections. |

Exact locations, discovery-source IDs, source traces, correction boundaries, and focused closure commands are preserved in [findings-validation.md](findings-validation.md). Only independently confirmed/revised findings are retained. No extra finding is assigned for OpenAI non-strict fallback, native framework aliases, the optional dependency directory, or non-blocking catalog wording.

## Independent review results and follow-up

| Role | Report | Discovery result |
|---|---|---|
| Behavior | [task-review-behavior.md](task-review-behavior.md) | FAIL |
| Invariants | [task-review-invariants.md](task-review-invariants.md) | FAIL |
| Repository standards | [standards-review.md](standards-review.md) | FAIL |
| Maintainer | [maintainer-review.md](maintainer-review.md) | FAIL |
| System resilience | [system-review.md](system-review.md) | FAIL |
| Security/RBAC domain | [domain-review-security.md](domain-review-security.md) | FAIL |
| Fresh structured Ponytail validation | [findings-validation.md](findings-validation.md) | Four retained findings; SPEC_REVISION_REQUIRED |

No focused discovery follow-up was needed: reports agreed on reachable paths and existing owners, with no material conflicting source evidence or unreviewed path. The fresh validator independently resolved the adapter-authority question and consolidated duplicates. Rationale and claim mapping are in [claim-comparison.md](claim-comparison.md).

## Prior feedback closure and verification limits

There was no prior formal verdict, FIND ledger or remediation task in the active packet. Candidate evidence records stop-hook feedback on ADK protocol negotiation and provider names. Modern protocol negotiation now works under ADK's experimental native setting; the fresh lane confirms it. Provider names pass only after custom ADK adaptation, so that reported closure does not satisfy r2. The recorded mTLS-stall diagnosis explains the test-scoped native environment setting; no extra timeout/retry mechanism was added.

Fresh review verification, detailed in [verification.md](verification.md): six Python framework tests passed in 5.11s; the managed-Postgres TypeScript reference test passed; the exact Rust catalog schema test passed; tracked source/diff checks stayed clean. These executions prove the current assertions, not omitted acceptance paths. ADK emitted upstream experimental/deprecation warnings; there were no live model calls or model credentials.

Formatting, lints, codegen, typechecks, docs and the broader MCP journey results are available as candidate task records, not fresh reruns of every gate. TypeScript used existing native artifacts; Python setup rebuilt the testing extension from the candidate. No fresh Python 3.14 or full aggregate gate was run. No production authorization, tenancy, durability or availability defect was independently found; the material risks are overstated catalog/ADK compatibility proof and the unresolved naming contract.

## Routing

No implementation remediation task is issued under this verdict: closing FIND-T01-3 needs approved compatibility authority first. Route that concrete conflict through `wyrd-spec`; preserve the other three validated bounded corrections for the resulting implementation task. The approved source remains unchanged and T01 has not passed its completion gate.
