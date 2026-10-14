# Owner-directed r3 transition and remediation handoff

The owner directed: revise the spec and provide a full remediation packet with
decision-complete, simple recommendations for every finding. Revision 3
records that direction by allowing bounded framework-local model aliases
while preserving the original MCP wire names and all durable/auth/transport
owners. It leaves the provider-name regex and every real-server proof
obligation intact. No further compatibility choice is delegated to the
implementer.

## Current authority

- Specification: `changes/active/mcp-compat/spec.md`, r3, approved under the
  owner's explicit direction to revise the reviewed constraint.
- Active task: `changes/active/mcp-compat/tasks/T01-framework-journeys.md`,
  reconciled to r3 with its old naming stop condition replaced.
- Ready remediation task:
  `changes/active/mcp-compat/review/T01-6fbd60f19/T01-R1-close-framework-findings.md`.
- Historical immutable subject/verdict: `84c2af51b..6fbd60f19`, reviewed under
  r2. Reports and FIND IDs remain unchanged; this transition is subsequent
  authority, not a rewritten verdict.

## Finding disposition

| Finding | Selected recommendation | Current state |
|---|---|---|
| FIND-T01-1 | Compare SDK list names with a separate unfiltered typed protocol request on the same administrator connection | Ready to implement |
| FIND-T01-2 | Invoke a real native ADK tool with viewer authority, require Wyrd RBAC code, then read guide through the same session manager | Ready to implement |
| FIND-T01-3 | Keep the existing small local ADK alias; r3 permits it. Prove valid unique model names/original-wire guide routing and document the required configuration | Policy decision resolved; implementation/proof/documentation closure still required |
| FIND-T01-4 | Add the four concise panic rustdoc contracts with runtime bodies unchanged | Ready to implement, batched with the other corrections |

The complete diagnoses, selected owners, excluded alternatives, acceptance
criteria, execution scenarios, and exact focused commands are in the one
remediation task. No additional planning cycle or replacement machinery is
needed. Send that task directly to `$wyrd-implement`; subsequent independent
task review audits the complete cumulative candidate under r3.

Only planning/specification artifacts were changed for this handoff. No test
pass or implementation closure is claimed from writing this packet.
