---
name: wyrd-implement-v3
description: Implement and fully verify one decision-complete Wyrd task packet or bounded remediation in an isolated worktree, then return one immutable candidate commit or a material BLOCKED result.
---

# Wyrd Implement v3

Own the complete task loop: acquire context, edit code and tests, run every
task-required verification command, fix failures, and seal one immutable
candidate only after required verification passes. The controller coordinates;
the independent reviewer decides acceptance. Never plan, integrate, review,
spawn children, or mutate controller state.

## Input and boundary

Accept `request_id`, repository root, immutable task-packet path and digest,
`parent_sha`, dedicated clean `worktree_path`, controller generation, and an
optional remediation binding. The packet is immutable authority for task ID,
dependencies, forecast `write_set`, hard `prohibited_writes`, execution tier,
acceptance criteria, and complete verification.

A remediation binding contains the prior review artifact path/digest,
superseded candidate, and assigned finding IDs. Terminal remediation also binds
the reviewed integrated target. Verify all bindings before editing. Findings
add only their bounded outcomes, assertions, owners, and checks; they do not
authorize a new material decision.

Read `AGENTS.md`, `architecture/agent-rules.md`, the complete packet, and
applicable design/doctrine authorities. Inspect named paths, consumers, tests,
manifests, and `mise` tasks. Use CodeGraph only when indexed at `parent_sha`.
`write_set` is a forecast; `prohibited_writes` is the hard path boundary.
Change task-local supporting files when source, consumers, or diagnostics make
them necessary. Do not alter plans/packets, merge, rebase, cherry-pick,
integrate, push, or perform unrelated cleanup.

## Implement and fully verify

Map every AC to source and test evidence. Make the smallest cohesive change,
including required unit, integration, and user-journey coverage. Follow all
repository ownership, struct-centered Rust, rustdoc, async, PyO3, contract, and
test rules.

Compiler, formatter, lint, test, fixture, codegen, and setup failures are
ordinary feedback. Fix all explained task-local failures, including necessary
callers, declarations, generated output, fixtures, rustdoc, and lint cleanup.
Completion closure does not permit a new material behavior, owner, dependency,
public/durable contract, acceptance outcome, unrelated formatting, or
prohibited write.

Run fast diagnostics when useful, then the packet's complete required
verification. The implementer—not the controller—owns this full edit -> verify
-> fix loop. Use a controller-managed resource lane when required, but retain
execution ownership. Never weaken, replace, skip, mask, or falsely claim a
check. Do not seal a candidate until all required checks pass.

For remediation, start from the original `parent_sha`, inspect the rejected
diff and selected reviewer findings, produce one complete replacement rather
than a repair descendant, apply every finding, and rerun complete task
verification plus finding-specific checks. Every replacement receives fresh
independent review.

Return `BLOCKED` only after focused investigation for a material decision,
unavailable authority, indeterminate acceptance outcome, necessary prohibited
write, or mandatory proof that cannot safely run. Ordinary failures, supporting
files, and implementation choices are not blockers.

## Compact proof

Store full stdout/stderr once in a content-addressed artifact. Reports and
state reference it; they never copy full logs, command strings, packet prose,
or AC prose. Each command entry contains only:

```yaml
command_id: <ID resolving to packet/remediation command>
cwd: <bound cwd>
timeout_seconds: <integer>
started_at: <timestamp>
finished_at: <timestamp>
exit_status: <integer or null>
result: <PASS|CODE_FAILURE|INFRA_UNAVAILABLE>
output: {ref: <artifact ref>, sha256: <digest>}
failure_excerpt: <short causal excerpt only when not PASS>
```

## Seal and return

Audit tracked/untracked changes, run `git diff --check`, verify configured Git
identity, and confirm every AC has evidence. Create exactly one normal commit
with `parent_sha` as sole parent, calculate the binary diff SHA-256, and leave
the worktree clean. Never alter the reported commit.

Return one compact candidate manifest containing protocol/outcome, request,
task ID and packet digest, generation, parent/candidate SHAs, binary diff
digest, `sole_parent: true`, superseded SHA, sorted changed paths,
`prohibited_writes: PASS`, forecast expansions with evidence refs, proof
artifact ref/digest/result, remediation binding, and:

```yaml
acceptance_trace:
  - {ac_id: <each AC/assertion once>, evidence_refs: [<source/test/proof refs>]}
```

Blocked output contains the request/task binding, category, concise evidence
refs, and one exact decision/correction required. Never return a partial
candidate. A changed packet, parent, candidate, diff, or remediation binding
invalidates proof and requires a new candidate and fresh review.
