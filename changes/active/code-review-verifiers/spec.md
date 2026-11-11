---
id: SPEC-code-review-verifiers
revision: 1
status: draft
---

# API-invoked code review through Wyrd Verifiers

## Problem and proposed behavior

Teams need to submit an exact code snapshot, optional review requirements, and
an intended review domain through an API, then receive a durable judgment with
findings, evidence, provenance, and a lifecycle that is safe to poll and retry.
The motivating API accepts one Python or SQL target, supporting files, one of
four specialist selectors or automatic routing, and caller-supplied
requirements. It persists an assignment and returns findings plus requirement
assessments.

Wyrd already owns the durable primitives this needs: exact Artifact and
Verifier Card versions, generic direct and queued Verifier execution, evidence
lineage, structured Agent checks, stable errors, tenant-scoped authorization,
and retained results. This change therefore adds code review as a reusable
Verifier configuration and result schema, not as another verification service.

A user publishes the target and supporting files together as one exact
Artifact Card version, chooses an exact code-review Verifier Card (or the exact
automatic-review Verifier), and calls the existing verification API. The
Artifact is both the subject being judged and the source collection supplied to
the Verifier. An optional JSON input carries caller requirements and identifies
the target path. Direct review returns a normal `VerificationResult`; queued
review returns an `execution_id` that the normal execution endpoint exposes.

For example, a tenant publishes this immutable Artifact version:

```text
checkout-review@1.0.0
|-- src/checkout.py       target
`-- src/authz.py          supporting context
```

It then enqueues the exact application-security reviewer:

```http
POST /v1/verification/enqueue
Authorization: Bearer <Wyrd access token>
Idempotency-Key: checkout-auth-v1
Content-Type: application/json

{
  "verifier_card_uid": "<exact code-review-application-security UID>",
  "card_uid": "<exact checkout-review Artifact UID>",
  "run_id": "example-run-17",
  "inputs": {
    "source": {
      "kind": "artifact",
      "artifact": {
        "kind": "Artifact",
        "name": "checkout-review",
        "version": "1.0.0",
        "uid": "<exact checkout-review Artifact UID>"
      }
    },
    "review_request": {
      "kind": "json",
      "value": {
        "schema_version": 1,
        "target_path": "src/checkout.py",
        "requirements": [
          {
            "id": "checkout-auth",
            "statement": "Only the account owner can complete checkout.",
            "name": "Checkout authorization",
            "rationale": "Prevent unauthorized purchases.",
            "version": "1"
          }
        ]
      }
    }
  }
}
```

This is intentionally not wire-compatible with the motivating API. Wyrd's
existing Cards, access-token flow, Verifier execution routes, result envelope,
status vocabulary, Problem Details errors, and `Idempotency-Key` semantics are
authoritative. A migration analysis below identifies every known breaking
difference.

### Value, costs, and limits

Users gain API-driven code review without losing Wyrd's exact-version
provenance or creating a second assignment, quota, billing, requirement, or
result model. Review definitions remain visible and versioned Cards that a
tenant can inspect, fork, bind, and grant independently. The same operation is
available through HTTP, MCP, and the Rust, Python, and TypeScript SDKs.

The deliberate cost is a multi-step input lifecycle: source bytes are published
as an Artifact before review instead of being embedded in the execution JSON.
That gives the snapshot durable identity, byte integrity, ordinary retention,
and reuse across reviewers, but callers must choose an Artifact name and
version and need `cards:write` to publish it. A caller that may review an
existing Artifact but may not publish one can still invoke review with
`cards:read` plus the required Verifier and tool permissions.

The first release reviews Python and SQL text. It does not execute submitted
code, retrieve repositories, modify source, post pull-request comments, or
claim that a model review is complete or correct merely because several model
calls agree. Source and results may be sensitive. The normal Artifact, Bifrost,
gateway-capture, and audit retention policies apply.

## Authority, current ownership, and related changes

- `architecture/wyrd-design.md` owns Cards, Artifact identity, Agents,
  Verifiers, the verification routes, execution/result semantics, and the rule
  that Verifiers are pure judgment producers.
- `architecture/wyrd-security-posture.md` owns token exchange, tenant identity,
  `verifier:run`, `cards:read`/`cards:write`, tool and gateway permissions,
  non-blocking audit, and foreign-tenant non-disclosure.
- `architecture/bifrost-design.md` owns retained verification input, output,
  result, gateway, and audit evidence.
- Approved `SPEC-verifier-artifact-workspaces` owns exact Artifact collection
  selection, bounded read-only workspaces, `fs.list`, `fs.read`, `fs.search`,
  citation behavior, and the rule that mounting does not implicitly parse or
  execute content. This change extends execution inputs to select such a
  collection dynamically; it does not create another workspace owner.
- Draft `SPEC-agent-swarm-review` uses code review as a motivating fan-out
  workflow. This change neither depends on nor approves internal fan-out. Each
  initial reviewer may use one bounded Agent check. If fan-out later ships, a
  new exact reviewer version may adopt it without changing this API contract.

The inspected current execution contract accepts exact Verifier and subject
UIDs, inline JSON, a single uploaded file or one exact Artifact file, and a
trace. A media input that references a multi-file Artifact must currently name
one file. `VerificationResult` already returns the exact Verifier, subject,
Wyrd verdict, declared requirement breakdown, pinned evidence, citations, and
Agent provenance. The missing generic primitive is a dynamic input slot for an
exact Artifact file, directory, or whole collection.

## Definitions

- **Code-review bundle**: the release-versioned set of Prompt, Agent, and
  Verifier Card documents for the four review domains and automatic routing.
  A tenant declares the bundle through the ordinary Card declaration lifecycle;
  the bundle is not a global hidden registry.
- **Reviewer**: one exact Verifier Card from that bundle or a tenant-authored
  Verifier satisfying the same input and result contract.
- **Source Artifact**: the exact Artifact Card version containing one target
  and zero or more supporting files. It is the review subject.
- **Target**: the one UTF-8 Python or SQL file identified by
  `review_request.target_path`.
- **Context file**: every other selected file in the Source Artifact. Context
  supplies evidence but is not independently routed or separately reviewed.
- **Caller requirement**: one bounded claim supplied in `review_request` as
  evidence for this execution. It is not a new authored Verifier requirement
  and does not mutate the Verifier Card.
- **Native review requirement**: the fixed declared Wyrd requirement
  `code_review` that produces the execution's one Wyrd verdict and carries the
  structured code-review data.
- **Blocking finding**: a finding whose `blocking` field is true. Severity and
  blocking disposition are separate; severity alone never silently changes
  the verdict.

## Required behavior

### Exact, declarable reviewers

**REQ-001 — Versioned reviewer bundle.** Wyrd ships one declarable code-review
bundle containing these reviewer roles:

| Card name | Domain |
|---|---|
| `code-review-data-ml` | Data and machine-learning methodology |
| `code-review-backend` | Backend and distributed systems |
| `code-review-application-security` | Application security |
| `code-review-optimization` | Optimization and operations research |
| `code-review-auto` | Bounded target routing to one of the four roles |

Each declared reviewer is an ordinary exact Verifier Card using ordinary exact
Agent and Prompt references. The shipped bundle records the Wyrd release that
supplied it and declares the existing Wyrd Gateway deployment name
`code-review`; invocation fails through the ordinary gateway error when that
tenant deployment does not exist or is unavailable. A tenant may edit, fork,
rename, or route these Cards before declaration; doing so creates different
exact Card versions and identities. Installing or upgrading Wyrd never silently
registers, replaces, retargets, or broadens a tenant's reviewer Cards. There is
no process-global specialist registry, entitlement-only reviewer, compatibility
alias, or hidden server-selected Verifier.

**REQ-002 — Existing invocation contract.** Callers invoke a reviewer only
through `POST /v1/verification/execute` or
`POST /v1/verification/enqueue`, read queued work through
`GET /v1/verification/executions/{execution_id}`, and use the equivalent MCP
and first-class SDK projections. Requests name the exact reviewer UID and exact
subject UID. No `/code-reviews`, `/verifications`, assignment, specialist, or
legacy projection is added. Direct execution retains its deadline and no-
Operator semantics; queued execution retains ordinary staging, retry, and
polling semantics. Caller-enqueued code review is not a binding-created
execution and therefore fires no Operator.

**REQ-003 — Automatic review is a Verifier.** `code-review-auto` is one exact
Verifier Card with a bounded structured-output router Agent. The router examines
only the submitted target's normalized path, declared media type, and content;
it does not inspect context files. Its output is exactly one of `data_ml`,
`backend`, `application_security`, `optimization`, `unsupported`, or
`ambiguous`, plus a bounded reason. The first four values select the
corresponding exact reviewer Agent. The last two run no specialist and produce
an inconclusive `code_review` outcome. Malformed or unrecognized router output
also produces inconclusive; it never falls back to an unrelated default. The
router reason is nonblank and at most 4,096 characters.

Routing is bounded model judgment, not a deterministic language classifier or
quality guarantee. The execution's Verifier identity remains
`code-review-auto`, while result data reports the resolved role and normal Wyrd
provenance records the exact router and selected reviewer Agent/Prompt routes.
Changing the router Prompt, Agent, route, model, role catalog, or output schema
requires a new `code-review-auto` Card version.

### Exact multi-file source evidence

**REQ-004 — Artifact collection input slot.** Add `artifact` to the generic
Verifier input-slot contract. Its execution declaration reuses
`InputDeclaration::Artifact { artifact, path? }`:

- `artifact` must be an exact UID-bearing Artifact `CardRef` visible to the
  accepted caller;
- `path` selects one inventory file or one directory prefix on path boundaries;
- omitting `path` selects the whole Artifact collection;
- selection order is normalized relative-path order;
- a missing, empty, traversal, wrong-kind, inactive, UID-mismatched, or
  unauthorized selection is refused without exposing inventory contents; and
- selected file count and bytes are admitted under the existing per-file,
  aggregate, workspace, object-integrity, and deadline limits.

An `artifact` slot can be mounted through the existing execution-scope
workspace reference and retains paths relative to the selected file, directory,
or collection root, matching the approved Artifact-workspace contract. The
existing `media` slot remains a single pinned file: an Artifact declaration for
a media slot still requires a path unless the Artifact has exactly one file.
No implicit archive expansion, extraction, parsing, content mutation, or
provider upload is added.

**REQ-005 — Source snapshot and target.** Every code-review Verifier declares
`source` as an `artifact` slot and `review_request` as a schema-constrained JSON
slot. The exact Source Artifact is also the request's subject `card_uid`;
supplying a different Artifact in `source` is refused before any model call.
`target_path` must identify exactly one selected regular file. A `.py` file
accepts stored media type `text/plain` or `text/x-python`; a `.sql` file accepts
`text/plain` or `text/x-sql`. The target must be nonempty, valid UTF-8, at most
1 MiB, and readable through the ordinary verified Artifact inventory. Every
other selected file is context; there may be at most 50, for at most 51 selected
files including the target. Each context file must use the same supported
extension/media-type pairing and be nonempty valid UTF-8 of at most 1 MiB.
Context files together may be at most 8 MiB. The target and context together
therefore reach at most 9 MiB before a stricter existing deployment limit
applies. Limit refusals report the effective bound honestly. Duplicate or
nested-conflicting Artifact manifest paths remain impossible under the existing
Artifact contract.

`target_path` is relative to the selected input root, not always the Artifact
root. Selecting the whole Artifact may name `src/checkout.py`; selecting
Artifact directory `src` names the same file as `checkout.py`. Resolution occurs
once against that root. Selecting exact file `src/checkout.py` retains its
basename at the selected root, so `target_path` must be `checkout.py`; `.` and
the original root-relative path are not aliases. The retained input declaration,
selected-root identity, inventory digests, and result citations together
reconstruct the original Artifact path without accepting absolute paths or a
second normalization.

Completing a review does not create a registry relationship that prevents
ordinary authorized deletion or retention expiry of the Source Artifact. The
result retains the exact Artifact identity, input declaration, evidence digests,
and citations, but source bytes need not remain readable after deletion or
expiry. A tenant that requires later byte reproduction must retain the Artifact
under its normal Card/storage policy.

The reviewer receives the target designation and the selected read-only source
tree. It does not execute the target. The shipped reviewer Agents declare only
the filesystem tools needed to list, search, and read evidence; they do not
declare `code.run`, Bifrost query, Cards mutation, fetch, shell, or external
write tools.

### Caller requirements and result meaning

**REQ-006 — Bounded request schema.** `review_request` has this closed shape:

```yaml
schema_version: 1
target_path: <portable relative Artifact path>
requirements:
  - id: <safe identifier, 1..128 characters>
    statement: <nonblank UTF-8 text, at most 10,000 characters>
    name: <optional string, at most 256 characters>
    rationale: <optional string, at most 10,000 characters>
    version: <optional string, at most 128 characters>
```

`requirements` defaults to an empty list, contains at most 50 entries, and has
unique IDs matching `[A-Za-z0-9][A-Za-z0-9._:-]{0,127}`. Unknown fields,
unsupported schema versions, invalid paths, duplicate IDs, oversized values,
and nonconforming JSON are refused through the normal Wyrd input error. The
request has no caller-supplied capability, model, provider, tenant, contract
hash, verifier implementation, intensity, or arbitrary context dictionary.

Caller requirements are immutable input evidence and appear in the returned
`outcome.evidence.review_request`. Wyrd does not discover, merge, or silently
bind additional tenant requirements during submission. A tenant that wants a
standing policy declares it in the exact reviewer Verifier/Prompt/Artifact
graph or includes it explicitly in the request.

**REQ-007 — One native Wyrd requirement.** Each shipped reviewer declares one
whole-Verifier requirement with ID `code_review`. Its Agent check returns the
ordinary closed Wyrd Verdict and a declared `CodeReviewData` object. Outcome is
derived in this precedence order:

1. Malformed, schema-invalid, oversized, internally inconsistent, or
   structurally incomplete output is rejected at the Agent boundary and yields
   native `inconclusive` with no accepted partial `CodeReviewData`.
2. For accepted data, `failed` wins when any caller requirement is assessed
   `failed` or any finding is blocking, even if another assessment is
   inconclusive or the protocol reports `complete: false`.
3. Otherwise, the result is `inconclusive` when `complete` is false or any
   caller requirement is assessed `inconclusive`.
4. Otherwise, the result is `passed`: `complete` is true, every supplied
   requirement is assessed `passed`, and no blocking finding exists.
5. An invoked review never yields `not_applicable`.

With no caller requirements, the reviewer still judges the declared
`code_review` requirement: it passes only after a complete review with no
blocking finding, fails on a supported blocking defect, and is otherwise
inconclusive. Code review therefore never returns a null or finding-count-only
verdict.

**REQ-008 — Structured code-review data.** The `code_review` requirement's
custom data uses this closed, bounded public shape:

```yaml
schema_version: 1
resolved_reviewer: data_ml | backend | application_security | optimization | null
complete: boolean
incomplete_reason: string | null
summary: string
findings:
  - id: string
    title: string
    description: string
    severity: critical | warning | opportunity
    blocking: boolean
    requirement_ids: [string]
    citation_indexes: [integer]
requirement_assessments:
  - requirement_id: string
    outcome: passed | failed | inconclusive
    reason: string
    finding_ids: [string]
    citation_indexes: [integer]
requirement_summary:
  passed: integer
  failed: integer
  inconclusive: integer
```

Finding IDs are unique. Assessment IDs exactly equal the supplied requirement
IDs once each; an omitted, duplicated, or invented assessment makes the native
outcome inconclusive and exposes no partial pass. Finding requirement IDs name
supplied requirements only. Assessment finding IDs name returned findings only,
and `requirement_summary` counts exactly match the assessment outcomes. Any
broken link or inconsistent count makes the structured output invalid. Citation
indexes refer to the native
`RequirementResult.citations` array; every referenced citation has already
passed Wyrd's original-evidence validation. Invalid indexes, paths, line ranges,
or cross-file claims make the output invalid and therefore inconclusive. The
summary is explanatory only; clients determine completion from execution
status and the native verdict, not from text or finding count.

`complete` describes protocol completion, not correctness or exhaustive defect
detection. It is true only when a supported reviewer was selected, every
declared review stage returned valid output without truncation or known
provider/tool/budget/deadline loss, and every supplied requirement was assessed.
`incomplete_reason` is non-null exactly when `complete` is false.
`resolved_reviewer` is non-null exactly when a specialist ran. Deterministic
aggregation validates these known conditions; a model's assertion cannot
override a known execution failure or missing stage.

Every finding and every `passed` or `failed` requirement assessment has at least
one citation index. An inconclusive assessment may have none. An empty required
citation list or any invalid index, path, line range, or cross-file claim makes
the structured output invalid and therefore yields the REQ-007 inconclusive
boundary outcome.

The data object remains within Wyrd's existing 65,536-byte compact-JSON and
depth-16 custom-data limits. The bundle additionally bounds findings to 100;
summary and `incomplete_reason` to 4,096 characters each; finding IDs to 128;
finding titles to 256; finding descriptions and assessment reasons to 10,000;
each finding's requirement IDs and citation indexes to 50; each assessment's
citation indexes to 50; and each assessment's finding IDs to 100. All supplied
requirement IDs retain REQ-006's bound. When a review would exceed a bound, it
returns an inconclusive native outcome with a safe reason instead of truncating
into a clean or failed judgment.

**REQ-009 — Findings are projections, not another durable noun.** Findings and
caller requirement assessments live in the native requirement's typed data and
are retained with the normal verification output. They do not create Finding,
Assignment, Assurance, Contract, or Specialist Card kinds, separate SQL tables,
or independent lifecycle resources. The exact Source Artifact, Verifier,
Prompt, Agent route, evidence digests, validated citations, subject/actor
identity, application Run, and execution remain available through ordinary
Wyrd provenance and Bifrost records.

### Lifecycle, authorization, and cost

**REQ-010 — Wyrd lifecycle and errors.** Direct review returns `200` with a
`VerificationResult` when judgment completes. Queued review returns `202` with
an `execution_id`, which confirms process staging only. Polling uses Wyrd's
`pending`, `running`, `retrying`, `completed`, `cancelled`, `timed_out`, and
`errored` statuses, separate from `passed`, `failed`, and `inconclusive`.
`completed` may briefly lack a visible result under the existing publication
contract. A failed review verdict is a successful verification result, never an
HTTP error. All refusals use the stable Wyrd Problem Details catalog; no
framework-native validation array or code-review error envelope is added.

**REQ-011 — Existing authorization and audit.** Authentication resolves the
tenant and principal from Wyrd credentials. Requests cannot select a tenant.
Publication requires ordinary Artifact/Card write authority. Review requires
`cards:read` for the exact Source Artifact, `verifier:run` for the exact reviewer,
`gateway:invoke` for each model call, and `tool:invoke` for each filesystem tool
the Agent uses. A declaration or subject choice grants none of these. Each
permission is checked at its existing boundary and audited through the ordinary
non-blocking path. A missing, inactive, unauthorized, or foreign-tenant Card or
execution discloses no foreign metadata and follows Wyrd's not-found behavior.

**REQ-012 — Existing idempotency.** Artifact publication and review execution
keep their separate existing idempotency boundaries. A queued review uses the
HTTP `Idempotency-Key` header or the equivalent MCP/SDK field. Matching retries
under the existing tenant and accepted-actor scope return the same execution
only when the complete canonical `VerificationRequest` identity matches. That
identity includes exact Verifier and subject UIDs, optional Run/group identity,
all input declarations and values, permitted route overrides, and deployment
environment. A semantic conflict leaves the original unchanged and settles the
conflicting execution with Wyrd's idempotency-conflict error under the existing
queued handoff. Direct execution is not replay-safe after an ambiguous transport
failure and is not given an assignment ID.

**REQ-013 — Existing gateway accounting.** Every reviewer model call uses the
declared Wyrd Gateway route and its existing authorization, provider selection,
budget admission, usage capture, and cost settlement. This change adds no
reviewer-specific submission quota, credit unit, provider-credential fallback,
saved intensity profile, or promise about model count, price, latency, or
consensus. Gateway or provider refusal cannot become a failed code finding or a
pass; it produces the normal inconclusive or terminal execution outcome.

**REQ-014 — Surface parity and documentation.** The Artifact slot,
`CodeReviewData` schema, bundle, and examples are represented consistently in
the typed HTTP/OpenAPI contract, MCP descriptors, generated schemas, CLI/card
authoring, and Rust, Python, and TypeScript SDKs. The SDKs may add thin typed
builders for `review_request` and typed decoding of `CodeReviewData`, but they
must call the existing Cards and verification clients rather than implement a
second transport or lifecycle. Documentation includes the publish → select
exact reviewer → execute/enqueue → poll → inspect native verdict journey and
the migration table below.

## Invariants and prohibited outcomes

**INV-001 — One verification model.** Code review remains an ordinary Verifier
execution about an exact Card. There is no parallel assignment service,
specialist registry, requirement engine, polling resource, quota, billing
ledger, or result store.

**INV-002 — Exact and attributable inputs.** Every result identifies the exact
Source Artifact, Verifier, Agent/Prompt provenance, accepted caller, SYSTEM
executor, and evidence digests. A newer Artifact, reviewer bundle, Prompt,
router, or gateway configuration never retargets an accepted execution.

**INV-003 — No hidden requirement expansion.** Only requirements in the pinned
request and standing policy already fixed by the exact reviewer graph affect
the judgment. Workspace or organization state cannot silently add, remove, or
rewrite caller requirements.

**INV-004 — Evidence is not authority.** Source contents, paths, requirement
text, model output, and context metadata cannot select a tenant, principal,
permission, gateway credential, route, Verifier, or subject. Prompt injection
inside submitted code grants no tool or network capability.

**INV-005 — Submitted code is never executed.** Shipped reviewers inspect
source through bounded read-only filesystem tools. They do not run the target,
install dependencies, invoke a compiler, launch subprocesses, access the
network, or write outside bounded scratch. A tenant-authored reviewer that
deliberately declares another tool is a different exact Verifier and remains
subject to that tool's ordinary authorization.

**INV-006 — No false completion.** An empty finding list, generated summary,
HTTP success, accepted queue submission, model response, or completed execution
alone never means the code passed. A pass requires the native Wyrd verdict and
the completeness and requirement conditions in REQ-007–008.

**INV-007 — Tenant isolation and sensitive output.** Source bytes, prompts,
findings, snippets, citations, provider capture, and results stay within the
accepted tenant's existing storage, gateway, Bifrost, and audit boundaries.
Logs, metrics, errors, and audit metadata never contain raw source, requirement
statements, model responses, credentials, or secret values.

**INV-008 — Portable and bounded.** The capability behaves identically on the
supported macOS and Linux architectures. File counts, bytes, JSON, findings,
Agent iterations, tool calls, model calls, workspaces, concurrency, deadlines,
and output are bounded by existing Wyrd limits plus the lower review limits in
this specification. No operating-system sandbox feature forbidden by
`AGENTS.md` is introduced.

## Scope and non-goals

In scope:

- four reusable code-review domains and one automatic reviewer;
- Python and SQL target files;
- exact multi-file Artifact evidence selected at execution time;
- optional caller requirements with native aggregate Wyrd judgment;
- structured findings and per-caller-requirement assessments;
- direct and queued HTTP, MCP, Rust, Python, and TypeScript invocation; and
- explicit migration guidance from the motivating API.

Out of scope:

- a new CodeReview, Assignment, Finding, Assurance, Contract, or Specialist
  Card kind;
- source embedded directly in verification JSON;
- GitHub/GitLab checkout, webhook triggers, repository credentials, diff
  retrieval, comments, fixes, commits, merges, or deployment;
- executing, compiling, testing, or dynamically importing submitted code;
- documents, spreadsheets, notebooks, binary code, generated archives, or
  languages other than Python and SQL;
- automatic organization/workspace requirement discovery or binding;
- API compatibility aliases for motivating reviewer IDs, intensity values,
  legacy request forms, status strings, or error bodies;
- quality, independence, deterministic-output, consensus, latency, or cost
  guarantees for model reviewers; and
- internal reviewer fan-out from `SPEC-agent-swarm-review`.

## Compatibility and migration from the motivating API

The following differences are intentional because Wyrd authority wins. An
existing client of the motivating API must change in every row marked breaking.

| Area | Motivating API | Wyrd contract in this specification | Existing client impact |
|---|---|---|---|
| Endpoint | `POST /api/v1/verifications`; `GET /api/v1/verifications/{id}` | Existing `/v1/verification/enqueue`, `/execute`, and `/executions/{execution_id}` | **Breaking:** paths and direct/queued choice change. |
| Authentication | API key used directly as Bearer token | Wyrd API key exchanges for a short-lived Wyrd access token; SDKs manage renewal | **Breaking:** raw-key HTTP clients must implement token exchange or use a Wyrd SDK. |
| Polling authentication | API key or application browser session | Wyrd execution reads use ordinary Wyrd access-token authorization | **Breaking:** clients cannot rely on the motivating application's browser session. |
| Tenant | Organization derived from key | Tenant derived from verified Wyrd principal | Semantically aligned; identifiers and credentials change. |
| Source transport | Target and context file contents embedded in POST JSON | Publish one exact Artifact Card, then submit its UID-bearing reference | **Breaking:** request is multi-step and needs Card write/read permissions. |
| Source retention | Assignment persistence keeps submitted content available with the assignment | The review result retains Artifact identity and evidence digests, but does not pin source bytes against authorized Card deletion or retention expiry | **Breaking:** clients that require later source reread must retain the Source Artifact under normal Wyrd policy. |
| Subject | Implicit assignment target | Exact `card_uid` required; Source Artifact is the subject | **Breaking:** callers must create and retain exact Artifact identity. |
| Specialist selector | `spearman`, `mauchly`, `driscoll`, `blackett`, or `auto` | Exact reviewer Verifier UID; descriptive Card names; automatic review is another exact Verifier | **Breaking:** all selector strings and the `fathom` alias are rejected. |
| Automatic routing entitlement | Plan-gated server router | Ordinary exact `code-review-auto` Verifier plus normal grants and gateway budget | **Breaking:** no plan-upgrade error or hidden registry; permissions/configuration differ. |
| Unsupported automatic route | Submission rejected with `422 router_skip` | Accepted automatic execution resolves to native inconclusive without running a specialist | **Breaking:** HTTP/error handling and cost expectations change. |
| Intensity | `light`, `standard`, `high`, plus `low`/`medium` aliases | No intensity field or aliases; exact Cards and permitted route overrides fix execution | **Breaking:** intensity fields are unknown and rejected. |
| Requirements | Request requirements plus silently merged workspace requirements | Only pinned request requirements plus policy already declared by the exact reviewer graph | **Breaking:** no automatic workspace merge or binding usage charge. |
| Requirement fields | Optional `canonical_hash`; legacy capability, importance, context, and contract identity | Closed `id`, `statement`, optional `name`, `rationale`, and `version`; request identity is pinned by Wyrd evidence | **Breaking:** unsupported metadata and contract fields are rejected rather than echoed. |
| File paths and limits | Raw relative paths; one 1 MiB target and up to 50 context files totaling 8 MiB | Artifact inventory paths with the same review-specific counts and byte ceilings, additionally bounded by ordinary Artifact and deployment limits | Usually aligned, but **breaking** when an existing request violates Artifact path/media rules or a stricter deployment bound. |
| Path identity | Backslashes normalize for validation while submitted spelling may be retained in provenance | Portable normalized Artifact inventory paths and selected-root-relative `target_path`; single-file selection uses the basename | **Breaking:** path spelling, normalization, and digest/provenance comparisons can change. |
| Requirement results | Top-level dynamic requirement results drive nullable verdict | One native `code_review` RequirementResult carries typed caller assessments and always yields passed/failed/inconclusive | **Breaking:** result traversal and artifact-only null-verdict handling change. |
| Assessment vocabulary | `passed`, `failed`, `inconclusive`, `unavailable`, and `pending` projections; legacy uses another vocabulary | Caller assessments are only `passed`, `failed`, or `inconclusive`; incomplete protocol is represented explicitly and aggregates to native inconclusive | **Breaking:** enum handling and summary columns change. |
| Findings | Open redacted finding projections | Closed bounded `CodeReviewData.findings`, linked to validated native citations by index | **Breaking:** finding fields, severity/disposition use, and extensibility assumptions change. |
| Evidence and input identity | Bespoke file/aggregate digests and extensible evidence objects | Native pinned Wyrd evidence, Artifact inventory digests, and validated `RequirementResult.citations` | **Breaking:** digest construction, evidence fields, citation traversal, and path representation change. |
| Execution ID | Integer `verification_id` | UUIDv7 `execution_id` | **Breaking:** storage and parsing types change. |
| Idempotency | Body `external_assignment_id`, generated from request when omitted; mismatch returns immediate `409` | Optional `Idempotency-Key`; omission creates no derived replay key, and queued replay/conflict follows Wyrd staging and terminal-error semantics | **Breaking:** key location, omission behavior, scope, and conflict timing change. |
| Submission response | Canonical replay always `202`; legacy may return `200` terminal | Direct returns `200` result; queued returns `202` staging response | **Breaking:** clients must choose mode and inspect Wyrd execution status. |
| Lifecycle | `queued`, `running`, `completed`, `timed_out`, `errored`, `cancelled`; legacy has another set | `pending`, `running`, `retrying`, `completed`, `cancelled`, `timed_out`, `errored` | **Breaking:** `queued` becomes `pending`, `retrying` is explicit, legacy statuses disappear. |
| Unknown lifecycle values | Serializer can pass through unlisted internal strings | Wyrd exposes its closed typed execution-status vocabulary | **Breaking:** defensive clients still fail safely, but must not expect arbitrary passthrough values. |
| Context | Advisory `context.wyrd` dictionary | Native `run_id`, optional `group_id`, and `deployment_environment`; no arbitrary context dictionary | **Breaking:** context shape and echo disappear. |
| Assurance echo | Supplied canonical requirement metadata echoed separately | Pinned `review_request` appears in native execution evidence | **Breaking:** response location and null serialization differ. |
| Errors | Ad hoc `{error,message}`, plan/quota bodies, or FastAPI `detail[]` | Stable Wyrd Problem Details and Wyrd error codes only | **Breaking:** error decoding changes; validation never bypasses the Wyrd envelope. |
| Quota | Fixed 100 submissions/hour organization quota | Existing Wyrd authorization, runtime capacity, and gateway budget only | **Breaking:** no equivalent quota headers or promises. Deployment owners may configure ordinary limits. |
| Billing | Bohmian credit holds/settlement and optional organization provider credentials | Existing Wyrd Gateway admission, capture, usage, and provider ownership | **Breaking:** billing fields and funding fallback semantics disappear. |
| Response metadata | Dedicated result ID, request digest, verifier display/version fields, usage/billing objects, and legacy timestamps are in the API projection | Native execution/result IDs, exact Card/route provenance, Wyrd timestamps, and gateway/Bifrost records remain in their existing owners | **Breaking:** field names, locations, optionality, and serialization change. |
| Legacy request | Flat legacy JSON accepted and projected back | Unknown fields and all legacy shapes/aliases are rejected | **Breaking:** legacy clients must migrate; no compatibility period is promised here. |

## Acceptance obligations and credible evidence

| ID | Situation and expected observation |
|---|---|
| **AC-001** | Through each real Rust, Python, and TypeScript SDK, publish a Python target plus one context file as an Artifact, invoke the exact application-security reviewer directly and queued, and observe the same exact subject/Verifier identity, one native `code_review` requirement, validated source citations, structured findings, caller assessments, and aggregate verdict. HTTP and MCP journeys prove the same contracts. |
| **AC-002** | Review equivalent Python and SQL Artifacts through `code-review-auto`. A deterministic mock router response controls the expected exact reviewer role; changing only a context file does not change the router input. `unsupported`, `ambiguous`, malformed, and unrecognized router responses cannot become a clean pass or unrelated default review. |
| **AC-003** | Invoke every explicit reviewer with no caller requirements. A supported blocking defect fails with a finding and citation; a complete clean review passes; unavailable evidence or malformed provider output is inconclusive. No result uses a null verdict or finding-count summary as the decision. |
| **AC-004** | Supply two requirements and deterministic reviewer responses for pass, fail, and inconclusive. The assessment set exactly matches request IDs, summaries count correctly, finding links resolve, and native aggregation follows REQ-007. Missing, duplicate, invented, or invalidly cited assessments cannot pass. |
| **AC-005** | Exact queued replay returns one execution and performs no duplicate model review; neither the original nor replay fires an Operator because caller-enqueued review is not binding-created. Reusing the key with changed Artifact, target, requirements, reviewer, subject, route, or environment preserves the original and exposes Wyrd's idempotency conflict through the ordinary execution lifecycle. |
| **AC-006** | Exercise missing/foreign/inactive Artifact and Verifier UIDs, mismatched subject/source, denied Card/Verifier/gateway/tool grants, path traversal, absent target, duplicate requirement IDs, unknown fields, invalid schema version, non-UTF-8 target, and every file/count/JSON/output boundary. Protected bytes never reach the model or tools after refusal, foreign identity is not disclosed, and audited decisions use existing owners. |
| **AC-007** | The reviewer asks only for declared filesystem operations. A prompt-injection string in source cannot obtain `code.run`, network, Cards mutation, Bifrost query, another tenant's file, credential material, or an undeclared route. Submitted target code is never executed. |
| **AC-008** | Trigger gateway budget refusal, provider refusal, malformed structured output, process capacity refusal, queued retry, cancellation, timeout, and delayed result visibility. Each remains distinct from a failed code judgment; no empty findings or generated summary implies pass. |
| **AC-009** | Declare bundle version 1, then a changed version 2. Existing executions and results keep version-1 Verifier/Agent/Prompt provenance; no registration, upgrade, route change, or auto-router change retargets them. A tenant-forked reviewer remains a different exact identity. |
| **AC-010** | Raw motivating canonical and legacy bodies, legacy reviewer aliases, `intensity`, body idempotency, and unknown fields are rejected through Wyrd Problem Details. Migration documentation contains the complete compatibility table and a working publish/enqueue/poll replacement example. |
| **AC-011** | Complete a review, then exercise ordinary authorized Artifact deletion or retention expiry. The retained result, exact identity, declaration, digests, citations, and provenance are not rewritten or silently pinned; a later source-byte read fails through the normal Artifact/storage contract. Keeping the Artifact under policy preserves exact reread. |

Journey evidence uses `WyrdTestServer`, repository-managed PostgreSQL, real
Card registration, storage upload/inventory, Wyrd authorization, hosted
workspaces/tools, queued execution, Bifrost publication/readback, and all
first-class client projections. A deterministic mock model upstream may control
review findings and routing responses; replacing the Wyrd execution engine,
storage, permission checks, or result reader with an in-process stub is not
acceptance evidence. Target execution is never part of a journey.

## Required authority updates

Before implementation completes, update the owning sections of
`architecture/wyrd-design.md` to describe the generic `artifact` input slot,
its collection semantics, and code-review bundles as ordinary Verifier/Agent
graphs using the existing execution routes. Update security documentation only
if the existing permission descriptions need the new journey as an example;
do not introduce a code-review permission. Generated OpenAPI, schemas, SDK
types, MCP descriptors, and public documentation must follow the approved
source contracts rather than becoming independent authorities.

## Open material decisions

None. This draft makes the expensive decisions explicit: exact tenant-declared
Verifiers instead of a hidden specialist registry; an exact Artifact subject
instead of inline source; one generic Artifact collection slot instead of a
code-review upload path; native Wyrd execution/result semantics instead of an
assignment façade; caller requirements as pinned evidence with one native
aggregate requirement instead of runtime mutation of the Verifier; and no
legacy aliases or compatibility response.

## Revision history

- **Revision 1 — 2026-11-11, draft.** Initial Wyrd-native code-review API
  specification derived from the supplied Bohmian code verification API
  reference. Wyrd authority wins every identified tension. Not approved and
  does not authorize implementation.

## Material authority references

- `AGENTS.md`
- `architecture/README.md`
- `architecture/wyrd-design.md`: Doctrine, Artifact, Agent, Verifier, registry
  lifecycle, public verification routes, and client model
- `architecture/wyrd-security-posture.md`: API keys and access tokens,
  permission model, tenant isolation, audit integrity, and secret handling
- `architecture/bifrost-design.md`: retained verification and audit evidence
- `changes/spec-driven-development.md`
- `changes/active/verifier-artifact-workspaces/spec.md` (approved revision 3)
- `changes/active/agent-swarm-review/spec.md` (draft revision 2; informative
  relationship only)
