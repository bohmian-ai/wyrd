---
name: wyrd-task-review
description: Review an immutable Wyrd task with adversarial discovery, independent source validation, materiality checks, and focused remediation review. Add specialists for affected boundaries.
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

Keep the reviewed source immutable. The first review covers the complete
base-to-candidate range, not the implementation summary. After remediation,
retain the original task, prior verdict and findings, remediation task, prior
candidate, and cumulative candidate as the acceptance context.

On a remediation round, review prior-finding closure, the diff from the prior
candidate to the new candidate, and affected callers, consumers, invariants,
and tests. Carry
forward source-backed acceptance and closure evidence for unchanged behavior;
check whether changed dependencies or authority invalidate it. Reopen an
unchanged area only when new evidence establishes a material defect or a fix
can affect its accepted behavior. Record that reason and the expanded scope.
If prior coverage or evidence is missing, inspect the specific gap. Do not
restart discovery across the whole task merely because another round began,
or rediscover the same defect one caller at a time.

After two unsuccessful remediation reviews, include a brief convergence
assessment in the verdict before prescribing another round. Distinguish
incomplete fixes, fix-induced regressions, missed original defects, and
expanding expectations; identify their causes and the next bounded review
scope. Reject scope expansion and consolidate shared causes. This is a
calibration step, not an automatic PASS, round limit, or new approval gate.

## Establish the subject

Require the repository root, unambiguous base and candidate commits, approved
spec, original task, actual diff, repository rules, and available verification
results. If the candidate changes during review, return `BLOCKED`.

Read `AGENTS.md`, the nested `AGENTS.md` files for every directory the diff touches,
[spec-driven development](../../../changes/spec-driven-development.md),
[maintainer style](../../../agent_docs/maintainer-style.md),
and only the architecture and testing references applicable to the task. Follow
CodeGraph instructions. Build a compact navigation map of changed symbols,
owners, likely callers and consumers, and relevant tests. Record the round's
review scope, carried-forward evidence, and specialist selection with reasons.
Give every reviewer the same map as a starting point, not as complete coverage
or a conclusion; reviewers must expand and
verify it against source. Use existing repository navigation tools. Do not add
a path or symbol script solely for this review.

Before spawning discovery reviewers, create a new
`changes/active/<slug>/review/<review-name>/` directory without overwriting a
prior attempt. Assign every reviewer its exact report path in that directory.

The review directory holds reports only. Never write a diff, patch, or source archive into it:
the base and candidate commits recorded in the review identify the subject, and
reviewers read the recorded range directly from Git. Use
`git diff <base>...<candidate>` for the first review and
`git diff <prior-candidate> <candidate>` for remediation, with the cumulative
range available for context (an external repository at its recorded commits).
If rebasing changes ancestry, establish patch correspondence before carrying
forward evidence; inspect any differences that affect the reviewed behavior.

## Required agent topology

The calling agent is the review orchestrator, not one of the reviewers. It MUST
use the harness's agent-delegation capability to spawn separate fresh subagents;
simulating multiple reviewer roles in one context does not satisfy this skill.
No agent may fill more than one role.

Select reviewers for the round's affected boundaries:

1. **First review:** spawn one fresh `behavior-rev` for comprehensive adversarial
   discovery. It covers behavior and invariants, applicable repository rules,
   maintainability, and credible failure/recovery paths. These obligations
   remain required when no specialist is selected.
2. **Conditional specialists:** add `invariant-rev` for materially complex shared
   state or lifecycle behavior, `repo-rev` for broad cross-tier rule coverage,
   `maintainer-rev` for substantial ownership or abstraction changes, and
   `system-rev` for material failure-propagation or recovery changes. Spawn a
   `domain-rev` for each materially changed sensitive boundary requiring domain
   expertise, such as security/RBAC, tenancy, concurrency, or persistent data.
   Mere use of an established boundary does not require its own reviewer.
   Scope specialists to that boundary and run independent discovery in parallel.
3. **Remediation review:** default to one fresh `ponytail-rev` checking closure,
   changed code, and affected paths. Add discovery or specialist reviewers only
   for material changes or unresolved questions in that scope; do not recreate
   the first round's team automatically.
4. **Compare claims:** group proposals by violated behavior or invariant, not
   line number. Agreement is corroboration, not proof. Use one focused
   `followup-rev` only for a material conflict, an unresolved failing path, or
   an untraced common cause. A unique finding alone does not require another
   discovery pass. Record why follow-up was or was not needed.
5. **Independent validation:** always use a fresh `ponytail-rev`, even when no
   findings are proposed. It validates source, materiality, correction burden,
   and closure, and produces the final ledger and recommendations. If it
   discovers a new blocker itself, obtain separate focused source validation
   before retaining it; no reviewer validates its own new finding.

Discovery reviewers receive the immutable subject, assigned scope, and inputs
needed for that scope, but not another reviewer's conclusions or an intended
verdict. The `followup-rev` receives the specific uncertainty and source paths.
The `ponytail-rev` receives applicable authorities, relevant diffs, prior
acceptance/closure evidence, and every selected discovery and follow-up report,
without an intended verdict. Full cumulative context remains available to all.
The orchestrator routes inputs, records coverage and reviewer selection,
compares claims, and writes artifacts from the validated ledger; it does not
validate its own findings. A missing selected reviewer/report or a changed
candidate returns `BLOCKED`; an unselected specialist requires no report.

## Materiality and correction burden

Apply this threshold to every role and every round. A blocker must identify an
explicit approved obligation or a supported-use regression, a source-proven
failing scenario, and a meaningful consequence. Explain why the scenario
matters for supported use or a required boundary. Reachability, schema
acceptance, and the existence of a conceivable input do not alone establish
materiality. Broad promises such as “complete” or “all states” do not authorize
unbounded hardening. An unusual input still blocks when it violates an explicit
acceptance obligation or exposes a material security, tenancy, integrity,
availability, accessibility, durability, or public-contract failure. Preserve
mandatory repository gates, including changed-item rustdoc; do not waive them
as low probability or inconvenient.

For DRIFT, identify concretely unearned machinery and the deletion or existing
owner that preserves the task. A different conventional implementation, a
line-count preference, or a cheaper-looking alternative is not enough.
Placement, naming, structure, and wording without a material consequence or an
explicit mandatory gate remain non-blocking. Record genuine minor observations
separately; omit optional improvements and speculative hardening.

Challenge the correction as hard as the defect. Explain its implementation and
maintenance burden and why deletion, reuse, or a smaller local correction is
insufficient. If a marginal case would require substantial machinery, recheck
its materiality and authority before retaining it. Do not use cost to waive a
required safety boundary or explicit obligation; route a genuinely new material
decision to specification revision. A rejected proposal creates no remediation
work, and review does not need to exhaust every conceivable input to PASS.

## Independent task implementation reviews (`behavior-rev`, `invariant-rev`)

Every selected discovery reviewer must be fresh relative to implementation
and independent of the other reviewers. Give each the original specification
and task, assigned review scope and actual diffs, relevant repository rules,
and available verification results—not the implementation agent's completion
summary or an intended verdict. The first behavior review covers the full task;
remediation and specialists follow the recorded scope.

Start unconvinced. The candidate earns acceptance through repository, diff, and
verification evidence; intent, summaries, plausible code, and green checks
alone do not establish completion. Within the assigned scope, try to falsify
each acceptance criterion, constraint, non-goal, and claimed regression boundary
through a realistic reachable path. Stop when credible acceptance evidence and
material findings have been resolved; no finding quota or exhaustive input
search is required.

The `behavior-rev` follows realistic caller-to-result paths and challenges
acceptance criteria, negative flows, and regression boundaries. The
`invariant-rev` follows values and state from producer to sink across shared
owners, sibling consumers, lifecycle transitions, and failure paths. Without
an invariant specialist, behavior-rev covers these paths too. Both trace
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

Reject unearned complexity. Follow applicable published standards and prefer
established repository and conventional mechanisms. Report additional machinery
as `DRIFT` when it duplicates an existing owner or serves no approved outcome;
show what can be removed while preserving required behavior. A departure from
a common implementation is not itself a defect. Never prescribe speculative
machinery in a finding or remediation.

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

Apply the materiality and correction-burden threshold above. Tests prove
behavior; they do not prove that the requested behavior was built. Rely on
repository source and the diff, not agent summaries. Carry forward unaffected
acceptance-matrix rows with their prior evidence references; update rows whose
behavior, dependencies, governing authority, or proof changed.

Each reviewer returns its own `task-review-behavior.md` or
`task-review-invariants.md` with the acceptance matrix, proposed findings, and
one overall `PASS`, `FAIL`, or `BLOCKED` result. Each finding needs a
source-local ID, classification, violated obligation, exact location, evidence,
observable consequence, materiality rationale, and required testable correction.
For `DRIFT`, identify what can be deleted or which existing or native mechanism already covers the
outcome. Do not treat matching findings as automatically true or discard a
finding because only one reviewer reported it.

## Independent repository standards review (`repo-rev`)

Use this specialist only when selected under the topology above. Inspect its
assigned scope with cumulative context available; report coverage of that scope.

Give the repository-standards specialist the immutable subject, scoped diff,
repository root, and available verification results. Do not provide other
reviewers' conclusions or an intended verdict.

The specialist reads `AGENTS.md`, every nested `AGENTS.md` the diff touches,
the applicable `agent_docs/` topic guides, and the owning sections of the
[architecture authorities](../../../architecture/README.md). It maps every
changed surface to its governing rules and inspects enough surrounding source,
tests, manifests, generated artifacts, and consumers to determine compliance.
It audits all touched languages and layers; one surface cannot stand in for
Rust, Python, TypeScript, server, contract, test, documentation, or tooling
rules that independently apply.

Inspect actual SQL capability types in production signatures and fields.
`check:tenant-isolation` covers only the directories it scans; a green check
cannot excuse `PgPool` where `OperatorPool` or `TenantConn` is required. Include materially changed test helpers and rustdoc in the same
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

Use this specialist only when selected under the topology above. Inspect its
assigned scope with cumulative context available; report coverage of that scope.

Give the specialist the immutable subject, scoped diff, repository root,
[maintainer style](../../../agent_docs/maintainer-style.md),
applicable Wyrd authority, and available verification results. Do not provide
other reviewers' conclusions or an intended verdict.

The specialist reads each materially changed symbol in its assigned scope and
owning module, traces its callers and relevant tests, and reviews layout, owner
and method
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

Use this specialist only when selected under the topology above. Inspect its
assigned scope with cumulative context available; report coverage of that scope.

Give this specialist the immutable subject, scoped diff, deployment and
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

Always spawn a fresh `ponytail-rev` after any selected discovery and follow-up
are complete, even when their proposed finding union is empty. Give it the
immutable subject, applicable authorities, round scope and relevant diffs,
all selected reviewer reports, prior verdicts and acceptance/closure evidence,
and remediation tasks, but no intended verdict. On remediation it independently
checks the fix and affected paths rather than merely accepting claimed closure;
unaffected evidence carries forward under the scope rules above. Check that
selected and carried-forward evidence cover the applicable acceptance and
repository obligations, including when the proposed finding union is empty.
Investigate a specific missing proof; do not repeat comprehensive discovery.

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
   including affected callers, sibling consumers and writers of the same
   authority, and existing helpers; read the full body of each function the
   correction would change or move. Expand tracing where shared state or
   authority can propagate the defect, rather than inventorying unrelated code;
2. prove the reported path is reachable and required by the approved task,
   rejecting dormant, test-only, speculative, or zero-caller surfaces unless
   the task explicitly requires them;
3. distinguish the requested behavior from bundled adjacent behavior and reject
   a correction that moves, duplicates, or weakens unrelated lifecycle,
   admission, durability, security, resource ownership, or sibling service
   availability;
4. ask whether the finding or remediation can be deleted, whether existing
   behavior already satisfies the task, and whether the proposed proof is the
   smallest credible check without a new dependency or test harness; assess
   materiality and implementation/maintenance burden before retaining either;
5. compare prior findings and remediations for a shared source, consolidating
   related failures into one correction when the same invariant owns them; and
6. return `CONFIRMED`, `REVISED`, or `REJECTED` with source evidence and the
   smallest safe correction boundary. Explain why any retained consumer guard
   belongs there.

The final deduplicated ledger records each retained finding's stable
`FIND-<task>-<n>` ID, discovery source IDs, `CONFIRMED` or `REVISED` status,
classification, violated obligation, exact location, evidence, observable
consequence, materiality rationale, correction burden, decision-complete
correction, and focused closure proof. Preserve prior `FIND-*` IDs during remediation and assign the next unused number only to
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

In the established review directory, preserve the selected reviewers' reports
and `findings-validation.md`, then write `verdict.md` containing the immutable
subject, round scope and carried-forward evidence, reviewer selection and
coverage, reconciled acceptance matrix, independent review results, follow-up
decision, validated finding ledger, verification limits, prior-finding closure,
any required convergence assessment, and one verdict:

- `PASS` — all required reports are complete, the independently validated ledger
  is empty, every obligation passes after reconciling proposed findings with
  source or carried-forward evidence, non-goals remain excluded, verification
  is credible, and no unrelated change entered the diff. Exhaustive edge-case
  discovery is not a PASS condition. A rejected proposal does not force failure
  merely because its discovery reviewer reported `FAIL`;
- `FIX_REQUIRED` — one or more independently validated material implementation
  findings or explicit mandatory-gate violations remain;
- `SPEC_REVISION_REQUIRED` — correction requires changing approved behavior or
  an expensive-to-reverse decision; or
- `BLOCKED` — the immutable subject, authority, diff, or required independent
  review cannot be obtained.

For `FIX_REQUIRED`, also write one self-contained remediation task named
`<task-id>-R<n>-<name>.md` in the same review directory. It must contain:

1. the approved spec path, original task path, and candidate identities;
2. an issue diagnosis for each material finding: the violated obligation,
   current behavior, exact evidence, observable consequence, and why the
   candidate or its existing proof falls short, including materiality;
3. the intended correction outcome;
4. a decision-complete recommendation within the approved behavior: select the
   minimal correction approach, name the existing owner or mechanism to reuse,
   identify the source of the invalid state and affected consumers, resolve
   alternatives that would change scope or proof, and explain why that approach
   closes the diagnosed gap without relying on repeated downstream guards,
   with the correction burden justified against smaller alternatives;
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
directly to `$wyrd-implement`. A later review checks closure and affected paths
under the remediation scope rules, retaining the original task and cumulative
candidate as acceptance context.

Return only the verdict, verdict path, remediation task path when present, and
finding IDs. `PASS` is the task's completion gate. Review does not implement,
merge, push, or deploy.
