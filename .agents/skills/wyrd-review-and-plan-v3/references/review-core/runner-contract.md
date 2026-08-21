# External Codex Specialist Runner Contract

Use the skill's `scripts/run_specialists.py` as the default specialist backend.
It launches independent ephemeral Codex CLI sessions without consuming the
root conversation's collaboration slots. The runner owns process execution,
timeouts, bounded retries, output parsing, identity validation, and atomic
report publication. It never selects assignments, interprets findings, assigns
final IDs, or decides a verdict.

## Model policy

- Terminal review root: `gpt-5.6-sol`, medium reasoning.
- Baseline and triggered specialists: `gpt-5.6-terra`, high reasoning.
- Independent final adjudicator: `gpt-5.6-sol`, high reasoning.

Sol high is reserved for final independent adjudication. A specialist never
changes its model or reasoning effort. Weak evidence returns to the Sol-medium
root. Invoke Sol high exactly when the independent-validation floor in
`evidence-contract.md` triggers, including a high-risk boundary declared
clean; do not add another, narrower adjudication rule.

## Immutable input packet

The root writes one JSON runner packet outside the reviewed worktree:

```json
{
  "schema_version": 1,
  "review_id": "review-id",
  "repository_root": "/absolute/detached-target-worktree",
  "review_dir": "/absolute/review-output",
  "assignments": [
    {
      "assignment_id": "baseline-security",
      "domain": "security",
      "reviewer_id": "security-session",
      "target_sha": "full-commit-sha",
      "prompt_path": "/absolute/review-security.md",
      "prompt_sha256": "64-lowercase-hex",
      "input_path": "/absolute/complete-assignment.txt",
      "input_sha256": "64-lowercase-hex",
      "report_path": "/absolute/review/evidence/specialists/baseline-security.md"
    }
  ]
}
```

Every reviewer ID is unique. Every assignment binds the same target SHA. The
assignment context file contains the immutable review tuple, scoped impact
slice, authorities, requirements, task criteria, report contract, and
prohibition on writes, tests, planning, verdicts, and nested agent launches.
It does not replace lens instructions: the runner reads the digest-verified
`prompt_path` and prepends those exact bytes to the assignment context before
starting Codex. A packet cannot bind one prompt digest while sending another
prompt to a specialist.

## Execution

Run:

```bash
python "${SKILL_ROOT}/scripts/run_specialists.py" \
  "${RUNNER_PACKET}" \
  --max-parallel 7 \
  --timeout-seconds 1800 \
  --retries 1
```

The fixed child invocation is ephemeral, read-only, schema-constrained, and
explicitly routed to Terra high. The read-only sandbox forbids source writes.
The concurrency value is a ceiling. Reduce it for provider throttling or host
pressure; never omit or combine required assignments.

An infrastructure failure, timeout, or malformed output receives at most one
retry by default. Identity mismatch is never retried. After all processes
finish, any failed required assignment makes the terminal review
`REVIEW_BLOCKED`; successful reports remain durable evidence for diagnosis but
do not permit a partial clean verdict.

## Result and publication

Each session returns the JSON shape in
`../schemas/specialist-result.json`. The runner verifies assignment, domain,
reviewer, target, and prompt identities, candidate IDs, report content, and
static limits. Only then does it atomically publish the Markdown report beneath
`evidence/specialists/`.

The runner prints one compact completion manifest to stdout. The root records
the backend as `codex-exec`, verifies the complete roster, and performs all
candidate validation and consolidation itself.

The final adjudicator is not part of the Terra specialist packet. When the
independent-validation floor in `evidence-contract.md` triggers, launch one
separate ephemeral read-only
session with `--model gpt-5.6-sol` and
`--config 'model_reasoning_effort="high"'`, give it the immutable adjudication
packet and `validation/independent-validation.md`, and schema-constrain its
result. It must have a new reviewer identity and must not have produced a
specialist candidate report.

## Fallback

If the Codex executable, authentication, or external-session capability is
unavailable, the root may use the built-in collaboration backend. Dispatch in
capacity-bounded waves with the same immutable assignments, models, prompts,
identities, and evidence schema. Record `backend: collaboration`. Backend
choice never changes coverage or acceptance requirements.
