---
name: wyrd-task-review
description: Review one immutable cumulative Wyrd task through independent implementation, system-resilience, standards, and domain audits, source validation, and root-cause remediation.
---

# Wyrd Task Review

Answer one question: does the resulting repository satisfy the original task
exactly? This is an acceptance audit, not an opportunity to improve, redesign,
or refactor the implementation.

Audit the task against stronger repository and specification authority before
treating its prescribed mechanics as acceptance criteria. Independently compare
new or materially changed graph stores, traversals, loaders, parsers,
transports, validators, lifecycle owners, caches, and orchestration with existing
owners and callers. Record the existing alternative, missing behavior, and
why extension is or is not sufficient. A meaningful dependency-owning struct
may still duplicate an existing workflow; task compliance and leaf-call reuse
cannot waive that violation. Use the existing maintainer and implementation
reviewers, not an additional reviewer role or symbol allowlist.

An invalid private task instruction routes to task correction through
`$wyrd-plan`; a material behavior or boundary change returns
`SPEC_REVISION_REQUIRED`. Do not strengthen replacement machinery merely
because the task prescribed it.

Keep the reviewed source immutable. Review the complete base-to-candidate range,
not the implementation summary or only the latest fix diff. After remediation,
include the original task, prior verdict and findings, remediation task, and
cumulative candidate.

On a remediation round, use the latest fix diff to locate changed owners and
check closure of prior findings, while retaining the complete cumulative
acceptance audit. A previously closed finding stays closed only with source
evidence. Do not rediscover the same defect one caller at a time.

## Establish the subject

Require the repository root, unambiguous base and candidate commits, approved
spec, original task, actual diff, repository rules, and available verification
results. If the candidate changes during review, return `BLOCKED`.

Read `AGENTS.md`, [agent rules](../../../architecture/agent-rules.md),
[spec-driven development](../../../architecture/references/languages/spec-driven-development.md),
[maintainer style](../../../architecture/references/languages/maintainer-style.md),
and only the architecture and testing references applicable to the task. Follow
CodeGraph instructions. Build a compact navigation map of changed symbols,
owners, likely callers and consumers, and relevant tests, and select the
required `domain-rev` agents. Give every reviewer the same map as a starting
point, not as complete coverage or a conclusion; reviewers must expand and
verify it against source. Use existing repository navigation tools. Do not add
a path or symbol script solely for this review.

Before spawning discovery reviewers, create a new
`changes/active/<slug>/review/<review-name>/` directory without overwriting a
prior attempt. Assign every reviewer its exact report path in that directory.

## Required agent topology

The calling agent is the review orchestrator, not one of the reviewers. It MUST
use the harness's agent-delegation capability to spawn separate fresh subagents;
simulating multiple reviewer roles in one context does not satisfy this skill.
No agent may fill more than one role.

Run independent discovery, conditional follow-up, and validation:

1. **Independent discovery, in parallel:** always spawn two task implementation
   reviewers (`behavior-rev` and `invariant-rev`), a repository-standards
   reviewer (`repo-rev`), a maintainer specialist (`maintainer-rev`), and a
   system-resilience reviewer (`system-rev`). Also spawn one domain reviewer
   (`domain-rev`) for each materially changed sensitive domain, including
   security/RBAC, tenancy, concurrency, durability, persistent data, or another
   boundary whose correctness needs domain expertise.
2. **Compare claims:** after discovery reports are complete, group findings by
   violated behavior or invariant rather than line number. Agreement is useful
   corroboration, not proof; unique findings still require validation. If
   reports materially conflict, reveal an unreviewed reachable path, or repeat
   remediation suggests a common source the reviewers did not trace, spawn one
   fresh focused `followup-rev` to investigate only that uncertainty. Record
   why it was or was not needed.
3. **Independent validation:** after any follow-up, always spawn one fresh
   structured Ponytail reviewer (`ponytail-rev`). It validates every proposed
   finding against source and produces the final finding ledger and
   decision-complete recommendations, even when the finding union is empty.

Discovery reviewers receive the immutable subject and inputs needed for their
own scope, but not another reviewer's conclusions or an intended verdict. The
`followup-rev`, when needed, receives the conflicting claims and source paths,
without an intended verdict. The `ponytail-rev` receives the complete diff,
applicable authorities, and every discovery and follow-up report. The
orchestrator establishes the subject, routes inputs, compares claims, checks
report completeness, and writes final artifacts from the validated ledger; it
does not validate its own findings. If a required agent or report is missing,
or the candidate changes during review, return `BLOCKED`.

## Independent task implementation reviews (`behavior-rev`, `invariant-rev`)

Both reviewers must be fresh relative to implementation and independent of each
other. Give each the original specification and task, actual cumulative diff,
relevant repository rules, and available verification results—not the
implementation agent's completion summary or an intended verdict. Both review
the full task and candidate; their lenses guide investigation, not scope.

Start unconvinced. The candidate earns acceptance through repository, diff, and
verification evidence; intent, summaries, plausible code, and green checks
alone do not establish completion. Try to falsify every acceptance criterion,
constraint, non-goal, and claimed regression boundary through a realistic
reachable path.

The `behavior-rev` follows realistic caller-to-result paths and challenges
acceptance criteria, negative flows, and regression boundaries. The
`invariant-rev` follows values and state from producer to sink across shared
owners, sibling consumers, lifecycle transitions, and failure paths. Both trace
a failure at a consumer back to its producer before locating the defect. On a
repeat review, use prior findings as hypotheses: determine whether apparently
separate failures share the same source, including one introduced by an earlier
remediation.

Apply the Ponytail ladder to every changed abstraction, dependency,
configuration surface, compatibility path, generic layer, and speculative
extension: delete it if the task does not need it; otherwise reuse repository,
standard-library, native-platform, or installed-dependency behavior before
accepting new code. Require the smallest safe root-cause correction without
weakening validation, error handling, security, accessibility, or durability.

Reject unearned complexity. Where a standard and conventional way exists (a
published standard such as an RFC, or the established practice of comparable
widely used projects), the candidate must use it. Classify any mechanism,
state, check, file, setting, option, or error beyond that standard as `DRIFT`
unless approved authority explicitly requires it. Never require one in a
finding or remediation; a correction follows the standard way too.

Treat each failure diagnosis in the task evidence as a claim to falsify: the
recorded cause must explain the trace, and the fix must sit at that cause. A
change to a test, assertion, timeout, sleep, retry, `#[ignore]`, `#[allow]`, or
skip made to clear a failure, without a recorded diagnosis and diagnostician
report, is a `VIOLATION`.

For each suspected defect, enumerate sibling callers and writers of the same
authority before proposing a correction. If the candidate adds a second way
to resolve the same identity, use the same SQL capability, lock the same
authority, validate the same token profile, or observe the same test condition,
report the shared cause and the existing owner to reuse. Distinguish a
standard-required check from task drift: for OIDC ID tokens and JWT bearer
assertions, compare the complete applicable OIDC Core, RFC 7523, and RFC 8725
rules without duplicating signature/JWKS verification or inventing a profile.

Build an explicit matrix:

| Requirement, acceptance criterion, constraint, or non-goal | Implementation evidence | Verification evidence | Result |
|---|---|---|---|
| `<obligation>` | `<diff or source location>` | `<test/check or N/A>` | `PASS | FAIL` |

Inspect specifically for:

- **MISSING** — required behavior was not implemented;
- **INCORRECT** — behavior exists but does not satisfy the requirement;
- **DRIFT** — implementation extends beyond the requested scope;
- **VIOLATION** — an explicit constraint or non-goal was violated; and
- **REGRESSION** — existing behavior was unintentionally changed.

Do not report optional improvements, speculative hardening, preferences,
unrelated pre-existing debt, or refactors not required by the task.

A finding blocks only when it has a behavioral, security, tenancy,
durability, or public-contract consequence, or when it deletes unearned code.
Placement, naming, structure, and wording findings with no such consequence
are non-blocking even when a style rule can be cited; record them under
non-blocking notes and never let them alone produce `FIX_REQUIRED`. Missing
rustdoc on a changed item stays blocking per `AGENTS.md`. Tests prove
behavior; they do not prove that the requested behavior was built. Rely on
repository source and the diff, not agent summaries.

Each reviewer returns its own `task-review-behavior.md` or
`task-review-invariants.md` with the acceptance matrix, proposed findings, and
one overall `PASS`, `FAIL`, or `BLOCKED` result. Each finding needs a
source-local ID, classification, violated obligation, exact location, evidence,
observable consequence, and required testable correction. For `DRIFT`, identify
what can be deleted or which existing or native mechanism already covers the
outcome. Do not treat matching findings as automatically true or discard a
finding because only one reviewer reported it.

## Independent repository standards review (`repo-rev`)

Give the repository-standards specialist the immutable subject, complete diff,
repository root, and available verification results. Do not provide either task
reviewer's conclusions or an intended verdict.

The specialist reads `AGENTS.md`, [agent rules](../../../architecture/agent-rules.md),
and the complete applicable authority selected through the
[reference router](../../../architecture/references/README.md). It maps every
changed surface to its governing rules and inspects enough surrounding source,
tests, manifests, generated artifacts, and consumers to determine compliance.
It audits all touched languages and layers; one surface cannot stand in for
Rust, Python, TypeScript, server, contract, test, documentation, or tooling
rules that independently apply.

Inspect actual SQL capability types in production signatures and fields.
`check:from-pools-allowlist` covers construction, not raw-pool propagation;
a green check cannot excuse `PgPool` where `OperatorPool` or `TenantConn` is
required. Include materially changed test helpers and rustdoc in the same
standards pass, so documentation corrections do not become separate rounds.

The `repo-rev` returns:

1. an authority-coverage table mapping each changed surface to every applicable
   repository authority;
2. a pass or fail result for each applicable rule, with exact rule and source
   evidence; and
3. material repository-rule findings with source-local IDs, the violated rule,
   location, consequence, and testable correction; and
4. one overall `PASS`, `FAIL`, or `BLOCKED` result.

The specialist does not review task acceptance, propose optional improvements,
or perform the Ponytail audit. Missing authority or incomplete coverage blocks
the review. Preserve its report as `standards-review.md`.

## Independent maintainer review (`maintainer-rev`)

Give the specialist the immutable subject, complete diff, repository root,
[maintainer style](../../../architecture/references/languages/maintainer-style.md),
applicable Wyrd authority, and available verification results. Do not provide
other reviewers' conclusions or an intended verdict.

The specialist reads each materially changed symbol in its owning module,
traces its callers and relevant tests, and reviews layout, owner and method
shape, naming, argument and return types, test clarity, documentation, and
generated declaration parity. It asks whether a maintainer
can find, follow, and safely change the behavior. It applies the guide under
Wyrd's explicit rules and does not require code to imitate a sample.

Return `maintainer-review.md` with changed-surface coverage, material findings,
and one overall `PASS`, `FAIL`, or `BLOCKED` result. Each finding needs a
source-local ID, changed location, governing rule or guide principle, concrete
maintenance cost, and smallest testable correction. Cite a nearby Wyrd pattern
when available. Personal preference without a concrete cost is not a blocking
finding; record uncertain preferences separately for calibration. Incomplete
coverage blocks this review.

## Independent system-resilience review (`system-rev`)

Give this specialist the immutable subject, complete diff, deployment and
process topology from applicable architecture and source, and available
verification results. Do not provide other reviewers' conclusions or an
intended verdict. Its starting premise is that processes crash, dependencies
become unavailable, messages are delayed, and recovery follows interruption.
It reviews how the candidate changes the deployed system, not just whether the
changed component works in isolation.

Trace each material changed runtime path across its owner, shared process or
pod, dependent services, and user-facing capabilities. Select credible failure
and recovery paths for that topology: component failure, process restart,
dependency outage, timeout, cancellation, or rolling replacement as relevant.
For each, establish what stops, what remains available, whether retries or
health checks amplify the fault, what state survives, and how service resumes.
Inspect the actual callers, lifecycle ownership, and failure propagation before
accepting a local fix. A recommendation to fail closed must name its boundary:
request, component, process, or service. Refuse unsafe work, but do not treat a
component error as permission to crash a shared server or take unrelated
capabilities offline unless approved authority requires that result. Do not
demand availability that would violate integrity, and do not report speculative
hardening outside the approved task or a reachable regression.

Return `system-review.md` with deployed-path and failure-path evidence,
affected capabilities, recovery and proof assessment, material proposed
findings, and one overall `PASS`, `FAIL`, or `BLOCKED` result. If the candidate
has no runtime or deployment effect, establish that from the diff and callers.
Each finding needs a source-local ID, violated obligation or regression
boundary, exact location, observable system consequence, and testable
correction. A healthy-path test alone cannot prove a changed recovery path.

## Independent sensitive domain review (`domain-rev`)

Spawn a separate `domain-rev` for each sensitive domain materially changed by
the candidate. Scope each agent to one coherent domain and give it the approved
specification, original task, complete diff, governing authority, surrounding
code, available verification results, and exact report path needed to trace that
boundary end to end. Examples include an RBAC/security reviewer for permission
and trust-boundary changes or a data-layer reviewer for transaction, RLS,
durability, concurrency, and persistent-state changes.

Each `domain-rev` returns `domain-review-<domain>.md` with its reviewed boundary,
authority and source coverage, verification limits, material proposed findings,
and one overall `PASS`, `FAIL`, or `BLOCKED` result. Every finding needs a
source-local ID, violated obligation, exact location, evidence, observable
consequence, and testable correction. It does not broaden the approved task or
report speculative hardening.

## Focused follow-up (`followup-rev`)

When the claim comparison triggers a follow-up, give one fresh reviewer the
specific conflict or unexplored path, complete cumulative diff, original task,
governing authority, and relevant source. Require `followup-review.md` with the
source path inspected, evidence resolving or narrowing the uncertainty, any
new proposed findings, and `RESOLVED` or `UNRESOLVED`. The follow-up is a
discovery pass, not a vote on existing findings. A new or unique finding still
goes to independent validation. Do not run extra passes merely to reach a
predetermined count or obtain agreement.

## Structured Ponytail validation (`ponytail-rev`)

Always spawn a fresh `ponytail-rev` after discovery and any follow-up are
complete, even when their proposed finding union is empty. Give it the
immutable subject, applicable authorities, complete diff, both task-review
reports, `standards-review.md`, `maintainer-review.md`, `system-review.md`, every
`domain-review-<domain>.md`, and any `followup-review.md`, but no intended verdict.

The `ponytail-rev` independently inspects the actual source, validates every
proposed finding and correction, including claims from only one reviewer. It
removes duplicates and resolves contradictions from approved authority. It
checks cited symbols and paths against current source; agreement among
reviewers does not replace this proof. Apply this ladder to every finding and
proposed remediation:

1. Can it be deleted while preserving the complete task?
2. Does existing repository behavior already solve it?
3. Is there a standard and conventional way to do it? Use that, nothing more.
4. Does the standard library or native platform solve it?
5. Does an already-installed dependency solve it?
6. Only then, what is the minimum necessary correction?

Prefer a correction at the source that makes invalid state unrepresentable or
prevents it from being produced. A guard at a consumer is appropriate when that
consumer owns the invariant or enforces a real trust boundary; otherwise trace
upstream before accepting it. Do not mistake fewer lines for a valid
simplification when it weakens validation, error handling, security,
accessibility, durability, or another explicit requirement. For each proposed
finding the `ponytail-rev` must:

1. trace the failing state from its producer to the observed consumer,
   including every caller, sibling consumer and writer of the same durable
   authority, and existing helper; read the full body of each function the
   correction would change or move;
2. prove the reported path is reachable and required by the approved task,
   rejecting dormant, test-only, speculative, or zero-caller surfaces unless
   the task explicitly requires them;
3. distinguish the requested behavior from bundled adjacent behavior and reject
   a correction that moves, duplicates, or weakens unrelated lifecycle,
   admission, durability, security, resource ownership, or sibling service
   availability;
4. ask whether the finding or remediation can be deleted, whether existing
   behavior already satisfies the task, and whether the proposed proof is the
   smallest credible check without a new dependency or test harness;
5. compare prior findings and remediations for a shared source, consolidating
   related failures into one correction when the same invariant owns them; and
6. return `CONFIRMED`, `REVISED`, or `REJECTED` with source evidence and the
   smallest safe correction boundary. Explain why any retained consumer guard
   belongs there.

The final deduplicated ledger records each retained finding's stable
`FIND-<task>-<n>` ID, discovery source IDs, `CONFIRMED` or `REVISED` status,
classification, violated obligation, exact location, evidence, observable
consequence, decision-complete correction, and focused closure proof. Preserve
prior `FIND-*` IDs during remediation and assign the next unused number only to
new findings. Each correction selects the smallest safe approach, names the
existing owner or mechanism to reuse, and preserves adjacent behavior. When
discovery proposed no findings, return an explicitly validated empty ledger.
Group findings with one cause into one correction boundary and list all
affected paths. Treat human-directed additions as separate from independently
validated reviewer findings; never assign them a `FIND-*` ID retroactively.

The orchestrator may include only independently confirmed or revised findings.
A rejected finding is omitted, not softened into optional advice. If validation
shows that the correction needs a new product, public API,
architecture, security, compatibility, cross-service, concurrency, resource-
ownership, or persistent-data decision, do not prescribe it as remediation;
return `SPEC_REVISION_REQUIRED` when the approved task truly requires that
decision, otherwise reject the finding as out of scope.

Missing source, incomplete caller tracing, an unavailable independent
`ponytail-rev`, or disagreement that cannot be resolved from approved authority
blocks the review. Preserve its final ledger and recommendations as
`findings-validation.md` in the review directory.

## Verdict and remediation task

In the established review directory, preserve both task-review reports,
`standards-review.md`, `maintainer-review.md`, `system-review.md`, every
`domain-review-<domain>.md`,
any `followup-review.md`, and `findings-validation.md`, then write `verdict.md`
containing the immutable subject, reconciled acceptance matrix, independent
review results, the follow-up decision, validated finding ledger, verification
limits, prior-finding closure, and one verdict:

- `PASS` — all required reports are complete, the independently validated ledger
  is empty, every obligation passes after reconciling proposed findings with
  source, non-goals remain excluded, verification is credible, and no unrelated
  change entered the diff. A rejected proposal does not force failure merely
  because its discovery reviewer reported `FAIL`;
- `FIX_REQUIRED` — one or more bounded implementation findings remain;
- `SPEC_REVISION_REQUIRED` — correction requires changing approved behavior or
  an expensive-to-reverse decision; or
- `BLOCKED` — the immutable subject, authority, diff, or required independent
  review cannot be obtained.

For `FIX_REQUIRED`, also write one self-contained remediation task named
`<task-id>-R<n>-<name>.md` in the same review directory. It must contain:

1. the approved spec path, original task path, and candidate identities;
2. an issue diagnosis for each material finding: the violated obligation,
   current behavior, exact evidence, observable consequence, and why the
   candidate or its existing proof falls short;
3. the intended correction outcome;
4. a decision-complete recommendation within the approved behavior: select the
   minimal correction approach, name the existing owner or mechanism to reuse,
   identify the source of the invalid state and affected consumers, resolve
   alternatives that would change scope or proof, and explain why that approach
   closes the diagnosed gap without relying on repeated downstream guards;
5. constraints, preserved behavior, and explicit non-goals;
6. acceptance criteria mapped to every finding; and
7. focused proof that directly exercises the gap plus only the narrowest
   lanes covering the remediation's write set. Never require full user-journey
   suites or broad aggregates in a task remediation; they run once at change
   review.

Batch bounded documentation corrections with the same remediation task. Keep
the rustdoc gate intact, but never create a style-only round. If approved spec text
conflicts with the required correction, return `SPEC_REVISION_REQUIRED` and
name the conflict instead of prescribing an unapproved behavior change.

Do not write an outcome checklist or merely restate the acceptance matrix. The
diagnosis and recommendation are the substance of the remediation task;
acceptance criteria only prove that correction. An implementer must not need to
rediscover the defect or choose the correction boundary. If that recommendation
requires a new product, public API, architecture, security, compatibility,
cross-service, concurrency-semantics, or persistent-data decision, return
`SPEC_REVISION_REQUIRED` instead.

The remediation task packages validated findings for a fresh implementation
agent; it is not another design plan. Do not specify helpers, private methods,
local control flow, fixture structure, or optional improvements. Route it
directly to `$wyrd-implement`. A later review reassesses the complete cumulative
candidate against the original task.

Return only the verdict, verdict path, remediation task path when present, and
finding IDs. `PASS` is the task's completion gate. Review does not implement,
merge, push, or deploy.
