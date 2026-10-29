# Repository standards review — TASK-001

Result: **FAIL**. One grouped mandatory Rust documentation finding remains.

## Immutable subject and scope

- Repository: `/home/thorrester/Documents/GitHub/wyrd`.
- Base: `911a8e278d829ab2722fc20ae881b67481dcf9f3`.
- Candidate: `c82414d38561cc3f7e17ac3f3ff52223378abde8`.
- Reviewed `git diff 911a8e278...c82414d38`, all 38 changed files, with surrounding
  source and the starting map in `scope.md`. No prior implementation acceptance
  is carried forward. No other reviewer's conclusions were read.
- This report audits repository standards, not task acceptance or alternative
  designs. Source remained unchanged; only this report was written.

## Authority coverage

| Changed surface | Applicable authority | Source coverage |
|---|---|---|
| `wyrd-spec` contracts, SlotName, error catalog and inline tests; Cargo manifest | Root `AGENTS.md`; `agent_docs/{index,rust,errors,agent-surfaces,maintainer-style}.md`; `architecture/README.md`; `wyrd-design.md` Prompt, Agent, Verifier and error-catalog sections; `TESTING.md` | `Verdict::response_format`, `portable`, `restrict_slots`, canonical citation/locator types, added inline tests, error metadata/details/catalog test, SlotName macro invocation and dev dependency |
| Skald spec authoring and Prompt mutation | Same root and Rust/maintainer guides; Prompt/Agent architecture; testing | Named-format lowering and tests; portable request builder and Chat lowering; full new `edit_response_schema_mut` and its test; response-type extraction |
| Skald Agent validation, loop, errors, gateway and integration helpers/tests | Root/Rust/errors/maintainer/testing guides; Agent architecture; approved spec's safe-error/sensitive-content boundary | Complete new `structured.rs`; modified run-loop and error/classifier bodies; callback, loop-encoding, session-conversion/journal and structured-output diffs with surrounding helpers |
| Vala Agent actions/checks and integration tests | Root/Rust/errors/maintainer/testing guides; Verifier architecture including evidence and tools/workspaces | Bind/invoke/execute/error mapping; removed `conforms`; source-set construction; full new Recording/sourced/sent_slots helpers and modified negative tests; unchanged final `verdict_from` boundary |
| MCP verification journey | Root/Rust/agent-surfaces/errors/testing guides; client model and Verifier architecture | New test in existing `pg_tests` module; existing `judging_server`, MCP call/structured-result helpers and shared judge fixture |
| Rust SDK journey | Root/Rust/maintainer/testing guides; client model | New repair test, modified PolicyDesk upstream setup and exhaustion assertions, SDK public imports and existing server fixture |
| Python journey, upstream fixture and live smoke | Root rules; `sdks/wyrd-sdk-python/AGENTS.md`; maintainer/testing guides; client model and Prompt architecture | Full changed bodies, public `wyrd.prompt` import, runtime schema extraction, existing live gateway/native provider clients and journey fixture/result flow |
| TypeScript journey and generated types/error codes | Root rules; `sdks/wyrd-sdk-ts/AGENTS.md`; agent-surfaces/errors/maintainer/testing guides; client model | Upstream and test changes, typed public SDK use, generated error union and SlotName documentation change |
| Shared YAML judge fixtures | Root rules; Prompt/Agent/Verifier architecture; testing fixtures | Two existing Agent envelopes, route and Prompt ownership; changed iteration budgets only |
| Architecture and reference docs / generated docs | Root rules; architecture authority map and normative-writing rules; `docs/AGENTS.md`, `docs/CONTRIBUTING.md`, `docs/PRODUCT.md`; agent-surfaces/errors guides | All architecture/reference changes, error catalog projection and llms-full synchronization |
| Spec/task packet and dependency lockfile | `changes/spec-driven-development.md`; root rules; immutable approved revision 2 and task; testing map | Packet approval/status/evidence, added dependencies and lock entries |

Only nested AGENTS applying to this write set are the Python, TypeScript and
docs rules. The SQL directory rules do not govern any changed path. No new or
modified production signature or field holds `PgPool`, `TenantConn` or
`OperatorPool`; the changed production code introduces no SQL authority.
Security-posture credential/diagnostic rules were consulted for diagnostic
safety; this range does not modify credentials, permission or tenancy mechanics.

## Rule results

| Applicable rule | Result | Evidence |
|---|---|---|
| Contracts in spec, execution in owning Rust runtime, thin SDKs; no new durable client behavior | PASS | Schema/ids/catalog remain in wyrd-spec; repair in existing Skald Agent loop; Vala owns evidence; SDK changes are tests/generated projection only |
| Dependency separation; IO/async/PyO3-free spec; no new wildcard versions or unearned features | PASS | Production `jsonschema` added only to skald-agent using existing workspace dependency (`default-features = false`); wyrd-spec addition is dev-only. Cargo.lock gains only those edges; no feature definitions change |
| Portable platform behavior, no forbidden OS mechanisms | PASS | Changes use serde/collections/jsonschema and existing provider loop; no target-OS gates or OS sandbox/resource mechanisms added |
| Stateful behavior has an owning struct, pure transformations remain synchronous; no new ad hoc runtime | PASS | `StructuredOutput` retains schema/validator; pure schema projection/check/feedback functions stay synchronous; loop and Vala IO reuse existing owners |
| Public errors use derive-backed stable catalog and one projection across surfaces | PASS | `AgentStructuredInvalid` has code/status/title/remediation metadata; Agent conversion constructs it; catalog test/TS generated union/docs replace decode code consistently; no handwritten public problem mapper added |
| Error diagnostics omit reply content; conversation correction remains conversation content | PASS for standards boundary | `Rejection` separates correction from schema-path/fixed violations; error projections store violations; Vala uses catalog code plus safe Agent error; no new ordinary log statement copies correction or reply. Runtime edge-case acceptance is covered by behavior/system reviewers |
| Domain ids and no raw parsing/IO unwrap in production changes | PASS | Slots use `BTreeSet<SlotName>`; no production unwrap introduced; generated-schema expects name invariants; parse failures return typed/fixed diagnostics |
| Every new/changed Rust item has meaningful rustdoc and required panic/error sections | **FAIL** | Grouped finding `REPO-001` below |
| Unit/integration/journey placement and no test suppression to bypass failures | PASS | Pure generation tests inline; Skald/Vala seams extend existing integration binary; SDK/MCP journeys use real WyrdTestServer/Postgres owners. Added ignores use the established explicit journey lifecycle and are selected by journey commands; no changed existing test is newly skipped |
| User journeys use public SDK surfaces, checked-in Cards and deployment-shaped harness | PASS | Existing fixtures retained; new recovery tests call ordinary verification APIs; mock-provider inspection observes repair traffic. No private runtime binding or database access added in SDK test bodies |
| Python boundary and test conventions | PASS | Public `wyrd.prompt.Prompt`; top-level tests; no new native binding/public type/export/stub change; no Python durable validation path added |
| TypeScript boundary, typed contract and naming conventions | PASS | Generated catalog matches Rust; no N-API or public wrapper behavior changed; existing kebab-case journey file and typed SDK result flow retained |
| Generated artifacts come from source, no committed generated JSON schema | PASS on source/diff plus recorded checks | TS/error/reference/llms changes correspond to Rust source/reference text; no JSON schema artifact added. Packet records regeneration/codegen checks; this reviewer did not rerun them |
| Architecture/reference authoritative shape and documentation conventions | PASS | Prompt/Agent/Verifier owning sections updated; reference remains neutral and reports error/budget behavior; generated API error table matches metadata; no new copied public example or unsupported adoption/performance claim |
| No unreviewed SQL/tenancy capability substitution | PASS / no changed SQL seam | No changed SQL/persistence signature, query, lease, credential, auth or storage implementation |

One nonblocking placement observation: the new method at
`skald-spec/src/prompt.rs:649–651` uses function-scoped imports despite
`agent_docs/rust.md` preferring module imports. It changes no behavior and is
not an explicit merge gate, so it creates no remediation finding.

## REPO-001 — changed Rust helpers omit required documentation

Classification: **VIOLATION**. Governing obligation: root AGENTS Code says
“Every new or changed Rust item has rustdoc” and “Missing rustdoc blocks merge.”
The Rust topic guide explicitly includes private helpers, test helpers, tests,
fields and `# Panics` for possible panics. This is an explicit gate, independent
of the helpers' runtime risk or green Clippy results.

The candidate adds new structured-output test builders and widens existing
fixtures for sibling-module reuse. Several of those changed items have no
rustdoc; others use explicit panic/expect paths without their required panic
contract. The shared helpers are actually consumed by the new repair suite, not
dormant surfaces. The resulting repository cannot satisfy its mandatory
documentation gate, and maintainers lack the input/recording/panic contracts
for the helpers on which the new proof depends.

Affected-item inventory (paths relative to `crates/`):

| Location | Gap |
|---|---|
| `skald/skald-agent/tests/integration/structured_output.rs:32` `registry_returning_text` | Changed body; no rustdoc describing one queued OpenAI answer |
| Same file `:84` `format` | New helper; no rustdoc or `# Panics` for invalid/non-object input |
| Same file `:88` `chat_prompt` | New helper; no rustdoc or `# Panics` for schema/Prompt construction failure |
| Same file `:138` `encodings` | New helper's rustdoc lacks `# Panics` for provider Prompt construction |
| Same file `:192` `text_of` | New helper's serialization expect lacks `# Panics` invariant |
| Same file `:197` `last_user` | New helper's rustdoc lacks `# Panics` for non-Chat request, absent user turn, or non-text content |
| Same file `:224` `structured_output_parses_when_schema_declared` | Changed test adds iteration assertion and uses the new builders; no rustdoc / `# Panics` |
| `skald/skald-agent/tests/integration/callbacks.rs:346–347` `RecordingProvider` and new `name` field | Changed/shared owner and new field have no rustdoc |
| Same file `:353` `RecordingProvider::new`, `:399` `Provider::name` | Changed functions have no rustdoc for default OpenAI registration / configured-name reporting |
| `skald/skald-agent/tests/integration/loop_basic.rs:78` `openai_tool_call_response` | Widened helper reused by repair suite; no rustdoc |
| `skald/skald-agent/tests/integration/loop_gemini.rs:50` `gemini_text_response` | Widened helper reused by repair suite; no rustdoc |
| `skald/skald-agent/tests/integration/session_journal.rs:617,622,626` `RecordingJournal`, `new`, `events` | Widened owner/methods reused by repair suite; no rustdoc for in-memory recording and snapshot semantics |
| `skald/skald-spec/src/prompt.rs:2186` `response_schema_edit` | New test module has no module rustdoc |
| `vala/vala-verify/tests/integration/blocks_agent.rs:824` `Recording::send` | New method uses mutex `expect("unpoisoned")` but lacks `# Panics` |
| `wyrd-spec/src/verification/execution.rs:1091` nested test helper `portable` | New recursive assertion helper lacks `# Panics` for rejected keyword/reference sibling |

Existing unchanged undocumented items are excluded (for example the final
`text_prompt_leaves_structured_output_none` test, unchanged RecordingJournal
append implementation and unchanged fields). Existing documented helpers
whose visibility changed (`responses_answer` and `message`) do have rustdoc.

Smallest correction: add concise, behavior-specific rustdoc and required panic
sections to this bounded changed-item inventory, preserving implementation and
assertions. Do not redesign helpers or add checks/dependencies/suppressions.
Burden is documentation only; deleting assertions or shared helpers would
discard approved proof rather than satisfy the gate. Static inspection of the
listed items and normal format checks directly close the documentation gap;
no new behavior test or full journey rerun is needed for this correction.

## Verification and limits

Root's independently executed `verification.md` records 303 passing tests:
eight Verdict execution tests and 295 Skald-spec/Agent/Vala tests, with clean
range whitespace. The task packet additionally records 1,054 total owner tests,
23 hosted-code guards, four-surface focused Postgres journeys, four live cases,
and repository/language/codegen/docs checks. Those wider recorded runs were
not reproduced by this standards reviewer. No Cargo or live commands were run
by this reviewer, avoiding overlapping builds in the shared checkout.

No change to the candidate or authority was observed. No optional refactoring,
new test harness, feature or security hardening is proposed. The sole retained
standards finding is `REPO-001`, subject to fresh independent validation.
