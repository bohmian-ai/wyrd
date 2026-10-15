# Registry data-integrity domain review

Result: **FAIL**

## Reviewed boundary and immutable subject

Base `c1aac2d132b19848ca1940c3907e1fa68a8d08a5`; candidate/observed HEAD `02c06e21bd5fd613341935a843fabe5350ea0a9d`. Reviewed cumulative diff. This audit covers server-generated inline Prompt identity, transaction composition, normal registry line resolution, replay, lifecycle races, immutable Agent references, relationship persistence, and consumption by the shared hydrated state. No reviewed source changed.

## Authority and coverage

Authorities inspected: approved `changes/active/agent-prompt/spec.md` revision 2; original `tasks/TASK-001-agent-prompt.md` scenarios 1–3 and acceptance criteria; supplied AGENTS.md, `architecture/agent-rules.md`, protocol doctrine 17 and Agent/reference/authoring identity sections, public doctrine diff, reference router, architecture patterns, spec-driven development, maintainer style, and testing workflows. REQ-002/003/005 and INV-001/003 are the governing data-integrity obligations. Implementation-evidence PASS claims were treated as claims, not proof.

CodeGraph was used before expanding source coverage. Existing owners were inspected independently, including their full relevant bodies:

| Boundary | Source inspected | Assessment |
|---|---|---|
| Authenticated registration to transaction | server `components/cards/service.rs::register_card`, `resolve_external`, graph planning, `RegistrationWriter::write` | Existing single TenantConn and operation reservation reused; generated child is absent from planned graph and external-ref set. |
| Generated identity and canonical value | `inline_prompt_submission`, `submission_card`, `persist_node` | Correct derived name, effective space, Prompt kind, independent unversioned intent; spec canonicalization occurs before persistence. |
| Registry version authority and concurrency | `resolve_existing`; SQL `version_sql::lock_version_line`, `version_resolve::resolve_version/latest_stable_in_line`, `get::find_card_by_ref`, `register::insert_card_row`; identity index migration | Tenant-qualified transaction lock serializes writers, but auto selection sees only Active rows; generated dependencies obtain no lifecycle row lock. Findings below. |
| Replay and rollback | `hash_request`, `replay`, `replay_operation`, `wait_for_replay`, `replay_seed`, `insert_registration_operation`, commit; TenantConn lifecycle | Generated child included in durable response seed. Same-key winner handled by reservation; distinct keys enter independent version-resolution transactions. Error drops rollback generated child, Agent and operation together. |
| Lifecycle and sibling writers | `initialize_uploads`, `complete_card_inner`, completion/blob seams, `delete_card_with_kind/by_ref`; SQL delete owner, `recheck_active_card_refs`, `persist_outbound_relationships`; relationships migration | Existing Pending → post-commit activation is preserved. External refs have FOR SHARE protection; generated reuse bypasses it. FK proves tenant/UID existence, not Active status. |
| Graph and sibling consumers | spec graph `build`, `graph_ready_submissions`, topo ordering and composition validation; `EffectiveSpecs`; shared state `load_index`, spec/relationship validation, `TypedCards::agent`, `HydratedAgent` | Multiple pinned versions and separately authored same-name Prompt submissions are valid graph identities. Shared state keys include UID and navigation uses that validated exact key; no fresh version choice on lookup. |
| Actual proof | new HTTP `inline_agent_prompt_materializes`; Rust `agent_prompt_registration_identity`, navigation test; existing `concurrent_same_key_resolves_to_one_registration` | Sequential reuse/versioning/old-pin/name collision/tenant and permission checks present. Newly added tests do not cover the lifecycle race, same-transaction line collision, or distinct-key overlap. |

## Material findings

### DATA-001 — Generated Prompt reuse bypasses the registry's delete-versus-reference lock

Classification: **INCORRECT**. Severity: **Important**.

Changed location: `crates/wyrd/wyrd-server/src/components/cards/service.rs:1130–1134` (generated child `persist_node` result immediately becomes Agent reference).

Violated obligations: REQ-002 registered independent Prompt relationship; REQ-005 preservation of atomic registry/conflict guarantees; INV-003 existing registration authority reuse. The existing external-reference owner explicitly guarantees that a dependency remains Active until the referencing transaction commits (`queries/cards/relationships.rs:17–28,36–68`).

Reachable producer-to-consumer path:

1. Independently register an Active Prompt named `desk-prompt` with the same content the new Agent `desk` will inline. This name collision is expressly approved by REQ-003; it requires no existing inbound Agent.
2. A deletion transaction takes that Prompt's `FOR UPDATE` row lock (`queries/cards/delete.rs:19–26,77–105`), observes no inbound references (`105–125`), and pauses before its update/commit.
3. A distinct registration request for inline Agent `desk` resolves only authored external refs before its write. The generated Prompt does not exist in that set. Its `persist_node` takes the version-line advisory lock, which the deletion owner never acquires. The Active-row version query and `find_card_by_ref` are ordinary nonlocking reads (`version_resolve.rs:48–67`; `get.rs:147–165`), so they can read the previously committed Active row while deletion holds its lock.
4. The generated outcome's exact UID is assigned to `agent.prompt` and the Agent's outbound relationship is inserted. If the relationship FK waits for deletion's row lock, deletion can still commit first: the retained soft-deleted row satisfies the tenant/UID FK (`20260601000018_card_relationships.sql:23–25`). No code rechecks status afterwards.
5. Registration can commit a new Agent referencing a deleted Prompt. Agent activation checks its own manifests/blob and does not revalidate outbound dependency lifecycle. Public reads exclude deleted targets (`queries/cards/get.rs:12–27`), so normal graph hydration cannot recover that Prompt.

The same race cannot pass the ordinary external-ref path: its exact UID/Active `FOR SHARE` recheck waits, then rejects the deleted target. Serializing version writers is insufficient because deletion uses the row lifecycle lock.

Smallest correction boundary: retain shared version selection and generated identity, but route reuse of an existing generated Prompt through the existing exact Active-reference locking authority on the same registration TenantConn before persisting its Agent. Fresh children written in this transaction remain caller-owned Pending children; do not incorrectly demand they already be Active, add a second lifecycle owner, or change delete policy.

Focused closure proof: start from an independently authored matching `desk-prompt` with no inbound refs; coordinate the delete row lock before inline Agent materialization and let deletion commit while registration is pending. Assert registration either retains a live protected exact target or refuses/rolls back all its writes with existing catalog behavior; it must never create a live Agent pointing to a deleted UID. Exercise direct HTTP with distinct keys, not same-key replay.

### DATA-002 — A valid composite can allocate its generated Prompt version twice before activation

Classification: **INCORRECT**. Severity: **Important**.

Changed location: `crates/wyrd/wyrd-server/src/components/cards/service.rs:1130–1133`; generated `metadata.version = None` at `1574`.

Violated obligations: REQ-002 same composite Card transaction; REQ-003 unchanged inline content reuse and normal same-name independently authored line semantics; INV-003 existing graph/registration ownership.

Concrete input: one Service-root composite contains a pinned authored Prompt `desk-prompt@1.0.0`, an Agent `desk@1.0.0` with the identical Prompt inline, and Service components referencing both sibling Cards. Nothing in graph/composition validation rejects this input: identities differ by kind; Prompt sorts before Agent (`CardKind` order and `graph/topo.rs::topo_sort`). A second valid form is two pinned versions of Agent `desk`, referenced by distinct Service aliases, whose inline Prompt bodies match; graph identities include version (`graph/mod.rs:136–154`; existing `resolve.rs::sibling_identity_includes_version` proves that contract).

The first Prompt is inserted Pending by `persist_node` (`service.rs:1209–1221`). The generated child then auto-resolves on the same TenantConn. `latest_stable_in_line` ignores every Pending row, including the caller's own newly inserted one (`version_resolve.rs:48–50`). On an initially empty line it returns Fresh `1.0.0` (`103–119`); the second insert hits the live identity unique index, which includes Pending (`20260601000014_card_registration_operations.sql:60–62`). It is mapped to `WYRD_REGISTRY_503_REGISTRY_UNAVAILABLE` by `insert_card_row`, and the otherwise valid entire graph rolls back. Repeating the request cannot succeed while the line stays empty. With an older Active line, both allocations can similarly select the same next version.

This is not a request for sharing across differently named Agents or overwriting an authored exact pin. It concerns the same explicitly approved generated name/space/tenant line, with identical content, already selected inside the very transaction materialization must compose with. The normal generated Prompt is not represented in the plan's existing graph/outcome identity authority, so its insertion creates an additional same-line producer the Active-only resolver cannot see.

Smallest correction boundary: make the existing registration/version owner account for matching Prompt outcomes already selected in its current transaction when materializing this same line, reusing the exact selected identity for identical content and preserving existing exact-pin conflict behavior. Do not widen public read selection to arbitrary Pending Cards, introduce a separate graph store/version policy, or activate a child before the composite commits. Reuse the current transaction and existing canonical identity/hash authority.

Focused closure proof: register the Service-root same-request authored-Prompt/inline-Agent composite against an initially empty line, assert exactly one Prompt UID and Agent pin/relationship to it, and hydrate it. Also cover two versions of the same Agent with unchanged inline content in one request, so a second ad hoc collision branch does not leave the shared cause open.

## Proof limits and unaffected guarantees

No builds/tests were executed by this reviewer, per orchestration coordination; source analysis establishes the reachable interleavings and deterministic same-transaction collision. The task records GREEN A/B/R/P/T and relevant gates, but those tests are sequential and do not exercise these cases. Existing same-key concurrency proof concerns a standalone Prompt and one operation reservation, so it does not prove distinct-key materialization or caller-local Pending reuse. Recorded TDD limitations remain as recorded; no authorship/baseline comparison was performed.

Positive source assessment: no fabricated identity, mutation of old Agent versions, tenant-unqualified child write, cascading delete, new version field, or hidden lookup registration was found. Name validation failures roll back the caller's entire Card transaction. Same-key replay preserves recorded generated Prompt UID/outcomes; sequential distinct-key registration deduplicates after normal activation. Later Agent versions persist newly selected exact refs without changing prior rows. Shared state validates closure and keys with UID before local Agent-to-Prompt navigation.

Only this report was written. Runtime reproduction and the broader language/runtime surface were not rerun; the retained risks are the two registry integrity/composition paths above.
