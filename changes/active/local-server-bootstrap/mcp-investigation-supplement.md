# MCP investigation supplement to TASK-003

**Status:** proposed contract and implementation plan. TASK-003 delivered host
connection to the existing `/mcp` catalog; this supplement expands what an
agent can discover and do after connecting. The approved revision 5 spec
explicitly says TASK-003 adds no server tool contract, so the contract below
requires an approved spec revision before it becomes an executable task packet.
It does not reopen the completed proxy or host-install work.

## Outcome

A developer connects Codex, Claude Code, or another supported MCP host through
`wyrd mcp install` and can ask open-ended questions about a Service, component,
request, verification run, Eval outcome, or Bifrost table. The host can discover
the right tools, inspect exact Card relationships, query authorized evidence,
and explain an answer with the Card UIDs, result IDs, table names, and query
results it actually used. Wyrd supplies capabilities and guidance; the agent
chooses the investigation path. There is no fixed report entry point.

## Contract decisions for the spec revision

The spec revision should add `REQ-017` for Card discovery and exact-reference
reads, `REQ-018` for navigation through existing server-derived relationships,
`REQ-019` for MCP evidence workflows through existing Bifrost query authority,
and `REQ-020` for the in-MCP guide and complete agent documentation. Add
`AC-009` for automated MCP investigation journeys and `AC-010` for Codex and
Claude Code usability evidence. Clarify REQ-013's “no server tool contract”
sentence as a boundary of the completed TASK-003 proxy, not of this newly
approved supplement; keep its one-catalog and shared-auth decisions intact.

1. **Card discovery.** Add read-only `cards.list` with the existing
   `ListCardsRequest` filters and `ListCardsResponse` cursor. Add read-only
   `cards.get_by_ref` accepting `{card_ref: CardRef}` and returning
   `GetCardResponse`; the reference includes its required version.
   Preserve `cards.get {kind, card_uid}` unchanged. A name-only question uses
   `cards.list` (for example `kind: Service, name: customer-support`), then
   reads the selected exact version. Do not silently choose a version or add
   fuzzy search. Both calls use the existing `cards:read` decision, tenant
   binding, audit, validation, and stable errors from the Card read/list paths.
2. **Relationship navigation.** The full Card returned by `cards.get` and
   `cards.get_by_ref` already carries server-derived `outbound_refs` and
   `inbound_refs`. Describe these fields in both tool contracts and the guide.
   The agent follows exact references with repeated reads. Do not add a graph
   store, recursive traversal endpoint, or duplicate `cards.relationships`
   tool. Service `components` and declared tables provide its starting scope;
   a principal and a Card remain distinct identities.
3. **Evidence retrieval.** Keep `verification.get_binding`,
   `verification.get_run`, and `bifrost.query` as the durable path. Run status
   supplies `result_id`; the agent queries `vala.verification.results` and,
   where applicable, `vala.drift.result_features` or
   `vala.eval.result_items`. For Card-led investigation, query by the exact
   subject or owner Card UID and inspect the matching result IDs. For
   request-led investigation, use the relevant described table's correlation
   columns. Do not add `verification.get_result` or a second result store:
   Wyrd design assigns verification history to Bifrost SQL. Direct
   `verification.execute` returns its one-off judgment; explain its different
   lifecycle. Existing query admission, table and payload permissions, result
   ceilings, cancellation, and terminal handling remain authoritative.
4. **Agent guide through MCP.** Add an always-advertised read-only `wyrd.guide`
   tool with a closed `topic` enum: `overview`, `cards`, `verification`, and
   `bifrost_sql`. Put a short instruction in MCP server discovery directing
   agents to `tools/list` for the current authorized catalog and to
   `wyrd.guide` for workflows. The closed input is `{topic}`. Its typed output
   carries `{topic, steps, examples}`; each example names the relevant tool,
   purpose, and argument shape. Keep these contract types in `wyrd-spec` and
   project them through the MCP adapter. Guide output is bounded structured
   content, available to any authenticated caller, contains no tenant data or
   secrets, and never claims permissions the caller does not have. No separate MCP
   resource, prompt, agent runtime, or query-template execution service is
   needed.

   The `bifrost_sql` topic must explain the actual DataFusion-facing contract:
   call `bifrost.list_tables`, then `bifrost.describe_table` for field names
   and types; use a single `SELECT`, explicit projections, suitable time
   bounds and `LIMIT`; qualify table names exactly as discovery returns them;
   inspect `columns`, positional `rows`, and `terminal`; treat `isError` or a
   failed terminal as failure rather than a partial answer. Include tested
   examples for telemetry correlation, verification summary plus Drift/Eval
   detail by `result_id`, and a declared custom table. The examples must use
   real current schemas and the arguments `bifrost.query` actually accepts
   (`sql`, optional `deadline_ms`, `max_rows`, `max_bytes`). Explain that
   result ceilings refuse oversized results rather than truncating them.
   The other topics cover Card discovery/version selection/relationship
   navigation and binding/run/result interpretation. The overview directs
   agents to the tool descriptors for all remaining operator, principal, and
   gateway operations rather than copying their changing schemas.

## Implementation sequence and ownership

| Outcome | Existing owner and reuse | Required change and consumer closure |
| --- | --- | --- |
| Discover and resolve Cards | `wyrd-spec::registry::{ListCardsRequest, ListCardsResponse, GetCardResponse}`; `components::cards::{routes,service}` | Add typed MCP descriptors and dispatch in `wyrd-server/src/mcp`; share the HTTP Card authorization/audit operation for list and exact-ref reads, with no parallel list logic. Update the MCP catalog test. |
| Navigate the graph | `GetCardResponse.card.relationships`; Card service already hydrates inbound edges | Make tool descriptions and guide teach `outbound_refs`/`inbound_refs`; prove multi-kind Service-to-component navigation through real `/mcp`. No new graph API. |
| Read judgments and analytical evidence | `verification.get_run`; `bifrost.{list_tables,describe_table,query}`; Oracle query service | Keep result retrieval in SQL. Improve discovery text and examples; prove results, Eval/Drift details, telemetry, and declared custom-table reads through the existing tool. |
| Guide the agent | MCP `ServerInfo`, tool catalog/descriptor helpers, existing Bifrost schema and docs | Add `wyrd.guide` and concise server instructions in the same MCP handler. Keep guide examples small and executable against seeded tables. Update human and machine-readable documentation. |

Do the Card contract and MCP guide first, then the combined journeys and docs.
The guide's tool names, result paths, and SQL examples must describe the
shipped catalog at the same commit. This supplement changes server MCP and
docs, not the TASK-003 stdio proxy, shared credential chain, CLI host files,
or language SDK contracts.

## Documentation deliverables

- Rewrite `docs/src/content/docs/for-agents/index.svx` where its current
  `/api/v1/cards` and relationship-route examples disagree with the assembled
  `/v1` server routes. Make the page point agents to MCP discovery and the
  in-MCP guide, while retaining HTTP and schema references that are current.
- Add an MCP investigation page under `docs/src/content/docs/for-agents/`.
  Document connection through `wyrd mcp install`, the full shipped tool catalog
  by group (including permissions and read/write status), the four guide
  topics, the Card → relationship → binding/run → Bifrost evidence workflow,
  and direct table-query workflows. Document what `get_run` does and does not
  return, direct versus durable verification, and how to cite exact evidence.
- Extend the Bifrost reading/schema docs with MCP SQL examples that match the
  real catalog and `describe_table` output. Correct any example that names a
  nonexistent column or suggests a `bifrost.query` argument it does not have.
  Explain table-level and sensitive-payload authorization and the complete
  result ceiling. Keep detailed table schemas in the Bifrost docs; the MCP
  guide teaches discovery and a few tested query shapes.
- Ensure `llms.txt` and `llms-full.txt` discover the new/updated page through
  the existing docsite generator. Do not maintain a second hand-written MCP
  schema catalog: `tools/list` remains the live source for exact arguments.

## Acceptance and proof

1. An authenticated reader can list Cards with filters and cursor, resolve an
   exact Card reference, inspect the returned relationship refs, and traverse
   a Service plus at least two component kinds through `/mcp`. A missing Card,
   malformed ref/cursor, denied `cards:read`, and a foreign-tenant Card fail
   with the same Wyrd semantics as HTTP. Existing `cards.get` callers still
   work.
2. `tools/list` advertises the two new Card tools and `wyrd.guide` with closed,
   accurate input/output schemas and read-only annotations. The guide points
   to valid tool names, covers all four topics, gives usable SQL instructions,
   and does not return tenant data. Its query examples execute against the
   current schemas; catalog/guide mismatches fail a focused test.
3. A real MCP client can start with (a) a Service name, (b) a request/trace
   pivot, (c) a verification run or subject Card, and (d) a direct SQL ask.
   The paths reach Card context, relevant telemetry/custom tables, and
   verification/Eval/Drift evidence where seeded. Assertions cover a denied
   table or payload read, an empty result, and an oversized result refusal.
   Assert the tool returns structured failure without partial successful rows.
4. Through the completed TASK-003 `wyrd mcp install` connection, run the same
   seeded questions in Codex and Claude Code. Record the prompts, advertised
   tools, tool-call trace, final answer, and an evidence rubric: correct tool
   choice, exact Card/version and table attribution, supported conclusion,
   honest uncertainty, refusal under insufficient permissions, and no claim
   that a failed or incomplete query supplied complete evidence. These are
   human-reviewed agent usability checks, not nondeterministic CI gates.
   Automated MCP journeys remain the blocking contract tests.
5. Documentation names every shipped MCP tool group and the new guide, and a
   reader can follow its examples without guessing SQL fields. No stale route
   or nonexistent table/column is introduced.

Use the existing `wyrd-mcp` real-server journey suite for the new Card and
evidence scenarios (`mise run test:bifrost:journey:mcp`) and its exact focused
`mise exec -- cargo nextest run` selector during RED/GREEN iteration. Run
`mise run test:cards:integration` only if the shared Card read/list operation
changes. Run `mise run fmt`, `mise run lints`, `mise run docs:check`,
`mise run codegen:check` for the typed guide contract, and
`mise run check:deps` for its crate boundary. Inspect
`mise.toml` for the final capability gate when the write set is known; use
`mise run gate` only if the integrated change is broad enough under AGENTS.md.
No live model credential is required for the automated journeys.

## Decisions deliberately excluded

No fixed diagnostic report tool, fuzzy Card search, recursive graph traversal,
new analytical table, MCP write capability, second SQL parser, custom query
language, or client-owned verification history. A persisted investigation
case file belongs to a separately specified Audit capability if needed.

## Authority

- `changes/active/local-server-bootstrap/spec.md` revision 5 and
  `tasks/TASK-003-mcp-host-connection.md` (completed connection boundary).
- `AGENTS.md`; `architecture/agent-rules.md`;
  `architecture/wyrd-design.md` (Card relationships, observation identity,
  verification history); `architecture/bifrost-design.md` (query owner);
  `architecture/wyrd-security-posture.md` (tenant and permission boundary).
- Current owners: `crates/wyrd/wyrd-server/src/mcp/`,
  `crates/wyrd/wyrd-server/src/components/cards/`,
  `crates/wyrd/wyrd-server/src/query/collect.rs`,
  `crates/wyrd/wyrd-mcp/tests/bifrost/mcp/`, and `docs/`.
