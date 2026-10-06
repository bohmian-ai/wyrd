---
name: wyrd-task-review
description: Review one immutable cumulative Wyrd task through independent implementation, system-resilience, standards, and domain audits, source validation, and root-cause remediation.
---

# Wyrd Task Review

Answer one question: does the resulting repository satisfy the original task
exactly? This is an acceptance audit, not an opportunity to improve, redesign,
or refactor the implementation.

Keep the reviewed source immutable. Review the complete base-to-candidate range,
not the implementation summary or only the latest fix diff. After remediation,
include the original task, prior verdict and findings, remediation task, and
cumulative candidate.

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
   reviewer (`repo-rev`), a maintainer specialist (`maintainer-rev`), a
   system-resilience reviewer (`system-rev`), and a reuse reviewer
   (`reuse-rev`). Also spawn one domain reviewer
   (`domain-rev`) for each materially changed sensitive domain, including
   security/RBAC, tenancy, concurrency, durability, persistent data, or another
   boundary whose correctness needs domain expertise.
2. **Compare claims:** after discovery reports are complete, group findings by
   violated behavior or invariant rather than line number. Agreement is useful
   corroboration, not proof; unique findings still require validation. If
   reports materially conflict or reveal an unreviewed reachable path, spawn
   one fresh focused `followup-rev` to investigate only that uncertainty.
   Record why it was or was not needed. On a repeat review (any earlier review
   directory exists for this task), always spawn a fresh `followup-rev` in
   root-cause mode, whatever the discovery reports contain; it may also carry
   any conflict above.
3. **Independent validation:** after any follow-up, always spawn one fresh
   structured Ponytail reviewer (`ponytail-rev`). It validates every proposed
   finding against source and produces the final finding ledger and
   decision-complete recommendations, even when the finding union is empty.

Discovery reviewers receive the immutable subject and inputs needed for their
own scope, but not another reviewer's conclusions or an intended verdict. The
`followup-rev`, when needed, receives the conflicting claims and source paths,
without an intended verdict; in root-cause mode it also receives every earlier
review directory's `verdict.md` and remediation task, and the remediation
commits since the first reviewed candidate. The `ponytail-rev` receives the
complete diff, applicable authorities, and every discovery and follow-up
report. The
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
repeat review, report each finding's relationship to prior findings you notice;
the root-cause `followup-rev` owns that analysis.

Apply the Ponytail ladder to every changed abstraction, dependency,
configuration surface, compatibility path, generic layer, and speculative
extension: delete it if the task does not need it; otherwise reuse repository,
standard-library, native-platform, or installed-dependency behavior before
accepting new code. Require the smallest safe root-cause correction without
weakening validation, error handling, security, accessibility, or durability.

Treat each failure diagnosis in the task evidence as a claim to falsify: the
recorded cause must explain the trace, and the fix must sit at that cause. A
change to a test, assertion, timeout, sleep, retry, `#[ignore]`, `#[allow]`, or
skip made to clear a failure, without a recorded diagnosis and diagnostician
report, is a `VIOLATION`.

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
unrelated pre-existing debt, or refactors not required by the task. Tests prove
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

## Independent reuse review (`reuse-rev`)

Give this specialist the immutable subject, complete cumulative diff, base
revision, repository root, and the task's pinned dependency and fork diffs. Do
not provide other reviewers' conclusions or an intended verdict. It has one job:
find every place the candidate added a mechanism that is semantically
equivalent to, or parallel with, one that already exists, instead of reusing or
extending the existing owner. Differently written code that does the same job
counts; textual similarity is not required.

For every added or materially changed type, function, module, conversion,
encoder/decoder, validator, error mapping or carrier, rendering, schema/type
mapping, wire field, test fixture or harness helper, and configuration, search
the base tree (`git grep <term> <base>`, and CodeGraph callers, callees, and
symbol search when `.codegraph/` exists) and the installed dependencies' public
APIs for an existing owner that already does the job or should have been
extended. Also find duplicates inside the candidate: the same branch pasted
into several callers, two implementations of one job in one file, or one case
given its own path beside the general one.

Return `reuse-review.md`. For each confirmed duplicate, give a source-local ID,
the new location, the existing owner (file:line or crate API), evidence that
both do the same job, and the smallest consolidation that deletes the
duplicate side. Then list each checked surface found not duplicated, one line
each, with the owner searched. End with one overall `PASS`, `FAIL`, or
`BLOCKED` result. A confirmed duplicate is a `VIOLATION` of
[AGENTS.md](../../../AGENTS.md) §15 and is blocking in every review round.
Uncovered added surfaces block this review.

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

### Root-cause mode (repeat reviews)

This pass owns the question of whether separate findings across rounds have
one cause. For every finding in this round's discovery reports, and every
prior-round finding and its remediation commits, identify findings that touch
the same owner, value, or invariant, or code that an earlier remediation
changed. For each such group, trace each symptom back through source to its
producer and decide:

- **shared root** — name the root (owner, mechanism, or decision) and show the
  source path from it to each symptom, including symptoms an earlier round
  already patched; or
- **independent** — show the evidence that the causes differ.

Proximity, timing, or a plausible story is not a shared root; without a traced
path, record `independent` or `UNRESOLVED`. Return `root-cause.md` with one row
per finding: related prior findings and remediation commits, the decision,
source evidence, and for a shared root the correction site at the root and
every symptom it closes. New findings at the root still go to independent
validation.

## Structured Ponytail validation (`ponytail-rev`)

Always spawn a fresh `ponytail-rev` after discovery and any follow-up are
complete, even when their proposed finding union is empty. Give it the
immutable subject, applicable authorities, complete diff, both task-review
reports, `standards-review.md`, `maintainer-review.md`, `system-review.md`,
`reuse-review.md`, every `domain-review-<domain>.md`, and any
`followup-review.md`, but no intended verdict.

The `ponytail-rev` independently inspects the actual source, validates every
proposed finding and correction, including claims from only one reviewer. It
removes duplicates and resolves contradictions from approved authority. It
checks cited symbols and paths against current source; agreement among
reviewers does not replace this proof. Apply this ladder to every finding and
proposed remediation:

1. Can it be deleted while preserving the complete task?
2. Does existing repository behavior already solve it?
3. Does the standard library or native platform solve it?
4. Does an already-installed dependency solve it?
5. Only then, what is the minimum necessary correction?

Prefer a correction at the source that makes invalid state unrepresentable or
prevents it from being produced. A guard at a consumer is appropriate when that
consumer owns the invariant or enforces a real trust boundary; otherwise trace
upstream before accepting it. Do not mistake fewer lines for a valid
simplification when it weakens validation, error handling, security,
accessibility, durability, or another explicit requirement. For each proposed
finding the `ponytail-rev` must:

1. trace the failing state from its producer to the observed consumer, inspect
   sibling consumers, and read the full body and callers of each function the
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
5. on a repeat review, validate every `root-cause.md` decision against source.
   For a confirmed shared root, consolidate its symptoms into one correction at
   the root. A correction or earlier remediation that patches a symptom of a
   confirmed shared root is `INCORRECT`, and the correction removes the symptom
   patches it makes redundant; and
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

The orchestrator may include only independently confirmed or revised findings.
A rejected finding is omitted, not softened into optional advice. If validation
shows that the correction needs a new product, public API, architecture,
security, compatibility, cross-service, concurrency, resource-ownership, or
persistent-data decision, return `SPEC_REVISION_REQUIRED` when the approved
task truly requires that decision, otherwise reject the finding as out of
scope. `SPEC_REVISION_REQUIRED` is an approval gate, not permission to omit the
remedy: retain the finding and provide a decision-complete recommended spec
revision together with the implementation correction and proof that follow if
the user approves it. Do not defer discovery of the fix to `$wyrd-spec` or the
implementation agent.

Missing source, incomplete caller tracing, an unavailable independent
`ponytail-rev`, or disagreement that cannot be resolved from approved authority
blocks the review. Preserve its final ledger and recommendations as
`findings-validation.md` in the review directory.

## Verdict and remediation task

In the established review directory, preserve both task-review reports,
`standards-review.md`, `maintainer-review.md`, `system-review.md`,
`reuse-review.md`, every `domain-review-<domain>.md`,
any `followup-review.md`, any `root-cause.md`, and `findings-validation.md`,
then write `verdict.md` containing the immutable subject, reconciled acceptance
matrix, independent review results, the follow-up decision, validated finding
ledger, the validated root-cause groups on a repeat review, verification limits,
prior-finding closure, and one verdict:

- `PASS` — all required reports are complete, the independently validated ledger
  is empty, every obligation passes after reconciling proposed findings with
  source, non-goals remain excluded, verification is credible, and no unrelated
  change entered the diff. A rejected proposal does not force failure merely
  because its discovery reviewer reported `FAIL`;
- `FIX_REQUIRED` — one or more bounded implementation findings remain;
- `SPEC_REVISION_REQUIRED` — one or more retained findings require explicit
  approval of a recommended spec change before their implementation correction
  may proceed; or
- `BLOCKED` — the immutable subject, authority, diff, or required independent
  review cannot be obtained.

For `FIX_REQUIRED` and `SPEC_REVISION_REQUIRED`, also write one self-contained
remediation task named `<task-id>-R<n>-<name>.md` in the same review directory.
Include every retained finding; do not drop findings that remain implementable
under the current spec merely because another finding requires a revision. It
must contain:

1. the approved spec path, original task path, and candidate identities;
2. an issue diagnosis for each material finding: the violated obligation,
   current behavior, exact evidence, observable consequence, and why the
   candidate or its existing proof falls short; for a confirmed shared root,
   one diagnosis at the root that lists every symptom it closes, including
   symptoms patched in earlier rounds;
3. the intended correction outcome;
4. a decision-complete recommendation: for findings already authorized by the
   approved spec, select the minimal correction approach, name the existing
   owner or mechanism to reuse, identify the source of the invalid state and
   affected consumers, resolve alternatives that would change scope or proof,
   and explain why that approach closes the diagnosed gap without relying on
   repeated downstream guards; for spec-revision findings, provide the same
   implementation detail contingent on the draft revision below;
5. constraints, preserved behavior, and explicit non-goals;
6. acceptance criteria mapped to every finding; and
7. focused proof that directly exercises the gap plus broader verification.

For each finding that requires a spec revision, the remediation task must also
contain:

1. the exact approved clauses that conflict with or fail to authorize the
   correction;
2. the recommended replacement, addition, or deletion as concrete draft spec
   text, including the next revision number and a revision-history entry;
3. the material decision and rationale the user must approve, with rejected
   alternatives when they affect observable behavior or an expensive-to-reverse
   boundary;
4. the implementation correction that becomes authorized by that draft text,
   including its owner, affected consumers, and preserved behavior; and
5. sequencing that requires explicit human approval of the spec revision before
   changing code, followed by the same focused and broader proof required for
   any other remediation.

Do not revise the spec merely to waive a failed obligation or make the current
candidate pass. Removing or weakening an obligation is a valid recommendation
only when higher authority and the user's stated intent establish that the
obligation itself is wrong; state that evidence and still identify any code,
tests, or generated artifacts that the approved revision would require.

Do not write an outcome checklist or merely restate the acceptance matrix. The
diagnosis and recommendation are the substance of the remediation task;
acceptance criteria only prove that correction. An implementer must not need to
rediscover the defect or choose the correction boundary. If that recommendation
requires a new product, public API, architecture, security, compatibility,
cross-service, concurrency-semantics, or persistent-data decision, use
`SPEC_REVISION_REQUIRED` and include the draft spec change and contingent
implementation correction above; never substitute the verdict for the
remediation packet.

The remediation task packages validated findings for a fresh implementation
agent; it is not another design plan. Do not specify helpers, private methods,
local control flow, fixture structure, or optional improvements. Route a
`FIX_REQUIRED` packet directly to `$wyrd-implement`. Route a
`SPEC_REVISION_REQUIRED` packet first to `$wyrd-spec` for the named approval;
after approval, route that same packet to `$wyrd-implement` without making the
implementer reconstruct the omitted fix. A later review reassesses the complete
cumulative candidate against the original task and approved revision.

Return only the verdict, verdict path, remediation task path when present, and
finding IDs. `PASS` is the task's completion gate. Review does not implement,
merge, push, or deploy.
