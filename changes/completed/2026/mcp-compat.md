# MCP compatibility with reference and agent-framework clients

- Change ID: `SPEC-mcp-compat`, approved revision 3.
- Completed: 2026-10-14.
- Reviewed candidate: `6fbd60f19163ea10689029982b809a158dbc4987` under revision 2.
- Completion target: `06a89286943a64c83eb588562b308c3d35663d66`, including revision-3 remediation and its evidence, integrated through PR #157 at `945a1548c`.
- Delivery reference: PR #157; release reference not supplied.
- Completion authority: explicit human override and official approval on 2026-10-14. No final-change-review PASS or independent post-remediation acceptance verdict is claimed.

## Intent and implemented behavior

Developers can discover and call Wyrd tools through the official TypeScript
MCP SDK and the Python OpenAI Agents SDK, Pydantic AI and Google ADK clients,
using the documented framework settings and model-facing aliases. This is
bounded interoperability repair, not a claim of compatibility with every
framework or unconfigured native compatibility.

Wyrd serves protocol revision 2026-07-28. Permission-dependent tool lists have
zero TTL and private cache scope. Advertised input schemas have an object root
without top-level composition or enum/constant/negation constraints;
`operator_connections.create` nests its provider union under `request`.
The reference client checks the complete administrator catalog and validates a
structured guide result against its advertised output schema.

Each named Python framework converts the caller's complete catalog, invokes the
guide through its own MCP client, excludes writes from a read-only catalog,
and surfaces an explicitly attempted hidden write as a Wyrd-coded framework
tool error. The ADK refusal journey also proves a subsequent successful read
through the same connection owner.

## Lasting constraints and material decisions

Public dotted Wyrd MCP tool identities remain unchanged. Model-facing aliases
are unique and match provider function-name constraints while preserving the
original wire identity, arguments, permissions, results and errors. OpenAI and
Pydantic AI use native naming facilities. ADK uses the approved small local
copied-tool name override in framework journeys and documented examples; it
does not own transport, sessions, schema conversion, authorization, retries or
durable state. No public Wyrd SDK adapter or alias registry is introduced.

Server-owned authentication, authorization, tenant isolation, audit and secret
handling are unchanged. Tool discovery grants no permission. The endpoint
remains stateless without replica affinity. Framework packages stay in an
optional test-only Python dependency group, outside runtime dependencies and
the fast default development lane. No older direct protocol, resources,
prompts, sampling, workflow execution, server aliases or embedded host
credentials are added by this change.

Revision 1 carried the compatibility requirement into its own change.
Approved revision 2 selected the TypeScript reference client and three Python
frameworks, adding complete negative-path and model-name proof. Approved
revision 3 explicitly permits bounded local model-name aliasing where native
facilities are absent, resolving the earlier ADK adapter-policy conflict
without changing MCP wire names or server ownership. Completion changes none
of those decisions; the owner override accepts the recorded remediation and
remaining final-review limits.

## Acceptance evidence and limits

| Obligation | Evidence |
|---|---|
| REQ-003 protocol, catalog cache metadata and schemas | Prior independent reference/framework executions and exact Rust catalog schema test passed; recorded protocol/cache sensitivity evidence supports the contract. |
| Complete TypeScript catalog and structured guide validation | Remediation records a passing exact reference journey and an omission experiment proving full-name equality detects a missing tool. |
| Actual framework conversion, invocation and hidden-write refusal | Six framework journeys passed in recorded remediation evidence; ADK's actual Wyrd RBAC-code assertion and subsequent guide read passed, replacing a local undeclared-name rejection. |
| AC-004 valid unique aliases preserving wire identity | Recorded full-catalog assertions and alias-collision sensitivity proof passed; guide round trips and documented configuration preserve original tool identity. |
| INV-001/002/003/004 and isolated dependencies | Independent source review found no authorization, tenancy, persistence or availability defect; implementation retains existing owners and test-only framework dependencies. |
| AC-002 schema-root proof has one owner | Rust catalog test owns root validation; client journeys validate client-specific interoperability without duplicate handwritten root checks. |
| AC-003 owning verification | Task evidence records passing framework/reference/catalog journeys and format/lint/typecheck/codegen/docs checks; bounded remediation records six framework tests plus reference/ADK selectors and changed-language/docs checks passing. |

The historical revision-2 review required revision and four corrections.
Revision 3 and the implementation record address catalog completeness, actual
ADK server refusal, alias policy/proof and panic documentation. These are
recorded remediation results, not an invented post-remediation review PASS.
ADK modern-protocol negotiation remains experimental; its alias recipe depends
on the framework retaining the original MCP tool identity, guarded by the
guide round trip. Optional framework tests are excluded from default Python
typechecking. Broader final integrated verification was not established by
closeout, and no live model or release claim is made.

## Current authority, owners and tests

- [Wyrd design](../../../architecture/wyrd-design.md), [security posture](../../../architecture/wyrd-security-posture.md), and [MCP reference/configuration](../../../docs/src/content/docs/reference/mcp-tools.md).
- [MCP catalog and dispatch](../../../crates/wyrd/wyrd-server/src/mcp/mod.rs), [gateway descriptors](../../../crates/wyrd/wyrd-server/src/mcp/gateway.rs), and [guide owner](../../../crates/wyrd/wyrd-server/src/mcp/guide.rs).
- [TypeScript reference journey](../../../sdks/wyrd-sdk-ts/wyrd/tests/integration/mcp-reference-client.test.ts), [Python framework journeys](../../../sdks/wyrd-sdk-python/tests/integration/mcp_frameworks/test_agent_frameworks.py), and [verification lanes](../../../mise.toml).
