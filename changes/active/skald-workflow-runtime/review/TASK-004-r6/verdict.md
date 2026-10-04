# TASK-004 R6 Review Verdict

## Immutable subject

- Repository: `/home/thorrester/Documents/GitHub/wyrd`
- Base: `b90335e0991438e8c28b65ed9c0c4cd80f9b0d56`
- Candidate: `5f3b521b5005c26277d53e7dfd2458c4f740e8be`
- Approved specification:
  `changes/active/skald-workflow-runtime/spec.md`, Revision 14
- Original task:
  `changes/active/skald-workflow-runtime/tasks/TASK-004-accepted-server-jobs.md`
- Remediations: TASK-004 R1 through R5 in their preceding review directories

The candidate remained unchanged throughout discovery, follow-up, validation,
and verdict preparation. `.codegraph/` is absent, so the review used immutable
Git objects, repository search, and direct source/caller inspection. The
reviewed source was not modified.

## Independent review results

| Review | Result | Material result |
|---|---|---|
| Behavior | FAIL | `BEHAVIOR-R6-001`: active TASK-004/R5 authority still targets Revision 13 and retains live text contradicted by Revision 14. |
| Invariants | FAIL | `INV-R6-001`: the same active-authority regression, independently traced. |
| Repository standards | FAIL | `STANDARDS-R6-001`: task metadata/current-authority text does not identify approved Revision 14. |
| Maintainer | FAIL | `MAINT-R6-1` and `MAINT-R6-2`: false schema/destination documentation and missing required construction-seam rustdoc. |
| System resilience | PASS | Accepted-run, gateway, query, shutdown, restart, and dependency-failure paths remain bounded and recover as approved. |
| Concurrency/lifecycle domain | PASS | Preparation, acceptance, attempts, cancellation, deadlines, retention, shutdown, and grant-stream-close release pass. |
| Security/tenancy domain | PASS | Authentication, audit, captured authority, tenant isolation, secrets, SSRF, and nondisclosure pass. |
| Query-settlement domain | PASS | Deadline precedence, tracked settlement, pod loss, participant release, and later serviceability pass. |
| Provider-contract domain | FAIL | `DOMAIN-PROVIDER-R6-1` and `DOMAIN-PROVIDER-R6-2`: Vertex media errors lose destination attribution and cache keys lose custom-provider isolation. |
| Focused follow-up | RESOLVED | Confirmed the two provider and two documentation paths and narrowed their distinct correction boundaries. |
| Ponytail validation | COMPLETE | Reopened `FIND-TASK-004-17` and retained `FIND-TASK-004-24` through `FIND-TASK-004-27`. |

All required reports are present. No required reviewer was unavailable.

## Reconciled acceptance matrix

| Obligation | Result | Reconciled evidence |
|---|---|---|
| Tracked preparation, scoped idempotency, bounded admission, retention, and shared-deadline shutdown | PASS | Prior `FIND-TASK-004-1`, `-2`, `-9`, `-11`, and `-13` remain closed. |
| Query opening, cancellation, settlement, owner loss, capacity recovery, and sibling serviceability | PASS | Prior `FIND-TASK-004-3`, `-4`, `-10`, `-21`, and `-22` remain closed. |
| Tenant isolation, authorization, audit, secrets, and accepted authority independent of bearer lifetime | PASS | Prior `FIND-TASK-004-5`, `-8`, and `-14` remain closed. |
| Built-in tools, exact schemas/bounds, denials, and trustworthy terminal handling | PASS | Prior `FIND-TASK-004-6` and `-7` remain closed. |
| Gateway fallback, cancellation, routing, persistence, and direct external ownership | PASS | Prior `FIND-TASK-004-12`, `-15`, and `-23` remain closed under Revision 14. |
| Grant-stream-close follower release with no polling, idle refusal, or leader acknowledgement | PASS | Prior `FIND-TASK-004-18` remains closed and the fixed human decision is preserved. |
| Published `Running` reserves attempt one; interrupted published work settles `Cancelled` | PASS | Prior `FIND-TASK-004-19` remains closed. |
| Revision 14 uses one request variant per wire schema, optional `Prompt.provider`, Vertex as a GenerateContent body targeted to Vertex, and unchanged local Vertex refusal | PASS | Request, Prompt, persistence, dispatch, gateway, Python, schema, fixture, and recorded journey evidence agree. |
| Active TASK-004/R5 authority unambiguously targets Revision 14 | **FAIL** | `FIND-TASK-004-17` is reopened: both active tasks identify Revision 13 and R5 retains mutually exclusive compatible-variant requirements. |
| Provider-bearing Prompt errors preserve the effective destination after the GenerateContent schema fold | **FAIL** | `FIND-TASK-004-24`: a reachable Vertex media refusal reports Google. |
| Provider-scoped cache keys remain isolated after Chat destination moves to `Prompt.provider` | **FAIL** | `FIND-TASK-004-25`: request-only key derivation necessarily scopes custom Chat requests as OpenAI. |
| Materially changed Rust documentation accurately distinguishes schema defaults from effective destinations | **FAIL** | `FIND-TASK-004-26`: response and Agent telemetry rustdoc state the opposite contract. |
| Materially changed fallible Prompt construction items document errors and destination preservation | **FAIL** | `FIND-TASK-004-27`: four changed construction items violate the repository's hard rustdoc rule. |
| Non-goals and standing human decisions remain excluded | PASS | No durable queue, compatibility variant, migration reader, polling/acknowledgement mechanism, second deadline owner, new harness, check, setting, or option entered the candidate or is required by remediation. |

## Follow-up decision

A focused follow-up was required because the provider-domain findings exposed
reachable Revision 14 consumers that the general behavior and invariant
reviews had treated as conforming, and because the maintainer findings required
materiality checks. It traced both provider paths and all four documented
symbols through their callers. It resolved all proposals: the media, cache,
response/telemetry documentation, and construction documentation claims are
reachable and material, share the Revision 14 destination move, and require
three distinct correction boundaries. No further discovery pass was needed.

## Validated finding ledger

- `FIND-TASK-004-17` — **REVISED / REOPENED / VIOLATION**: active TASK-004
  and R5 authority identifies Revision 13 and R5 retains live compatible-
  variant instructions contradicted by approved Revision 14. Align only current
  task authority and acceptance text; preserve immutable historical review
  subjects and the still-valid deadline proof.
- `FIND-TASK-004-24` — **CONFIRMED / REGRESSION**: the shared
  GenerateContent media path hard-codes Google in the stable unsupported-media
  error even when the Prompt's effective destination is Vertex. Reuse
  `Prompt::provider()` at the existing media owner.
- `FIND-TASK-004-25` — **REVISED / REGRESSION**: request-only cache-key
  derivation cannot observe the destination now owned by `Prompt`, so custom
  Chat endpoints and OpenAI can collide. Replace the unsafe derivation boundary
  with one that receives the existing effective destination; do not retain a
  second request-only alternative.
- `FIND-TASK-004-26` — **CONFIRMED / VIOLATION**: two changed rustdoc blocks
  falsely conflate a response/request schema default with the effective native
  destination. Correct documentation only.
- `FIND-TASK-004-27` — **CONFIRMED / VIOLATION**: four materially changed
  fallible Prompt construction items omit required `# Errors` and destination-
  preservation documentation. Add substantive rustdoc only.

No retained correction requires a product, public wire, architecture,
security, compatibility, cross-service, concurrency, resource-ownership,
persistent-data, or deployment decision. The cache change replaces one
unpublished workspace Rust operation made insufficient by Revision 14; it does
not add a parallel API or compatibility promise.

## Prior-finding closure

`FIND-TASK-004-1` through `-16` and `FIND-TASK-004-18` through `-23` remain
closed. `FIND-TASK-004-17` is reopened because the same authority-chain
invariant regressed when Revision 14 was approved and assigned to TASK-004.
The R5 runtime closures remain intact: the forwarded Oracle journey now proves
omitted, longer, and shorter positive query deadlines, and the single OpenAI
Chat schema with a custom Prompt destination reaches the existing external
Chat route.

The fixed human decisions remain intact: supervisor drain polling and idle
refusal stay deleted; follower release is grant-stream close with no leader
acknowledgement; the foreign-tenant harness remains unchanged; published
`Running` reserves attempt one and interrupted work settles `Cancelled`; built-
in query tools consume the one-time-bound prepared deadline and a shorter
positive `deadline_ms` wins; and the local Wyrd client keeps its prior Vertex
refusal.

## Verification limits

Per the review instruction, no build, test, Cargo, `mise`, formatter, linter,
code-generation, or package-manager command was run. Recorded evidence was
accepted only where its current named assertions exercise the claimed path.
The R5 final-tree evidence covers the broad Skald, Wyrd, shared, gateway,
Python, TypeScript, codegen, and Oracle lanes, but it does not exercise the
Vertex unsupported-media error or destination-distinct cache keys. The two
documentation findings and the authority conflict are established directly
from current source and approved authority.

## Verdict

**FIX_REQUIRED**

The independently validated ledger contains five bounded findings. Remediation
is specified in
`TASK-004-R6-close-revision-14-consumer-and-authority-gaps.md`.
