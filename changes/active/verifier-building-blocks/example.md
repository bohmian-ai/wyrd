# Verifier examples

Worked examples for the [Verifier building blocks proposal](proposal.md). Each
one exercises a different combination of primitives. Read the proposal's
"Authoring model" and "Agent harness" sections for the rules; these examples
show the rules in use.

| # | Example | Primitives it shows |
|---|---|---|
| 1 | [Assertions and an LLM judge](#1-assertions-and-an-llm-judge) | `json` inputs, no steps, `assert`, agent check, `requirement.statement` |
| 2 | [A PDF against a policy](#2-a-pdf-against-a-policy) | `media` input injected into a judge, one judge reused per clause |
| 3 | [A PowerPoint deck](#3-a-powerpoint-deck) | `extract`, step requirements, `each`, an agent step reading a file through its workspace |
| 4 | [A code-review DAG](#4-a-code-review-dag) | `code` action, parallel agent steps, `when` gated on a verdict, workspace tools, a Verifier requirement over step outputs |
| 5 | [Drift](#5-drift) | `samples` input, `drift` check, `submit` records rows, a `schedule` Trigger judges the window |
| 6 | [An upload-and-verify service](#6-an-upload-and-verify-service) | `submit`, `ExecutionId`, `wait`, Operators |

## How to read a Verifier

Every reference starts from one of three roots and reads the same everywhere:

| Reference | Means |
|---|---|
| `inputs.<slot>…` | What the caller sent for that slot |
| `steps.<id>.output…` | What that step's action returned |
| `steps.<id>.status` | `succeeded`, `failed`, or `skipped` |
| `steps.<id>.requirements.<rid>.outcome` | That step requirement's verdict (readable by `when` and action inputs only) |
| `requirement.statement` | The statement of the requirement being checked (inside a check only) |

- A step that references `steps.X` depends on `X`. Steps with no dependency
  between them run at the same time.
- A **step requirement** checks one step's result immediately after that step
  succeeds, so later steps can use its verdict.
- A **Verifier requirement** checks the work as a whole, once, after every
  step has finished.
- Requirements never feed each other. Every requirement produces one verdict,
  and `decide` combines all of them once: any failed → failed; otherwise any
  inconclusive → inconclusive; otherwise passed.
- An agent block has two channels. `inputs:` values are **injected** into the
  model request through the Prompt's placeholders. `workspace:` values are
  **files** the Agent can open with the tools assigned on its Agent Card.

---

## 1. Assertions and an LLM judge

A support bot's reply must stay within a refund limit, be confident, and
actually answer the question.

```yaml
apiVersion: wyrd/v1
kind: Verifier
metadata: { space: support, name: reply-quality, version: 1.0.0 }
spec:
  inputs:
    question: { type: json }
    reply:    { type: json }        # { text, refund_amount, confidence }
  requirements:
    - id: refund-limit
      statement: Refunds offered are at most $500
      check: { kind: assert, path: inputs.reply.refund_amount, operator: less_than_or_equals, expected: 500 }
    - id: confident
      statement: Model confidence is at least 0.7
      check: { kind: assert, path: inputs.reply.confidence, operator: greater_than_or_equals, expected: 0.7 }
    - id: answers-question
      statement: The reply directly answers the customer's question
      check:
        kind: agent
        agent: ./agents/judge.yaml
        inputs:                                  # injected: keys are the Prompt's placeholders
          claim: requirement.statement
          question: inputs.question
          reply: inputs.reply.text
```

The judge is a plain Agent with no tools. Its Prompt's `response_format` is
the Verdict schema, which every check-position Agent must return.

```yaml
# agents/judge.yaml
apiVersion: wyrd/v1
kind: Agent
metadata: { space: support, name: judge, version: 1.0.0 }
spec:
  execution_route: { kind: wyrd_gateway, deployment: anthropic-primary }
  run_config: { max_iterations: 1, timeout_ms: 30000 }
  prompt:
    provider: anthropic
    model: claude-sonnet-5-5
    system: You judge one claim against the evidence. Cite the evidence you used.
    messages:
      - role: user
        content: |
          Claim: {{claim}}
          Question: {{question}}
          Reply: {{reply}}
    settings: { max_output_tokens: 1024 }
    response_format:                             # the Verdict schema
      type: object
      properties:
        outcome:   { type: string, enum: [passed, failed, inconclusive] }
        reason:    { type: string }
        citations:
          type: array
          items:
            type: object
            properties:
              slot:    { type: string }
              locator: { type: object }          # { json_pointer } | { page } | { slide } | { lines: { start, end } }
            required: [slot, locator]
      required: [outcome, reason, citations]
      additionalProperties: false
```

How it runs:

- There are no steps, so all three requirements run against the inputs.
- The two asserts are deterministic. The judge gets the statement, question,
  and reply text injected through `{{claim}}`, `{{question}}`, `{{reply}}`.
- A judge `passed` without a valid citation becomes `inconclusive`.

```python
result = run.observe.verify("support/reply-quality",
                            question={"text": "Can I return a damaged lamp?"},
                            reply={"text": "Yes — ...", "refund_amount": 80, "confidence": 0.91})
result.outcome.verdict      # passed
```

---

## 2. A PDF against a policy

Each policy clause is a requirement. One judge Agent serves every clause
because the statement is passed in explicitly.

```yaml
apiVersion: wyrd/v1
kind: Verifier
metadata: { space: legal, name: msa-policy, version: 1.0.0 }
spec:
  inputs:
    contract: { type: media, formats: [pdf] }
  requirements:
    - id: liability-cap
      statement: Liability is capped at no more than 12 months of fees
      check:
        kind: agent
        agent: { kind: Agent, name: clause-judge, version: 1.0.0 }
        inputs: { claim: requirement.statement, document: inputs.contract }
    - id: governing-law
      statement: Governing law is Delaware or New York
      check:
        kind: agent
        agent: { kind: Agent, name: clause-judge, version: 1.0.0 }
        inputs: { claim: requirement.statement, document: inputs.contract }
```

```yaml
# clause-judge Prompt: the PDF goes in as a native document block
prompt:
  provider: anthropic
  model: claude-sonnet-5-5
  messages:
    - role: user
      content:
        - { type: document, source: "${media:document}" }
        - { type: text, text: "Does this contract satisfy: {{claim}}? Cite pages." }
  settings: { max_output_tokens: 1024 }
  response_format: { ... }                       # the Verdict schema, as in example 1
```

How it runs:

- `document: inputs.contract` binds the uploaded PDF to the Prompt's
  `${media:document}` placeholder. The server attaches the bytes as a native
  document block; the model reads the PDF directly. No step, no tools, no
  workspace.
- The two requirements run concurrently; each returns its own verdict with page
  citations.
- Injection only works for provider-native formats (pdf, png, jpeg, gif, webp,
  text) and the attach limit. A pptx or a very large file goes through a step
  instead (examples 3 and 4).

```python
result = run.observe.verify("legal/msa-policy", contract=wyrd.File("acme-msa.pdf"))
for r in result.outcome.requirements:
    print(r.id, r.outcome, r.reason, r.citations)
# liability-cap   failed   "cap is 24 months of fees"   contract p.7
# governing-law   passed   "Delaware"                   contract p.12
```

---

## 3. A PowerPoint deck

Mechanical checks on the parsed deck, plus an Agent that reads the slides and
scores the narrative. Models cannot take pptx natively, so the Agent reads it
through its workspace.

```yaml
apiVersion: wyrd/v1
kind: Verifier
metadata: { space: marketing, name: deck-spec, version: 1.0.0 }
spec:
  inputs:
    deck: { type: media, formats: [pptx] }
  steps:
    - id: structure
      action: { kind: extract, from: inputs.deck }
      requirements:
        - id: slide-titles
          statement: Every slide has a title
          check: { kind: assert, each: steps.structure.output.slides[*].title, operator: not_equals, expected: null }

    - id: narrative
      action:
        kind: agent
        agent: { kind: Agent, name: deck-reviewer, version: 1.0.0 }
        inputs:    { outline: steps.structure.output }       # injected through {{outline}}
        workspace: { /inputs/deck.pptx: inputs.deck }        # a file its tools can open
      requirements:
        - id: clear-story
          statement: Reviewer scores the narrative at least 4 of 5
          check: { kind: assert, path: steps.narrative.output.score, operator: greater_than_or_equals, expected: 4 }
```

```yaml
# deck-reviewer Agent Card
spec:
  execution_route: { kind: wyrd_gateway, deployment: anthropic-primary }
  tool_names: [fs.list, fs.read]
  run_config: { max_iterations: 10, timeout_ms: 120000 }
  prompt:
    provider: anthropic
    model: claude-sonnet-5-5
    messages:
      - role: user
        content: |
          The deck is at /inputs/deck.pptx; read it slide by slide with fs.read.
          Its parsed outline: {{outline}}
          Score the executive narrative from 1 to 5.
    settings: { max_output_tokens: 2048 }
    response_format:
      type: object
      properties: { score: { type: integer, minimum: 1, maximum: 5 }, reason: { type: string } }
      required: [score, reason]
      additionalProperties: false
```

How it runs:

1. `structure` parses the pptx. Its output is the fixed pptx extract document:
   `{ format: "pptx", slides: [{ number, title, paragraphs, tables, notes }] }`
   from [REQ-004](spec.md#locked-extraction-output-types). `slide-titles`
   checks it straight away. Counts use array lengths in a code check when needed.
2. `narrative` depends on `structure` because it references
   `steps.structure.output`. Before the Agent loop starts, Wyrd copies the deck
   into the Agent's workspace at `/inputs/deck.pptx`. `fs.read` on a pptx
   returns the extracted text with slide markers.
3. The Agent's output is the object its `response_format` declares
   (`{ score, reason }`). `clear-story` asserts on it: the Agent produces data,
   the assert produces the verdict.
4. `decide` combines both verdicts.

```python
result = run.observe.verify("marketing/deck-spec", deck=wyrd.File("q3.pptx"))
# structure  slide-titles failed   actual=[..., null, ...]   cite: slide 6
# narrative  clear-story  passed
# verdict: failed
```

---

## 4. A code-review DAG

A cheap deterministic step sizes the PR. Two specialist reviewers run in
parallel only when the PR is reviewable, each exploring a repository snapshot
with tools. A final requirement judges the work as a whole.

```yaml
apiVersion: wyrd/v1
kind: Verifier
metadata: { space: eng, name: pr-review, version: 1.0.0 }
spec:
  inputs:
    diff: { type: json }                         # { files: [{ path, additions, deletions, patch }] }
    repo: { type: media, formats: [tar_gz] }     # snapshot at the PR head

  steps:
    - id: scope                                  # stage 1: references no step
      action:
        kind: code
        runtime: python
        entrypoint: summarize
        inputs: { diff: inputs.diff }
        source:
          inline: |
            def summarize(diff):
                return {"files": [f["path"] for f in diff["files"]],
                        "changed_lines": sum(f["additions"] + f["deletions"] for f in diff["files"])}
      requirements:
        - id: reviewable-size
          statement: PR changes at most 800 lines
          check: { kind: assert, path: steps.scope.output.changed_lines, operator: less_than_or_equals, expected: 800 }

    - id: security                               # stage 2
      when: { path: steps.scope.requirements.reviewable-size.outcome, operator: equals, expected: passed }
      action:
        kind: agent
        agent: { kind: Agent, name: security-reviewer, version: 1.0.0 }
        inputs:    { diff: inputs.diff }
        workspace:
          /inputs/repo: inputs.repo              # tar_gz → unpacked directory
          /inputs/scope.json: steps.scope.output
      requirements:
        - id: no-critical
          statement: No critical security findings
          check: { kind: assert, path: steps.security.output.counts.critical, operator: equals, expected: 0 }

    - id: correctness                            # stage 2, concurrent with security
      when: { path: steps.scope.requirements.reviewable-size.outcome, operator: equals, expected: passed }
      action:
        kind: agent
        agent: { kind: Agent, name: correctness-reviewer, version: 1.0.0 }
        inputs:    { diff: inputs.diff }
        workspace: { /inputs/repo: inputs.repo }
      requirements:
        - id: no-blocking
          statement: No blocking correctness defects
          check: { kind: assert, path: steps.correctness.output.counts.blocking, operator: equals, expected: 0 }

  requirements:
    - id: tested
      statement: Every behavior change has a covering test
      check:
        kind: agent
        agent: ./agents/judge.yaml               # no tools; sees only what is injected
        inputs:
          claim: requirement.statement
          diff: inputs.diff
          findings: steps.correctness.output
```

```yaml
# security-reviewer Agent Card: tools are assigned here, never in the Verifier
spec:
  execution_route: { kind: wyrd_gateway, deployment: anthropic-primary }
  tool_names: [fs.list, fs.read, fs.search, code.run]
  run_config: { max_iterations: 25, timeout_ms: 300000 }
  prompt:
    provider: anthropic
    model: claude-sonnet-5-5
    system: |
      You are a security reviewer. The repository is at /inputs/repo and the
      changed files are listed in /inputs/scope.json. Read surrounding code
      before reporting. Use code.run for any counting or parsing.
    messages:
      - role: user
        content: "Review this diff: {{diff}}"
    settings: { max_output_tokens: 4096 }
    response_format:
      type: object
      properties:
        findings:
          type: array
          items:
            type: object
            properties:
              severity: { type: string, enum: [critical, high, medium, low] }
              path:     { type: string }
              line:     { type: integer }
              detail:   { type: string }
            required: [severity, path, detail]
        counts:
          type: object
          properties: { critical: { type: integer }, high: { type: integer } }
          required: [critical, high]
      required: [findings, counts]
      additionalProperties: false
```

How it runs:

1. **Stage 1, `scope`.** Wyrd runs the user's Python in the sandbox and returns
   `{ files, changed_lines }`. `reviewable-size` checks it immediately.
2. **Stage 2, `security` and `correctness`.** Both reference `steps.scope`,
   so both wait for it and run concurrently. Their `when` reads
   `reviewable-size`'s verdict: an 1,800-line PR skips both reviews (their
   requirements become `not_applicable`), and the failed `reviewable-size` still
   fails the Verifier.
3. **Each reviewer's harness.** Wyrd fetches `repo` and `scope.json` into a
   fresh per-call scratch directory, mounts it read-only at `/inputs`, and gives
   the Agent a scratch `/work` held in memory. The diff is injected through
   `{{diff}}`. The Agent loops: `fs.search` for call sites, `fs.read` around
   them, `code.run` to parse or count. Every tool call is recorded. When the
   loop ends the workspace is deleted, and the output is the `response_format`
   object.
4. **After all steps, `tested`.** It spans the diff and the correctness
   findings, so it is a Verifier requirement. Its judge has no tools and sees
   only what is injected.
5. **`decide`** combines `reviewable-size`, `no-critical`, `no-blocking`, and
   `tested`, each counted once.

The reviewers never run the tests. If test results matter, the caller's CI runs
them and passes them as an input (`tests: { type: json }`) for a requirement to
judge.

```python
job = run.observe.submit("eng/pr-review",
                         diff=load_diff(412),
                         repo=wyrd.File("head.tar.gz"))
result = job.wait(timeout=900)
```

---

## 5. Drift

Drift is never real time. `submit` records rows; the bound `schedule` Trigger
judges each window.

```yaml
apiVersion: wyrd/v1
kind: Verifier
metadata: { space: risk, name: credit-model-stable, version: 1.0.0 }
spec:
  inputs:
    current: { type: samples, features: [income, dti, fico, state] }
  requirements:
    - id: inputs-stable
      statement: No scoring input drifted from the training distribution
      check:
        kind: drift
        on: inputs.current
        method: psi
        signal: { distribution: { baseline_ref: { kind: Data, name: credit-train, version: 3.1.0 }, features: [income, dti, fico, state] } }
        condition: statistical
        profile: { psi: { binning_strategy: { quantile: 10 }, categorical_features: [state], threshold: { chi_square: { alpha: 0.05 } } } }
```

```yaml
# on the credit-scoring Service Card
verified_by:
  - verifier: { kind: Verifier, space: risk, name: credit-model-stable, version: 1.0.0 }
    runs_on:  { kind: Trigger, space: risk, name: nightly, version: 1.0.0 }   # a schedule Trigger
    on_failure: [{ kind: Operator, space: risk, name: page-model-owner, version: 1.0.0 }]
```

How it runs:

- Every scoring call does
  `run.observe.submit("risk/credit-model-stable", current={...})`. The SDK sees a
  `drift` check (`CheckEvidence::Window`), writes the row through today's drift
  observation path, and returns `Submission::Recorded`.
- Each night the scheduler reads the window of rows and the fitted baseline,
  and runs the Verifier once. A failed verdict dispatches `page-model-owner`.
- `run.observe.verify` on this Verifier is refused locally with
  `WYRD_VERIFICATION_422_INPUT_UNSUPPORTED`, with remediation to use `submit`.

---

## 6. An upload-and-verify service

A company's backend accepts customer uploads and verifies each one. The
backend is the Wyrd client; its Service Card is the subject. Uploads are
evidence, not Cards.

```python
@app.post("/uploads")
def upload(file: UploadFile):
    run = state.run()                            # run_id: this request, minted locally
    job = run.observe.submit("legal/msa-policy", contract=wyrd.File(file.file.read()))
    db.save(upload_id=..., execution_id=job.id)   # execution_id: this Verifier execution
    return {"status": "verifying"}, 202

@worker
def relay(execution_id):
    result = wyrd.VerificationExecution(client, execution_id).wait(timeout=900)
    notify_customer(result.outcome.verdict, result.outcome.requirements)
```

- `submit` uploads the bytes to `evidence/sha256/{hex}` (skipped if that digest
  is already stored) and gets `202 { execution_id }` back without any Postgres
  write on the request path.
- `wait` polls `GET /v1/verification/executions/{execution_id}` and returns
  the same `VerificationResult` that `verify` would.
- Failed verdicts dispatch the Operators named on the Service's binding. A small
  check on the request path can use `verify` instead and gets the verdict in the
  response.
