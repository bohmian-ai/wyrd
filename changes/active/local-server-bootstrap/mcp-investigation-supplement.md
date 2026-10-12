# MCP investigation supplement to TASK-003

**Status:** executable follow-up to completed TASK-003. TASK-003 connected MCP
hosts to the existing `/mcp` catalog. Implement this supplement against that
connection; keep the approved spec and completed TASK-003 unchanged. The
supplement extends the server's existing MCP catalog, not the proxy or host
installer.

## Outcome

A developer connects Codex, Claude Code, or another supported MCP host through
`wyrd mcp install` and can ask open-ended questions about a Service, component,
request, verification run, Eval outcome, or Bifrost table. The host can discover
the right tools, inspect exact Card relationships, query authorized evidence,
and explain an answer with the Card UIDs, result IDs, table names, and query
results it actually used. Wyrd supplies capabilities and guidance; the agent
chooses the investigation path. There is no fixed report entry point.

## Decisions to implement

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
| Guide the agent | MCP `ServerInfo`, tool catalog/descriptor helpers, existing Bifrost schema | Add `wyrd.guide` and concise server instructions in the same MCP handler. Keep guide examples small and executable against seeded tables. Update the separate docsite rebuild as specified below. |

## Execution order

Work in the existing `wyrd-mcp --test mcp` journey target. Add each named test
before its production change, run its focused command to see the missing
behavior, implement the corresponding step, and rerun it. The test names below
are the required new journey selectors, not new task packets.

1. **Discover Cards.** Add
   `verification::pg_tests::agent_discovers_service_and_component_cards` in
   `crates/wyrd/wyrd-mcp/tests/bifrost/mcp/verification.rs`. Seed a Service
   with at least two component kinds and a second tenant. Assert filtered
   listing, opaque cursor continuation, exact-reference read, inbound and
   outbound relationship navigation, and unchanged UID read. Assert bad
   reference/cursor, missing or foreign Card, and denied `cards:read` through
   the real MCP client. Then add `cards.list` and `cards.get_by_ref` descriptors
   and dispatch in `crates/wyrd/wyrd-server/src/mcp/`. Reuse
   `components::cards::service::list_cards` and
   `components::cards::routes::get_card_by_ref_for`; share the HTTP list
   authorization/audit step rather than copying it into MCP. Update the exact
   catalog assertion in `discovery.rs`.
2. **Teach MCP.** Add
   `discovery::pg_tests::agent_discovers_mcp_guide_and_sql_examples` in
   `crates/wyrd/wyrd-mcp/tests/bifrost/mcp/discovery.rs`. Assert discovery
   instructions point to `tools/list` and `wyrd.guide`, every guide topic
   returns its typed shape, named tools exist in the advertised catalog, and
   SQL examples work after substituting seeded identifiers. Then add the
   guide contract in `wyrd-spec` and one read-only MCP tool in the existing
   server handler. Keep overview guidance concise; `tools/list` supplies the
   caller's actual catalog and JSON schemas.
3. **Prove open-ended evidence reads.** Add
   `query::pg_tests::agent_investigates_from_multiple_evidence_pivots` in
   `crates/wyrd/wyrd-mcp/tests/bifrost/mcp/query.rs`. Start from a
   request/trace, a Card UID, a verification `result_id`, and direct SQL.
   Describe tables before using their fields; retrieve telemetry, a declared
   custom table, a verification summary, and Drift/Eval detail. Assert empty
   evidence, denied table or sensitive payload, and query-size refusal. Keep
   all reads on the existing `bifrost.query` path; change only inaccurate tool
   descriptions or guide text discovered by the journey.
4. **Publish guidance.** After the tool behavior and examples pass, complete
   the docsite worktree update below. Keep its MCP reference and procedures
   aligned with the delivered catalog; do not edit the old `docs/` tree in
   this worktree.
5. **Validate with coding agents.** Use the TASK-003 `wyrd mcp install`
   connection for Codex and Claude Code against a seeded local server. Run
   the four prompts in the acceptance section, save the tool trace and answer,
   and score the evidence rubric. This is a human-reviewed usability check;
   deterministic MCP journeys are the blocking automated proof.

Focused commands for steps 1–3 (run one at a time with the repository-managed
Postgres wrapper):

```bash
scripts/postgres/with-test-postgres.sh -- bash -lc "mise run db:migrate:inner && mise exec -- cargo nextest run --locked -p wyrd-mcp --test mcp -P journey --run-ignored=all -E 'test(=verification::pg_tests::agent_discovers_service_and_component_cards)'"
scripts/postgres/with-test-postgres.sh -- bash -lc "mise run db:migrate:inner && mise exec -- cargo nextest run --locked -p wyrd-mcp --test mcp -P journey --run-ignored=all -E 'test(=discovery::pg_tests::agent_discovers_mcp_guide_and_sql_examples)'"
scripts/postgres/with-test-postgres.sh -- bash -lc "mise run db:migrate:inner && mise exec -- cargo nextest run --locked -p wyrd-mcp --test mcp -P journey --run-ignored=all -E 'test(=query::pg_tests::agent_investigates_from_multiple_evidence_pivots)'"
```

No change to the TASK-003 stdio proxy, shared credential chain, CLI host
files, or language SDK contracts is required.

## Docsite Rebuild Update

After implementation and verification pass, update the developer docsite
rebuild so developers can use what this task delivered. The rebuild lives in
the `wyrd-doc-site` worktree under the `developer-docsite-rebuild` change
packet. Its spec sets the page map, and its tasks set the page rules. Use
`$human-tech-docs`, keep pages `draft: true` with an accurate `status`, and
describe only behavior this task delivered and verified. Check the published
MCP tool catalog against the authorized `tools/list` response and server
descriptors; run each documented command and query, including a denied-call
example, against the delivered build. Then run the docsite's
`docs:check:commands`, `docs:linkcheck`, `docs:build`, and `docs:a11y`.
Record the pages changed and the checks run in Implementation Evidence.

Pages:

- Connect → *Connect an agent through MCP*: extend the TASK-003 host connection
  guide with `tools/list` and `wyrd.guide` discovery, one Card read, one bounded
  Bifrost query, and links to the MCP reference and investigation guides.
- Reference → *MCP tools*: list every shipped tool by exact wire name and
  capability group, with its purpose, read/write status, permission,
  input/output shape, refusal behavior, and limits. Explain caller-dependent
  visibility and cover all four `wyrd.guide` topics. Use the delivered server
  descriptors and `tools/list` as the catalog source.
- Build and verify → *Write and query custom data with Bifrost*: show table
  discovery and description, DataFusion SQL using actual field names, a bounded
  `SELECT` across supported telemetry or custom tables, and interpretation of
  `columns`, positional `rows`, and `terminal`.
- Build and verify → *Run a verification and inspect its evidence*: show Card
  discovery and exact relationship navigation, binding and run status, and
  Bifrost queries for persisted verification and Eval/Drift evidence. Explain
  what direct execution returns and what it does not persist.

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
   seeded questions in Codex and Claude Code:

   | Entry point | Prompt shape | Expected evidence path |
   | --- | --- | --- |
   | Service | “Analyze service `<seeded-service>` and explain whether its components are behaving as expected.” | Card discovery → exact Service/component refs → bindings/results → relevant Bifrost tables. |
   | Request/trace | “Where did request or trace `<seeded-id>` fail, and why?” | Table discovery → correlated spans/logs and related Card/result evidence where present. |
   | Verification | “Show the latest seeded verification and Eval findings for `<seeded-card>`.” | Card/binding/run context → result IDs → verification and Eval detail SQL. |
   | Direct query | “Run this read-only query and explain its result: `<seeded-SQL>`.” | `bifrost.query` → columns, rows, terminal, and bounded interpretation. |

   Record each prompt, advertised tools, tool-call trace, final answer, and an
   evidence rubric: correct tool choice, exact Card/version and table
   attribution, supported conclusion, honest uncertainty, refusal under
   insufficient permissions, and no claim that a failed or incomplete query
   supplied complete evidence. These are human-reviewed usability checks,
   not nondeterministic CI gates. Automated MCP journeys remain the blocking
   contract tests.
5. The docsite rebuild's Connect page leads to the complete Reference MCP
   catalog and runnable Observe/Verify procedures. The reference covers every
   shipped tool group and the new guide; its examples use real fields and
   supported routes. The docsite commands above pass in `wyrd-doc-site`.

After the focused commands, run the existing `wyrd-mcp` real-server journey
suite (`mise run test:bifrost:journey:mcp`). Run
`mise run test:cards:integration` only if the shared Card read/list operation
changes. Run `mise run fmt`, `mise run lints`,
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
  `crates/wyrd/wyrd-mcp/tests/bifrost/mcp/`. Documentation belongs to the
  `wyrd-doc-site` worktree's `developer-docsite-rebuild` packet.

## Implementation evidence

Card MCP operations now belong to `wyrd-server/src/mcp/cards.rs`: the existing
UID read and the two discovery/reference reads. `verification.rs` owns only
verification tools. Dispatch and catalog use the Card module. The shared HTTP
read/list operations retain authorization, audit, tenancy, and errors.

Tests use the checked-in `register_and_hydrate` and `observe_a_run` Card graphs.
The new `mcp_investigation` fixtures declare the custom dataset and native
Drift/Eval reports. Journey methods separate setup, discovery, relationship
navigation, evidence reads, refusals, and shutdown.

### Diagnoses during exact journey iteration

Each unexpected failure was read with tracing enabled and independently
diagnosed by a fresh read-only agent before changing tests.

| Symptom / evidence | Cause and fix site | Other callers checked |
| --- | --- | --- |
| Guide peer-info borrow did not compile | Retain `peer_info` before borrowing instructions in discovery fixture | Existing discovery reads |
| Card version assertion did not compile | Compare typed VersionBlock values in Card fixture | Existing Card reference assertions |
| Service pagination found three Cards | bootstrap_service adds its own Service; bootstrap_agent keeps the fixture's two Services | Shared bootstrap implementation and existing journey setup |
| Result ID did not compile | new returns Result; use established new_v7 construction | Result mapper and verification fixtures |
| Card/guide refusal assertion failed | These tools use JSON-RPC problem errors; reuse the existing refusal helper moved to connectivity | Existing verification refusals; Bifrost retains structured query errors |
| Absent exact Card returned 400 | Missing space is invalid before lookup; specify default space for the 404 case | Shared get_card_by_ref_for and HTTP readers |
| Role construction rejected names | RoleName allows underscores, not hyphens; use mcp_no_cards and mcp_evidence_reader | Principal journey role fixtures |
| Dataset insert returned CardScopeDenied | Unbound admin principal cannot attribute dataset rows to an arbitrary Card; custom SQL fixture uses Correlation::default | Existing dataset startup/ingest journeys; exact subject attribution remains on result rows |
| Trace SQL returned [] at investigation.rs:266; /tmp/wyrd-mcp-correlation-trace.log | Hand-copied trace hex contained an extra byte; derive it from canonical_signals::TRACE_ID in query and discovery fixtures | Shared canonical telemetry fixture and guide SQL substitutions |

Focused Card and guide journeys passed before the module relocation. The final
exact journeys, owner gates, docsite pages, and installed-host checks are pending.
No commit, merge, or deployment has been performed.

Payload diagnosis: `/tmp/wyrd-mcp-payload-trace.log` reproduced a successful
verification `details` read. Independent inspection of Bifrost design and
`Oracle::authorize_payload_columns` established that table query permission
covers verification/Eval columns; the additional payload permission applies
only to gateway request/response payloads. The negative journey now selects
`vala.gateway.calls.request_payload_json`, and the guide describes the actual
permission rule. Existing Oracle tests and shared MCP/HTTP/gRPC callers were
checked; authorization was not changed.

All three exact investigation journeys passed after Card tool relocation
(`/tmp/wyrd-mcp-final-focused.log`, 3/3). Clippy then flagged `format_collect`
in the two trace substitutions. A traced rerun and independent diagnosis
identified per-byte allocations; both now use the existing validated
`wyrd_spec::vala::ids::TraceId::from_bytes(...).to_hex()` owner. No formatter,
dependency, or permission behavior was added. Final lints pending.

Host-seed diagnosis: `/tmp/wyrd-host-path-trace.log` confirmed loader
PATH_ESCAPE from newly added sibling-directory component paths. Independent
inspection identified the intended per-document loader sandbox. The Service
fixture now uses exact registry CardRefs, and setup registers the existing
shared Model/Agent fixtures first. Loader containment is unchanged. The exact
evidence journey and temporary host-seed copy are the affected callers.

Doc reader tests covered connection/credentials, writer lifecycle, correlated
query completeness, latest retained findings, and mutation/read permissions.
Clarifications were applied to catalog permissions, fresh-table expectations,
and completed per-implementation result selection. Production a11y visits no
drafts; a local dev preview applied its existing check_page to all four pages
successfully. Two initial ad hoc markup assumptions failed because reference
pages need no fenced block and procedures need no table; independent readers
confirmed the source and rendering were correct. Those invented markup
expectations were removed; existing accessibility requirements were retained.

Installed-host diagnosis: initial JSONL traces are preserved in
`evidence/mcp-investigation-hosts/attempt-01`. Claude returned authentication
failure because its `--bare` mode intentionally skips OAuth/keychain lookup.
Codex correctly reported no available MCP evidence: installation fell back to
`wyrd` while the binary was absent from PATH. A fresh independent diagnosis
checked native CLI help, HostInstaller::from_process, and proxy callers. The
temporary harness now places the compiled CLI on PATH before installation and
retains Claude's ordinary saved-login lookup. No production code or normal
host configuration changed. Usability rerun pending.

Claude second-attempt diagnosis: the init trace showed Wyrd connected but
`tools: []`. Independent native-help/trace inspection established that
`--tools ""` disables all tools, including MCP. The temporary harness removed
that flag, retained strict MCP config, and explicitly denied shell/file/web
and subagent tools. Initial no-evidence answers were honest but do not pass
usability; they are preserved under `attempt-02`. Production behavior is
unchanged.

Production catalog diagnosis: Claude's debug trace retried tools/list and
rejected `tools.32.inputSchema.type`. The descriptor is
`operator_connections.create`: its provider-tagged enum generates a root
oneOf without a type. Independent inspection of rmcp's native input/output
schema builders confirmed MCP requires an object input root; output schemas
may be nonobjects. The shared `mcp/principals.rs::tool` now adds the required
input root type while retaining generated variants/definitions; outputs are
unchanged. Card, guide, principal, verification, and Operator descriptors
share this owner; gateway/Bifrost own builders already declare object inputs.
The exact guide-discovery journey now asserts this property across the real
catalog and failed RED (`/tmp/wyrd-mcp-schema-root-red.log`) before the fix.
No permissions or operation payloads changed. GREEN/Claude rerun pending.

Attribution fixture diagnosis: Codex's real tool trace listed investigation-drift
but both UID/reference reads refused its inactive immutable blob. Independent
inspection found VerificationFixture::custom_drift_verifier inserts only a
DB row with placeholder spec hash, which is appropriate for its internal
engine tests but cannot supply an agent-readable Card. Evidence setup now
registers a checked-in Custom Drift Verifier through Cards, retains its exact
receipt ref, and uses that ref in the existing native result publisher. The
journey reads both retained Verifier Cards before selecting result details.
SYSTEM writer provisioning remains the existing fixture helper. Registry
hydration and all existing engine-fixture callers are unchanged.

Installed discovery closure: the stdio proxy originally discarded upstream
instructions. The existing exact CLI proxy journey now asserts tools/list and
wyrd.guide are present in the host's initialization metadata; it failed RED
at `mcp_journey.rs:152` (`/tmp/wyrd-mcp-proxy-instructions-trace.log`). The
existing McpProxy now forwards only upstream instructions while preserving
independent host/upstream protocol negotiation. No transport, session,
credential, or operation behavior was added.

Verification environment diagnosis: overlapping wrappers produced
`role wyrd_platform already exists` and timed-out pools. A fresh independent
reader confirmed the wrapper's 48-character Compose-name truncation drops
BASHPID with this worktree name, making both processes share a lifecycle.
Affected attempts are invalid environment runs, not product assertions.
Remaining environment-owning lanes are run serially; no role check, timeout,
test, or assertion was weakened. The existing Postgres concurrency task is
outside this supplement's scope and was not claimed as passed.

### Final acceptance evidence

| Acceptance criterion | Implementation evidence | Verification evidence | Result |
| --- | --- | --- | --- |
| Exact Card discovery/navigation and HTTP semantics | mcp/cards.rs; shared cards/routes.rs; checked-in Card graphs | Exact Card journey; Card integration lane | PASS |
| Typed guide/catalog, valid tool names and SQL examples | wyrd-spec/src/mcp.rs; mcp/guide.rs; runtime tools/list schemas; shared MCP input normalization | Exact discovery journey; codegen:check; actual Claude 35-tool discovery | PASS |
| Multiple retained-evidence pivots and refusals | query/investigation.rs; mcp_investigation fixtures; native result mapper/outbox | Exact evidence journey; complete 17-test MCP lane | PASS |
| Installed Codex and Claude Code questions and rubric | Compiled CLI proxy/install; forwarded instructions; isolated temporary host configs | Eight entry-point runs plus two restricted-query runs; evidence/mcp-investigation-hosts/rubric.md | CHECKED; model qualifications recorded |
| Four source-backed docsite pages | Connect MCP; Reference MCP tools; Observe custom data; Verify evidence in wyrd-doc-site | Commands, links, build, a11y lanes; five fresh reader tests; existing a11y checks applied to all four draft previews | PASS |

Final exact commands (all passed):

```bash
scripts/postgres/with-test-postgres.sh -- bash -lc "mise run db:migrate:inner && WYRD_LOG=info mise exec -- cargo nextest run --locked -p wyrd-mcp --test mcp -P journey --run-ignored=all -E 'test(=verification::pg_tests::agent_discovers_service_and_component_cards) | test(=discovery::pg_tests::agent_discovers_mcp_guide_and_sql_examples) | test(=query::pg_tests::agent_investigates_from_multiple_evidence_pivots)' && WYRD_LOG=info mise exec -- cargo nextest run --locked -p wyrd-cli --test cli -E 'test(=mcp_proxy_discovers_and_reads_with_shared_auth)'"
mise run test:bifrost:journey:mcp
mise run test:cards:integration
mise run fmt
mise run lints
mise run codegen:check
mise run check:deps
```

The exact command passed 3/3 MCP cases and 1/1 CLI proxy case
(`/tmp/wyrd-mcp-final-exact.log`). The final MCP owner lane passed 17/17
(`/tmp/wyrd-mcp-owner-final.log`). Card integration passed its migration,
client, SQL, registration-route, verification-route, and SDK graph cases
(`/tmp/wyrd-mcp-card-integration.log`). Full workspace/all-feature/all-target
Clippy and production server Clippy passed (`/tmp/wyrd-mcp-lints.log`).
Formatting, dependency checks, and codegen checks passed; generated files
were produced by codegen, not edited manually.

In wyrd-doc-site, all four requested commands passed:
`mise run docs:check:commands`, `mise run docs:linkcheck`,
`mise run docs:build`, `mise run docs:a11y`. Draft pages remain unpublished.
The published a11y lane visits zero draft pages; the existing check_page
function separately passed against locally rendered HTML for all four drafts.
The humanizer lint passed. Documentation reader qualifications and their
corrections are recorded above.

Runtime host transcripts/catalogs/prompts/answers are in the repository-ignored
`evidence/mcp-investigation-hosts/` directory; rubric.md records hashes and
human review qualifications. All ten final runs exited 0 and used only Wyrd
read calls (plus Claude ToolSearch). Both native hosts distinguish the
restricted payload refusal from an empty/successful query. No Wyrd machine
credential occurs in the artifacts. A temporary server example and host
configuration were removed from the candidate; ordinary host configuration
was never edited.

Non-goals remain excluded: no report tool, fuzzy search, recursive traversal,
analytical store/table, MCP write capability, second SQL parser, query
language, client-owned history, or persisted investigation case was added.
The custom dataset is a declared fixture, not a new analytical server table.
Final tracked/untracked diffs and diff --check were inspected in both
worktrees. The existing user-authored supplement edits were retained.

No commit, approval, merge, push, publication, or deployment was performed.
This reversible candidate is IMPLEMENTED and awaits independent
$wyrd-task-review. Material limits: model conclusions still need human
review (see rubric), and Postgres wrappers were run serially after their
long-name collision was diagnosed. No global gate or live cloud lane was
needed for the classified Card/MCP/CLI/docs write set.

### Standalone guide schema cleanup

Removed the four standalone mcp_guide JSON exports and their generator
registrations: no runtime or SDK consumer reads them. MCP still derives the
guide input/output schemas from the Rust DTOs for tools/list. Existing SDK
and gateway schema dependencies remain. After removal, mise run codegen:check,
mise run fmt, and git diff --check passed; codegen did not recreate the files.
