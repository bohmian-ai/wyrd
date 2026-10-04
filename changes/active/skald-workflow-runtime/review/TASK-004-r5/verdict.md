# TASK-004 R5 Review Verdict

## Immutable subject

- Repository: `/home/thorrester/Documents/GitHub/wyrd`
- Base: `b90335e0991438e8c28b65ed9c0c4cd80f9b0d56`
- Candidate: `d4d4e2da53abfc677abdb804e71517c3b6849f49`
- Approved specification:
  `changes/active/skald-workflow-runtime/spec.md`, Revision 13
- Original task:
  `changes/active/skald-workflow-runtime/tasks/TASK-004-accepted-server-jobs.md`
- Remediations: TASK-004 R1 through R4 in their preceding review directories

The candidate remained unchanged throughout the review. `.codegraph/` is
absent, so reviewers used the immutable Git range, repository search, and
direct source inspection. The source was not modified.

## Independent review results

| Review | Result | Material result |
|---|---|---|
| Behavior | FAIL | `BEHAVIOR-R5-001`: R4 omitted its required positive explicit-deadline proof. |
| Invariants | FAIL | `INV-R5-001`: same proof gap, independently traced. |
| Repository standards | PASS | Complete authority coverage; no material repository-rule finding. |
| Maintainer | PASS | Complete changed-surface coverage; no material maintainer finding. |
| System resilience | PASS | Deadline binding preserves settlement, shutdown, and recovery ownership. |
| Concurrency/lifecycle domain | PASS | Reservation, promotion, attempt, cancellation, deadline, drain, and retention invariants pass. |
| Security/tenancy domain | PASS | Authorization, audit, captured authority, tenant isolation, secrets, and non-disclosure pass. |
| Query-settlement domain | PASS | Query ownership, terminal validation, pod loss, grant-stream close, and capacity recovery pass. |
| Provider-contract domain | FAIL | `DOMAIN-PROVIDER-1`: the reachable compatible OpenAI Chat variant is rejected by external routing and dispatch. |
| Focused follow-up | RESOLVED | Confirmed provider reachability and task relevance as `FOLLOWUP-PROVIDER-1`. |
| Ponytail validation | COMPLETE | Retained `FIND-TASK-004-22` and `FIND-TASK-004-23`; prior findings remain closed. |

All required reports are present. No required reviewer was unavailable.

## Reconciled acceptance matrix

| Obligation | Result | Reconciled evidence |
|---|---|---|
| Tracked preparation, scoped idempotency, bounded admission, retention, and shutdown | PASS | Prior `FIND-TASK-004-1`, `-2`, `-9`, `-11`, and `-13` remain closed. |
| Query opening, cancellation, settlement, owner loss, capacity recovery, and sibling serviceability | PASS | Prior `FIND-TASK-004-3`, `-4`, and `-10` remain closed. |
| Tenant isolation, authorization, audit, secrets, and accepted authority independent of bearer lifetime | PASS | Prior `FIND-TASK-004-5`, `-8`, and `-14` remain closed. |
| Built-in tool declarations, schemas, bounds, denials, and terminal handling | PASS | Prior `FIND-TASK-004-6` and `-7` remain closed. |
| Gateway fallback, cancellation, provider tagging, persistence, and Vertex projection | PASS | Prior `FIND-TASK-004-12` and `-15` remain closed. |
| Revision 13 metadata, import rules, and grant-stream-close Oracle documentation | PASS | Prior `FIND-TASK-004-16`, `-17`, `-18`, and `-20` remain closed. |
| Published `Running` reserves attempt one; interrupted published work settles `Cancelled` with timestamps | PASS | Prior `FIND-TASK-004-19` remains closed. |
| Every built-in query tool clone uses the exact prepared-run deadline through the approved one-time bind | PASS | Prior `FIND-TASK-004-21` is closed in production source. |
| R4 proves omitted, explicit longer, and explicit shorter positive query-deadline precedence through the actual tool/query path | **FAIL** | `FIND-TASK-004-22`. |
| A stored `OpenAiChatCompatible` Prompt can use `ExtGateway(protocol = openai_chat)` through validation and direct dispatch | **FAIL** | `FIND-TASK-004-23`. |
| Non-goals and standing human decisions remain excluded | PASS | No durable workflow service, new protocol, acknowledgement, polling mechanism, check, setting, option, compatibility layer, or foreign-tenant credential expansion was introduced or required. |

## Follow-up decision

A focused follow-up was required because the provider-domain report revealed a
reachable path not covered by the other discovery reports. It traced the
tagged compatible request from authored/stored Prompt through hydration, Agent
validation, route matching, provider registry dispatch, and external send.
The follow-up resolved that the path is required by Revision 13 `REQ-038` and
`REQ-039`, and confirmed that both rejection sites are one defect. No further
follow-up was needed: the two implementation reviewers already independently
agreed on the R4 proof gap, and Ponytail validation resolved both proposals
from source.

## Validated finding ledger

- `FIND-TASK-004-22` — **REVISED / MISSING**: R4's production deadline owner
  and one-time bind are correct, but its mandatory focused evidence never
  drives positive explicit shorter and longer `deadline_ms` values through
  `QueryTool` and Oracle. The minimum correction is to extend the existing
  forwarded-query journey; no production mechanism or new harness is needed.
- `FIND-TASK-004-23` — **CONFIRMED / INCORRECT**:
  `ProviderRequest::OpenAiChatCompatible` is an OpenAI Chat dialect accepted by
  the Agent loop, but `protocol_matches` rejects it for `OpenAiChat` and
  `ExternalGatewayClient::send` would reject it again. The minimum correction
  is to extend those two existing OpenAI Chat match arms and the existing
  server external-route journey.

No retained correction requires a product, public API, architecture,
security, compatibility, cross-service, concurrency, resource-ownership, or
persistent-data decision.

## Prior-finding closure

`FIND-TASK-004-1` through `FIND-TASK-004-21` remain source-closed. In
particular, `FIND-TASK-004-21` is not reopened: `PreparedWorkflowRun` owns the
one deadline and every `RunTools` clone receives it once before acceptance.
`FIND-TASK-004-22` concerns the separately required proof of the existing
positive deadline-precedence branches.

The standing human decisions remain fixed: Oracle graph-drain polling and
supervisor idle refusal stay deleted; follower release is grant-stream close
without a leader acknowledgement; the foreign-tenant fixture is not expanded
with gateway credentials; published `Running` reserves attempt one and
interrupted published work settles `Cancelled`; and built-in query tools use
the prepared deadline through a one-time bind.

## Verification limits

Per the review instruction, no build, test, Cargo, mise, formatter, linter,
code-generation, or package-manager command was run. Recorded evidence was
accepted only where the current named assertion actually exercises the claimed
behavior. This restriction does not block the verdict: both retained findings
are established from reachable source and the recorded evidence gap is itself
an explicit R4 acceptance failure.

## Verdict

**FIX_REQUIRED**

The validated ledger contains two bounded implementation findings. Remediation
is specified in `TASK-004-R5-close-compatible-route-and-deadline-proof-gaps.md`.
