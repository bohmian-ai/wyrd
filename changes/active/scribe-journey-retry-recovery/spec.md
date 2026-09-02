---
id: SPEC-scribe-journey-retry-recovery
revision: 1
status: approved
---

# Scribe journey and staged-member retry recovery

## Human intent and user value

The production Scribe journey must distinguish a real private-peer dial failure
from an invalid test fixture, and Scribe must recover when persistence fails
after a member has already reached durable local staging.

A caller whose write was acknowledged must retain exactly-once query visibility
through a failed flush, a retry, and a restart. Operators must not have to delete
durable directories by hand, weaken the physical qualification journey, or
accept silent data loss to recover the shard.

## Scope

- Correct the Scribe private-peer negative journey so it uses a valid HTTPS
  endpoint and reaches the intended transport failure and public error.
- Define retry and restart behavior when an exact member already has local
  durable evidence from an earlier persistence attempt.
- Preserve acknowledged-row visibility and exactly-once authority while Scribe
  reconciles absent, incomplete, valid, terminal, or contradictory local member
  state.
- Prove the post-staging failure boundary deterministically with the existing
  Scribe fault-injection and durable recovery facilities.
- Keep the existing 512 MiB physical-object qualification as the production
  geometry gate.

## Non-goals

- A new Scribe lifecycle, durable record format, retry framework, storage
  engine, or public recovery API.
- A new public request, response, error, CLI, MCP, SDK, schema, or configuration
  contract.
- Automatic retry of arbitrary failures whose durable identity cannot be
  established safely.
- Overwriting, adopting, or deleting an arbitrary existing member directory.
- Reducing the 512 MiB target, changing production sizing geometry, adding
  sleeps or test-only retries, or serializing the lane to conceal the defect.
- Restoring retired Bifrost benches or moving production-harness journeys back
  into a Bifrost module.
- Changing Oracle or Forge behavior beyond consuming the existing Scribe
  visibility contract.

## Definitions

- **Exact member:** the one Scribe member identified by all identity and source
  boundaries already recorded by the current WAL, shard generation, staging,
  and lifecycle authorities. A partial identity match is not exact.
- **Final durable member record:** the existing fsynced and validated local
  record that makes a staged member eligible for query registration or a later
  lifecycle transition.
- **Incomplete residue:** local staging output for which no valid final durable
  member record exists and whose ownership can be proven to belong to the exact
  retrying member.
- **Valid staged evidence:** an exact, internally consistent durable member
  record and its referenced files, digests, identity, and lifecycle state.
- **Contradictory evidence:** an existing directory or durable record whose
  identity, state, files, or digests do not prove it is the exact expected
  member.
- **Post-staging failure:** a persistence failure after the final durable member
  record exists but before the complete manifest/publication workflow settles.
- **Exactly-once visibility:** acknowledged rows remain continuously readable
  once each across live, staged, and published authority without a gap or
  duplicate caused by recovery.

## Required behavior

### REQ-001 — The private-peer journey reaches the intended failure

The Scribe private-peer negative journey shall begin with a reachable advertised
Scribe endpoint, replace it with a syntactically valid but unreachable HTTPS
endpoint, refresh the registry view, and exercise the production client-to-server
query path.

The journey shall fail at peer reachability, not during fixture construction or
URI scheme validation, and the public strict fused query shall terminate with
`WYRD_VALA_503_QUERY_VISIBILITY_UNAVAILABLE`.

### REQ-002 — Retry classifies existing local evidence before staging

Before creating local output for an exact member, Scribe shall classify any
existing durable directory and record through the current staging and lifecycle
authority.

- When no evidence exists, normal staging may begin.
- When only proven incomplete residue exists, Scribe may remove that residue
  and restage from the authoritative WAL-backed source.
- When valid staged evidence exists, Scribe shall reuse that exact member and
  resume the remaining lifecycle work without re-encoding it.
- When the exact member is already in a later or terminal lifecycle state,
  Scribe shall reconcile that state instead of creating a second member.
- When evidence is contradictory, corrupt, ambiguous, or belongs to another
  identity, Scribe shall fail closed.

### REQ-003 — A post-staging failure preserves progress

If persistence fails after an exact member has a valid final durable member
record, completion of that failed attempt shall preserve enough existing
authority for an in-process retry and a process restart to recognize and resume
that member.

The failed operation shall still report its real failure to the caller. Scribe
shall not report success merely because local staging completed.

### REQ-004 — Pre-registration failure retains the authoritative source

If persistence fails before a valid final durable member record exists, the
WAL-backed immutable source shall remain authoritative. Incomplete residue shall
not be query-visible, registered as ready, or treated as proof that staging
completed.

Scribe may retry from that source only after proving that any removed residue is
both incomplete and owned by the exact retrying member.

### REQ-005 — Recovery is fail-closed for invalid evidence

Scribe shall not overwrite, silently delete, serve, publish, or adopt
contradictory evidence. It shall retain the WAL and available local evidence,
emit the existing structured operational failure and diagnostics, and require
reconciliation before advancing authority.

Tenant, table, schema, layout, partition, node, epoch, shard, generation, and
source-range authority shall never be widened to make an existing directory
appear reusable.

### REQ-006 — Acknowledged rows remain visible exactly once

Across a post-staging failure and successful retry, every acknowledged row shall
remain query-visible exactly once. Recovery shall not create a second durable
member, duplicate a physical object, repeat a publication transition, repeat a
logical audit transition, or expose both old and recovered authority as separate
rows.

WAL retirement and source cleanup shall occur only after every affected member
has a valid durable replacement and the existing lifecycle permits retirement.

### REQ-007 — Live retry and restart recovery agree

An in-process retry and startup recovery shall make the same authority decision
for the same durable evidence. Restart shall not be required for an otherwise
safe retry, and restarting shall not turn contradictory evidence into valid
evidence.

Both paths shall converge valid staged, published, or cleanup-pending evidence
through the existing lifecycle rather than a second recovery state machine.

### REQ-008 — Physical qualification remains unchanged

The Scribe qualification journey shall continue to prove the production 512 MiB
physical-object geometry, required residue behavior, and exact row results. This
change shall address the reachable recovery defect without weakening that
journey or its limits.

## Invariants and prohibited outcomes

- A valid existing durable member directory is never overwritten.
- Contradictory or corrupt durable evidence is never deleted automatically.
- A member is never re-encoded after its exact final durable record has been
  validated.
- A shared-cohort WAL is never retired before all required members have valid
  durable replacements and query authority.
- Recovery never broadens tenant or member identity and never crosses a source
  range.
- Retry and restart never double-register, double-publish, double-audit, or
  double-serve acknowledged rows.
- A failed manifest or publication transition is never converted into false
  success.
- The private-peer journey never uses an invalid cleartext endpoint to simulate
  an HTTPS transport failure.
- The physical qualification gate is not weakened to make the lane green.

## Externally observable behavior and failure modes

- A strict fused query whose refreshed Scribe peer is unreachable through a
  valid HTTPS address terminates with
  `WYRD_VALA_503_QUERY_VISIBILITY_UNAVAILABLE`.
- A flush that encounters the injected post-staging failure returns a failure;
  acknowledged rows remain readable exactly once.
- A later flush or restart with valid exact staged evidence resumes and
  converges without a directory-collision failure.
- Proven incomplete residue may be replaced from the retained authoritative
  source without becoming visible prematurely.
- Contradictory or corrupt evidence produces a fail-closed operational error;
  the authoritative WAL and diagnostic evidence remain available.
- Successful recovery leaves one settled ownership chain and no duplicate
  logical object, publication, audit transition, or query row.

## Material constraints

- Reuse the existing WAL, durable member record, lifecycle, recovery, and
  fault-injection authority. Do not add a parallel model for retry progress.
- Keep the existing refusal to overwrite an occupied durable member directory;
  recovery must decide whether to reuse, safely replace, or reject before the
  encoder reaches that guard.
- Preserve existing public and persisted contracts. Any discovery that a
  contract or durable schema change is necessary requires a new specification
  revision and explicit approval.
- Add no dependency or Cargo feature for this change.
- Use repository-managed Postgres and the production `WyrdTestServer` harness
  for the Tier-1 journey evidence.
- Keep production error handling, validation, tenant isolation, durability, and
  audit guarantees intact; test-only shortcuts may not enter production paths.

## Required system boundaries and cross-boundary flow

1. The production harness starts the real server and obtains the advertised
   HTTPS Scribe endpoint through the existing registry path.
2. The negative journey replaces that endpoint with a valid unreachable HTTPS
   address, refreshes registry authority, and queries through the real client
   surface to the typed visibility terminal.
3. For persistence recovery, Scribe accepts rows into its existing WAL-backed
   source and stages the exact member through the existing durable record path.
4. A deterministic fault after durable staging but before manifest/publication
   settlement causes the first flush to fail without discarding durable member
   authority or acknowledged-row visibility.
5. The next flush or startup recovery classifies the existing evidence, reuses
   the exact valid member, and resumes the existing manifest/publication
   lifecycle.
6. Query authority, audit, publication, WAL retirement, and cleanup settle once
   under their current owners.

No new public interface is required. Private implementation shape remains a
planning decision so long as all requirements and ownership boundaries above
hold.

## Acceptance obligations and evidence classes

### AC-001 — Valid HTTPS reachability failure

A Tier-1 Scribe journey proves a reachable initial peer, a refreshed valid but
unreachable HTTPS peer, and the exact public
`WYRD_VALA_503_QUERY_VISIBILITY_UNAVAILABLE` terminal without setup panic or URI
scheme rejection. Covers REQ-001.

### AC-002 — Deterministic post-staging retry

A deterministic Scribe test uses the existing post-staging fault boundary to
prove that the first flush fails, a second flush succeeds, and the retry does
not collide with or re-encode the existing exact durable member. Covers
REQ-002, REQ-003, and REQ-007.

### AC-003 — Exactly-once authority and visibility

Tier-1 journey evidence proves acknowledged rows remain readable exactly once
before and after retry and that durable member, physical object, publication,
audit, ownership, and cleanup identities settle once. Covers REQ-006.

### AC-004 — Incomplete and contradictory evidence

Focused supporting tests prove that owned incomplete residue can be safely
restaged from retained authority, while mismatched, corrupt, or ambiguous
evidence fails closed without overwrite, deletion, visibility, or WAL
retirement. Covers REQ-004 and REQ-005.

### AC-005 — Restart equivalence

Focused recovery evidence proves a restart reaches the same decision as a live
retry for valid staged and contradictory evidence. Covers REQ-005 and REQ-007.

### AC-006 — Unchanged production qualification

The existing 512 MiB Scribe qualification journey passes with its production
geometry, residue expectations, and exact row assertions unchanged. Covers
REQ-008.

### AC-007 — Focused and lane verification

Evidence includes exact focused commands for every named Rust test, the
repository-managed environment wrapper where required, the complete
`bifrost:journey:scribe` lane, the nearest Bifrost crate/family task, formatting,
linting, and clean generated-contract checks if contract surfaces unexpectedly
change.

## Open material decisions

None. Revision 1 is behaviorally decision-complete and awaits human approval.

## Planning-decision inventory

The later single remediation task shall decide only private implementation and
verification details that preserve this specification:

- the narrowest shared persistence seam at which existing member evidence is
  classified before encoding;
- how the current staged-member and startup-recovery facilities are reused so
  failed in-process completion retains recoverable progress without adding a
  durable state model;
- the precise existing signals that distinguish absent, incomplete, valid,
  terminal, and contradictory evidence;
- the minimum focused supporting tests needed in addition to the two Tier-1
  journeys;
- the exact existing ownership and audit assertions used to prove one settled
  outcome; and
- the focused `mise` commands and repository Postgres wrapper needed for the
  immutable implementation candidate.

These decisions may not weaken the requirements, introduce new public or
durable contracts, or expand the change beyond one cohesive remediation task.

## Revision history

- **Revision 1 — approved 2026-09-02:** Defines the valid-HTTPS Scribe journey correction and
  the existing-authority retry/restart contract for failures after durable local
  staging.

## Authority links

- `AGENTS.md`
- `architecture/agent-rules.md`
- `architecture/wyrd-design.md`
- `architecture/wyrd-doctrine.mdx`
- `architecture/bifrost-design.md`
- `architecture/references/domain/analytical-operations-reliability.md`
- `architecture/references/domain/olap-serving.md`
- `architecture/references/languages/spec-driven-development.md`
