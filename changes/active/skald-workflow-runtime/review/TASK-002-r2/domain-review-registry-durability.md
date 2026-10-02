# Registry durability and concurrency domain review

## Subject and result

**FAIL** — one proposed material registration regression, `REG-DUR-R2-001`.

Base: `0569b79702218600c4f9790f45cc03100d5c6f1c`.
Candidate: `e7d16b5bd622b9a564a49edb18239df7f209ca92` (HEAD confirmed unchanged).
Authority: approved Revision 12, original TASK-002 and replacement
TASK-002-cleanup. Prior FIND-TASK-002-1 and FIND-TASK-002-4 were considered as
closure hypotheses, not assumed resolved from implementation evidence.

## Boundary and authority coverage

Reviewed the cumulative changed registration, SQL relationship, loader,
reference-projection, shared hydration and Skald body-lowering surfaces, then
expanded into their existing callers and persistence/lifecycle owners. This
is a domain review, not a substitute for the other full-task reviewers.

| Boundary | Authority | Source and consumer coverage | Assessment |
|---|---|---|---|
| Authored intent → preflight → server version resolution | `wyrd-design.md` defaults schema, especially lines 1730–1737; REQ-028; replacement task preservation of existing composite registration | Cards route → `register_card` → `validate_request` / `plan_registration_graph` / `resolve_external` → `EffectiveSpecs::validate_workflows`; `graph_ready_submissions`; `WorkflowCard::from_envelope`; `persist_node` / `resolve_existing` | FAIL: preflight now requires a root version pin before server version resolution |
| Effective body provenance | Reference-slot doctrine; REQ-056/059; FIND-TASK-002-1 | `validate_and_collect_refs`, `EffectiveSpecs::{new,load,body,validate_bindings,validate_baselines,validate_workflows}`, `InlineableRef::to_durable`; binding/Drift sibling consumers; client `WorkflowBodies::body` | PASS: sibling and external inputs use separate stores; external lookup uses resolved registry UID |
| Validated external UID → bound/persisted UID | REQ-028/059; FIND-TASK-002-4 | `resolve_external`, `write_registration`, `recheck_active_card_refs`, `bind_card_references`, `persist_node`, outbound relationship SQL | PASS: recheck receives saved pairs and selects exact identity + expected UID + Active status |
| Dependency lifecycle exclusion | AGENTS.md tenant/durability rules; existing registry lifecycle contract | `FOR SHARE` recheck; deletion's `SELECT ... FOR UPDATE`, inbound-reference check and status update; transaction ownership in `TenantConn` | PASS: dependency locks remain until registration commit/rollback and conflict with deletion/update |
| Atomicity, replay and failure | AGENTS.md and agent-rules transactional audit requirements; existing registration contract | `register_card`, `write_registration`, replay/wait ownership, operation insertion/commit, `persist_node`, `resolve_existing`, version-line lock, `BindingProjector`, upload handoff | PASS: changed validation precedes mutation; write-time rejection rolls back audit/bookkeeping/graph; upload lifecycle remains the existing post-commit seam |
| Declaration/runtime equivalence | REQ-028/057/059, Revision 12 no tool/secret resolution during registration | `card_body_dependencies`, `Workflow::{validate_card_bodies,from_card_bodies,from_card_with_agent_resolver}`, CardBodyResolver Agent/Prompt projections; client GraphTraversal runtime/bundle consumers | PASS: same Skald pure/resolved checks, intentional tool unbinding only during declarative validation |

Read AGENTS.md, agent rules, spec-driven-development, maintainer style and the
reference router; applied Wyrd design/doctrine and tenant/audit portions of
security posture. Bifrost mechanics are not changed by this boundary. No
CodeGraph index exists in the established subject, so navigation used rg and
source reads. Existing server/SQL manifests and relevant mise tasks were
inspected; no new persistence owner or dependency was proposed.

## Proposed finding

### REG-DUR-R2-001 — REGRESSION: Workflow preflight rejects server-owned version intents

- **Violated obligation:** `architecture/wyrd-design.md:1730` explicitly makes
  omitted, scoped and pinned versions three distinct register intents; the
  server owns their resolution. REQ-028 and TASK-002-cleanup preserve existing
  composite registration and add declarative Workflow validation, without
  authorizing a new pinned-only registration contract.
- **Exact changed location:**
  `crates/wyrd/wyrd-server/src/components/cards/resolve.rs:430`.
- **Producer-to-consumer evidence:** a valid Workflow submission with inline
  Agent/Prompt bodies and `metadata.version` omitted, or a scoped version such
  as `"1"`, passes `validate_request`. `plan_registration_graph` already uses
  `graph_ready_submissions` (`wyrd-spec/src/graph/mod.rs:86`) to give non-pin
  intents a graph-only placeholder while preserving authored submissions.
  `resolve_external` passes the original submission to new Workflow preflight.
  Line 430 converts that original envelope through
  `WorkflowCard::from_envelope`, whose `resolved_pin()` requirement at
  `wyrd-spec/src/card/workflow.rs:1004` returns "Workflow Card envelope missing
  resolved version pin". Registration therefore never reaches `persist_node`
  and `resolve_existing` (`service.rs:1355`), which implement the ordinary
  `None`/`Scope`/`Pin` branches under the version-line lock. External or pinned
  sibling Agent references do not change this root-metadata failure.
- **Reachability:** the public Cards HTTP registration route calls
  `register_card` directly with typed `CreateCardRequest`; no loader or route
  requirement forces a Workflow root pin. A single Workflow containing valid
  inline bodies avoids the existing, unrelated exact-version requirement for
  path-reference targets. The original base has no call to Workflow preflight,
  and retains the same server version resolver.
- **Observable consequence:** valid auto-bump and prefix-line Workflow
  registrations that previously reached the normal version resolver now fail
  with Workflow validation before any Card is registered. The change affects
  every client entering the common registration route. Exact-version
  registered loading remains a separate, valid requirement.
- **Smallest testable correction:** keep version resolution and its locking on
  the existing server owner. Declarative preflight must validate the Workflow
  spec/bodies without requiring an already-resolved root version; reuse the
  existing graph-only version projection for the transient validation view,
  while retaining the original authored version intent for hashing/replay and
  persistence. Do not fill the submitted metadata with a default pin or move
  durable version allocation into preflight. Do not relax exact dependency
  identities, registered-load selectors, provenance, or the UID fence.
- **Closure proof:** extend the existing real registration journey with a valid
  Workflow root using omitted and scoped versions, keeping dependencies inline
  or exactly pinned. Assert registration returns the server-resolved exact
  version and reloading succeeds; prove an invalid resolved binding still
  refuses with no operation/Card/relationship write for these intents. Run the
  exact existing target through the repository-managed Postgres wrapper, plus
  existing UID-replacement and lifecycle-lock regression tests.

## Prior finding closure and proof assessment

FIND-TASK-002-1's server defect is closed at the effective-body source rather
than by a consumer guard. `Ref` loads only cached external registry specs;
`Sibling` loads only submitted specs. The collision registration journey checks
an incompatible registered Agent cannot be replaced by the submitted body.
The same change protects verifier/trigger/operator and Drift baseline sibling
consumers rather than special-casing Workflow only.

FIND-TASK-002-4 is closed: saved `(CardRef, CardUid)` pairs are not reduced to
identity-only values before recheck. SQL selects the expected UID, retains it
in the result, and holds `FOR SHARE` through bindings, relationships and commit.
The stale-preflight journey deliberately blocks registration on the audit head,
replaces the validated UID, and asserts refusal without operation, Workflow or
replacement inbound relationship. Its administrative SQL interleaving is
appropriate fault injection; ordinary public deletion remains protected by
inbound relationships. The SQL lifecycle test additionally proves a competing
status update gets lock timeout while registration holds its dependency lock.

The transitive Prompt closure feeds the same resolved-pair list into recheck;
registered body immutability and existing outbound deletion protection preserve
the validated Agent/Prompt graph. Client registered traversal uses locked
relationships and UID-bearing exact reads, checks loaded identity and Active
state, and publishes only a completely hydrated Workflow. Existing Service
bundle hydration retains the broader traversal scope and artifact handling.

## Verification limits

This review performed static source/caller/diff analysis only and started no
build or Postgres lane, to avoid competing with the orchestrator's verification.
The task supplies reported PASS results, not raw logs. I inspected the actual
three `pg_workflow_registration` tests and SQL lifecycle-lock test and their
assertions; their pinned-root fixtures do not exercise the proposed regression.
`test:cards:integration` does not itself select the new Workflow target, so the
reported exact target executions are necessary additional evidence.

The proposed failure is determined by the explicit `resolved_pin()` branch,
not inferred from a timeout or panic. No runtime reproduction is claimed.
No source, tests, other reports, generated files or candidate identity were
changed during this review.
