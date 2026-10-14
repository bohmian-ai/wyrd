# Focused follow-up review

## Immutable subject and scope

- Base: `ef03f624a1f3a6bcd51bbd17060271f0b5e8cabf`
- Candidate: `c4ba2b0502f8968a78e5e960bcc9eba43e960643`
- Authority: `changes/active/example-journey-fixes/spec.md` revision 4,
  binding `owner-decisions.md` OD-1 through OD-7, `AGENTS.md`,
  `architecture/agent-rules.md`, and the applicable architecture references
- Scope: only the five conflicts routed to this follow-up. This is not a vote
  on the complete change.

I inspected the complete cumulative diff inventory and traced each disputed
path through its current producers, callers, durable sinks, and focused tests.
The candidate remained `c4ba2b0502f8968a78e5e960bcc9eba43e960643`.

## A. Jev fallible-method rustdoc

### Source path inspected

- `crates/skald/skald-providers/src/clients/jev.rs:100-149`
- `crates/vala/vala-eval/tests/integration/orchestrator_judge_skald.rs:433-467`
- `crates/skald/skald-providers/src/trait_.rs:24-31`
- `crates/skald/skald-runtime/src/provider.rs:18-65`
- `AGENTS.md:709-727`
- `architecture/agent-rules.md:32`

### Evidence and resolution

`JevClient::send` is a new implementation of the fallible `ProviderClient`
boundary. At `clients/jev.rs:136-138` it returns
`ProviderResult<ProviderResponse>` and forwards all documented failures of
`send_native`, but its rustdoc contains no `# Errors` section. The new
`JevProvider` integration-test double has two more changed fallible methods:
`send` at `orchestrator_judge_skald.rs:442-457` has no `# Errors` section and
can panic at the poisoned mutex and typed-answer `expect` sites; `stream` at
`:460-463` deliberately returns `ProviderError::Timeout` but has no `# Errors`
section. The first method therefore also lacks the required `# Panics`
section. Trait documentation does not waive item-local documentation: AGENTS
§16 expressly covers methods and test helpers regardless of visibility and
requires `# Errors` on every fallible function or method and `# Panics` when a
panic remains. It labels missing rustdoc a hard blocker.

**Resolution: MNT-001 is confirmed as a `VIOLATION`.** The standards report's
blanket rustdoc pass missed these three changed items. The bounded correction
is documentation on those items only: name forwarded `send_native` failures,
state that the test sender has no ordinary error outcome, state the deliberate
stream timeout, and document the sender's two panic invariants. No code or new
test is needed.

## B. Direct execution without an active SYSTEM principal

### Source path inspected

- `crates/wyrd/wyrd-server/src/components/verification/routes.rs:229-256`
- `crates/wyrd/wyrd-server/src/mcp/verification.rs:174-193`
- `crates/wyrd/wyrd-server/src/components/verification/service.rs:417-511`
- `crates/wyrd/wyrd-server/src/components/verification/service.rs:513-638`
- `crates/wyrd/wyrd-server/src/verification/direct.rs:51-159`
- `crates/wyrd/wyrd-server/src/scribe_outbox.rs:403-443`
- `crates/shared/wyrd-client/src/observe/mod.rs:462-484`

### Evidence and resolution

HTTP, MCP, and the SDK all reach `VerificationControl::execute`. After
authorization and tenant-local Card resolution, `resolve_direct` reads the
active SYSTEM principal as an `Option` (`service.rs:618-631`). Absence does not
refuse the invocation. The engine executes and can make its attributed judge
calls (`:457-489`), then `stage_result` returns early when the optional SYSTEM
principal is absent (`:535-537`). The method nevertheless returns the completed
`Judgment` to HTTP/MCP (`:501-510`). Consequently the authenticated caller's
`requested_by_principal_id`, which the payload would otherwise set at
`:563-579`, is never retained because there is no result row at all.

The SYSTEM identity here is not the execution actor. It is the existing
server-internal Scribe admission identity: result frames are deliberately
submitted under the tenant SYSTEM principal with Verifier Card scope
(`scribe_outbox.rs:403-435`). The result payload separately records the direct
actor as `caller.principal.id`. Substituting that caller into the SYSTEM frame
would falsely reclassify the authenticated principal and is not a valid fix.

OD-1's nullable-requester exception is explicitly limited to internally
started scheduled or observation work when SYSTEM is not guaranteed. Direct
execution is manual/authenticated work, and both OD-1 and REQ-004 require its
authenticated caller to be recorded. The owner's known exception in the
review request likewise names an **internal run**, not direct execution.

**Resolution: BEH-001 is confirmed, classified `INCORRECT`.** The reachable
failure is narrower than “null requester”: a successful direct execution and
its judge calls can be returned while the entire verification result is
dropped. The smallest existing-boundary correction is to make the required
SYSTEM result-writer identity a pre-execution readiness condition for direct
execution, so this state refuses before provider/scorer work rather than
returning an unrecorded judgment. It must use the existing SYSTEM lookup and a
current stable server-unavailable/internal error; it must not borrow the caller
as a SYSTEM writer or add a second result path. Focused proof should remove or
withhold SYSTEM, call the public direct endpoint, and establish that no engine
or provider work occurs and no successful judgment is returned.

## C. Historical verifier-run requester migration

### Source path inspected

- `crates/wyrd/wyrd-sql/migrations/20260601000028_system_principal.sql:35-42,89-107`
- `crates/wyrd/wyrd-sql/migrations/20260601000029_verifier_runs.sql:81-100`
- `crates/wyrd/wyrd-sql/migrations/20261014000001_verifier_run_requester_every_origin.sql:1-30`
- `crates/wyrd/wyrd-sql/src/queries/verifier_runs.rs:1003-1012,1582-1637`
- `crates/wyrd/wyrd-sql/tests/integration/pg_verifier_runs.rs:1039-1115,1815-1886`
- `changes/active/example-journey-fixes/tasks/TASK-004-gateway-attribution-and-trace.md:40-60,101-117`

### Evidence and resolution

The original table constraint requires every pre-change scheduled and
observation row to have a null requester (`20260601000029...sql:98-100`). The
candidate migration only drops that constraint and adds “manual requires a
requester” (`20261014000001...sql:12-30`); it does not update existing rows.
TASK-004 records this exact limit at lines 57-58. New enqueue paths do select
SYSTEM, and their tests prove only post-migration inserts.

This is not blocked on a new persistent-data decision. The existing SYSTEM
principal migration creates a unique, always-active SYSTEM identity for every
existing ordinary data tenant (`20260601000028...sql:35-42,89-107`). OD-1
already chooses that exact identity as the actor for both internal origins and
allows null only where such an identity is genuinely absent. Thus the actor of
the old rows is deterministically resolvable from existing durable authority;
no new column, attribution rule, or compatibility contract is needed.

The review request excludes only the separate pre-release evolution gap for
the built-in `vala.verification.results` table. It does not exempt durable
`wyrd.verifier_runs`, and REQ-004/OD-1 say every scheduled and
observation-triggered run records SYSTEM. “A public release” being a non-goal
does not authorize two attribution regimes in a migration that explicitly
supports an existing table.

**Resolution: DATA-001 is confirmed as `MISSING`.** Backfill null requesters
for existing `schedule` and `observation` rows from the same tenant's existing
SYSTEM principal before installing the relaxed constraint, leaving null only
when no SYSTEM row is resolvable. A migration test must seed the old shapes,
apply the migration, and prove same-tenant SYSTEM attribution while preserving
manual requesters and the owner-approved no-SYSTEM exception.

## D. Jev 401/403 upstream-body relay and credential secrecy

### Source path inspected

- `crates/wyrd/wyrd-gateway/src/adapter/http.rs:420-500`
- `crates/wyrd/wyrd-gateway/src/adapter/mod.rs:298-409`
- `crates/wyrd/wyrd-gateway/src/adapter/tests.rs:1659-1701,2971-3030,3623-3685`
- `architecture/wyrd-security-posture.md:9-27`
- spec INV-002 and binding OD-4

### Evidence and resolution

The Jev-specific mapper sends 401/403 through shared `failure` and `refusal`
(`http.rs:471-480`). That path relays provider-controlled body strings after
removing literal secret occurrences and suppresses the body only when a whole
string or data URL decodes under standard base64 to bytes containing the
secret (`adapter/mod.rs:314-348,381-408`). Tests cover literal and those exact
standard-base64 shapes. A Jev upstream that received the bearer credential can
return an equivalent percent-encoded, hexadecimal, URL-safe-base64, embedded,
or split representation; the shared relay returns it unchanged to the model
caller. This is a reachable disclosure boundary, not a hypothetical dormant
surface. The candidate newly routes Jev authentication refusals through it,
and INV-002 plus the normative security posture prohibit secrets in returned
errors.

No finite encoding blacklist can prove the absolute secrecy invariant against
an upstream-controlled opaque string. Extending `encodes_secret` is therefore
not a bounded correction. Conversely, replacing the provider body with a
Wyrd-owned fixed envelope would satisfy INV-002 but contradict OD-4's explicit
decision that the upstream 401/403 **body is relayed**. Even an allowlisted
provider string remains an attacker-controlled carrier unless the owner
defines a projection whose values cannot carry such content.

**Resolution: SEC-001 is a confirmed `VIOLATION`, and its correction requires
owner reconciliation of OD-4 with INV-002.** It is not speculative, but it is
also not safely remediable inside the currently approved behavior. The final
review should route this conflict as `SPEC_REVISION_REQUIRED`, not prescribe
another scrubber or silently stop relaying the body. OD-4 must choose which
bounded response content, if any, may cross this trust boundary while keeping
the non-retry and status behavior.

## E. Malformed `tracestate`

### Source path inspected

- `crates/wyrd/wyrd-server/src/http/middleware/trace_context.rs:52-129`
- `crates/wyrd-spec/src/trace.rs:19-45`
- `crates/wyrd/wyrd-server/src/components/gateway/capture.rs:358-382,850-878`
- W3C Trace Context, sections 3.3.1 through 3.3.3:
  `https://www.w3.org/TR/trace-context/`

### Evidence and resolution

`inbound` validates only one `traceparent`; it concatenates every
string-decodable `tracestate` header and passes the result to
`TraceContext::parse_traceparent` (`trace_context.rs:57-70`). That parser
validates only `traceparent` and copies `trace_state` without inspection
(`trace.rs:24-44`). Gateway capture then writes the value to retained OTLP spans.
The only malformed-header tests cover malformed, zero, repeated, or absent
`traceparent` (`trace_context.rs:115-129`).

The W3C grammar is substantive, not “visible ASCII”: a list has at most 32
members; keys and values have restricted grammar and length; values exclude
`,` and `=` and cannot end in a blank; and keys must be unique. Multiple header
fields are combined in order. Crucially, invalid `tracestate` must not invalidate
a valid `traceparent`; the state is ignored while the parent remains usable.
The current code instead accepts and persists malformed state. REQ-005 says it
honors W3C `traceparent` **and `tracestate`** and that malformed headers are
ignored, so that clause includes malformed `tracestate`.

**Resolution: SEC-002 is confirmed as `INCORRECT`.** Validate the combined
state at the inbound owner, retain the valid parent while dropping invalid
state, and prove invalid key/value grammar, duplicate keys, over-32 members,
and per-member length bounds plus a valid repeated-header combination. The
W3C 512-character propagation floor is a `SHOULD`, not itself a mandatory
malformed-header threshold, so it should not be invented as a hard rejection
without a documented repository limit.

## Shared-cause assessment

B and C enforce the same approved actor invariant but have different sources
and correction boundaries: B is pre-execution Scribe-writer readiness for a
new direct result; C is migration of already-durable queue rows. Combining
them into downstream guards would hide both defects. A is documentation-only,
D is an unresolved security-contract conflict, and E is ingress standards
validation; none shares a correction owner with another item.

## Proposed material finding ledger for independent validation

| Source ID | Classification | Resolution |
|---|---|---|
| MNT-001 | VIOLATION | Confirmed: three changed fallible Jev methods violate hard AGENTS §16 rustdoc requirements. |
| BEH-001 | INCORRECT | Confirmed: a reachable direct execution can return success while dropping its result and authenticated requester. |
| DATA-001 | MISSING | Confirmed: the migration leaves resolvable historical internal-run actors null; no new persistent-data decision is needed. |
| SEC-001 | VIOLATION | Confirmed reachable leak, but approved OD-4 and INV-002 conflict; correction requires specification revision. |
| SEC-002 | INCORRECT | Confirmed: malformed W3C `tracestate` is accepted and retained instead of ignored independently of a valid parent. |

**RESOLVED**
